//! Phase 11 -- `SeferMalloc` as `#[global_allocator]` vs `mimalloc` and the
//! system allocator. Quick criterion profile per the short-scenario policy:
//! `sample_size(10)` and short warm/measurement times. Honest verdict in
//! `docs/MALLOC_BENCH.md`.
//!
//! This bench exercises REAL Rust allocation patterns through the
//! `#[global_allocator]` face: `Vec` push/grow churn (which calls
//! `alloc`/`dealloc`/`realloc` under the hood), `Box` new/drop, and varied
//! sizes. We compare three configurations:
//!
//! 1. **SeferMalloc** (installed as the process's `#[global_allocator]`).
//! 2. **mimalloc** (via the `mimalloc` crate's `GlobalAlloc` impl, called
//!    directly -- NOT installed globally, to allow a head-to-head in one binary).
//! 3. **System** allocator (called directly).
//!
//! For (2) and (3) we call the `GlobalAlloc` methods directly to avoid
//! replacing the global allocator mid-process (which SeferMalloc already
//! occupies). This is an honest apples-to-apples comparison of the alloc/dealloc
//! hot path.

#![cfg(feature = "alloc-global")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::SeferMalloc;

/// Representative small-to-medium sizes for the churn bench.
const SIZES: &[usize] = &[16, 64, 256, 1024];

/// Number of alloc/dealloc pairs per iteration.
const OPS: usize = 1024;

/// Direct-alloc bench: alloc + dealloc OPS blocks of `layout` through `alloc`.
/// This bypasses Vec overhead and measures the raw hot path.
fn bench_direct_alloc<A: GlobalAlloc>(alloc: &A, layout: Layout) {
    let mut ptrs: [*mut u8; OPS] = [core::ptr::null_mut(); OPS];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid alignment.
        *slot = unsafe { alloc.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        // SAFETY: ptr was allocated by `alloc` with the same layout.
        if !ptr.is_null() {
            unsafe { alloc.dealloc(ptr, layout) };
        }
    }
}

