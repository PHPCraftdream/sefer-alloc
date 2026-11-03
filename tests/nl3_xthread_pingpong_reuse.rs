//! NL-3: steady-state cross-thread ping-pong — the owner allocates a small batch,
//! another thread frees seven blocks in eight, the owner waits for the ack and
//! allocates again, round after round (the shape of the `larson` / `mstress`
//! workloads, not the single million-free burst of
//! `nl3_xthread_remote_free_reuse_small_classes`).
//!
//! Oracle: once warmed up, the blocks the owner allocates must be addresses that
//! another thread freed earlier (reuse), and the live segment count must grow only
//! by what the retained one-in-eight blocks need. A remote free the owner refuses
//! or loses shows as fresh addresses and as segment growth. Run in the debug and the
//! release test rows: a trusting (no misuse checks) mode is a release-profile
//! change and its debug assertions are gone there.
//!
//! Excluded under `numa-aware` for the reason given in `g1_ingress_remote_free_reuse`.
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
const BATCH: usize = 512;
const ROUNDS: usize = 1500;
const WARMUP: usize = 100;

fn live_segments() -> i64 {
    let s = GLOBAL.stats();
    s.segments_reserved_total as i64 - s.segments_released_total as i64
}

/// Run the ping-pong for `block`-byte blocks; return
/// `(reused, fresh, segment growth after warm-up, retained bytes)`.
fn ping_pong(block: usize) -> (usize, usize, i64, usize) {
    let layout = Layout::from_size_align(block, 16).unwrap();
    let (to_free, freer_in) = mpsc::channel::<Vec<usize>>();
    let (ack_tx, ack_rx) = mpsc::channel::<Vec<usize>>();
    let freer = thread::spawn(move || {
        while let Ok(batch) = freer_in.recv() {
            for &p in &batch {
                // SAFETY: allocated by the owner with `layout`, ownership moved here.
                unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
            }
            ack_tx.send(batch).unwrap();
        }
    });

    let mut kept: Vec<usize> = Vec::new();
    let mut freed_pool: HashSet<usize> = HashSet::new();
    let (mut reused, mut fresh) = (0usize, 0usize);
    let mut live_after_warmup = 0i64;
    for round in 0..ROUNDS {
        if round == WARMUP {
            live_after_warmup = live_segments();
        }
        let mut to_remote = Vec::with_capacity(BATCH);
        for i in 0..BATCH {
            // SAFETY: nonzero size; every block is freed exactly once.
            let p = unsafe { GLOBAL.alloc(layout) } as usize;
            assert_ne!(p, 0, "{block} B: allocation failed");
            if round >= WARMUP {
                if freed_pool.remove(&p) {
                    reused += 1;
                } else {
                    fresh += 1;
                }
            }
            if i % 8 == 0 {
                kept.push(p);
            } else {
                to_remote.push(p);
            }
        }
        to_free.send(to_remote).unwrap();
        for p in ack_rx.recv().unwrap() {
            freed_pool.insert(p);
        }
    }
    let growth = live_segments() - live_after_warmup;
    drop(to_free);
    freer.join().unwrap();
    let retained = kept.len() * block;
    for p in kept {
        // SAFETY: allocated above, freed once, on the owner thread.
        unsafe { GLOBAL.dealloc(p as *mut u8, layout) };
    }
    (reused, fresh, growth, retained)
}

#[test]
fn ping_pong_remote_frees_are_reused_in_steady_state() {
    let mut failures = Vec::new();
    for block in [16usize, 64, 256] {
        let (reused, fresh, growth, retained) = ping_pong(block);
        // Retained blocks (one in eight) legitimately take space; the rest must be
        // recycled. Allow two segments of slack on top of the retained bytes.
        let allowed = (retained / SEGMENT) as i64 + 2;
        let total = reused + fresh;
        // Retained blocks are a stable 1/8 of every batch and are never in the pool,
        // so at best 7/8 of the allocations can be reuse; require 70 % of the total.
        if growth > allowed || (reused as f64) < 0.70 * total as f64 {
            failures.push(format!(
                "{block} B: reused {reused} of {total} allocations after warm-up, \
                 segment growth {growth} (allowed {allowed}, retained {retained} bytes)"
            ));
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}
