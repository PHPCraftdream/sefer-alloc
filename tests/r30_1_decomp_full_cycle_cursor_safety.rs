//! R30-1 (task #450) — counterfactual for the dangling-`small_cur` fix.
//!
//! ## The bug this proves fixed
//!
//! `AllocCore::dbg_decomp_full_cycle` (`src/alloc_core/alloc_core_small_pool.rs`)
//! is a safe `pub fn`, `bench-internals`-gated, that measures one reserve→
//! release segment-lifecycle cycle. Before this fix it called
//! `reserve_small_segment()` — whose LAST statement publishes the freshly
//! reserved segment as the live bump-carve cursor, `self.small_cur = base`
//! (`alloc_core_small.rs`, then near line 2210) — and immediately afterward
//! called `release_or_pool_empty_segment(base)`. When the empty-small-segment
//! hysteresis pool is already at capacity, that release is a REAL one: the OS
//! reservation is returned (`release_empty_segment_now`) and the table slot
//! is recycled (`self.table.recycle(base)`). Neither function restored
//! `small_cur` afterward, so it was left pointing at unmapped/recycled
//! memory. The very next ordinary small allocation on the SAME heap reads
//! through it at `alloc_small`/`alloc_small_with_virgin`'s first dispatch
//! step, `self.pop_free(self.small_cur, ...)` — a use of a dangling base.
//!
//! `examples/r29_3_decomposition_gate.rs` drives exactly this release branch
//! (its own comment: pre-filling the pool "not the pool-push path") but never
//! performs an ordinary alloc afterward on the same heap, so the dangling
//! cursor was never actually read back — caller luck, not a sound API. This
//! test closes that gap: it performs a real small alloc/write/free on the
//! SAME heap immediately after driving the release branch.
//!
//! ## The fix
//!
//! `reserve_small_segment` was split into a cursor-publishing wrapper (kept
//! for the three production callers) and a new cursor-FREE
//! `reserve_small_segment_impl` (`pub(super)`). `dbg_decomp_full_cycle` (and
//! its sibling pair `dbg_decomp_reserve_and_keep` / `dbg_decomp_release`) now
//! route through `reserve_small_segment_impl`, so `self.small_cur` is never
//! touched by these measurement-only hooks and can never be left dangling by
//! them, however many times they run or how full the pool is.
//!
//! ## Non-vacuity
//!
//! This test was run against the pre-fix code (temporarily reverting the
//! `dbg_decomp_full_cycle`/`dbg_decomp_reserve_and_keep` call sites back to
//! `reserve_small_segment()`) and observed to fail — see the commit message
//! for the exact failure mode observed: the whole test process crashed with
//! `STATUS_ACCESS_VIOLATION` (Windows hard fault, exit code `0xc0000005`), a
//! genuine use-after-free reading through the dangling `small_cur`. It
//! passes unconditionally after the fix.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]

use core::alloc::Layout;

use sefer_alloc::AllocCore;

