//! R26-1 (task #410) — the REMEASUREMENT of R25-5's invalidated RSS/commit
//! axis, with **one fresh OS process per `(pool_segments, thread_count,
//! repetition)` tuple** so the registry-slot reuse bug that invalidated R25-5
//! is structurally eliminated (not just made less likely).
//!
//! ## Why this exists
//!
//! `examples/r25_5_pool_cap_sweep_probe.rs::measure_rss_axis` swept
//! `pool_segments` = 4/8/16/32 at 1/8/32 threads **sequentially in one
//! `cargo run --release` process**, claiming a per-thread heap via
//! `SeferAlloc::with_config`. That is unsound for this registry:
//! `HeapRegistry::claim_with_config` (`src/registry/heap_registry.rs:209`) is
//! first-claim-wins for a slot's whole process lifetime — a re-claimed slot
//! keeps the config set at first materialisation, incrementing only the
//! `CONFIG_CONFLICTS` counter (`heap_registry.rs:263`) and arming a
//! `debug_assert!` (`heap_registry.rs:285`) that is **compiled OUT of
//! `--release`**; `recycle` (`heap_registry.rs:342`) returns slots to
//! `free_slots` on thread exit; `pick_slot` (`heap_registry.rs:316-322`) pops
//! recycled slots before minting fresh ones. So arm N+1's threads could silently
//! reuse arm N's already-configured slot and run under arm N's older cap. Rows
//! labelled cap=8/16/32 in R25-5's RSS table may have actually run under cap=4.
//! See `docs/perf/R25_5_POOL_CAP_SWEEP_GATE.md` §8 for the full analysis and
//! `docs/reviews/2026-07-28-r25-readonly-review.md`'s "Required correction"
//! for the recipe this probe implements.
//!
//! ## The fix — process isolation by construction
//!
//! If every `(pool_segments, thread_count, repetition)` combination runs in its
//! OWN freshly-spawned OS process, there is no possibility of registry-slot
//! reuse across arms: a fresh process has a fresh, empty `HeapRegistry`, so the
//! FIRST claim in that process is unconditionally the arm's own requested config.
//! This eliminates the bug by construction, not by better luck.
//!
//! ## Mode split — orchestrator vs child (same binary, re-exec'd)
//!
//! - **Orchestrator mode** (default, no env var set): iterates the 4×3 sweep
//!   grid × N repetitions, re-exec'ing THIS binary (via `std::env::current_exe`
//!   + `std::process::Command`) with env vars encoding the arm
//!     (`R26_1_POOL_SEGMENTS`, `R26_1_THREAD_COUNT`, `R26_1_REPETITION`), captures
//!     the child's stdout, and parses the one machine-readable `RESULT key=value`
//!     line per child run (this project's established probe protocol — see
//!     `crates/proc-probe/src/lib.rs`'s `RESULT` convention).
//! - **Child/arm mode** (the env vars present): spawns `thread_count` OS
//!   threads, each claiming its OWN heap via `HeapRegistry::claim_with_config`
//!   (NOT `SeferAlloc` — this sidesteps `SeferAlloc`'s private `current_heap()`
//!   and yields a raw `*mut HeapCore` whose `dbg_pool_cap()` is directly
//!   readable, following `examples/r13_9_class_aware_dirty_sidecar_rss.rs`'s
//!   `claim_heap_with_materialised_sidecar` precedent), runs the same batched
//!   churn workload as R25-5 (`churn_prefill`/`churn_step`/`churn_teardown`,
//!   copied byte-for-byte from `benches/global_alloc.rs` via R25-5) for
//!   `RSS_RUN_DURATION`, while a monitor thread polls `proc_probe::snapshot()`
//!   for peak RSS/commit. Before trusting this arm's RSS number, it HARD-asserts
//!   the resolved cap matches the requested one AND that this arm's
//!   `CONFIG_CONFLICTS` delta is exactly zero.
//!
//! ## Workload fidelity
//!
//! Identical to R25-5: `SIZE=1024`, `CHURN_WORKING_SET=256`, `OPS=1024`,
//! `LATENCY_BATCH_SIZE=120` (the batched-setup shape that reproduces criterion's
//! `iter_batched`/`SmallInput` concurrent-multi-segment pressure — see R25-5's
//! module doc "Critical fidelity detail"), `RSS_RUN_DURATION=1.5s`. The churn
//! primitives are byte-for-byte copies of `benches/global_alloc.rs`'s (a
//! `[[bench]]` binary, unreachable from an `[[example]]`), copied again here.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r26_1_pool_cap_rss_subprocess_probe --features "production alloc-stats bench-internals"
//! ```
//!
//! `alloc-decommit` (pulled in by `production`) is REQUIRED —
//! `HeapRegistry::claim_with_config`/`SmallSegmentPoolConfig` only exist under it.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "bench-internals"
))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    LargeCacheConfig, SmallSegmentPoolConfig,
};

