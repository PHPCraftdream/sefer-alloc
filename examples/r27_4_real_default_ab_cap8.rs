//! R27-4 (task #422) — A/B arm: the REAL prospective DEFAULT config
//! `(pool_segments=8, pool_byte_cap=32 MiB)` — the PROSPECTIVE paired default
//! from task #419. Runs the EXACT `bench_global_alloc_churn_with_teardown`@1024B
//! shape R25-5/R26-1/R26-3 measured, driven through the REAL `#[global_allocator]`
//! SeferAlloc, at the REAL byte cap (NOT R26-3's 256 MiB ceiling). Since
//! `pool_cap` resolves as `min(pool_segments, pool_byte_cap/SEGMENT)`, R26-3's
//! 256 MiB and this arm's 32 MiB both resolve to effective cap 8 — so this
//! proves the real byte cap does not unexpectedly bind.
//!
//! ## Warm-up timing fix (vs R26-3)
//!
//! R26-3's `main()` placed `t0` BEFORE `run_workload()`, whose first batch is a
//! documented "untimed" warm-up — so all 9 batches were timed. This binary fixes
//! that: warm-up batch runs BEFORE `t0`, then 8 labelled-timed batches. So
//! `elapsed_ns` covers exactly 8 x 120 = 960 timed cycles (not R26-3's 9/1080).
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r27_4_real_default_ab_cap8 --features "production alloc-stats"
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

/// `pool_segments = 8` — the PROSPECTIVE paired-default raise (with 32 MiB byte
/// cap). Resolves to effective cap min(8, 32 MiB / 4 MiB) = 8. (DEFAULT_POOL_SEGMENTS
/// stays 4; this task does NOT change it.)
const POOL_SEGMENTS: usize = 8;

/// 32 MiB — the PROSPECTIVE paired DEFAULT_POOL_BYTE_CAP (NOT R26-3's 256 MiB
/// ceiling). Resolves to effective cap min(8, 32 MiB / 4 MiB) = 8.
const REAL_POOL_BYTE_CAP: usize = 32 * 1024 * 1024;

// `SeferAlloc::with_config` and every builder in the chain are `const fn`, so
// this composes as a `static` initializer; the config threads into the TLS
// bind slow path on the main thread's first allocation.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::with_config(
    LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(POOL_SEGMENTS)
            .pool_byte_cap(REAL_POOL_BYTE_CAP),
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

fn main() {
    let layout = Layout::from_size_align(SIZE, 8).unwrap();
    // Untimed warm-up batch (absorbs primordial-segment bootstrap) — runs BEFORE
    // the timer so `elapsed_ns` covers only the 8 labelled-timed batches. This is
    // the R26-3 warm-up-placement fix applied from the start (R26-3 timed all 9
    // batches by mistake — see R27-4 report methodology). 8 x 120 = 960 timed cycles.
    run_latency_batch(layout, LATENCY_BATCH_SIZE);
    let t0 = Instant::now();
    for _ in 0..LATENCY_BATCHES {
        run_latency_batch(layout, LATENCY_BATCH_SIZE);
    }
    let elapsed_ns = t0.elapsed().as_nanos();

    let stats = GLOBAL.stats();
    let snap = proc_probe::snapshot();

    proc_probe::emit("arm", "cap8");
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("rss_after_kib", snap.rss / 1024);
    proc_probe::emit_u64("commit_after_kib", snap.commit / 1024);
    proc_probe::emit_u64("decommit_calls_total", stats.decommit_calls);
}
