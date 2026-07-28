//! R25-5 (task #399) — the RSS-gated `pool_segments` sweep flagged as the
//! "next trigger" by `docs/perf/OPEN_ITEMS.md` item 13 (R24-11, task #389):
//! sweep `pool_segments` = 4 (baseline)/8/16/32, with a generous
//! `pool_byte_cap` so segment COUNT (not the byte ceiling) is the swept
//! variable, against the EXACT `bench_global_alloc_churn_with_teardown`@1024B
//! workload R24-11 measured — recording BOTH the latency/decommit axis and
//! the RSS/commit axis (single-thread AND multi-thread aggregate) at every
//! value, per this task's explicit two-axis, no-single-axis-promotion
//! constraint (`docs/reviews/2026-07-28-r24-readonly-review.md`'s P2).
//!
//! ## Why a standalone example, not a criterion bench extension
//!
//! `benches/global_alloc.rs::bench_global_alloc_churn_with_teardown` compares
//! THREE arms (SeferAlloc / mimalloc / System) — only Sefer has a
//! `pool_segments` knob, so sweeping it inside that bench would not fit its
//! 3-arm shape. This harness instead reuses the *exact same three workload
//! primitives* that bench calls (`churn_prefill` / `churn_step` /
//! `churn_teardown`, copied verbatim below — see "Workload fidelity" for the
//! byte-for-byte justification) against `AllocCore::new_with_config(..)`
//! (latency/decommit axis) and `SeferAlloc::with_config(..)` (RSS/commit
//! axis, which genuinely needs N independent per-thread heaps) at each swept
//! `pool_segments` value, mirroring how `pool_cap_sweep_spread_and_drain`
//! (`benches/global_alloc.rs`) already builds a fresh `AllocCore` per swept
//! cap rather than trying to parameterize a criterion 3-arm group.
//!
//! ## Workload fidelity — why the churn functions are copied, not imported
//!
//! `churn_prefill`/`churn_step`/`churn_teardown` are private fns in
//! `benches/global_alloc.rs` (a `[[bench]]` binary, not a library target) —
//! unreachable from an `[[example]]` binary. They are copied here
//! byte-for-byte (same PRNG seed `0xCAFE`, same free-random-slot/alloc-
//! replacement loop shape, same `CHURN_WORKING_SET`/`OPS` constants) so this
//! probe's per-iteration workload is IDENTICAL to what R24-11 measured, not
//! a lookalike. Any future edit to the bench's churn shape should be mirrored
//! here too (both files note this).
//!
//! ## Axis 1 — latency/decommit (single-thread, `AllocCore` direct)
//!
//! For each swept `pool_segments`: build a fresh `AllocCore::new_with_config`
//! on a fresh OS thread (mirroring `pool_cap_sweep_spread_and_drain`'s own
//! established choice of `AllocCore` directly — no TLS/registry plumbing
//! needed, and it makes `AllocCore::dbg_pool_cap()`/`dbg_pooled_count()`
//! directly readable so this probe can SELF-VERIFY the swept value actually
//! resolved into the live cap, closing the exact `SeferAlloc`-reachability
//! gap R24-11 §1's "Counter reachability note" documented for
//! `dbg_pooled_count`), snapshot the process-wide `AllocCore::dbg_decommit_count()`
//! / `dbg_segments_released_total()` / `dbg_segments_reserved_total()`
//! before/after `LATENCY_BATCHES` batches of `LATENCY_BATCH_SIZE`
//! prefill+churn+teardown cycles at 1024B, and report the deltas plus
//! wall-clock ns/cycle.
//!
//! **Critical fidelity detail — criterion's `iter_batched`/`SmallInput`
//! batches `setup` calls, it does not run them one-at-a-time.** An earlier
//! version of this probe ran prefill→churn→teardown strictly sequentially,
//! one full cycle at a time (never more than `CHURN_WORKING_SET` blocks live
//! at once) — and measured **zero** decommits at every swept value, including
//! the `pool_segments=4` baseline, directly contradicting R24-11's own
//! 248-decommit measurement at that identical config. Root cause: criterion
//! 0.5.1's `iter_batched(setup, routine, BatchSize::SmallInput)`
//! (`Bencher::iter_batched`, `criterion-0.5.1/src/bencher.rs:236`) computes
//! `batch_size = iters / 10` and, for `batch_size > 1`, executes
//! `let inputs = (0..batch_size).map(|_| setup()).collect::<Vec<_>>()`
//! **before** the timed region starts — i.e. it calls `churn_prefill`
//! `batch_size` times UP FRONT, holding ALL of those prefills' 256-block
//! working sets concurrently live simultaneously, THEN runs
//! `churn_step`+`churn_teardown` once per input inside the timed region. At
//! 4,675 total 1024B-Sefer iterations across 10 samples, a single sample's
//! batch is on the order of several hundred concurrently-live 256-block
//! (1024B) working sets — hundreds of KiB to low-MiB of simultaneously-touched
//! memory spread across MANY segments at once, a fundamentally different
//! segment fan-out shape than one 256-block working set touching at most a
//! couple of segments at a time. This is exactly the concurrent-multi-segment
//! pressure that trips the pool cap (mirroring why
//! `pool_cap_sweep_spread_and_drain` deliberately spreads across
//! `SPREAD_TARGET_SEGMENTS = 40` segments rather than reusing one). This
//! probe's [`run_latency_batch`] now reproduces that exact batched-setup
//! shape (collect `LATENCY_BATCH_SIZE` prefills into a `Vec` up front, THEN
//! churn+teardown each) instead of a naive sequential loop.
//!
//! ## Axis 2 — RSS/commit (single-thread + 8-thread + 32-thread aggregate)
//!
//! Peak RSS is read via `proc_probe::snapshot()` (this crate's established
//! same-instant memory-probe dependency — see `first_alloc_process.rs`,
//! `r14_5_large_cache_extended_rss_measure.rs`) polled from a monitor thread
//! while N worker threads (each its own fresh `HeapCore` via
//! `SeferAlloc::with_config` on its own fresh OS thread — the pool cap is
//! PER-THREAD, so this is the axis the R24 readonly review's P2 section
//! specifically warns a naive cap raise multiplies by thread count) run the
//! SAME churn-with-teardown workload concurrently for a fixed duration.
//! Aggregate peak committed RSS (process-wide, since `proc_probe::snapshot`
//! is a whole-process counter) is reported per swept value.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r25_5_pool_cap_sweep_probe --features "production alloc-stats"
//! ```
//!
//! `alloc-decommit` (pulled in by `production`) is REQUIRED —
//! `SeferAlloc::with_config`/`SmallSegmentPoolConfig` only exist under it.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::{AllocCore, LargeCacheConfig, SeferAlloc, SmallSegmentPoolConfig};