// ---------------------------------------------------------------------------
// Sweep parameters — reuse R25-5's exact grid (the values the invalidated
// R25-5 RSS table was measured at, so corrected-vs-original comparison is
// apples-to-apples).
// ---------------------------------------------------------------------------

/// `pool_segments` = 4 (current `DEFAULT_POOL_SEGMENTS` baseline)/8/16/32 —
/// the four values R25-5 swept.
const POOL_SEGMENTS_SWEEP: &[usize] = &[4, 8, 16, 32];

/// Thread counts R25-5 measured: 1 / 8 / 32.
const THREAD_COUNTS: &[usize] = &[1, 8, 32];

/// Generous byte cap (256 MiB = 64 segments' worth) so `pool_segments` alone
/// constrains occupancy at every swept value — identical to R25-5 (which
/// mirrors `pool_cap_sweep_spread_and_drain`'s identical choice).
const GENEROUS_POOL_BYTE_CAP: usize = 256 * 1024 * 1024;

/// Repetitions per `(pool_segments, thread_count)` cell. 3 is the default per
/// this project's "Speed: short scenario by default" convention — enough to
/// surface obvious outliers and report median+range without taxing the
/// shared host. NOT silently capped below this.
const REPETITIONS: usize = 3;

const SIZE: usize = 1024;
const CHURN_WORKING_SET: usize = 256;
const OPS: usize = 1024;

/// Batch size for the RSS-axis worker loop (capped below the latency axis's
/// `LATENCY_BATCH_SIZE=120` to keep each worker iteration's alloc count modest
/// under high thread counts — same `.min(50)` clamp R25-5's `measure_rss_axis`
/// used at `examples/r25_5_pool_cap_sweep_probe.rs:456`).
const RSS_BATCH_SIZE: usize = 50;

const RSS_RUN_DURATION: Duration = Duration::from_millis(1500);
const RSS_POLL_INTERVAL: Duration = Duration::from_millis(20);

// ---------------------------------------------------------------------------
// Byte-for-byte copies of `benches/global_alloc.rs`'s churn primitives (copied
// via R25-5; see that file's "Workload fidelity" note — `benches/global_alloc.rs`
// is a `[[bench]]` binary, unreachable from an `[[example]]`).
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

/// `HeapCore`-flavored churn (R26-1 claims heaps via `HeapRegistry` and drives
/// `HeapCore::alloc`/`dealloc` directly, NOT through a `GlobalAlloc` impl —
/// identical in logic to R25-5's `ac_churn_*` set, which itself mirrors the
/// generic `churn_*` fns in `benches/global_alloc.rs`).
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

/// One prefill+churn+teardown cycle — the exact shape
/// `bench_global_alloc_churn_with_teardown`'s Sefer arm times per criterion
/// iteration.
fn hc_churn_with_teardown_cycle(heap: &mut HeapCore, layout: Layout) {
    let mut live = hc_churn_prefill(heap, layout, CHURN_WORKING_SET);
    hc_churn_step(heap, layout, &mut live, OPS);
    hc_churn_teardown(heap, layout, &live);
}

/// One criterion-`iter_batched`/`SmallInput`-shaped batch: collect `batch_size`
/// independent `hc_churn_prefill` outputs into a `Vec` UP FRONT (all
/// `batch_size * CHURN_WORKING_SET` blocks concurrently live at once), THEN run
/// `hc_churn_step`+`hc_churn_teardown` once per prefilled working set. Mirrors
/// R25-5's `run_latency_batch` / `ac_run_latency_batch` (which themselves
/// reproduce criterion's `inputs = (0..batch_size).map(|_|
/// setup()).collect::<Vec<_>>()` batching).
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

