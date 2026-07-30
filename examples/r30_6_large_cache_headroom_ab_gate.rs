//! R30-6 (task #455) — the large-cache `headroom_bytes` BENEFIT-side gate:
//! hit-rate / RSS / burst-idle-burst A/B across the headroom sweep {0, 16,
//! 64, 256 MiB}, the missing counterpart to R29-13's RETENTION-cost gate
//! (`docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md`). Mirrors R29-13's own
//! subprocess-per-arm + config-self-verification methodology (itself R27-3's
//! methodology, generalized to the large cache) — see this file's sibling
//! `examples/r29_13_large_cache_retention_gate.rs` for the shared shape.
//!
//! ## What this axis measures that R29-13 explicitly did NOT
//!
//! R29-13 §7 ("What this gate does NOT claim"): *"No throughput/hit-rate
//! trade-off measurement... A follow-on 'does a smaller headroom hurt
//! large-object churn latency' A/B... was NOT run here."* `docs/perf/OPEN_ITEMS.md`
//! item 27 names the exact missing piece: *"a throughput/hit-rate A/B at a
//! smaller headroom through the real `#[global_allocator]` (the large-cache
//! analogue of R27-4)."* This harness supplies the hit-rate/RSS/burst side
//! of that (registry-bypass, matching R27-3's own retention-probe shape);
//! the REAL-`#[global_allocator]` latency companion lives in four small
//! sibling binaries (`r30_6_latency_h0`/`_h16`/`_h64`/`_h256`, mirroring
//! R27-4's `cap4`/`cap8` real-allocator shape) because `SeferAlloc::with_config`
//! bakes its `LargeCacheConfig` into a `static` at compile time — a runtime
//! headroom sweep through the real global allocator needs one binary per
//! headroom value, exactly as R27-4 needed one binary per pool-cap value.
//!
//! ## Workload — MIXED small + large (the key methodology extension vs R29-13)
//!
//! R29-13's workload was LARGE-only (8x34 MiB, nothing else) — sufficient to
//! prove the RETENTION floor, but silent on whether a smaller headroom's
//! reduced cache residency actually costs anything in a workload that also
//! does ordinary small-object churn interleaved with large-object traffic
//! (the realistic shape this project's classes are meant to interact
//! under — see `benches/global_alloc.rs`'s churn primitives, reused here for
//! the small-object half). Each thread interleaves: a SMALL churn step
//! (1024 B, `benches/global_alloc.rs`'s own `churn_prefill`/`churn_step`/
//! `churn_teardown` shape, byte-for-byte copied) with a LARGE fill/free step
//! (8 distinct 6 MiB objects per burst — smaller than R29-13's 34 MiB so a
//! burst/idle/burst SEQUENCE completes quickly at all three thread counts,
//! while still comfortably `> SMALL_MAX` so classification as `Large` is
//! unambiguous, see `LARGE_OBJ_BYTES`'s doc).
//!
//! ## Burst -> idle -> burst (the shape event-driven-only decay is visible in)
//!
//! Per R29-13 §3: decay never fires mid-tight-loop (the first dealloc in a
//! burst only primes the timer). This harness's per-thread sequence is
//! BURST1 (fill+free 8 large objects) -> IDLE (fixed wall-clock sleep, >
//! the 1000 ms decay interval) -> BURST2 (fill+free 8 MORE large objects,
//! this time crossing the primed timer so a REAL decay tick CAN fire) ->
//! measure hit rate on BURST2 specifically (BURST2's hits prove whether a
//! smaller headroom's reduced residency actually cost cache hits the
//! WORKLOAD would have gotten at 256 MiB).
//!
//! ## Path-activation oracle (R30-3's established pattern, this file's shape)
//!
//! Per arm: hard-assert `admissions > 0` (`used_post_teardown_max > 0`,
//! R29-13's own oracle) AND `hits > 0` (a NEW oracle this gate adds — R29-13
//! never needed it because it never re-allocated a same-sized object after
//! freeing one; this gate's BURST2 does, so a genuine cache hit is only
//! actually observable once BURST2 reuses one of BURST1's freed slot sizes).
//! An arm that fails either is printed with `oracle=FAIL` and excluded from
//! the report's headline table (mirrors R30-3's `oracle=FAIL` convention
//! exactly).
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r30_6_large_cache_headroom_ab_gate --features "production alloc-stats bench-internals"
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

