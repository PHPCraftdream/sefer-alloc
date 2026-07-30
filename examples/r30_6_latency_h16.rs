//! R30-6 (task #455) — REAL-`#[global_allocator]` latency arm: `headroom_bytes
//! = 16 MiB`. One of four near-identical sibling binaries
//! (`_h0`/`_h16`/`_h64`/`_h256`), differing ONLY in the compile-time
//! `HEADROOM_BYTES` const baked into the `#[global_allocator]` `static`
//! (`SeferAlloc::with_config` bakes its `LargeCacheConfig` at compile time,
//! exactly like R27-4's `cap4`/`cap8` arms had to be separate binaries for
//! the same structural reason — see that report's §1 for the precedent).
//!
//! Mixed small+large workload (interleaved 1024 B churn + 6 MiB x 8-object
//! large burst, matching `examples/r30_6_large_cache_headroom_ab_gate.rs`'s
//! own workload shape so the latency and hit-rate/RSS axes measure the SAME
//! logical workload through two different entry points — registry-bypass for
//! the RSS/hit-rate axis, the real `#[global_allocator]` here for latency),
//! driven through `std::alloc` (which routes to the installed
//! `#[global_allocator]`), timed with an untimed warm-up batch first (R27-4's
//! warm-up-placement fix, applied from the start since these are new files).
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r30_6_latency_h16 --features "production alloc-stats"
//! ./target/release/examples/r30_6_latency_h16.exe
//! ```
//!
//! Driven in an A/B/B/A paired comparison via
//! `scripts/paired-ab-runner.mjs --config docs/perf/r30_6_latency_run.json`
//! (see that config file for the exact arm pairing used in the report).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::{alloc, dealloc, Layout};
use std::time::Instant;

use sefer_alloc::{LargeCacheConfig, SeferAlloc};

/// This arm's headroom — see the sibling files for the other three grid
/// points (16 MiB / 64 MiB / 256 MiB).
const HEADROOM_BYTES: usize = 16 * 1024 * 1024;

#[global_allocator]
static GLOBAL: SeferAlloc =
    SeferAlloc::with_config(LargeCacheConfig::new().headroom_bytes(HEADROOM_BYTES));

const SMALL_SIZE: usize = 1024;
const SMALL_WORKING_SET: usize = 64;
const SMALL_OPS: usize = 128;

const LARGE_OBJ_BYTES: usize = 6 * 1024 * 1024;
const LARGE_OBJ_COUNT: usize = 8;

/// Timed batches of the mixed (small-churn + large-burst) cycle.
const BATCHES: usize = 8;

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

fn small_churn(layout: Layout) {
    let mut live: Vec<*mut u8> = (0..SMALL_WORKING_SET)
        .map(|_| unsafe { alloc(layout) })
        .collect();
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..SMALL_OPS {
        let idx = rng.next_usize() % SMALL_WORKING_SET;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated with `layout`, freed once here.
            unsafe { dealloc(old, layout) };
        }
        // SAFETY: same layout preconditions as the initial prefill.
        live[idx] = unsafe { alloc(layout) };
    }
    for p in live {
        if !p.is_null() {
            // SAFETY: `p` still live, allocated with `layout`.
            unsafe { dealloc(p, layout) };
        }
    }
}

fn large_burst(layout: Layout) {
    let mut live = Vec::with_capacity(LARGE_OBJ_COUNT);
    for _ in 0..LARGE_OBJ_COUNT {
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc(layout) };
        assert!(!p.is_null(), "large alloc failed (OOM?)");
        let page = 4096usize;
        let mut off = 0usize;
        // SAFETY: `p` is a fresh allocation of `layout.size()` bytes, not yet freed.
        unsafe {
            while off < layout.size() {
                p.add(off).write_volatile(0xAB);
                off += page;
            }
        }
        live.push(p);
    }
    for p in live {
        // SAFETY: `p` was allocated above with `layout`, freed exactly once.
        unsafe { dealloc(p, layout) };
    }
}

fn run_mixed_batch(small_layout: Layout, large_layout: Layout) {
    small_churn(small_layout);
    large_burst(large_layout);
}

fn main() {
    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();
    let large_layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();

    // Untimed warm-up batch (absorbs primordial-segment bootstrap) — runs
    // BEFORE the timer, R27-4's warm-up-placement fix applied from the start.
    run_mixed_batch(small_layout, large_layout);

    let t0 = Instant::now();
    for _ in 0..BATCHES {
        run_mixed_batch(small_layout, large_layout);
    }
    let elapsed_ns = t0.elapsed().as_nanos();

    let stats = GLOBAL.stats();
    let snap = proc_probe::snapshot();

    proc_probe::emit("arm", "h16");
    proc_probe::emit_u64("headroom_bytes", HEADROOM_BYTES as u64);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns);
    proc_probe::emit_u64("batches", BATCHES as u64);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("large_cache_hits", stats.large_cache_hits);
    proc_probe::emit_u64("rss_after_kib", snap.rss / 1024);
    proc_probe::emit_u64("commit_after_kib", snap.commit / 1024);
    proc_probe::emit_u64("decommit_calls_total", stats.decommit_calls);
}
