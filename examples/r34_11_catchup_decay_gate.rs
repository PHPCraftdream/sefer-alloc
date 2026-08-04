//! R34-11 (task #530) — catch-up decay gate: measures BOTH the cost fix
//! (sparse-gap closed/reduced) AND the benefit preservation (R32-8's ~61%
//! stride-clock-read win unchanged) in ONE report, per CLAUDE.md's same-regime
//! rule (R31-1: cost and benefit in the SAME workload regime — not combined
//! into one Pareto claim, but measured side-by-side so neither is assumed).
//!
//! ## What changed (R34-11 code change)
//!
//! R34-10 (`docs/perf/R34_10_SPARSE_DECAY_GATE.md`) found that
//! `DECAY_CLOCK_CHECK_STRIDE = 64` (R32-8) lets many decay intervals elapse
//! between clock reads in sparse-traffic regimes, but `run_decay_step` fired
//! only ONE step per read — so the retention gap accumulated to 4 segments
//! and persisted for 95% of the run. R34-11 adds a **bounded catch-up loop**
//! (`DECAY_CATCHUP_MAX_STEPS = 8`, `src/alloc_core/alloc_core_large_cache.rs`):
//! once the clock IS read and the interval has elapsed, fire as many steps as
//! intervals are due (capped at 8). This does NOT change when the clock is
//! read (the stride throttle is untouched → R32-8's benefit is preserved),
//! only how many decay steps fire once it is.
//!
//! ## Two regimes, one report (NOT a Pareto claim)
//!
//! 1. **Sparse regime (cost/fix check):** the SAME allocfree matrix R34-10
//!    used — events {1,2,4,8} × 40 consecutive intervals × 2 arms
//!    (throttled stride=64 vs unthrottled stride=1 via
//!    `FORCE_DECAY_CLOCK_READ`). Confirms the catch-up loop closes/reduces
//!    the 4-segment gap R34-10 measured.
//! 2. **Throughput regime (benefit check):** 200K alloc+free cycles × 2 arms
//!    (throttled vs unthrottled), the SAME methodology as R32-8's stride-fix
//!    gate (`examples/r32_8_large_cache_decay_stride_fix_gate.rs`). Confirms
//!    the ~61% ns/cycle benefit is preserved (the catch-up loop is never
//!    reached in high-throughput because elapsed < interval on every read).
//!
//! These are two INDEPENDENT results, not a combined "small cost, big benefit"
//! Pareto claim (per CLAUDE.md R31-1).
//!
//! ## Path-activation oracle (R30-8 rule)
//!
//! Sparse regime, three pieces per child (all asserted by the derive script):
//! 1. **Headroom crossed:** `used_baseline > headroom_bytes` (the stride
//!    applies).
//! 2. **Unthrottled arm read the clock:** `guard_passed_delta ≥ 1`.
//! 3. **Catch-up active (the fix's mechanism):** throttled
//!    `released_delta > guard_passed_delta` when `guard_passed_delta` is small
//!    (proves MORE than one step fired per clock read — the catch-up loop is
//!    actually running, not just compiled in).
//!
//! Throughput regime:
//! - `stayed_above_headroom`: used > headroom throughout (the workload
//!   genuinely exercises the past-headroom path).
//! - `guard_passed_delta` matches arm expectation: unthrottled (forced) ==
//!   expected_calls; throttled << expected_calls (stride reduces reads).
//!
//! ## Config-resolution evidence (R26-4 rule)
//!
//! Every child self-verifies `verified_headroom == HEADROOM_BYTES` AND
//! `config_conflicts_delta == 0` (fresh subprocess ⇒ first claim is
//! unconditionally the arm's config).
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r34_11_catchup_decay_gate --features "production alloc-stats bench-internals internals"
//! ```

#![cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    AllocCore, LargeCacheConfig,
};

// ---------------------------------------------------------------------------
// Shared constants
// ---------------------------------------------------------------------------

/// Mirrors `DECAY_CLOCK_CHECK_STRIDE` in `alloc_core_large_cache.rs`.
const DECAY_CLOCK_CHECK_STRIDE: u32 = 64;

/// Mirrors `DECAY_CATCHUP_MAX_STEPS` in `alloc_core_large_cache.rs` (R34-11).
const DECAY_CATCHUP_MAX_STEPS: u32 = 8;

// ---------------------------------------------------------------------------
// Sparse regime constants (from R34-10)
// ---------------------------------------------------------------------------

const DECAY_INTERVAL_MS: u64 = 100;
const INTERVAL_WAIT_MS: u64 = 150;
const HEADROOM_BYTES: usize = 16 * 1024 * 1024;
const OBJ_BYTES: usize = 2 * 1024 * 1024;
const SLOTS: usize = 8;
const INTERVALS: usize = 40;
const EVENTS_ARMS: &[usize] = &[1, 2, 4, 8];

