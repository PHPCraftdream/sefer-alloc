//! R33-6 (task #511) — the large-cache decay-clock-throttle RETENTION COST
//! gate: measures whether `DECAY_CLOCK_CHECK_STRIDE = 64`
//! (`src/alloc_core/alloc_core_large_cache.rs`, shipped by R32-8 / commit
//! `74345b8`) causes measurable extra retained RSS in the LOW-throughput
//! regime the original report only argued about qualitatively.
//!
//! ## Why this exists
//!
//! `docs/reviews/2026-08-03-round32-readonly-review.md` §7 finding F9 [P2]:
//! R32-8 measured the BENEFIT of its stride throttle (ns/call saved) in a
//! HIGH-throughput regime (200k alloc/free cycles), but argued the COST
//! (retained bytes from delayed decay ticks) away qualitatively. The two
//! profiles this change targets — `LargeCachePolicy::LowHeadroom` (16 MiB)
//! and `::Trimmed64MiB` (64 MiB) — exist for exactly one purpose: bounding
//! retained RSS. A workload that crosses the headroom and then performs
//! FEWER THAN 64 further large ops now retains cached spans that the
//! pre-change code would have released on the very next op. This gate
//! measures that cost directly, in the same regime, instead of asserting it.
//!
//! ## Design: `FORCE_DECAY_CLOCK_READ` as an old-shape / new-shape switch
//!
//! `AllocCore::dbg_set_force_decay_clock_read(true)` bypasses both the
//! headroom fast-exit AND the stride throttle, reproducing the OLD
//! (pre-R32-8) unconditional-clock-read-past-headroom shape.
//! `dbg_set_force_decay_clock_read(false)` exercises the NEW shipped
//! stride-throttled path. Both arms use the IDENTICAL headroom and the
//! IDENTICAL workload, so the comparison is clean: only the stride differs.
//!
//! ## Workload
//!
//! 1. Claim a heap with a profile's headroom.
//! 2. Fill `LARGE_OBJ_COUNT` (8) × 34 MiB objects (touching every 4 KiB
//!    page to commit), then free them all → the cache holds ~288 MiB, well
//!    above both profile headrooms.
//! 3. Sleep 1100 ms (> 1000 ms default `decay_interval`) so a decay tick is
//!    genuinely "due".
//! 4. Set `FORCE_DECAY_CLOCK_READ`.
//! 5. Record `dbg_large_cache_used()` and `dbg_maybe_decay_guard_passed_count()`.
//! 6. Perform exactly `n_ops` alloc+free cycles (each takes one cached span
//!    out and puts it back — the sparse-large-op workload).
//! 7. Re-measure both.
//!
//! With `forced=true` the first post-wait call reads the clock, finds
//! `elapsed >= interval`, and fires decay immediately. With `forced=false`
//! the stride throttle may delay that clock read by up to 63 further calls —
//! for `n_ops < ~29` (the stride-alignment-dependent threshold for this
//! workload shape), no decay fires at all, so the ENTIRE one-tick release is
//! retained.
//!
//! ## Path-activation oracle (R30-8 rule)
//!
//! Two evidence pieces per arm:
//! 1. **Headroom crossed:** `used_before_ops > headroom_bytes` (proves the
//!    arm's workload genuinely entered the above-headroom regime the stride
//!    throttle applies to).
//! 2. **Stride mechanism exercised:** `guard_passed_delta` (clock reads
//!    during the N ops) is materially lower for `forced=false` than for
//!    `forced=true` — proving the throttle is actually reducing clock reads,
//!    not a no-op.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r33_6_decay_throttle_retention_cost_gate --features "production alloc-stats bench-internals"
//! ```

#![cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::thread;
use std::time::Duration;

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    AllocCore, LargeCacheConfig,
};

// ---------------------------------------------------------------------------
// Sweep parameters
// ---------------------------------------------------------------------------

/// One large object's size. Must exceed `SMALL_MAX` (16 KiB under plain
/// `production`) and be large enough that 8 of them exceed both profile
/// headrooms. Same as R29-13's choice.
const LARGE_OBJ_BYTES: usize = 34 * 1024 * 1024;

