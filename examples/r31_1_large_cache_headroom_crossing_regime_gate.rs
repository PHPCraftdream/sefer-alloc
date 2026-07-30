//! R31-1 (task #464) — the large-cache `headroom_bytes` BENEFIT-side gate at
//! a burst size that GENUINELY exceeds 64 MiB. Direct sibling of
//! `examples/r30_6_large_cache_headroom_ab_gate.rs` (same subprocess-per-arm
//! methodology, same config-identity/path-activation oracle machinery) —
//! this file exists ONLY because R30-6's own 48 MiB/burst-labelled workload
//! (`8 * 6 MiB`) actually rounds, per `AllocCore::alloc_large`'s
//! whole-`SEGMENT` rounding (`src/alloc_core/alloc_core_large.rs:188-192`,
//! `SEGMENT = 4 MiB`, `src/alloc_core/os.rs:65`), to an **8 MiB usable span
//! per object** (6 MiB + header exceeds 1 segment, so it rounds UP to 2
//! segments = 8 MiB) — i.e. a real **64 MiB** working set, sitting EXACTLY
//! on the 64-vs-256 MiB boundary. R30-6's own committed CSV already proves
//! this arithmetic: `burst1_used_max_bytes` is `67108864` (= 64 MiB, not 48
//! MiB) in every one of its 36 rows
//! (`docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE_summary.csv`). The 64 MiB
//! and 256 MiB arms therefore never entered the range where they could
//! actually differ — this file supplies that missing measurement.
//!
//! ## Two crossing-regime burst sizes, plus a boundary control
//!
//! Per the task brief ("128-272 MiB — R29-13's own regime, where the cap
//! would actually bind"), this gate sweeps THREE object sizes (all still 8
//! objects/burst, `LARGE_CACHE_SLOTS = 8`, so the slot-count ceiling itself
//! is held constant across arms — only the byte-size axis moves):
//!
//! - **6 MiB/object** (`AT_BOUNDARY`) — R30-6's EXACT original size,
//!   included here unmodified as an in-run control: its rounded working set
//!   (64 MiB) sits exactly at the 64 MiB headroom target, so a same-process
//!   comparison of "at the boundary" vs "genuinely past it" is possible
//!   within ONE gate's own raw log, not just by re-citing a different
//!   report's numbers.
//! - **12 MiB/object** (`CROSSING_MODEST`) — rounds to 16 MiB/object
//!   (`ceil((12 MiB + header) / 4 MiB) = 4 segments`), so 8 objects =
//!   **128 MiB**, exactly DOUBLE the 64 MiB headroom target — a modest,
//!   unambiguous crossing.
//! - **34 MiB/object** (`CROSSING_R29_13`) — R29-13's OWN object size
//!   (`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md`), rounds to 36
//!   MiB/object, so 8 objects = **288 MiB** — deep in R29-13's own regime,
//!   4.5x past the 64 MiB headroom target.
//!
//! Only the `headroom_bytes` = 64 and 256 MiB arms are swept here (0/16 MiB
//! already showed a real, reproducible hit-rate cost in R30-6 at the
//! at-boundary size; this gate's whole point is resolving whether 64 vs 256
//! MiB stay tied once the burst genuinely exceeds 64 MiB, so re-sweeping
//! 0/16 MiB here would not add new information — R30-6 remains the citation
//! for those two arms).
//!
//! ## Path-activation oracle + config identity (unchanged from R30-6)
//!
//! Byte-for-byte the same two-piece oracle (`admissions_ok`,
//! `burst1_used_max > 0`; `hits_ok`, `burst2_hits_sum > 0`) and the same 4
//! R26-4 config-identity pieces (requested/resolved/conflict-delta/
//! subprocess-per-arm process identity) as R30-6 — see that file's module
//! doc for the full rationale, not repeated here.
//!
//! ## Between-arm mechanism delta (CLAUDE.md's R30-8 rule)
//!
//! Per burst size, this gate reports `burst1_used_max_bytes` (the actual
//! rounded working set the mechanism admitted) alongside the hit-rate — the
//! MECHANISM evidence that headroom=64 MiB's decay fast-path
//! (`large_cache_used_bytes <= headroom_bytes` early-return,
//! `alloc_core_large_cache.rs:320-330`) does or does not fire differently
//! than headroom=256 MiB's, not just the labelled burst size. See §ORACLE
//! in the orchestrator's printed table.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r31_1_large_cache_headroom_crossing_regime_gate --features "production alloc-stats bench-internals"
//! ```

