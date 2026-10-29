#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{
    RouteDirectory, RouteKind, RouteRegistration, SmallSidecar,
};
use sefer_alloc::AllocCore;

#[test]
fn scalar_commit_publishes_only_starts_and_preserves_prepare_failure() {
    let layout = Layout::from_size_align(32, 16).unwrap();
    let mut core = AllocCore::dbg_new_routed_for_test().unwrap();
    let before = core.dbg_r11_issue_state_for_test(1).unwrap();
    let totals = SmallSidecar::system_totals_for_test();
    SmallSidecar::fail_prepare_after_for_test(1);
    assert!(core.dbg_r11_scalar_carve_for_test(1).is_none());
    assert_eq!(core.dbg_r11_issue_state_for_test(1), Some(before));
    assert_eq!(SmallSidecar::system_totals_for_test(), totals);

    let first = core.dbg_r11_scalar_carve_for_test(1).unwrap();
    let at = RouteRegistration::class_at_global_address_for_test;
    assert_eq!(at(first.addr()), Some(1));
    assert_eq!(at(first.addr() + 16), None);
    assert_eq!(at(first.addr() + 32), None);
    let prepared_totals = SmallSidecar::system_totals_for_test();
    let second = core.dbg_r11_scalar_carve_for_test(1).unwrap();
    assert_eq!(second.addr(), first.addr() + 32);
    assert_eq!(at(second.addr()), Some(1));
    assert_eq!(SmallSidecar::system_totals_for_test(), prepared_totals);

    // SAFETY: first — живой блок этого core с исходным layout.
    unsafe { core.dealloc(first, layout) };
    let recycled = core.alloc(layout);
    assert_eq!(recycled, first);
    assert_eq!(at(recycled.addr()), Some(1));
    assert_eq!(SmallSidecar::system_totals_for_test(), prepared_totals);
    // SAFETY: два различных живых блока освобождаются по одному разу.
    unsafe {
        core.dealloc(recycled, layout);
        core.dealloc(second, layout);
    }
    drop(core);

    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(4 * 1024 * 1024, 4 * 1024 * 1024).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, 4 * 1024 * 1024, root, 0, RouteKind::Small)
        .unwrap();
    let sidecar = route.small_sidecar().unwrap();
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);
    assert!(!route.issue_small(1, 1));
    assert!(!route.issue_small(0, u8::MAX));
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);
    assert!(route.issue_small(0, 1));
    assert_eq!(sidecar.mixed_leaves_for_test(), 1);
    assert_eq!(sidecar.class_at_for_test(0), Some(1));
    assert_eq!(sidecar.class_at_for_test(16), None);
    drop(route);
    drop(reservation);
}
