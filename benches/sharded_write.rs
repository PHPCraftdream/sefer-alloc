#![allow(deprecated)]
//! Phase 7a — sharded write-scaling benches (criterion).
//!
//! Compares write throughput of `ShardedRegion<T>` (thread-local shard binding,
//! zero new `unsafe`) against the two lock-serialised baselines:
//!
//! - `SyncRegion<T>` — `RwLock<Region<T>>`, the trusted concurrent default.
//! - `Arc<Mutex<Region<T>>>` — the coarse-grained lock baseline.
//!
//! across thread counts 1, 2, 4. The expectation (per `docs/PLAN.md` 7a): write
//! throughput RISES with thread count for `ShardedRegion` (writers in different
//! shards never meet on a lock), while the two baselines serialise on one lock
//! and roughly flat-line (or degrade from contention overhead).
//!
//! This is a SHORT scenario per the short-scenario policy: `sample_size(10)`
//! and short warm/measurement times so the whole suite finishes in a few
//! seconds. Numbers are rough; the relative SCALING (does ShardedRegion rise
//! with threads?) is what matters. The honest verdict will land in
//! `docs/BENCHMARKS.md`.

#![cfg(feature = "experimental")]
// Benches are not shipped code. Pedantic lints that flag intentional patterns
// here (fixed-N truncation casts, criterion closure formatting) are allowed at
// the file level, mirroring `benches/locality.rs`.
// The library itself stays fully pedantic-clean.
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::hint::black_box;
use std::sync::{Arc, Mutex};
use std::thread::scope;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::{Region, ShardedRegion, SyncRegion};

/// Inserts per thread per bench iteration. Modest so a single iter is fast and
/// criterion can take many samples; large enough that lock contention (not
/// thread spawn) dominates the measurement.
const INSERTS_PER_THREAD: usize = 4_000;

/// The thread counts to sweep.
const THREAD_COUNTS: &[usize] = &[1, 2, 4];

/// `ShardedRegion` write throughput: each thread inserts into its own
/// thread-local shard, so writers in different shards never contend.
fn bench_sharded_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("sharded_write");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &threads in THREAD_COUNTS {
        let label = format!("ShardedRegion/threads={threads}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                // One shard per thread (the 7a sweet spot for a bounded pool).
                let region = Arc::new(ShardedRegion::<u64>::with_shards(
                    threads,
                    INSERTS_PER_THREAD,
                ));
                scope(|s| {
                    for _ in 0..threads {
                        let region = Arc::clone(&region);
                        s.spawn(move || {
                            for v in 0..u64::try_from(INSERTS_PER_THREAD).unwrap() {
                                // Insert may return Err only if the shard is full;
                                // sized to fit, so it won't here.
                                let _ = black_box(region.insert(v));
                            }
                        });
                    }
                });
                black_box(region);
            });
        });
    }

    group.finish();
}

/// `SyncRegion` baseline: every writer takes the `RwLock` write guard, so
/// writers fully serialise regardless of thread count.
fn bench_sync_region_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("sharded_write");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &threads in THREAD_COUNTS {
        let label = format!("SyncRegion/threads={threads}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let region = Arc::new(SyncRegion::<u64>::with_capacity(
                    INSERTS_PER_THREAD * threads,
                ));
                scope(|s| {
                    for _ in 0..threads {
                        let region = Arc::clone(&region);
                        s.spawn(move || {
                            for v in 0..u64::try_from(INSERTS_PER_THREAD).unwrap() {
                                let _ = black_box(region.insert(v));
                            }
                        });
                    }
                });
                black_box(region);
            });
        });
    }

    group.finish();
}

/// `Arc<Mutex<Region<T>>>` baseline: the coarse-grained lock. Writers fully
/// serialise (one mutex for the whole region), like `SyncRegion` but without
/// the `RwLock`'s read/write distinction.
fn bench_mutex_region_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("sharded_write");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &threads in THREAD_COUNTS {
        let label = format!("Arc<Mutex<Region>>/threads={threads}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let region = Arc::new(Mutex::new(Region::<u64>::with_capacity(
                    INSERTS_PER_THREAD * threads,
                )));
                scope(|s| {
                    for _ in 0..threads {
                        let region = Arc::clone(&region);
                        s.spawn(move || {
                            for v in 0..u64::try_from(INSERTS_PER_THREAD).unwrap() {
                                let _ = black_box(region.lock().unwrap().insert(v));
                            }
                        });
                    }
                });
                black_box(region);
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_sharded_writes,
    bench_sync_region_writes,
    bench_mutex_region_writes
);
criterion_main!(benches);
