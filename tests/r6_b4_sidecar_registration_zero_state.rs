//! B4-R6: a freshly registered route's sidecar backing reads as all zero —
//! the System.alloc_zeroed construction contract (pending bits, presence
//! bits, and class codes all start at the zero state).
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};

const SEGMENT: usize = 4 * 1024 * 1024;
const GRANULES: usize = 262_144;
const MIN_BLOCK: usize = 16;

#[test]
fn fresh_route_backing_reads_all_zero() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 9, RouteKind::Small)
        .unwrap();
    let sidecar = route.small_sidecar().unwrap();
    assert_eq!(sidecar.mixed_leaves_for_test(), 0);
    let pin = directory.lookup(root).unwrap();
    // Exhaustive: every granule's pending bit is clear and its class code is
    // the "never issued" encoding (None). Sampled under Miri.
    #[cfg(not(miri))]
    for granule in 0..GRANULES {
        let offset = (granule * MIN_BLOCK) as u32;
        let addr = root.with_addr(root.addr() + offset as usize);
        assert!(!pin.pending_for_test(addr), "pending bit {granule}");
        assert_eq!(sidecar.class_at_for_test(offset), None, "class {granule}");
    }
    #[cfg(miri)]
    for granule in [0usize, 1, 63, 64, GRANULES / 2, GRANULES - 1] {
        let offset = (granule * MIN_BLOCK) as u32;
        let addr = root.with_addr(root.addr() + offset as usize);
        assert!(!pin.pending_for_test(addr), "pending bit {granule}");
        assert_eq!(sidecar.class_at_for_test(offset), None, "class {granule}");
    }
}
