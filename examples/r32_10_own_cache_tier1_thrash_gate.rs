//! R32-10 (task #501, F2) — the Large-heavy workload that structurally
//! forces `SegmentTable::contains_base`'s Tier-1 direct-mapped `own_cache`
//! to thrash, plus the before/after `OWN_CACHE_SIZE` A/B this file exists to
//! drive.
//!
//! ## Why `realloc` (in-place, same size), not free+alloc rotation — TWO
//! false starts corrected during this gate's own construction (kept here,
//! not scrubbed, per this project's honest-measurement-process convention)
//!
//! F2's own trigger condition (`docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md`
//! §F2) is explicit: "a workload with N concurrently-live Large objects has N
//! distinct hot bases going through this cache. At N > 4 the cache is
//! thrashing by construction." The natural first design — free-then-
//! reallocate every one of K rotating Large objects every round — turned out
//! to be STRUCTURALLY INCAPABLE of showing any `OWN_CACHE_SIZE` effect, for
//! two independent, compounding reasons discovered while building this file:
//!
//! **False start 1 — the double-call artifact.** Under `alloc-xthread +
//! fastbin` (both in `production`), ONE `HeapCore::dealloc` of a Large
//! object drives 2 `contains_base` calls back-to-back on the SAME base:
//! `dealloc_routing`'s own-thread check, then (because a Large object is not
//! `fastbin`-magazine-eligible) `dealloc_own_thread_with_base`'s "Large /
//! non-small / non-fastbin: delegate to core" fallthrough calls
//! `AllocCore::dealloc`, which RE-DERIVES `base` and RE-RUNS
//! `contains_base` from scratch (the same F6-shaped redundancy task #494
//! fixed for `realloc`'s move leg, left un-fixed here — noted for a future
//! F6-family follow-up, out of THIS task's scope). Call 2 always hits
//! (call 1 just filled the slot), so every dealloc contributes exactly one
//! guaranteed hit no matter what.
//!
//! **False start 2 — the deeper, structural reason: `own_cache` for a Large
//! object is invalidated by its OWN free, unconditionally.** Even after
//! accounting for false start 1 algebraically, a K-sweep straddling every
//! candidate `OWN_CACHE_SIZE` (4/8/16/24/32/48/64) STILL measured EXACTLY
//! 50.00% hit rate at every single K, including K=4 (well within even the
//! OLD 4-entry cache) — meaning the "real" first call was ALWAYS a genuine
//! miss, regardless of cache size or rotation width. The reason:
//! `AllocCore::dealloc`'s Large branch calls `self.table.unregister(base)`
//! UNCONDITIONALLY on every Large free (whether the segment is admitted to
//! `large_cache`, admission-rejected, or `alloc-decommit` is off entirely —
//! all three branches in `alloc_core.rs` call it), and `unregister` itself
//! calls `own_cache_clear(base)` — evicting the base from `own_cache` at the
//! END of the very dealloc call that just warmed it. So a base's cache slot
//! can NEVER survive from one free to the next visit of that base under a
//! free-then-realloc rotation: `own_cache` for a repeatedly-FREED Large
//! object is self-defeating by construction, independent of
//! `OWN_CACHE_SIZE`. Raising the cache size cannot help a workload shaped
//! this way — not because the cache is too small, but because nothing ever
//! stays IN the cache across two touches of the same base.
//!
//! **The actual correct thrashing shape: repeated in-place `realloc`
//! (same size) on K LIVE Large objects, never freed.** `HeapCore::realloc`'s
//! in-place success path (`try_realloc_inplace_known_base` returning
//! `Some`, OPT-G Large-grow-in-span with `new_size == old_size`) calls
//! `contains_base` EXACTLY ONCE per call and does NOT unregister the
//! segment — the object stays live and its base stays a candidate for a
//! warm cache hit on the object's NEXT visit, `K-1` other objects later.
//! This is the workload F2's own trigger condition actually describes: "N
//! concurrently-live Large objects" whose bases cycle through the
//! ownership-check cache, not N objects that get destroyed and rebuilt each
//! round.
//!
//! ## Path-activation oracle (CLAUDE.md R30-8 rule, applied twice)
//!
//! 1. **Config/mechanism-reachability**: `HeapCore::dbg_table_count() >= K`
//!    at the floor proves the rotation genuinely touches K distinct Large
//!    segments; additionally, every timed-region `realloc` call is asserted
//!    non-null and identical to its input pointer (`p_out == p_in`) — a
//!    structural proof the call actually took the in-place path (a moved
//!    pointer would be a DIFFERENT pointer and would itself indicate this
//!    workload silently stopped exercising the intended mechanism).
//! 2. **The actual claim under test**: `HeapCore::dbg_contains_base_tier1_hits`/
//!    `dbg_contains_base_tier1_misses` (new this task,
//!    `src/alloc_core/segment_table.rs`'s `contains_base`) report the REAL
//!    Tier-1 hit rate this workload drove through the real production
//!    ownership check, cross-checked against `EXPECTED_CALLS_PER_REALLOC`
//!    (asserted, not assumed). This is the missing instrument
//!    `docs/perf/R23_3_HOT_PATH_ATTRIBUTION_GATE.md` §1.3/§6.2 said could
//!    not be built "portably": that report is right that a benchmark cannot
//!    portably PREDICT which OS-assigned addresses will collide in the
//!    direct-mapped cache, but it does not need to — this harness OBSERVES
//!    the resulting hit rate after the fact via the new counter,
//!    sidestepping the prediction problem entirely.
//!
//! ## Config/process-identity evidence (CLAUDE.md R26-4 rule)
//!
//! No runtime CONFIG axis — every arm uses `HeapRegistry::claim()`'s default
//! config. `config_conflicts_delta` is still emitted (expected 0, asserted).
//! Each (K, repetition) cell runs in its OWN freshly spawned subprocess
//! (matching R29-13/R30-6/R32-9's established shape) — this matters doubly
//! here because `CONTAINS_BASE_TIER1_HITS`/`_MISSES` are PROCESS-WIDE
//! statics, not per-heap fields; subprocess isolation is what makes reading
//! them a valid per-arm measurement at all.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r32_10_own_cache_tier1_thrash_gate --features "production bench-internals"
//! ```
//!
//! `OWN_CACHE_SIZE` itself (`src/alloc_core/segment_table.rs`) is a
//! compile-time `pub(crate) const`, so measuring "before" (4) vs "after"
//! (the new value) requires two SEPARATE builds — this file measures
//! whichever `OWN_CACHE_SIZE` is compiled into the crate at build time; the
//! gate report cites which commit/worktree each run's numbers came from.

