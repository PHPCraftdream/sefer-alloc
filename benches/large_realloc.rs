//! Large-block and realloc-heavy profiling — SeferAlloc vs mimalloc vs System.
//!
//! Targets three gaps identified in task #54:
//!
//! 1. **`large_alloc_free`** — single-shot alloc+free of 4 MiB / 16 MiB / 64 MiB
//!    blocks. In the current substrate every large allocation is a dedicated
//!    `mmap`/`VirtualAlloc` segment, so this directly measures the OS-round-trip
//!    cost for each allocator.
//!
//! 2. **`realloc_grow_geometric`** — manual doubling from 64 B up through 4 MiB
//!    (16 doublings). Uses `GlobalAlloc::realloc` directly (no `Vec` amortization
//!    hiding the cost) so the full grow cycle is captured honestly.
//!
//! 3. **`realloc_in_place_unfavorable`** — repeated `realloc` of a single block
//!    while competing allocations live between the old and next candidate
//!    addresses, preventing in-place growth. Quantifies the copy-and-free cost
//!    under adversarial neighbour pressure.
//!
//! All three groups compare three allocators through their `GlobalAlloc` trait
//! implementations called directly (no process-level `#[global_allocator]`
//! installation in this bench binary). Quick criterion profile per the
//! short-scenario policy: `sample_size(10)`, 1-2 s warm-up, 2-3 s measurement.

#![cfg(feature = "alloc-global")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sefer_alloc::SeferAlloc;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Allocate a block through `alloc`, black-box the pointer, then free it.
/// Used by `large_alloc_free`.
#[inline]
fn alloc_free_one<A: GlobalAlloc>(a: &A, layout: Layout) {
    // SAFETY: layout has non-zero size and power-of-two alignment.
    let ptr = unsafe { a.alloc(layout) };
    black_box(ptr);
    if !ptr.is_null() {
        // SAFETY: ptr was returned by `a.alloc(layout)` above.
        unsafe { a.dealloc(ptr, layout) };
    }
}

/// Perform a geometric grow sequence via `GlobalAlloc::realloc`.
///
/// Starts with a block of `start` bytes and doubles it `doublings` times,
/// calling `realloc` at each step. Returns the final size so the compiler
/// cannot optimise the whole loop away.
#[inline]
fn realloc_grow<A: GlobalAlloc>(a: &A, start: usize, doublings: u32) -> usize {
    let align = 8_usize;
    let init_layout = Layout::from_size_align(start, align).unwrap();
    // SAFETY: init_layout has non-zero size and valid alignment.
    let mut ptr = unsafe { a.alloc(init_layout) };
    if ptr.is_null() {
        return 0;
    }
    let mut current_size = start;

    for _ in 0..doublings {
        let new_size = current_size * 2;
        let old_layout = Layout::from_size_align(current_size, align).unwrap();
        // SAFETY: ptr was returned by a prior alloc/realloc call with `old_layout`;
        // `new_size` is non-zero.
        let new_ptr = unsafe { a.realloc(ptr, old_layout, new_size) };
        if new_ptr.is_null() {
            // OOM — free what we have and bail early.
            unsafe { a.dealloc(ptr, old_layout) };
            return current_size;
        }
        ptr = new_ptr;
        current_size = new_size;
    }

    black_box(ptr);
    let final_layout = Layout::from_size_align(current_size, align).unwrap();
    // SAFETY: ptr is the result of the last successful alloc/realloc with
    // `final_layout`.
    unsafe { a.dealloc(ptr, final_layout) };
    current_size
}

/// Perform a series of `realloc` grow steps while holding a set of "neighbour"
/// allocations that prevent any in-place extension (the allocator must
/// copy-and-free every time a contiguous extension would have been possible).
///
/// The pattern:
///   1. Alloc `start` bytes (the subject block).
///   2. Alloc `neighbours` small blocks as noise (pins address space around subject).
///   3. Repeatedly realloc-grow the subject `steps` times by `step_size`.
///   4. Free neighbours, then free subject.
///
/// The neighbours are held alive through the whole grow sequence, so the
/// subject block cannot reuse or extend its original span in-place.
#[inline]
fn realloc_unfavorable<A: GlobalAlloc>(
    a: &A,
    start: usize,
    step_size: usize,
    steps: usize,
    neighbours: usize,
) {
    let align = 8_usize;
    let start_layout = Layout::from_size_align(start, align).unwrap();
    let noise_layout = Layout::from_size_align(64, align).unwrap();

    // Subject allocation.
    // SAFETY: start_layout is non-zero size, valid alignment.
    let mut subject = unsafe { a.alloc(start_layout) };
    if subject.is_null() {
        return;
    }
    let mut subject_size = start;

    // Neighbours: allocated after the subject to occupy subsequent address space.
    let mut noise: Vec<*mut u8> = Vec::with_capacity(neighbours);
    for _ in 0..neighbours {
        // SAFETY: noise_layout is non-zero size, valid alignment.
        let p = unsafe { a.alloc(noise_layout) };
        noise.push(p);
    }

    // Grow subject while neighbours are live.
    for _ in 0..steps {
        let new_size = subject_size + step_size;
        let old_layout = Layout::from_size_align(subject_size, align).unwrap();
        // SAFETY: subject was returned by the last alloc/realloc with old_layout.
        let new_ptr = unsafe { a.realloc(subject, old_layout, new_size) };
        if new_ptr.is_null() {
            let cur_layout = Layout::from_size_align(subject_size, align).unwrap();
            unsafe { a.dealloc(subject, cur_layout) };
            subject = core::ptr::null_mut();
            break;
        }
        subject = new_ptr;
        subject_size = new_size;
    }

    black_box(subject);

    // Free neighbours.
    for &p in &noise {
        if !p.is_null() {
            // SAFETY: p was returned by `a.alloc(noise_layout)`.
            unsafe { a.dealloc(p, noise_layout) };
        }
    }

    // Free subject.
    if !subject.is_null() {
        let final_layout = Layout::from_size_align(subject_size, align).unwrap();
        // SAFETY: subject is the result of the last successful realloc.
        unsafe { a.dealloc(subject, final_layout) };
    }
}