/// `headroom_bytes` arms — same grid as R29-13, the task's own required
/// matrix.
const HEADROOM_ARMS: &[usize] = &[0, 16 * 1024 * 1024, 64 * 1024 * 1024, 256 * 1024 * 1024];

/// Heap-count arms — the task's own required matrix (1, 8, 32).
const THREAD_COUNTS: &[usize] = &[1, 8, 32];

/// Repetitions per cell. Kept at 3 (R27-3/R29-13's precedent) for a
/// median+range without turning the full 4x3x3=36-cell matrix into a
/// multi-hour sweep (CLAUDE.md "Speed: short scenario by default" — see the
/// report's explicit timing note for the measured wall-clock cost of this
/// choice).
const REPETITIONS: usize = 3;

/// One large object's size for the mixed workload's LARGE half. Deliberately
/// SMALLER than R29-13's 34 MiB (chosen there to force retention past the
/// 256 MiB headroom in ONE burst) — this gate's large objects only need to
/// exceed `SMALL_MAX` (~253 KiB under plain `production`) unambiguously and
/// leave room for TWO bursts (16 objects total) to fit under the smallest
/// non-zero headroom arm (16 MiB) so a hit is even POSSIBLE at that arm:
/// `8 * 6 MiB = 48 MiB` per burst comfortably exceeds 16 MiB (so headroom=16
/// MiB still exercises real eviction pressure) while `LARGE_CACHE_SLOTS = 8`
/// means at most 8 distinct sizes are resident at once regardless of
/// headroom (slot-count-limited, not byte-limited, at this size).
const LARGE_OBJ_BYTES: usize = 6 * 1024 * 1024;

/// Distinct large objects per burst — one per base large-cache slot
/// (`LARGE_CACHE_SLOTS = 8`, matching R29-13).
const LARGE_OBJ_COUNT: usize = 8;

/// Small-object churn size (matches `benches/global_alloc.rs`'s own default
/// SIZES entry and R27-3/R27-4's small-pool gates, for cross-report
/// consistency of "what a small object means in this project's gates").
const SMALL_SIZE: usize = 1024;

/// Small-object churn working set / op count per burst-adjacent churn step —
/// deliberately small (this gate's point is the LARGE-cache axis; the small
/// churn only needs to prove the mixed workload's small half is genuinely
/// exercised alongside the large half, not to re-measure the small pool,
/// which R27-3/R27-4 already cover exhaustively).
const SMALL_WORKING_SET: usize = 64;
const SMALL_OPS: usize = 128;

/// Idle interval between BURST1 and BURST2 — deliberately ABOVE the 1000 ms
/// default decay interval (`DECAY_INTERVAL_MS`) so BURST2's first large
/// dealloc, which primed the timer at the end of BURST1... actually see
/// module doc: BURST1's last dealloc primes the timer (R29-13 §3); an idle
/// interval > the decay interval means BURST2's FIRST dealloc, whenever it
/// runs, finds `elapsed >= decay_interval` and CAN fire a real decay tick —
/// this is the one wall-clock design choice that makes a real (non-forced)
/// decay tick observable in this gate's natural workload, unlike R29-13's
/// tight-loop-only shape.
const IDLE_MS: u64 = 1200;

/// The configured decay interval (ms), read from source — left at default
/// (matches R29-13; this probe does not override `decay_interval_ms`, so the
/// measured behavior matches what a real `LargeCacheConfig::new().headroom_bytes(n)`
/// caller actually gets).
const DECAY_INTERVAL_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Config builder
// ---------------------------------------------------------------------------

