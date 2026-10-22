//! B4-R6: a freshly registered route's sidecar backing reads as all zero —
//! the in-place initialize_at construction contract (pending bits and start
//! bits all start at the zero state; D1' phase-1b removed the class codes).
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::mem::size_of;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};

const SEGMENT: usize = 4 * 1024 * 1024;
const GRANULES: usize = 262_144;
const MIN_BLOCK: usize = 16;

#[test]
fn fresh_route_backing_reads_all_zero() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    // Poison a same-size System block and free it immediately before the
    // registration: the sidecar allocation is then likely served RECYCLED
    // (dirty) memory, so a skipped initialize_at field surfaces as observable
    // garbage instead of page-luck zeros (counterfactual sensitivity).
    let poison = vec![0xA5_u8; size_of::<SmallSidecar>()];
    drop(poison);
    // The registration must stay alive: its Drop unlinks the route.
    let _route = directory
        .register(root, SEGMENT, root, 9, RouteKind::Small)
        .unwrap();
    // Exhaustive: every granule's pending bit is clear AND its publication is
    // rejected (the zeroed start bank drops it before any RMW — the D1'
    // phase-1b "never issued ⇒ no publication" zero state). Sampled under
    // Miri.
    let pin = directory.lookup(root).unwrap();
    #[cfg(not(miri))]
    for granule in 0..GRANULES {
        let offset = (granule * MIN_BLOCK) as u32;
        let addr = root.with_addr(root.addr() + offset as usize);
        assert!(!pin.pending_for_test(addr), "pending bit {granule}");
        // SAFETY: a never-issued granule of this test's own reservation; the
        // zeroed start bank rejects the publication without mutations. The
        // borrowed form keeps the single lookup pin alive across the sweep.
        assert!(
            !unsafe { pin.publish_small_borrowed(offset) },
            "start bit {granule}"
        );
    }
    #[cfg(miri)]
    for granule in [0usize, 1, 63, 64, GRANULES / 2, GRANULES - 1] {
        let offset = (granule * MIN_BLOCK) as u32;
        let addr = root.with_addr(root.addr() + offset as usize);
        assert!(!pin.pending_for_test(addr), "pending bit {granule}");
        // SAFETY: a never-issued granule of this test's own reservation; the
        // zeroed start bank rejects the publication without mutations.
        assert!(
            !unsafe { pin.publish_small_borrowed(offset) },
            "start bit {granule}"
        );
    }
}
