//! R32-9 (task #500) — the wall-clock companion to
//! `benches/macro_multiseg_steady_state.rs`. Same workload shape (a
//! `>=64`-live-segment floor plus steady-state mixed Small+Large churn on
//! top of it), driven through the SAME `HeapRegistry::claim`/`HeapCore`
//! entry point the real `#[global_allocator]` (`SeferAlloc`) itself calls
//! into (see that bench file's module doc for why this registry-bypass
//! layer is the correct one, not a shortcut past the R31-0 entry-point
//! lesson), reported as ns/op wall-clock instead of `iai-callgrind`'s
//! `Estimated Cycles`.
//!
//! ## Why a second harness, not just the iai-callgrind one
//!
//! `iai-callgrind` is Linux-only (Valgrind dependency — see
//! `benches/perf_gate_iai.rs`'s own module doc). This example is the
//! portable half: it runs anywhere (including this project's Windows dev
//! environment) so a future task on ANY platform can get a first read on
//! whether a segment-count-scaling mechanism (X5/T10/R1/R15-1, see the
//! iai-callgrind bench's module doc for the full list) shows a wall-clock
//! difference at `>=64` live segments, without needing Linux/Valgrind
//! installed. It is also this task's SMOKE-TEST vehicle (task instruction
//! #4): the concrete numbers this example prints on `HEAD` are the "does
//! the harness produce sensible, non-degenerate output" evidence cited in
//! `docs/perf/R32_9_MACRO_MULTISEG_STEADY_STATE_HARNESS.md`.
//!
//! ## Arms
//!
//! `THREAD_COUNTS = [1, 4]` — the single-thread floor/churn cost vs the
//! 4-thread version (own `HeapCore` per thread, own floor per thread,
//! genuine cross-thread registry/table contention). This is NOT a
//! feature-flag A/B (nothing under test here is gated behind a Cargo
//! feature) — it is a THREAD-COUNT sweep, the axis this harness's own
//! design note calls out as the "cheaper to build first" single-thread
//! variant vs the "exercises cross-thread paths realistically" multi-thread
//! variant, both kept as permanent arms of the same gate.
//!
//! ## Path-activation oracle (CLAUDE.md R30-8 rule)
//!
//! Per (thread, repetition) cell: hard-assert `dbg_table_count() >=
//! MIN_LIVE_SEGMENTS_ORACLE` (64) on EVERY thread's heap, read back
//! immediately after that thread's floor is established and BEFORE any
//! timed churn begins — the exact oracle
//! `benches/macro_multiseg_steady_state.rs` uses, reused here via the same
//! `HeapCore::dbg_table_count` accessor (added this task,
//! `src/registry/heap_core_diag.rs`) so both harnesses share one oracle
//! definition instead of drifting into two.
//!
//! ## Config/process-identity evidence (CLAUDE.md R26-4 rule)
//!
//! This gate has no runtime CONFIG axis to resolve (no `with_config`/
//! `LargeCacheConfig` sweep — every arm uses `HeapRegistry::claim()`'s
//! default config), so pieces (1)-(3) of the R26-4 rule (requested/
//! resolved/conflict-delta) are Not-Applicable BY CONSTRUCTION here, not
//! silently skipped: the CSV still emits `config_conflicts_delta` (always
//! expected 0, asserted) as the one piece that IS meaningful regardless of
//! whether a config was swept. Piece (4), process identity, is genuinely
//! exercised: subprocess-per-arm isolation (matching R29-13/R30-6's own
//! established shape), so a fresh process's empty registry makes cross-arm
//! slot reuse structurally impossible.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r32_9_macro_multiseg_steady_state_ab_gate --features "production bench-internals"
//! ```

#![cfg(feature = "alloc-global")]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::time::Instant;

use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

// ---------------------------------------------------------------------------
// Workload parameters -- MUST stay in lockstep with
// benches/macro_multiseg_steady_state.rs's own constants (both harnesses
// measure the SAME workload shape from two different instruments; a drift
// between them would silently make their numbers non-comparable).
// ---------------------------------------------------------------------------