// ---------------------------------------------------------------------------
// Throughput regime constants (from R32-8 stride-fix gate)
// ---------------------------------------------------------------------------

/// Small headroom so the workload's resident cache genuinely and persistently
/// exceeds it (the LowHeadroom/Trimmed64MiB regime the stride fix targets).
const TP_HEADROOM_BYTES: usize = 64 * 1024;

/// Large object size — > TP_HEADROOM_BYTES so the cache stays above headroom.
const TP_LARGE_OBJ_BYTES: usize = 512 * 1024;

const TP_CYCLES: usize = 200_000;
const TP_WARMUP_CYCLES: usize = 64;
const TP_REPETITIONS: usize = 7;

// ---------------------------------------------------------------------------
// Workload helpers (shared)
// ---------------------------------------------------------------------------

/// # Safety
///
/// `p` must be a valid allocation of at least `size` bytes, not yet freed.
unsafe fn touch_pages(p: *mut u8, size: usize) {
    let page = 4096usize;
    let mut off = 0usize;
    while off < size {
        p.add(off).write_volatile(0xAB);
        off += page;
    }
}

// ---------------------------------------------------------------------------
// Child mode dispatch
// ---------------------------------------------------------------------------

fn parse_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|e| panic!("{name} env var required ({e})"))
}

fn parse_env_usize(name: &str) -> usize {
    parse_env(name)
        .parse::<usize>()
        .unwrap_or_else(|e| panic!("{name} not usize ({e})"))
}

fn parse_env_bool(name: &str) -> bool {
    std::env::var(name).map(|v| v == "1").unwrap_or(false)
}

fn snapshot_rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

// ===========================================================================
// SPARSE REGIME CHILD
// ===========================================================================

fn run_sparse_child() {
    let events = parse_env_usize("R34_11_EVENTS");
    let arm = parse_env("R34_11_ARM"); // "throttled" | "unthrottled"
    let intervals = parse_env_usize("R34_11_INTERVALS");
    let forced = arm == "unthrottled";

    let conflicts_before = config_conflicts_total();

    let heap_ptr = HeapRegistry::claim_with_config(
        LargeCacheConfig::new()
            .headroom_bytes(HEADROOM_BYTES)
            .decay_interval_ms(DECAY_INTERVAL_MS as u32),
    );
    assert!(!heap_ptr.is_null(), "claim_with_config returned null");
    // SAFETY: `heap_ptr` returned by `claim_with_config`, owned by this thread.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    let (rate_bp, resolved_interval, resolved_headroom) = heap.dbg_decay_config();
    assert_eq!(
        resolved_headroom, HEADROOM_BYTES,
        "resolved headroom {resolved_headroom} != requested {HEADROOM_BYTES}"
    );
    assert_eq!(
        resolved_interval, DECAY_INTERVAL_MS,
        "resolved interval {resolved_interval} != requested {DECAY_INTERVAL_MS}"
    );

    let layout = Layout::from_size_align(OBJ_BYTES, 8).unwrap();

    // Pre-fill: allocate 8 objects, free all 8 → cache = 8 slots (32 MiB).
    let mut live = Vec::with_capacity(SLOTS);
    for _ in 0..SLOTS {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "prefill alloc failed");
        // SAFETY: freshly allocated, not yet freed.
        unsafe { touch_pages(p, layout.size()) };
        live.push(p);
    }
    // SAFETY: all SLOTS pointers allocated by `heap` with `layout`.
    for &p in &live {
        // SAFETY: each pointer allocated by `heap`, freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    drop(live);

    let used_baseline = heap.dbg_large_cache_used();
    let guard_passed_baseline = AllocCore::dbg_maybe_decay_guard_passed_count();
    let released_baseline = AllocCore::dbg_segments_released_total();
    let rss_baseline_kib = snapshot_rss_kib();

    let headroom_crossed = used_baseline > HEADROOM_BYTES;
    assert!(
        headroom_crossed,
        "workload precondition: used_baseline={used_baseline} must exceed HEADROOM_BYTES={HEADROOM_BYTES}"
    );

    AllocCore::dbg_set_force_decay_clock_read(forced);

    for i in 0..intervals {
        thread::sleep(Duration::from_millis(INTERVAL_WAIT_MS));

        for _ in 0..events {
            let p = heap.alloc(layout);
            assert!(!p.is_null(), "alloc failed");
            // SAFETY: freshly allocated by `heap` with `layout`.
            unsafe { heap.dealloc(p, layout) };
        }

        let used_post = heap.dbg_large_cache_used();
        let guard_passed_cum = AllocCore::dbg_maybe_decay_guard_passed_count();
        let released_cum = AllocCore::dbg_segments_released_total();
        let rss_kib = snapshot_rss_kib();

        println!(
            "RESULT sparse_ts=1 interval={i} events={events} arm={arm} \
             used_post={used_post} guard_passed_cum={guard_passed_cum} \
             released_cum={released_cum} rss_kib={rss_kib}"
        );
    }

    AllocCore::dbg_set_force_decay_clock_read(false);

    let guard_passed_delta =
        AllocCore::dbg_maybe_decay_guard_passed_count().saturating_sub(guard_passed_baseline);
    let released_delta = AllocCore::dbg_segments_released_total().saturating_sub(released_baseline);

    let unthrottled_read = if forced {
        guard_passed_delta >= 1
    } else {
        true
    };

    // Catch-up oracle: the throttled arm released MORE segments than it read
    // the clock (released_delta > guard_passed_delta), proving the catch-up
    // loop fired multiple steps per clock read. For the unthrottled arm, this
    // is trivially true (reads every call), so we only assert it for throttled.
    let catchup_active = if !forced {
        released_delta > guard_passed_delta
    } else {
        true
    };

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);

    // SAFETY: `heap_ptr` returned by `claim_with_config`, not yet recycled.
    unsafe { HeapRegistry::recycle(heap_ptr) };

    let oracle_pass =
        headroom_crossed && unthrottled_read && catchup_active && conflicts_delta == 0;

    println!(
        "RESULT sparse_config=1 events={events} arm={arm} intervals={intervals} \
         headroom_bytes={HEADROOM_BYTES} obj_bytes={OBJ_BYTES} \
         decay_interval_ms={DECAY_INTERVAL_MS} decay_rate_bp={rate_bp} \
         stride={DECAY_CLOCK_CHECK_STRIDE} catchup_max={DECAY_CATCHUP_MAX_STEPS} \
         slots={SLOTS} verified_headroom={resolved_headroom} \
         verified_interval_ms={resolved_interval} config_conflicts_delta={conflicts_delta} \
         used_baseline={used_baseline} guard_passed_baseline={guard_passed_baseline} \
         released_baseline={released_baseline} rss_baseline_kib={rss_baseline_kib} \
         guard_passed_delta={guard_passed_delta} released_delta={released_delta} \
         headroom_crossed={} unthrottled_read={} catchup_active={} \
         process_identity=subprocess",
        u64::from(headroom_crossed),
        u64::from(unthrottled_read),
        u64::from(catchup_active),
    );

    println!(
        "RESULT sparse_oracle=1 events={events} arm={arm} oracle_pass={}",
        u64::from(oracle_pass),
    );
}

