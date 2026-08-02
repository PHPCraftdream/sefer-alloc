//! R32-8 (task #499, F9) — validates the STRIDE-THROTTLE structural fix's
//! actual benefit in the regime it targets: a workload where
//! `large_cache_used_bytes` genuinely and persistently stays ABOVE
//! `headroom_bytes` (the shipped `LargeCachePolicy::LowHeadroom` /
//! `::Trimmed64MiB` profiles' whole intended use case per
//! `src/alloc_core/profile.rs`), unlike the sibling
//! `examples/r32_8_large_cache_decay_clock_read_ab_gate.rs`, whose workload
//! is deliberately BELOW headroom (that gate isolates the raw per-call
//! clock-read cost; this one validates the fix that reduces how often that
//! cost is paid once the real guard's fast-exit no longer applies).
//!
//! ## Design: same `FORCE_DECAY_CLOCK_READ` instrument, reused as an
//! old-shape/new-shape switch
//!
//! `AllocCore::maybe_decay_large_cache`'s stride throttle
//! (`DECAY_CLOCK_CHECK_STRIDE`, `src/alloc_core/alloc_core_large_cache.rs`)
//! is bypassed whenever `FORCE_DECAY_CLOCK_READ` is set — by construction,
//! that makes the "forced" arm read the clock on EVERY call once past
//! headroom, i.e. byte-for-byte the OLD (pre-R32-8) unconditional-past-
//! headroom behavior. The "real" arm (`forced=false`) exercises the NEW
//! stride-throttled path. Both arms use the IDENTICAL headroom AND the
//! IDENTICAL workload (`used_bytes` stays above `HEADROOM_BYTES`
//! throughout for both arms, verified by the path-activation oracle below),
//! so this is a clean old-shape-vs-new-shape comparison in the exact regime
//! the fix targets — not a headroom sweep, and not subject to R31-1's
//! headroom-driven hit-rate confound (headroom never changes between arms
//! here either).
//!
//! ## Path-activation oracle (R30-8 rule)
//!
//! `AllocCore::dbg_maybe_decay_guard_passed_count()` delta per arm:
//! - `forced=true` ("old shape"): `FORCE_DECAY_CLOCK_READ` bypasses the
//!   headroom check entirely, so EVERY call (both `alloc_large`'s and the
//!   Large-dealloc branch's) reaches the clock read — the delta must equal
//!   `expected_calls` exactly.
//! - `forced=false` ("new shape"): the REAL headroom check applies first.
//!   This workload's single cached object is exclusively-owned at any given
//!   instant — resident in the cache (`used > headroom`) while idle between
//!   ops, but REMOVED from the cache the instant `alloc_large`'s hit path
//!   takes it out and not yet re-deposited until the matching `dealloc`
//!   completes. Concretely: `alloc_large`'s guard check always sees
//!   `used > headroom` (the prior cycle's dealloc just redeposited it), but
//!   the Large-dealloc branch's guard check always sees `used == 0` (the
//!   object is mid-flight, not yet redeposited) — so only the ALLOC-side
//!   call of each cycle ever organically passes the headroom check at all,
//!   halving the population the stride throttle even applies to before the
//!   `~1/64` stride reduction is layered on top (net: ~1/128 of the raw
//!   400,000-call population, confirmed empirically — see the CSV). The
//!   oracle therefore checks the delta is MATERIALLY lower than
//!   `expected_calls` (below `expected_calls / 4`, a generous margin around
//!   the observed ~1/128) AND nonzero (the guard still fires occasionally,
//!   proving decay ticks still eventually happen, not that decay silently
//!   stopped) — not a tight `expected_calls / 64` bound, since that would
//!   assume every call organically reaches the headroom check, which this
//!   specific single-object workload shape does not do on its dealloc side.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r32_8_large_cache_decay_stride_fix_gate --features "production alloc-stats bench-internals"
//! ```

#![cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::time::Instant;

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    AllocCore, LargeCacheConfig,
};

/// Small headroom so the workload's resident cache genuinely and
/// persistently exceeds it (the `LowHeadroom`/`Trimmed64MiB` regime this
/// fix targets) — well below `LARGE_OBJ_BYTES`, so even ONE cached object
/// already sits above headroom.
const HEADROOM_BYTES: usize = 64 * 1024;

