//! SC-6 A-exact (P23): an armed heap trusts a negative discovery answer, so
//! every remote free must reach its owner through the ingress notification.
//! The owner thread grows its heap over many segments, another thread frees
//! every block (remote frees), and the owner allocates the same amount again.
//!
//! Non-vacuous: with a broken notification (never notifying, or an owner that
//! never takes/drains) the owner cannot see the remote frees, trusts "no free
//! block", and carves new segments: the net live segment count doubles and
//! the reuse fraction drops to zero, failing both assertions below.
#![cfg(feature = "alloc-global")]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;
use std::thread;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const BLOCK: usize = 1024;
const PER_SEGMENT: usize = 4 * 1024 * 1024 / BLOCK;

fn live_segments() -> i64 {
    let s = GLOBAL.stats();
    s.segments_reserved_total as i64 - s.segments_released_total as i64
}

fn round_trip(segments: usize) {
    let layout = Layout::from_size_align(BLOCK, 16).unwrap();
    let n = segments * PER_SEGMENT;
    // SAFETY: nonzero size; every block is freed exactly once below.
    let first: Vec<usize> = (0..n)
        .map(|_| unsafe { GLOBAL.alloc(layout) } as usize)
        .collect();
    assert!(first.iter().all(|&p| p != 0));
    let peak = live_segments();

    let (tx, rx) = mpsc::channel::<Vec<usize>>();
    let freer = thread::spawn(move || {
        for p in rx.recv().unwrap() {
            // SAFETY: allocated above with `layout`, ownership moved here.
            unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
        }
    });
    tx.send(first.clone()).unwrap();
    freer.join().unwrap();

    // SAFETY: as above.
    let second: Vec<usize> = (0..n)
        .map(|_| unsafe { GLOBAL.alloc(layout) } as usize)
        .collect();
    let after = live_segments();
    let mut old = first;
    old.sort_unstable();
    let reused = second
        .iter()
        .filter(|p| old.binary_search(p).is_ok())
        .count();
    for &p in &second {
        // SAFETY: allocated above, freed once.
        unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
    }
    assert!(
        after <= peak + 2,
        "owner grew instead of reusing remote frees: live segments {peak} -> {after} ({segments} segments)"
    );
    assert!(
        reused * 10 >= n * 9,
        "only {reused}/{n} remote-freed blocks were reissued ({segments} segments)"
    );
}

/// Both regimes in one test: the live-segment counter is process-wide, so the
/// two round trips must not overlap. Below the directory threshold (32
/// segments) discovery is a bins-only scan then ingress; above it the
/// directory miss is trusted.
#[test]
fn remote_frees_reach_the_owner() {
    round_trip(8);
    round_trip(48);
}