#![cfg(feature = "alloc-global")]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::time::Instant;

use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

// ---------------------------------------------------------------------------
// Workload parameters
// ---------------------------------------------------------------------------

/// The K (rotating-object-count) sweep, straddling every candidate
/// `OWN_CACHE_SIZE` this task considers (4 old default, 16/32 new
/// candidates): values at/just-above 4, at/just-above 16, at/just-above 32,
/// and one well past all of them (64) as the "should thrash regardless"
/// confirmation arm.
const K_VALUES: &[usize] = &[4, 8, 16, 24, 32, 48, 64];

/// Number of full rotation rounds in the timed region, per K. Each round
/// calls in-place `realloc` on EVERY one of the K live objects, round-robin.
/// Raised from an initial 512 to 8192 (a first pass at 512 produced a hit-
/// rate signal clean enough to be bit-identical/near-identical across
/// repetitions, but a NOISY ns/op signal on this single-shot `Instant::now`
/// Windows harness -- no clean latency delta was visible despite the
/// dramatic hit-rate delta; this larger round count averages out more of
/// that per-process timing noise for the latency axis specifically).
const ROTATING_ROUNDS: usize = 8192;

/// Repetitions per (K) arm (R27-3/R29-13/R30-6/R32-9 precedent).
const REPETITIONS: usize = 7;