fn config_for(pool_segments: usize) -> LargeCacheConfig {
    LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(pool_segments)
            .pool_byte_cap(GENEROUS_POOL_BYTE_CAP),
    )
}

// ---------------------------------------------------------------------------
// Child/arm mode — run ONE `(pool_segments, thread_count)` arm in this fresh
// process, self-verify the resolved cap + zero CONFIG_CONFLICTS delta, then
// emit one `RESULT key=value ...` line.
// ---------------------------------------------------------------------------

/// Read `name` from the env, panic on missing/parse failure.
fn parse_env_usize(name: &str) -> usize {
    let raw = std::env::var(name)
        .unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"));
    raw.parse::<usize>()
        .unwrap_or_else(|e| panic!("{name}={raw:?} not a valid usize ({e})"))
}

struct ChildOutcome {
    pool_segments: usize,
    thread_count: usize,
    repetition: usize,
    /// Verified cap (must equal `pool_segments`). Forwarded into the RESULT
    /// line as the direct proof the arm actually used its labelled config.
    verified_cap: usize,
    /// `config_conflicts_total()` delta across this arm's run — must be 0
    /// (fresh process, first claim unconditionally wins). Forwarded as proof.
    config_conflicts_delta: u64,
    peak_rss_delta_kib: u64,
    peak_commit_delta_kib: u64,
    rss_before_kib: u64,
    rss_after_kib: u64,
}

