//! NL-3: remote frees of the hot small classes reach their owner and are reused.
//!
//! `g1_ingress_remote_free_reuse` pins this for the 1 KiB class only. The classes
//! that carry the magazine (16, 64, 256 B) are the ones whose per-block state a
//! trusting (no misuse checks) mode would stop maintaining: a block that arrives
//! at the owner through the cross-thread path with a stale state byte must still
//! be returned to a bin, not refused. This test is the behaviour oracle for that
//! set, in every build: the owner grows its heap over several segments, another
//! thread frees every block, and the owner allocates the same amount again; a
//! refused or lost remote free makes the owner carve new segments, so the live
//! segment count would grow.
//!
//! Block addresses are not the oracle (a segment a full drain empties may be
//! pooled or released and re-reserved); the live-segment count is. The counter
//! is process-wide, so all round trips run in one test, one after the other.
//! Excluded under `numa-aware` for the reason given in the 1 KiB test.
#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    not(feature = "numa-aware")
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;
use std::thread;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const SEGMENT: usize = 4 * 1024 * 1024;

fn live_segments() -> i64 {
    let s = GLOBAL.stats();
    s.segments_reserved_total as i64 - s.segments_released_total as i64
}

/// Grow over `segments` segments of `block`-byte blocks, free everything from
/// another thread, allocate the same amount again; return `(peak, after)`.
fn round_trip(block: usize, segments: usize) -> (i64, i64) {
    let layout = Layout::from_size_align(block, 16).unwrap();
    let n = segments * (SEGMENT / block);
    // SAFETY: nonzero size; every block is freed exactly once below.
    let first: Vec<usize> = (0..n)
        .map(|_| unsafe { GLOBAL.alloc(layout) } as usize)
        .collect();
    assert!(
        first.iter().all(|&p| p != 0),
        "{block} B: allocation failed"
    );
    let peak = live_segments();

    let (tx, rx) = mpsc::channel::<Vec<usize>>();
    let freer = thread::spawn(move || {
        for p in rx.recv().unwrap() {
            // SAFETY: allocated above with `layout`, ownership moved here.
            unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
        }
    });
    tx.send(first).unwrap();
    freer.join().unwrap();

    // SAFETY: as above.
    let second: Vec<usize> = (0..n)
        .map(|_| unsafe { GLOBAL.alloc(layout) } as usize)
        .collect();
    assert!(
        second.iter().all(|&p| p != 0),
        "{block} B: reallocation failed"
    );
    let after = live_segments();
    for p in second {
        // SAFETY: allocated above, freed once.
        unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
    }
    (peak, after)
}

#[test]
fn remote_frees_of_hot_small_classes_are_reused() {
    for block in [16usize, 64, 256] {
        // 8 segments' worth: a total loss of the remote frees grows the heap by 8
        // segments, far above the slack of 2 (16 B is about 2 million blocks).
        let (peak, after) = round_trip(block, 8);
        assert!(
            after <= peak + 2,
            "{block} B: owner grew instead of reusing remote frees: live segments {peak} -> {after}"
        );
    }
}
