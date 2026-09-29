#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};

const SEGMENT: usize = 4 * 1024 * 1024;

#[test]
fn all_owner_and_two_pin_release_orders_reclaim_slot() {
    for order in [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ] {
        let directory = RouteDirectory::new();
        let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
        let root = reservation.as_ptr();
        let mut owner = Some(
            directory
                .register(root, SEGMENT, root, 8, RouteKind::Small)
                .unwrap(),
        );
        let mut pins = [directory.lookup(root), directory.lookup(root)];
        for action in order {
            match action {
                0 => drop(owner.take()),
                1 | 2 => drop(pins[action - 1].take()),
                _ => unreachable!(),
            }
            assert_eq!(directory.lookup(root).is_some(), owner.is_some());
            for pin in pins.iter().flatten() {
                assert_eq!(pin.owner(), 8);
            }
        }
        assert!(directory.lookup(root).is_none());
        let reused = directory
            .register(root, SEGMENT, root, 9, RouteKind::Small)
            .unwrap();
        assert_eq!(directory.lookup(root).unwrap().owner(), 9);
        drop(reused);
    }
}