// ===========================================================================
// THROUGHPUT REGIME CHILD
// ===========================================================================

fn run_throughput_child() {
    let forced = parse_env_bool("R34_11_FORCE");
    let rep = parse_env_usize("R34_11_REP");

    let conflicts_before = config_conflicts_total();

    let heap_ptr =
        HeapRegistry::claim_with_config(LargeCacheConfig::new().headroom_bytes(TP_HEADROOM_BYTES));
    assert!(!heap_ptr.is_null(), "claim_with_config returned null");
    // SAFETY: `heap_ptr` returned by `claim_with_config`, owned by this thread.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    let (_, _, resolved) = heap.dbg_decay_config();
    assert_eq!(
        resolved, TP_HEADROOM_BYTES,
        "resolved headroom {resolved} != requested {TP_HEADROOM_BYTES}"
    );

    AllocCore::dbg_set_force_decay_clock_read(forced);

    let layout = Layout::from_size_align(TP_LARGE_OBJ_BYTES, 8).unwrap();

    for _ in 0..TP_WARMUP_CYCLES {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "warmup alloc failed");
        // SAFETY: freshly allocated, freed once.
        unsafe { heap.dealloc(p, layout) };
    }

    let used_before_timed = heap.dbg_large_cache_used();
    assert!(
        used_before_timed > TP_HEADROOM_BYTES,
        "workload precondition: used_before_timed={used_before_timed} must exceed \
         TP_HEADROOM_BYTES={TP_HEADROOM_BYTES}"
    );

    let guard_passed_before = AllocCore::dbg_maybe_decay_guard_passed_count();

    let t0 = Instant::now();
    for _ in 0..TP_CYCLES {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "alloc failed");
        // SAFETY: freshly allocated by `heap` with `layout`, freed once.
        unsafe { heap.dealloc(p, layout) };
    }
    let elapsed_ns = t0.elapsed().as_nanos() as u64;

    let guard_passed_after = AllocCore::dbg_maybe_decay_guard_passed_count();
    let guard_passed_delta = guard_passed_after.saturating_sub(guard_passed_before);
    let used_after_timed = heap.dbg_large_cache_used();
    let expected_calls = (TP_CYCLES * 2) as u64;

    AllocCore::dbg_set_force_decay_clock_read(false);

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);
    assert_eq!(
        conflicts_delta, 0,
        "throughput child (forced={forced}): config_conflicts delta = {conflicts_delta}"
    );

    let stayed_above_headroom = used_after_timed > TP_HEADROOM_BYTES;
    let oracle_pass = if forced {
        stayed_above_headroom && guard_passed_delta == expected_calls
    } else {
        stayed_above_headroom && guard_passed_delta > 0 && guard_passed_delta < expected_calls / 4
    };

    // SAFETY: `heap_ptr` returned by `claim_with_config`, not yet recycled.
    unsafe { HeapRegistry::recycle(heap_ptr) };

    let ns_per_cycle = elapsed_ns as f64 / TP_CYCLES as f64;

    println!(
        "RESULT throughput_ts=1 forced={} rep={rep} \
         headroom_bytes={TP_HEADROOM_BYTES} verified_headroom={resolved} \
         config_conflicts_delta={conflicts_delta} large_obj_bytes={TP_LARGE_OBJ_BYTES} \
         cycles={TP_CYCLES} used_before_timed={used_before_timed} \
         used_after_timed={used_after_timed} stayed_above_headroom={} \
         guard_passed_delta={guard_passed_delta} expected_calls={expected_calls} \
         elapsed_ns={elapsed_ns} ns_per_cycle={ns_per_cycle:.4} \
         stride={DECAY_CLOCK_CHECK_STRIDE} catchup_max={DECAY_CATCHUP_MAX_STEPS} \
         oracle_pass={} process_identity=subprocess",
        u64::from(forced),
        u64::from(stayed_above_headroom),
        u64::from(oracle_pass),
    );
}

