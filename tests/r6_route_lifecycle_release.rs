#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::{LargeCacheConfig, SegmentLayout};

#[test]
fn uncached_large_removes_route_before_os_release() {
    let mut lease =
        HeapRegistry::dbg_claim_lease_with_config(LargeCacheConfig::new().budget_bytes(0))
            .expect("claim_with_config");
    let heap = lease.core();
    let large = Layout::from_size_align(
        SegmentLayout::SMALL_MAX + SegmentLayout::PAGE,
        SegmentLayout::PAGE,
    )
    .unwrap();
    let p = heap.alloc(large);
    assert!(!p.is_null());
    let pin = RouteDirectory::global().lookup(p).unwrap();
    // SAFETY: p is live and allocated by this heap with large.
    unsafe { heap.dealloc(p, large) };
    assert!(RouteDirectory::global().lookup(p).is_none());
    assert_eq!(pin.kind(), RouteKind::Large);
    drop(pin);
    drop(lease);
}
