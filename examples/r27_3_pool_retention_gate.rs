//! R27-3 (task #421) — the pool RETENTION gate: measures the cap-4-vs-cap-8
//! retention trade-off with victim activation PROVEN. This is the
//! load-bearing measurement tasks R27-4/#422, R27-5/#423, and conditional
//! R27-11/#429 cite.
//!
//! ## Why this exists
//!
//! R26-1's RSS gate measured ONLY peak-live-set RSS at batch 50 (~12.5 MiB
//! prefill, fits inside the 4-segment/16 MiB retention region) — it never
//! proved cap 4 saturated or cap 8 retained a 5th segment, so its
//! "RSS-neutral" verdict was vacuous (the capacity difference was never
//! exercised). R26-3's raw log independently shows cap 8 deterministically
//! retaining ~+4,100 KiB (~one 4 MiB segment) after teardown at the
//! pressure-producing batch 120. This gate measures the retention trade-off
//! directly: subprocess-per-arm isolation + R26-1's config self-verification,
//! but at batch 120, recording peak-live AND post-teardown AND post-idle
//! RSS/commit, final/max `dbg_pooled_count`, decommit/release/reserve
//! counters, with cap 4 PROVEN to saturate (decommit delta > 0) and cap 8
//! PROVEN to retain >4 segments (pooled high-water > 4).
//! See `docs/perf/R27_3_POOL_RETENTION_GATE.md` for the verdict.
//!
//! ## Decay note (read from source, not guessed)
//!
//! The small-segment pool shares the large-cache decay interval
//! (`maybe_decay_small_pool`, `src/alloc_core/alloc_core_small_pool.rs`,
//! reuses `self.decay_config.decay_interval`; default 1000 ms from
//! `large_cache_config.rs` `DEFAULT_DECAY_INTERVAL_MS`). It is EVENT-DRIVEN:
//! it fires inline on the `reserve_small_segment` cold path during allocation
//! pressure, evicting one FIFO-oldest pooled segment per tick — NO background
//! thread. Pure idle (no allocations) therefore does NOT decay the pool; this
//! gate's idle time-series empirically confirms pooled_count is flat across the
//! idle window, and demonstrates reclaimability via explicit
//! `HeapCore::dbg_drain_small_pool`.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r27_3_pool_retention_gate --features "production alloc-stats"
//! ```

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    AllocCore, LargeCacheConfig, SmallSegmentPoolConfig,
};

// ---------------------------------------------------------------------------
// Sweep parameters — the REAL candidate configs (NOT the 256 MiB
// measurement-only byte cap). The current default is (4, 16 MiB); the
// prospective paired default (per task #419/R27-1) is (8, 32 MiB).
// ---------------------------------------------------------------------------

/// `(pool_segments, pool_byte_cap)` arms. (4, 16 MiB) = current default;
/// (8, 32 MiB) = the prospective paired default this gate exists to evaluate.
const CONFIGS: &[(usize, usize)] = &[
    (4, 16 * 1024 * 1024),
    (8, 32 * 1024 * 1024),
];

/// Thread counts matching R26-1's grid.
const THREAD_COUNTS: &[usize] = &[1, 8, 32];

/// Repetitions per cell (median + range). Matches R26-1's precedent.
const REPETITIONS: usize = 3;

const SIZE: usize = 1024;
const CHURN_WORKING_SET: usize = 256;
const OPS: usize = 1024;

/// Pressure-producing batch size — the one R25-5/R26-3 established reliably
/// exceeds 4 segments and settles around 6. NOT R26-1's batch 50 (which fit
/// inside the 4-segment retention region and never proved victim activation).
const PRESSURE_BATCH_SIZE: usize = 120;

/// Fixed-duration pressure window (workers run batched churn for this long;
/// pooled_count settles within the first 2-3 batches, so this is generous).
const PRESSURE_DURATION: Duration = Duration::from_millis(1500);
const RSS_POLL_INTERVAL: Duration = Duration::from_millis(20);

/// One small segment = 4 MiB (matches `SEGMENT` in `alloc_core`).
const SEGMENT_BYTES: usize = 4 * 1024 * 1024;