fn run_child() -> ChildOutcome {
    let pool_segments = parse_env_usize("R26_1_POOL_SEGMENTS");
    let thread_count = parse_env_usize("R26_1_THREAD_COUNT");
    let repetition = parse_env_usize("R26_1_REPETITION");

    let config_conflicts_before = config_conflicts_total();

    let stop = Arc::new(AtomicBool::new(false));
    let ready_count = Arc::new(AtomicU64::new(0));
    // Collected per-worker `dbg_pool_cap()` readouts (each must equal
    // `pool_segments`) — workers stash their readout here so the main thread
    // can assert ALL of them after join.
    let caps: Arc<Vec<AtomicU64>> = Arc::new(
        (0..thread_count)
            .map(|_| AtomicU64::new(u64::MAX))
            .collect(),
    );

    // Take the RSS "before" snapshot AFTER the registry is bootstrapped but
    // BEFORE any worker claims a heap / allocates — the per-review recipe's
    // step 5 ("take the baseline only after process bootstrap and before
    // workload materialisation").
    let before = proc_probe::snapshot();
    let rss_before_kib = before.rss / 1024;
    let commit_before_kib = before.commit / 1024;

    let mut handles = Vec::with_capacity(thread_count);
    for i in 0..thread_count {
        let stop = Arc::clone(&stop);
        let ready_count = Arc::clone(&ready_count);
        let caps = Arc::clone(&caps);
        handles.push(thread::spawn(move || {
            // Each thread claims its OWN heap via `HeapRegistry::claim_with_config`
            // — NOT `SeferAlloc` (which would hide the HeapCore behind private
            // TLS). Raw `*mut HeapCore` is the established pattern from
            // `r13_9_class_aware_dirty_sidecar_rss.rs::claim_heap_with_materialised_sidecar`.
            let heap_ptr = HeapRegistry::claim_with_config(config_for(pool_segments));
            assert!(
                !heap_ptr.is_null(),
                "HeapRegistry::claim_with_config returned null at thread {i}"
            );
            // SAFETY: `heap_ptr` was just returned by `claim_with_config` and is
            // owned by THIS thread until we `recycle` it below. No other thread
            // holds a reference to this slot's HeapCore.
            let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

            // === SELF-VERIFICATION #1: resolved cap equals the requested one ===
            // This is the direct proof R25-5's RSS axis lacked. A panic here
            // (rather than a soft log) means the isolation contract is broken
            // and the arm's number is untrustworthy — surfacing it loudly is
            // the point.
            let resolved = heap.dbg_pool_cap();
            assert_eq!(
                resolved, pool_segments,
                "R26-1 child (pool_segments={pool_segments}, thread={i}, rep={repetition}): \
                 resolved cap ({resolved}) != requested ({pool_segments}) — isolation contract \
                 broken, this arm's RSS number is untrustworthy"
            );
            caps[i].store(resolved as u64, Ordering::Release);

            let layout = Layout::from_size_align(SIZE, 8).unwrap();
            // One warm-up cycle before signaling ready.
            hc_churn_with_teardown_cycle(heap, layout);
            ready_count.fetch_add(1, Ordering::Release);
            // Sustained batched churn — same shape R25-5's RSS axis used.
            while !stop.load(Ordering::Relaxed) {
                hc_run_batch(heap, layout, RSS_BATCH_SIZE);
            }

            // Recycle so the slot returns to `free_slots` cleanly on thread exit
            // (mirrors `r13_9_class_aware_dirty_sidecar_rss.rs:266`'s
            // `unsafe { HeapRegistry::recycle(h) }` pattern). In a fresh
            // single-arm process this is cosmetic (no later arm will reuse the
            // slot), but keeping the established claim/recycle discipline makes
            // the worker identical in shape to production usage.
            // SAFETY: `heap_ptr` was returned by `claim_with_config` above and
            // has not yet been recycled — exactly the contract `recycle`
            // requires.
            unsafe { HeapRegistry::recycle(heap_ptr) };
        }));
    }

    // Wait for every worker to have claimed + warmed + self-verified its cap.
    while ready_count.load(Ordering::Acquire) < thread_count as u64 {
        thread::sleep(Duration::from_millis(1));
    }

    // Monitor thread: poll peak RSS/commit over the run window.
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
    for (i, h) in handles.into_iter().enumerate() {
        h.join().unwrap_or_else(|e| {
            panic!(
                "R26-1 child (pool_segments={pool_segments}, thread={i}, rep={repetition}): \
                 worker thread panicked: {e:?}"
            )
        });
    }

    let after = proc_probe::snapshot();
    let rss_after_kib = after.rss / 1024;

    // === SELF-VERIFICATION #1 (post-join recheck): every worker's stashed
    // resolved cap equals the requested value. (The per-worker assert above
    // already panicked on mismatch, so reaching here means all passed — but
    // re-checking the collected readouts is cheap and makes the "every thread,
    // not just the last" property explicit in the RESULT line.) ===
    let mut verified_cap = pool_segments;
    for (i, slot) in caps.iter().enumerate() {
        let v = slot.load(Ordering::Acquire);
        assert_eq!(
            v, pool_segments as u64,
            "R26-1 child: worker {i} stashed resolved cap {v}, expected {pool_segments}"
        );
        verified_cap = v as usize;
    }

    // === SELF-VERIFICATION #2: CONFIG_CONFLICTS delta across this arm is 0 ===
    // Fresh process ⇒ the first claim in each slot is unconditionally the arm's
    // config ⇒ no conflict possible. A non-zero delta would indicate a genuine
    // regression (e.g. slot reuse inside one process) worth catching loudly.
    let config_conflicts_delta = config_conflicts_total().saturating_sub(config_conflicts_before);
    assert_eq!(
        config_conflicts_delta, 0,
        "R26-1 child (pool_segments={pool_segments}, threads={thread_count}, rep={repetition}): \
         CONFIG_CONFLICTS delta = {config_conflicts_delta} (expected 0 in a fresh process) — \
         isolation contract broken"
    );

    ChildOutcome {
        pool_segments,
        thread_count,
        repetition,
        verified_cap,
        config_conflicts_delta,
        peak_rss_delta_kib: peak_rss_kib.saturating_sub(rss_before_kib),
        peak_commit_delta_kib: peak_commit_kib.saturating_sub(commit_before_kib),
        rss_before_kib,
        rss_after_kib,
    }
}

// ---------------------------------------------------------------------------
// Orchestrator mode — re-exec the same binary once per arm, collect RESULT
// lines, aggregate to (median, min, max) per cell.
// ---------------------------------------------------------------------------

/// Parse the child's stdout for the single `RESULT key=value ...` line and the
/// trailing `OK <hex>...` self-checks; extract the numeric metrics.
struct ChildMetrics {
    pool_segments: usize,
    thread_count: usize,
    repetition: usize,
    verified_cap: usize,
    config_conflicts_delta: u64,
    peak_rss_delta_kib: u64,
    peak_commit_delta_kib: u64,
}

