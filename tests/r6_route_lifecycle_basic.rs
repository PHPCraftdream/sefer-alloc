#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SegmentLayout;

#[test]
fn heap_routes_primordial_before_issue_and_each_new_segment_before_return() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let primordial = heap.segment_bases().next().expect("primordial");
    let first = RouteDirectory::global()
        .lookup(primordial)
        .expect("primordial route exists before first allocation");
    assert_eq!(first.kind(), RouteKind::Primordial);
    assert_eq!(first.owner(), heap.id() as usize);
    drop(first);

    let small = Layout::from_size_align(SegmentLayout::SMALL_MAX, 16).unwrap();
    let mut blocks = Vec::new();
    let mut secondary = None;
    for _ in 0..(SegmentLayout::SEGMENT / SegmentLayout::SMALL_MAX + 128) {
        let p = heap.alloc(small);
        assert!(!p.is_null());
        let pin = RouteDirectory::global()
            .lookup(p)
            .expect("every issued small block has a route");
        assert_eq!(pin.owner(), heap.id() as usize);
        if pin.kind() == RouteKind::Small {
            secondary = Some(p);
        }
        blocks.push(p);
        if secondary.is_some() {
            break;
        }
    }
    assert!(
        secondary.is_some(),
        "workload must reserve a new Small segment"
    );

    let large = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    let mut large_blocks = Vec::new();
    let mut retained_pin = None;
    for _ in 0..7 {
        let p = heap.alloc(large);
        assert!(!p.is_null());
        let pin = RouteDirectory::global()
            .lookup(p)
            .expect("Large route exists before return");
        assert_eq!(pin.kind(), RouteKind::Large);
        assert_eq!(pin.owner(), heap.id() as usize);
        if retained_pin.is_none() {
            retained_pin = Some(pin);
        }
        large_blocks.push(p);
    }
    // Primordial + Small + seven Large routes force RouteSlots to grow and
    // move non-Copy handles while an earlier pin remains usable.
    assert_eq!(retained_pin.unwrap().kind(), RouteKind::Large);

    // SAFETY: all pointers came from this heap with their matching layouts.
    unsafe {
        for p in large_blocks {
            heap.dealloc(p, large);
        }
        for p in blocks {
            heap.dealloc(p, small);
        }
    }
    drop(lease);
    // Recycling a registry slot retains the HeapCore and its primordial mapping.
    assert!(RouteDirectory::global().lookup(primordial).is_some());
}
