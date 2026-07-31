//! R31-10 (task #474) — AC5 gate: measured burst-idle-burst RSS win from
//! `SeferAlloc::trim_current_thread()`.
//!
//! ## What this gate measures
//!
//! The design doc (`docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md`) §1
//! established (citing R29-13 and R27-3) that pure idle reclaims 0 KiB
//! from both the large cache and the small-segment pool — both decay
//! mechanisms are event-driven (fire only on alloc/dealloc traffic) and a
//! heap that goes idle after a burst never calls that path again. This
//! gate measures whether `trim_current_thread()` — called explicitly by
//! the application at the phase boundary — closes that gap: does a
//! burst → `trim_current_thread()` → idle → burst sequence reclaim RSS
//! DURING the idle window that an otherwise-identical burst → idle → burst
//! sequence (no trim call) does NOT?
//!
//! ## Methodology
//!
//! Subprocess-per-arm isolation (R27-3/R29-13 protocol): each arm runs in
//! its OWN freshly-spawned OS process, so a fresh process has an empty
//! heap registry and cross-arm state contamination is impossible by
//! construction (R26-4's strongest identity evidence form).
//!
//! Two arms:
//! - **TRIM**: burst → `trim_current_thread()` → idle → burst.
//! - **NO_TRIM**: burst → idle → burst (no trim call).
//!
//! Both arms use the IDENTICAL workload (same object sizes, same counts,
//! same touch pattern, same idle duration) — the ONLY difference is the
//! single `trim_current_thread()` call in the TRIM arm. The key metric is
//! `rss_idle`: the RSS measured AFTER the idle window, BEFORE burst2. The
//! TRIM arm's `rss_idle` should be materially lower than the NO_TRIM arm's
//! because the trim released the cached large spans to the OS; the NO_TRIM
//! arm's `rss_idle` should stay at ~`rss_burst1` because nothing reclaims
//! during pure idle.
//!
//! ## Allocator layer under test
//!
//! `SeferAlloc` (direct `GlobalAlloc::alloc`/`dealloc` trait calls +
//! `SeferAlloc::trim_current_thread()`). This is the SAME layer as a real
//! `#[global_allocator]` — the same code paths, same TLS resolution, same
//! `trim_for_recycle` primitive — exercised without system-allocator noise
//! mixing into the RSS measurement. NOT the `HeapCore`/`HeapRegistry`
//! substrate layer (which would bypass the `SeferAlloc` entry point the
//! feature actually ships at — see CLAUDE.md's entry-point rule).
//!
//! ## Mechanism oracle (CLAUDE.md R30-8 rule)
//!
//! Every TRIM arm hard-asserts `trim_released_delta > 0` (the trim call
//! actually incremented `segments_released_total` — proving the eviction
//! mechanism fired, not just the RSS counter moving). Every NO_TRIM arm
//! reports `idle_released_delta` — the segments released across the PURE
//! IDLE window — which should be 0, directly confirming the gap this API
//! exists to fill.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r31_10_trim_rss_gate --features "production"
//! ```

#![cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::{GlobalAlloc, Layout};
use std::thread;
use std::time::Duration;

use sefer_alloc::SeferAlloc;

// ---------------------------------------------------------------------------
// Workload parameters
// ---------------------------------------------------------------------------

/// Large-object size: 32 MiB per object. Rounds to exactly 8 × 4 MiB
/// segments (32 MiB / 4 MiB = 8, header fits in the first segment's
/// prefix), so each object's committed footprint is 32 MiB.
const LARGE_OBJ_BYTES: usize = 32 * 1024 * 1024;

/// Distinct large objects per burst. 4 × 32 MiB = 128 MiB total — large
/// enough to be clearly visible above OS RSS granularity and measurement
/// noise.
const LARGE_OBJ_COUNT: usize = 4;

/// Page size for the touch pattern (ensures every page is faulted into
/// the working set, so the allocation's full RSS is materialised).
const PAGE: usize = 4096;

/// Idle window duration between burst1 and burst2. Must be long enough for
/// the OS to reclaim decommitted pages from the working set after the trim
/// call. 500 ms is generous on Windows (decommit is immediate; working-set
/// trimming follows within tens of ms under normal conditions).
const IDLE_MS: u64 = 500;

/// Repetitions per arm (median of 3 keeps the measurement fast while
/// reducing single-run noise — CLAUDE.md "short scenario by default").
const REPETITIONS: usize = 3;

// ---------------------------------------------------------------------------
// Child mode — run one arm
// ---------------------------------------------------------------------------

fn snapshot_kib() -> (u64, u64) {
    let m = proc_probe::snapshot();
    (m.rss / 1024, m.commit / 1024)
}

