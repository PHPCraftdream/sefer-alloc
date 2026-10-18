//! B4-R6: ClassLeaves::Drop frees exactly the leaves whose presence bits the
//! owner's promotion set — verified through the quiescent System-totals
//! oracle (requested alloc/zeroed/free bytes and call counts).
#![cfg(all(
    feature = "alloc-global",
    feature = "internals",
    feature = "bench-internals"
))]

use core::mem::size_of;
use std::sync::Mutex;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};

const SEGMENT: usize = 4 * 1024 * 1024;
const LEAF: usize = 4096;

// The System totals are process-global; serialize the test binary.
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn drop_frees_exactly_the_promoted_leaves() {
    let _guard = TEST_LOCK.lock().unwrap();
    let before = SmallSidecar::system_totals_for_test();
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 21, RouteKind::Small)
        .unwrap();
    let sidecar = route.small_sidecar().unwrap();
    // Leaves spanning three presence words (0/1/63 -> word 0, 64 -> word 1,
    // 1023 -> word 15 bit 63).
    let promoted = [0usize, 1, 63, 64, 1023];
    for &leaf in &promoted {
        assert!(route.prepare_small((leaf * LEAF) as u32, 1), "leaf {leaf}");
    }
    assert_eq!(sidecar.mixed_leaves_for_test(), promoted.len());
    let size = size_of::<SmallSidecar>();
    let leaf_bytes = promoted.len() * 256;
    let live = SmallSidecar::system_totals_for_test();
    assert_eq!(live.0 - before.0, size + leaf_bytes);
    // Only the sidecar backing is zeroed; the leaf spills are unzeroed.
    assert_eq!(live.1 - before.1, size);
    assert_eq!(live.2 - before.2, 0);
    assert_eq!(live.3 - before.3, 1 + promoted.len());
    assert_eq!(live.4 - before.4, 0);

    // Unlink the route while a pin still holds the sidecar: nothing freed.
    let pin = directory.lookup(root).unwrap();
    drop(route);
    let held = SmallSidecar::system_totals_for_test();
    assert_eq!(held.2 - before.2, 0);

    // Last pin dropped: the Drop frees exactly backing + promoted leaves.
    drop(pin);
    let after = SmallSidecar::system_totals_for_test();
    assert_eq!(after.2 - before.2, size + leaf_bytes);
    assert_eq!(after.3 - before.3, 1 + promoted.len());
    assert_eq!(after.4 - before.4, 1 + promoted.len());
    assert_eq!(after.0 - before.0, after.2 - before.2);
    assert_eq!(after.1 - before.1, size);
    drop(reservation);
}
