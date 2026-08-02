//! R31-10 cost side (task #492) — the missing counterpart to
//! `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md`, which measured ONLY
//! the RSS/commit BENEFIT of `SeferAlloc::trim_current_thread()`. Per
//! CLAUDE.md's cost/benefit same-workload-regime rule, a policy/API
//! optimization is not gate-complete until its cost is measured in the SAME
//! regime the benefit was measured in — this binary reruns the EXACT R31-10
//! burst → (trim?) → idle → burst2 sequence and additionally measures:
//!
//! 1. **Trim call latency** — wall time of the `trim_current_thread()` call
//!    itself (`RESULT trim_call_ns`).
//! 2. **Second-burst latency** — wall time of burst2 (`RESULT elapsed_ns`,
//!    the metric `scripts/paired-ab-runner.mjs` pairs across alternating
//!    process launches), compared TRIM vs NO_TRIM to isolate the cold-start
//!    cost trim specifically causes (burst2 must re-materialise the large
//!    cache from scratch in the TRIM arm; it reuses the still-warm cache in
//!    the NO_TRIM arm).
//! 3. Throughput/CPU cost is not separately distinguishable from (1)/(2) at
//!    this workload's scale (4 large allocs/frees per burst) — wall-clock
//!    IS the throughput signal here, so no separate metric is added; see
//!    the report's "Cost side" section for the explicit scope note this
//!    corresponds to (task #492 Part B, item 3).
//!
//! A MIXED Small/Large workload variant (task #492 Part B, item 4) is
//! explicitly SCOPED OUT: R31-10's own workload is already Large-only (4 ×
//! 32 MiB), and `trim_current_thread()`'s cost is dominated by the
//! large-cache eviction / re-materialisation path (each 32 MiB object is 9
//! segments — see the RSS gate's §4 correction); a Small-class admixture
//! would not exercise a materially different trim code path (the
//! small-pool drain is a cheap, already-fast O(pooled segments) loop with
//! no OS call per drained segment, unlike the large-cache eviction's
//! `os::release_segment` calls) and would only dilute the signal this gate
//! exists to isolate. Noted here explicitly per the task's own instruction
//! to state the scope-down decision, not silently skip it.
//!
//! ## Methodology — reuses R31-10's own protocol, unchanged
//!
//! Subprocess-per-arm isolation: this binary is BOTH the two-arm probe (run
//! via `--config` through `scripts/paired-ab-runner.mjs` for the
//! statistically-judged A/B on burst2 latency) AND its own orchestrator (for
//! a quick single-shot view of the mechanism-oracle numbers and trim
//! latency, printed via `RESULT` lines any run can read).
//!
//! Mode selection: `R31_10_COST_MODE=TRIM` or `R31_10_COST_MODE=NO_TRIM`
//! (env var, mirroring the RSS gate's `R31_10_ARM`). Workload constants
//! (`LARGE_OBJ_BYTES`, `LARGE_OBJ_COUNT`, `IDLE_MS`) are IDENTICAL to
//! `examples/r31_10_trim_rss_gate.rs` — same regime, per CLAUDE.md's rule.
//!
//! ## Entry point under test
//!
//! `SeferAlloc` installed as the REAL `#[global_allocator]` (stronger than
//! R31-10's own direct-`GlobalAlloc`-call approach — this binary goes one
//! step further and is a genuine installed-allocator process, matching
//! `examples/paired_ab_sefer.rs`'s pattern) + `SeferAlloc::trim_current_thread()`.
//!
//! ## Run
//!
//! Single-shot view (both arms, sequential, orchestrator mode):
//! ```text
//! cargo run --release --example r31_10_trim_cost_gate --features production
//! ```
//!
//! Statistically-judged A/B via the paired runner (writes provenance JSON +
//! runs the paired t-test / sign test):
//! ```text
//! cargo build --release --example r31_10_trim_cost_gate --features production
//! node scripts/paired-ab-runner.mjs --config docs/perf/r31_10_cost_ab_config.json
//! node scripts/paired-ab-runner.mjs --config docs/perf/r31_10_cost_ab_config.json --arms TRIM,TRIM
//! ```

#![cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// ---------------------------------------------------------------------------
// Workload parameters — IDENTICAL to `examples/r31_10_trim_rss_gate.rs`
// (same regime, per CLAUDE.md's cost/benefit same-workload-regime rule).
// ---------------------------------------------------------------------------

/// Large-object size: 32 MiB per object (9 × 4 MiB segments after
/// whole-`SEGMENT` rounding — see the RSS gate's §4 correction).
const LARGE_OBJ_BYTES: usize = 32 * 1024 * 1024;