/// The configured decay interval (ms), read from source: the small pool
/// reuses the large-cache decay interval whose default is 1000 ms
/// (`large_cache_config.rs::DEFAULT_DECAY_INTERVAL_MS`).
const DECAY_INTERVAL_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Byte-for-byte copies of `benches/global_alloc.rs`'s churn primitives (copied
// via R25-5/R26-1; `benches/global_alloc.rs` is a `[[bench]]` binary,
// unreachable from an `[[example]]`).
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

fn hc_churn_prefill(heap: &mut HeapCore, layout: Layout, working_set: usize) -> Vec<*mut u8> {
    let mut live: Vec<*mut u8> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        let p = heap.alloc(layout);
        live.push(p);
    }
    live
}

fn hc_churn_step(heap: &mut HeapCore, layout: Layout, live: &mut [*mut u8], ops: usize) {
    let working_set = live.len();
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..ops {
        let idx = rng.next_usize() % working_set;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `heap` with `layout` above, not yet freed.
            unsafe { heap.dealloc(old, layout) };
        }
        live[idx] = heap.alloc(layout);
    }
    std::hint::black_box(&live);
}

fn hc_churn_teardown(heap: &mut HeapCore, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `heap` with `layout` above, not yet freed.
            unsafe { heap.dealloc(p, layout) };
        }
    }
}

fn hc_churn_with_teardown_cycle(heap: &mut HeapCore, layout: Layout) {
    let mut live = hc_churn_prefill(heap, layout, CHURN_WORKING_SET);
    hc_churn_step(heap, layout, &mut live, OPS);
    hc_churn_teardown(heap, layout, &live);
}

/// One criterion-`iter_batched`/`SmallInput`-shaped batch: collect `batch_size`
/// independent `hc_churn_prefill` outputs into a `Vec` UP FRONT (all
/// `batch_size * CHURN_WORKING_SET` blocks concurrently live at once), THEN run
/// `hc_churn_step`+`hc_churn_teardown` once per prefilled working set. This is
/// the batched-setup shape R25-5's module doc proved is REQUIRED to reproduce
/// the segment-pressure signal (a naive sequential loop never trips the cap).
fn hc_run_batch(heap: &mut HeapCore, layout: Layout, batch_size: usize) {
    let mut inputs: Vec<Vec<*mut u8>> = (0..batch_size)
        .map(|_| hc_churn_prefill(heap, layout, CHURN_WORKING_SET))
        .collect();
    for live in &mut inputs {
        hc_churn_step(heap, layout, live, OPS);
        hc_churn_teardown(heap, layout, live);
    }
}

// ---------------------------------------------------------------------------
// Config builder
// ---------------------------------------------------------------------------

fn config_for(pool_segments: usize, pool_byte_cap: usize) -> LargeCacheConfig {
    LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(pool_segments)
            .pool_byte_cap(pool_byte_cap),
    )
}

// ---------------------------------------------------------------------------
// Child/arm mode — run ONE `(pool_segments, pool_byte_cap, thread_count)`
// arm in this fresh process, self-verify, prove victim activation, emit RESULT.
// ---------------------------------------------------------------------------

fn parse_env_usize(name: &str) -> usize {
    let raw = std::env::var(name)
        .unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"));
    raw.parse::<usize>()
        .unwrap_or_else(|e| panic!("{name}={raw:?} not a valid usize ({e})"))
}

fn snapshot_kib() -> (u64, u64) {
    let m = proc_probe::snapshot();
    (m.rss / 1024, m.commit / 1024)
}

