//! Smoke tests: verify that `run` and `sweep` produce non-zero ops/sec when
//! running against the system allocator with a small budget.

use malloc_bench_rs::{run, sweep, Config, Workload};
use std::alloc::System;

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