/// Fill the small-segment hysteresis pool to (and past) capacity by driving
/// the full-cycle dbg hook repeatedly — mirrors
/// `examples/r29_3_decomposition_gate.rs`'s own pre-fill loop exactly (`pool_cap
/// + 2` iterations, "not the pool-push path"), so every subsequent
/// `dbg_decomp_full_cycle` call takes the RELEASE branch of
/// `release_or_pool_empty_segment` (pool full ⇒ genuine OS release +
/// `table.recycle`), not the pool-push branch.
///
/// Then performs an ordinary small allocation, a write, a readback, and a
/// free on the SAME heap — the exact sequence that would read through a
/// dangling `small_cur` pre-fix.
#[test]
fn full_cycle_hook_leaves_small_cur_valid_for_ordinary_alloc() {
    let mut ac = AllocCore::new().expect("primordial");
    let pool_cap = ac.dbg_pool_cap();
    assert!(
        pool_cap > 0,
        "pool must be enabled (production default) for this test"
    );

    // Pre-fill the pool past capacity — every call from here on drives the
    // RELEASE branch (genuine OS release + table.recycle), exactly like
    // `examples/r29_3_decomposition_gate.rs`'s own pre-fill loop.
    for _ in 0..(pool_cap + 2) {
        assert!(ac.dbg_decomp_full_cycle(), "reserve failed during pre-fill");
    }
    assert_eq!(
        ac.dbg_pooled_count(),
        pool_cap,
        "pool must be at capacity after pre-fill"
    );

    // Drive the release branch several more times — this is the sequence
    // that dangled `small_cur` pre-fix: `reserve_small_segment` published
    // the fresh base as `small_cur`, then the immediate release recycled
    // that exact base, leaving `small_cur` pointing at unmapped memory.
    for _ in 0..8 {
        assert!(
            ac.dbg_decomp_full_cycle(),
            "reserve failed during release-branch churn"
        );
    }

    // ── The counterfactual: an ORDINARY small allocation on the SAME heap ──
    //
    // Pre-fix, `alloc_small_with_virgin`/`alloc_small`'s first dispatch step
    // (`self.pop_free(self.small_cur, ...)`) reads through the dangling
    // cursor here — a use-after-free of the released segment's header/
    // bin-table. Post-fix, `small_cur` was never touched by the hook above,
    // so it still points at whatever valid segment it held before (the
    // primordial segment, on a fresh `AllocCore`).
    let layout = Layout::from_size_align(256, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(
        !p.is_null(),
        "ordinary alloc after release-branch churn must succeed"
    );
    assert_eq!((p as usize) % 8, 0, "must be correctly aligned");

    // Write and read back a distinctive pattern — proves the returned block
    // is real, writable memory (not a bogus pointer derived from a stale
    // cursor) and not aliased with anything else.
    unsafe {
        core::ptr::write_bytes(p, 0xAB, 256);
    }
    let mut buf = [0u8; 256];
    unsafe {
        core::ptr::copy_nonoverlapping(p, buf.as_mut_ptr(), 256);
    }
    assert!(buf.iter().all(|&b| b == 0xAB), "readback mismatch");

    // Free it cleanly — a further proof the allocator's bookkeeping (bitmap,
    // bin table, live_count) for the segment `p` was carved from is
    // internally consistent, not corrupted by a dangling-cursor write into
    // unmapped/foreign memory.
    // SAFETY: `p` was returned by the matching `ac.alloc(layout)` immediately
    // above, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };

    // A second post-free allocation burst confirms the heap is still fully
    // healthy (not merely "the first alloc happened to survive").
    let mut keep = Vec::new();
    for _ in 0..4096 {
        let q = ac.alloc(layout);
        assert!(!q.is_null(), "heap unhealthy after release-branch churn");
        keep.push(q);
    }
    for q in keep {
        // SAFETY: each `q` was returned by the matching `ac.alloc(layout)`
        // above, is live, and is freed exactly once here.
        unsafe { ac.dealloc(q, layout) };
    }
}

/// Same hazard, exercised via the `dbg_decomp_reserve_and_keep` /
/// `dbg_decomp_release` pair (used by
/// `examples/r29_3_decomposition_gate.rs`'s measurement B / A' arms) instead
/// of the single-call `dbg_decomp_full_cycle`. Confirms the fix covers both
/// hooks named in the task, not just the first.
#[test]
fn reserve_and_keep_release_pair_leaves_small_cur_valid_for_ordinary_alloc() {
    let mut ac = AllocCore::new().expect("primordial");
    let pool_cap = ac.dbg_pool_cap();
    assert!(
        pool_cap > 0,
        "pool must be enabled (production default) for this test"
    );

    // Fill the pool to capacity via the full-cycle hook first (cheaper than
    // reserve_and_keep/release for the fill phase).
    for _ in 0..pool_cap {
        assert!(ac.dbg_decomp_full_cycle(), "reserve failed during pre-fill");
    }
    assert_eq!(ac.dbg_pooled_count(), pool_cap, "pool must be at capacity");

    // Now drive the release branch several times via the reserve_and_keep /
    // release pair specifically.
    for _ in 0..8 {
        let base = ac
            .dbg_decomp_reserve_and_keep()
            .expect("reserve_and_keep failed");
        // SAFETY: `base` was just returned by `dbg_decomp_reserve_and_keep`
        // on this same `ac`, with `live_count == 0` (freshly reserved,
        // nothing ever carved from it).
        unsafe { ac.dbg_decomp_release(base) };
    }

    // Ordinary alloc/write/free on the same heap — the counterfactual.
    let layout = Layout::from_size_align(128, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(
        !p.is_null(),
        "ordinary alloc after release-branch churn must succeed"
    );
    unsafe {
        core::ptr::write_bytes(p, 0xCD, 128);
    }
    let mut buf = [0u8; 128];
    unsafe {
        core::ptr::copy_nonoverlapping(p, buf.as_mut_ptr(), 128);
    }
    assert!(buf.iter().all(|&b| b == 0xCD), "readback mismatch");
    // SAFETY: `p` was returned by the matching `ac.alloc(layout)` immediately
    // above, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };
}
