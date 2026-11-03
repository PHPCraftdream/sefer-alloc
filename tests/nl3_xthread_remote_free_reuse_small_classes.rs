//! NL-3: remote frees of the hot small classes reach their owner and are reused.
//!
//! `g1_ingress_remote_free_reuse` pins this for the 1 KiB class only. The classes
//! that carry the magazine (16, 64, 256 B) are the ones whose per-block state a
//! trusting (no misuse checks) mode would stop maintaining: a block that arrives
//! at the owner through the cross-thread path with a stale state byte must still
//! be returned to a bin, not refused. This test is the behaviour oracle for that
//! set, in every build: the owner grows its heap over several segments, another
//! thread frees seven blocks in eight (the eighth stays live, so no segment becomes
//! empty and is recycled wholesale: only block-level reuse can satisfy the owner),
//! and the owner allocates the freed amount again; a refused or lost remote free
//! makes the owner carve new segments, so the live segment count would grow while
//! the freed addresses are not handed out again. The remote-free burst itself reserves
//! sidecar segments (about one per 260 thousand 16 B frees in the decommit builds), so
//! the growth is measured over the owner's second allocation phase only.
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
use std::collections::HashSet;
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

/// Grow over `segments` segments of `block`-byte blocks, free seven blocks in
/// eight from another thread, allocate that many again; return `(owner growth, reused fraction, detail)`.
fn round_trip(block: usize, segments: usize) -> (i64, f64, String) {
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

    let mut kept = Vec::with_capacity(n / 8 + 1);
    let mut freed = Vec::with_capacity(n - n / 8);
    for (i, p) in first.into_iter().enumerate() {
        if i % 8 == 0 {
            kept.push(p)
        } else {
            freed.push(p)
        }
    }
    let again = freed.len();
    let freed_set: HashSet<usize> = freed.iter().copied().collect();
    let before = GLOBAL.stats();
    let live_before = live_segments();
    let (tx, rx) = mpsc::channel::<Vec<usize>>();
    let freer = thread::spawn(move || {
        for p in rx.recv().unwrap() {
            // SAFETY: allocated above with `layout`, ownership moved here.
            unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
        }
    });
    tx.send(freed).unwrap();
    freer.join().unwrap();

    // SAFETY: as above.
    let second: Vec<usize> = (0..again)
        .map(|_| unsafe { GLOBAL.alloc(layout) } as usize)
        .collect();
    assert!(
        second.iter().all(|&p| p != 0),
        "{block} B: reallocation failed"
    );
    let growth = live_segments() - live_before;
    let reused = second.iter().filter(|p| freed_set.contains(p)).count();
    let detail = format!(
        "reused addresses {reused}/{again}; stats before {before:?}; after {:?}",
        GLOBAL.stats()
    );
    let reuse = reused as f64 / again as f64;
    for p in second.into_iter().chain(kept) {
        // SAFETY: allocated above, freed once.
        unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
    }
    (growth, reuse, detail)
}

#[test]
fn remote_frees_of_hot_small_classes_are_reused() {
    let mut failures = Vec::new();
    for block in [16usize, 64, 256] {
        // 8 segments' worth: a total loss of the remote frees grows the heap by 7
        // segments in the owner's second phase, far above the slack of 2 (16 B is
        // about 2 million blocks).
        let (growth, reuse, detail) = round_trip(block, 8);
        if growth > 2 || reuse < 0.9 {
            failures.push(format!(
                "{block} B: owner grew by {growth} segments, reused {:.1} % of the remotely freed addresses; {detail}",
                reuse * 100.0
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
