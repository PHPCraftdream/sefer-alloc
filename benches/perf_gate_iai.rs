//! Task #127 — CI perf-gate: instruction-count regression guard.
//!
//! Why `iai-callgrind` and not the existing `criterion` benches: criterion
//! measures wall-clock time, which is noisy on shared GitHub Actions
//! runners (neighbour VMs, thermal throttling, scheduler jitter). A ±15-20%
//! threshold would be needed to avoid false positives on wall-clock, which
//! is wide enough to have missed the exact regression class this gate exists
//! to catch: the task #114 const-builder change that cost 22-31% on
//! `db_handler`-shaped workloads (per-call align/size dispatch, not gross
//! algorithmic change). `iai-callgrind` instead counts CPU *instructions*
//! retired under Valgrind/Callgrind emulation, which is deterministic
//! run-to-run on the same binary+input regardless of host contention — a
//! tight (~5-10%) threshold is viable without flaking.
//!
//! Scope: four microbenchmarks chosen to cover the hot paths touched by
//! recent fixes/regressions:
//!
//! - `small_churn_16b` — alloc+dealloc of the smallest size class (magazine/
//!   tcache fast path).
//! - `aligned_churn_640b_a128` — 640 B @ align(128): the tokio-shaped
//!   over-alignment case central to the #114 regression (align>16 no longer
//!   burns a 4 MiB segment per allocation).
//! - `large_alloc_free_cycle` — 4 MiB single-shot alloc+free: the
//!   dedicated-segment / OS-round-trip path (D1 large_cache territory).
//! - `realloc_grow` — geometric realloc growth 64 B → 4 MiB (16 doublings):
//!   the C2 realloc-grow path.
//!
//! Platform note: `iai-callgrind` benchmarks require Valgrind to actually
//! *run* (they compile a normal binary, then iai-callgrind's runner drives
//! it under `valgrind --tool=callgrind`). Valgrind is Linux-only, and the
//! `iai-callgrind` dev-dependency itself is scoped to
//! `[target.'cfg(target_os = "linux")'.dev-dependencies]` in Cargo.toml. All
//! items below (imports, benchmark functions, the `main!` invocation) are
//! `#[cfg(target_os = "linux")]`-gated except for the non-Linux `fn main`
//! fallback: Cargo still needs a `main` for this `harness = false` bench
//! target to link on every platform it resolves the target for
//! (Windows/macOS included), so the fallback compiles everywhere while the
//! real Callgrind body only exists — and only ever runs — on Linux CI.
//!
//! First-run / enforcing behavior (task #128): the perf-gate workflow now
//! PERSISTS a `main` baseline across runs (via `actions/cache`) and, on a
//! labelled PR, compares against it with `--baseline=main` plus an
//! `IAI_CALLGRIND_REGRESSION='Ir=10'` limit — so a >10% instruction-count
//! regression FAILS the (non-blocking) job. The first main-branch run merely
//! records the baseline (nothing to regress against yet). The exact numbers,
//! and that the limit actually trips, are only observable on real Linux CI
//! hardware (Valgrind is Linux-only); the threshold may be tuned once those
//! first numbers are in.

#![allow(clippy::missing_safety_doc)]

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
use std::alloc::{GlobalAlloc, Layout};
#[cfg(target_os = "linux")]
use std::hint::black_box;

#[cfg(target_os = "linux")]
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
#[cfg(target_os = "linux")]
use sefer_alloc::SeferAlloc;

/// Number of alloc/dealloc pairs per churn iteration. Kept small relative to
/// the criterion benches (which use 1024) — callgrind emulation is far
/// slower than native execution; the instruction *count* is what we compare,
/// not wall-clock, so a smaller fixed op-count is enough to get a stable
/// signal without inflating CI job time.
#[cfg(target_os = "linux")]
const CHURN_OPS: usize = 64;

/// Small-block (16 B) alloc+dealloc churn — the magazine/tcache fast path
/// exercised by every allocator-heavy workload (db_handler-shaped included).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn small_churn_16b() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(16, 8).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

/// 640 B @ align(128) alloc+dealloc churn — the tokio-shaped over-alignment
/// case at the center of the task #114 regression (align>16 previously
/// burned a full 4 MiB segment per allocation instead of routing through
/// the size-class path).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn aligned_churn_640b_a128() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(640, 128).unwrap();
    for _ in 0..CHURN_OPS {
        // SAFETY: layout has non-zero size and valid (power-of-two) alignment.
        let ptr = unsafe { sefer.alloc(layout) };
        black_box(ptr);
        if !ptr.is_null() {
            // SAFETY: ptr was returned by the immediately preceding `alloc`
            // call with the same layout.
            unsafe { sefer.dealloc(ptr, layout) };
        }
    }
}

/// Single-shot 4 MiB alloc+free — the dedicated-segment / OS-round-trip path
/// (D1 large_cache territory: `mmap`/`VirtualAlloc` cost per large block).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn large_alloc_free_cycle() {
    let sefer = SeferAlloc::new();
    let layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { sefer.alloc(layout) };
    black_box(ptr);
    if !ptr.is_null() {
        // SAFETY: ptr was returned by the `alloc` call directly above with
        // the same layout.
        unsafe { sefer.dealloc(ptr, layout) };
    }
}

/// Geometric realloc growth: 64 B doubled 16x up to 4 MiB via
/// `GlobalAlloc::realloc` (the C2 realloc-grow path; no `Vec` amortization
/// hiding the per-step cost).
#[cfg(target_os = "linux")]
#[library_benchmark]
fn realloc_grow() {
    let sefer = SeferAlloc::new();
    let align = 8_usize;
    let start = 64_usize;
    let doublings = 16_u32;

    let init_layout = Layout::from_size_align(start, align).unwrap();
    // SAFETY: init_layout has non-zero size and valid alignment.
    let mut ptr = unsafe { sefer.alloc(init_layout) };
    if ptr.is_null() {
        return;
    }
    let mut current_size = start;

    for _ in 0..doublings {
        let new_size = current_size * 2;
        let old_layout = Layout::from_size_align(current_size, align).unwrap();
        // SAFETY: ptr was returned by a prior alloc/realloc call with
        // `old_layout`; `new_size` is non-zero.
        let new_ptr = unsafe { sefer.realloc(ptr, old_layout, new_size) };
        if new_ptr.is_null() {
            // SAFETY: ptr is still valid for `old_layout` (realloc did not
            // free on OOM).
            unsafe { sefer.dealloc(ptr, old_layout) };
            return;
        }
        ptr = new_ptr;
        current_size = new_size;
    }

    black_box(ptr);
    let final_layout = Layout::from_size_align(current_size, align).unwrap();
    // SAFETY: ptr is the result of the last successful alloc/realloc call
    // with `final_layout`.
    unsafe { sefer.dealloc(ptr, final_layout) };
}

#[cfg(target_os = "linux")]
library_benchmark_group!(
    name = perf_gate;
    benchmarks =
        small_churn_16b,
        aligned_churn_640b_a128,
        large_alloc_free_cycle,
        realloc_grow,
);

#[cfg(target_os = "linux")]
main!(library_benchmark_groups = perf_gate);
