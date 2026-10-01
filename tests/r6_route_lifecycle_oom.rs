#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SegmentLayout;

#[test]
fn registration_oom_rolls_back_primordial_small_and_large_before_issue() {
    RouteDirectory::fail_next_registration_for_test();
    assert!(HeapRegistry::claim().is_null());

    let heap_ptr = HeapRegistry::claim();
    assert!(
        !heap_ptr.is_null(),
        "failed first materialization is retryable"
    );
    // SAFETY: this thread owns the claimed heap until recycle below.
    let heap = unsafe { &mut *heap_ptr };
    let before = heap.dbg_table_count();
    let large = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    RouteDirectory::fail_next_registration_for_test();
    assert!(heap.alloc(large).is_null());
    assert_eq!(heap.dbg_table_count(), before);

    let p = heap.alloc(large);
    assert!(!p.is_null());
    assert!(RouteDirectory::global().lookup(p).is_some());

    let small = Layout::from_size_align(SegmentLayout::SMALL_MAX, 16).unwrap();
    let mut blocks = Vec::new();
    // A magazine miss may retry after its full Large rescue.
    RouteDirectory::fail_next_registrations_for_test(if cfg!(feature = "fastbin") { 2 } else { 1 });
    loop {
        assert!(blocks.len() < SegmentLayout::SEGMENT / SegmentLayout::SMALL_MAX + 128);
        let before = heap.dbg_table_count();
        let block = heap.alloc(small);
        if block.is_null() {
            assert_eq!(heap.dbg_table_count(), before);
            break;
        }
        assert!(RouteDirectory::global().lookup(block).is_some());
        blocks.push(block);
    }
    let recovered = heap.alloc(small);
    assert!(!recovered.is_null());
    assert!(RouteDirectory::global().lookup(recovered).is_some());
    // SAFETY: p came from this heap with large; the slot is then recycled.
    unsafe {
        heap.dealloc(p, large);
        for block in blocks {
            heap.dealloc(block, small);
        }
        heap.dealloc(recovered, small);
        HeapRegistry::recycle(heap_ptr);
    }
}