// ===========================================================================
// Orchestrator
// ===========================================================================

fn run_one_child(envs: &[(&str, String)]) {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!("child failed: {:?}; envs={:?}", output.status.code(), envs);
    }
    print!("{}", String::from_utf8_lossy(&output.stdout));
}

fn run_orchestrator() {
    println!(
        "=== R34-11 catch-up decay gate — stride={DECAY_CLOCK_CHECK_STRIDE} \
         catchup_max={DECAY_CATCHUP_MAX_STEPS} ==="
    );
    println!();

    // ── Phase 1: sparse regime ──────────────────────────────────────────────
    println!(
        "--- SPARSE REGIME: allocfree, events {{1,2,4,8}} × {INTERVALS} intervals × 2 arms ---"
    );
    println!(
        "headroom={HEADROOM_BYTES} obj_bytes={OBJ_BYTES} \
         decay_interval_ms={DECAY_INTERVAL_MS} wait_ms={INTERVAL_WAIT_MS} slots={SLOTS}"
    );
    println!();

    for &events in EVENTS_ARMS {
        for arm in ["throttled", "unthrottled"] {
            eprintln!("--- sparse events={events} arm={arm} ---");
            run_one_child(&[
                ("R34_11_MODE", "sparse".to_string()),
                ("R34_11_EVENTS", events.to_string()),
                ("R34_11_ARM", arm.to_string()),
                ("R34_11_INTERVALS", INTERVALS.to_string()),
            ]);
        }
    }

    println!();
    println!("--- THROUGHPUT REGIME: {TP_CYCLES} cycles × 2 arms × {TP_REPETITIONS} reps ---");
    println!(
        "headroom={TP_HEADROOM_BYTES} large_obj_bytes={TP_LARGE_OBJ_BYTES} \
         warmup={TP_WARMUP_CYCLES}"
    );
    println!();

    // ── Phase 2: throughput regime ──────────────────────────────────────────
    for &forced in &[false, true] {
        for rep in 0..TP_REPETITIONS {
            eprintln!("--- throughput forced={forced} rep={rep}/{TP_REPETITIONS} ---");
            run_one_child(&[
                ("R34_11_MODE", "throughput".to_string()),
                ("R34_11_FORCE", if forced { "1" } else { "0" }.to_string()),
                ("R34_11_REP", rep.to_string()),
            ]);
        }
    }

    println!();
    println!(
        "=== all children complete; raw per-sample data above is what the derive \
         script (scripts/r34_11_catchup_decay_summary.mjs) turns into the summary \
         CSV + report tables ==="
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let mode = std::env::var_os("R34_11_MODE");
    match mode.as_deref() {
        Some(os_str) if os_str == "sparse" => run_sparse_child(),
        Some(os_str) if os_str == "throughput" => run_throughput_child(),
        _ => run_orchestrator(),
    }
}