#![cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    LargeCacheConfig,
};

// ---------------------------------------------------------------------------
// Sweep parameters
// ---------------------------------------------------------------------------

/// `headroom_bytes` arms — only the two R30-6 found tied at the at-boundary
/// size. 0/16 MiB are not re-swept here (see module doc).
const HEADROOM_ARMS: &[usize] = &[64 * 1024 * 1024, 256 * 1024 * 1024];

/// Heap-count arms — same grid as R30-6/R29-13.
const THREAD_COUNTS: &[usize] = &[1, 8, 32];

/// Repetitions per cell — same as R30-6 (CLAUDE.md "short scenario by
/// default"; 3x3x2x3 = 18-cell matrix stays fast).
const REPETITIONS: usize = 3;

/// Per-object large-allocation sizes, each labelled with what it rounds to.
/// All values verified against `alloc_large`'s whole-`SEGMENT` rounding
/// (`SEGMENT = 4 MiB`, `src/alloc_core/alloc_core_large.rs:188-192`) — see
/// the module doc for the exact arithmetic.
const BURST_ARMS: &[(&str, usize)] = &[
    ("AT_BOUNDARY_6MiB", 6 * 1024 * 1024), // rounds to 8 MiB/obj -> 64 MiB/burst (R30-6's original size)
    ("CROSSING_MODEST_12MiB", 12 * 1024 * 1024), // rounds to 16 MiB/obj -> 128 MiB/burst
    ("CROSSING_R29_13_34MiB", 34 * 1024 * 1024), // rounds to 36 MiB/obj -> 288 MiB/burst (R29-13's own size)
];

/// Distinct large objects per burst — one per base large-cache slot
/// (`LARGE_CACHE_SLOTS = 8`), held constant across all three burst-size arms
/// so only the byte-size axis moves.
const LARGE_OBJ_COUNT: usize = 8;

/// Small-object churn size (matches R30-6 / `benches/global_alloc.rs`).
const SMALL_SIZE: usize = 1024;
const SMALL_WORKING_SET: usize = 64;
const SMALL_OPS: usize = 128;

/// Idle interval between BURST1 and BURST2 — same as R30-6 (> the 1000 ms
/// default decay interval, so BURST2's first dealloc can fire a real,
/// non-forced decay tick).
const IDLE_MS: u64 = 1200;
const DECAY_INTERVAL_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Config builder
// ---------------------------------------------------------------------------

fn config_for(headroom_bytes: usize) -> LargeCacheConfig {
    LargeCacheConfig::new().headroom_bytes(headroom_bytes)
}

// ---------------------------------------------------------------------------
// Workload primitives (byte-for-byte the same shapes as R30-6)
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

fn small_churn_burst(heap: &mut HeapCore, layout: Layout) {
    let mut live: Vec<*mut u8> = (0..SMALL_WORKING_SET).map(|_| heap.alloc(layout)).collect();
    let mut rng = XorShift64::new(0xCAFE);
    for _ in 0..SMALL_OPS {
        let idx = rng.next_usize() % SMALL_WORKING_SET;
        let old = live[idx];
        if !old.is_null() {
            // SAFETY: `old` was allocated by `heap` with `layout`, freed once here.
            unsafe { heap.dealloc(old, layout) };
        }
        live[idx] = heap.alloc(layout);
    }
    for p in live {
        if !p.is_null() {
            // SAFETY: `p` still live, allocated by `heap` with `layout`.
            unsafe { heap.dealloc(p, layout) };
        }
    }
}