/// Touch every page in the allocation to ensure it is fully committed and
/// in the working set.
///
/// SAFETY: `p` must be a valid allocation of at least `size` bytes.
unsafe fn touch_all_pages(p: *mut u8, size: usize) {
    let mut off = 0usize;
    while off < size {
        p.add(off).write_volatile(0xAB);
        off += PAGE;
    }
}

fn run_child() {
    let arm = std::env::var("R31_10_ARM").unwrap_or_else(|e| panic!("R31_10_ARM required ({e})"));
    let do_trim = arm == "TRIM";
    let repetition: usize = std::env::var("R31_10_REPETITION")
        .unwrap_or_else(|e| panic!("R31_10_REPETITION required ({e})"))
        .parse()
        .unwrap_or_else(|e| panic!("R31_10_REPETITION parse error ({e})"));

    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();

    let (rss_before_kib, commit_before_kib) = snapshot_kib();

    // --- Burst 1: allocate, touch, free (spans go to large cache) ---
    let mut live = Vec::with_capacity(LARGE_OBJ_COUNT);
    for _ in 0..LARGE_OBJ_COUNT {
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null(), "burst1 alloc failed (OOM?)");
        // SAFETY: `p` is a fresh allocation of `LARGE_OBJ_BYTES` bytes.
        unsafe { touch_all_pages(p, LARGE_OBJ_BYTES) };
        live.push(p);
    }
    for &p in &live {
        // SAFETY: `p` was allocated by `a` with `layout`, freed once here.
        unsafe { a.dealloc(p, layout) };
    }
    drop(live);

    let (rss_burst1_kib, commit_burst1_kib) = snapshot_kib();

    // --- Mechanism oracle: read released_total before trim/idle ---
    let released_before = a.stats().segments_released_total;

    // --- Trim (TRIM arm only) ---
    if do_trim {
        a.trim_current_thread();
    }

    let released_after_action = a.stats().segments_released_total;
    let action_released_delta = released_after_action.saturating_sub(released_before);

    // Mechanism oracle: in the TRIM arm, the trim MUST have released segments.
    if do_trim {
        assert!(
            action_released_delta > 0,
            "TRIM arm (rep={repetition}): trim_current_thread() released zero segments — \
             the eviction mechanism did not fire (released_before={released_before}, \
             released_after={released_after_action})"
        );
    }

    // --- Idle window ---
    thread::sleep(Duration::from_millis(IDLE_MS));

    let (rss_idle_kib, commit_idle_kib) = snapshot_kib();

    let released_after_idle = a.stats().segments_released_total;
    let idle_released_delta = released_after_idle.saturating_sub(released_after_action);

    // --- Burst 2: same workload (proves the thread still works) ---
    let mut live2 = Vec::with_capacity(LARGE_OBJ_COUNT);
    for _ in 0..LARGE_OBJ_COUNT {
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null(), "burst2 alloc failed (OOM?)");
        // SAFETY: `p` is a fresh allocation of `LARGE_OBJ_BYTES` bytes.
        unsafe { touch_all_pages(p, LARGE_OBJ_BYTES) };
        live2.push(p);
    }
    for &p in &live2 {
        // SAFETY: `p` was allocated by `a` with `layout`, freed once here.
        unsafe { a.dealloc(p, layout) };
    }
    drop(live2);

    let (rss_burst2_kib, commit_burst2_kib) = snapshot_kib();

    // --- Emit results ---
    proc_probe::emit("arm", &arm);
    proc_probe::emit_u64("repetition", repetition as u64);
    proc_probe::emit_u64("large_obj_bytes", LARGE_OBJ_BYTES as u64);
    proc_probe::emit_u64("large_obj_count", LARGE_OBJ_COUNT as u64);
    proc_probe::emit_u64(
        "total_burst_bytes",
        (LARGE_OBJ_BYTES * LARGE_OBJ_COUNT) as u64,
    );
    proc_probe::emit_u64("idle_ms", IDLE_MS);
    proc_probe::emit_u64("rss_before_kib", rss_before_kib);
    proc_probe::emit_u64("commit_before_kib", commit_before_kib);
    proc_probe::emit_u64("rss_burst1_kib", rss_burst1_kib);
    proc_probe::emit_u64("commit_burst1_kib", commit_burst1_kib);
    proc_probe::emit_u64("action_released_delta", action_released_delta);
    proc_probe::emit_u64("rss_idle_kib", rss_idle_kib);
    proc_probe::emit_u64("commit_idle_kib", commit_idle_kib);
    proc_probe::emit_u64("idle_released_delta", idle_released_delta);
    proc_probe::emit_u64("rss_burst2_kib", rss_burst2_kib);
    proc_probe::emit_u64("commit_burst2_kib", commit_burst2_kib);
    proc_probe::emit_u64(
        "rss_drop_burst1_to_idle_kib",
        rss_burst1_kib.saturating_sub(rss_idle_kib),
    );
    proc_probe::emit_u64(
        "commit_drop_burst1_to_idle_kib",
        commit_burst1_kib.saturating_sub(commit_idle_kib),
    );

    println!(
        "OK arm={arm} rep={repetition} rss_burst1={rss_burst1_kib} rss_idle={rss_idle_kib} \
         rss_burst2={rss_burst2_kib} commit_burst1={commit_burst1_kib} commit_idle={commit_idle_kib} \
         action_released={action_released_delta} idle_released={idle_released_delta}"
    );
}

