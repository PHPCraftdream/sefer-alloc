//! Phase 7d — sharded byte-arena parallel-allocation bench (criterion).
//!
//! Compares parallel raw-allocation throughput of [`ShardedByteArena`] (one
//! `Mutex<ByteRegion>` per shard; a thread allocates only from its own shard,
//! so threads in different shards never contend) against the single-`Mutex`
//! [`ByteAllocator`] (every alloc/dealloc serialises through one global mutex),
//! across thread counts 1, 2, 4.
//!
//! Each worker does `OPS_PER_THREAD` alloc→dealloc cycles of a fixed 64-byte
//! layout (its own pointers — no cross-thread free, so this measures the
//! alloc/dealloc hot path, not the owner-scan). The expectation per
//! `docs/PLAN.md` 7d: the sharded arena's throughput rises with thread count
//! (independent shards), while the single-`Mutex` baseline flat-lines or
//! degrades under contention.
//!
//! SHORT scenario per the speed policy: `sample_size(10)` + short warm/measure,
//! so the whole suite finishes in a few seconds. Numbers are rough; the
//! relative SCALING is the point. The honest verdict lives in
//! `docs/BYTE_SHARDED_BENCH.md` — this arena parallelises across shards but is
//! NOT a mimalloc competitor.

#![cfg(feature = "byte-sharded")]
// Benches are not shipped code; allow the patterns benches use. The library
// itself stays fully pedantic-clean.
#![allow(clippy::cast_possible_truncation, clippy::semicolon_if_nothing_returned)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::sync::Arc;
use std::thread::scope;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::{ByteAllocator, ShardedByteArena};

/// Alloc→dealloc cycles per thread per bench iteration. Modest so a single
/// iter is fast while still dominated by allocator work, not thread spawn.
const OPS_PER_THREAD: usize = 4_000;

/// The thread counts to sweep.
const THREAD_COUNTS: &[usize] = &[1, 2, 4];

/// The fixed allocation layout — a small in-arena size class.
fn layout() -> Layout {
    Layout::from_size_align(64, 8).expect("valid layout")
}

/// `ShardedByteArena`: each thread allocs/deallocs in its own bound shard, so
/// threads in different shards never contend on a lock.
fn bench_sharded_arena(c: &mut Criterion) {
    let mut group = c.benchmark_group("byte_sharded");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &threads in THREAD_COUNTS {
        let label = format!("ShardedByteArena/threads={threads}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                // One shard per thread (the 7d sweet spot for a bounded pool).
                let arena = Arc::new(ShardedByteArena::with_shards(threads));
                scope(|s| {
                    for _ in 0..threads {
                        let arena = Arc::clone(&arena);
                        s.spawn(move || {
                            let l = layout();
                            for _ in 0..OPS_PER_THREAD {
                                let p = arena.alloc(l);
                                // SAFETY: `p` was just returned by `arena.alloc(l)`
                                // and has not been freed; `l` matches.
                                unsafe { arena.dealloc(black_box(p), l) };
                            }
                        });
                    }
                });
                black_box(arena);
            });
        });
    }
    group.finish();
}

/// `ByteAllocator` baseline: every alloc/dealloc takes the one global mutex, so
/// the threads fully serialise regardless of thread count.
fn bench_single_mutex_allocator(c: &mut Criterion) {
    let mut group = c.benchmark_group("byte_sharded");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &threads in THREAD_COUNTS {
        let label = format!("ByteAllocator(1 Mutex)/threads={threads}");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let alloc = Arc::new(ByteAllocator::new());
                scope(|s| {
                    for _ in 0..threads {
                        let alloc = Arc::clone(&alloc);
                        s.spawn(move || {
                            let l = layout();
                            for _ in 0..OPS_PER_THREAD {
                                // SAFETY: `GlobalAlloc::alloc`/`dealloc` — `p` is
                                // freshly allocated with `l` and freed once with
                                // the same `l`.
                                unsafe {
                                    let p = alloc.alloc(l);
                                    alloc.dealloc(black_box(p), l);
                                }
                            }
                        });
                    }
                });
                black_box(alloc);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_sharded_arena, bench_single_mutex_allocator);
criterion_main!(benches);
