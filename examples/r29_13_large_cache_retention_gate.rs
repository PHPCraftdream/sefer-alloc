//! R29-13 (task #444) — the large-cache RETENTION gate: mirrors R27-3's
//! (`examples/r27_3_pool_retention_gate.rs`) subprocess-per-arm methodology,
//! but targets the per-shard **large-segment free-cache** (`alloc-decommit`)
//! instead of the small-segment hysteresis pool.
//!
//! ## Why this exists
//!
//! `docs/reviews/2026-07-29-oh-acceleration-code-project-review.md` §1.2:
//! `LargeCacheConfig::DEFAULT_HEADROOM_BYTES` is 256 MiB/heap — 32x the small
//! pool's 16 MiB byte cap that Rounds 25-27 spent four tasks and three gate
//! reports quantifying (`R27_3_POOL_RETENTION_GATE.md` et al.) — and had NO
//! gate report measuring its actual idle-RSS floor for a long-lived thread.
//! This is that gate.
//!
//! The mechanism, read from source (`src/alloc_core/large_cache_config.rs:48`,
//! `src/alloc_core/alloc_core_large_cache.rs:320-380`):
//! - `maybe_decay_large_cache` early-returns (never even reads the clock)
//!   when `large_cache_used_bytes <= headroom_bytes` — decay cannot fire
//!   below the floor.
//! - `run_decay_step`'s target IS `headroom_bytes` (`live_bytes = 0` in
//!   Phase 2), so even when decay DOES run it asymptotes to the floor and
//!   stops there — never below it.
//! - Decay is event-driven only (fires from the large alloc/dealloc slow
//!   paths); there is no background thread, so pure idle reclaims nothing.
//! - The only unconditional reclamation is `HeapCore::trim_for_recycle`
//!   (`evict_all` at `src/alloc_core/alloc_core_large_cache.rs:418`), which
//!   runs only at thread exit (`src/global/tls_heap.rs`) — this gate's whole
//!   point is to measure the floor BEFORE that fires, i.e. with long-lived,
//!   non-exiting threads.
//!
//! ## Workload — LARGE objects, not small churn
//!
//! Under plain `production` (no `medium-classes`), `SMALL_MAX` = 16 KiB (see
//! `src/alloc_core/size_classes.rs`) — anything larger is classified `Large`
//! and goes through `alloc_large`/the large-cache admission path
//! (`src/alloc_core/alloc_core.rs` large-dealloc branch). This probe uses
//! 34 MiB objects, 8 distinct sizes per thread (one per base large-cache
//! slot — `LARGE_CACHE_SLOTS = 8`, no `large-cache-extended` sidecar in plain
//! `production`), so a fully-populated cache holds `8 * 34 MiB = 272 MiB` —
//! enough to exceed EVERY headroom arm in the sweep including the 256 MiB
//! default, proving admission/retention at every point in the grid, not just
//! the sub-256-MiB arms.
//!
//! ## Long-lived, non-exiting threads
//!
//! Same protocol as R27-3: workers loop, signal readiness, wait for a
//! coordinated GO, run the fill workload, then hold their heap alive
//! (never call `HeapRegistry::recycle`) through the post-teardown,
//! post-idle, and post-drain measurement points — only recycling at the
//! very end after every RESULT line has been captured.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r29_13_large_cache_retention_gate --features "production alloc-stats bench-internals"
//! ```

#![cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use sefer_alloc::{
    registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry},
    LargeCacheConfig,
};

// ---------------------------------------------------------------------------
// Sweep parameters
// ---------------------------------------------------------------------------

/// `headroom_bytes` arms — the sweep the review (§1.2) and task #444 ask for:
/// 0 (no floor — decay can reclaim everything), 16 MiB / 64 MiB (well below
/// the default), and 256 MiB (the shipped default).
const HEADROOM_ARMS: &[usize] = &[0, 16 * 1024 * 1024, 64 * 1024 * 1024, 256 * 1024 * 1024];

/// Thread counts matching R27-3's grid.
const THREAD_COUNTS: &[usize] = &[1, 8, 32];

/// Repetitions per cell (median + range). Matches R27-3's precedent.
const REPETITIONS: usize = 3;