/// Distinct large objects per burst.
const LARGE_OBJ_COUNT: usize = 4;

/// Page size for the touch pattern.
const PAGE: usize = 4096;

/// Idle window duration between burst1 and burst2 — same as the RSS gate.
const IDLE_MS: u64 = 500;

/// Touch every page in the allocation so it is fully committed / faulted in.
///
/// SAFETY: `p` must be a valid allocation of at least `size` bytes.
unsafe fn touch_all_pages(p: *mut u8, size: usize) {
    let mut off = 0usize;
    while off < size {
        p.add(off).write_volatile(0xAB);
        off += PAGE;
    }
}

/// Run one burst: allocate `LARGE_OBJ_COUNT` objects of `LARGE_OBJ_BYTES`,
/// touch every page, free them all. Returns the wall-clock duration of the
/// WHOLE burst (alloc + touch + free), matching what a real caller's
/// request-handling burst would pay end to end.
fn run_burst() -> Duration {
    let layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();
    let t0 = Instant::now();
    let mut live = Vec::with_capacity(LARGE_OBJ_COUNT);
    for _ in 0..LARGE_OBJ_COUNT {
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { std::alloc::alloc(layout) };
        assert!(!p.is_null(), "burst alloc failed (OOM?)");
        // SAFETY: `p` is a fresh allocation of `LARGE_OBJ_BYTES` bytes.
        unsafe { touch_all_pages(p, LARGE_OBJ_BYTES) };
        live.push(p);
    }
    for &p in &live {
        // SAFETY: `p` was allocated above with `layout`, freed once here.
        unsafe { std::alloc::dealloc(p, layout) };
    }
    drop(live);
    t0.elapsed()
}

// ---------------------------------------------------------------------------
// Child mode — run one arm, print RESULT lines.
// ---------------------------------------------------------------------------

fn run_child() {
    // Mode comes from either a CLI arg (`--mode TRIM|NO_TRIM`, what
    // `scripts/paired-ab-runner.mjs --config` drives via each arm's `args` —
    // that runner does not plumb per-arm env vars through to the spawned
    // process, only `command`/`args`) or the `R31_10_COST_MODE` env var
    // (what this binary's own `run_one_child` orchestrator uses, and a
    // convenient manual-invocation form). CLI arg takes precedence if both
    // are present.
    let mode = std::env::args()
        .position(|a| a == "--mode")
        .and_then(|i| std::env::args().nth(i + 1))
        .or_else(|| std::env::var("R31_10_COST_MODE").ok())
        .unwrap_or_else(|| {
            panic!("mode required: pass --mode TRIM|NO_TRIM or set R31_10_COST_MODE")
        });
    let do_trim = match mode.as_str() {
        "TRIM" => true,
        "NO_TRIM" => false,
        other => panic!("R31_10_COST_MODE must be TRIM or NO_TRIM, got {other:?}"),
    };

    // --- Burst 1: warms the large cache (untimed for this gate's headline —
    //     R31-10's RSS gate already covers burst1's own cost). ---
    let _burst1_ns = run_burst();

    // Mechanism oracle: released-segments counter before the trim/no-trim
    // action (CLAUDE.md R30-8 path-activation rule — same oracle R31-10's
    // RSS gate uses).
    let released_before = GLOBAL.stats().segments_released_total;
    let reserved_before_trim = GLOBAL.stats().segments_reserved_total;

    // --- Trim call latency (TRIM arm only) ---
    let trim_call_ns: u128 = if do_trim {
        let t0 = Instant::now();
        GLOBAL.trim_current_thread();
        t0.elapsed().as_nanos()
    } else {
        0
    };

    let released_after_action = GLOBAL.stats().segments_released_total;
    let action_released_delta = released_after_action.saturating_sub(released_before);

    if do_trim {
        assert!(
            action_released_delta > 0,
            "TRIM arm: trim_current_thread() released zero segments — the \
             eviction mechanism did not fire (released_before={released_before}, \
             released_after={released_after_action})"
        );
    }

    // --- Idle window (identical to the RSS gate) ---
    thread::sleep(Duration::from_millis(IDLE_MS));

    // --- Burst 2: the TIMED metric this gate exists to produce. In the TRIM
    //     arm the large cache is empty (evicted), so burst2 pays the cold
    //     re-materialisation cost (fresh OS segment reservations). In the
    //     NO_TRIM arm the cache is still warm, so burst2 should be cheaper.
    //     This isolates trim's cold-start cost as a wall-clock delta. ---
    let reserved_before_burst2 = GLOBAL.stats().segments_reserved_total;
    let burst2_dur = run_burst();
    let burst2_ns = burst2_dur.as_nanos();
    let reserved_after_burst2 = GLOBAL.stats().segments_reserved_total;
    let burst2_reserved_delta = reserved_after_burst2.saturating_sub(reserved_before_burst2);

    // --- Emit results (paired-ab-runner.mjs parses `RESULT key=value`) ---
    proc_probe::emit("arm", &mode);
    proc_probe::emit_ns("elapsed_ns", burst2_ns); // the paired metric: burst2 wall time
    proc_probe::emit_ns("trim_call_ns", trim_call_ns);
    proc_probe::emit_u64("action_released_delta", action_released_delta);
    proc_probe::emit_u64("segments_reserved_total", reserved_after_burst2);
    proc_probe::emit_u64("burst2_reserved_delta", burst2_reserved_delta);
    proc_probe::emit_u64("reserved_before_trim", reserved_before_trim);
    proc_probe::emit_u64("reserved_before_burst2", reserved_before_burst2);

    println!(
        "OK mode={mode} trim_call_ns={trim_call_ns} burst2_ns={burst2_ns} \
         action_released_delta={action_released_delta} burst2_reserved_delta={burst2_reserved_delta}"
    );
}