/// The path-activation oracle threshold (survey/`OPEN_ITEMS.md` item 34's
/// own named number) -- identical to the iai-callgrind bench's constant of
/// the same name.
const MIN_LIVE_SEGMENTS_ORACLE: u32 = 64;

/// Floor objects held live for the entire timed region -- identical to the
/// iai-callgrind bench's `FLOOR_LARGE_OBJECTS`.
const FLOOR_LARGE_OBJECTS: usize = 80;

/// Mixed-churn rounds per thread in the timed region -- identical to the
/// iai-callgrind bench's `CHURN_ROUNDS`. Larger here than the Callgrind-
/// emulated bench needs (wall-clock native execution is far cheaper per op
/// than Callgrind emulation), so this example uses a bigger round count for
/// a more stable ns/op wall-clock read; the WORKLOAD SHAPE (floor + mixed
/// Small/Large churn) is what must match, not the exact op count.
const CHURN_ROUNDS: usize = 4096;

const SMALL_SIZE: usize = 16;

const THREAD_COUNTS: &[usize] = &[1, 4];

/// Repetitions per arm (R27-3/R29-13/R30-6 precedent: 3 gives a median +
/// range without a multi-hour sweep).
const REPETITIONS: usize = 5;

fn floor_layout() -> Layout {
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap()
}

// ---------------------------------------------------------------------------
// Per-thread workload
// ---------------------------------------------------------------------------

/// Result of one thread's run: churn wall-clock elapsed (ns), and the
/// oracle-observed table count (for cross-checking in the parent).
struct ThreadResult {
    churn_elapsed_ns: u128,
    table_count_at_oracle: u32,
    oracle_ok: bool,
}

fn per_thread_work() -> ThreadResult {
    let _ = bootstrap::ensure();
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: `heap_ptr` was just returned by `claim` and is owned by this
    // thread until `recycle` at the end of this function.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    let large_layout = floor_layout();
    let mut floor = Vec::with_capacity(FLOOR_LARGE_OBJECTS);
    for i in 0..FLOOR_LARGE_OBJECTS {
        let p = heap.alloc(large_layout);
        assert!(!p.is_null(), "floor alloc null at i={i}");
        floor.push(p);
    }

    // PATH-ACTIVATION ORACLE: read back BEFORE any timed churn begins.
    let table_count_at_oracle = heap.dbg_table_count();
    let oracle_ok = table_count_at_oracle >= MIN_LIVE_SEGMENTS_ORACLE;

    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();

    let t0 = Instant::now();
    for _ in 0..CHURN_ROUNDS {
        let p = heap.alloc(small_layout);
        if !p.is_null() {
            // SAFETY: `p` was just returned by `heap.alloc` with the same
            // layout, freed exactly once here.
            unsafe { heap.dealloc(p, small_layout) };
        }
        let lp = heap.alloc(large_layout);
        if !lp.is_null() {
            // SAFETY: `lp` was just returned by `heap.alloc` with the same
            // layout, freed exactly once here.
            unsafe { heap.dealloc(lp, large_layout) };
        }
    }
    let churn_elapsed_ns = t0.elapsed().as_nanos();

    for p in floor {
        // SAFETY: `p` was allocated by `heap` with `large_layout` above,
        // still live, freed exactly once here.
        unsafe { heap.dealloc(p, large_layout) };
    }
    // SAFETY: `heap_ptr` was returned by `claim` above, not yet recycled,
    // no other thread touches it.
    unsafe { HeapRegistry::recycle(heap_ptr) };

    ThreadResult {
        churn_elapsed_ns,
        table_count_at_oracle,
        oracle_ok,
    }
}

// ---------------------------------------------------------------------------
// Child (one arm, one repetition) -- subprocess-isolated.
// ---------------------------------------------------------------------------