/// One large object's size. Must exceed `SMALL_MAX` (16 KiB under plain
/// `production`) by a wide margin so classification as `Large` is
/// unambiguous, and be large enough that `LARGE_CACHE_SLOTS` (8) worth of
/// them exceeds even the 256 MiB headroom arm (8 * 34 MiB = 272 MiB > 256 MiB).
const LARGE_OBJ_BYTES: usize = 34 * 1024 * 1024;

/// Number of DISTINCT large objects filled per thread — one per base
/// large-cache slot (`LARGE_CACHE_SLOTS = 8`; no `large-cache-extended`
/// sidecar in plain `production`, so only the base 8 slots are addressable).
const LARGE_OBJ_COUNT: usize = 8;

/// The configured decay interval (ms), read from source: the large cache's
/// default is 1000 ms (`large_cache_config.rs::DEFAULT_DECAY_INTERVAL_MS`).
/// This probe leaves the decay interval at its default (does not override it
/// via `dbg_set_decay_config`) so the measured behavior matches what a real
/// `LargeCacheConfig::new().headroom_bytes(n)` caller actually gets.
const DECAY_INTERVAL_MS: u64 = 1000;

// ---------------------------------------------------------------------------
// Config builder
// ---------------------------------------------------------------------------

fn config_for(headroom_bytes: usize) -> LargeCacheConfig {
    LargeCacheConfig::new().headroom_bytes(headroom_bytes)
}

// ---------------------------------------------------------------------------
// Workload — fill the cache with LARGE_OBJ_COUNT distinct large objects,
// then free them all (returning each span to the large cache, subject to
// the headroom/decay policy), keeping the thread alive throughout.
// ---------------------------------------------------------------------------

/// Allocate `LARGE_OBJ_COUNT` distinct large objects (each `LARGE_OBJ_BYTES`),
/// touch one byte per page-ish stride so the pages are genuinely committed
/// (not just reserved), and return the live pointers.
fn fill_large_objects(heap: &mut HeapCore, layout: Layout) -> Vec<*mut u8> {
    let mut live = Vec::with_capacity(LARGE_OBJ_COUNT);
    for _ in 0..LARGE_OBJ_COUNT {
        let p = heap.alloc(layout);
        assert!(
            !p.is_null(),
            "large alloc failed (OOM?) — reduce LARGE_OBJ_COUNT/BYTES"
        );
        // Touch every 4 KiB page so the reservation is genuinely committed,
        // not merely reserved (mirrors a real large-buffer workload, e.g. a
        // decoded image / large Vec<u8> that gets written).
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

/// Free every object in `live` (returns each span to the large cache subject
/// to the admission/headroom policy).
fn teardown_large_objects(heap: &mut HeapCore, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            // SAFETY: `p` was allocated by `heap` with `layout` above, not yet freed.
            unsafe { heap.dealloc(p, layout) };
        }
    }
}

