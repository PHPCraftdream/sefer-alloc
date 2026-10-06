#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

/// Pins routed miss behavior: a negative directory lookup does not use the
/// standalone trusted-negative cadence; fallback scans and self-healing run.
#[test]
fn routed_negative_directory_miss_scans_and_self_heals() {
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let large_class = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let class_zero = Layout::from_size_align(AllocCore::dbg_block_size(0), 8).unwrap();

    // Materialize the directory using many distinct Small roots, without
    // populating class zero's BinTable.
    let mut large_class_live = Vec::new();
    for _ in 0..4096 {
        let ptr = core.alloc(large_class);
        assert!(!ptr.is_null(), "setup allocation failed");
        large_class_live.push(ptr);
        if core.dbg_directory_is_materialised() {
            break;
        }
    }
    assert!(
        core.dbg_directory_is_materialised(),
        "setup crosses directory threshold"
    );

    // Put two class-zero blocks on one root, then free both and clear its bit.
    let a = core.alloc(class_zero);
    let b = core.alloc(class_zero);
    assert!(!a.is_null() && !b.is_null());
    assert_eq!(
        SegmentLayout::segment_base_of(a as usize),
        SegmentLayout::segment_base_of(b as usize),
        "back-to-back allocations should share a root"
    );
    // SAFETY: both are live class-zero allocations, freed exactly once.
    unsafe {
        core.dealloc(a, class_zero);
        core.dealloc(b, class_zero);
    }
    let target_slot = core.dbg_segment_id_of(a) as usize;
    assert_eq!(core.dbg_directory_get_bit(0, target_slot), Some(true));
    assert!(core.dbg_directory_force_clear_bit(0, target_slot));
    // Confirm the deliberately cleared bit is a real negative lookup before
    // exercising the routed fallback oracle.
    assert_eq!(core.dbg_directory_get_bit(0, target_slot), Some(false));

    let authoritative_before = AllocCore::dbg_directory_authoritative_miss();
    let probes_before = AllocCore::dbg_full_scan_slots_examined();
    let heals_before = AllocCore::dbg_directory_miss_self_heal();
    assert!(core.dbg_find_segment_with_free(0).is_some());
    assert_eq!(
        AllocCore::dbg_directory_authoritative_miss(),
        authoritative_before,
        "routed negative lookup must not activate the trusted-negative path"
    );
    assert!(AllocCore::dbg_full_scan_slots_examined() > probes_before);
    assert!(AllocCore::dbg_directory_miss_self_heal() > heals_before);
    assert_eq!(core.dbg_directory_get_bit(0, target_slot), Some(true));

    for ptr in large_class_live {
        // SAFETY: each pointer is live, distinct, and freed once here.
        unsafe { core.dealloc(ptr, large_class) };
    }
}
