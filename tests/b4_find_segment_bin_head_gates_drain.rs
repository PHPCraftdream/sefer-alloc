#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

//! B4 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS) counterfactual for FIX 2:
//! `find_segment`'s linear scan must gate `drain_segment_sidecar` on a
//! pre-drain bin read (`pre_nonempty`). The observer is the static
//! `B4_SCAN_CALLS` counter (index 1 of
//! `SeferAlloc::dbg_b4_collapse_counters()`, incremented at
//! `drain_segment_sidecar` entry): an empty bin drains (delta == 1 and the
//! published block is found), a non-empty bin must NOT drain (delta == 0).
//! Scenario mirrors `tests/r12_o4_routed_miss_counters.rs` (b) then (a), but
//! deliberately WITHOUT materialising the segment directory: r12_o4's
//! `SMALL_MAX`-sized setup allocations carve ~32 Small segments, each of
//! which has an empty class-0 bin and therefore drains too (delta 32,
//! segment-count dependent). Skipping the directory materialisation leaves
//! exactly ONE Small segment, making the drain-count deltas exact.
//! The non-empty-bin case needs no directory to hide from: the bin head is
//! set, so `pre_nonempty` gates the drain regardless of the lookup path.
//!
//! Minimal feature set: `production` supplies `alloc-global`/`alloc-xthread`
//! (compile gates of `pre_nonempty`/`drain_segment_sidecar`); `internals` +
//! `bench-internals` gate the `dbg_*` hooks and the B4 counters;
//! `alloc-stats` is NOT required by this test (no alloc-stats counters are
//! read) and is enabled only to match the r12_o4 header gate.

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

/// Index of `B4_SCAN_CALLS` in `dbg_b4_collapse_counters` (positional,
/// pinned by the probe).
fn b4_scan_calls() -> u64 {
    sefer_alloc::SeferAlloc::dbg_b4_collapse_counters()[1]
}

#[test]
fn b4_empty_bin_drains_and_finds_published_block_but_nonempty_bin_does_not() {
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let class_zero = Layout::from_size_align(AllocCore::dbg_block_size(0), 8).unwrap();

    // 32 allocations carve ONE Small segment plus its 31-block refill and
    // leave its class-0 bin empty again.
    let mut live = Vec::new();
    for _ in 0..32 {
        let ptr = core.alloc(class_zero);
        assert!(!ptr.is_null());
        live.push(ptr);
    }
    let slot = core.dbg_segment_id_of(live[0]) as usize;

    // CASE "empty bin": a terminal publication behind an empty bin — the scan
    // MUST drain the sidecar (B4_SCAN_CALLS delta == 1) and find the
    // victim's segment.
    let victim1 = live.remove(0);
    // SAFETY: victim1 is a live class-zero block of `core`; its single free is
    // transferred to the sidecar here and never freed again directly.
    assert!(unsafe { core.dbg_publish_small_sidecar_free(victim1) });

    let before = b4_scan_calls();
    let found = core.dbg_find_segment_with_free(0);
    let delta = b4_scan_calls() - before;
    assert!(found.is_some(), "empty-bin case must find a segment");
    assert_eq!(
        core.dbg_segment_id_of(found.unwrap()) as usize,
        slot,
        "found segment must be victim1's"
    );
    assert_eq!(
        delta, 1,
        "empty bin: exactly one sidecar drain must be performed (B4 FIX 2)"
    );

    // CASE "non-empty bin": reissue the drained block, then free it back into
    // the bin. The bin head is now set, so the pre-drain gate must skip the
    // sidecar drain (delta == 0) even though a published bit exists.
    let reused = core.alloc(class_zero);
    assert_eq!(reused, victim1, "the drained block is the one reissued");
    // SAFETY: reused is a live class-zero block, freed exactly once here.
    unsafe { core.dealloc(reused, class_zero) };
    if core.dbg_directory_is_materialised() {
        // Belt-and-braces (only meaningful if the directory exists here):
        // prove the block really sits in the bin, then simulate directory
        // drift so the lookup goes through the linear scan, not a bit hit.
        assert_eq!(core.dbg_directory_get_bit(0, slot), Some(true));
        assert!(core.dbg_directory_force_clear_bit(0, slot));
    }

    let before = b4_scan_calls();
    let found = core.dbg_find_segment_with_free(0);
    let delta = b4_scan_calls() - before;
    assert!(found.is_some(), "non-empty bin case must find a segment");
    assert_eq!(
        core.dbg_segment_id_of(found.unwrap()) as usize,
        slot,
        "found segment must be victim1's"
    );
    assert_eq!(
        delta, 0,
        "non-empty bin: the pre-drain gate must skip the sidecar drain (B4 FIX 2 counterfactual)"
    );

    // Cleanup: victim1's ownership was transferred to the allocator by the
    // publish/reissue cycle above; free everything still live.
    for ptr in live {
        // SAFETY: each pointer is a live, distinct class-zero block.
        unsafe { core.dealloc(ptr, class_zero) };
    }
}
