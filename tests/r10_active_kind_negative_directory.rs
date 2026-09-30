#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn routed_negative_and_forced_rescue_inspect_only_small_roots() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let mut large_ptrs = Vec::with_capacity(127);
    for _ in 0..127 {
        let ptr = core.alloc(large);
        assert!(!ptr.is_null());
        large_ptrs.push(ptr);
    }
    assert_eq!(core.dbg_table_count(), 128);
    for ptr in large_ptrs.drain(..126) {
        // SAFETY: each ptr is distinct, live, and freed once with its layout.
        unsafe { core.dealloc(ptr, large) };
    }
    assert_eq!(core.dbg_active_kind_census(), (1, 1, true));

    let small = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    assert!(SegmentLayout::class_for(SegmentLayout::SMALL_MAX, 1).is_some());
    let mut small_ptrs = Vec::new();
    for _ in 0..64 {
        let ptr = core.alloc(small);
        assert!(!ptr.is_null());
        small_ptrs.push(ptr);
        if core.dbg_active_kind_census().0 == 2 {
            break;
        }
    }
    assert_eq!(core.dbg_active_kind_census(), (2, 1, true));
    assert!(core.dbg_directory_is_materialised());
    assert_eq!(core.dbg_directory_get_bit(0, 0), Some(false));
    let fallback_before = AllocCore::dbg_directory_fallback_scans();
    let probes_before = AllocCore::dbg_full_scan_slots_examined();
    assert_eq!(core.dbg_find_segment_with_free(0), None);
    assert_eq!(
        AllocCore::dbg_directory_fallback_scans() - fallback_before,
        1
    );
    assert_eq!(AllocCore::dbg_full_scan_slots_examined() - probes_before, 2);

    let probes_before = AllocCore::dbg_full_scan_slots_examined();
    assert_eq!(core.dbg_directory_rescue_scan(0), None);
    assert_eq!(AllocCore::dbg_full_scan_slots_examined() - probes_before, 2);
    assert_eq!(core.dbg_active_kind_census(), (2, 1, true));

    for ptr in small_ptrs {
        // SAFETY: each ptr is a distinct live allocation of `small`.
        unsafe { core.dealloc(ptr, small) };
    }
    // SAFETY: the one retained Large ptr is still live.
    unsafe { core.dealloc(large_ptrs[0], large) };
    assert!(core.dbg_active_kind_census().2);
}
