//! B4 state-byte oracle: raw metadata observation checked against residency and
//! live-accounting effects. Periodic duplicate frees test guard rejection while
//! the block is actually MAG (after the first free), not while user-live.

// `dbg_live_count_for` on `HeapCore` is gated `alloc-decommit` + `internals`
// (H2, task #572) and the live-accounting assertions below need it.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use core::alloc::Layout;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

const N: usize = 100_000;
const SMALL_N: usize = 512;
const STATE_LIVE: u8 = 0;
const STATE_MAG: u8 = 1;

fn l16() -> Layout {
    Layout::from_size_align(16, 8).unwrap()
}

#[test]
fn state_byte_matches_residency_full_coverage() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let mut p = heap.alloc(l16());
    assert!(!p.is_null());

    for i in 0..SMALL_N {
        let _live_before = heap.dbg_live_count_for(p);
        // SAFETY: p is currently live.
        unsafe { heap.dealloc(p, l16()) };
        let live_after_first = heap.dbg_live_count_for(p);
        let mag_before = heap.dbg_tcache_count(0);
        if i % 1000 == 0 {
            // SAFETY: the first deallocation placed p in MAG.
            unsafe { heap.dealloc(p, l16()) };
        }
        let free_ok = heap.dbg_live_count_for(p) == live_after_first
            && heap.dbg_tcache_count(0) == mag_before
            && heap.dbg_tcache_contains(0, p)
            && heap.dbg_state_byte_state_for(p) == Some(STATE_MAG);
        assert!(free_ok, "small[{i}]: MAG free guard altered state");
        p = heap.alloc(l16());
        assert!(!p.is_null());
        assert!(!heap.dbg_tcache_contains(0, p));
        assert_eq!(heap.dbg_state_byte_state_for(p), Some(STATE_LIVE));
    }

    let mut mismatches = 0u64;
    for i in 0..N {
        let _live_before = heap.dbg_live_count_for(p);
        // SAFETY: p is currently live.
        unsafe { heap.dealloc(p, l16()) };
        let live_after_first = heap.dbg_live_count_for(p);
        let mag_before = heap.dbg_tcache_count(0);
        if i % 1000 == 0 {
            // SAFETY: the first deallocation placed p in MAG.
            unsafe { heap.dealloc(p, l16()) };
        }
        let free_ok = heap.dbg_live_count_for(p) == live_after_first
            && heap.dbg_tcache_count(0) == mag_before
            && heap.dbg_tcache_contains(0, p)
            && heap.dbg_state_byte_state_for(p) == Some(STATE_MAG);
        p = heap.alloc(l16());
        assert!(!p.is_null());
        let alloc_ok =
            !heap.dbg_tcache_contains(0, p) && heap.dbg_state_byte_state_for(p) == Some(STATE_LIVE);
        if !(free_ok && alloc_ok) {
            mismatches += 1;
        }
    }
    let pct = (N as u64 - mismatches) as f64 / N as f64 * 100.0;
    println!("RESULT state_byte_oracle mismatches={mismatches}/{N} ({pct:.4}% matching)");
    assert_eq!(
        mismatches, 0,
        "state-byte oracle mismatches over {N} cycles"
    );
    // SAFETY: p is live at loop exit.
    unsafe { heap.dealloc(p, l16()) };
    drop(lease);
}
