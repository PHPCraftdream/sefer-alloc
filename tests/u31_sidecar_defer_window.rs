#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

//! U31 (docs/perf/OPEN_ITEMS.md item 90): after a routed linear scan whose
//! sidecar drains reclaimed nothing (three in a row), the owner defers the
//! drains while `small_cur` carves `drained range / 16` more. Pins four
//! properties on a real routed `AllocCore`:
//!
//! 1. activation — a growing heap defers most discovery drains, so the
//!    drain count falls far below the discovery count (base: equal);
//! 2. bound — the window never reaches past 1/16 of the payload range
//!    drained so far (`end - bump <= (bump - payload start) / 8 + one refill`);
//! 3. liveness — a free published while the window is open is reissued
//!    once the bump passes the window end (plus one refill batch);
//! 4. reset — the drain that reclaims it resets the empty-scan streak.
//!
//! A repeated discovery with no carve in between is never deferred; that
//! half is pinned by the publish-then-discover protocol tests
//! (`r10_active_kind_late_publish`, `r12_o4_routed_miss_counters`,
//! `b4_u31_trigger_growth16_oracle`), which drive no allocation between
//! the empty scan and the publication.
//!
//! One `#[test]`: the B4 counters are process-global statics.

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SeferAlloc};

/// `B4_FIND_SEGMENT_CALLS` and `B4_SCAN_CALLS` (positional, see
/// `SeferAlloc::dbg_b4_collapse_counters`).
fn find_and_scan_calls() -> (u64, u64) {
    let c = SeferAlloc::dbg_b4_collapse_counters();
    (c[0], c[1])
}

#[test]
fn u31_defers_empty_drains_within_bound_and_reissues_published_free() {
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let block = AllocCore::dbg_block_size(0);
    let class_zero = Layout::from_size_align(block, 8).unwrap();
    // One carve refill: the caller's block plus REFILL_BATCH (31) extras.
    let refill_bytes = 32 * block;

    // Warm-up block fixes the segment and its payload start.
    let first = core.alloc(class_zero);
    assert!(!first.is_null());
    let (payload_start, _, _, _) = core.dbg_sidecar_defer_window();
    let mut live = vec![first];

    // 1 + 2: grow the heap; check the bound after every allocation.
    let (find0, scan0) = find_and_scan_calls();
    let deferred0 = SeferAlloc::dbg_b4_u31_deferred_scans();
    for _ in 0..8192 {
        let ptr = core.alloc(class_zero);
        assert!(!ptr.is_null());
        live.push(ptr);
        let (bump, _, until, _) = core.dbg_sidecar_defer_window();
        if until > bump {
            let range = bump - payload_start;
            assert!(
                until - bump <= range / 16 + refill_bytes,
                "window {} past bump {bump} exceeds 1/16 of drained range {range}",
                until - bump
            );
        }
    }
    let (find1, scan1) = find_and_scan_calls();
    let deferred = SeferAlloc::dbg_b4_u31_deferred_scans() - deferred0;
    let (finds, scans) = (find1 - find0, scan1 - scan0);
    assert!(
        finds >= 200,
        "growth must reach discovery on refill misses: {finds}"
    );
    assert!(deferred > 0, "no deferred scan in a growing heap");
    assert!(
        scans * 2 < finds,
        "drains must fall far below discoveries: {scans} drains / {finds} discoveries"
    );

    // 3: open a window, publish a free inside it, keep allocating.
    let mut guard = 0;
    while {
        let (bump, _, until, _) = core.dbg_sidecar_defer_window();
        until <= bump
    } {
        let ptr = core.alloc(class_zero);
        assert!(!ptr.is_null());
        live.push(ptr);
        guard += 1;
        assert!(guard < 65_536, "window never opened");
    }
    let (_, _, until_at_publish, _) = core.dbg_sidecar_defer_window();
    let victim = live.remove(live.len() / 2);
    // SAFETY: `victim` is a live class-zero block of `core`; its single free
    // is transferred to the sidecar here and never freed directly.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim) });
    let mut reissued_at = None;
    for _ in 0..65_536 {
        let ptr = core.alloc(class_zero);
        assert!(!ptr.is_null());
        live.push(ptr);
        if ptr == victim {
            let (bump, _, _, streak) = core.dbg_sidecar_defer_window();
            assert_eq!(
                streak, 0,
                "the reclaiming drain must reset the empty-scan streak"
            );
            reissued_at = Some(bump);
            break;
        }
    }
    let bump = reissued_at.expect("published free was never reissued");
    assert!(
        bump <= until_at_publish + 2 * refill_bytes,
        "reissue at bump {bump} is past the window end {until_at_publish} by more than a refill"
    );

    for ptr in live {
        // SAFETY: each pointer is a live, distinct class-zero block (the
        // victim was reissued and is in `live` again).
        unsafe { core.dealloc(ptr, class_zero) };
    }
}
