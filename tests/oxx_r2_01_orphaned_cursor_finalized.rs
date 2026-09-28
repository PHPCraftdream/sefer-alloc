//! oxx R2-01 regression: a fully-emptied former `small_cur` must be
//! finalized (pooled or released) the moment `reserve_small_segment`
//! replaces it as the bump-carve cursor — not left as a permanent
//! `small_empty_orphan`, committed and registered but outside
//! `pool_segments` and unreachable by `trim_current_thread()` / trim on
//! thread exit.
//!
//! **Mechanism (see `docs/reviews/2026-09-28-154558-src-review-oxx-round-2.md`
//! R2-01).** Small segments are shared bump-carve targets across every small
//! class. `dec_live_and_maybe_decommit` deliberately skips finalizing the
//! CURRENT cursor (`base == small_cur`) — correct at that instant, since the
//! segment is about to be carved into again. If every block of the current
//! cursor is freed while it is STILL the cursor, and the NEXT small
//! allocation is of a class this cursor cannot serve locally (no free block
//! of that class anywhere, and no carve room left in the cursor's tail), a
//! brand new segment is reserved and becomes the new cursor — and the old,
//! now-empty cursor is never revisited: nothing else ever finalizes it. The
//! fix (`reserve_small_segment`, `src/alloc_core/small/alloc_core_small/reserve.rs`)
//! checks the OUTGOING cursor with the same eligibility test
//! `finalize_orphaned_empty_segments` uses (`kind == Small`, `live_count ==
//! 0`, not decommitted, not already pooled) and finalizes it right there.
//!
//! **Construction.** Both arms below drive exactly this transition on a
//! SINGLE, continuously-held heap/`AllocCore` — never straddling a
//! `HeapRegistry::claim`/`recycle` boundary, because a recycled registry
//! slot's `HeapCore`/`AllocCore` stays whole (the "whole-slot-reuse teardown
//! model": all prior segments persist for the next claimant), so a freshly
//! `claim()`ed heap is not guaranteed to be a pristine substrate the way a
//! fresh `AllocCore::new()` is — measuring `K` on one claim and filling on a
//! separate one can silently observe two DIFFERENT segments' leftover state.
//! Without depending on any hardcoded per-class segment capacity:
//!
//! 1. **Discover K.** Allocate `FILL_BYTES` blocks one at a time, tracking
//!    each returned pointer's segment base and `SegmentKind`
//!    (`dbg_kind_at_tag`), until a transition OUT OF a `Small`-kind segment
//!    is observed. That segment's block count is `K` — the capacity of one
//!    ordinary (non-Primordial) small segment for this class. Skipping the
//!    Primordial segment's own (differently-sized) residual capacity this
//!    way means `K` is measured on a genuinely representative fresh `Small`
//!    segment, matching every later one on the SAME heap.
//! 2. **Fill the target.** The allocation that caused the transition above
//!    already placed the first block of the NEXT segment — continue on that
//!    same segment (still on the SAME heap), adding `K - 1` more, so it
//!    reaches exactly `K` blocks and stops BEFORE the `(K+1)`-th call would
//!    force yet another reservation. The target segment is now both fully
//!    tail-exhausted (no room left for another block of ANY class) and
//!    still `small_cur` (nothing has moved the cursor away from it).
//! 3. **Empty it.** Free all `K` blocks. `live_count` reaches `0` while the
//!    segment is still the cursor — `dec_live_and_maybe_decommit`'s
//!    `base == small_cur` guard correctly skips finalizing it, exactly as
//!    R2-01 describes.
//! 4. **Switch the cursor.** Allocate one `OTHER_BYTES` block — a different
//!    small class the target segment has zero free-list entries for and, by
//!    construction, zero carve room left for either. `reserve_small_segment`
//!    must run, replacing the cursor. Under the pre-fix code the emptied
//!    target segment becomes a permanent `small_empty_orphan`; under the fix
//!    it is pooled or released in the SAME call.
//!
//! `FILL_BYTES` (200 000 B) is large enough that `HeapCore`'s magazine
//! refill batch clamps to exactly 1 block per miss
//! (`REFILL_BYTE_BUDGET / block_size` rounds to `0`, clamped to `1` — see
//! `src/registry/heap_core/state/tcache.rs`'s `refill_n_for_class`), so each
//! `alloc()` call independently carves at most one fresh block there,
//! matching the standalone `AllocCore::alloc_small` path's own per-call
//! granularity (its `carve_block_with_refill` batches up to 31 extra blocks
//! per COLD carve, but those extras are carved-then-immediately-freed in the
//! SAME call — net zero live-count change — so the discovery/fill loops
//! below see identical per-call segment-base transitions on both paths).
//! `OTHER_BYTES` (250 000 B) is a strictly larger, strictly distinct small
//! size class under every size-class table this crate ships (default,
//! `medium-classes`, `medium-classes-wide` all place `SMALL_MAX` well above
//! it), so it never reclassifies as Large and never shares a free list with
//! `FILL_BYTES`.
//!
//! **Counterfactual.** With the R2-01 fix reverted (the outgoing-cursor
//! finalize call removed from `reserve_small_segment`), both tests below go
//! RED: `small_empty_orphan.count == 1` (and, on the standalone path,
//! `orphan.committed_bytes` equal to one whole `SEGMENT`). With the fix
//! applied, both are GREEN: `small_empty_orphan.count == 0`.
//!
//! Feature gate: `internals` + `alloc-decommit` + `bench-internals` (the
//! `dbg_segment_state_reconciliation` / `dbg_kind_at_tag` / `dbg_live_count_for`
//! seams this test relies on) for the standalone `AllocCore` arm; the
//! `HeapCore`/`HeapRegistry` production-path arm additionally requires
//! `alloc-global` + `fastbin` (`dbg_flush_all` and the registry itself).
//! Compiled by CI's `production internals bench-internals` `cargo test` row
//! (`.github/workflows/ci.yml`) and by `scripts/check-matrix.mjs`'s matching
//! local-check row.