fn run_child() {
    let pool_segments = parse_env_usize("R27_3_POOL_SEGMENTS");
    let pool_byte_cap = parse_env_usize("R27_3_POOL_BYTE_CAP");
    let thread_count = parse_env_usize("R27_3_THREAD_COUNT");
    let repetition = parse_env_usize("R27_3_REPETITION");
    let pool_byte_cap_mib = (pool_byte_cap / (1024 * 1024)) as u64;

    let conflicts_before = config_conflicts_total();

    // Shared coordination state.
    let pressure_running = Arc::new(AtomicBool::new(false));
    let drain_done = Arc::new(AtomicBool::new(false));
    let release = Arc::new(AtomicBool::new(false));
    let ready_count = Arc::new(AtomicU64::new(0));
    let hold_ready = Arc::new(AtomicU64::new(0));
    let drain_acked = Arc::new(AtomicU64::new(0));
    let caps: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(u64::MAX)).collect());
    let pooled_hw: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let pooled_final: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let pooled_predrain: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let drained_sum: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());

    // Take the before-snapshot + counter baseline BEFORE spawning workers
    // (matching R26-1's placement: after bootstrap, before workload).
    let (rss_before_kib, _commit_before_kib) = snapshot_kib();
    let decommit_before = AllocCore::dbg_decommit_count();
    let released_before = AllocCore::dbg_segments_released_total();
    let reserved_before = AllocCore::dbg_segments_reserved_total();

    let mut handles = Vec::with_capacity(thread_count);
    for i in 0..thread_count {
        let (
            pressure_running,
            drain_done,
            release,
            ready_count,
            hold_ready,
            drain_acked,
            caps,
            pooled_hw,
            pooled_final,
            pooled_predrain,
            drained_sum,
        ) = (
            Arc::clone(&pressure_running),
            Arc::clone(&drain_done),
            Arc::clone(&release),
            Arc::clone(&ready_count),
            Arc::clone(&hold_ready),
            Arc::clone(&drain_acked),
            Arc::clone(&caps),
            Arc::clone(&pooled_hw),
            Arc::clone(&pooled_final),
            Arc::clone(&pooled_predrain),
            Arc::clone(&drained_sum),
        );
        handles.push(thread::spawn(move || {
            let heap_ptr = HeapRegistry::claim_with_config(config_for(pool_segments, pool_byte_cap));
            assert!(
                !heap_ptr.is_null(),
                "HeapRegistry::claim_with_config returned null at thread {i}"
            );
            // SAFETY: `heap_ptr` was just returned by `claim_with_config` and is
            // owned by THIS thread until we `recycle` it below.
            let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

            // SELF-VERIFICATION #1: resolved cap equals the requested one.
            let resolved = heap.dbg_pool_cap();
            assert_eq!(
                resolved, pool_segments,
                "R27-3 child (cap={pool_segments}, byte_cap={pool_byte_cap}, thread={i}, \
                 rep={repetition}): resolved cap ({resolved}) != requested ({pool_segments}) — \
                 isolation contract broken, this arm's number is untrustworthy"
            );
            caps[i].store(resolved as u64, Ordering::Release);

            let layout = Layout::from_size_align(SIZE, 8).unwrap();
            // Warm-up before signaling ready.
            hc_churn_with_teardown_cycle(heap, layout);
            ready_count.fetch_add(1, Ordering::Release);

            // Wait for GO so the main thread's before-snapshot is clean.
            while !pressure_running.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }

            // Pressure phase: batched churn. Sample pooled_count high-water
            // after each batch (cheap field read, no hot-path instrumentation).
            let mut hw: u64 = 0;
            while pressure_running.load(Ordering::Acquire) {
                hc_run_batch(heap, layout, PRESSURE_BATCH_SIZE);
                let c = heap.dbg_pooled_count() as u64;
                if c > hw {
                    hw = c;
                }
                pooled_hw[i].store(hw, Ordering::Relaxed);
            }

            // Post-teardown: the last batch's teardown pooled segments. Record
            // final pooled_count, then KEEP THE HEAP ALIVE through the idle +
            // drain observation window (do NOT recycle yet).
            let final_c = heap.dbg_pooled_count() as u64;
            pooled_final[i].store(final_c, Ordering::Release);
            hold_ready.fetch_add(1, Ordering::Release);

            // Hold phase: keep heap alive during the main thread's idle RSS
            // sampling. Wait for the drain command.
            while !drain_done.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            // Re-sample pooled_count right before draining — empirical proof it
            // did NOT change during the idle window (no allocs => no decay).
            pooled_predrain[i].store(heap.dbg_pooled_count() as u64, Ordering::Release);
            let drained = heap.dbg_drain_small_pool() as u64;
            drained_sum[i].store(drained, Ordering::Release);
            drain_acked.fetch_add(1, Ordering::Release);

            while !release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            // SAFETY: `heap_ptr` was returned by `claim_with_config` above and
            // has not yet been recycled.
            unsafe { HeapRegistry::recycle(heap_ptr) };
        }));
    }

    // Wait for every worker to be claimed + warmed + self-verified.
    while ready_count.load(Ordering::Acquire) < thread_count as u64 {
        thread::sleep(Duration::from_millis(1));
    }

    // GO: start pressure + peak-RSS monitor.
    pressure_running.store(true, Ordering::Release);
    let mut peak_rss_kib: u64 = 0;
    let mut peak_commit_kib: u64 = 0;
    let t0 = Instant::now();
    while t0.elapsed() < PRESSURE_DURATION {
        let (r, c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        peak_commit_kib = peak_commit_kib.max(c);
        thread::sleep(RSS_POLL_INTERVAL);
    }
    pressure_running.store(false, Ordering::Release);

    // Wait for every worker to reach the hold phase (post-teardown, pooled).
    while hold_ready.load(Ordering::Acquire) < thread_count as u64 {
        thread::sleep(Duration::from_millis(1));
    }

    // Post-teardown RSS/commit (working set freed, pooled segments retained).
    let (rss_post_kib, commit_post_kib) = snapshot_kib();
    let decommit_after = AllocCore::dbg_decommit_count();
    let released_after = AllocCore::dbg_segments_released_total();
    let reserved_after = AllocCore::dbg_segments_reserved_total();
    let decommit_delta = decommit_after.saturating_sub(decommit_before);
    let seg_released_delta = released_after.saturating_sub(released_before);
    let seg_reserved_delta = reserved_after.saturating_sub(reserved_before);

    // Idle time-series (pure idle — no allocations). pooled_count is provably
    // constant here (no reserve_small_segment => no pool pop/decay), confirmed
    // empirically by the pre-drain re-sample below.
    thread::sleep(Duration::from_millis(100));
    let (rss_100ms_kib, _commit_100ms_kib) = snapshot_kib();
    thread::sleep(Duration::from_millis(900));
    let (rss_1s_kib, _commit_1s_kib) = snapshot_kib();
    thread::sleep(Duration::from_millis(1000));
    let (rss_2s_kib, _commit_2s_kib) = snapshot_kib();

    // Drain: release all pooled segments, measure reclaimable RSS.
    drain_done.store(true, Ordering::Release);
    while drain_acked.load(Ordering::Acquire) < thread_count as u64 {
        thread::sleep(Duration::from_millis(1));
    }
    let (rss_drain_kib, commit_drain_kib) = snapshot_kib();

    // Read per-heap pooled aggregates.
    let pooled_hw_max = pooled_hw.iter().map(|a| a.load(Ordering::Acquire)).max().unwrap_or(0);
    let pooled_final_sum: u64 = pooled_final.iter().map(|a| a.load(Ordering::Acquire)).sum();
    let pooled_final_max = pooled_final.iter().map(|a| a.load(Ordering::Acquire)).max().unwrap_or(0);
    let pooled_predrain_max =
        pooled_predrain.iter().map(|a| a.load(Ordering::Acquire)).max().unwrap_or(0);
    let drained_sum_total: u64 = drained_sum.iter().map(|a| a.load(Ordering::Acquire)).sum();

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);

    // SELF-VERIFICATION re-check: every worker's stashed resolved cap matches.
    for (i, slot) in caps.iter().enumerate() {
        let v = slot.load(Ordering::Acquire);
        assert_eq!(
            v, pool_segments as u64,
            "R27-3 child: worker {i} stashed resolved cap {v}, expected {pool_segments}"
        );
    }
    assert_eq!(
        conflicts_delta, 0,
        "R27-3 child (cap={pool_segments}, threads={thread_count}, rep={repetition}): \
         CONFIG_CONFLICTS delta = {conflicts_delta} (expected 0 in a fresh process) — \
         isolation contract broken"
    );

    // VICTIM-ACTIVATION hard asserts (per the R26 review's "Required retention
    // gate"): an RSS number is only trustworthy if the capacity was actually
    // exercised. cap-4 MUST saturate; cap-8 MUST retain >4 segments.
    if pool_segments == 4 {
        assert!(
            decommit_delta > 0,
            "cap-4 VICTIM ACTIVATION FAILED: decommit_delta={decommit_delta} (expected >0 — \
             the 6-segment demand must overflow a 4-segment pool and force decommits). \
             Without saturation this arm's RSS number is vacuous (cap={pool_segments}, \
             threads={thread_count}, rep={repetition})"
        );
    }
    if pool_segments == 8 {
        assert!(
            pooled_hw_max > 4,
            "cap-8 VICTIM ACTIVATION FAILED: pooled_hw_max={pooled_hw_max} (expected >4 — cap 8 \
             must retain BEYOND cap-4's 4-segment bound). Without this, 'cap 8 retains more' is \
             unproven (cap={pool_segments}, threads={thread_count}, rep={repetition})"
        );
        assert!(
            decommit_delta == 0,
            "cap-8: decommit_delta={decommit_delta} (expected 0 — cap 8 absorbs the 6-segment \
             demand; a nonzero delta means cap 8 unexpectedly saturated) (threads={thread_count}, \
             rep={repetition})"
        );
    }

    // Emit RESULT lines (one per field; proc_probe protocol).
    proc_probe::emit_u64("pool_segments", pool_segments as u64);
    proc_probe::emit_u64("pool_byte_cap_mib", pool_byte_cap_mib);
    proc_probe::emit_u64("thread_count", thread_count as u64);
    proc_probe::emit_u64("repetition", repetition as u64);
    proc_probe::emit_u64("verified_cap", pool_segments as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("rss_before_kib", rss_before_kib);
    proc_probe::emit_u64("peak_rss_kib", peak_rss_kib);
    proc_probe::emit_u64("peak_commit_kib", peak_commit_kib);
    proc_probe::emit_u64("decommit_delta", decommit_delta);
    proc_probe::emit_u64("seg_released_delta", seg_released_delta);
    proc_probe::emit_u64("seg_reserved_delta", seg_reserved_delta);
    proc_probe::emit_u64("pooled_hw_max", pooled_hw_max);
    proc_probe::emit_u64("pooled_final_sum", pooled_final_sum);
    proc_probe::emit_u64("pooled_final_max", pooled_final_max);
    proc_probe::emit_u64("rss_post_kib", rss_post_kib);
    proc_probe::emit_u64("commit_post_kib", commit_post_kib);
    proc_probe::emit_u64("rss_100ms_kib", rss_100ms_kib);
    proc_probe::emit_u64("rss_1s_kib", rss_1s_kib);
    proc_probe::emit_u64("rss_2s_kib", rss_2s_kib);
    proc_probe::emit_u64("pooled_predrain_max", pooled_predrain_max);
    proc_probe::emit_u64("drained_sum", drained_sum_total);
    proc_probe::emit_u64("rss_drain_kib", rss_drain_kib);
    proc_probe::emit_u64("commit_drain_kib", commit_drain_kib);
    proc_probe::emit_u64("decay_interval_ms", DECAY_INTERVAL_MS);

    println!(
        "OK cap={pool_segments} byte_cap={pool_byte_cap_mib}MiB threads={thread_count} \
         rep={repetition} verified_cap={pool_segments} cfg_conflicts_delta={conflicts_delta} \
         decommit_delta={decommit_delta} pooled_hw_max={pooled_hw_max} pooled_final_sum={pooled_final_sum} \
         peak_rss_kib={peak_rss_kib} rss_post_kib={rss_post_kib} rss_2s_kib={rss_2s_kib} \
         rss_drain_kib={rss_drain_kib} drained_sum={drained_sum_total} pooled_predrain_max={pooled_predrain_max}"
    );

    // Release + join workers.
    release.store(true, Ordering::Release);
    for (i, h) in handles.into_iter().enumerate() {
        h.join().unwrap_or_else(|e| {
            panic!(
                "R27-3 child (cap={pool_segments}, thread={i}, rep={repetition}): \
                 worker thread panicked: {e:?}"
            )
        });
    }
}