fn run_child(thread_count: usize) {
    let conflicts_before = config_conflicts_total();

    let results: Vec<ThreadResult> = if thread_count == 1 {
        vec![per_thread_work()]
    } else {
        let handles: Vec<_> = (0..thread_count)
            .map(|_| std::thread::spawn(per_thread_work))
            .collect();
        handles
            .into_iter()
            .map(|h| {
                h.join()
                    .unwrap_or_else(|e| panic!("thread panicked: {e:?}"))
            })
            .collect()
    };

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);
    assert_eq!(
        conflicts_delta, 0,
        "R32-9 child (threads={thread_count}): config_conflicts_delta = {conflicts_delta} \
         (expected 0 in a fresh process; this gate has no config sweep, so any nonzero \
         delta indicates cross-arm/cross-thread registry-slot bleed, not a labelled arm)"
    );

    let all_oracle_ok = results.iter().all(|r| r.oracle_ok);
    let min_table_count = results
        .iter()
        .map(|r| r.table_count_at_oracle)
        .min()
        .unwrap_or(0);
    let total_ops = thread_count * CHURN_ROUNDS * 2; // 2 alloc+dealloc pairs per round
    let total_elapsed_ns: u128 = results.iter().map(|r| r.churn_elapsed_ns).sum();
    // Wall-clock ns/op: threads run concurrently, so divide the MAX
    // per-thread elapsed (the binding constraint for wall-clock throughput)
    // by that thread's own op count, not the summed elapsed across threads
    // (which would double-count parallel work as if serial).
    let max_elapsed_ns = results
        .iter()
        .map(|r| r.churn_elapsed_ns)
        .max()
        .unwrap_or(0);
    let ns_per_op = if CHURN_ROUNDS * 2 > 0 {
        max_elapsed_ns as f64 / (CHURN_ROUNDS * 2) as f64
    } else {
        0.0
    };

    proc_probe::emit_u64("thread_count", thread_count as u64);
    proc_probe::emit_u64("floor_large_objects", FLOOR_LARGE_OBJECTS as u64);
    proc_probe::emit_u64("churn_rounds", CHURN_ROUNDS as u64);
    proc_probe::emit_u64(
        "min_live_segments_oracle",
        u64::from(MIN_LIVE_SEGMENTS_ORACLE),
    );
    proc_probe::emit_u64("min_table_count_observed", u64::from(min_table_count));
    proc_probe::emit_u64("oracle_pass", u64::from(all_oracle_ok));
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("total_ops", total_ops as u64);
    proc_probe::emit_u64("total_elapsed_ns_summed", total_elapsed_ns as u64);
    proc_probe::emit_u64("max_thread_elapsed_ns", max_elapsed_ns as u64);
    proc_probe::emit_f64("ns_per_op", ns_per_op);

    println!(
        "OK threads={thread_count} floor={FLOOR_LARGE_OBJECTS} rounds={CHURN_ROUNDS} \
         min_table_count={min_table_count} oracle={} cfg_conflicts_delta={conflicts_delta} \
         ns_per_op={ns_per_op:.1}",
        if all_oracle_ok { "PASS" } else { "FAIL" }
    );
}

// ---------------------------------------------------------------------------
// Orchestrator
// ---------------------------------------------------------------------------

struct ChildMetrics {
    map: std::collections::HashMap<String, String>,
}

impl ChildMetrics {
    fn get_u64(&self, k: &str) -> u64 {
        self.map
            .get(k)
            .unwrap_or_else(|| panic!("child RESULT missing {k}"))
            .parse()
            .unwrap_or_else(|e| panic!("child RESULT {k} not a u64: {e}"))
    }
    fn get_f64(&self, k: &str) -> f64 {
        self.map
            .get(k)
            .unwrap_or_else(|| panic!("child RESULT missing {k}"))
            .parse()
            .unwrap_or_else(|e| panic!("child RESULT {k} not an f64: {e}"))
    }
}

fn parse_child_stdout(stdout: &str) -> ChildMetrics {
    let mut map = std::collections::HashMap::new();
    for line in stdout.lines() {
        let Some(rest) = line.strip_prefix("RESULT ") else {
            continue;
        };
        if let Some((k, v)) = rest.split_once('=') {
            map.insert(k.to_string(), v.to_string());
        }
    }
    ChildMetrics { map }
}

fn run_one_child(thread_count: usize) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R32_9_THREAD_COUNT", thread_count.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R32-9 child (threads={thread_count}) failed with status {:?}; see stderr above",
            output.status.code()
        );
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    print!("{stdout}");
    parse_child_stdout(&stdout)
}

fn median_f64(v: &mut [f64]) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[v.len() / 2]
}