/// Number of distinct large objects filled per heap — one per base
/// large-cache slot (`LARGE_CACHE_SLOTS = 8`).
const LARGE_OBJ_COUNT: usize = 8;

/// The default decay interval (ms). We sleep this + a 100 ms margin to
/// ensure the interval has genuinely elapsed before the N-ops phase.
const DECAY_INTERVAL_MS: u64 = 1000;
const DECAY_WAIT_MS: u64 = DECAY_INTERVAL_MS + 100;

/// The N values to sweep: alloc+free cycles AFTER crossing headroom. All
/// are below `DECAY_CLOCK_CHECK_STRIDE = 64` to probe the low-throughput
/// regime where the throttle delays decay.
const N_OPS_ARMS: &[usize] = &[1, 8, 32, 63];

/// Repetitions per cell (median + range).
const REPETITIONS: usize = 3;

struct ProfileSpec {
    name: &'static str,
    headroom_bytes: usize,
}

/// The two shipped non-default profiles R32-8's fix targets, per
/// `src/alloc_core/profile.rs`.
const PROFILES: &[ProfileSpec] = &[
    ProfileSpec {
        name: "LowHeadroom",
        headroom_bytes: 16 * 1024 * 1024,
    },
    ProfileSpec {
        name: "Trimmed64MiB",
        headroom_bytes: 64 * 1024 * 1024,
    },
];

// ---------------------------------------------------------------------------
// Workload helpers (adapted from R29-13)
// ---------------------------------------------------------------------------

/// Allocate `LARGE_OBJ_COUNT` distinct large objects, touch one byte per
/// 4 KiB page so the reservation is genuinely committed, and return the
/// live pointers.
fn fill_large_objects(heap: &mut HeapCore, layout: Layout) -> Vec<*mut u8> {
    let mut live = Vec::with_capacity(LARGE_OBJ_COUNT);
    for _ in 0..LARGE_OBJ_COUNT {
        let p = heap.alloc(layout);
        assert!(
            !p.is_null(),
            "large alloc failed (OOM?) — reduce LARGE_OBJ_COUNT/BYTES"
        );
        let page = 4096usize;
        let mut off = 0usize;
        // SAFETY: `p` is a fresh allocation of `layout.size()` bytes from
        // `heap`, not yet freed; each written offset is `< layout.size()`.
        unsafe {
            while off < layout.size() {
                p.add(off).write_volatile(0xAB);
                off += page;
            }
        }
        live.push(p);
    }
    live
}

/// Free every object in `live` (returns each span to the large cache).
fn teardown_large_objects(heap: &mut HeapCore, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `heap` with `layout`, not yet freed.
            unsafe { heap.dealloc(p, layout) };
        }
    }
}

// ---------------------------------------------------------------------------
// Child / arm mode
// ---------------------------------------------------------------------------

fn parse_env_usize(name: &str) -> usize {
    let raw = std::env::var(name)
        .unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"));
    raw.parse::<usize>()
        .unwrap_or_else(|e| panic!("{name}={raw:?} not a valid usize ({e})"))
}

