//! Real installed-allocator regression: joined waves must reuse FREE heaps.
#![cfg(all(feature = "production", feature = "internals", not(loom)))]
#![forbid(unsafe_code)]

use sefer_alloc::SeferAlloc;
use std::sync::{Arc, Barrier};

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

const K: usize = 4;
const W: usize = 6;

fn run_wave() {
    let barrier = Arc::new(Barrier::new(K + 1));
    let mut handles = Vec::with_capacity(K);
    for _ in 0..K {
        let barrier = Arc::clone(&barrier);
        handles.push(std::thread::spawn(move || {
            let block = vec![0xabu8; 128];
            std::hint::black_box(&block);
            drop(block);
            // Every worker has claimed and freed before any TLS exit can
            // publish a reuse hint. Keep all claims LIVE until release.
            barrier.wait();
            barrier.wait();
        }));
    }
    barrier.wait();
    barrier.wait();
    for handle in handles {
        handle
            .join()
            .expect("worker must finish, including TLS teardown");
    }
    assert!(!SeferAlloc::maintenance_running());
}

#[test]
fn joined_thread_waves_reuse_free_heaps() {
    assert!(!SeferAlloc::maintenance_running());
    // Bind this test thread before measuring and retain it across all waves.
    let test_live = vec![1u8; 4096];
    std::hint::black_box(&test_live);
    run_wave();
    let before = GLOBAL.stats();
    for _ in 0..W {
        run_wave();
    }
    let after = GLOBAL.stats();
    let delta = after.heaps_claimed_high_water - before.heaps_claimed_high_water;
    let conflicts_delta = after.config_conflicts - before.config_conflicts;
    println!(
        "W={W} K={K} heaps_claimed_high_water before={} after={} delta={delta}; config_conflicts before={} after={} delta={conflicts_delta}",
        before.heaps_claimed_high_water,
        after.heaps_claimed_high_water,
        before.config_conflicts,
        after.config_conflicts,
    );
    assert_eq!(
        conflicts_delta, 0,
        "waves must not introduce config conflicts"
    );
    assert!(
        delta <= K as u64,
        "joined waves must reuse FREE heaps: delta={delta}, limit={K}"
    );
    std::hint::black_box(&test_live);
}
