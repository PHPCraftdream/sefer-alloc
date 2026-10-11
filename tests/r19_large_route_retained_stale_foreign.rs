//! PRF03 B: retained routes and invalid-use rejection parity, not valid-use
//! safety claims for stale/double frees. No global allocator or maintenance.
//! A local mutex serializes all global census/counter observations in this binary.
#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats"
))]

use core::alloc::Layout;
use sefer_alloc::registry::{bootstrap, segment_route::RouteDirectory, HeapCore, HeapRegistry};
use sefer_alloc::AllocCore;
use std::sync::Mutex;

static SERIAL: Mutex<()> = Mutex::new(());

fn layout() -> Layout {
    Layout::from_size_align(4 * 1024 * 1024, 16).unwrap()
}

fn cache(heap: &HeapCore) -> (usize, [Option<usize>; 8], u64) {
    (
        heap.dbg_large_cache_used(),
        heap.dbg_large_cache_slot_sizes(),
        heap.dbg_large_cache_hits(),
    )
}

fn foreign(address: usize, realloc: bool) -> bool {
    std::thread::spawn(move || {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("foreign lease");
        let heap = lease.core();
        // Numeric key only: the tested rejection paths must not read cached bytes.
        let ptr = core::ptr::without_provenance_mut(address);
        if realloc {
            // SAFETY: deliberate invalid-use hardening probe of a mapped cached
            // reservation, with the owner paused until join; expected null before
            // any source read. This is NOT the ordinary realloc caller contract.
            unsafe { heap.realloc(ptr, layout(), layout().size() * 2).is_null() }
        } else {
            // SAFETY: deliberate stale-free rejection probe, not a valid transfer;
            // owner retains the mapped cache and does not reuse/evict until join.
            unsafe { heap.dealloc(ptr, layout()) };
            true
        }
    })
    .join()
    .expect("foreign thread")
}

#[test]
fn cached_stale_foreign_free_preserves_cache_and_next_hit() {
    let _serial = SERIAL.lock().expect("no prior test panic");
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("owner lease");
    let heap = lease.core();
    let ptr = heap.alloc(layout());
    assert!(!ptr.is_null());
    // SAFETY: exact live start/layout, transferred once into the owner's cache.
    unsafe { heap.dealloc(ptr, layout()) };
    assert!(heap.dbg_large_cache_used() > 0, "cache admission");
    let cached = cache(heap);
    let routes = RouteDirectory::global().live_route_census_for_test().2;
    let issued = RouteDirectory::global().incarnations_issued_for_test();
    let count = AllocCore::dbg_foreign_or_unroutable_frees();
    assert!(foreign(ptr.addr(), false));
    assert_eq!(AllocCore::dbg_foreign_or_unroutable_frees(), count + 1);
    assert_eq!(cache(heap), cached);
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test().2,
        routes
    );
    // A foreign lease may register a primordial route; isolate the hit delta.
    let before_hit = RouteDirectory::global().incarnations_issued_for_test();
    assert!(before_hit >= issued);
    let hit = heap.alloc(layout());
    assert_eq!(hit, ptr);
    assert_eq!(heap.dbg_large_cache_hits(), cached.2 + 1);
    // B retains the same-payload registration.
    assert_eq!(
        RouteDirectory::global().incarnations_issued_for_test(),
        before_hit
    );
    assert!(RouteDirectory::global().lookup(hit).is_some());
    // SAFETY: hit is the new uniquely issued instance, freed once.
    unsafe { heap.dealloc(hit, layout()) };
}

#[test]
fn cached_stale_foreign_realloc_returns_null() {
    let _serial = SERIAL.lock().expect("no prior test panic");
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("owner lease");
    let heap = lease.core();
    let ptr = heap.alloc(layout());
    assert!(!ptr.is_null());
    // SAFETY: exact live start/layout, freed once into the cache.
    unsafe { heap.dealloc(ptr, layout()) };
    assert!(heap.dbg_large_cache_used() > 0, "cache admission");
    let cached = cache(heap);
    let routes = RouteDirectory::global().live_route_census_for_test().2;
    assert!(foreign(ptr.addr(), true));
    assert_eq!(cache(heap), cached);
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test().2,
        routes
    );
    let hit = heap.alloc(layout());
    assert_eq!(hit, ptr);
    assert_eq!(heap.dbg_large_cache_hits(), cached.2 + 1);
    // SAFETY: new current instance with its exact layout, freed once.
    unsafe { heap.dealloc(hit, layout()) };
}

#[test]
fn own_free_after_foreign_publish_is_counted_noop() {
    let _serial = SERIAL.lock().expect("no prior test panic");
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("owner lease");
    let heap = lease.core();
    let ptr = heap.alloc(layout());
    assert!(!ptr.is_null());
    let pin = RouteDirectory::global()
        .lookup(ptr)
        .expect("live Large route");
    std::thread::spawn(move || {
        // SAFETY: the pin transfers this issued Large instance exactly once;
        // owner is paused until join. Only independent route metadata is touched.
        assert!(unsafe { pin.publish_large() });
    })
    .join()
    .expect("publisher");
    assert!(RouteDirectory::global().lookup(ptr).is_some());
    let count = AllocCore::dbg_foreign_or_unroutable_frees();
    let cached = cache(heap);
    let routes = RouteDirectory::global().live_route_census_for_test().2;
    // SAFETY: deliberate sequential double-free hardening probe, not valid use.
    // Physical header is still LIVE and mapped; no producer remains active.
    unsafe { heap.dealloc(ptr, layout()) };
    assert_eq!(AllocCore::dbg_foreign_or_unroutable_frees(), count + 1);
    assert_eq!(cache(heap), cached);
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test().2,
        routes
    );
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    assert!(heap.dbg_large_cache_used() > cached.0);
    let after = cache(heap);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    assert_eq!(cache(heap), after);
    let hit = heap.alloc(layout());
    assert_eq!(hit, ptr);
    assert_eq!(heap.dbg_large_cache_hits(), after.2 + 1);
    // SAFETY: new uniquely issued allocation after the rejection probe.
    unsafe { heap.dealloc(hit, layout()) };
}