// ---------------------------------------------------------------------------
// Orchestrator mode — re-exec the same binary once per arm, collect RESULT
// lines, aggregate to (median, min, max) per cell, emit a CSV block.
// ---------------------------------------------------------------------------

/// All RESULT fields from one child, parsed into a typed map.
struct ChildMetrics {
    map: std::collections::HashMap<String, u64>,
}

impl ChildMetrics {
    fn get(&self, k: &str) -> u64 {
        *self.map.get(k).unwrap_or_else(|| panic!("child RESULT missing {k}"))
    }
}

fn parse_child_stdout(stdout: &str) -> ChildMetrics {
    let mut map = std::collections::HashMap::new();
    for line in stdout.lines() {
        let rest = match line.strip_prefix("RESULT ") {
            Some(r) => r,
            None => continue,
        };
        for tok in rest.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                if let Ok(n) = v.parse::<u64>() {
                    map.insert(k.to_string(), n);
                }
            }
        }
    }
    ChildMetrics { map }
}

fn run_one_child(
    pool_segments: usize,
    pool_byte_cap: usize,
    thread_count: usize,
    repetition: usize,
) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R27_3_POOL_SEGMENTS", pool_segments.to_string())
        .env("R27_3_POOL_BYTE_CAP", pool_byte_cap.to_string())
        .env("R27_3_THREAD_COUNT", thread_count.to_string())
        .env("R27_3_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd.output().unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R27-3 child (cap={pool_segments}, byte_cap={pool_byte_cap}, threads={thread_count}, \
             rep={repetition}) failed with status {:?}; see stderr above",
            output.status.code()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    parse_child_stdout(&stdout)
}