/// Number of real `contains_base` calls ONE rotation-step in-place `realloc`
/// drives — see the module doc's "false start" sections for why this is 1
/// (not 2, unlike the rejected free+alloc design). Asserted via oracle #2,
/// not just assumed, so a future change to the realloc call graph that
/// alters this count is caught as a FAILING oracle rather than a silently
/// wrong `expected_total`.
const EXPECTED_CALLS_PER_REALLOC: usize = 1;

fn large_layout() -> Layout {
    // Just over the Small/Large boundary -- one dedicated SEGMENT each,
    // matching R32-9's own `floor_layout()` exactly (same size class the
    // large-cache's slots key on). Leaves headroom under `SEGMENT` so the
    // in-place OPT-G grow path (`end <= span_usable`) trivially succeeds for
    // a same-size realloc (`new_eff == old_eff` always satisfies
    // `new_eff >= old_eff`, and `end == payload_off + old_eff` was already
    // within the segment by construction of the original allocation).
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap()
}

// ---------------------------------------------------------------------------
// Child (one (K, repetition) cell) -- subprocess-isolated.
// ---------------------------------------------------------------------------

fn run_child(k: usize) {
    let _ = bootstrap::ensure();
    let conflicts_before = config_conflicts_total();

    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: `heap_ptr` was just returned by `claim` and is owned by this
    // thread (this process's only thread) until `recycle` at the end.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    let layout = large_layout();

    // Establish the rotation floor: K distinct Large objects, held LIVE for
    // the entire timed region (never freed until teardown).
    let mut slots: Vec<*mut u8> = Vec::with_capacity(k);
    for i in 0..k {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "floor alloc null at i={i} (k={k})");
        slots.push(p);
    }

    // PATH-ACTIVATION ORACLE #1 (mechanism-reachability): the floor
    // genuinely registered >= K distinct segments on THIS heap.
    let table_count_at_floor = heap.dbg_table_count();
    let oracle1_reachability_ok = table_count_at_floor >= k as u32;

    // Reset the process-wide Tier-1 counters AFTER the floor (the floor's
    // ALLOCS never call contains_base) so the measured window is exactly the
    // timed rotation region.
    HeapCore::dbg_reset_contains_base_tier1_counters();

    let mut inplace_ok = true;
    let t0 = Instant::now();
    for _round in 0..ROTATING_ROUNDS {
        for slot in slots.iter_mut() {
            let p_in = *slot;
            // Same-size in-place realloc -- exactly ONE contains_base call,
            // no unregister, the object stays live for the next round.
            // SAFETY: `p_in` was returned by a prior matching `heap.alloc`/
            // `heap.realloc` call, still live (never freed), `layout`
            // exactly matches its current allocation, freed at most once
            // (never, in this loop) -- honoring `GlobalAlloc::realloc`'s
            // contract.
            let p_out = unsafe { heap.realloc(p_in, layout, layout.size()) };
            // PATH-ACTIVATION ORACLE #1 (continued): a same-size realloc
            // that took the in-place path always returns the SAME pointer
            // (see `try_realloc_inplace_known_base`'s own doc: "always
            // returns the SAME pointer on success, never moves the block").
            // A moved pointer here would mean this workload silently
            // stopped exercising the intended in-place mechanism.
            if p_out.is_null() || p_out != p_in {
                inplace_ok = false;
            }
            *slot = p_out;
        }
    }
    let churn_elapsed_ns = t0.elapsed().as_nanos();
    let oracle1_ok = oracle1_reachability_ok && inplace_ok;

    // PATH-ACTIVATION ORACLE #2: the actual claim under test. Read back the
    // REAL Tier-1 hit/miss split the rotation above drove through the real
    // production `contains_base` call site.
    let tier1_hits = HeapCore::dbg_contains_base_tier1_hits();
    let tier1_misses = HeapCore::dbg_contains_base_tier1_misses();
    let tier1_total = tier1_hits + tier1_misses;
    let expected_total = (ROTATING_ROUNDS * k * EXPECTED_CALLS_PER_REALLOC) as u64;
    let hit_rate_pct = if tier1_total > 0 {
        100.0 * tier1_hits as f64 / tier1_total as f64
    } else {
        0.0
    };

    // Teardown.
    for &p in &slots {
        // SAFETY: `p` is the current occupant of this slot, live, freed
        // exactly once here.
        unsafe { heap.dealloc(p, layout) };
    }
    // SAFETY: `heap_ptr` was returned by `claim` above, not yet recycled,
    // no other thread touches it (single-threaded probe).
    unsafe { HeapRegistry::recycle(heap_ptr) };

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);
    assert_eq!(
        conflicts_delta, 0,
        "R32-10 child (k={k}): config_conflicts_delta = {conflicts_delta} (expected 0 in a \
         fresh process; this gate has no config sweep, so any nonzero delta indicates \
         cross-arm/cross-thread registry-slot bleed, not a labelled arm)"
    );

    let ns_per_op = if ROTATING_ROUNDS * k > 0 {
        churn_elapsed_ns as f64 / (ROTATING_ROUNDS * k) as f64
    } else {
        0.0
    };

    proc_probe::emit_u64("k", k as u64);
    proc_probe::emit_u64("rotating_rounds", ROTATING_ROUNDS as u64);
    proc_probe::emit_u64("table_count_at_floor", u64::from(table_count_at_floor));
    proc_probe::emit_u64("oracle1_pass", u64::from(oracle1_ok));
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("tier1_hits", tier1_hits);
    proc_probe::emit_u64("tier1_misses", tier1_misses);
    proc_probe::emit_u64("tier1_total", tier1_total);
    proc_probe::emit_u64("expected_total", expected_total);
    proc_probe::emit_u64("oracle2_pass", u64::from(tier1_total == expected_total));
    proc_probe::emit_f64("tier1_hit_rate_pct", hit_rate_pct);
    proc_probe::emit_ns("churn_elapsed_ns", churn_elapsed_ns);
    proc_probe::emit_f64("ns_per_op", ns_per_op);

    println!(
        "OK k={k} rounds={ROTATING_ROUNDS} table_count={table_count_at_floor} \
         oracle1={} cfg_conflicts_delta={conflicts_delta} tier1_hits={tier1_hits} \
         tier1_misses={tier1_misses} hit_rate={hit_rate_pct:.2}% oracle2={} ns_per_op={ns_per_op:.1}",
        if oracle1_ok { "PASS" } else { "FAIL" },
        if tier1_total == expected_total {
            "PASS"
        } else {
            "FAIL"
        }
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
        if let Some((key, v)) = rest.split_once('=') {
            map.insert(key.to_string(), v.to_string());
        }
    }
    ChildMetrics { map }
}

