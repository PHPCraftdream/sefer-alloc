#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![allow(unused_unsafe)] // The RoutePin API is becoming unsafe in the parent branch.

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, RouteRecord};

const SEGMENT: usize = 4 * 1024 * 1024;

#[test]
fn publication_after_word_cut_waits_for_next_bounded_scan() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 1, RouteKind::Primordial)
        .unwrap();
    let sidecar = route.small_sidecar().unwrap();
    assert!(route.issue_small(0, 0));
    assert!(route.issue_small(16, 0));
    // SAFETY: the test owns this live aligned reservation and has issued its
    // disjoint 16-byte block at offset 0 exactly once; no other free occurs.
    assert!(unsafe { directory.lookup(root).unwrap().publish_small(0) });

    let mut scan = sidecar.scan(32).unwrap();
    let mut cut = scan.next_cut().unwrap();
    // SAFETY: offset 16 is the other live, disjoint 16-byte suballocation;
    // this is its sole transfer and no old dealloc follows.
    assert!(unsafe { directory.lookup(root).unwrap().publish_small(16) });
    assert_eq!(
        cut.pop(),
        Some(RouteRecord {
            offset: 0,
            class: 0
        })
    );
    assert_eq!(cut.pop(), None);
    assert!(scan.next_cut().is_none());
    drop(cut);
    drop(scan);

    let mut later = sidecar.scan(32).unwrap();
    let mut cut = later.next_cut().unwrap();
    assert_eq!(
        cut.pop(),
        Some(RouteRecord {
            offset: 16,
            class: 0
        })
    );
    assert_eq!(cut.pop(), None);
    assert!(later.next_cut().is_none());
    assert!(sidecar.scan(32).unwrap().next_cut().unwrap().is_empty());
}
