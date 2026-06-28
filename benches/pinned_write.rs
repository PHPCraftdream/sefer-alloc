#![allow(deprecated)]
//! Phase 7c — pinned thread-per-core write bench (criterion).
//!
//! Compares write throughput of `ShardedRegion<T>` when each worker is **pinned
//! to a core AND explicitly bound to the matching shard** (the `pinning` path)
//! against the **unpinned** 7a path (each worker lazily claims a shard via the
//! TLS router, with no OS affinity).
//!
//! ## Honest verdict (workload-dependent)
//!
//! Pinning is **not** a guaranteed win — see `src/concurrent/pinning.rs` and
//! `docs/PLAN.md` §7c. It helps when the per-shard working set is cache-hot and
//! shard access is truly thread-local; it does little when the workload is
//! memory-bandwidth-bound or read-heavy with random cross-shard handles. This
//! bench uses a tight insert loop (cache-hostile: each insert touches the slot
//! table), so the win here is a LOWER BOUND on the locality benefit a real
//! read-heavy workload might see. Treat the numbers as "does pinning hurt?"
//! (it should not) rather than "how much does it help?" (measure your own
//! workload).
//!
//! This is a SHORT scenario per the speed policy: `sample_size(10)` and short
//! warm/measurement times so the whole suite finishes in a few seconds. Numbers
//! are rough; the relative ordering (pinned vs unpinned) is what matters.

#![cfg(feature = "pinning")]
// Benches are not shipped code. Pedantic lints that flag intentional patterns
// here (fixed-N truncation casts, criterion closure formatting) are allowed at
// the file level, mirroring `benches/sharded_write.rs`. The library itself
// stays fully pedantic-clean.
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::hint::black_box;
use std::sync::Arc;
use std::thread::scope;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::{PinnedRunner, ShardedRegion};

/// Inserts per thread per bench iteration. Mirrors `benches/sharded_write.rs`.
const INSERTS_PER_THREAD: usize = 4_000;

/// The thread counts to sweep. Capped modestly so the bench stays fast.
const THREAD_COUNTS: &[usize] = &[1, 2, 4];

/// Pinned path: one worker per core, each pinned to core *i* and bound to
/// shard *i* via `PinnedRunner`. Pinning is best-effort; the binding routes
/// deterministically regardless.
fn bench_pinned_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("pinned_write");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &threads in THREAD_COUNTS {
        let label = format!("pinned/threads={threads}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let region = Arc::new(ShardedRegion::<u64>::with_shards(
                    threads,
                    INSERTS_PER_THREAD,
                ));
                // `with_workers` caps the runner to exactly `threads` workers
                // (and to the region's shard count, which is also `threads`).
                let runner = PinnedRunner::with_workers(&region, threads)
                    .expect("core_affinity::get_core_ids should succeed in a bench process");
                assert_eq!(runner.worker_count(), threads);
                runner.run_arc(&region, |_shard_id, region| {
                    for v in 0..u64::try_from(INSERTS_PER_THREAD).unwrap() {
                        let _ = black_box(region.insert(v));
                    }
                });
                black_box(region);
            });
        });
    }

    group.finish();
}

/// Unpinned baseline: the 7a path. Each worker lazily claims a shard via the
/// TLS router; no OS affinity, no explicit bind. This is the apples-to-apples
/// comparison for "does pinning help?".
fn bench_unpinned_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("pinned_write");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &threads in THREAD_COUNTS {
        let label = format!("unpinned/threads={threads}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let region = Arc::new(ShardedRegion::<u64>::with_shards(
                    threads,
                    INSERTS_PER_THREAD,
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

criterion_group!(benches, bench_pinned_writes, bench_unpinned_writes);
criterion_main!(benches);