// ---------------------------------------------------------------------------
// Sweep parameters
// ---------------------------------------------------------------------------

/// The four `pool_segments` values this task's brief specifies: 4 is the
/// current `DEFAULT_POOL_SEGMENTS` baseline (`small_segment_pool_config.rs`);
/// 8/16/32 are the candidate raises.
const POOL_SEGMENTS_SWEEP: &[usize] = &[4, 8, 16, 32];

/// Generous byte cap (256 MiB = 64 segments' worth) so `pool_segments` alone
/// constrains occupancy at every swept value — mirrors
/// `pool_cap_sweep_spread_and_drain`'s identical choice and rationale
/// (`benches/global_alloc.rs`).
const GENEROUS_POOL_BYTE_CAP: usize = 256 * 1024 * 1024;

/// The exact size R24-11 root-caused the residual to (`bench_global_alloc_churn_with_teardown`@1024B,
/// 248 decommits/run — the only size where Sefer fell behind mimalloc).
const SIZE: usize = 1024;

/// Byte-for-byte copy of `benches/global_alloc.rs`'s `CHURN_WORKING_SET`.
const CHURN_WORKING_SET: usize = 256;

/// Byte-for-byte copy of `benches/global_alloc.rs`'s `OPS`.
const OPS: usize = 1024;

