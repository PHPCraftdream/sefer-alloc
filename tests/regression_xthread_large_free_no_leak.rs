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
fn large_owner_slow_path_consumes_descriptor_obligation_before_cache_reuse() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX + 4096, 16).unwrap();
    let first = heap.alloc(layout);
    assert!(!first.is_null());
    let pin = RouteDirectory::global().lookup(first).unwrap();
    let incarnation = pin.incarnation();
    let address = first.expose_provenance();
    std::thread::spawn(move || {
        // SAFETY: first is transferred uniquely to this producer and freed once.
        unsafe {
            SeferAlloc::new().dealloc(std::ptr::with_exposed_provenance_mut(address), layout);
        }
    })
    .join()
    .unwrap();
    // The allocation entry point, not a diagnostic sweep, consumes the pending
    // Large descriptor before considering physical cache reuse.
    let second = heap.alloc(layout);
    assert!(!second.is_null());
    assert_eq!(second, first);
    let next = RouteDirectory::global().lookup(second).unwrap();
    assert_ne!(next.incarnation(), incarnation);
    assert_eq!(
        pin.incarnation(),
        incarnation,
        "old descriptor pin remains independent"
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    drop(pin);
    drop(next);
    // SAFETY: second is a new current allocation instance, freed exactly once.
    unsafe { heap.dealloc(second, layout) };
    // Drop of the lease recycles the slot (LIVE -> FREE, Release).
    drop(lease);
}
