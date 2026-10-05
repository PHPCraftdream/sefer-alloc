#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::{SeferAlloc, SegmentLayout};
use std::alloc::{GlobalAlloc, Layout};

#[test]
fn realloc_grown_large_routes_remote_free_to_its_owner() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let initial = Layout::from_size_align(SegmentLayout::SMALL_MAX + 4096, 16).unwrap();
    let grown_layout =
        Layout::from_size_align(initial.size() + SegmentLayout::SEGMENT, 16).unwrap();
    let first = heap.alloc(initial);
    assert!(!first.is_null());
    // SAFETY: first is this heap's current unique allocation with initial Layout.
    let grown = unsafe { heap.realloc(first, initial, grown_layout.size()) };
    assert!(!grown.is_null());
    let old_route = RouteDirectory::global().lookup(grown).unwrap();
    let old_incarnation = old_route.incarnation();
    let address = grown.expose_provenance();
    std::thread::spawn(move || {
        // SAFETY: grown is uniquely transferred, with its realloc-produced Layout.
        unsafe {
            SeferAlloc::new().dealloc(std::ptr::with_exposed_provenance_mut(address), grown_layout);
        }
    })
    .join()
    .unwrap();
    let reissued = heap.alloc(grown_layout);
    assert_eq!(
        reissued, grown,
        "owner allocation must consume the resized descriptor"
    );
    assert_ne!(
        RouteDirectory::global()
            .lookup(reissued)
            .unwrap()
            .incarnation(),
        old_incarnation
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    drop(old_route);
    // SAFETY: reissued is a new unique allocation instance with grown_layout.
    unsafe { heap.dealloc(reissued, grown_layout) };
    // Drop of the lease recycles the slot (LIVE -> FREE, Release).
    drop(lease);
}
