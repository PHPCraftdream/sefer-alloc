#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![allow(unused_unsafe)] // The RoutePin API is becoming unsafe in the parent branch.

use sefer_alloc::{
    registry::segment_route::{RouteDirectory, RouteKind, RouteRecord},
    SegmentLayout,
};

const SEGMENT: usize = 4 * 1024 * 1024;
const START_WORD: usize = SegmentLayout::PRIMORDIAL_PAYLOAD_START_WORD;
const PAYLOAD_START: usize = START_WORD * 64 * SegmentLayout::MIN_BLOCK;

#[test]
fn publication_after_word_cut_waits_for_next_bounded_scan() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 1, RouteKind::Primordial)
        .unwrap();
    let sidecar = route.small_sidecar().unwrap();
    assert!(route.issue_small(PAYLOAD_START as u32, 0));
    assert!(route.issue_small((PAYLOAD_START + 16) as u32, 0));
    // SAFETY: the test owns this live aligned reservation and has issued its
    // disjoint payload blocks exactly once; no other free occurs.
    assert!(unsafe {
        directory
            .lookup(root)
            .unwrap()
            .publish_small(PAYLOAD_START as u32)
    });

    let high_water_bytes = PAYLOAD_START + 32;
    let mut scan = sidecar.scan_from(high_water_bytes, START_WORD).unwrap();
    let mut cut = scan.next_cut().unwrap();
    // SAFETY: the second live disjoint payload block is transferred once.
    assert!(unsafe {
        directory
            .lookup(root)
            .unwrap()
            .publish_small((PAYLOAD_START + 16) as u32)
    });
    assert_eq!(
        cut.pop(),
        Some(RouteRecord {
            offset: PAYLOAD_START as u32,
            class: 0
        })
    );
    assert_eq!(cut.pop(), None);
    assert!(scan.next_cut().is_none());
    drop(cut);
    drop(scan);

    let mut later = sidecar.scan_from(high_water_bytes, START_WORD).unwrap();
    let mut cut = later.next_cut().unwrap();
    assert_eq!(
        cut.pop(),
        Some(RouteRecord {
            offset: (PAYLOAD_START + 16) as u32,
            class: 0
        })
    );
    assert_eq!(cut.pop(), None);
    assert!(later.next_cut().is_none());
    assert!(sidecar
        .scan_from(high_water_bytes, START_WORD)
        .unwrap()
        .next_cut()
        .unwrap()
        .is_empty());
}
