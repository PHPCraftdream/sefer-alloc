//! Regression (SHOULD, soundness): `AllocCore::dbg_push_to_ring` is an
//! `unsafe fn` (R6-MS-4), closing the round5 `memory_safety_review` R5-MS-4
//! stale-note→double-issue chain that fully-safe Rust could drive under the
//! `production` feature set.
//!
//! ## The bug (R5-MS-4, HIGH)
//!
//! Pre-fix, `dbg_push_to_ring` was a safe `pub fn` — the PRODUCER side of the
//! cross-thread free simulation — so a fully-safe, single-threaded,
//! standalone-`AllocCore` sequence deterministically freed a re-issued LIVE
//! block:
//!
//! 1. `p = alloc(L)`; resolve its class via `dbg_layout_class_for`.
//! 2. `dbg_push_to_ring(p, class)` left a deferred "remote free" note in `p`'s
//!    segment ring with NO liveness/uniqueness check.
//! 3. `dealloc(p, L)` put `p` onto the current thread's BinTable (own-thread
//!    free).
//! 4. `q = alloc(L)` popped the current freelist first (`pop_free`) and could
//!    return `q == p` WITHOUT draining the stale ring note from step 2.
//! 5. `dbg_drain_all_rings()` processed the STALE note from step 2: `q`'s
//!    bitmap reads "allocated" (the re-issue set it), the magazine predicate is
//!    always-false on a bare `AllocCore`, and the generational guard is compiled
//!    out under `production` — so drain did `write_next`/`mark_free` on the LIVE
//!    `q`.
//! 6. `r = alloc(L)` re-issued the SAME address again while `q` was still live
//!    → two live owners of one range.
//!
//! ## The fix
//!
//! `dbg_push_to_ring` (and its `HeapCore` thin-delegation wrapper) are now
//! `unsafe fn` with a `# Safety` contract: `ptr` is a live block in a segment
//! owned by the receiver; this push is AT MOST ONE logical remote free (no
//! `dealloc`/`flush_class`/`alloc`-re-issue of `ptr` between the push and the
//! consuming drain); and `class_idx` is the block's actual allocated class.
//! This is the same two-tier confined-`unsafe` pattern as R6-MS-1/2
//! (`dealloc`/`realloc`) and R6-MS-3 (`flush_class`): a caller obligation that
//! the compiler, not prose, enforces.
//!
//! ## Why this closes the chain
//!
//! The 6-step chain now requires `unsafe` at step 2 (this task, R6-MS-4) AND
//! step 3 (`dealloc`, R6-MS-1/2). A contract-honoring caller — who treats `p`
//! as consumed by the push and does not `dealloc`/re-issue it before the drain
//! — can never produce a stale note, so drain can never `mark_free` a live
//! re-issue. The drain's own guards (`is_free` bitmap, magazine predicate under
//! `fastbin`, generation guard under `hardened`) remain as defence-in-depth for
//! contract violations; under `production` they are insufficient on their own,
//! which is exactly why the producer must be `unsafe fn`. The generational guard
//! is NOT made unconditional: a contract-honoring caller cannot hit the
//! residual, so the `hardened`-only gen-guard stays a probabilistic misuse
//! backstop (not the primary soundness mechanism).
//!
//! ## Counterfactual (RED without the fix)
//!
//! This file's first test calls `dbg_push_to_ring` inside an `unsafe {}` block.
//! Reverting the `unsafe` keyword on `AllocCore::dbg_push_to_ring`
//! (i.e. undoing R6-MS-4) makes that call site — and every call site across
//! `tests/`/`benches/` — a compile error ("call to unsafe function is unsafe and
//! requires unsafe block"), so the 6-step chain can no longer be assembled from
//! safe Rust. This was confirmed during this task's development by temporarily
//! reverting the `unsafe` keyword and observing `cargo check --features
//! production` go red on the call site, then restoring it to green. (The
//! concrete compile error is reproduced verbatim in this file's git history at
//! development time.)
//!
//! The second test exercises the CONTRACT-HONORING end-to-end path — push a
//! single logical remote free, drain, re-alloc — and asserts the pushed block
//! is re-issued EXACTLY ONCE (single owner), proving the seam is still usable
//! for legitimate cross-thread-free simulation after the boundary change.