fn config_for(headroom_bytes: usize) -> LargeCacheConfig {
    LargeCacheConfig::new().headroom_bytes(headroom_bytes)
}

// ---------------------------------------------------------------------------
// Workload primitives
// ---------------------------------------------------------------------------

/// XorShift64, seed `0xCAFE` — verbatim from `benches/global_alloc.rs`.
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

/// Small-object churn: prefill `working_set` blocks, `ops` random
/// replace-cycles, free everything. Byte-for-byte the same SHAPE as
/// `benches/global_alloc.rs`'s `churn_prefill`/`churn_step`/`churn_teardown`,
/// routed through `HeapCore` (registry-bypass, matching R29-13's own large
/// workload's routing) instead of `std::alloc`.
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
/// (touched, genuinely committed), then free every one of them (returning
/// each span to the large cache subject to the headroom/decay policy).
/// Returns the number of `alloc_large` calls served from the cache (a cache
/// HIT) during this burst's allocation half, read via
/// `AllocCore::dbg_large_cache_hits()` before/after.
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

fn snapshot_kib() -> (u64, u64) {
    let m = proc_probe::snapshot();
    (m.rss / 1024, m.commit / 1024)
}

fn run_child() {
    let headroom_bytes = parse_env_usize("R30_6_HEADROOM_BYTES");
    let thread_count = parse_env_usize("R30_6_THREAD_COUNT");
    let repetition = parse_env_usize("R30_6_REPETITION");
    let headroom_mib = (headroom_bytes / (1024 * 1024)) as u64;

    let conflicts_before = config_conflicts_total();

    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();
    let large_layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();

    // Per-thread result slots.
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
        handles.push(thread::spawn(move || {
            let heap_ptr = HeapRegistry::claim_with_config(config_for(headroom_bytes));
            assert!(
                !heap_ptr.is_null(),
                "HeapRegistry::claim_with_config returned null at thread {i}"
            );
            // SAFETY: `heap_ptr` was just returned by `claim_with_config` and
            // is owned by THIS thread until `recycle` at the end.
            let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

            // SELF-VERIFICATION (R26-4 config-sweep evidence rule, piece 2):
            // resolved headroom read back from the diagnostic surface, not
            // assumed.
            let (_, _, resolved) = heap.dbg_decay_config();
            assert_eq!(
                resolved, headroom_bytes,
                "R30-6 child (headroom={headroom_bytes}, thread={i}, rep={repetition}): \
                 resolved headroom ({resolved}) != requested ({headroom_bytes})"
            );
            resolved_headroom[i].store(resolved as u64, Ordering::Release);

            while !go.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }

            // Mixed workload, phase A: small churn (proves the small half is
            // genuinely exercised alongside the large half).
            let t_small = Instant::now();
            small_churn_burst(heap, small_layout);
            small_elapsed_ns[i].store(t_small.elapsed().as_nanos() as u64, Ordering::Release);

            // BURST1: fill+free 8 large objects (primes the decay timer per
            // R29-13 §3 — the FIRST dealloc of the process/heap's life always
            // just primes without decaying).
            let _burst1_hits = large_burst(heap, large_layout);
            burst1_used[i].store(heap.dbg_large_cache_used() as u64, Ordering::Release);
            burst1_done.fetch_add(1, Ordering::Release);

            // IDLE: wait for the coordinator's signal that the idle interval
            // (> decay_interval_ms) has elapsed — pure idle, no allocation
            // activity on this or any other thread during the wait.
            while !idle_done.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }

            // BURST2: fill+free 8 MORE large objects of the SAME size — this
            // time the timer primed by BURST1 has had > decay_interval_ms to
            // elapse, so a REAL (non-forced) decay tick CAN fire on the
            // first dealloc, exercising the natural (not
            // dbg_force_decay_tick-forced) decay path this gate's burst-idle-
            // burst design specifically targets. Hits here are the load-
            // bearing hit-rate number: whether the (possibly-just-decayed)
            // cache still had a same-size slot resident from BURST1.
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

    // Release threads; wait for every thread to finish BURST1.
    go.store(true, Ordering::Release);
    let mut peak_rss_kib: u64 = 0;
    while burst1_done.load(Ordering::Acquire) < thread_count as u64 {
        let (r, _c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        thread::sleep(Duration::from_millis(10));
    }
    let (rss_burst1_kib, commit_burst1_kib) = snapshot_kib();
    peak_rss_kib = peak_rss_kib.max(rss_burst1_kib);

    // IDLE window: sleep for IDLE_MS with zero allocation activity anywhere
    // in this process (every worker thread is parked in its idle-wait loop).
    thread::sleep(Duration::from_millis(IDLE_MS));
    let (rss_idle_kib, _c) = snapshot_kib();

    // Release BURST2.
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

    // Aggregate per-thread results.
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

    // SELF-VERIFICATION re-check.
    for (i, slot) in resolved_headroom.iter().enumerate() {
        let v = slot.load(Ordering::Acquire);
        assert_eq!(
            v, headroom_bytes as u64,
            "R30-6 child: worker {i} stashed resolved headroom {v}, expected {headroom_bytes}"
        );
    }
    assert_eq!(
        conflicts_delta, 0,
        "R30-6 child (headroom={headroom_bytes}, threads={thread_count}, rep={repetition}): \
         CONFIG_CONFLICTS delta = {conflicts_delta} (expected 0 in a fresh process)"
    );

    // PATH-ACTIVATION ORACLE (R30-3 pattern): admissions > 0 AND hits > 0.
    let admissions_ok = burst1_used_max > 0;
    let hits_ok = burst2_hits_sum > 0;
    let oracle_pass = admissions_ok && hits_ok;

    proc_probe::emit_u64("headroom_bytes", headroom_bytes as u64);
    proc_probe::emit_u64("headroom_mib", headroom_mib);
    proc_probe::emit_u64("thread_count", thread_count as u64);
    proc_probe::emit_u64("repetition", repetition as u64);
    proc_probe::emit_u64("verified_headroom", headroom_bytes as u64);
    proc_probe::emit_u64("config_conflicts_delta", conflicts_delta);
    proc_probe::emit_u64("large_obj_bytes", LARGE_OBJ_BYTES as u64);
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
        "OK headroom={headroom_bytes} MiB={headroom_mib} threads={thread_count} rep={repetition} \
         verified_headroom={headroom_bytes} cfg_conflicts_delta={conflicts_delta} \
         burst1_used_max={burst1_used_max} burst2_hits_sum={burst2_hits_sum}/{} \
         burst2_elapsed_ns_sum={burst2_elapsed_ns_sum} rss_burst2_kib={rss_burst2_kib} \
         oracle={}",
        LARGE_OBJ_COUNT * thread_count,
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

fn run_one_child(headroom_bytes: usize, thread_count: usize, repetition: usize) -> ChildMetrics {
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let mut cmd = std::process::Command::new(&exe);
    cmd.env("R30_6_HEADROOM_BYTES", headroom_bytes.to_string())
        .env("R30_6_THREAD_COUNT", thread_count.to_string())
        .env("R30_6_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R30-6 child (headroom={headroom_bytes}, threads={thread_count}, rep={repetition}) \
             failed with status {:?}; see stderr above",
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
    println!("=== R30-6 large-cache HEADROOM A/B gate — subprocess-per-arm isolation ===");
    println!(
        "grid: headroom_bytes={:?} x threads={:?} x {REPETITIONS} reps; LARGE_OBJ_BYTES={LARGE_OBJ_BYTES} \
         LARGE_OBJ_COUNT={LARGE_OBJ_COUNT} SMALL_SIZE={SMALL_SIZE} SMALL_WORKING_SET={SMALL_WORKING_SET} \
         SMALL_OPS={SMALL_OPS} IDLE_MS={IDLE_MS} decay_interval_ms={DECAY_INTERVAL_MS}",
        HEADROOM_ARMS, THREAD_COUNTS
    );
    println!();

    let mut all: Vec<ChildMetrics> = Vec::new();
    let mut oracle_failures: Vec<(usize, usize, usize)> = Vec::new();
    for &hb in HEADROOM_ARMS {
        for &tc in THREAD_COUNTS {
            for rep in 0..REPETITIONS {
                eprintln!(
                    "--- arm headroom={hb} MiB={} threads={tc} rep={rep}/{REPETITIONS} ---",
                    hb / (1024 * 1024)
                );
                let m = run_one_child(hb, tc, rep);
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
                    oracle_failures.push((hb, tc, rep));
                }
                all.push(m);
            }
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps; min..max in parens) ===");
    println!(
        "{:>10} {:>8} {:>14} {:>16} {:>14} {:>12} {:>10} {:>12}",
        "headroomMB",
        "threads",
        "burst1_used",
        "burst2_hits/poss",
        "hit_rate_pct",
        "rss_b2_kib",
        "rss_idle",
        "ns_per_b2_op",
    );
    for &hb in HEADROOM_ARMS {
        for &tc in THREAD_COUNTS {
            let cell: Vec<&ChildMetrics> = all
                .iter()
                .filter(|m| {
                    m.get("headroom_bytes") == hb as u64 && m.get("thread_count") == tc as u64
                })
                .collect();
            let pick = |k: &str| -> Vec<u64> { cell.iter().map(|m| m.get(k)).collect() };
            let mut b1 = pick("burst1_used_max");
            let mut hits = pick("burst2_hits_sum");
            let poss = cell[0].get("burst2_possible_sum");
            let mut rss2 = pick("rss_burst2_kib");
            let mut rssidle = pick("rss_idle_kib");
            let mut elapsed = pick("burst2_elapsed_ns_sum");
            let (b1_m, hits_m, rss2_m, rssidle_m, elapsed_m) = (
                median(&mut b1),
                median(&mut hits),
                median(&mut rss2),
                median(&mut rssidle),
                median(&mut elapsed),
            );
            let hit_rate_pct = if poss > 0 {
                100.0 * hits_m as f64 / poss as f64
            } else {
                0.0
            };
            let ns_per_op = if poss > 0 {
                elapsed_m as f64 / poss as f64
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
                "{:>10} {:>8} {:>14} {:>16} {:>13.1}% {:>12} {:>10} {:>12.1}{}",
                hb / (1024 * 1024),
                tc,
                b1_m,
                format!("{hits_m}/{poss}"),
                hit_rate_pct,
                rss2_m,
                rssidle_m,
                ns_per_op,
                oracle_tag,
            );
        }
    }

    println!();
    println!("=== CSV (one row per child) ===");
    let cols = [
        "headroom_bytes",
        "headroom_mib",
        "thread_count",
        "repetition",
        "verified_headroom",
        "config_conflicts_delta",
        "process_identity",
        "large_obj_bytes",
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
        "NOTE: each (headroom_bytes, thread_count, rep) cell ran in its OWN freshly-spawned OS \
         process (subprocess-per-arm isolation, registry-bypass via HeapRegistry::claim_with_config \
         — matches R27-3/R29-13's own established shape for the RSS/hit-rate axis). Every arm \
         hard-asserted resolved headroom == requested headroom (read back via \
         AllocCore::dbg_decay_config, NOT assumed) AND config_conflicts_delta == 0 (R26-4 config \
         identity, all 4 pieces: requested/resolved/conflict-delta/process-identity). The REAL \
         #[global_allocator] latency companion for this same headroom sweep lives in the four \
         sibling binaries r30_6_latency_h0/_h16/_h64/_h256."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R30_6_HEADROOM_BYTES").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