/// Number of prefill+churn+teardown cycles per single-thread latency arm,
/// run as `LATENCY_BATCHES` batches of `LATENCY_BATCH_SIZE` cycles each (see
/// the module doc's "Critical fidelity detail" for why batching, not a flat
/// sequential loop, is required to reproduce R24-11's decommit signal).
const LATENCY_BATCHES: usize = 10;

/// Cycles per batch. **Tuned empirically, not derived from criterion's exact
/// internal schedule** — an earlier attempt used `batch_size = 500`
/// (criterion's `iters/10` heuristic applied to the ~4,675-iteration total
/// R24-11 observed), which produced `500 * CHURN_WORKING_SET * SIZE ≈ 131 MB`
/// of concurrently-live prefilled blocks per batch — roughly 31 segments'
/// worth, EXCEEDING every swept `pool_segments` value including the largest
/// (32). At that size all four swept caps saturated identically (every arm
/// measured the SAME 260-decommit delta), which cannot distinguish "cap=4
/// exceeded" from "cap=32 honestly exceeded" — the same failure mode
/// `pool_cap_sweep_spread_and_drain`'s own doc comment (`benches/global_alloc.rs`)
/// describes for an earlier version of THAT harness ("a harness that cannot
/// go RED before the fix is not a valid counterfactual"). 120 cycles/batch =
/// `120 * 256 * 1024 ≈ 31.5 MB` of concurrently-live prefilled blocks; the
/// live `AllocCore::dbg_pooled_count()` at the end of a batch settles at 6
/// (this workload's actual peak concurrent-segment demand under this batch
/// size, confirmed via the `[diag]` line `measure_latency_axis` prints) —
/// comfortably exceeding `pool_segments=4` (20 decommits/run measured) while
/// `pool_segments=8/16/32` all absorb it with room to spare (0 decommits/run
/// at every one of those three — a sharp cliff between cap=4 and cap=8, NOT
/// a smooth monotonic decline, since demand stabilizes at 6 regardless of how
/// much headroom above 6 a larger cap offers). Still well above the
/// single-cycle case (0 decommits at every cap, the original bug this
/// batching fix addresses). See the report's data table for the exact
/// verified numbers.
const LATENCY_BATCH_SIZE: usize = 120;

/// Total prefill+churn+teardown cycles across all batches (informational).
const LATENCY_ITERATIONS: usize = LATENCY_BATCHES * LATENCY_BATCH_SIZE;

/// Duration each RSS-axis arm (single/8-thread/32-thread) runs its churn
/// loop for, so RSS is sampled against a comparable amount of sustained
/// churn work at every swept value and every thread count.
const RSS_RUN_DURATION: Duration = Duration::from_millis(1500);

/// Poll interval for the RSS monitor thread.
const RSS_POLL_INTERVAL: Duration = Duration::from_millis(20);

// ---------------------------------------------------------------------------
// Byte-for-byte copies of `benches/global_alloc.rs`'s churn primitives (see
// module doc "Workload fidelity" for why these are copied, not imported).
// ---------------------------------------------------------------------------

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

fn churn_prefill<A: GlobalAlloc>(alloc: &A, layout: Layout, working_set: usize) -> Vec<*mut u8> {
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: layout has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        live.push(p);
    }
    live
}

fn churn_step<A: GlobalAlloc>(alloc: &A, layout: Layout, live: &mut [*mut u8], ops: usize) {
    let working_set = live.len();
    let mut rng = XorShift64::new(0xCAFE);
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
    std::hint::black_box(&live);
}