fn parse_child_stdout(stdout: &str) -> ChildMetrics {
    let mut pool_segments = None;
    let mut thread_count = None;
    let mut repetition = None;
    let mut verified_cap = None;
    let mut config_conflicts_delta = None;
    let mut peak_rss_delta_kib = None;
    let mut peak_commit_delta_kib = None;
    for line in stdout.lines() {
        let rest = match line.strip_prefix("RESULT ") {
            Some(r) => r,
            None => continue,
        };
        for tok in rest.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                let val: u64 = match v.parse() {
                    Ok(n) => n,
                    Err(_) => continue,
                };
                match k {
                    "pool_segments" => pool_segments = Some(val as usize),
                    "thread_count" => thread_count = Some(val as usize),
                    "repetition" => repetition = Some(val as usize),
                    "verified_cap" => verified_cap = Some(val as usize),
                    "config_conflicts_delta" => config_conflicts_delta = Some(val),
                    "peak_rss_delta_kib" => peak_rss_delta_kib = Some(val),
                    "peak_commit_delta_kib" => peak_commit_delta_kib = Some(val),
                    _ => {}
                }
            }
        }
    }
    ChildMetrics {
        pool_segments: pool_segments.expect("child RESULT missing pool_segments"),
        thread_count: thread_count.expect("child RESULT missing thread_count"),
        repetition: repetition.expect("child RESULT missing repetition"),
        verified_cap: verified_cap.expect("child RESULT missing verified_cap"),
        config_conflicts_delta: config_conflicts_delta
            .expect("child RESULT missing config_conflicts_delta"),
        peak_rss_delta_kib: peak_rss_delta_kib.expect("child RESULT missing peak_rss_delta_kib"),
        peak_commit_delta_kib: peak_commit_delta_kib
            .expect("child RESULT missing peak_commit_delta_kib"),
    }
}

fn run_one_child(pool_segments: usize, thread_count: usize, repetition: usize) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R26_1_POOL_SEGMENTS", pool_segments.to_string())
        .env("R26_1_THREAD_COUNT", thread_count.to_string())
        .env("R26_1_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    // Inherit the parent's features implicitly — re-exec'ing the SAME binary
    // means the SAME `--features` it was compiled with are baked in; no env
    // reconfiguration needed.
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R26-1 child (pool_segments={pool_segments}, threads={thread_count}, rep={repetition}) \
             failed with status {:?}; see stderr above",
            output.status.code()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Echo child stdout so the raw log carries everything the child printed.
    print!("{stdout}");
    parse_child_stdout(&stdout)
}