// ---------------------------------------------------------------------------
// Orchestrator mode — quick single-shot both-arms view (for a fast local
// read, independent of the paired-ab-runner's statistical judge).
// ---------------------------------------------------------------------------

fn run_one_child(mode: &str) -> std::collections::HashMap<String, u64> {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R31_10_COST_MODE", mode)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R31-10 cost-gate child (mode={mode}) failed with status {:?}; see stderr above",
            output.status.code()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    let mut num = std::collections::HashMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("RESULT ") else {
            continue;
        };
        for tok in rest.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                if let Ok(n) = v.parse::<u64>() {
                    num.insert(k.to_string(), n);
                }
            }
        }
    }
    num
}

fn run_orchestrator() {
    println!("=== R31-10 trim_current_thread() COST gate (task #492) — single-shot view ===");
    println!(
        "Same regime as docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md: burst1 → (trim?) → \
         idle({IDLE_MS}ms) → burst2. LARGE_OBJ={LARGE_OBJ_BYTES} bytes x {LARGE_OBJ_COUNT}."
    );
    println!(
        "For the statistically-judged A/B (paired t-test + sign test, N=20), use:\n  \
         node scripts/paired-ab-runner.mjs --config docs/perf/r31_10_cost_ab_config.json"
    );
    println!();

    let trim = run_one_child("TRIM");
    let no_trim = run_one_child("NO_TRIM");

    let get = |m: &std::collections::HashMap<String, u64>, k: &str| -> u64 {
        *m.get(k).unwrap_or_else(|| panic!("missing RESULT {k}"))
    };

    println!();
    println!("=== headline (single-shot; run the paired-ab-runner for a real N=20 verdict) ===");
    println!(
        "trim_call_ns:            TRIM={}",
        get(&trim, "trim_call_ns")
    );
    println!(
        "burst2 elapsed_ns:       TRIM={}  NO_TRIM={}  delta(TRIM-NO_TRIM)={}",
        get(&trim, "elapsed_ns"),
        get(&no_trim, "elapsed_ns"),
        get(&trim, "elapsed_ns") as i64 - get(&no_trim, "elapsed_ns") as i64,
    );
    println!(
        "burst2_reserved_delta:   TRIM={}  NO_TRIM={}  (higher = more fresh OS reservations, \
         i.e. cold re-materialisation)",
        get(&trim, "burst2_reserved_delta"),
        get(&no_trim, "burst2_reserved_delta"),
    );
    println!(
        "action_released_delta:  TRIM={}  NO_TRIM={}  (mechanism oracle: TRIM must be > 0)",
        get(&trim, "action_released_delta"),
        get(&no_trim, "action_released_delta"),
    );

    assert!(
        get(&trim, "action_released_delta") > 0,
        "mechanism oracle FAILED: TRIM arm's action_released_delta must be > 0"
    );
    assert_eq!(
        get(&no_trim, "action_released_delta"),
        0,
        "mechanism oracle FAILED: NO_TRIM arm must never release segments (no trim call)"
    );
}

fn main() {
    // Child mode is selected by EITHER the `R31_10_COST_MODE` env var (the
    // orchestrator's own spawn form) OR a `--mode` CLI arg (the form
    // `scripts/paired-ab-runner.mjs --config` drives, since that runner only
    // plumbs `command`/`args` through, not per-arm env vars — see
    // `run_child`'s own doc comment on the mode-resolution precedence).
    let child_mode =
        std::env::var_os("R31_10_COST_MODE").is_some() || std::env::args().any(|a| a == "--mode");
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
