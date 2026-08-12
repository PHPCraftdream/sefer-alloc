//! Task #849 (V20/P17, `docs/reviews/2026-08-12-aligned-vmem-code-quality-review.md`):
//! measure `try_reserve_aligned_exact`'s Unix hit rate per align regime,
//! using the `bench-internals` counters that already exist for this purpose
//! (`unix_exact_reserve_attempts()` / `unix_exact_reserve_hits()`).
//!
//! Measurement-only: this does not change `unix_reserve`'s dispatch logic.
//!
//! Run (single process):
//! ```text
//! cargo run -p aligned-vmem --release --features bench-internals --example v20_849_unix_exact_reserve_hit_rate
//! ```
//!
//! For `align == size` (the fast path this measures), consecutive reservations
//! within ONE process are highly correlated with each other (Linux places
//! them at `base - k*size` under top-down `mmap`, so their alignment residue
//! mod `size` is identical for every `k`) but the winning residue itself is
//! set once per process by ASLR's `mmap_base` draw. A single process run is
//! one Bernoulli trial, not `ITERS` independent trials — repeat the whole
//! process N times (e.g. a shell loop) and aggregate hits/attempts across
//! runs to sample the ASLR distribution; see the task #849 commit message
//! for a 30-run aggregate on this project's dev host.

use aligned_vmem::{
    reserve_aligned, reset_bench_internals_counters, unix_exact_reserve_attempts,
    unix_exact_reserve_hits,
};

const ITERS: usize = 16;

/// Batch-allocate all `ITERS` reservations BEFORE releasing any of them,
/// rather than an alloc-then-immediately-free loop.
///
/// An alloc/free/alloc/free loop of the SAME size, with nothing else
/// allocating in between, has the kernel hand back the just-freed VA gap on
/// the very next `mmap` call almost every time — so the whole loop measures
/// one address's alignment class, repeated, not a representative sample.
/// Confirmed empirically: that shape produced a 100.0000% hit rate steady
/// state that did not match the review's hand-derived prediction of a
/// syscall-losing regime for large aligns. Holding all reservations live
/// (so each `mmap` call sees a genuinely different, still-growing free
/// region) is the realistic-regime fix, per this project's own "measure
/// under the real workload regime, not an artifact of the harness" rule.
fn measure(label: &str, size: usize, align: usize) {
    reset_bench_internals_counters();
    let mut held = Vec::with_capacity(ITERS);
    for _ in 0..ITERS {
        held.push(reserve_aligned(size, align));
    }
    let attempts = unix_exact_reserve_attempts();
    let hits = unix_exact_reserve_hits();
    drop(held); // Release all reservations after the measurement window closes.
    let pct = if attempts == 0 {
        f64::NAN
    } else {
        100.0 * hits as f64 / attempts as f64
    };
    println!("{label}: size={size} align={align} hits={hits}/{attempts} ({pct:.4}%)");
}

fn main() {
    // Existing bench arms' regimes (benches/vmem_bench.rs), plus the new
    // 4 MiB regime V20/P17 flags as the crate's own flagship
    // SEGMENT-aligned-SEGMENT use case (e.g. sefer-alloc's own segments).
    measure("page-size (4 KiB)", 4 * 1024, 4 * 1024);
    measure("existing default arm (64 KiB)", 64 * 1024, 64 * 1024);
    measure("existing large arm (1 MiB)", 1024 * 1024, 1024 * 1024);
    measure(
        "V20/P17 flagship regime (4 MiB)",
        4 * 1024 * 1024,
        4 * 1024 * 1024,
    );
}
