#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::sync::mpsc;
use std::thread;

use sefer_alloc::registry::segment_route::{
    RouteDirectory, RouteError, RouteKind, RouteRecord, RouteRegistration,
};

const SEGMENT: usize = 4 * 1024 * 1024;

fn cut_one(route: &RouteRegistration<'_>) -> RouteRecord {
    let mut scan = route.small_sidecar().unwrap().scan(16).unwrap();
    let mut cut = scan.next_cut().unwrap();
    let record = cut.pop().unwrap();
    assert!(cut.pop().is_none());
    assert!(scan.next_cut().is_none());
    record
}

#[test]
fn duplicate_key_and_same_address_reuse_preserve_old_pin_identity() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let first = directory
        .register(root, SEGMENT, root, 17, RouteKind::Small)
        .unwrap();
    let first_id = first.incarnation();
    let old_sidecar_addr = core::ptr::from_ref(first.small_sidecar().unwrap()).addr();
    assert_eq!(first.root(), root);
    let old = directory.lookup(root).unwrap();
    assert_eq!(old.owner(), 17);
    assert!(matches!(
        directory.register(root, SEGMENT, root, 18, RouteKind::Small),
        Err(RouteError::Duplicate)
    ));

    drop(first);
    assert!(directory.lookup(root).is_none());
    let second = directory
        .register(root, SEGMENT, root, 19, RouteKind::Small)
        .unwrap();
    let current = directory.lookup(root).unwrap();
    assert!(second.incarnation() > first_id);
    assert_eq!(current.owner(), 19);
    assert_eq!(old.owner(), 17);
    assert_eq!(old.incarnation(), first_id);
    assert_ne!(
        old_sidecar_addr,
        core::ptr::from_ref(second.small_sidecar().unwrap()).addr()
    );
    drop(second);
    assert!(directory.lookup(root).is_none());
}

#[test]
fn paused_published_pin_survives_removal_and_same_address_registration() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 1, RouteKind::Primordial)
        .unwrap();
    assert!(route.issue_small(0, 0));
    let old_pin = directory.lookup(root).unwrap();
    let old_sidecar_addr = core::ptr::from_ref(route.small_sidecar().unwrap()).addr();
    let old_incarnation = route.incarnation();
    let (published_tx, published_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let addr = root.addr();
    thread::scope(|scope| {
        let directory_ref = &directory;
        scope.spawn(move || {
            let pin = directory_ref
                .lookup(core::ptr::without_provenance_mut(addr))
                .unwrap();
            // SAFETY: the model issued offset zero once and transfers it here.
            assert!(unsafe { pin.publish_small(0) });
            published_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
        });
        published_rx.recv().unwrap();
        assert_eq!(
            cut_one(&route),
            RouteRecord {
                offset: 0,
                class: 0
            }
        );
        drop(route);
        let replacement = directory
            .register(root, SEGMENT, root, 2, RouteKind::Primordial)
            .unwrap();
        assert!(replacement.incarnation() > old_incarnation);
        assert_ne!(
            core::ptr::from_ref(replacement.small_sidecar().unwrap()).addr(),
            old_sidecar_addr
        );
        assert_eq!(directory.lookup(root).unwrap().owner(), 2);
        drop(replacement);
        drop(reservation);
        drop(old_pin);
        resume_tx.send(()).unwrap();
    });
}

#[test]
fn published_pin_cleanup_survives_reservation_unmap() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 2, RouteKind::Small)
        .unwrap();
    assert!(route.issue_small(0, 0));
    let pin = directory.lookup(root).unwrap();
    // SAFETY: the owned model block at offset zero is issued and transferred once.
    assert!(unsafe { pin.publish_small(0) });
    assert_eq!(
        cut_one(&route),
        RouteRecord {
            offset: 0,
            class: 0
        }
    );
    drop(route);
    drop(reservation);
    // Consuming publication already released the producer pin.
}

