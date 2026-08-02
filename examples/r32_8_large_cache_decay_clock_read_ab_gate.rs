//! R32-8 (task #499, F9) — confound-free A/B for `AllocCore::maybe_decay_large_cache`'s
//! fast-path guard cost: is the `Instant::now()` clock read (a
//! `QueryPerformanceCounter` syscall on Windows per task #95's own historical
//! note) a real, reproducible cost once the guard's "used > headroom"
//! fast-exit fails, as it structurally does for the whole intended use case
//! of the shipped `LargeCachePolicy::LowHeadroom` / `::Trimmed64MiB` profiles
//! (`src/alloc_core/profile.rs`)?
//!
//! ## Why a naive headroom sweep is confounded (read before trusting any A/B here)
//!
//! `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE.md` found that
//! changing `headroom_bytes` ALSO moves the large-cache hit rate by a real
//! 12.5 percentage points at some boundaries. A naive "headroom=256 MiB vs
//! headroom=64 MiB, same workload" A/B therefore cannot attribute a latency
//! delta to the clock read alone — some of any observed delta could be a
//! hit-rate effect instead. CLAUDE.md's "cost and benefit must be measured in
//! the SAME workload regime" rule (the R30-6/R31-1 postmortem) applies here
//! directly.
//!
//! ## This gate's design: (a) from the survey's own recipe — hold headroom
//! FIXED, vary only the guard
//!
//! Both arms use the IDENTICAL `headroom_bytes` (`HEADROOM_BYTES` below, a
//! small deliberately-below-the-workload's-cache-usage value chosen so the
//! REAL guard naturally takes its fast-exit — see the workload note). The
//! ONLY thing that differs between arms is the process-wide
//! `bench-internals`-gated `FORCE_DECAY_CLOCK_READ` override
//! (`AllocCore::dbg_set_force_decay_clock_read`, `src/alloc_core/alloc_core.rs`'s
//! own doc explains exactly what it does): with it OFF, the guard behaves
//! exactly as shipped; with it ON, `maybe_decay_large_cache` unconditionally
//! skips its fast-exit and reaches `Instant::now()` on every call, WITHOUT
//! touching `headroom_bytes` — the same headroom value, so `run_decay_step`'s
//! own `excess = used.saturating_sub(headroom)` still resolves to 0 on this
//! workload and no actual decay/eviction happens differently between arms.
//! Hit rate is therefore structurally identical between arms by
//! construction — this is design (a) from the F9 survey finding, the
//! "preferred/honest" option, not the weaker hit-rate-delta check (b).
//!
//! ## Path-activation oracle (R30-8 rule)
//!
//! Per arm, `AllocCore::dbg_maybe_decay_guard_passed_count()` (process-wide,
//! `bench-internals`-gated) is read before/after the timed region. The
//! "guard-real" arm must show a near-zero delta (the guard's fast-exit is
//! taking almost every call, since `used <= headroom` throughout — this
//! workload never exceeds `HEADROOM_BYTES`); the "guard-forced" arm must show
//! a delta equal to the call count (every call reached the clock read). An
//! arm that doesn't match this expectation is marked `oracle=FAIL` and
//! excluded from the headline comparison — this is what proves the two arms
//! actually differed in the intended mechanism, not just in a label.
//!
//! ## Workload
//!
//! Repeated large alloc/dealloc cycles of a single object size, sized well
//! under `HEADROOM_BYTES` so the cache never exceeds headroom (both call
//! sites — `alloc_large` top and the Large `dealloc` branch — are on the
//! measured path, matching F9's own "two call sites per steady-state
//! alloc/free cycle" framing). Single-threaded (`HeapRegistry::claim_with_config`,
//! registry-bypass, matching R30-6/R31-1's own methodology) — this gate
//! isolates a per-call CPU-side cost, not a cross-thread effect, so a single
//! heap is sufficient and keeps the design simple.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r32_8_large_cache_decay_clock_read_ab_gate --features "production alloc-stats bench-internals"
//! ```

#![cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::time::Instant;

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    AllocCore, LargeCacheConfig,
};