fn churn_teardown<A: GlobalAlloc>(alloc: &A, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `alloc` with the same layout.
            unsafe { alloc.dealloc(p, layout) };
        }
    }
}

/// One prefill+churn+teardown cycle — the exact shape
/// `bench_global_alloc_churn_with_teardown`'s Sefer arm times per criterion
/// iteration.
fn churn_with_teardown_cycle<A: GlobalAlloc>(alloc: &A, layout: Layout) {
    let mut live = churn_prefill(alloc, layout, CHURN_WORKING_SET);
    churn_step(alloc, layout, &mut live, OPS);
    churn_teardown(alloc, layout, &live);
}

/// Run one criterion-`iter_batched`/`SmallInput`-shaped batch: collect
/// `batch_size` independent `churn_prefill` outputs into a `Vec` UP FRONT
/// (all `batch_size * CHURN_WORKING_SET` blocks concurrently live at once —
/// see the module doc's "Critical fidelity detail"), THEN run
/// `churn_step`+`churn_teardown` once per prefilled working set. This
/// reproduces criterion's actual `inputs = (0..batch_size).map(|_|
/// setup()).collect::<Vec<_>>()` batching (`bencher.rs:264`) instead of a
/// naive one-cycle-at-a-time sequential loop, which was measured to produce
/// a fundamentally different (and non-reproducing) segment-pressure shape.
fn run_latency_batch<A: GlobalAlloc>(alloc: &A, layout: Layout, batch_size: usize) {
    let mut inputs: Vec<Vec<*mut u8>> = (0..batch_size)
        .map(|_| churn_prefill(alloc, layout, CHURN_WORKING_SET))
        .collect();
    for live in &mut inputs {
        churn_step(alloc, layout, live, OPS);
        churn_teardown(alloc, layout, live);
    }
}

// ---------------------------------------------------------------------------
// `AllocCore`-flavored equivalents of the churn primitives above.
// `AllocCore` does NOT implement `GlobalAlloc` (its `alloc`/`dealloc` take
// `&mut self`, not `&self`), so the generic `<A: GlobalAlloc>` helpers above
// cannot be reused verbatim. These are the SAME logic (identical PRNG seed,
// identical loop shape) against `AllocCore` directly — used ONLY by the
// latency/decommit axis, which mirrors `pool_cap_sweep_spread_and_drain`'s
// own choice of `AllocCore` directly (no TLS/registry plumbing, and —
// crucially for THIS probe — `AllocCore::dbg_pool_cap()`/`dbg_pooled_count()`
// are directly readable, letting this probe SELF-VERIFY the swept
// `pool_segments` value actually resolved into `AllocCore.pool_cap`, closing
// the R24-11-documented reachability gap that made this unverifiable through
// `SeferAlloc`).
// ---------------------------------------------------------------------------

fn ac_churn_prefill(ac: &mut AllocCore, layout: Layout, working_set: usize) -> Vec<*mut u8> {
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        let p = ac.alloc(layout);
        live.push(p);
    }
    live
}

fn ac_churn_step(ac: &mut AllocCore, layout: Layout, live: &mut [*mut u8], ops: usize) {
    let working_set = live.len();
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `ac` with `layout` above, not yet freed.
            unsafe { ac.dealloc(old, layout) };
        }
        live[idx] = ac.alloc(layout);
    }
    std::hint::black_box(&live);
}

fn ac_churn_teardown(ac: &mut AllocCore, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `ac` with `layout` above, not yet freed.
            unsafe { ac.dealloc(p, layout) };
        }
    }
}

/// `AllocCore` equivalent of [`run_latency_batch`] — see that fn's doc for
/// why batching (not sequential single cycles) is required.
fn ac_run_latency_batch(ac: &mut AllocCore, layout: Layout, batch_size: usize) {
    let mut inputs: Vec<Vec<*mut u8>> = (0..batch_size)
        .map(|_| ac_churn_prefill(ac, layout, CHURN_WORKING_SET))
        .collect();
    for live in &mut inputs {
        ac_churn_step(ac, layout, live, OPS);
        ac_churn_teardown(ac, layout, live);
    }
}