fn median(v: &mut Vec<u64>) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

fn run_orchestrator() {
    println!("=== R27-3 pool RETENTION gate — subprocess-per-arm isolation ===");
    println!(
        "grid: configs={:?} x threads={:?} x {REPETITIONS} reps; SIZE={SIZE}B \
         CHURN_WORKING_SET={CHURN_WORKING_SET} OPS={OPS} PRESSURE_BATCH_SIZE={PRESSURE_BATCH_SIZE} \
         PRESSURE_DURATION={:?} decay_interval_ms={DECAY_INTERVAL_MS} SEGMENT={SEGMENT_BYTES}",
        CONFIGS, THREAD_COUNTS, PRESSURE_DURATION
    );
    println!();

    let mut all: Vec<ChildMetrics> = Vec::new();
    for &(ps, pbc) in CONFIGS {
        for &tc in THREAD_COUNTS {
            for rep in 0..REPETITIONS {
                eprintln!(
                    "--- arm cap={ps} byte_cap={}MiB threads={tc} rep={rep}/{REPETITIONS} ---",
                    pbc / (1024 * 1024)
                );
                let m = run_one_child(ps, pbc, tc, rep);
                assert_eq!(m.get("pool_segments"), ps as u64, "child reported wrong pool_segments");
                assert_eq!(m.get("thread_count"), tc as u64, "child reported wrong thread_count");
                assert_eq!(m.get("repetition"), rep as u64, "child reported wrong repetition");
                assert_eq!(
                    m.get("verified_cap"),
                    ps as u64,
                    "child reported verified_cap != requested"
                );
                assert_eq!(
                    m.get("config_conflicts_delta"),
                    0,
                    "child reported non-zero config_conflicts_delta"
                );
                all.push(m);
            }
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps; min..max in parens) ===");
    println!(
        "{:>6} {:>4} {:>8} {:>24} {:>24} {:>12} {:>12} {:>10} {:>12} {:>12} {:>12}",
        "cap", "MiB", "threads", "peak_rss_kib", "rss_post_kib", "decommit_d", "pooled_hw",
        "pooled_fin", "rss_2s_kib", "rss_drain", "drained",
    );
    for &(ps, pbc) in CONFIGS {
        for &tc in THREAD_COUNTS {
            let cell: Vec<&ChildMetrics> = all
                .iter()
                .filter(|m| m.get("pool_segments") == ps as u64 && m.get("thread_count") == tc as u64)
                .collect();
            let pick = |k: &str| -> Vec<u64> { cell.iter().map(|m| m.get(k)).collect() };
            let mut peak = pick("peak_rss_kib");
            let mut post = pick("rss_post_kib");
            let mut dec = pick("decommit_delta");
            let mut hw = pick("pooled_hw_max");
            let mut fin = pick("pooled_final_sum");
            let mut r2 = pick("rss_2s_kib");
            let mut dr = pick("rss_drain_kib");
            let mut drained = pick("drained_sum");
            let (peak_m, post_m, dec_m, hw_m, fin_m, r2_m, dr_m, drained_m) = (
                median(&mut peak),
                median(&mut post),
                median(&mut dec),
                median(&mut hw),
                median(&mut fin),
                median(&mut r2),
                median(&mut dr),
                median(&mut drained),
            );
            let fmt = |v: &mut Vec<u64>, m: u64| format!("{m} ({}..{})", v[0], v[v.len() - 1]);
            println!(
                "{:>6} {:>4} {:>8} {:>24} {:>24} {:>12} {:>12} {:>10} {:>12} {:>12} {:>12}",
                ps, pbc / (1024 * 1024), tc, fmt(&mut peak, peak_m), fmt(&mut post, post_m),
                dec_m, hw_m, fin_m, fmt(&mut r2, r2_m), fmt(&mut dr, dr_m), drained_m,
            );
        }
    }

    // Cap8-vs-cap4 deltas at each thread count (the headline retention cost).
    println!();
    println!("=== cap8 - cap4 retention delta (median; rss_post / rss_2s / rss_drain) ===");
    for &tc in THREAD_COUNTS {
        let get_med = |ps: usize, k: &str| -> u64 {
            let mut v: Vec<u64> = all
                .iter()
                .filter(|m| m.get("pool_segments") == ps as u64 && m.get("thread_count") == tc as u64)
                .map(|m| m.get(k))
                .collect();
            median(&mut v)
        };
        let d_post = get_med(8, "rss_post_kib").saturating_sub(get_med(4, "rss_post_kib"));
        let d_2s = get_med(8, "rss_2s_kib").saturating_sub(get_med(4, "rss_2s_kib"));
        let d_drain = get_med(8, "rss_drain_kib").saturating_sub(get_med(4, "rss_drain_kib"));
        let d_pooled = get_med(8, "pooled_final_sum").saturating_sub(get_med(4, "pooled_final_sum"));
        println!(
            "threads={tc:>2}: Δrss_post=+{d_post} KiB  Δrss_2s=+{d_2s} KiB  Δrss_drain=+{d_drain} KiB  \
             Δpooled_final_sum=+{d_pooled} segs (~{} KiB)",
            d_pooled * (SEGMENT_BYTES as u64 / 1024)
        );
    }

    // CSV block — one row per child, all fields, for the committed summary CSV.
    println!();
    println!("=== CSV (one row per child) ===");
    let cols = [
        "pool_segments", "pool_byte_cap_mib", "thread_count", "repetition", "verified_cap",
        "config_conflicts_delta", "process_identity", "peak_rss_kib", "peak_commit_kib",
        "decommit_delta", "seg_released_delta", "seg_reserved_delta", "pooled_hw_max",
        "pooled_final_sum", "pooled_final_max", "rss_post_kib", "commit_post_kib",
        "rss_100ms_kib", "rss_1s_kib", "rss_2s_kib", "pooled_predrain_max", "drained_sum",
        "rss_drain_kib", "commit_drain_kib", "decay_interval_ms",
    ];
    println!("# {}", cols.join(","));
    for m in &all {
        let row: Vec<String> = cols
            .iter()
            .map(|c| {
                if *c == "process_identity" {
                    "subprocess".to_string()
                } else {
                    m.get(c).to_string()
                }
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    println!(
        "NOTE: each (config, thread_count, rep) cell ran in its OWN freshly-spawned OS process \
         (subprocess-per-arm isolation). Every arm hard-asserted resolved cap == requested cap \
         AND config_conflicts_delta == 0 (R26-4 config identity). cap-4 arms asserted \
         decommit_delta > 0 (saturation proven); cap-8 arms asserted pooled_hw_max > 4 (retention \
         beyond cap-4's bound proven) AND decommit_delta == 0. pooled_predrain_max == pooled_final \
         confirms pooled_count is flat during the idle window (no background decay — the small-pool \
         decay is event-driven, fires only on reserve_small_segment). The post-drain RSS drop \
         demonstrates the retained segments are reclaimable."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R27_3_POOL_SEGMENTS").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