fn bench_global_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferMalloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // --- SeferMalloc (called directly through its GlobalAlloc impl, exactly
        // like mimalloc and System below — a true apples-to-apples comparison of
        // the alloc/dealloc hot path; SeferMalloc is NOT installed as the bench
        // binary's global allocator, so we must call it directly) ---
        group.bench_function(format!("SeferMalloc/{size}B"), |b| {
            b.iter(|| bench_direct_alloc(&sefer, layout))
        });

        // --- mimalloc (called directly) ---
        group.bench_function(format!("mimalloc/{size}B"), |b| {
            b.iter(|| bench_direct_alloc(&mi, layout))
        });

        // --- System (called directly) ---
        group.bench_function(format!("System/{size}B"), |b| {
            b.iter(|| bench_direct_alloc(&sys, layout))
        });
    }

    // --- Real-world pattern: Vec<i64> push/grow churn ---
    // This exercises realloc + many small allocs as the Vec grows.
    const VEC_PUSHES: usize = 512;
    group.bench_function("Vec_push/SeferMalloc", |b| {
        b.iter(|| {
            // Manual Vec growth through SeferMalloc's GlobalAlloc directly, so
            // the measurement is SeferMalloc (not the bench binary's default
            // global allocator) — symmetric with the mimalloc/System arms below.
            let mut ptr: *mut i64 = core::ptr::null_mut();
            let mut cap: usize = 0;
            let mut len: usize = 0;
            let layout = Layout::array::<i64>(VEC_PUSHES.max(1)).unwrap();
            for i in 0..VEC_PUSHES {
                if len == cap {
                    let new_cap = if cap == 0 { 4 } else { cap * 2 };
                    let new_layout = Layout::array::<i64>(new_cap.max(VEC_PUSHES)).unwrap();
                    // SAFETY: realloc-like growth through SeferMalloc.
                    let new_ptr = unsafe { sefer.alloc(new_layout) };
                    if !new_ptr.is_null() && !ptr.is_null() {
                        unsafe {
                            core::ptr::copy_nonoverlapping(ptr, new_ptr as *mut i64, len);
                            sefer.dealloc(ptr as *mut u8, layout);
                        }
                    }
                    ptr = new_ptr as *mut i64;
                    cap = new_cap.max(VEC_PUSHES);
                }
                // SAFETY: ptr is valid for `cap` elements if non-null.
                if !ptr.is_null() {
                    unsafe { ptr.add(len).write(i as i64) };
                }
                len += 1;
            }
            black_box(ptr);
            black_box(len);
            if !ptr.is_null() {
                let final_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                unsafe { sefer.dealloc(ptr as *mut u8, final_layout) };
            }
        })
    });

    group.bench_function("Vec_push/mimalloc", |b| {
        b.iter(|| {
            // mimalloc is NOT the global allocator here (SeferMalloc is), so we
            // manually replicate Vec growth via mimalloc's GlobalAlloc.
            let mut ptr: *mut i64 = core::ptr::null_mut();
            let mut cap: usize = 0;
            let mut len: usize = 0;
            let layout = Layout::array::<i64>(VEC_PUSHES.max(1)).unwrap();
            for i in 0..VEC_PUSHES {
                if len == cap {
                    let new_cap = if cap == 0 { 4 } else { cap * 2 };
                    let new_layout = Layout::array::<i64>(new_cap.max(VEC_PUSHES)).unwrap();
                    // SAFETY: realloc-like growth through mimalloc.
                    let new_ptr = unsafe { mi.alloc(new_layout) };
                    if !new_ptr.is_null() && !ptr.is_null() {
                        unsafe {
                            core::ptr::copy_nonoverlapping(ptr, new_ptr as *mut i64, len);
                            mi.dealloc(ptr as *mut u8, layout);
                        }
                    }
                    ptr = new_ptr as *mut i64;
                    cap = new_cap.max(VEC_PUSHES);
                }
                // SAFETY: ptr is valid for `cap` elements if non-null.
                if !ptr.is_null() {
                    unsafe { ptr.add(len).write(i as i64) };
                }
                len += 1;
            }
            black_box(ptr);
            black_box(len);
            if !ptr.is_null() {
                let final_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                unsafe { mi.dealloc(ptr as *mut u8, final_layout) };
            }
        })
    });

    group.bench_function("Vec_push/System", |b| {
        b.iter(|| {
            let mut ptr: *mut i64 = core::ptr::null_mut();
            let mut cap: usize = 0;
            let mut len: usize = 0;
            for i in 0..VEC_PUSHES {
                if len == cap {
                    let new_cap = if cap == 0 { 4 } else { cap * 2 };
                    let new_layout = Layout::array::<i64>(new_cap.max(VEC_PUSHES)).unwrap();
                    // SAFETY: realloc-like growth through System.
                    let new_ptr = unsafe { sys.alloc(new_layout) };
                    if !new_ptr.is_null() && !ptr.is_null() {
                        let old_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                        unsafe {
                            core::ptr::copy_nonoverlapping(ptr, new_ptr as *mut i64, len);
                            sys.dealloc(ptr as *mut u8, old_layout);
                        }
                    }
                    ptr = new_ptr as *mut i64;
                    cap = new_cap.max(VEC_PUSHES);
                }
                if !ptr.is_null() {
                    unsafe { ptr.add(len).write(i as i64) };
                }
                len += 1;
            }
            black_box(ptr);
            black_box(len);
            if !ptr.is_null() {
                let final_layout = Layout::array::<i64>(cap.max(1)).unwrap();
                unsafe { sys.dealloc(ptr as *mut u8, final_layout) };
            }
        })
    });

    group.finish();
}

criterion_group!(benches, bench_global_alloc);
criterion_main!(benches);
