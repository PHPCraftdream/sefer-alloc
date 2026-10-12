#![cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]
use sefer_alloc::AllocCore;

// Exact field types of A 6904dced, recovered with git show (no A checkout).
#[allow(dead_code)]
struct CachedLargeA {
    reservation: *mut u8,
    reservation_len: usize,
    base: *mut u8,
    usable_size: usize,
    reserved_capacity: usize,
    seq: u64,
}

#[cfg(all(feature = "alloc-global", target_pointer_width = "64"))]
const _: () = {
    let (parked, cached, slot) = AllocCore::dbg_large_cache_layout();
    assert!(parked == 8);
    assert!(cached == 56);
    assert!(slot == 56);
};
#[cfg(all(not(feature = "alloc-global"), target_pointer_width = "64"))]
const _: () = {
    let (parked, cached, slot) = AllocCore::dbg_large_cache_layout();
    assert!(parked == 0);
    assert!(cached == 48);
    assert!(slot == 48);
};

#[cfg(feature = "production")]
#[test]
fn changed_payload_cycles_leave_one_route_then_cleanup() {
    use core::alloc::Layout;
    use sefer_alloc::registry::{bootstrap, segment_route::RouteDirectory, HeapRegistry};
    let _ = bootstrap::ensure();
    let before = RouteDirectory::global().live_route_census_for_test().2;
    let mut lease = HeapRegistry::dbg_claim_lease().expect("owner lease");
    let heap = lease.core();
    let mut prior = core::ptr::null_mut::<u8>();
    for i in 0..8 {
        let layout =
            Layout::from_size_align(4 * 1024 * 1024, if i % 2 == 0 { 8 } else { 65536 }).unwrap();
        let issued = RouteDirectory::global().incarnations_issued_for_test();
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        if i != 0 {
            assert_ne!(ptr, prior);
            assert!(RouteDirectory::global().lookup(prior).is_none());
        }
        assert_eq!(
            RouteDirectory::global().incarnations_issued_for_test(),
            issued + 1
        );
        assert_eq!(
            RouteDirectory::global().live_route_census_for_test().2,
            before + 1
        );
        // SAFETY: uniquely issued current start, exact layout; one deposit.
        unsafe { heap.dealloc(ptr, layout) };
        assert!(RouteDirectory::global().lookup(ptr).is_none());
        assert_eq!(
            RouteDirectory::global().live_route_census_for_test().2,
            before + 1
        );
        if i == 7 {
            let issued = RouteDirectory::global().incarnations_issued_for_test();
            let retained = heap.alloc(layout);
            assert_eq!(retained, ptr);
            assert_eq!(
                RouteDirectory::global().incarnations_issued_for_test(),
                issued
            );
            // SAFETY: uniquely issued same-payload hit, exact layout.
            unsafe { heap.dealloc(retained, layout) };
        }
        prior = ptr;
    }
    let failed_layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
    let issued = RouteDirectory::global().incarnations_issued_for_test();
    let hits = heap.dbg_large_cache_hits();
    RouteDirectory::fail_next_registration_for_test();
    // One-shot failure retires the cache candidate; fresh registration retries.
    let recovered = heap.alloc(failed_layout);
    assert!(!recovered.is_null());
    assert_eq!(heap.dbg_large_cache_hits(), hits);
    assert_eq!(
        RouteDirectory::global().incarnations_issued_for_test(),
        issued + 1
    );
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test().2,
        before + 1
    );
    assert!(RouteDirectory::global().lookup(recovered).is_some());
    // SAFETY: unique fresh fallback allocation with exact layout.
    unsafe { heap.dealloc(recovered, failed_layout) };
    drop(lease);
    let mut cursor = 0;
    assert!(HeapRegistry::maintenance_pass_for_test(&mut cursor, 4096) > 0);
    assert_eq!(
        RouteDirectory::global().live_route_census_for_test().2,
        before
    );
}

#[cfg(not(feature = "alloc-global"))]
#[test]
fn standalone_cache_reuses_then_drops() {
    use core::alloc::Layout;
    let mut core = AllocCore::new().expect("standalone core");
    let layout = Layout::from_size_align(4 * 1024 * 1024, 16).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    // SAFETY: unique live start with exact layout, deposited once.
    unsafe { core.dealloc(ptr, layout) };
    for _ in 0..3 {
        let hit = core.alloc(layout);
        assert_eq!(hit, ptr);
        // SAFETY: unique newly issued instance, deposited once.
        unsafe { core.dealloc(hit, layout) };
    }
    drop(core);
}

#[test]
fn actual_cache_layout_diagnostic() {
    if cfg!(target_pointer_width = "64") {
        assert_eq!(core::mem::size_of::<CachedLargeA>(), 48);
        assert_eq!(core::mem::size_of::<Option<CachedLargeA>>(), 56);
        let expected = if cfg!(feature = "alloc-global") {
            (8, 56, 56)
        } else {
            (0, 48, 48)
        };
        assert_eq!(AllocCore::dbg_large_cache_layout(), expected);
    }
    println!(
        "current={:?}; A CachedLarge={}, Option={}",
        AllocCore::dbg_large_cache_layout(),
        core::mem::size_of::<CachedLargeA>(),
        core::mem::size_of::<Option<CachedLargeA>>()
    );
}