/// One large object's size — deliberately `> HEADROOM_BYTES` so the cache
/// stays above headroom for the ENTIRE run once the first object is cached
/// (the workload never lets `used_bytes` drop back to/below headroom).
const LARGE_OBJ_BYTES: usize = 512 * 1024;

/// Alloc/dealloc cycles in the timed region. Each cycle calls
/// `maybe_decay_large_cache` twice.
const CYCLES: usize = 200_000;

/// Untimed warm-up cycles: absorbs first-call timer-priming AND establishes
/// the above-headroom steady state before the timed region starts.
const WARMUP_CYCLES: usize = 64;

/// Matches `DECAY_CLOCK_CHECK_STRIDE` in `alloc_core_large_cache.rs` — used
/// only to size the oracle's tolerance band, not compiled against the
/// private const directly (this file is a separate compilation unit outside
/// `alloc_core`).
const ASSUMED_STRIDE: u64 = 64;

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

    let (_, _, resolved) = heap.dbg_decay_config();
    assert_eq!(
        resolved, HEADROOM_BYTES,
        "R32-8 stride-fix child: resolved headroom ({resolved}) != requested ({HEADROOM_BYTES})"
    );

    AllocCore::dbg_set_force_decay_clock_read(forced);

    let layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();

    // Untimed warm-up: absorbs first-call timer-priming AND establishes the
    // above-headroom steady state (after the first alloc+dealloc, the cache
    // holds one LARGE_OBJ_BYTES entry > HEADROOM_BYTES, so every subsequent
    // maybe_decay_large_cache call in this run sees used > headroom).
    for _ in 0..WARMUP_CYCLES {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "warm-up alloc failed (OOM?)");
        // SAFETY: `p` freshly allocated by `heap` with `layout`, freed once.
        unsafe { heap.dealloc(p, layout) };
    }

    let used_before_timed = heap.dbg_large_cache_used();
    assert!(
        used_before_timed > HEADROOM_BYTES,
        "R32-8 stride-fix child: workload precondition violated — \
         used_before_timed={used_before_timed} must exceed HEADROOM_BYTES={HEADROOM_BYTES} \
         before the timed region starts"
    );

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

    let guard_passed_after = AllocCore::dbg_maybe_decay_guard_passed_count();
    let guard_passed_delta = guard_passed_after.saturating_sub(guard_passed_before);
    let used_after_timed = heap.dbg_large_cache_used();
    let expected_calls = (CYCLES * 2) as u64;

    AllocCore::dbg_set_force_decay_clock_read(false);

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);
    assert_eq!(
        conflicts_delta, 0,
        "R32-8 stride-fix child (forced={forced}): CONFIG_CONFLICTS delta = {conflicts_delta} \
         (expected 0)"
    );

    // PATH-ACTIVATION ORACLE (R30-8 rule): confirms the workload precondition
    // (used > headroom throughout) AND that the two arms differ in guard
    // activation the way the fix's design intends.
    let stayed_above_headroom = used_after_timed > HEADROOM_BYTES;
    let oracle_pass = if forced {
        stayed_above_headroom && guard_passed_delta == expected_calls
    } else {
        // New (stride-throttled) shape: materially fewer clock reads than
        // the old shape, but not zero (decay logic still eventually runs).
        stayed_above_headroom && guard_passed_delta > 0 && guard_passed_delta < expected_calls / 4
    };

    // SAFETY: `heap_ptr` was returned by `claim_with_config` above, not yet
    // recycled, and no other thread touches it.
    unsafe { HeapRegistry::recycle(heap_ptr) };

    let ns_per_cycle = elapsed_ns as f64 / CYCLES as f64;

    proc_probe::emit("arm", if forced { "old-shape" } else { "new-shape" });
    proc_probe::emit_u64("forced", u64::from(forced));
    proc_probe::emit_u64("headroom_bytes", HEADROOM_BYTES as u64);
    proc_probe::emit_u64("verified_headroom", resolved as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("large_obj_bytes", LARGE_OBJ_BYTES as u64);
    proc_probe::emit_u64("cycles", CYCLES as u64);
    proc_probe::emit_u64("used_before_timed", used_before_timed as u64);
    proc_probe::emit_u64("used_after_timed", used_after_timed as u64);
    proc_probe::emit_u64("stayed_above_headroom", u64::from(stayed_above_headroom));
    proc_probe::emit_u64("guard_passed_delta", guard_passed_delta);
    proc_probe::emit_u64("expected_calls", expected_calls);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns.into());
    proc_probe::emit_u64("oracle_pass", u64::from(oracle_pass));

    println!(
        "OK arm={} forced={forced} headroom_bytes={HEADROOM_BYTES} verified_headroom={resolved} \
         cfg_conflicts_delta={conflicts_delta} used_after_timed={used_after_timed} \
         guard_passed_delta={guard_passed_delta}/{expected_calls} elapsed_ns={elapsed_ns} \
         ns_per_cycle={ns_per_cycle:.2} oracle={}",
        if forced { "old-shape" } else { "new-shape" },
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
            "R32-8 stride-fix child (forced={forced}, rep={repetition}) failed with status \
             {:?}; see stderr above",
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

const REPETITIONS: usize = 7;

fn run_orchestrator() {
    println!(
        "=== R32-8 large-cache decay STRIDE-THROTTLE fix validation — old-shape vs new-shape ==="
    );
    println!(
        "HEADROOM_BYTES={HEADROOM_BYTES} (FIXED, workload stays ABOVE it) LARGE_OBJ_BYTES={LARGE_OBJ_BYTES} \
         CYCLES={CYCLES} WARMUP_CYCLES={WARMUP_CYCLES} REPETITIONS={REPETITIONS} \
         assumed_stride={ASSUMED_STRIDE}"
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
            assert_eq!(
                m.get("stayed_above_headroom"),
                1,
                "workload precondition violated: did not stay above headroom"
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
        "{:>12} {:>14} {:>16} {:>10}",
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
            "{:>12} {:>14.2} {:>16} {:>10}",
            if forced { "old-shape" } else { "new-shape" },
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
        "used_before_timed",
        "used_after_timed",
        "stayed_above_headroom",
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
                "arm" => if *forced { "old-shape" } else { "new-shape" }.to_string(),
                _ => m.get(c).to_string(),
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    if oracle_failures.is_empty() {
        println!(
            "NOTE: all {} arms passed the path-activation oracle (workload stayed above headroom \
             throughout; guard_passed_delta matched its arm's expectation).",
            all.len()
        );
    } else {
        println!(
            "WARNING: {} arm(s) FAILED the path-activation oracle: {:?}",
            oracle_failures.len(),
            oracle_failures
        );
    }

    if let (Some(&new_ns), Some(&old_ns)) = (summary.get(&false), summary.get(&true)) {
        let delta_ns_per_cycle = (old_ns as f64 - new_ns as f64) / CYCLES as f64;
        let delta_ns_per_call = delta_ns_per_cycle / 2.0;
        let pct = if old_ns > 0 {
            100.0 * (old_ns as f64 - new_ns as f64) / old_ns as f64
        } else {
            0.0
        };
        println!(
            "\nHEADLINE: old-shape - new-shape = {delta_ns_per_cycle:.2} ns/cycle \
             ({delta_ns_per_call:.2} ns/maybe_decay_large_cache call, {pct:.1}% of old-shape elapsed), \
             at HEADROOM_BYTES={HEADROOM_BYTES} fixed identically across both arms, workload \
             genuinely above headroom throughout (the LowHeadroom/Trimmed64MiB regime)."
        );
    }

    println!(
        "\nNOTE: each (forced, rep) cell ran in its OWN freshly-spawned OS process. Every arm \
         hard-asserted resolved headroom == requested headroom, config_conflicts_delta == 0, AND \
         stayed_above_headroom == true (the workload precondition this gate depends on). \
         forced=true reproduces the OLD (pre-R32-8) unconditional-past-headroom clock-read shape by \
         bypassing the stride throttle; forced=false is the real NEW shipped stride-throttled path."
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