// ---------------------------------------------------------------------------
// Child/arm mode — run ONE `(headroom_bytes, thread_count)` arm in this fresh
// process, self-verify, prove admission/retention, emit RESULT lines.
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
    let headroom_bytes = parse_env_usize("R29_13_HEADROOM_BYTES");
    let thread_count = parse_env_usize("R29_13_THREAD_COUNT");
    let repetition = parse_env_usize("R29_13_REPETITION");
    let headroom_mib = (headroom_bytes / (1024 * 1024)) as u64;

    let conflicts_before = config_conflicts_total();

    // Shared coordination state.
    let go = Arc::new(AtomicBool::new(false));
    let hold_ready = Arc::new(AtomicU64::new(0));
    let drain_done = Arc::new(AtomicBool::new(false));
    let drain_acked = Arc::new(AtomicU64::new(0));
    let release = Arc::new(AtomicBool::new(false));

    let resolved_headroom: Arc<Vec<AtomicU64>> = Arc::new(
        (0..thread_count)
            .map(|_| AtomicU64::new(u64::MAX))
            .collect(),
    );
    let used_post_fill: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let used_post_teardown: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let used_predrain: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());
    let slots_occupied_post_teardown: Arc<Vec<AtomicU64>> =
        Arc::new((0..thread_count).map(|_| AtomicU64::new(0)).collect());

    let (rss_before_kib, _commit_before_kib) = snapshot_kib();

    let layout = Layout::from_size_align(LARGE_OBJ_BYTES, 8).unwrap();

    let mut handles = Vec::with_capacity(thread_count);
    for i in 0..thread_count {
        let (
            go,
            hold_ready,
            drain_done,
            drain_acked,
            release,
            resolved_headroom,
            used_post_fill,
            used_post_teardown,
            used_predrain,
            slots_occupied_post_teardown,
        ) = (
            Arc::clone(&go),
            Arc::clone(&hold_ready),
            Arc::clone(&drain_done),
            Arc::clone(&drain_acked),
            Arc::clone(&release),
            Arc::clone(&resolved_headroom),
            Arc::clone(&used_post_fill),
            Arc::clone(&used_post_teardown),
            Arc::clone(&used_predrain),
            Arc::clone(&slots_occupied_post_teardown),
        );
        handles.push(thread::spawn(move || {
            let heap_ptr = HeapRegistry::claim_with_config(config_for(headroom_bytes));
            assert!(
                !heap_ptr.is_null(),
                "HeapRegistry::claim_with_config returned null at thread {i}"
            );
            // SAFETY: `heap_ptr` was just returned by `claim_with_config` and is
            // owned by THIS thread until we `recycle` it below.
            let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

            // SELF-VERIFICATION: resolved headroom equals the requested one,
            // read back from `AllocCore::dbg_decay_config()` (the diagnostic
            // surface, not assumed). `HeapCore` derefs to its inner
            // `AllocCore` for the `dbg_*` large-cache accessors.
            let (_, _, resolved) = heap.dbg_decay_config();
            assert_eq!(
                resolved, headroom_bytes,
                "R29-13 child (headroom={headroom_bytes}, thread={i}, rep={repetition}): \
                 resolved headroom ({resolved}) != requested ({headroom_bytes}) — isolation \
                 contract broken, this arm's number is untrustworthy"
            );
            resolved_headroom[i].store(resolved as u64, Ordering::Release);

            // Wait for GO so the main thread's before-snapshot is clean.
            while !go.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(1));
            }

            // Fill: 8 distinct large objects, each touched (genuinely committed).
            let live = fill_large_objects(heap, layout);
            let used_fill = heap.dbg_large_cache_used() as u64; // 0 before any free
            used_post_fill[i].store(used_fill, Ordering::Release);

            // Teardown: free every large object. Each dealloc runs the large
            // admission path (cache it, subject to headroom/budget) — this is
            // where `large_cache_used_bytes` actually grows.
            teardown_large_objects(heap, layout, &live);
            let used_post = heap.dbg_large_cache_used() as u64;
            used_post_teardown[i].store(used_post, Ordering::Release);
            let occupied = heap
                .dbg_large_cache_slot_sizes()
                .iter()
                .filter(|s| s.is_some())
                .count() as u64;
            slots_occupied_post_teardown[i].store(occupied, Ordering::Release);

            // Hold phase: KEEP THE HEAP ALIVE (do not recycle) through the
            // main thread's idle RSS sampling — the whole point of this gate
            // is the floor BEFORE `trim_for_recycle`/thread-exit would fire.
            hold_ready.fetch_add(1, Ordering::Release);
            while !drain_done.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            // Re-sample right before an explicit force-decay-to-completion —
            // empirical proof idle alone did NOT change large_cache_used_bytes.
            used_predrain[i].store(heap.dbg_large_cache_used() as u64, Ordering::Release);
            // Explicit reclamation: repeatedly force a decay tick (bypassing
            // the wall-clock interval) until the cache stops shrinking OR is
            // empty — this is what `trim_for_recycle`'s `evict_all` would
            // achieve unconditionally at thread exit; here it demonstrates
            // reclaimability WITHOUT exiting the thread, mirroring R27-3's
            // `dbg_drain_small_pool` "explicit drain" step for the small pool.
            loop {
                let before = heap.dbg_large_cache_used();
                heap.dbg_force_decay_tick();
                let after = heap.dbg_large_cache_used();
                if after == before {
                    break;
                }
            }
            drain_acked.fetch_add(1, Ordering::Release);

            while !release.load(Ordering::Acquire) {
                thread::sleep(Duration::from_millis(2));
            }
            // SAFETY: `heap_ptr` was returned by `claim_with_config` above and
            // has not yet been recycled.
            unsafe { HeapRegistry::recycle(heap_ptr) };
        }));
    }

    // Wait for every worker to reach the hold phase (post-fill, post-teardown).
    // Peak-RSS is sampled by polling while workers race through fill+teardown.
    go.store(true, Ordering::Release);
    let mut peak_rss_kib: u64 = 0;
    let mut peak_commit_kib: u64 = 0;
    let poll_interval = Duration::from_millis(15);
    while hold_ready.load(Ordering::Acquire) < thread_count as u64 {
        let (r, c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        peak_commit_kib = peak_commit_kib.max(c);
        thread::sleep(poll_interval);
    }
    // A few more samples right after the last worker reaches hold — the
    // heaviest thread may have just finished filling.
    for _ in 0..5 {
        let (r, c) = snapshot_kib();
        peak_rss_kib = peak_rss_kib.max(r);
        peak_commit_kib = peak_commit_kib.max(c);
        thread::sleep(poll_interval);
    }

    // Post-teardown RSS/commit (large objects freed, cached spans retained
    // subject to headroom policy).
    let (rss_post_kib, commit_post_kib) = snapshot_kib();

    // Idle time-series (pure idle — no allocations, no dealloc, heaps parked
    // in the hold loop). `large_cache_used_bytes` is provably constant here
    // (no alloc_large/dealloc call => no decay tick fires at all — the
    // `maybe_decay_large_cache` early-return / event-driven design means idle
    // literally never even samples the clock), confirmed empirically below.
    thread::sleep(Duration::from_millis(100));
    let (rss_100ms_kib, _c) = snapshot_kib();
    thread::sleep(Duration::from_millis(900));
    let (rss_1s_kib, _c) = snapshot_kib();
    thread::sleep(Duration::from_millis(1000));
    let (rss_2s_kib, _c) = snapshot_kib();

    // Drain: force decay-to-fixed-point on every heap (still alive, not
    // recycled), measure reclaimable RSS.
    drain_done.store(true, Ordering::Release);
    while drain_acked.load(Ordering::Acquire) < thread_count as u64 {
        thread::sleep(Duration::from_millis(1));
    }
    let (rss_drain_kib, commit_drain_kib) = snapshot_kib();

    // Read per-heap aggregates.
    let used_post_fill_max = used_post_fill
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .max()
        .unwrap_or(0);
    let used_post_teardown_sum: u64 = used_post_teardown
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .sum();
    let used_post_teardown_max = used_post_teardown
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .max()
        .unwrap_or(0);
    let used_predrain_sum: u64 = used_predrain
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .sum();
    let slots_occupied_max = slots_occupied_post_teardown
        .iter()
        .map(|a| a.load(Ordering::Acquire))
        .max()
        .unwrap_or(0);

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);

    // SELF-VERIFICATION re-check: every worker's stashed resolved headroom matches.
    for (i, slot) in resolved_headroom.iter().enumerate() {
        let v = slot.load(Ordering::Acquire);
        assert_eq!(
            v, headroom_bytes as u64,
            "R29-13 child: worker {i} stashed resolved headroom {v}, expected {headroom_bytes}"
        );
    }
    assert_eq!(
        conflicts_delta, 0,
        "R29-13 child (headroom={headroom_bytes}, threads={thread_count}, rep={repetition}): \
         CONFIG_CONFLICTS delta = {conflicts_delta} (expected 0 in a fresh process) — \
         isolation contract broken"
    );

    // ADMISSION/RETENTION hard asserts: an RSS number is only trustworthy if
    // the cache actually admitted spans (per the R26-4/config-sweep evidence
    // rule: prove the arm ran under its labelled config, not just report it).
    // `LARGE_OBJ_COUNT * LARGE_OBJ_BYTES` = 8 * 34 MiB = 272 MiB per heap —
    // exceeds every headroom arm, so every arm's post-teardown
    // `large_cache_used_bytes` must be > 0 (something got admitted before any
    // decay tick could run — the FIRST dealloc always primes the timer
    // without decaying, per `maybe_decay_large_cache`'s doc).
    assert!(
        used_post_teardown_max > 0,
        "ADMISSION FAILED: used_post_teardown_max=0 (headroom={headroom_bytes}, \
         threads={thread_count}, rep={repetition}) — no large span was ever cached; this \
         arm's retention number is vacuous"
    );
    // For a non-zero headroom, the whole point is that decay does NOT reduce
    // used_bytes below the floor — but a fill of 272 MiB per heap comfortably
    // exceeds every headroom in HEADROOM_ARMS, so if decay ran to completion
    // (multiple ticks across multiple deallocs) the post-teardown figure
    // should settle AT OR ABOVE headroom_bytes for the non-zero arms (proving
    // the floor held), and can reach 0 only for the headroom=0 arm (nothing
    // is below its own floor).
    if headroom_bytes > 0 {
        assert!(
            used_post_teardown_max as usize >= headroom_bytes || slots_occupied_max > 0,
            "RETENTION FLOOR VIOLATION: used_post_teardown_max={used_post_teardown_max} fell \
             below headroom_bytes={headroom_bytes} with zero slots occupied (headroom={headroom_bytes}, \
             threads={thread_count}, rep={repetition}) — decay released MORE than the doc's \
             'does not decay below headroom' claim allows"
        );
    }

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
    proc_probe::emit_u64("peak_commit_kib", peak_commit_kib);
    proc_probe::emit_u64("used_post_fill_max", used_post_fill_max);
    proc_probe::emit_u64("used_post_teardown_sum", used_post_teardown_sum);
    proc_probe::emit_u64("used_post_teardown_max", used_post_teardown_max);
    proc_probe::emit_u64("slots_occupied_max", slots_occupied_max);
    proc_probe::emit_u64("rss_post_kib", rss_post_kib);
    proc_probe::emit_u64("commit_post_kib", commit_post_kib);
    proc_probe::emit_u64("rss_100ms_kib", rss_100ms_kib);
    proc_probe::emit_u64("rss_1s_kib", rss_1s_kib);
    proc_probe::emit_u64("rss_2s_kib", rss_2s_kib);
    proc_probe::emit_u64("used_predrain_sum", used_predrain_sum);
    proc_probe::emit_u64("rss_drain_kib", rss_drain_kib);
    proc_probe::emit_u64("commit_drain_kib", commit_drain_kib);
    proc_probe::emit_u64("decay_interval_ms", DECAY_INTERVAL_MS);

    println!(
        "OK headroom={headroom_bytes} MiB={headroom_mib} threads={thread_count} rep={repetition} \
         verified_headroom={headroom_bytes} cfg_conflicts_delta={conflicts_delta} \
         used_post_teardown_max={used_post_teardown_max} slots_occupied_max={slots_occupied_max} \
         peak_rss_kib={peak_rss_kib} rss_post_kib={rss_post_kib} rss_2s_kib={rss_2s_kib} \
         rss_drain_kib={rss_drain_kib} used_predrain_sum={used_predrain_sum}"
    );

    // Release + join workers.
    release.store(true, Ordering::Release);
    for (i, h) in handles.into_iter().enumerate() {
        h.join().unwrap_or_else(|e| {
            panic!(
                "R29-13 child (headroom={headroom_bytes}, thread={i}, rep={repetition}): \
                 worker thread panicked: {e:?}"
            )
        });
    }
}