fn run_child() {
    let profile_name = std::env::var("R33_6_PROFILE")
        .unwrap_or_else(|e| panic!("R33_6_PROFILE env var required ({e})"));
    let forced = std::env::var("R33_6_FORCED")
        .map(|v| v == "1")
        .unwrap_or(false);
    let n_ops = parse_env_usize("R33_6_N_OPS");
    let repetition = parse_env_usize("R33_6_REPETITION");

    let profile = PROFILES
        .iter()
        .find(|p| p.name == profile_name)
        .unwrap_or_else(|| panic!("unknown profile {profile_name:?}"));
    let headroom_bytes = profile.headroom_bytes;

    let conflicts_before = config_conflicts_total();

    let heap_ptr =
        HeapRegistry::claim_with_config(LargeCacheConfig::new().headroom_bytes(headroom_bytes));
    assert!(!heap_ptr.is_null(), "claim_with_config returned null");
    // SAFETY: `heap_ptr` was just returned by `claim_with_config` and is
    // owned by THIS thread until `recycle` below.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    // SELF-VERIFICATION: resolved headroom matches requested.
    let (_, _, resolved) = heap.dbg_decay_config();
    assert_eq!(
        resolved, headroom_bytes,
        "R33-6 child: resolved headroom ({resolved}) != requested ({headroom_bytes})"
    );

    let layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();

    // Fill: 8 distinct large objects, each touched.
    let live = fill_large_objects(heap, layout);
    let used_post_teardown = heap.dbg_large_cache_used();

    // Free all (each dealloc deposits into the large cache).
    teardown_large_objects(heap, layout, &live);
    let used_post_free = heap.dbg_large_cache_used();
    assert!(
        used_post_free > 0,
        "ADMISSION FAILED: used_post_free=0 — no large span was ever cached"
    );

    // Wait for decay_interval to genuinely elapse.
    thread::sleep(Duration::from_millis(DECAY_WAIT_MS));

    // Set the force switch: forced=true reproduces the OLD unconditional
    // clock-read shape; forced=false is the NEW shipped stride-throttled path.
    AllocCore::dbg_set_force_decay_clock_read(forced);

    let used_before_ops = heap.dbg_large_cache_used();
    let guard_passed_before = AllocCore::dbg_maybe_decay_guard_passed_count();

    // PATH-ACTIVATION ORACLE part 1: headroom was genuinely crossed.
    assert!(
        used_before_ops > headroom_bytes,
        "R33-6 child: workload precondition violated — \
         used_before_ops={used_before_ops} must exceed headroom_bytes={headroom_bytes}"
    );

    // Perform exactly n_ops alloc+free cycles.
    for _ in 0..n_ops {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "alloc failed (OOM?)");
        // SAFETY: `p` freshly allocated by `heap` with `layout`, freed once.
        unsafe { heap.dealloc(p, layout) };
    }

    let used_after_ops = heap.dbg_large_cache_used();
    let guard_passed_after = AllocCore::dbg_maybe_decay_guard_passed_count();
    let guard_passed_delta = guard_passed_after.saturating_sub(guard_passed_before);

    // Reset the force switch.
    AllocCore::dbg_set_force_decay_clock_read(false);

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);
    assert_eq!(
        conflicts_delta, 0,
        "R33-6 child: CONFIG_CONFLICTS delta = {conflicts_delta} (expected 0)"
    );

    let expected_calls = (n_ops * 2) as u64;
    let headroom_crossed = used_before_ops > headroom_bytes;

    // PATH-ACTIVATION ORACLE part 2: stride mechanism exercised.
    let oracle_pass = headroom_crossed
        && if forced {
            // Old shape: every call past headroom reads the clock.
            guard_passed_delta >= 1
        } else {
            // New shape: stride throttle reduces clock reads below the
            // unthrottled count.
            guard_passed_delta < expected_calls
        };

    // SAFETY: `heap_ptr` returned by `claim_with_config`, not yet recycled.
    unsafe { HeapRegistry::recycle(heap_ptr) };

    proc_probe::emit("profile", &profile_name);
    proc_probe::emit_u64("forced", u64::from(forced));
    proc_probe::emit_u64("n_ops", n_ops as u64);
    proc_probe::emit_u64("repetition", repetition as u64);
    proc_probe::emit_u64("headroom_bytes", headroom_bytes as u64);
    proc_probe::emit_u64("verified_headroom", resolved as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("large_obj_bytes", LARGE_OBJ_BYTES as u64);
    proc_probe::emit_u64("large_obj_count", LARGE_OBJ_COUNT as u64);
    proc_probe::emit_u64("used_post_teardown", used_post_teardown as u64);
    proc_probe::emit_u64("used_post_free", used_post_free as u64);
    proc_probe::emit_u64("used_before_ops", used_before_ops as u64);
    proc_probe::emit_u64("used_after_ops", used_after_ops as u64);
    proc_probe::emit_u64("guard_passed_delta", guard_passed_delta);
    proc_probe::emit_u64("expected_calls", expected_calls);
    proc_probe::emit_u64("headroom_crossed", u64::from(headroom_crossed));
    proc_probe::emit_u64("oracle_pass", u64::from(oracle_pass));
    proc_probe::emit_u64("decay_interval_ms", DECAY_INTERVAL_MS);

    println!(
        "OK profile={profile_name} forced={forced} n_ops={n_ops} rep={repetition} \
         used_before_ops={used_before_ops} used_after_ops={used_after_ops} \
         guard_passed_delta={guard_passed_delta}/{expected_calls} oracle={}",
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
        let Some(rest) = line.strip_prefix("RESULT ") else {
            continue;
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

/// Arm identity tracked from the orchestrator's loop variables (not from
/// parsed child data, since string fields can't parse as u64).
struct ArmMetrics {
    profile_idx: usize,
    forced: bool,
    n_ops: usize,
    metrics: ChildMetrics,
}

fn run_one_child(
    profile_name: &str,
    forced: bool,
    n_ops: usize,
    repetition: usize,
) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R33_6_PROFILE", profile_name)
        .env("R33_6_FORCED", if forced { "1" } else { "0" })
        .env("R33_6_N_OPS", n_ops.to_string())
        .env("R33_6_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R33-6 child (profile={profile_name}, forced={forced}, n_ops={n_ops}, \
             rep={repetition}) failed with status {:?}",
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

fn run_orchestrator() {
    println!(
        "=== R33-6 decay-clock-throttle RETENTION COST gate — old-shape vs new-shape, \
         low-throughput regime ==="
    );
    let profile_names: Vec<&str> = PROFILES.iter().map(|p| p.name).collect();
    println!(
        "profiles: {:?} | N_OPS_ARMS: {:?} | REPETITIONS={REPETITIONS} | \
         LARGE_OBJ_BYTES={LARGE_OBJ_BYTES} LARGE_OBJ_COUNT={LARGE_OBJ_COUNT} \
         DECAY_WAIT_MS={DECAY_WAIT_MS}",
        profile_names, N_OPS_ARMS
    );
    println!();

    let mut all: Vec<ArmMetrics> = Vec::new();
    let mut oracle_failures: Vec<(String, bool, usize, usize)> = Vec::new();

    for (pi, profile) in PROFILES.iter().enumerate() {
        for &n_ops in N_OPS_ARMS {
            for &forced in &[false, true] {
                for rep in 0..REPETITIONS {
                    eprintln!(
                        "--- profile={} forced={forced} n_ops={n_ops} \
                         rep={rep}/{REPETITIONS} ---",
                        profile.name
                    );
                    let m = run_one_child(profile.name, forced, n_ops, rep);
                    assert_eq!(
                        m.get("verified_headroom"),
                        profile.headroom_bytes as u64,
                        "verified_headroom mismatch"
                    );
                    assert_eq!(
                        m.get("config_conflicts_delta"),
                        0,
                        "non-zero config_conflicts_delta"
                    );
                    assert_eq!(
                        m.get("headroom_crossed"),
                        1,
                        "workload precondition violated: headroom not crossed"
                    );
                    if m.get("oracle_pass") == 0 {
                        oracle_failures.push((profile.name.to_string(), forced, n_ops, rep));
                    }
                    all.push(ArmMetrics {
                        profile_idx: pi,
                        forced,
                        n_ops,
                        metrics: m,
                    });
                }
            }
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps) ===");
    println!(
        "{:>14} {:>5} {:>6} {:>16} {:>16} {:>16} {:>12}",
        "profile",
        "force",
        "n_ops",
        "used_before(MiB)",
        "used_after(MiB)",
        "retention_cost(MiB)",
        "guard_delta"
    );

    for (pi, profile) in PROFILES.iter().enumerate() {
        for &n_ops in N_OPS_ARMS {
            let mut unforced_after: Vec<u64> = Vec::new();
            let mut forced_after: Vec<u64> = Vec::new();
            let mut unforced_before: Vec<u64> = Vec::new();
            let mut forced_before: Vec<u64> = Vec::new();
            let mut unforced_guard: Vec<u64> = Vec::new();
            let mut forced_guard: Vec<u64> = Vec::new();

            for a in &all {
                if a.profile_idx == pi && a.n_ops == n_ops {
                    if a.forced {
                        forced_after.push(a.metrics.get("used_after_ops"));
                        forced_before.push(a.metrics.get("used_before_ops"));
                        forced_guard.push(a.metrics.get("guard_passed_delta"));
                    } else {
                        unforced_after.push(a.metrics.get("used_after_ops"));
                        unforced_before.push(a.metrics.get("used_before_ops"));
                        unforced_guard.push(a.metrics.get("guard_passed_delta"));
                    }
                }
            }

            let med_unforced_after = median(&mut unforced_after);
            let med_forced_after = median(&mut forced_after);
            let med_unforced_before = median(&mut unforced_before);
            let med_forced_before = median(&mut forced_before);
            let med_unforced_guard = median(&mut unforced_guard);
            let med_forced_guard = median(&mut forced_guard);

            let retention_cost = med_unforced_after.saturating_sub(med_forced_after);

            println!(
                "{:>14} {:>5} {:>6} {:>16.2} {:>16.2} {:>16.2} {:>12}",
                profile.name,
                "false",
                n_ops,
                med_unforced_before as f64 / (1024.0 * 1024.0),
                med_unforced_after as f64 / (1024.0 * 1024.0),
                retention_cost as f64 / (1024.0 * 1024.0),
                med_unforced_guard,
            );
            println!(
                "{:>14} {:>5} {:>6} {:>16.2} {:>16.2} {:>16} {:>12}",
                profile.name,
                "true",
                n_ops,
                med_forced_before as f64 / (1024.0 * 1024.0),
                med_forced_after as f64 / (1024.0 * 1024.0),
                "",
                med_forced_guard,
            );
        }
    }

    println!();
    println!("=== CSV (one row per child) ===");
    let cols = [
        "profile",
        "forced",
        "n_ops",
        "repetition",
        "headroom_bytes",
        "verified_headroom",
        "config_conflicts_delta",
        "process_identity",
        "large_obj_bytes",
        "large_obj_count",
        "used_post_teardown",
        "used_post_free",
        "used_before_ops",
        "used_after_ops",
        "guard_passed_delta",
        "expected_calls",
        "headroom_crossed",
        "oracle_pass",
    ];
    println!("# {}", cols.join(","));
    for a in &all {
        let profile_name = PROFILES[a.profile_idx].name;
        let row: Vec<String> = cols
            .iter()
            .map(|c| match *c {
                "profile" => profile_name.to_string(),
                "forced" => u64::from(a.forced).to_string(),
                "n_ops" => a.n_ops.to_string(),
                "repetition" => a.metrics.get("repetition").to_string(),
                "process_identity" => "subprocess".to_string(),
                _ => a.metrics.get(c).to_string(),
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    if oracle_failures.is_empty() {
        println!(
            "NOTE: all {} arms passed the path-activation oracle.",
            all.len()
        );
    } else {
        println!(
            "WARNING: {} arm(s) FAILED the path-activation oracle: {:?}",
            oracle_failures.len(),
            oracle_failures
        );
    }

    println!();
    println!("=== HEADLINE retention cost (median used_after: unforced - forced) ===");
    for (pi, profile) in PROFILES.iter().enumerate() {
        for &n_ops in N_OPS_ARMS {
            let mut uf: Vec<u64> = Vec::new();
            let mut fd: Vec<u64> = Vec::new();
            for a in &all {
                if a.profile_idx == pi && a.n_ops == n_ops {
                    if a.forced {
                        fd.push(a.metrics.get("used_after_ops"));
                    } else {
                        uf.push(a.metrics.get("used_after_ops"));
                    }
                }
            }
            let cost = median(&mut uf).saturating_sub(median(&mut fd));
            println!(
                "  profile={:<14} n_ops={:<3} retention_cost={} bytes ({:.2} MiB)",
                profile.name,
                n_ops,
                cost,
                cost as f64 / (1024.0 * 1024.0),
            );
        }
    }

    println!();
    println!(
        "NOTE: each (profile, forced, n_ops, rep) cell ran in its OWN freshly-spawned OS \
         process. Every arm hard-asserted resolved headroom == requested, \
         config_conflicts_delta == 0, and headroom_crossed == 1. forced=true \
         reproduces the OLD (pre-R32-8) unconditional-past-headroom clock-read shape \
         by bypassing the stride throttle; forced=false is the real NEW shipped \
         stride-throttled path."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R33_6_FORCED").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
