#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![allow(unused_unsafe)] // The RoutePin API is becoming unsafe in the parent branch.

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};

const SEGMENT: usize = 4 * 1024 * 1024;
// D1' phase-1a: published offsets must sit above the route kind's metadata
// end (the producer payload filter); base every issued/published offset here.
// Both offsets share pending word OFF/1024, so the 2 KiB window still models
// the same single-word cut.
const OFF: u32 = 384 * 1024;

#[test]
fn publication_after_word_cut_waits_for_next_bounded_scan() {
    let directory = RouteDirectory::new();
    let reservation = aligned_vmem::reserve_aligned(SEGMENT, SEGMENT).unwrap();
    let root = reservation.as_ptr();
    let route = directory
        .register(root, SEGMENT, root, 1, RouteKind::Primordial)
        .unwrap();
    let sidecar = route.small_sidecar().unwrap();
    route.small_sidecar().unwrap().issue_for_test(OFF);
    route.small_sidecar().unwrap().issue_for_test(OFF + 16);
    // SAFETY: the test owns this live aligned reservation and has issued its
    // disjoint 16-byte block at OFF exactly once; no other free occurs.
    assert!(unsafe {
        directory
            .lookup(root.with_addr(root.addr() + OFF as usize))
            .unwrap()
            .publish_small(OFF)
    });

    // D1' phase-1a: OFF lives in word OFF/1024; the window covers EXACTLY
    // that word (high_water = next word boundary), so the empty words below
    // it are skipped and the scan ends right after the modeled word.
    let rec_word = (OFF as usize) / 1024;
    let mut scan = sidecar.scan(OFF as usize + 1024).unwrap();
    let mut cut = None;
    for w in 0..=rec_word {
        let c = scan.next_cut().unwrap();
        if w == rec_word {
            cut = Some(c);
        } else {
            assert!(c.is_empty(), "words below the model word stay empty");
        }
    }
    let mut cut = cut.unwrap();
    // SAFETY: OFF + 16 is the other live, disjoint 16-byte suballocation;
    // this is its sole transfer and no old dealloc follows.
    assert!(unsafe {
        directory
            .lookup(root.with_addr(root.addr() + OFF as usize + 16))
            .unwrap()
            .publish_small(OFF + 16)
    });
    assert_eq!(cut.pop(), Some(OFF));
    assert_eq!(cut.pop(), None);
    assert!(scan.next_cut().is_none());
    drop(cut);
    drop(scan);

    let mut later = sidecar.scan(OFF as usize + 2048).unwrap();
    let mut later_records = Vec::new();
    while let Some(mut cut) = later.next_cut() {
        while let Some(offset) = cut.pop() {
            later_records.push(offset);
        }
    }
    assert_eq!(later_records, vec![OFF + 16]);
    let mut final_scan = sidecar.scan(OFF as usize + 2048).unwrap();
    while let Some(cut) = final_scan.next_cut() {
        assert!(cut.is_empty(), "fully drained");
    }
}
