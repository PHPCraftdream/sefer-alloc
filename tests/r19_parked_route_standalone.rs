#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

#[cfg(not(feature = "alloc-global"))]
#[path = "../src/alloc_core/segment/segment_table/parked_route.rs"]
mod parked_route;

#[cfg(not(feature = "alloc-global"))]
#[test]
fn standalone_parked_route_is_zero_sized_and_inert() {
    assert_eq!(core::mem::size_of::<parked_route::ParkedRoute>(), 0);
    let mut route = parked_route::ParkedRoute::none();
    route.cache_consumed();
    route.discard_if_identity_changed(core::ptr::null_mut(), core::ptr::null_mut());
    assert!(!route.is_present());
    assert_eq!(route.begin_reuse(), None);
    route.release_before_unmap();
}

#[test]
fn standalone_core_cache_reuses_without_directory_owner() {
    use core::alloc::Layout;
    use sefer_alloc::AllocCore;
    let mut core = AllocCore::new().expect("standalone core");
    let layout = Layout::from_size_align(4 * 1024 * 1024, 16).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    // SAFETY: unique live allocation, exact layout, one deposit per issue.
    unsafe { core.dealloc(ptr, layout) };
    for _ in 0..3 {
        let hit = core.alloc(layout);
        assert_eq!(hit, ptr);
        // SAFETY: newly issued unique cache hit with exact layout.
        unsafe { core.dealloc(hit, layout) };
    }
    drop(core);
}