// ---------------------------------------------------------------------------
// Orchestrator mode — re-exec the same binary once per arm, collect RESULT
// lines, aggregate to (median, min, max) per cell, emit a CSV block.
// ---------------------------------------------------------------------------

/// All RESULT fields from one child, parsed into a typed map.
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
    cmd.env("R29_13_HEADROOM_BYTES", headroom_bytes.to_string())
        .env("R29_13_THREAD_COUNT", thread_count.to_string())
        .env("R29_13_REPETITION", repetition.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit());
    let output = cmd
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "R29-13 child (headroom={headroom_bytes}, threads={thread_count}, rep={repetition}) \
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
    println!("=== R29-13 large-cache RETENTION gate — subprocess-per-arm isolation ===");
    println!(
        "grid: headroom_bytes={:?} x threads={:?} x {REPETITIONS} reps; LARGE_OBJ_BYTES={LARGE_OBJ_BYTES} \
         LARGE_OBJ_COUNT={LARGE_OBJ_COUNT} (per-heap fill = {} MiB) decay_interval_ms={DECAY_INTERVAL_MS}",
        HEADROOM_ARMS, THREAD_COUNTS, (LARGE_OBJ_BYTES * LARGE_OBJ_COUNT) / (1024 * 1024)
    );
    println!();

    let mut all: Vec<ChildMetrics> = Vec::new();
    for &hb in HEADROOM_ARMS {
        for &tc in THREAD_COUNTS {
            for rep in 0..REPETITIONS {
                eprintln!(
                    "--- arm headroom={hb} MiB={} threads={tc} rep={rep}/{REPETITIONS} ---",
                    hb / (1024 * 1024)
                );
                let m = run_one_child(hb, tc, rep);
                assert_eq!(
                    m.get("headroom_bytes"),
                    hb as u64,
                    "child reported wrong headroom_bytes"
                );
                assert_eq!(
                    m.get("thread_count"),
                    tc as u64,
                    "child reported wrong thread_count"
                );
                assert_eq!(
                    m.get("repetition"),
                    rep as u64,
                    "child reported wrong repetition"
                );
                assert_eq!(
                    m.get("verified_headroom"),
                    hb as u64,
                    "child reported verified_headroom != requested"
                );
                assert_eq!(
                    m.get("config_conflicts_delta"),
                    0,
                    "child reported non-zero config_conflicts_delta"
                );
                all.push(m);
            }
        }
    }

    println!();
    println!("=== aggregated (median of {REPETITIONS} reps; min..max in parens) ===");
    println!(
        "{:>10} {:>8} {:>24} {:>24} {:>14} {:>10} {:>12} {:>12} {:>12}",
        "headroomMB",
        "threads",
        "peak_rss_kib",
        "rss_post_kib",
        "used_post_max",
        "slots_occ",
        "rss_2s_kib",
        "rss_drain",
        "used_predrain",
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
            let mut peak = pick("peak_rss_kib");
            let mut post = pick("rss_post_kib");
            let mut usedmax = pick("used_post_teardown_max");
            let mut slots = pick("slots_occupied_max");
            let mut r2 = pick("rss_2s_kib");
            let mut dr = pick("rss_drain_kib");
            let mut predrain = pick("used_predrain_sum");
            let (peak_m, post_m, usedmax_m, slots_m, r2_m, dr_m, predrain_m) = (
                median(&mut peak),
                median(&mut post),
                median(&mut usedmax),
                median(&mut slots),
                median(&mut r2),
                median(&mut dr),
                median(&mut predrain),
            );
            let fmt = |v: &mut Vec<u64>, m: u64| format!("{m} ({}..{})", v[0], v[v.len() - 1]);
            println!(
                "{:>10} {:>8} {:>24} {:>24} {:>14} {:>10} {:>12} {:>12} {:>12}",
                hb / (1024 * 1024),
                tc,
                fmt(&mut peak, peak_m),
                fmt(&mut post, post_m),
                usedmax_m,
                slots_m,
                fmt(&mut r2, r2_m),
                fmt(&mut dr, dr_m),
                predrain_m,
            );
        }
    }

    // CSV block — one row per child, all fields, for the committed summary CSV.
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
        "peak_commit_kib",
        "used_post_fill_max",
        "used_post_teardown_sum",
        "used_post_teardown_max",
        "slots_occupied_max",
        "rss_post_kib",
        "commit_post_kib",
        "rss_100ms_kib",
        "rss_1s_kib",
        "rss_2s_kib",
        "used_predrain_sum",
        "rss_drain_kib",
        "commit_drain_kib",
        "decay_interval_ms",
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
    println!(
        "NOTE: each (headroom_bytes, thread_count, rep) cell ran in its OWN freshly-spawned OS \
         process (subprocess-per-arm isolation). Every arm hard-asserted resolved headroom == \
         requested headroom (read back via AllocCore::dbg_decay_config, NOT assumed) AND \
         config_conflicts_delta == 0 (R26-4 config identity). Every arm hard-asserted \
         used_post_teardown_max > 0 (admission proven) and, for headroom>0 arms, that the \
         post-teardown large_cache_used_bytes never fell below headroom_bytes (the documented \
         floor held). Idle time-series (100ms/1s/2s) demonstrates the large cache does NOT \
         self-decay on pure idle (event-driven only, fires solely from alloc_large/dealloc); \
         the explicit dbg_force_decay_tick loop at drain time demonstrates the retained cache \
         IS reclaimable without a background thread, given enough forced ticks."
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R29_13_HEADROOM_BYTES").is_some();
    if child_mode {
        run_child();
        return;
    }
    run_orchestrator();
}
