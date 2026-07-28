//! R26-3 (task #412) — A/B arm: `pool_segments = 4` (the current
//! `DEFAULT_POOL_SEGMENTS` baseline). Runs the EXACT
//! `bench_global_alloc_churn_with_teardown`@1024B shape R25-5/R26-1 measured,
//! driven through the REAL installed `#[global_allocator]` SeferAlloc (via
//! `std::alloc::alloc`/`dealloc`), NOT `AllocCore` directly. R25-5/R26-1
//! measured the latency/decommit axis via an `AllocCore::new_with_config`
//! bypass; THIS binary confirms the same finding (cap 4->8 eliminates the
//! decommit-driven slowdown) through the un-bypassed production entry point,
//! paired against its cap=8 sibling by `scripts/paired-ab-runner.mjs`.
//!
//! The churn primitives are byte-for-byte copies of `benches/global_alloc.rs`'s
//! (via R25-5); the batched `run_latency_batch` shape is REQUIRED to reproduce
//! the segment-fan-out that trips cap=4 (a naive sequential loop measures zero
//! decommits — see R25-5's module doc "Critical fidelity detail"). The ONLY
//! difference vs `r26_3_teardown_ab_cap8.rs` is `POOL_SEGMENTS`.
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r26_3_teardown_ab_cap4 --features "production alloc-stats"
//! ```

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::{alloc, dealloc, Layout};
use std::hint::black_box;
use std::time::Instant;

use sefer_alloc::{LargeCacheConfig, SeferAlloc, SmallSegmentPoolConfig};

/// `pool_segments = 4` — current baseline. R25-5/R26-1 measured a ~20-decommit/run residual here.
const POOL_SEGMENTS: usize = 4;

/// 256 MiB so `pool_segments` alone constrains occupancy (mirrors R25-5/R26-1).
const GENEROUS_POOL_BYTE_CAP: usize = 256 * 1024 * 1024;

// `SeferAlloc::with_config` and every builder in the chain are `const fn`, so
// this composes as a `static` initializer; the config threads into the TLS
// bind slow path on the main thread's first allocation.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(
    LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(POOL_SEGMENTS)
            .pool_byte_cap(GENEROUS_POOL_BYTE_CAP),
    ),
);

const SIZE: usize = 1024;
const CHURN_WORKING_SET: usize = 256;
const OPS: usize = 1024;

/// Cycles per batch. At 120, pooled segment count settles at 6 (R25-5's
/// verified diag) — exceeds cap=4 (~2 decommits/batch) while cap=8+ absorbs it.
const LATENCY_BATCH_SIZE: usize = 120;

/// Timed batches: 8 x 120 = 960 cycles, ~110-170 ms timed region (cap=8 faster,
/// cap=4 slower). Paired judge's 80 launches finish well under a minute.
const LATENCY_BATCHES: usize = 8;

/// xorshift64, seed `0xCAFE` — verbatim from `benches/global_alloc.rs` (via R25-5).
struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }

    #[inline]
    fn next_usize(&mut self) -> usize {
        self.next_u64() as usize
    }
}

/// `churn_prefill` from `benches/global_alloc.rs`, routed through `std::alloc::alloc`.
fn churn_prefill(layout: Layout, working_set: usize) -> Vec<*mut u8> {
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: layout has non-zero size and valid alignment; routes to the installed #[global_allocator].
        let p = unsafe { alloc(layout) };
        live.push(p);
    }
    live
}

/// `churn_step` from `benches/global_alloc.rs`, routed through `std::alloc`.
fn churn_step(layout: Layout, live: &mut [*mut u8], ops: usize) {
    let working_set = live.len();
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated with this layout, freed once here.
            unsafe { dealloc(old, layout) };
        }
        // SAFETY: same layout preconditions as the prefill alloc above.
        live[idx] = unsafe { alloc(layout) };
    }
    black_box(&live);
}

/// `churn_teardown` from `benches/global_alloc.rs`.
fn churn_teardown(layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` still live, allocated with this layout.
            unsafe { dealloc(p, layout) };
        }
    }
}

/// criterion `iter_batched`/`SmallInput` shape: collect `batch_size` prefills
/// UP FRONT (all concurrently live), THEN churn+teardown each. Reproduces the
/// segment-fan-out that trips cap=4 (R25-5 "Critical fidelity detail").
fn run_latency_batch(layout: Layout, batch_size: usize) {
    let mut inputs: Vec<Vec<*mut u8>> = (0..batch_size)
        .map(|_| churn_prefill(layout, CHURN_WORKING_SET))
        .collect();
    for live in &mut inputs {
        churn_step(layout, live, OPS);
        churn_teardown(layout, live);
    }
}

/// One untimed warm-up batch (absorbs primordial-segment bootstrap), then `LATENCY_BATCHES` timed batches.
fn run_workload() {
    let layout = Layout::from_size_align(SIZE, 8).unwrap();
    run_latency_batch(layout, LATENCY_BATCH_SIZE);
    for _ in 0..LATENCY_BATCHES {
        run_latency_batch(layout, LATENCY_BATCH_SIZE);
    }
}

fn main() {
    let t0 = Instant::now();
    run_workload();
    let elapsed_ns = t0.elapsed().as_nanos();

    let stats = GLOBAL.stats();
    let snap = proc_probe::snapshot();

    proc_probe::emit("arm", "cap4");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("rss_after_kib", snap.rss / 1024);
    proc_probe::emit_u64("commit_after_kib", snap.commit / 1024);
    proc_probe::emit_u64("decommit_calls_total", stats.decommit_calls);
}
