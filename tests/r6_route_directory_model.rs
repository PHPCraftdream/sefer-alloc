#![cfg(all(feature = "alloc-global", feature = "internals"))]

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RoutePin};

const SEGMENT: usize = 4 * 1024 * 1024;

#[test]
fn terminal_publication_signatures_consume_the_pin() {
    let _: fn(RoutePin, u32) -> bool = RoutePin::publish_small;
    let _: fn(RoutePin) -> bool = RoutePin::publish_large;
}

#[test]
fn negative_control_cached_cannot_publish_before_owner_reset() {
    let directory = RouteDirectory::new();
    // Routing-only key; no reservation byte is accessed in this model.
    let root = core::ptr::without_provenance_mut::<u8>(SEGMENT);
    let route = directory
        .register(root, 1, root, 1, RouteKind::Large)
        .unwrap();
    assert!(directory.lookup(root).unwrap().publish_large());
    let generation = route.claim_large_pending().unwrap();
    assert!(route.cache_large_consumed(generation));
    let next = route.begin_large_reuse().unwrap();
    assert!(!directory.lookup(root).unwrap().publish_large());
    // This model checks the phase gate; production owner reset is out of scope.
    assert!(route.finish_large_reuse_after_reset(next));
    assert!(directory.lookup(root).unwrap().publish_large());
    let second = route.claim_large_pending().unwrap();
    assert!(route.cache_large_consumed(second));
    assert!(route.release_cached_large(second));
}

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