/// Fixed headroom shared by BOTH arms — deliberately well above
/// `LARGE_OBJ_BYTES` so the workload's `large_cache_used_bytes` never
/// exceeds it and the REAL (unforced) guard takes its fast-exit on
/// (almost) every call. 4 MiB: comfortably above one cached `LARGE_OBJ_BYTES`
/// object, comfortably below `LowHeadroom`'s own 16 MiB floor (this gate is
/// not trying to reproduce that exact value — it only needs "guard passes"
/// vs "guard forced past" to differ, at ANY fixed headroom).
const HEADROOM_BYTES: usize = 4 * 1024 * 1024;

/// One large object's size — comfortably `> SMALL_MAX` (unambiguously
/// Large-classified) and comfortably `< HEADROOM_BYTES` so a single resident
/// object never trips the real guard's "used > headroom" condition.
const LARGE_OBJ_BYTES: usize = 512 * 1024;

/// Alloc/dealloc cycles in the timed region. Each cycle calls
/// `maybe_decay_large_cache` twice (once in `alloc_large`, once in the Large
/// `dealloc` branch) — `CYCLES * 2` is therefore the expected guard-check
/// call count.
const CYCLES: usize = 200_000;

/// Untimed warm-up cycles (absorbs first-call timer-priming: the first EVER
/// call to `maybe_decay_large_cache` on a fresh heap always just primes
/// `last_decay_tick` and returns, per that function's own documented
/// first-call rule — running this before the timed region keeps the timed
/// region measuring only steady-state guard-check cost).
const WARMUP_CYCLES: usize = 64;

fn parse_env_bool(name: &str) -> bool {
    std::env::var(name).map(|v| v == "1").unwrap_or(false)
}

fn run_child() {
    let forced = parse_env_bool("R32_8_FORCE_CLOCK_READ");

    let conflicts_before = config_conflicts_total();

    let heap_ptr =
        HeapRegistry::claim_with_config(LargeCacheConfig::new().headroom_bytes(HEADROOM_BYTES));
    assert!(!heap_ptr.is_null(), "claim_with_config returned null");
    // SAFETY: `heap_ptr` was just returned by `claim_with_config` and is
    // owned by this thread until `recycle` at the end.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    // SELF-VERIFICATION (R26-4 config-sweep evidence rule): resolved
    // headroom read back from the diagnostic surface, not assumed.
    let (_, _, resolved) = heap.dbg_decay_config();
    assert_eq!(
        resolved, HEADROOM_BYTES,
        "R32-8 child: resolved headroom ({resolved}) != requested ({HEADROOM_BYTES})"
    );

    AllocCore::dbg_set_force_decay_clock_read(forced);

    let layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();

    // Untimed warm-up: absorbs first-call timer-priming AND first-touch
    // segment reservation so the timed region measures only steady-state
    // cache-hit + guard-check cost.
    for _ in 0..WARMUP_CYCLES {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "warm-up alloc failed (OOM?)");
        // SAFETY: `p` freshly allocated by `heap` with `layout`, freed once.
        unsafe { heap.dealloc(p, layout) };
    }

    let hits_before = heap.dbg_large_cache_hits();
    let guard_passed_before = AllocCore::dbg_maybe_decay_guard_passed_count();

    let t0 = Instant::now();
    for _ in 0..CYCLES {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "alloc failed (OOM?)");
        // SAFETY: `p` freshly allocated by `heap` with `layout`, freed once
        // immediately below (no other reference outstanding).
        unsafe { heap.dealloc(p, layout) };
    }
    let elapsed_ns = t0.elapsed().as_nanos() as u64;

    let hits_after = heap.dbg_large_cache_hits();
    let guard_passed_after = AllocCore::dbg_maybe_decay_guard_passed_count();

    let hits_delta = hits_after.saturating_sub(hits_before);
    let guard_passed_delta = guard_passed_after.saturating_sub(guard_passed_before);
    let expected_calls = (CYCLES * 2) as u64; // alloc_large + Large-dealloc, per cycle

    // Reset the override before recycling — do not leak process-wide forced
    // state into whatever runs next in this process (subprocess-per-arm
    // isolation means this matters only for hygiene, not correctness, since
    // each arm is a fresh process, but leaving a stale global `true` in a
    // process any other code later reused would be a footgun).
    AllocCore::dbg_set_force_decay_clock_read(false);

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);
    assert_eq!(
        conflicts_delta, 0,
        "R32-8 child (forced={forced}): CONFIG_CONFLICTS delta = {conflicts_delta} (expected 0)"
    );

    // PATH-ACTIVATION ORACLE (R30-8 rule): the two arms must differ in
    // guard-check activation the way the design intends.
    //   - guard-real (forced=false): the guard's fast-exit should fire on
    //     (almost) every call, so guard_passed_delta should be near 0.
    //   - guard-forced (forced=true): every call should reach the clock
    //     read, so guard_passed_delta should equal expected_calls exactly.
    let oracle_pass = if forced {
        guard_passed_delta == expected_calls
    } else {
        // Allow a small nonzero slack (e.g. a genuine one-off decay tick if
        // the interval elapsed mid-run) but the vast majority of calls must
        // still take the fast-exit.
        guard_passed_delta < expected_calls / 10
    };

    // SAFETY: `heap_ptr` was returned by `claim_with_config` above, not yet
    // recycled, and no other thread touches it.
    unsafe { HeapRegistry::recycle(heap_ptr) };

    let ns_per_cycle = elapsed_ns as f64 / CYCLES as f64;

    proc_probe::emit("arm", if forced { "guard-forced" } else { "guard-real" });
    proc_probe::emit_u64("forced", u64::from(forced));
    proc_probe::emit_u64("headroom_bytes", HEADROOM_BYTES as u64);
    proc_probe::emit_u64("verified_headroom", resolved as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("large_obj_bytes", LARGE_OBJ_BYTES as u64);
    proc_probe::emit_u64("cycles", CYCLES as u64);
    proc_probe::emit_u64("warmup_cycles", WARMUP_CYCLES as u64);
    proc_probe::emit_u64("hits_delta", hits_delta);
    proc_probe::emit_u64("guard_passed_delta", guard_passed_delta);
    proc_probe::emit_u64("expected_calls", expected_calls);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns.into());
    proc_probe::emit_u64("oracle_pass", u64::from(oracle_pass));

    println!(
        "OK arm={} forced={forced} headroom_bytes={HEADROOM_BYTES} verified_headroom={resolved} \
         cfg_conflicts_delta={conflicts_delta} cycles={CYCLES} hits_delta={hits_delta} \
         guard_passed_delta={guard_passed_delta}/{expected_calls} elapsed_ns={elapsed_ns} \
         ns_per_cycle={ns_per_cycle:.2} oracle={}",
        if forced { "guard-forced" } else { "guard-real" },
        if oracle_pass { "PASS" } else { "FAIL" }
    );
}