#![cfg(all(
    feature = "internals",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// The class filled to capacity in the target segment. Large enough that
/// `HeapCore`'s magazine refill batch clamps to exactly 1 (see the module
/// doc), and comfortably under `SMALL_MAX` in every size-class table this
/// crate ships.
const FILL_BYTES: usize = 200_000;

/// A different, strictly larger small class the target segment never
/// carves — the allocation that must force `reserve_small_segment` once the
/// target is both empty and tail-exhausted.
const OTHER_BYTES: usize = 250_000;

/// `dbg_kind_at_tag`'s `Small` tag (see that method's doc: `0` = Primordial,
/// `1` = Small, `2` = Large, `3` = Unknown).
const SMALL_KIND_TAG: u8 = 1;

fn fill_layout() -> Layout {
    Layout::from_size_align(FILL_BYTES, 8).expect("FILL_BYTES/8 is a valid layout")
}

fn other_layout() -> Layout {
    Layout::from_size_align(OTHER_BYTES, 8).expect("OTHER_BYTES/8 is a valid layout")
}

/// Drive a SINGLE, continuously-held heap/`AllocCore` (never straddling a
/// `claim`/`recycle` or a `new`/`drop` boundary — see the module doc) to a
/// fresh, `K`-full, still-current small-cursor segment.
///
/// `alloc_one` allocates one `FILL_BYTES` block and returns `(ptr,
/// segment_base, kind_tag)`. Two phases, both driven by the SAME loop of
/// `alloc_one` calls:
///
/// 1. **Discover K.** Skip past whatever the current cursor already is
///    (which may be Primordial, or — since a claimed heap's substrate can
///    carry history from an earlier claimant — an already partially-used
///    Small segment) until a transition OUT OF a `Small`-kind segment is
///    observed; that segment's block count is `K`.
/// 2. **Fill the target.** The very allocation that caused the transition
///    above already placed the first block of the NEXT segment. Continue
///    adding blocks to THAT segment until it holds exactly `K`, stopping
///    BEFORE the `(K+1)`-th call would force yet another reservation.
///
/// Returns the target segment's base and every live pointer placed in it.
/// The target segment is, on return, both fully tail-exhausted (no room for
/// another block of ANY class — this is what forces `reserve_small_segment`
/// later) and still the live bump-carve cursor (nothing has moved it away).
fn drive_target_segment_to_fresh_full_capacity(
    mut alloc_one: impl FnMut() -> (*mut u8, usize, u8),
) -> (usize, Vec<*mut u8>) {
    const ITER_CAP: usize = 200_000;
    let mut iters = 0usize;
    let mut call = || {
        iters += 1;
        assert!(
            iters <= ITER_CAP,
            "drive_target_segment_to_fresh_full_capacity: exceeded {ITER_CAP} \
             allocations without completing both phases"
        );
        alloc_one()
    };

    // Phase 1a: skip forward to the START of a Small-kind segment (handles
    // both a fresh heap's Primordial residual and a reused slot's
    // already-mid-segment cursor).
    let (mut first_ptr, mut base, mut kind) = call();
    while kind != SMALL_KIND_TAG {
        let (p, b, k) = call();
        first_ptr = p;
        base = b;
        kind = k;
    }
    // Phase 1b: count how many more land on the SAME base before it changes
    // — that count is K, and the call that changed it already placed the
    // first block of the target segment.
    let discovery_base = base;
    let mut k = 1usize;
    let (target_first_ptr, target_base) = loop {
        let (p, b, _k) = call();
        if b == discovery_base {
            k += 1;
            continue;
        }
        break (p, b);
    };
    let _ = first_ptr;

    // Phase 2: fill the target segment with the remaining `k - 1` blocks.
    let mut ptrs = Vec::with_capacity(k);
    ptrs.push(target_first_ptr);
    for _ in 1..k {
        let (p, b, _kind) = call();
        assert_eq!(
            b, target_base,
            "a same-class allocation must stay in the target segment until \
             it reaches its measured capacity K={k}"
        );
        ptrs.push(p);
    }
    assert_eq!(ptrs.len(), k);
    (target_base, ptrs)
}