// ---------------------------------------------------------------------------
// Orchestrator mode
// ---------------------------------------------------------------------------

struct ChildMetrics {
    num: std::collections::HashMap<String, u64>,
    arm: String,
}

impl ChildMetrics {
    fn get(&self, k: &str) -> u64 {
        *self
            .num
            .get(k)
            .unwrap_or_else(|| panic!("child RESULT missing {k}"))
    }
}

fn parse_child_stdout(stdout: &str) -> ChildMetrics {
    let mut num = std::collections::HashMap::new();
    let mut arm = String::new();
    for line in stdout.lines() {
        let rest = match line.strip_prefix("RESULT ") {
            Some(r) => r,
            None => continue,
        };
        for tok in rest.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                if k == "arm" {
                    arm = v.to_string();
                } else if let Ok(n) = v.parse::<u64>() {
                    num.insert(k.to_string(), n);
                }
            }
        }
    }
    ChildMetrics { num, arm }
}

#[allow(clippy::too_many_arguments)]
fn run_one_child(arm: &str, repetition: usize) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R31_10_ARM", arm)
        .env("R31_10_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R31-10 child (arm={arm}, rep={repetition}) failed with status {:?}; see stderr above",
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

fn kib_to_mib(kib: u64) -> f64 {
    kib as f64 / 1024.0
}

fn run_orchestrator() {
    let arms = ["TRIM", "NO_TRIM"];
    let total_burst_mib = (LARGE_OBJ_BYTES * LARGE_OBJ_COUNT) / (1024 * 1024);

    println!("=== R31-10 trim_current_thread() RSS gate — subprocess-per-arm isolation ===");
    println!(
        "arms: {arms:?} × {REPETITIONS} reps; LARGE_OBJ={LARGE_OBJ_BYTES} bytes × {LARGE_OBJ_COUNT} \
         = {total_burst_mib} MiB/burst; IDLE_MS={IDLE_MS}"
    );
    println!();

    let mut all: Vec<ChildMetrics> = Vec::new();
    for &arm in &arms {
        for rep in 0..REPETITIONS {
            eprintln!("--- arm={arm} rep={rep}/{REPETITIONS} ---");
            let m = run_one_child(arm, rep);
            assert_eq!(m.arm, arm, "wrong arm");
            assert_eq!(m.get("repetition"), rep as u64, "wrong repetition");
            all.push(m);
        }
    }

    // --- Aggregated table (median of REPETITIONS per arm) ---
    println!();
    println!("=== aggregated (median of {REPETITIONS} reps per arm) ===");
    println!(
        "{:>8} {:>12} {:>12} {:>12} {:>12} {:>14} {:>14} {:>14} {:>14}",
        "arm",
        "rss_burst1",
        "rss_idle",
        "rss_burst2",
        "cmt_burst1",
        "cmt_idle",
        "rss_drop",
        "cmt_drop",
        "act_released",
    );

    let mut trim_rss_idle = 0u64;
    let mut notrim_rss_idle = 0u64;
    let mut trim_commit_idle = 0u64;
    let mut notrim_commit_idle = 0u64;
    let mut trim_action_released = 0u64;
    let mut notrim_action_released = 0u64;

    for &arm in &arms {
        let cell: Vec<&ChildMetrics> = all.iter().filter(|m| m.arm == arm).collect();
        let pick = |k: &str| -> Vec<u64> { cell.iter().map(|m| m.get(k)).collect() };

        let mut rss1 = pick("rss_burst1_kib");
        let mut rssi = pick("rss_idle_kib");
        let mut rss2 = pick("rss_burst2_kib");
        let mut cmt1 = pick("commit_burst1_kib");
        let mut cmti = pick("commit_idle_kib");
        let mut ard = pick("action_released_delta");

        let (rss1_m, rssi_m, rss2_m, cmt1_m, cmti_m, ard_m) = (
            median(&mut rss1),
            median(&mut rssi),
            median(&mut rss2),
            median(&mut cmt1),
            median(&mut cmti),
            median(&mut ard),
        );

        let rss_drop = rss1_m.saturating_sub(rssi_m);
        let cmt_drop = cmt1_m.saturating_sub(cmti_m);

        println!(
            "{:>8} {:>9.1} MiB {:>9.1} MiB {:>9.1} MiB {:>9.1} MiB {:>9.1} MiB {:>9.1} MiB {:>9.1} MiB {:>14}",
            arm,
            kib_to_mib(rss1_m),
            kib_to_mib(rssi_m),
            kib_to_mib(rss2_m),
            kib_to_mib(cmt1_m),
            kib_to_mib(cmti_m),
            kib_to_mib(rss_drop),
            kib_to_mib(cmt_drop),
            ard_m,
        );

        // Stash for the headline comparison.
        match arm {
            "TRIM" => {
                trim_rss_idle = rssi_m;
                trim_commit_idle = cmti_m;
                trim_action_released = ard_m;
            }
            "NO_TRIM" => {
                notrim_rss_idle = rssi_m;
                notrim_commit_idle = cmti_m;
                notrim_action_released = ard_m;
            }
            _ => {}
        }
    }

    // --- Headline: RSS/commit win of TRIM vs NO_TRIM at idle ---
    println!();
    println!("=== headline: RSS/commit win at idle (TRIM vs NO_TRIM) ===");

    let rss_win_kib = notrim_rss_idle.saturating_sub(trim_rss_idle);
    let commit_win_kib = notrim_commit_idle.saturating_sub(trim_commit_idle);

    println!(
        "rss_idle:   TRIM={:.1} MiB  NO_TRIM={:.1} MiB  →  win={:.1} MiB ({})",
        kib_to_mib(trim_rss_idle),
        kib_to_mib(notrim_rss_idle),
        kib_to_mib(rss_win_kib),
        if rss_win_kib > 0 {
            "RSS RECLAIMED"
        } else {
            "no RSS win"
        },
    );
    println!(
        "commit_idle: TRIM={:.1} MiB  NO_TRIM={:.1} MiB  →  win={:.1} MiB ({})",
        kib_to_mib(trim_commit_idle),
        kib_to_mib(notrim_commit_idle),
        kib_to_mib(commit_win_kib),
        if commit_win_kib > 0 {
            "COMMIT RECLAIMED"
        } else {
            "no commit win"
        },
    );

    // Assert the headline: the trim arm's action_released_delta must be > 0
    // (mechanism oracle for the TRIM arm — CLAUDE.md R30-8 path-activation
    // rule).
    assert!(
        trim_action_released > 0,
        "TRIM arm mechanism oracle FAILED: action_released_delta={trim_action_released} (expected > 0)"
    );

    // --- Between-arm mechanism delta (CLAUDE.md R30-8 rule) ---
    println!();
    println!("=== between-arm mechanism delta (CLAUDE.md R30-8 rule) ===");
    println!(
        "TRIM action_released_delta={trim_action_released}  vs  NO_TRIM action_released_delta={notrim_action_released}  \
         (delta={})",
        trim_action_released.saturating_sub(notrim_action_released),
    );

    // --- Raw CSV dump (every child run, for the summary CSV) ---
    println!();
    println!("=== raw CSV (all {n} child runs) ===", n = all.len());
    let cols = [
        "arm",
        "repetition",
        "large_obj_bytes",
        "large_obj_count",
        "total_burst_bytes",
        "idle_ms",
        "rss_before_kib",
        "commit_before_kib",
        "rss_burst1_kib",
        "commit_burst1_kib",
        "action_released_delta",
        "rss_idle_kib",
        "commit_idle_kib",
        "idle_released_delta",
        "rss_burst2_kib",
        "commit_burst2_kib",
        "rss_drop_burst1_to_idle_kib",
        "commit_drop_burst1_to_idle_kib",
    ];
    println!("# {}", cols.join(","));
    for m in &all {
        let row: Vec<String> = cols
            .iter()
            .map(|c| {
                if *c == "arm" {
                    m.arm.clone()
                } else {
                    m.get(c).to_string()
                }
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    println!(
        "NOTE: each (arm, rep) cell ran in its OWN freshly-spawned OS process \
         (subprocess-per-arm isolation). The TRIM arm hard-asserted action_released_delta > 0 \
         (the eviction mechanism fired — CLAUDE.md R30-8 path-activation oracle). Entry point: \
         SeferAlloc (direct GlobalAlloc::alloc/dealloc + trim_current_thread)."
    );
}

fn main() {
    let child_mode = std::env::var_os("R31_10_ARM").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
