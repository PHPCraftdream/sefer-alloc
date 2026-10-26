//! R16 item84 activation receipt only, not proof of a mask optimization.
//! Hand-derived policy: capacity 16, half-flush 8; frees 17 and 25 overflow.
//! A no-op flush-all mutant must fail the final count assertion (not run here).
//! B4 depot (DP-3): a full magazine is parked whole while the depot has room,
//! so each round first fills the 16 B class's depot; the policy below is the
//! one that applies once the depot is full.
#![cfg(all(feature = "alloc-global", feature = "fastbin", feature = "internals"))]

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use std::alloc::Layout;

#[test]
fn overflow_boundaries_and_flush_all_activation() {
    let _ = bootstrap::ensure();
    for (frees, expected_events, expected_final) in [(16, 0, 16), (17, 1, 9), (32, 2, 16)] {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
        let heap = lease.core();
        let layout = Layout::from_size_align(16, 8).unwrap();
        let class = heap.dbg_class_for(layout).expect("16 B class");
        let mut ptrs = [core::ptr::null_mut(); 64];
        for slot in &mut ptrs {
            *slot = heap.alloc(layout);
            assert!(!slot.is_null());
        }
        // Fill the depot from extra LIVE blocks, then pop the magazine empty
        // (magazine hits only, so the depot stays full).
        let mut depot_held = Vec::new();
        for _ in 0..16 * 9 {
            let p = heap.alloc(layout);
            assert!(!p.is_null());
            depot_held.push(p);
        }
        let mut freed = 0;
        while heap.dbg_depot_len(class) < 8 {
            // SAFETY: live helper block of this lease, freed exactly once.
            unsafe { heap.dealloc(depot_held[freed], layout) };
            freed += 1;
        }
        let mut depot_held = depot_held.split_off(freed);
        while heap.dbg_tcache_count(class) > 0 {
            let p = heap.alloc(layout);
            assert!(!p.is_null());
            depot_held.push(p);
        }
        assert_eq!(heap.dbg_tcache_count(class), 0);
        let mut events = 0;
        for (i, &ptr) in ptrs[..frees].iter().enumerate() {
            let before = heap.dbg_tcache_count(class);
            // SAFETY: ptr is a non-null, live allocation of this exclusively
            // leased heap with exactly this layout; this is its only free.
            unsafe { heap.dealloc(ptr, layout) };
            let after = heap.dbg_tcache_count(class);
            // Independent boundary specification, not read from SUT constants.
            let expected = if i < 16 { i + 1 } else { 9 + (i - 16) % 8 };
            assert_eq!(usize::from(after), expected, "free {}", i + 1);
            if before == 16 && after == 9 {
                events += 1;
            }
        }
        assert_eq!(events, expected_events);
        assert_eq!(heap.dbg_tcache_count(class), expected_final);
        heap.dbg_flush_all();
        assert_eq!(heap.dbg_tcache_count(class), 0);
        println!("activation frees={frees} overflow_events={events} before_flush={expected_final} after_flush=0");
        for &ptr in &ptrs[frees..] {
            // SAFETY: remaining blocks are still live, owned by this lease,
            // allocated with this layout, and each is freed exactly once.
            unsafe { heap.dealloc(ptr, layout) };
        }
        for &ptr in &depot_held {
            // SAFETY: live helper block of this lease, freed exactly once.
            unsafe { heap.dealloc(ptr, layout) };
        }
        heap.dbg_flush_all();
    }
}