/// Standalone `AllocCore` arm.
#[test]
fn alloc_core_orphaned_cursor_is_finalized_on_cursor_switch() {
    let layout = fill_layout();

    let mut ac = AllocCore::new().expect("AllocCore::new must survive bootstrap");
    let (target_base, ptrs) = drive_target_segment_to_fresh_full_capacity(|| {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "fill alloc failed");
        let base = SegmentLayout::segment_base_of(p as usize);
        let kind = ac.dbg_kind_at_tag(p);
        (p, base, kind)
    });
    let target_base_ptr = target_base as *mut u8;

    assert_eq!(
        ac.dbg_live_count_for(target_base_ptr),
        Some(ptrs.len() as u32),
        "precondition: the freshly filled segment must hold exactly K live blocks"
    );

    // Empty it while it is still small_cur.
    for p in ptrs {
        // SAFETY: each `p` was returned by the matching `ac.alloc(layout)`
        // call above, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
    assert_eq!(
        ac.dbg_live_count_for(target_base_ptr),
        Some(0),
        "the target segment must be fully empty before the cursor-switching alloc"
    );

    // The cursor-switching allocation: OTHER_BYTES has zero free-list
    // entries anywhere and zero carve room left in the (tail-exhausted)
    // target cursor, forcing `reserve_small_segment`.
    let other = ac.alloc(other_layout());
    assert!(!other.is_null(), "the cursor-switching alloc must not fail");

    let rec = ac.dbg_segment_state_reconciliation();
    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(
        rec.small_empty_orphan.count, 0,
        "oxx R2-01: the emptied former cursor must be finalized (pooled or \
         released) at the moment reserve_small_segment replaces it, not left \
         as a permanent orphan outside pool_segments/trim"
    );
}

/// Production `HeapCore` (`HeapRegistry::claim`) arm.
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[test]
fn heap_core_orphaned_cursor_is_finalized_on_cursor_switch() {
    let _ = bootstrap::ensure();
    let layout = fill_layout();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let (target_base, ptrs) = drive_target_segment_to_fresh_full_capacity(|| {
        // SAFETY: `heap` is a live heap handed back by `HeapRegistry::claim`
        // above; every call here happens before its `recycle` below.
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "fill alloc failed");
        let base = unsafe { (*heap).dbg_segment_base_of_ptr(p) } as usize;
        let kind = unsafe { (*heap).dbg_kind_at_tag(p) };
        (p, base, kind)
    });
    let target_base_ptr = target_base as *mut u8;

    // Empty it own-thread. Own-thread frees are magazine-buffered (not
    // reflected in `live_count` until the magazine flushes a run back to the
    // substrate), so the assertion below runs AFTER an explicit flush.
    for &p in &ptrs {
        // SAFETY: each `p` was returned by the matching `(*heap).alloc(layout)`
        // call above, is live, and is freed exactly once here.
        unsafe { (*heap).dealloc(p, layout) };
    }
    // SAFETY: `heap` is the same live heap used throughout this test.
    unsafe {
        // Force every class's magazine back to the substrate so
        // `live_count` is exact...
        (*heap).dbg_flush_all();
        // ...and force-drain any incidentally-pooled segment, mirroring the
        // production teardown-trim sequence (`trim_for_recycle`) R2-01's own
        // probe used. This scenario pools nothing else, but the call is
        // harmless (0 drained) and keeps the sequence faithful to the report.
        (*heap).dbg_drain_small_pool();
    }

    assert_eq!(
        unsafe { (*heap).dbg_live_count_for(target_base_ptr) },
        Some(0),
        "the target segment must be fully empty (post-flush) before the \
         cursor-switching alloc"
    );

    // The cursor-switching allocation.
    // SAFETY: `heap` is the same live heap used throughout this test.
    let other = unsafe { (*heap).alloc(other_layout()) };
    assert!(!other.is_null(), "the cursor-switching alloc must not fail");

    // SAFETY: `heap` is the same live heap used throughout this test.
    let rec = unsafe { (*heap).dbg_segment_state_reconciliation() };
    assert_eq!(rec.unknown_count, 0, "no corrupt segment headers");
    assert_eq!(
        rec.small_empty_orphan.count, 0,
        "oxx R2-01: the emptied former cursor must be finalized (pooled or \
         released) at the moment reserve_small_segment replaces it on the \
         production HeapCore path too"
    );

    // Cleanup.
    // SAFETY: `other` was returned by the matching `(*heap).alloc` above,
    // is live, and is freed exactly once here; `heap` is not used again
    // after `recycle`.
    unsafe {
        (*heap).dealloc(other, other_layout());
        HeapRegistry::recycle(heap);
    }
}