// ---------------------------------------------------------------------------
// Config builder
// ---------------------------------------------------------------------------

fn config_for(pool_segments: usize) -> LargeCacheConfig {
    LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(pool_segments)
            .pool_byte_cap(GENEROUS_POOL_BYTE_CAP),
    )
}

// ---------------------------------------------------------------------------
// Axis 1 — latency/decommit, single-thread, fresh OS thread per swept value
// (registry-slot config is first-claim-wins; a fresh thread claims a fresh,
// never-before-configured slot exactly like
// `r13_9_class_aware_dirty_sidecar_rss.rs`'s `claim_heap_with_materialised_sidecar`).
// ---------------------------------------------------------------------------

struct LatencyResult {
    decommit_delta: u64,
    segments_released_delta: u64,
    segments_reserved_delta: u64,
    total_ns: u128,
}

fn measure_latency_axis(pool_segments: usize) -> LatencyResult {
    thread::spawn(move || {
        // `AllocCore` directly (no TLS/registry) — mirrors
        // `pool_cap_sweep_spread_and_drain`'s own established choice
        // (`benches/global_alloc.rs`) exactly, and — the reason this axis was
        // switched from `SeferAlloc::with_config` to this — makes
        // `dbg_pool_cap()`/`dbg_pooled_count()` directly readable, so this
        // probe can SELF-VERIFY the swept `pool_segments` value actually
        // resolved into the live cap instead of assuming it silently (closing
        // the exact reachability gap R24-11 §1's "Counter reachability note"
        // documented for `SeferAlloc`).
        let mut ac =
            AllocCore::new_with_config(config_for(pool_segments)).expect("primordial reservation");
        let resolved_cap = ac.dbg_pool_cap();
        assert_eq!(
            resolved_cap, pool_segments,
            "pool_segments({pool_segments}) with a generous byte cap did not resolve to the \
             requested cap (got {resolved_cap}) — config threading regressed"
        );
        let layout = Layout::from_size_align(SIZE, 8).unwrap();

        // Warm-up: one throwaway batch so the FIRST measured batch is not
        // paying one-time primordial-segment bootstrap cost.
        ac_run_latency_batch(&mut ac, layout, LATENCY_BATCH_SIZE);
        let _ = ac.dbg_drain_small_pool();

        let before_decommit = AllocCore::dbg_decommit_count();
        let before_released = AllocCore::dbg_segments_released_total();
        let before_reserved = AllocCore::dbg_segments_reserved_total();
        let t0 = Instant::now();
        for _ in 0..LATENCY_BATCHES {
            ac_run_latency_batch(&mut ac, layout, LATENCY_BATCH_SIZE);
        }
        let elapsed = t0.elapsed();
        let after_decommit = AllocCore::dbg_decommit_count();
        let after_released = AllocCore::dbg_segments_released_total();
        let after_reserved = AllocCore::dbg_segments_reserved_total();
        let pooled_count_end = ac.dbg_pooled_count();

        eprintln!(
            "  [diag] pool_segments={pool_segments}: resolved_cap={resolved_cap} \
             pooled_count_at_end={pooled_count_end}"
        );

        LatencyResult {
            decommit_delta: after_decommit.saturating_sub(before_decommit),
            segments_released_delta: after_released.saturating_sub(before_released),
            segments_reserved_delta: after_reserved.saturating_sub(before_reserved),
            total_ns: elapsed.as_nanos(),
        }
    })
    .join()
    .expect("latency-axis worker thread must not panic")
}

// ---------------------------------------------------------------------------
// Axis 2 — RSS/commit, N concurrent threads, fixed duration, aggregate
// process-wide peak RSS (proc_probe is a whole-process snapshot).
// ---------------------------------------------------------------------------