/// One LARGE burst: allocate `LARGE_OBJ_COUNT` distinct large objects
/// (touched, genuinely committed), then free every one of them. Returns the
/// number of `alloc_large` calls served from the cache (a cache HIT) during
/// this burst's allocation half.
fn large_burst(heap: &mut HeapCore, layout: Layout) -> u64 {
    let hits_before = heap.dbg_large_cache_hits();
    let mut live = Vec::with_capacity(LARGE_OBJ_COUNT);
    for _ in 0..LARGE_OBJ_COUNT {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "large alloc failed (OOM?)");
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
    let hits_after_alloc = heap.dbg_large_cache_hits();
    for p in &live {
        // SAFETY: `p` was allocated by `heap` with `layout` above, not yet freed.
        unsafe { heap.dealloc(*p, layout) };
    }
    hits_after_alloc.saturating_sub(hits_before)
}

// ---------------------------------------------------------------------------
// Child/arm mode
// ---------------------------------------------------------------------------

fn parse_env_usize(name: &str) -> usize {
    let raw = std::env::var(name)
        .unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"));
    raw.parse::<usize>()
        .unwrap_or_else(|e| panic!("{name}={raw:?} not a valid usize ({e})"))
}

fn parse_env_str(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"))
}

fn snapshot_kib() -> (u64, u64) {
    let m = proc_probe::snapshot();
    (m.rss / 1024, m.commit / 1024)
}

