//! R11-5 microbenchmark: per-call cost of `numa::current_node()` (real OS
//! syscall / Win32 API call) vs the new cached accessor
//! `AllocCore::current_node_cached()` (one syscall on the first call per
//! claim, then `Option::unwrap`-and-return on every subsequent call).
//!
//! ## What this measures on THIS host (Windows, single-NUMA)
//!
//! On this Windows/single-NUMA development host the real
//! `current_node_impl()` cost is TWO Win32 API calls
//! (`GetCurrentProcessorNumberEx` + `GetNumaProcessorNodeEx`) — real kernel
//! transitions. The bench reports the per-call cost of those API calls
//! versus a cached read, and the ratio is the speedup the cache buys on
//! this host. See `docs/PHASE_NUMA_DESIGN.md` §4.1 for the measured
//! numbers and the full design note.
//!
//! ## What this does NOT measure (Linux sysfs loop)
//!
//! The more dramatic case is Linux, where `current_node_impl()` loops over
//! up to 64 NUMA nodes and for EACH ONE calls `node_contains_cpu` →
//! `read_cpumap_contains_cpu`, which opens, reads, and closes
//! `/sys/devices/system/node/nodeN/cpumap` — potentially dozens of
//! `open`/`read`/`close` syscalls per single `current_node()` call. That
//! path is NOT directly measurable on this Windows host; the cache's
//! benefit there is qualitatively larger than the Windows numbers below
//! suggest. We do not claim a Linux number we did not measure.
//!
//! ## Build / run
//!
//! ```text
//! cargo bench --features numa-aware --bench numa_current_node_cache
//! ```

#![cfg(feature = "numa-aware")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::{alloc_core::numa, AllocCore};

/// Per-call cost: real `numa::current_node()` — the function the cache
/// exists to defray. Every iteration pays the full syscall/API cost.
fn bench_uncached(b: &mut criterion::Bencher<'_>) {
    b.iter(|| {
        let n = numa::current_node();
        black_box(n)
    });
}

/// Per-call cost: the cached accessor on a pre-populated `AllocCore` —
/// every iteration is a cache hit (load `Some(n)`, return `n`). The first
/// call (which actually queries the OS and populates the cache) runs ONCE
/// outside the timed loop, so the timed region measures purely the
/// cache-hit cost the hot path pays under the new design.
fn bench_cached_hit(b: &mut criterion::Bencher<'_>, core: &mut AllocCore) {
    // Pre-populate the cache ONCE so the entire timed region is cache hits.
    // (The very first call inside `b.iter` would otherwise land in the
    // first sample and skew that sample upward; pre-populating keeps every
    // measured call a pure cache hit.)
    let _ = core.dbg_current_node_cached();
    b.iter(|| {
        let n = core.dbg_current_node_cached();
        black_box(n)
    });
}

/// "Realistic batch" view: amortised per-call cost over a batch of N
/// back-to-back calls. Models the cost shape a single slot-claim lifetime
/// sees — under the cache, ONE claim-amortising call queries the OS and
/// the remaining N-1 calls are pure cache hits, so the per-call cost
/// drops toward the cache-hit cost as N grows. With N=1024 (a realistic
/// `find_segment_with_free` miss count over a slot-claim lifetime) the
/// cache-hit cost dominates.
///
/// For the UNCACHED baseline, all N calls query the OS — the per-call
/// cost stays flat at the syscall cost regardless of N.
fn bench_batch_uncached(b: &mut criterion::Bencher<'_>, n: usize) {
    b.iter(|| {
        let mut acc = 0u32;
        for _ in 0..n {
            acc = acc.wrapping_add(numa::current_node());
        }
        black_box(acc)
    });
}

fn bench_batch_cached(b: &mut criterion::Bencher<'_>, core: &mut AllocCore, n: usize) {
    let _ = core.dbg_current_node_cached(); // pre-populate
    b.iter(|| {
        let mut acc = 0u32;
        for _ in 0..n {
            acc = acc.wrapping_add(core.dbg_current_node_cached());
        }
        black_box(acc)
    });
}

fn bench_current_node(c: &mut Criterion) {
    let mut group = c.benchmark_group("numa_current_node");
    // Short-scenario policy (CLAUDE.md): sample_size(10) + short
    // warm/measurement. The relative order of the two arms is the
    // signal; absolute numbers are rough.
    group.sample_size(10);
    group.warm_up_time(std::time::Duration::from_millis(500));
    group.measurement_time(std::time::Duration::from_secs(2));

    // ── Per-call view (1 call per iter) ──────────────────────────────────
    group.bench_function("per_call_uncached", bench_uncached);

    let mut core = AllocCore::new().expect("primordial AllocCore");
    group.bench_function("per_call_cached_hit", |b| bench_cached_hit(b, &mut core));

    // ── Batch view (N calls per iter), at a realistic N = 1024 ───────────
    // 1024 ≈ a `find_segment_with_free` miss count over a slot-claim
    // lifetime under a churn workload (magazine refills dominate, but
    // misses still accumulate over a long-enough claim).
    const BATCH_N: usize = 1024;
    group.bench_function("batch_uncached_n1024", |b| bench_batch_uncached(b, BATCH_N));
    group.bench_function("batch_cached_n1024", |b| {
        bench_batch_cached(b, &mut core, BATCH_N)
    });

    group.finish();
}

criterion_group!(benches, bench_current_node);
criterion_main!(benches);
