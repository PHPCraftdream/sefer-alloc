//! Phase 9 -- per-thread heap alloc/dealloc hot path vs mimalloc and the
//! system allocator. Quick criterion profile per the short-scenario policy:
//! `sample_size(10)` and short warm/measurement times. Honest verdict in
//! `docs/HEAP_BENCH.md`.
//!
//! The bench measures the STEADY-STATE hot path: bootstrap overhead is outside
//! the timing loop (the heap is pre-warmed with one batch of alloc+dealloc so
//! the free lists are populated). This isolates the lock-free pop/push that
//! Phase 9 targets.

#![cfg(feature = "alloc")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use sefer_alloc::Heap;

/// Size classes to bench (representative small sizes).
const SIZES: &[usize] = &[16, 64, 256, 1024];

/// Number of alloc/dealloc pairs per iteration.
const OPS: usize = 1024;

fn bench_heap_vs_mimalloc_vs_system(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_alloc");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // --- sefer Heap (steady-state: free lists pre-populated) ---
        group.bench_function(format!("Heap/{size}B"), |b| {
            b.iter_batched(
                || {
                    // Setup: create heap + pre-warm the free list for this class.
                    let mut heap = Heap::new().unwrap();
                    let mut warm: Vec<*mut u8> = Vec::with_capacity(OPS);
                    for _ in 0..OPS {
                        warm.push(heap.alloc(layout));
                    }
                    for &p in &warm {
                        heap.dealloc(p, layout);
                    }
                    heap
                },
                |mut heap| {
                    // Timed: pure alloc+dealloc from the pre-warmed free list.
                    let mut ptrs = Vec::with_capacity(OPS);
                    for _ in 0..OPS {
                        ptrs.push(heap.alloc(layout));
                    }
                    black_box(&ptrs);
                    for &ptr in &ptrs {
                        heap.dealloc(ptr, layout);
                    }
                    black_box(&heap);
                },
                BatchSize::SmallInput,
            )
        });

        // --- mimalloc ---
        group.bench_function(format!("mimalloc/{size}B"), |b| {
            let mi = mimalloc::MiMalloc;
            b.iter(|| {
                let mut ptrs = Vec::with_capacity(OPS);
                for _ in 0..OPS {
                    // SAFETY: layout has non-zero size and valid alignment.
                    let ptr = unsafe { mi.alloc(layout) };
                    ptrs.push(ptr);
                }
                black_box(&ptrs);
                for &ptr in &ptrs {
                    // SAFETY: ptr was allocated by mimalloc with the same layout.
                    unsafe { mi.dealloc(ptr, layout) };
                }
            })
        });

        // --- System allocator ---
        group.bench_function(format!("System/{size}B"), |b| {
            b.iter(|| {
                let mut ptrs = Vec::with_capacity(OPS);
                for _ in 0..OPS {
                    // SAFETY: layout has non-zero size.
                    let ptr = unsafe { System.alloc(layout) };
                    ptrs.push(ptr);
                }
                black_box(&ptrs);
                for &ptr in &ptrs {
                    // SAFETY: ptr was allocated by System with the same layout.
                    unsafe { System.dealloc(ptr, layout) };
                }
            })
        });
    }

    group.finish();
}

criterion_group!(benches, bench_heap_vs_mimalloc_vs_system);
criterion_main!(benches);
