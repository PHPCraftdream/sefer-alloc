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

    // D1' phase-1b: no class leaves and no fallible prepare exist — the
    // sidecar is exactly the two 32 KiB banks, and the owner issue is an
    // infallible start-bit set (observable only through the publication
    // gate: offset 0 is metadata, so the publish must still be rejected by
    // the payload filter regardless of the start bit).
    assert_eq!(size, 65_536);
    let sidecar = route.small_sidecar().unwrap();
    sidecar.issue_for_test(0);
    let pin = directory.lookup(root).unwrap();
    // SAFETY: the test issued this standalone granule; offset 0 is below the
    // route's payload start, so the payload filter rejects the publication.
    assert!(!unsafe { pin.publish_small(0) });
}