// ── benchmark groups ──────────────────────────────────────────────────────────

/// Group 1: single large alloc+free for 4 MiB, 16 MiB, 64 MiB.
///
/// SeferAlloc routes each of these through a dedicated OS segment
/// (`alloc_large`). This measures the round-trip mmap/VirtualAlloc cost
/// per allocator.
fn bench_large_alloc_free(c: &mut Criterion) {
    let mut group = c.benchmark_group("large_alloc_free");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));

    // Sizes: 4 MiB, 16 MiB, 64 MiB.
    const LARGE_SIZES: &[(usize, &str)] = &[
        (4 * 1024 * 1024, "4MiB"),
        (16 * 1024 * 1024, "16MiB"),
        (64 * 1024 * 1024, "64MiB"),
    ];

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    for &(size, label) in LARGE_SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        group.bench_with_input(BenchmarkId::new("SeferAlloc", label), &layout, |b, &l| {
            b.iter(|| alloc_free_one(&sefer, l))
        });
        group.bench_with_input(BenchmarkId::new("mimalloc", label), &layout, |b, &l| {
            b.iter(|| alloc_free_one(&mi, l))
        });
        group.bench_with_input(BenchmarkId::new("System", label), &layout, |b, &l| {
            b.iter(|| alloc_free_one(&sys, l))
        });
    }

    group.finish();
}

/// Group 2: geometric grow via `GlobalAlloc::realloc` — start 64 B, double 16×
/// to reach 4 MiB.
///
/// This bypasses `Vec`'s amortized-copy strategy and calls `realloc` on every
/// step, so the bench captures the raw grow-cycle cost (alloc-new + copy-old +
/// dealloc-old when in-place is impossible, or in-place when supported).
fn bench_realloc_grow_geometric(c: &mut Criterion) {
    let mut group = c.benchmark_group("realloc_grow_geometric");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(2));

    // 16 doublings: 64 B → 128 B → … → 4 MiB.
    const START: usize = 64;
    const DOUBLINGS: u32 = 16;

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    group.bench_function("SeferAlloc", |b| {
        b.iter(|| black_box(realloc_grow(&sefer, START, DOUBLINGS)))
    });
    group.bench_function("mimalloc", |b| {
        b.iter(|| black_box(realloc_grow(&mi, START, DOUBLINGS)))
    });
    group.bench_function("System", |b| {
        b.iter(|| black_box(realloc_grow(&sys, START, DOUBLINGS)))
    });

    group.finish();
}

/// Group 3: realloc under adversarial neighbour pressure (unfavorable in-place
/// condition).
///
/// A subject block is grown in 256 KiB steps (8 steps: 512 KiB → 2.5 MiB)
/// while 32 neighbour allocations occupy adjacent address space. Because the
/// neighbours are held alive the entire time, the allocator cannot extend the
/// subject in-place — it must alloc-new + copy + dealloc-old on every step.
/// This measures the copy-and-free degradation path.
fn bench_realloc_in_place_unfavorable(c: &mut Criterion) {
    let mut group = c.benchmark_group("realloc_in_place_unfavorable");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    const START: usize = 512 * 1024; // 512 KiB
    const STEP: usize = 256 * 1024; // 256 KiB per step
    const STEPS: usize = 8; // up to 512 KiB + 8 × 256 KiB = 2.5 MiB
    const NEIGHBOURS: usize = 32; // noise allocs to pin address space

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    group.bench_function("SeferAlloc", |b| {
        b.iter(|| realloc_unfavorable(&sefer, START, STEP, STEPS, NEIGHBOURS))
    });
    group.bench_function("mimalloc", |b| {
        b.iter(|| realloc_unfavorable(&mi, START, STEP, STEPS, NEIGHBOURS))
    });
    group.bench_function("System", |b| {
        b.iter(|| realloc_unfavorable(&sys, START, STEP, STEPS, NEIGHBOURS))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_large_alloc_free,
    bench_realloc_grow_geometric,
    bench_realloc_in_place_unfavorable,
);
criterion_main!(benches);
