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
fn pool_release_and_large_cache_reissue_balance_routes() {
    let config = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let heap_ptr = HeapRegistry::claim_with_config(config);
    assert!(!heap_ptr.is_null());
    // SAFETY: the claimed slot has one owner until recycle below.
    let heap = unsafe { &mut *heap_ptr };
    let primordial = heap.segment_bases().next().expect("primordial");
    assert_eq!(
        RouteDirectory::global().lookup(primordial).unwrap().kind(),
        RouteKind::Primordial
    );
    let small = Layout::from_size_align(SegmentLayout::SMALL_MAX, 16).unwrap();
    let mut blocks = Vec::new();
    let mut former_base = None;
    let mut small_ptr = None;
    loop {
        assert!(blocks.len() < 2 * SegmentLayout::SEGMENT / SegmentLayout::SMALL_MAX + 256);
        let p = heap.alloc(small);
        assert!(!p.is_null());
        let kind = RouteDirectory::global().lookup(p).unwrap().kind();
        blocks.push(p);
        if kind == RouteKind::Small {
            let base = p.addr() & !(SegmentLayout::SEGMENT - 1);
            if let Some(former) = former_base {
                if base != former {
                    break;
                }
            } else {
                former_base = Some(base);
                small_ptr = Some(p);
            }
        }
    }
    let small_ptr = small_ptr.expect("first ordinary Small segment");
    let retained_pin = RouteDirectory::global().lookup(small_ptr).unwrap();
    // SAFETY: each block is live and belongs to this heap and layout.
    unsafe {
        for p in blocks {
            heap.dealloc(p, small);
        }
    }
    #[cfg(feature = "fastbin")]
    heap.dbg_flush_all();
    heap.dbg_drain_small_pool();
    assert!(RouteDirectory::global().lookup(small_ptr).is_none());
    assert_eq!(retained_pin.kind(), RouteKind::Small);
    drop(retained_pin);

    let large = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    let first = heap.alloc(large);
    assert!(!first.is_null());
    let old = RouteDirectory::global()
        .lookup(first)
        .unwrap()
        .incarnation();
    // SAFETY: first was allocated by this heap with large.
    unsafe { heap.dealloc(first, large) };
    assert!(RouteDirectory::global().lookup(first).is_none());
    let second = heap.alloc(large);
    assert_eq!(
        second, first,
        "same-size request should hit the Large cache"
    );
    let new = RouteDirectory::global().lookup(second).unwrap();
    assert_eq!(new.kind(), RouteKind::Large);
    assert_ne!(new.incarnation(), old);
    drop(new);
    // SAFETY: second is the live reissue with the same layout.
    unsafe {
        heap.dealloc(second, large);
        HeapRegistry::recycle(heap_ptr);
    }
}
