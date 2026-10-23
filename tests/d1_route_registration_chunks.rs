#![cfg(feature = "internals")]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};
use sefer_alloc::SegmentLayout;
use std::alloc::{GlobalAlloc, System};

#[test]
fn registration_constructs_only_touched_chunks_and_drop_unlinks_route() {
    let directory = RouteDirectory::with_initial_incarnation_for_test(0);
    let layout = Layout::from_size_align(SegmentLayout::SEGMENT, SegmentLayout::SEGMENT).unwrap();
    // SAFETY: the valid nonzero aligned layout is retained until after route
    // unlink; the route path stores only its numeric key and never dereferences it.
    let root = unsafe { System.alloc(layout) };
    assert!(!root.is_null());
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    let sidecar_totals_before = SmallSidecar::system_totals_for_test();

    let registration = directory
        .register(root, layout.size(), root, 1, RouteKind::Small)
        .expect("small route registration failed");
    let sidecar = registration
        .small_sidecar()
        .expect("small route omitted its sidecar");
    assert_eq!(sidecar.initialized_chunks_for_test(), 0);

    registration.issue_small_for_test(0);
    assert_eq!(sidecar.initialized_chunks_for_test(), 1);
    registration.issue_small_for_test((SegmentLayout::SEGMENT - SegmentLayout::MIN_BLOCK) as u32);
    assert_eq!(sidecar.initialized_chunks_for_test(), 2);
    let sidecar_bytes = SmallSidecar::system_totals_for_test().0 - sidecar_totals_before.0;
    assert_eq!(sidecar_bytes, Layout::new::<SmallSidecar>().size());
    let pin = directory
        .lookup(root)
        .expect("registered route should be pinnable");

    drop(registration);
    assert_eq!(directory.live_route_census_for_test(), (0, 0, 0));
    assert!(
        !pin.linked(),
        "owner unlink should retire lookup visibility"
    );
    assert!(
        !pin.pending_for_test(root),
        "the retained pin still safely reads its sidecar"
    );
    let after_owner_unlink = SmallSidecar::system_totals_for_test();
    assert_eq!(
        after_owner_unlink.2, sidecar_totals_before.2,
        "pin must retain the sidecar allocation"
    );

    drop(pin);
    let after_last_pin = SmallSidecar::system_totals_for_test();
    assert_eq!(after_last_pin.2 - sidecar_totals_before.2, sidecar_bytes);
    assert_eq!(after_last_pin.4 - sidecar_totals_before.4, 1);
    // SAFETY: route unlinked and its owner dropped above; this allocation is
    // still owned by this test and is released with its original layout.
    unsafe { System.dealloc(root, layout) };
}
