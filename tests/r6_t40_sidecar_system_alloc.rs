#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};

const SEGMENT: usize = 4 * 1024 * 1024;
const BANK_BYTES: usize = 2 * (SEGMENT / 16 / 8);
const READY_BYTES: usize = (SEGMENT / 8192) / 8;
const _: () = assert!(core::mem::size_of::<SmallSidecar>() == BANK_BYTES + READY_BYTES);
const _: () = assert!(core::mem::align_of::<SmallSidecar>() == 64);

#[test]
fn small_sidecar_uses_one_unzeroed_system_backing_and_lazy_chunks() {
    let before = SmallSidecar::system_totals_for_test();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 9, RouteKind::Small)
        .unwrap();
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(after.0 - before.0, core::mem::size_of::<SmallSidecar>());
    assert_eq!(after.1 - before.1, 0, "System не получает zeroed-запрос");
    assert_eq!(after.3 - before.3, 1);
    let sidecar = route.small_sidecar().unwrap();
    assert_eq!(sidecar.initialized_chunks_for_test(), 0);
    route.issue_small_for_test(0);
    assert_eq!(sidecar.initialized_chunks_for_test(), 1);
    route.issue_small_for_test((SEGMENT - 16) as u32);
    assert_eq!(
        sidecar.initialized_chunks_for_test(),
        2,
        "промежуточные чанки не constructed"
    );
    route.issue_small_for_test(16);
    assert_eq!(
        sidecar.initialized_chunks_for_test(),
        2,
        "старый чанк не reconstructed"
    );
    let issued = SmallSidecar::system_totals_for_test();
    assert_eq!(issued.0, after.0, "issue не выделяет новый backing");
    assert_eq!(issued.3, after.3);
    let pin = directory.lookup(root).unwrap();
    // SAFETY: синтетический issued offset; metadata-фильтр обязан его отвергнуть.
    assert!(!unsafe { pin.publish_small(0) });
}
