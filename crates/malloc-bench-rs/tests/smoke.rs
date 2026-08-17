//! Smoke tests: verify that `run` and `sweep` produce non-zero ops/sec when
//! running against the system allocator with a small budget, and that the
//! `run_with` / `sweep_with` per-thread start hook fires once per worker.

use malloc_bench_rs::{run, run_with, sweep, sweep_with, Config, Workload};
use std::alloc::System;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn small_cfg() -> Config {
    Config {
        threads: 2,
        steps_per_thread: 5_000,
        working_set: 64,
        mstress_blocks: 32,
    }
}

#[test]
fn larson_system_returns_positive_ops() {
    let ops = run(Workload::Larson, &small_cfg(), || System);
    assert!(ops > 0.0, "larson ops/sec should be positive, got {ops}");
}

#[test]
fn mstress_system_returns_positive_ops() {
    let ops = run(Workload::Mstress, &small_cfg(), || System);
    assert!(ops > 0.0, "mstress ops/sec should be positive, got {ops}");
}

#[test]
fn sweep_returns_one_entry_per_thread_count() {
    let thread_sweep = [1usize, 2];
    let results = sweep(Workload::Larson, &small_cfg(), &thread_sweep, || System);
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].0, 1);
    assert_eq!(results[1].0, 2);
    for (t, ops) in &results {
        assert!(
            *ops > 0.0,
            "sweep entry T={t} should have positive ops/sec, got {ops}"
        );
    }
}

#[test]
fn run_with_fires_hook_once_per_worker_with_each_index() {
    // `threads: 4` → the hook must fire exactly 4 times, once per worker, with
    // thread indices {0, 1, 2, 3} — the identity a caller pins by.
    let cfg = Config {
        threads: 4,
        steps_per_thread: 2_000,
        working_set: 32,
        mstress_blocks: 16,
    };
    let count = Arc::new(AtomicUsize::new(0));
    let seen = Arc::new(Mutex::new(HashSet::new()));
    let count_c = Arc::clone(&count);
    let seen_c = Arc::clone(&seen);

    let ops = run_with(
        Workload::Larson,
        &cfg,
        || System,
        move |i| {
            count_c.fetch_add(1, Ordering::Relaxed);
            seen_c.lock().unwrap().insert(i);
        },
    );

    assert!(ops > 0.0, "run_with should still produce positive ops");
    assert_eq!(
        count.load(Ordering::Relaxed),
        4,
        "hook fires once per worker"
    );
    let seen = seen.lock().unwrap();
    assert_eq!(
        &*seen,
        &HashSet::from([0usize, 1, 2, 3]),
        "hook receives each 0-based worker index exactly once"
    );
}

#[test]
fn run_defaults_to_noop_hook() {
    // The no-pin `run` path must behave exactly like `run_with(.., |_| {})`.
    let ops = run(Workload::Mstress, &small_cfg(), || System);
    assert!(ops > 0.0, "run (no-op hook) should produce positive ops");
}

#[test]
fn sweep_with_fires_hook_for_every_run_in_the_sweep() {
    // sweep over [1, 2] → hook fires 1 + 2 = 3 times total across the two runs,
    // and every worker index stays within its run's `0..T`.
    let thread_sweep = [1usize, 2];
    let count = Arc::new(AtomicUsize::new(0));
    let max_index = Arc::new(AtomicUsize::new(0));
    let count_c = Arc::clone(&count);
    let max_c = Arc::clone(&max_index);

    let results = sweep_with(
        Workload::Larson,
        &small_cfg(),
        &thread_sweep,
        || System,
        move |i| {
            count_c.fetch_add(1, Ordering::Relaxed);
            max_c.fetch_max(i, Ordering::Relaxed);
        },
    );

    assert_eq!(results.len(), 2);
    assert_eq!(
        count.load(Ordering::Relaxed),
        3,
        "hook fires once per worker across both sweep runs (1 + 2)"
    );
    assert_eq!(
        max_index.load(Ordering::Relaxed),
        1,
        "max worker index is T-1 = 1 for the 2-thread run"
    );
}