#[test]
fn large_route_key_and_invalid_span() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(2 * SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let later = root.with_addr(root.addr() + SEGMENT + 16);
    let route = directory
        .register(root, 2 * SEGMENT, later, 3, RouteKind::Large)
        .unwrap();
    assert!(directory.lookup(root).is_none());
    let pin = directory
        .lookup(core::ptr::without_provenance_mut(later.addr()))
        .unwrap();
    assert_eq!(pin.kind(), RouteKind::Large);
    assert_eq!(route.large_state().unwrap().generation(), 1);
    // SAFETY: this standalone route's live instance is owned by this test.
    assert!(unsafe { pin.publish_large() });
    let first_generation = route.claim_large_pending().unwrap();
    assert_eq!(first_generation, 1);
    assert!(route.cache_large_consumed(first_generation));
    let next_generation = route.begin_large_reuse().unwrap();
    assert_eq!(next_generation, 2);
    assert!(route.claim_large_pending().is_none());
    assert!(!route.finish_large_reuse_after_reset(first_generation));
    // Actual owner layout/table reset belongs to the later integration.
    assert!(route.finish_large_reuse_after_reset(next_generation));
    assert_eq!(route.large_state().unwrap().generation(), 2);
    let next = directory.lookup(later).unwrap();
    // SAFETY: reset completed and the test owns the newly issued instance.
    assert!(unsafe { next.publish_large() });
    let second_generation = route.claim_large_pending().unwrap();
    assert_eq!(second_generation, 2);
    assert!(route.cache_large_consumed(second_generation));
    assert!(route.release_cached_large(second_generation));
    drop(route);
    assert!(matches!(
        directory.register(root, 0, root, 3, RouteKind::Small),
        Err(RouteError::InvalidSpan)
    ));
}

#[test]
fn incarnation_exhaustion_never_wraps() {
    let directory = RouteDirectory::with_initial_incarnation_for_test(u64::MAX - 1);
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let last = directory
        .register(root, SEGMENT, root, 4, RouteKind::Small)
        .unwrap();
    assert_eq!(last.incarnation(), u64::MAX);
    drop(last);
    assert!(matches!(
        directory.register(root, SEGMENT, root, 4, RouteKind::Small),
        Err(RouteError::IncarnationExhausted)
    ));
}

#[test]
fn owner_registration_can_move_between_lease_threads() {
    fn assert_send<T: Send>() {}
    assert_send::<sefer_alloc::registry::segment_route::RouteRegistration<'static>>();
}

#[test]
fn more_than_4096_active_routes_and_same_shard_collision() {
    let directory = RouteDirectory::new();
    let mut routes = Vec::new();
    // Routing-only synthetic addresses: neither root nor payload is ever
    // dereferenced. This exercises directory cardinality, not OS reservation.
    for i in 1..=5000_usize {
        let key = i * SEGMENT;
        let p = core::ptr::without_provenance_mut::<u8>(key);
        routes.push(directory.register(p, 1, p, i, RouteKind::Large).unwrap());
    }
    assert_eq!(
        directory
            .lookup(core::ptr::without_provenance_mut(SEGMENT))
            .unwrap()
            .owner(),
        1
    );
    assert_eq!(
        directory
            .lookup(core::ptr::without_provenance_mut(65 * SEGMENT))
            .unwrap()
            .owner(),
        65
    );
    assert!(directory.retained_pointer_capacity_for_test() >= 5000);
    drop(routes);
    assert!(directory
        .lookup(core::ptr::without_provenance_mut(65 * SEGMENT))
        .is_none());
}

#[test]
fn repeated_churn_reuses_pointer_array_capacity() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    for _ in 0..512 {
        let route = directory
            .register(root, SEGMENT, root, 7, RouteKind::Large)
            .unwrap();
        let pin = directory.lookup(root).unwrap();
        drop(route);
        drop(pin);
    }
    // Block index (8 cells) plus one retained spare block (64 cells).
    assert_eq!(directory.retained_pointer_capacity_for_test(), 8 + 64);
}

#[test]
fn sorted_shard_keeps_neighbors_after_middle_unlink() {
    let directory = RouteDirectory::new();
    // Routing-only numeric keys; no synthetic reservation is dereferenced.
    let keys = [SEGMENT, 65 * SEGMENT, 129 * SEGMENT, 193 * SEGMENT];
    let mut routes = keys.map(|key| {
        let p = core::ptr::without_provenance_mut::<u8>(key);
        Some(directory.register(p, 1, p, key, RouteKind::Large).unwrap())
    });
    drop(routes[1].take());
    for (index, key) in keys.into_iter().enumerate() {
        let found = directory.lookup(core::ptr::without_provenance_mut(key));
        if index == 1 {
            assert!(found.is_none());
        } else {
            assert_eq!(found.unwrap().owner(), key);
        }
    }
    let middle = core::ptr::without_provenance_mut::<u8>(65 * SEGMENT);
    routes[1] = Some(
        directory
            .register(middle, 1, middle, 65, RouteKind::Large)
            .unwrap(),
    );
    assert_eq!(directory.lookup(middle).unwrap().owner(), 65);
}
