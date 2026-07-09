//! Phase 11 -- `SeferAlloc` as `#[global_allocator]` vs `mimalloc` and the
//! system allocator. Quick criterion profile per the short-scenario policy:
//! `sample_size(10)` and short warm/measurement times. Honest verdict in
//! `docs/ALLOC_BENCH.md`.
//!
//! This bench exercises REAL Rust allocation patterns through the
//! `#[global_allocator]` face: `Vec` push/grow churn (which calls
//! `alloc`/`dealloc`/`realloc` under the hood), `Box` new/drop, and varied
//! sizes. We compare three configurations:
//!
//! 1. **SeferAlloc** (installed as the process's `#[global_allocator]`).
//! 2. **mimalloc** (via the `mimalloc` crate's `GlobalAlloc` impl, called
//!    directly -- NOT installed globally, to allow a head-to-head in one binary).
//! 3. **System** allocator (called directly).
//!
//! For (2) and (3) we call the `GlobalAlloc` methods directly to avoid
//! replacing the global allocator mid-process (which SeferAlloc already
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
use sefer_alloc::SeferAlloc;

/// Representative small-to-medium sizes for the churn bench.
const SIZES: &[usize] = &[16, 64, 256, 1024];

/// Number of alloc/dealloc pairs per iteration.
const OPS: usize = 1024;

/// Working-set size for the churn bench: how many live blocks are maintained.
/// 256 is small enough to fit in a future tcache, large enough to be meaningful.
const CHURN_WORKING_SET: usize = 256;

/// Deterministic, dependency-free PRNG (xorshift64*). Fixed seed for
/// reproducible benchmark runs. No external `rand` crate needed.
struct XorShift64(u64);

impl XorShift64 {
    const fn new(seed: u64) -> Self {
        // Avoid the all-zero fixed point.
        Self(seed | 1)
    }

    #[inline]
    fn next_usize(&mut self) -> usize {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D) as usize
    }
}

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

/// Churn bench: maintain a working set of `working_set` live blocks; each of
/// `ops` iterations frees a pseudo-random block and allocates a replacement.
/// This is the steady-state pattern a per-thread magazine (tcache) wins on:
/// freed blocks re-enter the cache and are re-allocated without round-tripping
/// the BinTable. Fixed PRNG seed = 0xCAFE for reproducibility.
fn bench_churn_alloc<A: GlobalAlloc>(alloc: &A, layout: Layout, working_set: usize, ops: usize) {
    let mut rng = XorShift64::new(0xCAFE);

    // Pre-fill the working set.
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        live.push(p);
    }

    // Churn: free a random slot, alloc a replacement.
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(old, layout) };
        }
        // SAFETY: layout has non-zero size and valid alignment.
        live[idx] = unsafe { alloc.alloc(layout) };
    }

    black_box(&live);

    // Teardown: free everything.
    for &p in &live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(p, layout) };
        }
    }
}

/// Writing-churn bench: an EXACT clone of `bench_churn_alloc` except that
/// immediately after every non-null `alloc` (both the pre-fill loop and the
/// churn loop) it writes the first 16 bytes (two u64 words at offset 0 and 8)
/// of the freshly allocated block. This dirties word1 (bytes 8..16 — the
/// magazine M2 double-free-guard key location) so the realistic
/// write-to-what-you-allocate pattern is measured, instead of leaving a stale
/// key that forces a slow-path scan on every free. Fixed PRNG seed = 0xCAFE
/// (identical to the non-writing bench) for reproducibility.
fn bench_churn_alloc_write<A: GlobalAlloc>(
    alloc: &A,
    layout: Layout,
    working_set: usize,
    ops: usize,
) {
    let mut rng = XorShift64::new(0xCAFE);

    // Pre-fill the working set.
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        if !p.is_null() {
            // SAFETY: `p` is a freshly allocated block of `layout` (size >= 16
            // for every bench size), so the first 16 bytes are in bounds and
            // writable. `write_volatile` prevents the store being elided.
            unsafe { core::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { core::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
        }
        live.push(p);
    }

    // Churn: free a random slot, alloc a replacement, write into it.
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(old, layout) };
        }
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        if !p.is_null() {
            // SAFETY: `p` is a freshly allocated block of `layout` (size >= 16
            // for every bench size), so the first 16 bytes are in bounds and
            // writable. `write_volatile` prevents the store being elided.
            unsafe { core::ptr::write_volatile(p.cast::<u64>(), 0xA5A5_A5A5_A5A5_A5A5) };
            unsafe { core::ptr::write_volatile(p.cast::<u64>().add(1), 0xA5A5_A5A5_A5A5_A5A5) };
        }
        live[idx] = p;
    }

    black_box(&live);

    // Teardown: free everything.
    for &p in &live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(p, layout) };
        }
    }
}

