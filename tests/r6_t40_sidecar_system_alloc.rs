#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};

const SEGMENT: usize = 4 * 1024 * 1024;

#[test]
fn small_sidecar_uses_unzeroed_system_storage_then_initializes_all_state() {
    let before = SmallSidecar::system_totals_for_test();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 9, RouteKind::Small)
        .unwrap();
    let after = SmallSidecar::system_totals_for_test();
    let size = core::mem::size_of::<SmallSidecar>();
    assert_eq!(after.0 - before.0, size);
    assert_eq!(
        after.1 - before.1,
        0,
        "sidecar System bytes must not be zeroed"
    );
    assert_eq!(after.3 - before.3, 1);

    let sidecar = route.small_sidecar().unwrap();
    assert_eq!(sidecar.class_at_for_test(0), None);
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);
    assert!(route.prepare_small(0, 0));
    assert_eq!(sidecar.class_at_for_test(0), None);
    assert!(route.issue_small(0, 0));
    assert_eq!(sidecar.class_at_for_test(0), Some(0));
}