fn run_child() {
    let headroom_bytes = parse_env_usize("R31_1_HEADROOM_BYTES");
    let thread_count = parse_env_usize("R31_1_THREAD_COUNT");
    let repetition = parse_env_usize("R31_1_REPETITION");
    let burst_label = parse_env_str("R31_1_BURST_LABEL");
    let large_obj_bytes = parse_env_usize("R31_1_LARGE_OBJ_BYTES");
    let headroom_mib = (headroom_bytes / (1024 * 1024)) as u64;

    let conflicts_before = config_conflicts_total();

    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();
    let large_layout = Layout::from_size_align(large_obj_bytes, 8).unwrap();

    let resolved_headroom: Arc<Vec<AtomicU64>> = Arc::new(
        (0..thread_count)
            .map(|_| AtomicU64::new(u64::MAX))
            .collect(),
    );
    let burst1_used: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let burst2_hits: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let burst2_used: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let burst2_elapsed_ns: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let small_elapsed_ns: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());

    let go = Arc::new(AtomicBool::new(false));
    let burst1_done = Arc::new(AtomicU64::new(0));
    let idle_done = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicU64::new(0));

    let (rss_before_kib, _c) = snapshot_kib();

    let mut handles = Vec::with_capacity(thread_count);
    for i in 0..thread_count {
        let (
            go,
            burst1_done,
            idle_done,
            finished,
            resolved_headroom,
            burst1_used,
            burst2_hits,
            burst2_used,
            burst2_elapsed_ns,
            small_elapsed_ns,
        ) = (
            Arc::clone(&go),
            Arc::clone(&burst1_done),
            Arc::clone(&idle_done),
            Arc::clone(&finished),
            Arc::clone(&resolved_headroom),
            Arc::clone(&burst1_used),
            Arc::clone(&burst2_hits),
            Arc::clone(&burst2_used),
            Arc::clone(&burst2_elapsed_ns),
            Arc::clone(&small_elapsed_ns),
        );
        let burst_label = burst_label.clone();
        handles.push(thread::spawn(move || {
            let heap_ptr = HeapRegistry::claim_with_config(config_for(headroom_bytes));
            assert!(
                !heap_ptr.is_null(),
                "HeapRegistry::claim_with_config returned null at thread {i}"
            );
            // SAFETY: `heap_ptr` was just returned by `claim_with_config` and
            // is owned by THIS thread until `recycle` at the end.
            let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

            let (_, _, resolved) = heap.dbg_decay_config();
            assert_eq!(
                resolved, headroom_bytes,
                "R31-1 child (headroom={headroom_bytes}, thread={i}, rep={repetition}, \
                 burst={burst_label}): resolved headroom ({resolved}) != requested ({headroom_bytes})"
            );
            resolved_headroom[i].store(resolved as u64, Ordering::Release);

            while !go.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }

            let t_small = Instant::now();
            small_churn_burst(heap, small_layout);
            small_elapsed_ns[i].store(t_small.elapsed().as_nanos() as u64, Ordering::Release);

            let _burst1_hits = large_burst(heap, large_layout);
            burst1_used[i].store(heap.dbg_large_cache_used() as u64, Ordering::Release);
            burst1_done.fetch_add(1, Ordering::Release);

            while !idle_done.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }

            let t_burst2 = Instant::now();
            let hits2 = large_burst(heap, large_layout);
            burst2_elapsed_ns[i].store(t_burst2.elapsed().as_nanos() as u64, Ordering::Release);
            burst2_hits[i].store(hits2, Ordering::Release);
            burst2_used[i].store(heap.dbg_large_cache_used() as u64, Ordering::Release);

            finished.fetch_add(1, Ordering::Release);

            // SAFETY: `heap_ptr` was returned by `claim_with_config` above,
            // not yet recycled, and no other thread touches it.
            unsafe { HeapRegistry::recycle(heap_ptr) };
        }));
    }

    go.store(true, Ordering::Release);
    let mut peak_rss_kib: u64 = 0;
    while burst1_done.load(Ordering::Acquire) < thread_count as u64 {
        let (r, _c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        thread::sleep(Duration::from_millis(10));
    }
    let (rss_burst1_kib, commit_burst1_kib) = snapshot_kib();
    peak_rss_kib = peak_rss_kib.max(rss_burst1_kib);

    thread::sleep(Duration::from_millis(IDLE_MS));
    let (rss_idle_kib, _c) = snapshot_kib();

    idle_done.store(true, Ordering::Release);
    while finished.load(Ordering::Acquire) < thread_count as u64 {
        let (r, _c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        thread::sleep(Duration::from_millis(10));
    }
    let (rss_burst2_kib, commit_burst2_kib) = snapshot_kib();
    peak_rss_kib = peak_rss_kib.max(rss_burst2_kib);

    for h in handles {
        h.join()
            .unwrap_or_else(|e| panic!("worker panicked: {e:?}"));
    }

    let sum =
        |v: &Arc<Vec<AtomicU64>>| -> u64 { v.iter().map(|a| a.load(Ordering::Acquire)).sum() };
    let max = |v: &Arc<Vec<AtomicU64>>| -> u64 {
        v.iter()
            .map(|a| a.load(Ordering::Acquire))
            .max()
            .unwrap_or(0)
    };

    let burst1_used_max = max(&burst1_used);
    let burst2_used_max = max(&burst2_used);
    let burst2_hits_sum = sum(&burst2_hits);
    let burst2_elapsed_ns_sum = sum(&burst2_elapsed_ns);
    let small_elapsed_ns_sum = sum(&small_elapsed_ns);

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);

    for (i, slot) in resolved_headroom.iter().enumerate() {
        let v = slot.load(Ordering::Acquire);
        assert_eq!(
            v, headroom_bytes as u64,
            "R31-1 child: worker {i} stashed resolved headroom {v}, expected {headroom_bytes}"
        );
    }
    assert_eq!(
        conflicts_delta, 0,
        "R31-1 child (headroom={headroom_bytes}, threads={thread_count}, rep={repetition}, \
         burst={burst_label}): CONFIG_CONFLICTS delta = {conflicts_delta} (expected 0 in a fresh process)"
    );

    // SANITY ASSERTION (R31-12/task #476 P2-4 repair, applied here from the
    // start so this NEW harness cannot ship the same silently-unflagged
    // physically-impossible-RSS-collapse class of row R30-6's raw log
    // carried: a process cannot lose more RSS across a PURE IDLE window
    // than it held immediately before that window started. `rss_idle_kib`
    // is sampled strictly after `rss_burst1_kib` with zero deallocation
    // activity in between (every worker is parked in its idle-wait loop) —
    // so `rss_idle_kib` dropping by more than a generous OS-noise budget
    // relative to `rss_burst1_kib` is not a real allocator behavior, it is
    // a broken sample (e.g. a `proc_probe` snapshot race) and must fail
    // loudly rather than silently entering a future summary table.
    let idle_drop = rss_burst1_kib.saturating_sub(rss_idle_kib);
    assert!(
        idle_drop <= rss_burst1_kib / 10 + 4096,
        "R31-1 child (headroom={headroom_bytes}, threads={thread_count}, rep={repetition}, \
         burst={burst_label}): physically-impossible RSS collapse across pure idle window: \
         rss_burst1_kib={rss_burst1_kib} -> rss_idle_kib={rss_idle_kib} (drop={idle_drop} KiB, \
         budget={}) — no deallocation activity occurs between these two samples, so a drop this \
         large indicates a broken proc_probe sample, not real allocator behavior",
        rss_burst1_kib / 10 + 4096
    );

    let admissions_ok = burst1_used_max > 0;
    let hits_ok = burst2_hits_sum > 0;
    let oracle_pass = admissions_ok && hits_ok;

    proc_probe::emit("burst_label", &burst_label);
    proc_probe::emit_u64("large_obj_bytes", large_obj_bytes as u64);
    proc_probe::emit_u64("headroom_bytes", headroom_bytes as u64);
    proc_probe::emit_u64("headroom_mib", headroom_mib);
    proc_probe::emit_u64("thread_count", thread_count as u64);
    proc_probe::emit_u64("repetition", repetition as u64);
    proc_probe::emit_u64("verified_headroom", headroom_bytes as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("large_obj_count", LARGE_OBJ_COUNT as u64);
    proc_probe::emit_u64("rss_before_kib", rss_before_kib);
    proc_probe::emit_u64("peak_rss_kib", peak_rss_kib);
    proc_probe::emit_u64("rss_burst1_kib", rss_burst1_kib);
    proc_probe::emit_u64("commit_burst1_kib", commit_burst1_kib);
    proc_probe::emit_u64("burst1_used_max", burst1_used_max);
    proc_probe::emit_u64("rss_idle_kib", rss_idle_kib);
    proc_probe::emit_u64("idle_ms", IDLE_MS);
    proc_probe::emit_u64("rss_burst2_kib", rss_burst2_kib);
    proc_probe::emit_u64("commit_burst2_kib", commit_burst2_kib);
    proc_probe::emit_u64("burst2_used_max", burst2_used_max);
    proc_probe::emit_u64("burst2_hits_sum", burst2_hits_sum);
    proc_probe::emit_u64(
        "burst2_possible_sum",
        (LARGE_OBJ_COUNT * thread_count) as u64,
    );
    proc_probe::emit_u64("burst2_elapsed_ns_sum", burst2_elapsed_ns_sum);
    proc_probe::emit_u64("small_elapsed_ns_sum", small_elapsed_ns_sum);
    proc_probe::emit_u64("decay_interval_ms", DECAY_INTERVAL_MS);
    proc_probe::emit_u64("admissions_ok", u64::from(admissions_ok));
    proc_probe::emit_u64("hits_ok", u64::from(hits_ok));
    proc_probe::emit_u64("oracle_pass", u64::from(oracle_pass));

    println!(
        "OK burst={burst_label} obj_bytes={large_obj_bytes} headroom={headroom_bytes} MiB={headroom_mib} \
         threads={thread_count} rep={repetition} verified_headroom={headroom_bytes} \
         cfg_conflicts_delta={conflicts_delta} burst1_used_max={burst1_used_max} \
         burst2_hits_sum={burst2_hits_sum}/{} burst2_elapsed_ns_sum={burst2_elapsed_ns_sum} \
         rss_burst2_kib={rss_burst2_kib} oracle={}",
        LARGE_OBJ_COUNT * thread_count,
        if oracle_pass { "PASS" } else { "FAIL" }
    );
}

// ---------------------------------------------------------------------------
// Orchestrator mode
// ---------------------------------------------------------------------------

struct ChildMetrics {
    num: std::collections::HashMap<String, u64>,
    burst_label: String,
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
    let mut burst_label = String::new();
    for line in stdout.lines() {
        let rest = match line.strip_prefix("RESULT ") {
            Some(r) => r,
            None => continue,
        };
        for tok in rest.split_whitespace() {
            if let Some((k, v)) = tok.split_once('=') {
                if k == "burst_label" {
                    burst_label = v.to_string();
                } else if let Ok(n) = v.parse::<u64>() {
                    num.insert(k.to_string(), n);
                }
            }
        }
    }
    ChildMetrics { num, burst_label }
}

#[allow(clippy::too_many_arguments)]
fn run_one_child(
    burst_label: &str,
    large_obj_bytes: usize,
    headroom_bytes: usize,
    thread_count: usize,
    repetition: usize,
) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R31_1_BURST_LABEL", burst_label)
        .env("R31_1_LARGE_OBJ_BYTES", large_obj_bytes.to_string())
        .env("R31_1_HEADROOM_BYTES", headroom_bytes.to_string())
        .env("R31_1_THREAD_COUNT", thread_count.to_string())
        .env("R31_1_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R31-1 child (burst={burst_label}, headroom={headroom_bytes}, threads={thread_count}, \
             rep={repetition}) failed with status {:?}; see stderr above",
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
        "=== R31-1 large-cache HEADROOM crossing-regime gate — subprocess-per-arm isolation ==="
    );
    println!(
        "grid: burst_arms={:?} x headroom_bytes={:?} x threads={:?} x {REPETITIONS} reps; \
         LARGE_OBJ_COUNT={LARGE_OBJ_COUNT} SMALL_SIZE={SMALL_SIZE} SMALL_WORKING_SET={SMALL_WORKING_SET} \
         SMALL_OPS={SMALL_OPS} IDLE_MS={IDLE_MS} decay_interval_ms={DECAY_INTERVAL_MS}",
        BURST_ARMS.iter().map(|(l, _)| *l).collect::<Vec<_>>(),
        HEADROOM_ARMS,
        THREAD_COUNTS
    );
    println!();

    let mut all: Vec<ChildMetrics> = Vec::new();
    let mut oracle_failures: Vec<(&str, usize, usize, usize)> = Vec::new();
    for &(burst_label, obj_bytes) in BURST_ARMS {
        for &hb in HEADROOM_ARMS {
            for &tc in THREAD_COUNTS {
                for rep in 0..REPETITIONS {
                    eprintln!(
                        "--- arm burst={burst_label} obj_bytes={obj_bytes} headroom={hb} MiB={} \
                         threads={tc} rep={rep}/{REPETITIONS} ---",
                        hb / (1024 * 1024)
                    );
                    let m = run_one_child(burst_label, obj_bytes, hb, tc, rep);
                    assert_eq!(m.burst_label, burst_label, "wrong burst_label");
                    assert_eq!(
                        m.get("large_obj_bytes"),
                        obj_bytes as u64,
                        "wrong obj bytes"
                    );
                    assert_eq!(m.get("headroom_bytes"), hb as u64, "wrong headroom_bytes");
                    assert_eq!(m.get("thread_count"), tc as u64, "wrong thread_count");
                    assert_eq!(m.get("repetition"), rep as u64, "wrong repetition");
                    assert_eq!(
                        m.get("verified_headroom"),
                        hb as u64,
                        "verified_headroom != requested"
                    );
                    assert_eq!(
                        m.get("config_conflicts_delta"),
                        0,
                        "non-zero config_conflicts_delta"
                    );
                    if m.get("oracle_pass") == 0 {
                        oracle_failures.push((burst_label, hb, tc, rep));
                    }
                    all.push(m);
                }
            }
        }
    }

    println!();
    println!(
        "=== aggregated (median of {REPETITIONS} reps; per (burst, headroom, threads) cell) ==="
    );
    println!(
        "{:>24} {:>10} {:>8} {:>14} {:>16} {:>14} {:>12} {:>10}",
        "burst",
        "headroomMB",
        "threads",
        "burst1_used",
        "burst2_hits/poss",
        "hit_rate_pct",
        "rss_b2_kib",
        "rss_idle",
    );
    for &(burst_label, _) in BURST_ARMS {
        for &hb in HEADROOM_ARMS {
            for &tc in THREAD_COUNTS {
                let cell: Vec<&ChildMetrics> = all
                    .iter()
                    .filter(|m| {
                        m.burst_label == burst_label
                            && m.get("headroom_bytes") == hb as u64
                            && m.get("thread_count") == tc as u64
                    })
                    .collect();
                let pick = |k: &str| -> Vec<u64> { cell.iter().map(|m| m.get(k)).collect() };
                let mut b1 = pick("burst1_used_max");
                let mut hits = pick("burst2_hits_sum");
                let poss = cell[0].get("burst2_possible_sum");
                let mut rss2 = pick("rss_burst2_kib");
                let mut rssidle = pick("rss_idle_kib");
                let (b1_m, hits_m, rss2_m, rssidle_m) = (
                    median(&mut b1),
                    median(&mut hits),
                    median(&mut rss2),
                    median(&mut rssidle),
                );
                let hit_rate_pct = if poss > 0 {
                    100.0 * hits_m as f64 / poss as f64
                } else {
                    0.0
                };
                let fails = cell.iter().filter(|m| m.get("oracle_pass") == 0).count();
                let oracle_tag = if fails > 0 {
                    format!(" oracle_fail={fails}/{REPETITIONS}")
                } else {
                    String::new()
                };
                println!(
                    "{:>24} {:>10} {:>8} {:>14} {:>16} {:>13.1}% {:>12} {:>10}{}",
                    burst_label,
                    hb / (1024 * 1024),
                    tc,
                    b1_m,
                    format!("{hits_m}/{poss}"),
                    hit_rate_pct,
                    rss2_m,
                    rssidle_m,
                    oracle_tag,
                );
            }
        }
    }

    println!();
    println!("=== between-arm mechanism delta (CLAUDE.md R30-8 rule): burst1_used_max at 64 vs 256 MiB, per burst size ===");
    for &(burst_label, _) in BURST_ARMS {
        for &tc in THREAD_COUNTS {
            let get_used = |hb: usize| -> u64 {
                let mut v: Vec<u64> = all
                    .iter()
                    .filter(|m| {
                        m.burst_label == burst_label
                            && m.get("headroom_bytes") == hb as u64
                            && m.get("thread_count") == tc as u64
                    })
                    .map(|m| m.get("burst1_used_max"))
                    .collect();
                median(&mut v)
            };
            let used64 = get_used(64 * 1024 * 1024);
            let used256 = get_used(256 * 1024 * 1024);
            let delta = i64::from(used256 != used64);
            println!(
                "burst={burst_label} threads={tc}: burst1_used_max(64MiB)={used64} \
                 burst1_used_max(256MiB)={used256} mechanism_delta={}",
                if delta == 0 {
                    "ZERO (identical admission)"
                } else {
                    "NONZERO (admission differs)"
                }
            );
        }
    }

    println!();
    println!("=== CSV (one row per child) ===");
    let cols = [
        "burst_label",
        "large_obj_bytes",
        "headroom_bytes",
        "headroom_mib",
        "thread_count",
        "repetition",
        "verified_headroom",
        "config_conflicts_delta",
        "process_identity",
        "large_obj_count",
        "peak_rss_kib",
        "rss_burst1_kib",
        "commit_burst1_kib",
        "burst1_used_max",
        "rss_idle_kib",
        "idle_ms",
        "rss_burst2_kib",
        "commit_burst2_kib",
        "burst2_used_max",
        "burst2_hits_sum",
        "burst2_possible_sum",
        "burst2_elapsed_ns_sum",
        "small_elapsed_ns_sum",
        "decay_interval_ms",
        "admissions_ok",
        "hits_ok",
        "oracle_pass",
    ];
    println!("# {}", cols.join(","));
    for m in &all {
        let row: Vec<String> = cols
            .iter()
            .map(|c| {
                if *c == "process_identity" {
                    "subprocess".to_string()
                } else if *c == "burst_label" {
                    m.burst_label.clone()
                } else {
                    m.get(c).to_string()
                }
            })
            .collect();
        println!("{}", row.join(","));
    }

    println!();
    if oracle_failures.is_empty() {
        println!(
            "NOTE: all {} arms passed the path-activation oracle (admissions_ok AND hits_ok).",
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
        "NOTE: each (burst_label, headroom_bytes, thread_count, rep) cell ran in its OWN \
         freshly-spawned OS process (subprocess-per-arm isolation, registry-bypass via \
         HeapRegistry::claim_with_config). Every arm hard-asserted resolved headroom == requested \
         headroom AND config_conflicts_delta == 0 (R26-4 config identity, all 4 pieces). Every \
         arm additionally hard-asserted a sanity bound on rss_idle_kib vs rss_burst1_kib (R31-12 \
         repair, applied here from the start) to reject a physically-impossible RSS collapse \
         sample before it could enter this table."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R31_1_HEADROOM_BYTES").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