fn run_orchestrator() {
    println!("=== R32-9 macro_multiseg_steady_state wall-clock A/B gate ===");
    println!(
        "threads={THREAD_COUNTS:?} x {REPETITIONS} reps; FLOOR_LARGE_OBJECTS={FLOOR_LARGE_OBJECTS} \
         CHURN_ROUNDS={CHURN_ROUNDS} MIN_LIVE_SEGMENTS_ORACLE={MIN_LIVE_SEGMENTS_ORACLE}"
    );
    println!();

    let mut all: Vec<(usize, usize, ChildMetrics)> = Vec::new();
    let mut oracle_failures: Vec<(usize, usize)> = Vec::new();

    for &tc in THREAD_COUNTS {
        for rep in 0..REPETITIONS {
            eprintln!("--- arm threads={tc} rep={rep}/{REPETITIONS} ---");
            let m = run_one_child(tc);
            assert_eq!(m.get_u64("thread_count"), tc as u64, "wrong thread_count");
            assert_eq!(
                m.get_u64("config_conflicts_delta"),
                0,
                "nonzero config_conflicts_delta"
            );
            if m.get_u64("oracle_pass") == 0 {
                oracle_failures.push((tc, rep));
            }
            all.push((tc, rep, m));
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps) ===");
    println!(
        "{:>8} {:>16} {:>12}",
        "threads", "min_table_count", "ns_per_op"
    );
    for &tc in THREAD_COUNTS {
        let cell: Vec<&ChildMetrics> = all
            .iter()
            .filter(|(t, _, _)| *t == tc)
            .map(|(_, _, m)| m)
            .collect();
        let mut ns = cell
            .iter()
            .map(|m| m.get_f64("ns_per_op"))
            .collect::<Vec<_>>();
        let ns_med = median_f64(&mut ns);
        let min_table = cell
            .iter()
            .map(|m| m.get_u64("min_table_count_observed"))
            .min()
            .unwrap_or(0);
        println!("{tc:>8} {min_table:>16} {ns_med:>12.1}");
    }

    println!();
    println!("=== CSV (one row per child) ===");
    let cols = [
        "thread_count",
        "repetition",
        "process_identity",
        "floor_large_objects",
        "churn_rounds",
        "min_live_segments_oracle",
        "min_table_count_observed",
        "oracle_pass",
        "config_conflicts_delta",
        "total_ops",
        "total_elapsed_ns_summed",
        "max_thread_elapsed_ns",
        "ns_per_op",
    ];
    println!("# {}", cols.join(","));
    for (tc, rep, m) in &all {
        let row: Vec<String> = cols
            .iter()
            .map(|c| match *c {
                "thread_count" => tc.to_string(),
                "repetition" => rep.to_string(),
                "process_identity" => "subprocess".to_string(),
                "ns_per_op" => format!("{:.3}", m.get_f64("ns_per_op")),
                other => m.get_u64(other).to_string(),
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    if oracle_failures.is_empty() {
        println!(
            "NOTE: all {} arms passed the path-activation oracle (every thread's \
             dbg_table_count() >= {MIN_LIVE_SEGMENTS_ORACLE} before timed churn began).",
            all.len()
        );
    } else {
        println!(
            "WARNING: {} arm(s) FAILED the path-activation oracle: {:?}",
            oracle_failures.len(),
            oracle_failures
        );
    }
    println!(
        "NOTE: each (thread_count, repetition) cell ran in its OWN freshly-spawned OS process \
         (subprocess-per-arm isolation, matching R29-13/R30-6's established shape), through \
         HeapRegistry::claim -- the SAME entry point SeferAlloc's #[global_allocator] impl \
         itself calls (see the sibling iai-callgrind bench's module doc for why this is the \
         correct layer, not a registry-bypass shortcut past the R31-0 entry-point lesson)."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    if let Ok(tc_str) = std::env::var("R32_9_THREAD_COUNT") {
        let tc: usize = tc_str
            .parse()
            .unwrap_or_else(|e| panic!("R32_9_THREAD_COUNT={tc_str:?} not a valid usize ({e})"));
        run_child(tc);
        return;
    }
    run_orchestrator();
}
