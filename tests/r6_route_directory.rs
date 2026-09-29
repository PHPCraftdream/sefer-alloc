#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::sync::mpsc;
use std::thread;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteError, RouteKind};

const SEGMENT: usize = 4 * 1024 * 1024;

#[test]
fn duplicate_key_and_same_address_reuse_preserve_old_pin_identity() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let first = directory
        .register(root, SEGMENT, root, 17, RouteKind::Small)
        .unwrap();
    let first_id = first.incarnation();
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
    drop(second);
    assert!(directory.lookup(root).is_none());
}

#[test]
fn retirement_does_not_wait_for_a_paused_pin() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 1, RouteKind::Primordial)
        .unwrap();
    let (pinned_tx, pinned_rx) = mpsc::channel();
    let (resume_tx, resume_rx) = mpsc::channel();
    let addr = root.addr();
    thread::scope(|scope| {
        let directory_ref = &directory;
        scope.spawn(move || {
            let pin = directory_ref
                .lookup(core::ptr::without_provenance_mut(addr))
                .unwrap();
            pinned_tx.send(()).unwrap();
            resume_rx.recv().unwrap();
            assert_eq!(pin.owner(), 1);
        });
        pinned_rx.recv().unwrap();
        drop(route);
        assert!(directory.lookup(root).is_none());
        resume_tx.send(()).unwrap();
    });
}

#[test]
fn retired_pin_identity_survives_reservation_unmap() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 2, RouteKind::Small)
        .unwrap();
    let pin = directory.lookup(root).unwrap();
    drop(route);
    drop(reservation);
    assert_eq!(pin.owner(), 2);
    assert_eq!(pin.kind(), RouteKind::Small);
    drop(pin);
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
    drop(pin);
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
