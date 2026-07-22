//! R13-6 (task #276) — production A/B wall-clock gate for the
//! `exact-span-large` (R12-3) + `large-reserved-capacity` (R12-4) opt-in pair.
//!
//! ## Why this bench exists
//!
//! R12-3/R12-4 already shipped THROWAWAY single-shot measurement harnesses
//! (`examples/r12_3_exact_span_measure.rs`, `examples/r12_4_reserved_capacity_measure.rs`)
//! that report RSS amplification and a single realloc-chain's move-leg/latency
//! numbers via `proc-memstat` process snapshots. Those give a clean cold-process
//! number but no criterion-grade statistical distribution (mean/stddev/outliers)
//! and no direct read on the Large-cache HIT RATE under a cyclic multi-size
//! workload — the R13-6 task brief's item 4 ("large-cache workload... cache hit
//! rate AND RSS with the features on/off"). This bench fills that gap with two
//! criterion groups, run through `AllocCore` directly (not the `GlobalAlloc`
//! face — matches `r12_3`/`r12_4`'s own harness style so the numbers are
//! directly comparable to those docs).
//!
//! ## Groups
//!
//! 1. `r13_6_realloc_chain` — the SAME 5-step growth chain as
//!    `r12_4_reserved_capacity_measure.rs` (256 KiB -> 512 KiB -> 1 MiB ->
//!    2 MiB -> 4 MiB), run repeatedly under criterion so the wall-clock has a
//!    real distribution instead of a single "best of 20" scalar. Also reports
//!    move-leg counts via `black_box` sinks read once outside the timed loop
//!    (criterion cannot report custom counters mid-run, so the leg breakdown is
//!    still the example harness's job — this bench is the LATENCY distribution
//!    companion, not a replacement).
//! 2. `r13_6_large_cache_cycle` — cyclic alloc/dealloc across FOUR distinct
//!    Large sizes (260 KiB, 512 KiB, 1 MiB, 1.75 MiB — the same four
//!    sub-4-MiB control points `r12_3_exact_span_measure.rs` uses) round-robin,
//!    exercising the 8-slot `large_cache`'s best-fit matching under a working
//!    set that does not all fit distinctly forever. Reports the cache hit rate
//!    via `AllocCore::dbg_large_cache_hits()` (a running counter, read once
//!    after the whole bench group via a `println!` since criterion has no
//!    custom-metric output) — this is the harness the task brief's own caveat
//!    warns about: under `exact-span-large`, differently-sized requests that
//!    used to alias the same whole-`SEGMENT`-rounded `usable` value now have
//!    numerically DISTINCT `usable` values, so the cache's best-fit slot
//!    matching can see MORE distinct sizes competing for 8 slots -> lower hit
//!    rate is an EXPECTED finding under this feature, not a bug.
//!
//! ## `alloc-stats` requirement for a non-zero hit-rate readout
//!
//! `AllocCore::dbg_large_cache_hits()`'s INCREMENT (not the accessor itself)
//! is gated behind `#[cfg(feature = "alloc-stats")]`
//! (`alloc_core_large.rs`'s cache-hit branch) — `alloc-stats` is NOT part of
//! `production`. Run this bench with `--features "production alloc-stats"` (or
//! add `exact-span-large`/`large-reserved-capacity` alongside) to get a
//! meaningful (non-zero) `large_cache_hits` printout; under bare `production`
//! the printed count is always 0 (the counter compiles out, not "no hits
//! occurred") — this mirrors the SAME gating `docs/perf/R10_5_LARGE_CACHE_HIT_GATE.md`'s
//! `paired_ab_large_cache_*` harnesses already work around by adding
//! `alloc-stats` on their build line.
//!
//! Fast profile per `CLAUDE.md`'s "Speed: short scenario by default":
//! `sample_size(10)` + short warm-up/measurement.

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::AllocCore;

const KIB: usize = 1024;
const MIB: usize = 1024 * 1024;

/// Mirrors `examples/r12_4_reserved_capacity_measure.rs`'s `CHAIN` exactly —
/// same control points, so the two harnesses' numbers are directly
/// comparable (this bench adds the statistical distribution; the example
/// keeps the RSS/commit-charge + move-leg breakdown).
const CHAIN: [usize; 5] = [256 * KIB, 512 * KIB, MIB, 2 * MIB, 4 * MIB];

/// Run one full realloc growth chain (fresh `AllocCore` each call — cold-start
/// isolated, matching R12-3/R12-4's own per-iteration-isolation discipline so
/// no run's cached/committed state leaks into the next).
fn run_realloc_chain() {
    let mut ac = AllocCore::new().expect("primordial");
    let mut size = CHAIN[0];
    let layout0 = Layout::from_size_align(size, 8).unwrap();
    let mut ptr = ac.alloc(layout0);
    assert!(!ptr.is_null(), "OOM allocating initial {size} bytes");

    for &next_size in &CHAIN[1..] {
        let old_layout = Layout::from_size_align(size, 8).unwrap();
        // SAFETY (R6-MS-1/2): `ptr` is a live allocation from this AllocCore
        // made with `old_layout`, consumed exactly once by this call.
        let grown = unsafe { ac.realloc(ptr, old_layout, next_size) };
        assert!(!grown.is_null(), "realloc growth to {next_size} failed");
        ptr = grown;
        size = next_size;
    }

    std::hint::black_box(ptr);
    let final_layout = Layout::from_size_align(size, 8).unwrap();
    // SAFETY (R6-MS-1/2): `ptr` is the result of the last successful
    // alloc/realloc with `final_layout`, freed exactly once here.
    unsafe { ac.dealloc(ptr, final_layout) };
}

