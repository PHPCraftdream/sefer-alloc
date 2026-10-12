//! PRF03 retained-route activation, independent of performance measurements.
#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats"
))]
use core::alloc::Layout;
use sefer_alloc::registry::{bootstrap, segment_route::RouteDirectory, HeapRegistry};
use sefer_alloc::AllocCore;
use std::sync::Mutex;
static SERIAL: Mutex<()> = Mutex::new(());

#[test]
fn retained_same_payload_cycle_issues_no_new_incarnations() {
    let _serial = SERIAL.lock().unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("owner lease");
    let heap = lease.core();
    let layout = Layout::from_size_align(4 * 1024 * 1024, 16).unwrap();
    let ptr = heap.alloc(layout);
    assert!(!ptr.is_null());
    let live = RouteDirectory::global().live_route_census_for_test().2;
    // SAFETY: unique issued start with its exact layout.
    unsafe { heap.dealloc(ptr, layout) };
    let issued = RouteDirectory::global().incarnations_issued_for_test();
    let hits = heap.dbg_large_cache_hits();
    for _ in 0..4 {
        let hit = heap.alloc(layout);
        assert_eq!(hit, ptr);
        // SAFETY: each hit is a newly issued unique instance.
        unsafe { heap.dealloc(hit, layout) };
    }
    assert_eq!(heap.dbg_large_cache_hits(), hits + 4);
    assert_eq!(
        RouteDirectory::global().incarnations_issued_for_test(),
        issued,
        "same-payload hits must retain their route"
    );
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test().2,
        live
    );
    assert!(RouteDirectory::global().lookup(ptr).is_none());
}

fn publish_foreign(address: usize, layout: Layout) {
    std::thread::spawn(move || {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("foreign lease");
        let ptr = core::ptr::without_provenance_mut(address);
        // SAFETY: unique current allocation credit transferred to this joined
        // producer; routed dealloc uses only its numeric address, not its bytes.
        unsafe { lease.core().dealloc(ptr, layout) };
    })
    .join()
    .unwrap();
}

#[test]
fn valid_foreign_free_reuses_and_reclaims_exactly_once() {
    let _serial = SERIAL.lock().unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().unwrap();
    let heap = lease.core();
    let layout = Layout::from_size_align(4 * 1024 * 1024, 16).unwrap();
    let mut ptr = heap.alloc(layout);
    assert!(!ptr.is_null());
    for _ in 0..2 {
        let count = AllocCore::dbg_foreign_or_unroutable_frees();
        publish_foreign(ptr.addr(), layout);
        assert_eq!(AllocCore::dbg_foreign_or_unroutable_frees(), count);
        let used = heap.dbg_large_cache_used();
        assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
        assert!(heap.dbg_large_cache_used() > used);
        let cached = heap.dbg_large_cache_used();
        assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
        assert_eq!(heap.dbg_large_cache_used(), cached);
        let hits = heap.dbg_large_cache_hits();
        let hit = heap.alloc(layout);
        assert_eq!(hit, ptr);
        assert_eq!(heap.dbg_large_cache_hits(), hits + 1);
        ptr = hit;
    }
    // SAFETY: current unique instance, exact layout.
    unsafe { heap.dealloc(ptr, layout) };
}

#[test]
fn changed_payload_reregisters_and_routes_foreign_free() {
    let _serial = SERIAL.lock().unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().unwrap();
    let heap = lease.core();
    let old = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
    let new = Layout::from_size_align(old.size(), 65536).unwrap();
    let ptr = heap.alloc(old);
    assert!(!ptr.is_null());
    // SAFETY: exact current instance and layout.
    unsafe { heap.dealloc(ptr, old) };
    let issued = RouteDirectory::global().incarnations_issued_for_test();
    let hits = heap.dbg_large_cache_hits();
    let hit = heap.alloc(new);
    assert!(!hit.is_null());
    assert_ne!(hit.addr(), ptr.addr());
    assert_eq!(heap.dbg_large_cache_hits(), hits + 1);
    assert_eq!(
        RouteDirectory::global().incarnations_issued_for_test(),
        issued + 1
    );
    assert!(RouteDirectory::global().lookup(ptr).is_none());
    assert!(RouteDirectory::global().lookup(hit).is_some());
    let count = AllocCore::dbg_foreign_or_unroutable_frees();
    publish_foreign(hit.addr(), new);
    assert_eq!(AllocCore::dbg_foreign_or_unroutable_frees(), count);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
}

// The boundary observer is compiled only with debug assertions (never in perf builds).
#[cfg(debug_assertions)]
#[test]
fn eviction_boundary_observes_unlinked_large_route() {
    let _serial = SERIAL.lock().unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().unwrap();
    let heap = lease.core();
    let layout = Layout::from_size_align(4 * 1024 * 1024, 16).unwrap();
    let before = RouteDirectory::global().live_route_census_for_test().2;
    // The extension's default byte budget can bind before its 40-slot cap.
    // Use the real admission boundary, asserting every deposit before eviction.
    let max_slots = heap.dbg_large_cache_slot_sizes().len();
    #[cfg(feature = "large-cache-extended")]
    let max_slots = max_slots + heap.dbg_large_cache_extended_slot_sizes().len();
    let pointers: Vec<_> = (0..=max_slots).map(|_| heap.alloc(layout)).collect();
    assert!(pointers.iter().all(|ptr| !ptr.is_null()));
    let releases = AllocCore::dbg_cached_large_release_boundary().0;
    let mut evicted = false;
    for (index, &ptr) in pointers.iter().enumerate() {
        let used = heap.dbg_large_cache_used();
        // SAFETY: distinct unique live instance with its exact layout.
        unsafe { heap.dealloc(ptr, layout) };
        let boundary = AllocCore::dbg_cached_large_release_boundary();
        if !evicted && boundary.0 == releases {
            assert!(heap.dbg_large_cache_used() > used);
            assert_eq!(
                RouteDirectory::global().live_route_census_for_test().2,
                before + pointers.len()
            );
            continue;
        }
        if !evicted {
            assert!(index >= heap.dbg_large_cache_slot_sizes().len());
            let occupied = heap.dbg_large_cache_slot_sizes().iter().flatten().count();
            #[cfg(feature = "large-cache-extended")]
            let occupied = occupied
                + heap
                    .dbg_large_cache_extended_slot_sizes()
                    .iter()
                    .flatten()
                    .count();
            assert_eq!(occupied, index);
            assert!(
                occupied == heap.dbg_large_cache_total_slots()
                    || heap.dbg_large_cache_budget().is_some_and(|budget| {
                        let unit = heap
                            .dbg_large_cache_slot_sizes()
                            .into_iter()
                            .flatten()
                            .next()
                            .unwrap();
                        used + unit > budget
                    })
            );
            assert_eq!(boundary.0, releases + 1);
            assert_eq!(
                boundary.1,
                before + pointers.len() - 1,
                "victim absent JUST BEFORE OS release"
            );
            assert_eq!(
                RouteDirectory::global().live_route_census_for_test().2,
                before + pointers.len() - 1
            );
            evicted = true;
        }
    }
    assert!(
        evicted,
        "one more object than the actual slot cap must evict"
    );
}