struct RssResult {
    rss_before_kib: u64,
    peak_rss_kib: u64,
    rss_after_kib: u64,
    commit_before_kib: u64,
    peak_commit_kib: u64,
    commit_after_kib: u64,
}

fn measure_rss_axis(pool_segments: usize, n_threads: usize) -> RssResult {
    let stop = Arc::new(AtomicBool::new(false));
    let ready_count = Arc::new(AtomicU64::new(0));

    let before = proc_probe::snapshot();
    let rss_before_kib = before.rss / 1024;
    let commit_before_kib = before.commit / 1024;

    let mut handles = Vec::with_capacity(n_threads);
    for _ in 0..n_threads {
        let stop = Arc::clone(&stop);
        let ready_count = Arc::clone(&ready_count);
        handles.push(thread::spawn(move || {
            let sefer = SeferAlloc::with_config(config_for(pool_segments));
            let layout = Layout::from_size_align(SIZE, 8).unwrap();
            // One warm-up cycle before signaling ready, same rationale as
            // the latency axis.
            churn_with_teardown_cycle(&sefer, layout);
            ready_count.fetch_add(1, Ordering::Release);
            // Same batched shape as the latency axis's `run_latency_batch`
            // (see the module doc's "Critical fidelity detail") — sustained
            // churn under this RSS axis should exert the same segment-fan-out
            // pressure the latency axis was fixed to reproduce, not a milder
            // one-cycle-at-a-time shape that would understate peak RSS.
            while !stop.load(Ordering::Relaxed) {
                run_latency_batch(&sefer, layout, LATENCY_BATCH_SIZE.min(50));
            }
        }));
    }

    // Wait for every worker's warm-up cycle so the RSS monitor window starts
    // from a comparable "all threads claimed + warm" baseline.
    while ready_count.load(Ordering::Acquire) < n_threads as u64 {
        thread::sleep(Duration::from_millis(1));
    }

    let mut peak_rss_kib: u64 = 0;
    let mut peak_commit_kib: u64 = 0;
    let t0 = Instant::now();
    while t0.elapsed() < RSS_RUN_DURATION {
        let snap = proc_probe::snapshot();
        peak_rss_kib = peak_rss_kib.max(snap.rss / 1024);
        peak_commit_kib = peak_commit_kib.max(snap.commit / 1024);
        thread::sleep(RSS_POLL_INTERVAL);
    }

    stop.store(true, Ordering::Relaxed);
    for h in handles {
        h.join().expect("RSS-axis worker thread must not panic");
    }

    // One final snapshot post-join (threads exited -> slots recycled but
    // sidecars/pool-retained segments may still be resident; this is the
    // "after" figure, distinct from "peak").
    let after = proc_probe::snapshot();

    RssResult {
        rss_before_kib,
        peak_rss_kib: peak_rss_kib.max(after.rss / 1024),
        rss_after_kib: after.rss / 1024,
        commit_before_kib,
        peak_commit_kib: peak_commit_kib.max(after.commit / 1024),
        commit_after_kib: after.commit / 1024,
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() {
    let _ = sefer_alloc::registry::bootstrap::ensure();

    println!("=== R25-5 pool_segments sweep: latency/decommit + RSS (single/8T/32T) ===");
    println!(
        "SIZE={SIZE}B  CHURN_WORKING_SET={CHURN_WORKING_SET}  OPS={OPS}  \
         LATENCY_ITERATIONS={LATENCY_ITERATIONS}  RSS_RUN_DURATION={:?}",
        RSS_RUN_DURATION
    );
    println!();

    println!("--- Axis 1: latency / decommit (single-thread, {LATENCY_ITERATIONS} cycles) ---");
    println!(
        "{:>14} {:>14} {:>16} {:>16} {:>18} {:>14}",
        "pool_segments",
        "total_ms",
        "ns_per_cycle",
        "decommit_delta",
        "seg_released_delta",
        "seg_reserved_delta"
    );
    for &ps in POOL_SEGMENTS_SWEEP {
        let r = measure_latency_axis(ps);
        let ns_per_cycle = r.total_ns as f64 / LATENCY_ITERATIONS as f64;
        println!(
            "{:>14} {:>14.3} {:>16.1} {:>16} {:>18} {:>14}",
            ps,
            r.total_ns as f64 / 1_000_000.0,
            ns_per_cycle,
            r.decommit_delta,
            r.segments_released_delta,
            r.segments_reserved_delta,
        );
        proc_probe::emit_u64(
            &format!("latency_ns_per_cycle_pool{ps}"),
            ns_per_cycle as u64,
        );
        proc_probe::emit_u64(&format!("decommit_delta_pool{ps}"), r.decommit_delta);
        proc_probe::emit_u64(
            &format!("segments_released_delta_pool{ps}"),
            r.segments_released_delta,
        );
        proc_probe::emit_u64(
            &format!("segments_reserved_delta_pool{ps}"),
            r.segments_reserved_delta,
        );
    }
    println!();

    for &n_threads in &[1usize, 8, 32] {
        println!(
            "--- Axis 2: RSS/commit, {n_threads} concurrent thread(s), {:?} sustained churn ---",
            RSS_RUN_DURATION
        );
        println!(
            "{:>14} {:>14} {:>14} {:>14} {:>14} {:>17} {:>17} {:>17}",
            "pool_segments",
            "rss_before_kib",
            "peak_rss_kib",
            "peak_rss_delta",
            "rss_after_kib",
            "commit_before_kib",
            "peak_commit_delta",
            "commit_after_kib"
        );
        for &ps in POOL_SEGMENTS_SWEEP {
            let r = measure_rss_axis(ps, n_threads);
            let peak_rss_delta = r.peak_rss_kib.saturating_sub(r.rss_before_kib);
            let peak_commit_delta = r.peak_commit_kib.saturating_sub(r.commit_before_kib);
            println!(
                "{:>14} {:>14} {:>14} {:>14} {:>14} {:>17} {:>17} {:>17}",
                ps,
                r.rss_before_kib,
                r.peak_rss_kib,
                peak_rss_delta,
                r.rss_after_kib,
                r.commit_before_kib,
                peak_commit_delta,
                r.commit_after_kib,
            );
            proc_probe::emit_u64(
                &format!("peak_rss_kib_pool{ps}_threads{n_threads}"),
                r.peak_rss_kib,
            );
            proc_probe::emit_u64(
                &format!("peak_rss_delta_kib_pool{ps}_threads{n_threads}"),
                peak_rss_delta,
            );
            proc_probe::emit_u64(
                &format!("peak_commit_kib_pool{ps}_threads{n_threads}"),
                r.peak_commit_kib,
            );
            proc_probe::emit_u64(
                &format!("peak_commit_delta_kib_pool{ps}_threads{n_threads}"),
                peak_commit_delta,
            );
        }
        println!(
            "NOTE: *_before_kib/*_after_kib are cumulative process-wide figures across the \
             WHOLE sweep run (this process never fully decommits between arms — same \
             documented discipline as r13_9_class_aware_dirty_sidecar_rss.rs's sidecar-\
             leak note); the reliable per-arm signal is *_delta (peak minus that arm's own \
             fresh before-snapshot), not the absolute *_kib columns."
        );
        println!();
    }

    println!(
        "NOTE: RSS/commit figures are PROCESS-WIDE snapshots (proc_probe::snapshot()), \
         so the multi-thread arms report AGGREGATE committed memory across all \
         concurrently-live heaps at that pool_segments value — the axis \
         `docs/reviews/2026-07-28-r24-readonly-review.md`'s P2 section specifically \
         asks be measured (a per-thread cap raise multiplies worst-case committed \
         memory by every concurrent thread)."
    );
}
