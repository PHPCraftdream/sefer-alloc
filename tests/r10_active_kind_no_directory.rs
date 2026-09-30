#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats",
    not(feature = "alloc-segment-directory")
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn no_directory_fallback_ignores_active_large_roots() {
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let large_ptr = core.alloc(large);
    assert!(!large_ptr.is_null());
    let small = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
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
    let before = AllocCore::dbg_full_scan_slots_examined();
    assert_eq!(core.dbg_find_segment_with_free(0), None);
    assert_eq!(AllocCore::dbg_full_scan_slots_examined() - before, 2);
    for ptr in small_ptrs {
        // SAFETY: each is a distinct live Small allocation of `small`.
        unsafe { core.dealloc(ptr, small) };
    }
    // SAFETY: large_ptr is the one live Large allocation.
    unsafe { core.dealloc(large_ptr, large) };
    assert!(core.dbg_active_kind_census().2);
}