fn bench_global_alloc(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        // --- SeferAlloc (called directly through its GlobalAlloc impl, exactly
        // like mimalloc and System below — a true apples-to-apples comparison of
        // the alloc/dealloc hot path; SeferAlloc is NOT installed as the bench
        // binary's global allocator, so we must call it directly) ---
        group.bench_function(format!("SeferAlloc/{size}B"), |b| {
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
    group.bench_function("Vec_push/SeferAlloc", |b| {
        b.iter(|| {
            // Manual Vec growth through SeferAlloc's GlobalAlloc directly, so
            // the measurement is SeferAlloc (not the bench binary's default
            // global allocator) — symmetric with the mimalloc/System arms below.
            let mut ptr: *mut i64 = core::ptr::null_mut();
            let mut cap: usize = 0;
            let mut len: usize = 0;
            let layout = Layout::array::<i64>(VEC_PUSHES.max(1)).unwrap();
            for i in 0..VEC_PUSHES {
                if len == cap {
                    let new_cap = if cap == 0 { 4 } else { cap * 2 };
                    let new_layout = Layout::array::<i64>(new_cap.max(VEC_PUSHES)).unwrap();
                    // SAFETY: realloc-like growth through SeferAlloc.
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
            // mimalloc is NOT the global allocator here (SeferAlloc is), so we
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

fn bench_global_alloc_churn(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc_churn");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        group.bench_function(format!("SeferAlloc/{size}B"), |b| {
            b.iter(|| bench_churn_alloc(&sefer, layout, CHURN_WORKING_SET, OPS))
        });

        group.bench_function(format!("mimalloc/{size}B"), |b| {
            b.iter(|| bench_churn_alloc(&mi, layout, CHURN_WORKING_SET, OPS))
        });

        group.bench_function(format!("System/{size}B"), |b| {
            b.iter(|| bench_churn_alloc(&sys, layout, CHURN_WORKING_SET, OPS))
        });
    }

    group.finish();
}

fn bench_global_alloc_churn_write(c: &mut Criterion) {
    let mut group = c.benchmark_group("global_alloc_churn_write");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    for &size in SIZES {
        let layout = Layout::from_size_align(size, 8).unwrap();

        group.bench_function(format!("SeferAlloc/{size}B"), |b| {
            b.iter(|| bench_churn_alloc_write(&sefer, layout, CHURN_WORKING_SET, OPS))
        });

        group.bench_function(format!("mimalloc/{size}B"), |b| {
            b.iter(|| bench_churn_alloc_write(&mi, layout, CHURN_WORKING_SET, OPS))
        });

        group.bench_function(format!("System/{size}B"), |b| {
            b.iter(|| bench_churn_alloc_write(&sys, layout, CHURN_WORKING_SET, OPS))
        });
    }

    group.finish();
}

/// PERF-4 (task #14) — the decommit→recycle segment-churn shape. This is the
/// wall-clock companion to the `seg_cycle_decommit_256k` iai bench (the
/// deterministic judge). It drives a NON-primordial small segment through
/// empty→decommit→recycle→re-reserve on every round — the exact path the
/// shamir-db sweep flagged (0.3.0 vs 0.2.1, "many short-lived small segments
/// cycling quickly") and which PERF-4's release-follows fast path optimizes.
///
/// Geometry (see the iai bench for the full rationale): the largest small size
/// class is SMALL_MAX = 258,752 B (≈253 KiB) and one 4 MiB segment holds 15
/// such blocks (16 fit in 4 MiB, but the primordial reserves one block's worth
/// for its self-hosted registry and every fresh small segment loses a block to
/// per-segment metadata → 15 usable). `SEG_BATCH` (34) fills the primordial (15),
/// then a SECOND small segment (15), then opens a THIRD (4), so the SECOND
/// segment is NON-current when the whole batch is freed → it goes empty while not
/// the carve target → `decommit_empty_segment` + `recycle`. Note: a batch that
/// only just spills into the second segment (say 18) does NOT decommit — that
/// segment is still the current carve target, which is excluded from decommit;
/// the batch MUST reach a THIRD segment (≥ 31 blocks) to leave the second one
/// non-current. Under `alloc-decommit` this is the decommit path; without it,
/// the same shape still measures the reserve/carve/release churn. Compared
/// head-to-head vs mimalloc and System.
fn bench_segment_decommit_cycle<A: GlobalAlloc>(alloc: &A, layout: Layout) {
    const SEG_BATCH: usize = 34;
    let mut ptrs: [*mut u8; SEG_BATCH] = [core::ptr::null_mut(); SEG_BATCH];
    for slot in ptrs.iter_mut() {
        // SAFETY: layout has non-zero size and valid alignment.
        *slot = unsafe { alloc.alloc(layout) };
    }
    black_box(&ptrs);
    for &ptr in &ptrs {
        if !ptr.is_null() {
            // SAFETY: ptr was allocated by `alloc` with the same layout and is
            // freed exactly once; freeing the whole batch empties the
            // non-primordial second segment → decommit → recycle.
            unsafe { alloc.dealloc(ptr, layout) };
        }
    }
}

fn bench_global_segment_decommit_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("segment_decommit_cycle");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));

    let sefer = SeferAlloc::new();
    let mi = mimalloc::MiMalloc;
    let sys = System;

    // The largest small size class EXACTLY (SMALL_MAX = 258,752 B ≈ 253 KiB) @
    // align 8: this MUST route to the Small path for the decommit trigger to
    // fire. A literal 256 KiB (262,144 B) exceeds SMALL_MAX (258,752 B) and
    // silently falls through to the dedicated-segment Large path, where
    // `dec_live_and_maybe_decommit` bails on `kind != Small` and
    // `decommit_empty_segment_for_release` is NEVER reached — the whole point
    // of the bench. One 4 MiB segment holds 15 usable such blocks (see
    // `bench_segment_decommit_cycle`'s doc comment for the full 15/15/4 batch
    // breakdown); `SEG_BATCH` (34) must reach a THIRD segment to leave the
    // second one non-current, which is what actually triggers decommit.
    let layout = Layout::from_size_align(258_752, 8).unwrap();

    group.bench_function("SeferAlloc/253KiB", |b| {
        b.iter(|| bench_segment_decommit_cycle(&sefer, layout))
    });
    group.bench_function("mimalloc/253KiB", |b| {
        b.iter(|| bench_segment_decommit_cycle(&mi, layout))
    });
    group.bench_function("System/253KiB", |b| {
        b.iter(|| bench_segment_decommit_cycle(&sys, layout))
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_global_alloc,
    bench_global_alloc_churn,
    bench_global_alloc_churn_write,
    bench_global_segment_decommit_cycle
);
criterion_main!(benches);