fn run_orchestrator() {
    println!("=== R26-1 pool_segments RSS sweep — subprocess-per-arm isolation ===");
    println!(
        "grid: pool_segments in {:?} x threads in {:?} x {REPETITIONS} reps; SIZE={SIZE}B  \
         CHURN_WORKING_SET={CHURN_WORKING_SET}  OPS={OPS}  RSS_RUN_DURATION={:?}  \
         RSS_BATCH_SIZE={RSS_BATCH_SIZE}",
        POOL_SEGMENTS_SWEEP, THREAD_COUNTS, RSS_RUN_DURATION
    );
    println!();

    // Collect every child's metrics, then aggregate per cell.
    let mut all: Vec<ChildMetrics> = Vec::new();
    for &ps in POOL_SEGMENTS_SWEEP {
        for &tc in THREAD_COUNTS {
            for rep in 0..REPETITIONS {
                eprintln!("--- arm pool_segments={ps} threads={tc} rep={rep}/{REPETITIONS} ---");
                let m = run_one_child(ps, tc, rep);
                // Sanity: the child must self-report the cell it was asked to run.
                assert_eq!(m.pool_segments, ps, "child reported wrong pool_segments");
                assert_eq!(m.thread_count, tc, "child reported wrong thread_count");
                assert_eq!(m.repetition, rep, "child reported wrong repetition");
                // Cross-check: the verified cap the child stashed must equal ps
                // (the child already hard-asserted this internally, but
                // re-checking in the orchestrator is cheap and documents the
                // contract in the raw log).
                assert_eq!(
                    m.verified_cap, ps,
                    "child reported verified_cap != requested pool_segments"
                );
                assert_eq!(
                    m.config_conflicts_delta, 0,
                    "child reported non-zero config_conflicts_delta"
                );
                all.push(m);
            }
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps; min..max in parens) ===");
    println!(
        "{:>14} {:>10} {:>22} {:>22} {:>14} {:>14}",
        "pool_segments",
        "threads",
        "peak_rss_delta_kib",
        "peak_commit_delta_kib",
        "verified_cap",
        "cfg_conflicts",
    );
    for &ps in POOL_SEGMENTS_SWEEP {
        for &tc in THREAD_COUNTS {
            let cell: Vec<&ChildMetrics> = all
                .iter()
                .filter(|m| m.pool_segments == ps && m.thread_count == tc)
                .collect();
            let mut rss: Vec<u64> = cell.iter().map(|m| m.peak_rss_delta_kib).collect();
            let mut commit: Vec<u64> = cell.iter().map(|m| m.peak_commit_delta_kib).collect();
            rss.sort_unstable();
            commit.sort_unstable();
            let n = rss.len();
            let median = |v: &Vec<u64>| v[n / 2];
            let rss_med = median(&rss);
            let commit_med = median(&commit);
            let rss_fmt = format!("{rss_med} ({}..{})", rss[0], rss[n - 1]);
            let commit_fmt = format!("{commit_med} ({}..{})", commit[0], commit[n - 1]);
            println!(
                "{:>14} {:>10} {:>22} {:>22} {:>14} {:>14}",
                ps, tc, rss_fmt, commit_fmt, ps, 0,
            );
            // Emit the medians as RESULT lines too, so the raw log is
            // machine-grepable for the aggregated verdict.
            proc_probe::emit_u64(
                &format!("agg_peak_rss_delta_kib_pool{ps}_threads{tc}"),
                rss_med,
            );
            proc_probe::emit_u64(
                &format!("agg_peak_commit_delta_kib_pool{ps}_threads{tc}"),
                commit_med,
            );
            proc_probe::emit_u64(&format!("agg_rss_min_pool{ps}_threads{tc}"), rss[0]);
            proc_probe::emit_u64(&format!("agg_rss_max_pool{ps}_threads{tc}"), rss[n - 1]);
        }
    }
    println!();
    println!(
        "NOTE: each (pool_segments, thread_count, rep) cell ran in its OWN freshly-spawned \
         OS process (subprocess-per-arm isolation), eliminating the registry-slot-reuse bug \
         that invalidated R25-5's RSS axis. Every arm hard-asserted resolved cap == requested \
         cap AND config_conflicts_delta == 0 before its number was trusted (see each child's \
         own output above). Median is the reported figure; (min..max) across {REPETITIONS} reps \
         shows the spread. RSS/commit are process-wide deltas on a shared Windows host (noisy \
         point estimates) but the cap/conflicts self-checks are exact."
    );
}

// ---------------------------------------------------------------------------
// main — dispatch on env vars.
// ---------------------------------------------------------------------------

fn main() {
    let _ = bootstrap::ensure();

    let child_mode = std::env::var_os("R26_1_POOL_SEGMENTS").is_some();
    if child_mode {
        let out = run_child();
        // Emit exactly one RESULT line carrying every metric + both self-checks
        // so the orchestrator's parser and the raw log both carry the evidence.
        proc_probe::emit("pool_segments", &out.pool_segments.to_string());
        proc_probe::emit_u64("thread_count", out.thread_count as u64);
        proc_probe::emit_u64("repetition", out.repetition as u64);
        proc_probe::emit_u64("verified_cap", out.verified_cap as u64);
        proc_probe::emit_u64("config_conflicts_delta", out.config_conflicts_delta);
        proc_probe::emit_u64("peak_rss_delta_kib", out.peak_rss_delta_kib);
        proc_probe::emit_u64("peak_commit_delta_kib", out.peak_commit_delta_kib);
        proc_probe::emit_u64("rss_before_kib", out.rss_before_kib);
        proc_probe::emit_u64("rss_after_kib", out.rss_after_kib);
        // One combined human-readable self-check summary line for the raw log.
        println!(
            "OK pool_segments={} threads={} rep={} verified_cap={} cfg_conflicts_delta={} \
             peak_rss_delta_kib={} peak_commit_delta_kib={}",
            out.pool_segments,
            out.thread_count,
            out.repetition,
            out.verified_cap,
            out.config_conflicts_delta,
            out.peak_rss_delta_kib,
            out.peak_commit_delta_kib,
        );
        return;
    }

    run_orchestrator();
}
