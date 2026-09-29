#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::HeapCore;

#[test]
fn fallback_bootstrap_rolls_back_route_oom_then_publishes_primordial() {
    RouteDirectory::fail_next_registration_for_test();
    assert!(HeapCore::dbg_with_fallback_for_test(|_| ()).is_none());

    let (base, owner) = HeapCore::dbg_with_fallback_for_test(|heap| {
        (heap.segment_bases().next().expect("primordial"), heap.id())
    })
    .expect("fallback retry");
    let pin = RouteDirectory::global()
        .lookup(base)
        .expect("fallback primordial route precedes use");
    assert_eq!(pin.kind(), RouteKind::Primordial);
    assert_eq!(pin.owner(), owner as usize);
}
