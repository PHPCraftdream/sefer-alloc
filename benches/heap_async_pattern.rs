//! Synthetic async-like allocation pattern (task #62) — low-noise profile
//! target.
//!
//! Mimics a СУБД-pipeline workload — mixed-size alloc/dealloc/realloc-grow —
//! WITHOUT tokio runtime or scheduler noise, WITHOUT channels, WITHOUT spawns.
//! The "two roles" (producer allocates, consumer releases) are simulated in a
//! single-threaded loop that switches between accumulating allocations and
//! draining them, producing a realistic temporal pattern of live-set growth and
//! free-list reuse.
//!
//! Allocation mix:
//!   - Small vecs (`Vec<u8>` 64–1024 B): simulate row buffers.
//!   - Medium vecs (`Vec<u8>` 2–16 KiB): simulate query result sets.
//!   - Realloc-grow (`Vec<u8>` starting at 64 B, growing by 1.5×): simulate
//!     dynamic accumulators (sort buffers, network receive buffers).
//!   - Boxes (`Box<[u8; 64]>`): simulate per-task metadata nodes.
//!   - HashMaps (`HashMap<u64, u64>`): simulate per-row lookup tables; each
//!     insertion may trigger a realloc.
//!
//! The bench calls `GlobalAlloc` methods DIRECTLY on `SeferMalloc` (no
//! `#[global_allocator]` install) so the timing captures only SeferMalloc's
//! hot path, not the system allocator that backs `criterion`, `Vec`, etc.
//! This mirrors `global_alloc.rs` and `large_realloc.rs`.
//!
//! Gated on `alloc-global` (which brings `alloc-core` + `alloc`).

#![cfg(feature = "alloc-global")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::SeferMalloc;

// ── helpers ──────────────────────────────────────────────────────────────────

/// Allocate `size` bytes through `a`, write a byte pattern to every page
/// boundary to touch the pages (non-vacuous), return the pointer.
#[inline]
fn alloc_touch<A: GlobalAlloc>(a: &A, size: usize) -> *mut u8 {
    let layout = Layout::from_size_align(size, 8).unwrap_or_else(|_| {
        Layout::from_size_align(8, 8).unwrap()
    });
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { a.alloc(layout) };
    if !ptr.is_null() {
        // Touch first and last byte so the OS has to map the pages (realistic).
        // SAFETY: ptr is valid for `size` bytes as returned by `a.alloc`.
        unsafe {
            ptr.write(0xAB);
            ptr.add(size - 1).write(0xCD);
        }
    }
    ptr
}

/// Free a block previously allocated by `alloc_touch`.
#[inline]
fn free_touched<A: GlobalAlloc>(a: &A, ptr: *mut u8, size: usize) {
    if ptr.is_null() {
        return;
    }
    let layout = Layout::from_size_align(size, 8).unwrap_or_else(|_| {
        Layout::from_size_align(8, 8).unwrap()
    });
    // SAFETY: ptr was returned by the same `a` with the same `layout`.
    unsafe { a.dealloc(ptr, layout) };
}

/// Grow a block from `old_size` to `new_size` via `realloc`.
#[inline]
fn grow<A: GlobalAlloc>(a: &A, ptr: *mut u8, old_size: usize, new_size: usize) -> *mut u8 {
    if ptr.is_null() || old_size == 0 {
        return alloc_touch(a, new_size);
    }
    let old_layout = Layout::from_size_align(old_size, 8).unwrap_or_else(|_| {
        Layout::from_size_align(8, 8).unwrap()
    });
    // SAFETY: ptr was returned by `a` with `old_layout`; `new_size` is non-zero.
    let new_ptr = unsafe { a.realloc(ptr, old_layout, new_size) };
    if new_ptr.is_null() {
        // OOM: free the old block and return null.
        // SAFETY: ptr is still valid (realloc did not free on OOM).
        unsafe { a.dealloc(ptr, old_layout) };
    }
    new_ptr
}

// ── workload phases ───────────────────────────────────────────────────────────

/// Phase A — "producer" accumulates allocations: simulate a pipeline stage
/// filling a batch of row buffers. Returns (ptrs, sizes) for the live set.
///
/// Sizes: 8 × [64, 128, 256, 512, 1024] = 40 small blocks.
const SMALL_SIZES: &[usize] = &[64, 128, 256, 512, 1024];
const SMALL_REPS: usize = 8;

/// Phase B — realloc-grow accumulator: 64 B → ~8 KiB via ×1.5 steps.
const GROW_START: usize = 64;
const GROW_STEPS: usize = 10; // 64 × 1.5^10 ≈ 3.7 KiB

/// Phase C — medium: 4 × [2048, 4096, 8192, 16384].
const MEDIUM_SIZES: &[usize] = &[2048, 4096, 8192, 16_384];
const MEDIUM_REPS: usize = 4;

/// Full pipeline iteration: allocate a mixed live set, mutate it (grow some),
/// then free everything. The loop is the timing target.
fn pipeline_iteration<A: GlobalAlloc>(a: &A) {
    // ── Phase A: small row buffers ─────────────────────────────────────────
    let total_small = SMALL_SIZES.len() * SMALL_REPS;
    let mut small_ptrs: [*mut u8; 40] = [core::ptr::null_mut(); 40]; // 5×8
    let mut small_sizes: [usize; 40] = [0usize; 40];
    let mut si = 0usize;
    for rep in 0..SMALL_REPS {
        for &sz in SMALL_SIZES {
            let p = alloc_touch(a, sz);
            if si < total_small {
                small_ptrs[si] = p;
                small_sizes[si] = sz;
                si += 1;
            }
            black_box(rep);
        }
    }

    // ── Phase B: realloc-grow accumulator ─────────────────────────────────
    let mut grow_size = GROW_START;
    let mut grow_ptr: *mut u8 = alloc_touch(a, grow_size);
    for _ in 0..GROW_STEPS {
        let new_size = (grow_size * 3 / 2).max(grow_size + 8);
        grow_ptr = grow(a, grow_ptr, grow_size, new_size);
        grow_size = new_size;
    }
    black_box(grow_ptr);

    // ── Phase C: medium buffers ────────────────────────────────────────────
    let total_medium = MEDIUM_SIZES.len() * MEDIUM_REPS;
    let mut med_ptrs: [*mut u8; 16] = [core::ptr::null_mut(); 16]; // 4×4
    let mut med_sizes: [usize; 16] = [0usize; 16];
    let mut mi = 0usize;
    for rep in 0..MEDIUM_REPS {
        for &sz in MEDIUM_SIZES {
            let p = alloc_touch(a, sz);
            if mi < total_medium {
                med_ptrs[mi] = p;
                med_sizes[mi] = sz;
                mi += 1;
            }
            black_box(rep);
        }
    }

    // ── Drain: consumer releases everything ───────────────────────────────
    for i in 0..si {
        free_touched(a, small_ptrs[i], small_sizes[i]);
    }
    if !grow_ptr.is_null() {
        free_touched(a, grow_ptr, grow_size);
    }
    for i in 0..mi {
        free_touched(a, med_ptrs[i], med_sizes[i]);
    }
}

// ── benchmark ─────────────────────────────────────────────────────────────────

fn bench_async_pattern(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_async_pattern");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    let sefer = SeferMalloc::new();

    // Warm up: one pipeline pass to seed free lists before the timing loop.
    pipeline_iteration(&sefer);

    group.bench_function("SeferMalloc/pipeline", |b| {
        b.iter(|| pipeline_iteration(&sefer))
    });

    group.finish();
}

criterion_group!(benches, bench_async_pattern);
criterion_main!(benches);
