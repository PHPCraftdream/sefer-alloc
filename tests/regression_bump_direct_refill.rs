//! Regression (P3, task #147, Э1 — bump-direct batched carve).
//!
//! Exercises the `AllocCore::refill_class_bump` fast path directly through
//! `AllocCore` (deterministic — a magazine layer is not needed to hit it) in
//! a cold-storm → free-storm → churn sequence, asserting the guarantees the
//! bump-direct rewrite must preserve:
//!
//!   - Every handed-out pointer is DISTINCT and WRITABLE (no double-issue).
//!   - Every block frees cleanly (M2: bump-carved blocks are bitmap-allocated,
//!     so `dealloc_small` actually frees them instead of hitting the
//!     double-free no-op).
//!   - D1 (live_count) stays EXACT: after a cold-storm + full free-storm the
//!     segments return to `live_count == 0` and decommit fires (a bump-direct
//!     over-count would leak — never decommit; an under-count would
//!     over-decommit / free-list corruption). Verified via `dbg_live_count_for`
//!     and `dbg_decommit_count`.
//!
//! Counterfactual (why this is not vacuous): the whole point of bump-direct
//! is to carve straight into `out` with exactly ONE `inc_live` per block.
//!   - If a block were double-counted (`inc_live` twice), the source segment
//!     would never reach `live_count == 0` after the free-storm →
//!     `dbg_decommit_count` would NOT rise → the `decommit fired` assertion
//!     fails.
//!   - If a block were bitmap-FREE after carve (the old round-trip left it on
//!     a BinTable), the free-storm's `dealloc` would hit the M2 no-op and the
//!     block would stay "allocated" forever → `live_count` never returns to 0
//!     → same assertion fails; and the re-alloc would re-issue it → the
//!     distinct-pointer assertion fails.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::AllocCore;
// Only used by `seg_base` (itself gated to the `cfg(not(miri))` d1 test).
#[cfg(all(feature = "alloc-decommit", not(miri)))]
use sefer_alloc::SegmentLayout;

fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

// Only referenced by `d1_cold_storm_then_free_decommits`, which is
// `cfg(not(miri))`; gate the helper the same way to avoid a dead-code warning
// under miri (R3, #155).
#[cfg(all(feature = "alloc-decommit", not(miri)))]
fn seg_base(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

/// Cold-storm → free-storm → churn on a 16B class, asserting distinctness,
/// writability, and clean frees. Runs under every feature combo.
#[test]
fn cold_storm_free_storm_churn_16b() {
    cold_storm_free_storm_churn_inner(16, 8);
}

#[test]
fn cold_storm_free_storm_churn_256b() {
    cold_storm_free_storm_churn_inner(256, 8);
}

fn cold_storm_free_storm_churn_inner(size: usize, align: usize) {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    // Cold storm: refill a big batch straight from bump. With a fresh core
    // and no free blocks, every one of these is a bump-carve (the free-drain
    // branch finds nothing) — this is precisely the bump-direct path.
    //
    // Under miri the pointer-math / strict-provenance coverage is identical at
    // a small N, and the full 4096-block storm × 5 rounds is prohibitively slow
    // (miri interprets every write). Cap the storm under `cfg(miri)` only — a
    // test-only edit that does NOT change non-miri behavior (R3, #155).
    #[cfg(not(miri))]
    const N: usize = 4096;
    #[cfg(miri)]
    const N: usize = 256;
    let mut buf = vec![core::ptr::null_mut::<u8>(); N];
    let got = core.refill_class_bump(c, &mut buf);
    assert_eq!(got, N, "cold-storm refill short: {got}/{N}");

    // Distinct + writable.
    let unique: HashSet<usize> = buf.iter().map(|p| *p as usize).collect();
    assert_eq!(unique.len(), N, "duplicate pointer in cold storm");
    for (i, &p) in buf.iter().enumerate() {
        assert!(!p.is_null(), "null at {i}");
        unsafe { core::ptr::write_bytes(p, 0xA5, size) };
    }

    // Free storm: free everything.
    for &p in &buf {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }

    // Churn: re-alloc/free the same count repeatedly. Free-drain runs first,
    // so this reuses freed blocks; every live snapshot stays distinct.
    for round in 0..4 {
        let mut buf2 = vec![core::ptr::null_mut::<u8>(); N];
        let got2 = core.refill_class_bump(c, &mut buf2);
        assert_eq!(got2, N, "churn round {round} refill short: {got2}/{N}");
        let uniq2: HashSet<usize> = buf2.iter().map(|p| *p as usize).collect();
        assert_eq!(uniq2.len(), N, "duplicate pointer in churn round {round}");
        for &p in &buf2 {
            assert!(!p.is_null());
            unsafe { core::ptr::write_bytes(p, 0x5A, size) };
        }
        for &p in &buf2 {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { core.dealloc(p, layout) };
        }
    }

    // Allocator healthy after churn.
    let check = core.alloc(layout);
    assert!(!check.is_null(), "alloc after churn returned null");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(check, layout) };
}

