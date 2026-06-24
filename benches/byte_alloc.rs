//! Phase 4 — byte-allocator benches (criterion).
//!
//! Compares `ByteRegion` alloc/dealloc throughput against `std::alloc::System`
//! across a few size classes. This is a SHORT scenario per the short-scenario
//! policy: `sample_size(10)` and short warm/measurement times so the whole
//! suite finishes in a few seconds. Numbers are rough; the relative order is
//! what matters. The honest verdict lives in `docs/BYTE_BENCH.md`.

#![cfg(feature = "byte")]

// Benches are not shipped code. Pedantic lints that flag intentional patterns
// here (fixed-N truncation casts, criterion closure formatting) are allowed at
// the file level, mirroring `benches/locality.rs`. The library itself stays
// fully pedantic-clean.
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::ByteRegion;

/// Size classes to bench (sizes that exercise the in-arena path).
const SIZES: &[usize] = &[8, 64, 256, 1024];

fn bench_alloc_dealloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("byte_alloc");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        let label = format!("ByteRegion/{size}B");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let mut region = ByteRegion::new();
                let ptrs: Vec<*mut u8> = (0..512).map(|_| region.alloc(layout)).collect();
                black_box(&ptrs);
                for &ptr in &ptrs {
                    // SAFETY: each `ptr` was returned by `region.alloc(layout)`
                    // in this iteration and not yet freed.
                    unsafe { region.dealloc(ptr, layout) };
                }
                black_box(&region);
            });
        });

        let label = format!("System/{size}B");
        group.bench_function(&label, |b| {
            b.iter(|| {
                let ptrs: Vec<*mut u8> = (0..512)
                    .map(|_| unsafe { System.alloc(layout) })
                    .collect();
                black_box(&ptrs);
                for &ptr in &ptrs {
                    unsafe { System.dealloc(ptr, layout) };
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, bench_alloc_dealloc);
criterion_main!(benches);