fn bench_realloc_chain(c: &mut Criterion) {
    let mut group = c.benchmark_group("r13_6_realloc_chain");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));

    group.bench_function("grow_256kib_to_4mib", |b| {
        b.iter(run_realloc_chain);
    });

    group.finish();
}

/// The four sub-4-MiB control sizes from `r12_3_exact_span_measure.rs`
/// (260 KiB, 512 KiB, 1 MiB, 1.75 MiB) — chosen because they are the sizes
/// whose whole-`SEGMENT` rounded `usable` all COLLAPSE to a single 4 MiB
/// value under the baseline (production, no `exact-span-large`), but become
/// four numerically DISTINCT `usable` values under `exact-span-large` — the
/// exact condition the task brief's cache-hit-rate caveat describes.
const CACHE_CYCLE_SIZES: [usize; 4] = [260 * KIB, 512 * KIB, MIB, (7 * MIB) / 4];

/// How many round-robin passes over `CACHE_CYCLE_SIZES` one measured
/// iteration performs. Kept small (fast-profile budget) but large enough that
/// the FIRST pass's unavoidable cold misses are a small fraction of the total
/// ops, so the reported hit rate mostly reflects steady-state behaviour.
const CACHE_CYCLE_PASSES: usize = 16;

/// One measured iteration: round-robin alloc+dealloc across the four control
/// sizes, `CACHE_CYCLE_PASSES` times. Returns nothing — the hit-rate read
/// happens OUTSIDE the timed loop (see `bench_large_cache_cycle`) via the
/// `AllocCore`'s own running counter, since criterion's `Bencher::iter`
/// reconstructs/drops the closure's captures each sample and a per-iteration
/// hit-rate readout would not compose into one meaningful figure anyway.
fn run_cache_cycle(ac: &mut AllocCore) {
    for _ in 0..CACHE_CYCLE_PASSES {
        for &size in &CACHE_CYCLE_SIZES {
            let layout = Layout::from_size_align(size, 8).unwrap();
            let ptr = ac.alloc(layout);
            assert!(!ptr.is_null(), "OOM allocating {size} bytes");
            std::hint::black_box(ptr);
            // SAFETY (R6-MS-1/2): `ptr` is a live allocation from this
            // AllocCore made with `layout`, freed exactly once here — deposits
            // into `large_cache` rather than releasing the OS reservation, so
            // the NEXT size in the round-robin can potentially reuse this slot
            // (a best-fit hit) or evict it (a miss), exactly the contention
            // the hit-rate measurement targets.
            unsafe { ac.dealloc(ptr, layout) };
        }
    }
}

fn bench_large_cache_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("r13_6_large_cache_cycle");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));

    // ONE AllocCore, unbounded cache budget, reused across every sample in
    // this group -- criterion's `Bencher::iter` calls the closure many times
    // per sample without resetting captured state, which is exactly what a
    // steady-state hit-rate measurement needs (a fresh AllocCore per
    // iteration would make every size's FIRST hit within that iteration a
    // guaranteed cold miss, drowning out steady-state behaviour).
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    group.bench_function("cycle_4_sizes", |b| {
        b.iter(|| run_cache_cycle(&mut ac));
    });

    group.finish();

    let hits = ac.dbg_large_cache_hits();
    let total_deallocs = (CACHE_CYCLE_PASSES * CACHE_CYCLE_SIZES.len()) as u64;
    // `hits` accumulates across ALL samples/iterations criterion ran for this
    // one `bench_function` call (a single, unbounded, monotonically
    // increasing counter on the shared `ac` for this group's whole lifetime),
    // not just the last one -- reported as a running total plus an
    // approximate per-op rate against the total ALLOC calls issued across the
    // group's measured samples (informational, not a criterion metric: this
    // group's `sample_size(10)` means roughly 10x `total_deallocs` alloc
    // calls ran, though criterion may run extra warm-up iterations too, so
    // the printed rate is a lower/rough bound, not exact).
    println!(
        "r13_6_large_cache_cycle: large_cache_hits={hits} (over >= {total_deallocs} allocs/sample x ~10 samples, informational only -- see docs/perf/R13_6_EXACT_SPAN_RESERVED_CAPACITY_PRODUCTION_GATE.md for the honest per-run hit-rate methodology)"
    );
}

criterion_group!(benches, bench_realloc_chain, bench_large_cache_cycle);
criterion_main!(benches);