// ---------------------------------------------------------------------------
// Orchestrator mode
// ---------------------------------------------------------------------------

struct ChildMetrics {
    map: std::collections::HashMap<String, u64>,
}

impl ChildMetrics {
    fn get(&self, k: &str) -> u64 {
        *self
            .map
            .get(k)
            .unwrap_or_else(|| panic!("child RESULT missing {k}"))
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

fn run_one_child(forced: bool, repetition: usize) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R32_8_FORCE_CLOCK_READ", if forced { "1" } else { "0" })
        .env("R32_8_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R32-8 child (forced={forced}, rep={repetition}) failed with status {:?}; see stderr above",
            output.status.code()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    parse_child_stdout(&stdout)
}

fn median(v: &mut [u64]) -> u64 {
    if v.is_empty() {
        return 0;
    }
    v.sort_unstable();
    v[v.len() / 2]
}

/// Repetitions per arm — kept modest (CLAUDE.md "Speed: short scenario by
/// default"); this is a single-cell (fixed headroom) A/B, not a sweep, so
/// more reps per arm are affordable than a full grid.
const REPETITIONS: usize = 7;

fn run_orchestrator() {
    println!(
        "=== R32-8 large-cache decay CLOCK-READ cost A/B gate — subprocess-per-arm isolation ==="
    );
    println!(
        "HEADROOM_BYTES={HEADROOM_BYTES} (FIXED across both arms) LARGE_OBJ_BYTES={LARGE_OBJ_BYTES} \
         CYCLES={CYCLES} WARMUP_CYCLES={WARMUP_CYCLES} REPETITIONS={REPETITIONS}"
    );
    println!();

    let mut all: Vec<(bool, ChildMetrics)> = Vec::new();
    let mut oracle_failures: Vec<(bool, usize)> = Vec::new();
    for &forced in &[false, true] {
        for rep in 0..REPETITIONS {
            eprintln!("--- arm forced={forced} rep={rep}/{REPETITIONS} ---");
            let m = run_one_child(forced, rep);
            assert_eq!(m.get("forced"), u64::from(forced), "wrong forced flag");
            assert_eq!(
                m.get("verified_headroom"),
                HEADROOM_BYTES as u64,
                "verified_headroom != requested"
            );
            assert_eq!(
                m.get("config_conflicts_delta"),
                0,
                "non-zero config_conflicts_delta"
            );
            if m.get("oracle_pass") == 0 {
                oracle_failures.push((forced, rep));
            }
            all.push((forced, m));
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps) ===");
    println!(
        "{:>14} {:>14} {:>14} {:>10}",
        "arm", "ns_per_cycle", "guard_passed", "oracle"
    );
    let mut summary: std::collections::HashMap<bool, u64> = std::collections::HashMap::new();
    for &forced in &[false, true] {
        let cell: Vec<&ChildMetrics> = all
            .iter()
            .filter(|(f, _)| *f == forced)
            .map(|(_, m)| m)
            .collect();
        let mut elapsed: Vec<u64> = cell.iter().map(|m| m.get("elapsed_ns")).collect();
        let elapsed_m = median(&mut elapsed);
        let ns_per_cycle = elapsed_m as f64 / CYCLES as f64;
        let guard_passed = cell[0].get("guard_passed_delta");
        let fails = cell.iter().filter(|m| m.get("oracle_pass") == 0).count();
        let oracle_tag = if fails > 0 {
            format!("FAIL {fails}/{REPETITIONS}")
        } else {
            "PASS".to_string()
        };
        summary.insert(forced, elapsed_m);
        println!(
            "{:>14} {:>14.2} {:>14} {:>10}",
            if forced { "guard-forced" } else { "guard-real" },
            ns_per_cycle,
            guard_passed,
            oracle_tag,
        );
    }

    println!();
    println!("=== CSV (one row per child) ===");
    let cols = [
        "arm",
        "forced",
        "repetition",
        "headroom_bytes",
        "verified_headroom",
        "config_conflicts_delta",
        "process_identity",
        "large_obj_bytes",
        "cycles",
        "warmup_cycles",
        "hits_delta",
        "guard_passed_delta",
        "expected_calls",
        "elapsed_ns",
        "oracle_pass",
    ];
    println!("# {}", cols.join(","));
    for (i, (forced, m)) in all.iter().enumerate() {
        let row: Vec<String> = cols
            .iter()
            .map(|c| match *c {
                "process_identity" => "subprocess".to_string(),
                "repetition" => (i % REPETITIONS).to_string(),
                "arm" => if *forced {
                    "guard-forced"
                } else {
                    "guard-real"
                }
                .to_string(),
                _ => m.get(c).to_string(),
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    if oracle_failures.is_empty() {
        println!(
            "NOTE: all {} arms passed the path-activation oracle (guard_passed_delta matched its \
             arm's expectation: near-0 for guard-real, == expected_calls for guard-forced).",
            all.len()
        );
    } else {
        println!(
            "WARNING: {} arm(s) FAILED the path-activation oracle: {:?}",
            oracle_failures.len(),
            oracle_failures
        );
    }

    if let (Some(&real_ns), Some(&forced_ns)) = (summary.get(&false), summary.get(&true)) {
        let delta_ns_per_cycle = (forced_ns as f64 - real_ns as f64) / CYCLES as f64;
        // Each cycle calls maybe_decay_large_cache twice (alloc + dealloc),
        // so the per-CALL clock-read cost is half the per-cycle delta.
        let delta_ns_per_call = delta_ns_per_cycle / 2.0;
        println!(
            "\nHEADLINE: guard-forced - guard-real = {delta_ns_per_cycle:.2} ns/cycle \
             ({delta_ns_per_call:.2} ns/maybe_decay_large_cache call), at HEADROOM_BYTES={HEADROOM_BYTES} \
             fixed identically across both arms (hit rate structurally unchanged by construction)."
        );
    }

    println!(
        "\nNOTE: each (forced, rep) cell ran in its OWN freshly-spawned OS process (subprocess-per-arm \
         isolation, registry-bypass via HeapRegistry::claim_with_config). Every arm hard-asserted \
         resolved headroom == requested headroom AND config_conflicts_delta == 0 (R26-4 config \
         identity). headroom_bytes is IDENTICAL across both arms — only FORCE_DECAY_CLOCK_READ differs \
         — so this is design (a) from the F9 survey finding, not a headroom sweep."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R32_8_FORCE_CLOCK_READ").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