fn run_one_child(k: usize) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R32_10_CHILD_K", k.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R32-10 child (k={k}) failed with status {:?}; see stderr above",
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
    println!("=== R32-10 own_cache Tier-1 thrash gate ===");
    println!("K_VALUES={K_VALUES:?} REPETITIONS={REPETITIONS} ROTATING_ROUNDS={ROTATING_ROUNDS}");
    println!();

    let mut all: Vec<(usize, usize, ChildMetrics)> = Vec::new();
    let mut oracle1_failures: Vec<(usize, usize)> = Vec::new();
    let mut oracle2_failures: Vec<(usize, usize)> = Vec::new();

    for &k in K_VALUES {
        for rep in 0..REPETITIONS {
            eprintln!("--- k={k} rep={rep}/{REPETITIONS} ---");
            let m = run_one_child(k);
            assert_eq!(
                m.get_u64("config_conflicts_delta"),
                0,
                "nonzero config_conflicts_delta (k={k})"
            );
            if m.get_u64("oracle1_pass") == 0 {
                oracle1_failures.push((k, rep));
            }
            if m.get_u64("oracle2_pass") == 0 {
                oracle2_failures.push((k, rep));
            }
            all.push((k, rep, m));
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps per K) ===");
    println!(
        "{:>6} {:>12} {:>12} {:>10} {:>12}",
        "k", "tier1_hits", "tier1_miss", "hit_rate%", "ns_per_op"
    );
    for &k in K_VALUES {
        let cell: Vec<&ChildMetrics> = all
            .iter()
            .filter(|(kk, _, _)| *kk == k)
            .map(|(_, _, m)| m)
            .collect();
        let mut hit_rates: Vec<f64> = cell
            .iter()
            .map(|m| m.get_f64("tier1_hit_rate_pct"))
            .collect();
        let mut ns: Vec<f64> = cell.iter().map(|m| m.get_f64("ns_per_op")).collect();
        let hit_rate_med = median_f64(&mut hit_rates);
        let ns_med = median_f64(&mut ns);
        let hits_last = cell.last().map(|m| m.get_u64("tier1_hits")).unwrap_or(0);
        let misses_last = cell.last().map(|m| m.get_u64("tier1_misses")).unwrap_or(0);
        println!("{k:>6} {hits_last:>12} {misses_last:>12} {hit_rate_med:>10.2} {ns_med:>12.1}");
    }

    println!();
    println!("=== CSV (one row per (k, repetition)) ===");
    let cols = [
        "k",
        "repetition",
        "process_identity",
        "rotating_rounds",
        "table_count_at_floor",
        "oracle1_pass",
        "config_conflicts_delta",
        "tier1_hits",
        "tier1_misses",
        "tier1_total",
        "expected_total",
        "oracle2_pass",
        "tier1_hit_rate_pct",
        "churn_elapsed_ns",
        "ns_per_op",
    ];
    println!("# {}", cols.join(","));
    for (k, rep, m) in &all {
        let row: Vec<String> = cols
            .iter()
            .map(|c| match *c {
                "k" => k.to_string(),
                "repetition" => rep.to_string(),
                "process_identity" => "subprocess".to_string(),
                "tier1_hit_rate_pct" => format!("{:.4}", m.get_f64("tier1_hit_rate_pct")),
                "ns_per_op" => format!("{:.3}", m.get_f64("ns_per_op")),
                other => m.get_u64(other).to_string(),
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    if oracle1_failures.is_empty() {
        println!(
            "NOTE: all {} (k, repetition) cells passed oracle #1 (dbg_table_count() >= k \
             at the floor AND every timed realloc call returned the SAME pointer -- the \
             in-place mechanism this workload exists to exercise).",
            all.len()
        );
    } else {
        println!("WARNING: cells FAILED oracle #1: {oracle1_failures:?}");
    }
    if oracle2_failures.is_empty() {
        println!(
            "NOTE: all {} (k, repetition) cells passed oracle #2 (tier1_total == \
             expected_total, i.e. EXPECTED_CALLS_PER_REALLOC=1 held exactly).",
            all.len()
        );
    } else {
        println!("WARNING: cells FAILED oracle #2: {oracle2_failures:?}");
    }
    println!(
        "NOTE: each (k, repetition) cell ran in its OWN freshly-spawned OS process \
         (subprocess-per-arm isolation), through HeapRegistry::claim -- the SAME entry point \
         SeferAlloc's #[global_allocator] impl itself calls (R31-0 entry-point rule)."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    if let Ok(k_str) = std::env::var("R32_10_CHILD_K") {
        let k: usize = k_str
            .parse()
            .unwrap_or_else(|e| panic!("R32_10_CHILD_K={k_str:?} not a valid usize ({e})"));
        run_child(k);
        return;
    }
    run_orchestrator();
}
