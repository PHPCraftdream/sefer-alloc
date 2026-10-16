//! R6-T12: the owner-side sidecar drain retires a scan's accepted credits in
//! ONE `sub_live(k)` instead of one `dec_live` per record. The final
//! `live_count` must equal what the per-record form produced — exactly the
//! number of accepted records, never more, never less.
//!
//! This equivalence oracle passes with the old sequential implementation.
//! An extra or missing decrement makes the first post-drain live-count
//! assertion fail. The ingress cuts are consumed by that scan, so a second
//! pass cannot detect a missing batch by replaying the same records.
//!
//! Rejection of synthetic magazine-resident records is covered by the
//! existing `r6_terminal_owner_drain` synthetic seam.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;

use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SegmentLayout;

// Tests share the process-wide route directory and allocator; a block freed
// by one test can be reissued to another at the same address. Serialize them
// (same discipline as tests/r6_terminal_owner_drain.rs).
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn serialize() -> std::sync::MutexGuard<'static, ()> {
    SERIAL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn publish_small(ptr: *mut u8) {
    let pin = RouteDirectory::global().lookup(ptr).expect("issued route");
    assert!(matches!(
        pin.kind(),
        RouteKind::Small | RouteKind::Primordial
    ));
    let offset = (ptr.addr() & (SegmentLayout::SEGMENT - 1)) as u32;
    // SAFETY: ptr is one live allocation issued by this heap. This call is
    // its only ownership transfer; no old dealloc runs for ptr afterward.
    assert!(unsafe { pin.publish_small(offset) });
}

#[test]
fn one_scan_retires_exactly_the_accepted_credits() {
    let _guard = serialize();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(16, 16).unwrap();
    let anchor = heap.alloc(layout);
    assert!(!anchor.is_null());
    let sizes = [16usize, 32, 48, 64, 96, 128, 256, 512, 1024, 2048];
    let mut ptrs = Vec::new();
    for &size in &sizes {
        let ptr = heap.alloc(Layout::from_size_align(size, 16).unwrap());
        assert!(!ptr.is_null());
        ptrs.push(ptr);
    }
    // live_count is per segment and the allocations may span more than one,
    // so group the records by segment base first.
    let mut groups: Vec<(usize, u32, Vec<*mut u8>)> = Vec::new(); // (base, before, ptrs)
    for ptr in ptrs {
        let base = ptr.addr() & !(SegmentLayout::SEGMENT - 1);
        match groups.iter_mut().find(|(b, _, _)| *b == base) {
            Some((_, _, members)) => members.push(ptr),
            None => groups.push((base, 0, vec![ptr])),
        }
    }
    for (_, before, members) in &mut groups {
        *before = heap.dbg_live_count_for(members[0]).unwrap();
    }
    for (_, _, members) in &groups {
        for ptr in members {
            publish_small(*ptr);
        }
    }
    // One strict-trim pass consumes every pending publication in one scan.
    assert_eq!(heap.dbg_drain_sidecar_ingress(), sizes.len());
    for (_, before, members) in &groups {
        assert_eq!(
            heap.dbg_live_count_for(members[0]),
            Some(before - members.len() as u32),
            "live_count after one scan must drop by exactly the accepted records"
        );
        for ptr in members {
            assert!(heap.dbg_is_free_for(*ptr));
        }
    }
    // A second pass is a no-op: no credit left over, none double-counted.
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    for (_, before, members) in &groups {
        assert_eq!(
            heap.dbg_live_count_for(members[0]),
            Some(before - members.len() as u32)
        );
    }
    // Retire the held anchor only after all live-count assertions, so its
    // segment remains live throughout the observations.
    publish_small(anchor);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    // SAFETY: every issued allocation was retired by the owner-side drain.
    drop(lease);
}
