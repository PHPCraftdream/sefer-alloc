#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    not(feature = "alloc-global")
))]

#[path = "../src/alloc_core/segment/segment_table/parked_route.rs"]
mod parked_route;

#[test]
fn standalone_parked_route_is_zero_sized_and_inert() {
    assert_eq!(core::mem::size_of::<parked_route::ParkedRoute>(), 0);
    let mut route = parked_route::ParkedRoute::none();
    route.cache_consumed();
    route.discard_if_payload_changed(core::ptr::null_mut());
    assert_eq!(route.begin_reuse(), None);
    route.release_before_unmap();
}
