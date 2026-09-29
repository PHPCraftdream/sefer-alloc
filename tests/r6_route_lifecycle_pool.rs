#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::{LargeCacheConfig, SegmentLayout, SmallSegmentPoolConfig};

#[test]
fn pooled_small_keeps_route_until_pool_drain() {
    let config = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(1));
    let heap_ptr = HeapRegistry::claim_with_config(config);
    assert!(!heap_ptr.is_null());
    // SAFETY: this thread owns the claimed heap until recycle below.
    let heap = unsafe { &mut *heap_ptr };
    let small = Layout::from_size_align(SegmentLayout::SMALL_MAX, 16).unwrap();
    let mut blocks = Vec::new();
    let mut prior_base = None;
    let mut prior_ptr = None;
    loop {
        assert!(blocks.len() < 2 * SegmentLayout::SEGMENT / SegmentLayout::SMALL_MAX + 256);
        let p = heap.alloc(small);
        assert!(!p.is_null());
        blocks.push(p);
        if RouteDirectory::global().lookup(p).unwrap().kind() != RouteKind::Small {
            continue;
        }
        let base = p.addr() & !(SegmentLayout::SEGMENT - 1);
        if let Some(prior) = prior_base {
            if base != prior {
                break;
            }
        } else {
            prior_base = Some(base);
            prior_ptr = Some(p);
        }
    }
    let prior_ptr = prior_ptr.expect("first ordinary Small segment");
    // SAFETY: every block was issued by this heap with `small`.
    unsafe {
        for p in blocks {
            heap.dealloc(p, small);
        }
    }
    #[cfg(feature = "fastbin")]
    heap.dbg_flush_all();
    assert_eq!(heap.dbg_pooled_count(), 1);
    assert!(RouteDirectory::global().lookup(prior_ptr).is_some());
    assert_eq!(heap.dbg_drain_small_pool(), 1);
    assert!(RouteDirectory::global().lookup(prior_ptr).is_none());
    // SAFETY: the claimed heap is no longer in use.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}
