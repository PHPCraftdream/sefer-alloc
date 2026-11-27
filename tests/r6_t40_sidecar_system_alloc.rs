//! Task #223 rewrite of the historical old-path oracle
//! (`small_sidecar_uses_unzeroed_system_storage_then_initializes_all_state`):
//! a Small route's sidecar NO LONGER comes from a (calloc-shaped) System
//! allocation — the default backing is an OS-zeroed VM span, the System
//! byte totals must not move at registration, and the sidecar still reads
//! as the documented all-zero initial state and issues classes end to end.
#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};

const SEGMENT: usize = 4 * 1024 * 1024;

#[test]
fn small_sidecar_uses_os_zeroed_span_then_reads_and_issues_all_zero_state() {
    let system_before = SmallSidecar::system_totals_for_test();
    let counts_before = SmallSidecar::sidecar_backing_counts_for_test();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = unsafe { directory.register(root, SEGMENT, root, 9, RouteKind::Small) }.unwrap();
    let counts_after = SmallSidecar::sidecar_backing_counts_for_test();
    assert_eq!(
        counts_after.0 - counts_before.0,
        1,
        "sidecar backing is the OS-zeroed span"
    );
    assert_eq!(counts_after.2, counts_before.2, "heap fallback not taken");
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(
        after, system_before,
        "no System request at all for the sidecar (old path: +1 full-size alloc)"
    );

    let sidecar = route.small_sidecar().unwrap();
    assert_eq!(sidecar.class_at_for_test(0), None);
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);
    assert!(route.prepare_small(0, 0));
    assert_eq!(sidecar.class_at_for_test(0), None);
    assert!(route.issue_small(0, 0));
    assert_eq!(sidecar.class_at_for_test(0), Some(0));
    drop(route);
    let counts_released = SmallSidecar::sidecar_backing_counts_for_test();
    assert_eq!(counts_released.1 - counts_after.1, 1, "span released once");
    assert_eq!(SmallSidecar::system_totals_for_test(), system_before);
}
