#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats"
))]

use core::alloc::Layout;
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

#[test]
fn h128_to_one_large_and_first_small_magazine_miss_probe_one_slot() {
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null());
    // SAFETY: the claim gives this thread exclusive ownership until recycle.
    let heap = unsafe { &mut *heap_ptr };
    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let mut ptrs = Vec::with_capacity(127);
    for issued in 1..=127 {
        let ptr = heap.alloc(large);
        assert!(!ptr.is_null());
        ptrs.push(ptr);
        assert_eq!(heap.dbg_active_kind_census(), (1, issued, true));
    }
    assert_eq!(heap.dbg_table_count(), 128);
    assert_eq!(heap.dbg_active_kind_census(), (1, 127, true));
    for (retired, ptr) in ptrs.drain(..126).enumerate() {
        // SAFETY: each ptr is a distinct live allocation with this layout.
        unsafe { heap.dealloc(ptr, large) };
        assert_eq!(heap.dbg_active_kind_census(), (1, 126 - retired, true));
    }
    assert_eq!(heap.dbg_table_count(), 128);
    assert_eq!(heap.dbg_active_kind_census(), (1, 1, true));
    assert!(heap.dbg_large_cache_used() > 0);

    let before = HeapCore::dbg_large_sidecar_slot_inspections();
    let extra = heap.alloc(large);
    assert!(!extra.is_null());
    assert_eq!(HeapCore::dbg_large_sidecar_slot_inspections() - before, 1);
    assert_eq!(heap.dbg_active_kind_census(), (1, 2, true));
    // SAFETY: extra is the still-live allocation just returned by heap.alloc.
    unsafe { heap.dealloc(extra, large) };
    assert_eq!(heap.dbg_active_kind_census(), (1, 1, true));

    let small = Layout::from_size_align(64, 8).unwrap();
    let class = SegmentLayout::class_for(64, 8).unwrap();
    assert_eq!(heap.dbg_tcache_count(class), 0);
    let before = HeapCore::dbg_large_sidecar_slot_inspections();
    let block = heap.alloc(small);
    assert!(!block.is_null());
    assert_eq!(HeapCore::dbg_large_sidecar_slot_inspections() - before, 1);
    assert_eq!(heap.dbg_active_kind_census(), (1, 1, true));
    // SAFETY: block and ptrs[0] remain live, each freed once with its layout.
    unsafe {
        heap.dealloc(block, small);
        heap.dealloc(ptrs[0], large);
    }
    assert!(heap.dbg_active_kind_census().2);
    // SAFETY: this is the same claimed heap pointer, no reference is used later.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}
