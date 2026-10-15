//! Joined thread waves must reuse registry FREE slots instead of minting 2K heaps.
#![cfg(all(feature = "production", feature = "internals", not(loom)))]
#![forbid(unsafe_code)]

use sefer_alloc::SeferAlloc;
use std::sync::{mpsc, Arc, Barrier};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const K: usize = 4;
const WAVES: usize = 6;

fn wave() {
    let barrier = Arc::new(Barrier::new(K + 1));
    let (ready_tx, ready_rx) = mpsc::channel();
    let mut workers = Vec::with_capacity(K);
    for _ in 0..K {
        let ready_tx = ready_tx.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            // >32-byte object size avoids the exact-object-prototype class.
            let value = vec![0xabu8; 128];
            std::hint::black_box(&value);
            ready_tx.send(()).expect("coordinator remains connected");
            barrier.wait();
            drop(value);
        }));
    }
    drop(ready_tx);
    for _ in 0..K {
        ready_rx.recv().expect("each worker reaches barrier");
    }
    barrier.wait();
    for worker in workers {
        worker.join().expect("worker and TLS teardown");
    }
}

#[test]
fn repeated_joined_waves_bound_registry_growth_to_one_wave() {
    wave();
    let before = GLOBAL.stats();
    for _ in 0..WAVES {
        wave();
    }
    let after = GLOBAL.stats();
    let growth = after
        .heaps_claimed_high_water
        .saturating_sub(before.heaps_claimed_high_water);
    assert!(
        growth <= K as u64,
        "waves minted {growth} heaps; expected at most {K}"
    );
    assert_eq!(
        after.config_conflicts, before.config_conflicts,
        "reuse must preserve first-bind config"
    );
}