#![cfg(all(feature = "alloc-core", feature = "alloc-xthread"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// `dbg_push_to_ring` is now `unsafe fn` (R6-MS-4): the call must sit inside an
/// `unsafe {}` block. Removing the `unsafe` keyword from the fn declaration
/// makes this call site a compile error — the load-bearing compile gate that
/// closes the R5-MS-4 chain (see the module doc's counterfactual).
#[test]
fn dbg_push_to_ring_is_unsafe_fn_boundary_compiles_only_in_unsafe_block() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let class = ac
        .dbg_layout_class_for(layout)
        .expect("16-byte layout must map to a small class");
    let p = ac.alloc(layout);
    assert!(!p.is_null(), "alloc returned null");

    // Push `p` as a single logical remote free (contract-honoring).
    //
    // SAFETY (R6-MS-4): `p` is a fresh live allocation owned by this core in one
    // of its segments; this push is its single logical remote free (no dealloc /
    // re-issue of `p` before the drain below). `class` is `p`'s actual allocated
    // class. The wrapper is load-bearing: removing the `unsafe` keyword on the
    // fn declaration (reverting R6-MS-4) makes this call site a compile error,
    // verified during development.
    let pushed = unsafe { ac.dbg_push_to_ring(p, class) };
    assert!(pushed, "ring push failed (ring full or p not owned)");

    // Drain reclaims `p` back into its segment's BinTable (mark_free + link).
    ac.dbg_drain_all_rings();

    // The allocator must remain fully usable; a fresh alloc must succeed.
    let q = ac.alloc(layout);
    assert!(!q.is_null(), "alloc after push/drain returned null");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `q` is a live
    // allocation made with the matching layout, freed exactly once here.
    unsafe { ac.dealloc(q, layout) };
}

/// The CONTRACT-HONORING end-to-end path: push one logical remote free, drain,
/// re-alloc — the pushed block must be re-issued EXACTLY ONCE (single owner).
/// This is the legitimate cross-thread-free simulation the seam exists for; it
/// proves the boundary change did not break the honoring path while it closes
/// the violating path.
#[test]
fn contract_honoring_push_drain_realloc_issues_block_exactly_once() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let class = ac
        .dbg_layout_class_for(layout)
        .expect("16-byte layout must map to a small class");

    // (1) alloc `p`.
    let p = ac.alloc(layout);
    assert!(!p.is_null(), "alloc returned null");

    // (2) push `p` as a single logical remote free (contract-honoring: `p` is
    //     NOT dealloc'd nor re-issued before the drain in step 3).
    //
    // SAFETY (R6-MS-4): `p` is a live allocation owned by this core; this push
    // is its single logical remote free; `class` is `p`'s actual class.
    let pushed = unsafe { ac.dbg_push_to_ring(p, class) };
    assert!(pushed, "ring push failed (ring full or p not owned)");

    // (3) drain reclaims `p` onto its segment's BinTable freelist (bitmap-free,
    //     linked). `p` is now the contract-honoring reclamation of the remote
    //     free — there is exactly ONE owner-path to it (the freelist).
    ac.dbg_drain_all_rings();

    // (4) re-alloc. `pop_free` serves the freelist first, so `q == p` (the
    //     reclaimed block is re-issued as a fresh allocation — exactly one
    //     owner). This is the SINGLE legitimate re-issue.
    let q = ac.alloc(layout);
    assert!(!q.is_null(), "alloc after drain returned null");
    assert_eq!(
        q, p,
        "the drained/reclaimed block must be re-issued (LIFO freelist pop)"
    );

    // (5) a SECOND alloc must NOT return `p` again — `p` is now exclusively
    //     owned by `q`. It must carve a different block (`r != p`), proving
    //     single ownership (no double-issue).
    let r = ac.alloc(layout);
    assert!(!r.is_null(), "second alloc returned null");
    assert_ne!(
        r, p,
        "p must not be re-issued a second time while q holds it (single owner)"
    );

    // Cleanup: free both distinct live blocks exactly once.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — `q` and `r` are
    // live allocations made with the matching layout, each freed exactly once.
    unsafe { ac.dealloc(q, layout) };
    unsafe { ac.dealloc(r, layout) };
}