// ---------------------------------------------------------------------------
// D1 exactness: after a cold-storm + free-storm, non-current segments must
// decommit (live_count → 0). Only under alloc-decommit (the counter + decommit
// hook are gated on it).
// ---------------------------------------------------------------------------

// Skipped under miri (R3, #155): the >=3-segment assertion needs N=12_000
// (12 MiB of 1 KiB blocks), which is size-load-bearing and cannot be capped;
// interpreting that under miri would run for many minutes. The decommit-cycle
// UB coverage is already provided by the dedicated `decommit_miri_cycle` test.
#[cfg(all(feature = "alloc-decommit", not(miri)))]
#[test]
fn d1_cold_storm_then_free_decommits() {
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool — this test
    // asserts the decommit hook FIRES when non-current segments empty. With the
    // pool ON (production default) that churn is absorbed by the pool (retained
    // committed, no decommit). Disabling it exercises the flush→decommit path
    // this test covers (still live under `production` on pool-full/disabled).
    // Pool behaviour is covered by `tests/small_segment_pool.rs`.
    let mut core = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .unwrap();
    // Larger blocks so a moderate N spans multiple segments (a non-current
    // segment is what can decommit; the primordial and small_cur do not).
    let size = 1024usize;
    let align = 8usize;
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    // Enough to span >= 3 segments (primordial + >=2 Small).
    const N: usize = 12_000;
    let mut buf = vec![core::ptr::null_mut::<u8>(); N];
    let got = core.refill_class_bump(c, &mut buf);
    assert_eq!(got, N, "cold-storm refill short: {got}/{N}");

    // Distinct, spanning multiple segments.
    let bases: HashSet<usize> = buf.iter().map(|&p| seg_base(p)).collect();
    assert!(
        bases.len() >= 3,
        "need >= 3 segments for the decommit assertion, got {}",
        bases.len()
    );

    let decommit_before = AllocCore::dbg_decommit_count();

    // Free storm: return every block. Non-current Small segments that reach
    // live_count == 0 must decommit.
    for &p in &buf {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }

    let decommit_after = AllocCore::dbg_decommit_count();
    assert!(
        decommit_after > decommit_before,
        "D1 broken: no segment decommitted after the full free-storm \
         (before={decommit_before}, after={decommit_after}). A bump-direct \
         over-count or bitmap-free carve would leave live_count > 0 forever."
    );

    // Every still-registered small segment that holds one of our (now-freed)
    // blocks must report live_count == 0 OR be decommitted. We check the
    // per-block live counts: a freed block's segment is either decommitted
    // (dbg_live_count_for returns Some(0) after reset, or the segment was
    // recycled → None) — in all cases NOT a positive leak.
    for &p in &buf {
        if let Some(lc) = core.dbg_live_count_for(p) {
            assert_eq!(
                lc, 0,
                "D1 broken: segment of a fully-freed block still has \
                 live_count={lc} (expected 0)"
            );
        }
    }
}
