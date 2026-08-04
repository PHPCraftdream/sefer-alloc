//! R34-10 (task #529) — the sparse-decay accumulation gate: measures whether
//! `DECAY_CLOCK_CHECK_STRIDE = 64` (`src/alloc_core/alloc_core_large_cache.rs`,
//! shipped by R32-8 / commit `74345b8`) causes the large-cache RETENTION GAP
//! between the throttled and unthrottled arms to ACCUMULATE beyond one segment
//! over many CONSECUTIVE sparse decay intervals — the regime R33-6
//! (`docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE.md` §9) only argued about
//! qualitatively after measuring a SINGLE sleep + dense cluster.
//!
//! ## Why this exists
//!
//! R33-6 measured ONE sleep followed by a dense cluster of N ops and found the
//! retention cost bounded at exactly one segment (36 MiB) for n_ops ≤ 8. Its
//! §9.3 then asserted "the cost is bounded by one segment per missed decay
//! interval … the throttle delays the tick, it does not skip it entirely across
//! multiple intervals." That assertion was NEVER tested over many consecutive
//! sparse intervals. The decay mechanism is EVENT-DRIVEN (a tick can only fire
//! on a large alloc/free), and `run_decay_step` fires at most ONE step per
//! clock read with NO catch-up loop — so a throttled arm that skips the clock
//! for `stride / events_per_interval` consecutive intervals releases ~1 segment
//! per stride-period while the unthrottled arm releases ~1 segment per interval,
//! and the gap can grow to several segments before the cache hits headroom.
//!
//! This gate measures that gap directly, as a TIME SERIES after every interval,
//! instead of asserting it.
//!
//! ## Design
//!
//! Subprocess-per-(profile, events, arm) isolation (fresh OS process ⇒ fresh
//! registry ⇒ no cross-arm op-counter or `FORCE_DECAY_CLOCK_READ` leakage),
//! matching R33-6/R32-8's methodology. `FORCE_DECAY_CLOCK_READ` is the
//! old-shape/new-shape switch:
//! - `arm = unthrottled` (`forced = true`): bypasses the stride throttle,
//!   reading the clock on EVERY call past headroom — the stride=1 baseline.
//! - `arm = throttled` (`forced = false`): the real shipped stride-64 path.
//!
//! Both arms use the IDENTICAL headroom (16 MiB, `LargeCachePolicy::LowHeadroom`)
//! and the IDENTICAL workload, so the comparison is clean: only the stride
//! differs.
//!
//! **Matrix:** events-per-interval ∈ {1, 2, 4, 8} × 40 consecutive intervals ×
//! 3 profiles (alloc+free / dealloc-only / alloc-only) × 2 arms.
//!
//! ## Why decay_interval = 100 ms, not the 1000 ms shipped default
//!
//! The stride throttle is OP-COUNT-based (`DECAY_CLOCK_CHECK_STRIDE = 64` ops),
//! completely independent of the wall-clock `decay_interval` value: at 1
//! event/interval the throttled arm hits the 64-op stride boundary after the
//! same number of intervals whether each interval is 100 ms or 1000 ms. The
//! only thing the interval length changes is the wall-clock COST per missed
//! interval (the "seconds late" axis), not the op-counting mechanism or the
//! segment-accumulation bound this gate tests. A 100 ms interval keeps the full
//! 40-interval matrix runnable in ~3 minutes instead of ~30 minutes; the
//! "seconds late" numbers are reported and then scaled to the 1000 ms shipped
//! default in the report so the real-world cost is visible.
//!
//! ## Profiles
//!
//! - **allocfree** (PRIMARY, sustained): each event = one alloc+free cycle. The
//!   cache stays populated above headroom; decay is the only drain. This is the
//!   cleanest signal and the one the headline verdict rests on.
//! - **deallocate**: pre-fills 5 of 8 slots (20 MiB > 16 MiB headroom, leaving 3
//!   free), then frees a pre-allocated pool at `events`/interval. The gap
//!   manifests during the filling phase (3 deposits at 1 event/interval before
//!   the cache saturates); once full, deposit-eviction masks decay and both
//!   arms converge. Reported honestly as a transient signal.
//! - **allocate**: pre-fills all 8 slots, then allocs `events`/interval (held,
//!   draining the cache). The gap manifests during the drain phase (cache
//!   capacity-bounded); once the cache hits headroom, decay stops and both arms
//!   converge. Reported honestly as a finite-drain signal.
//!
//! ## Path-activation oracle (R30-8 rule)
//!
//! Three evidence pieces per child:
//! 1. **Headroom crossed:** `used_baseline > headroom_bytes` (the workload
//!    genuinely entered the above-headroom regime the stride applies to).
//! 2. **Unthrottled arm read the clock:** `guard_passed_delta ≥ 1` at end
//!    (the baseline arm actually exercised the clock-read path).
//! 3. **Stride mechanism differs across arms:** checked at the ORCHESTRATOR
//!    level — the throttled arm's `guard_passed_delta` is materially below the
//!    unthrottled arm's, proving the throttle is actually reducing reads.
//!
//! ## Config-resolution evidence (R26-4 rule)
//!
//! Every child self-verifies `verified_headroom == HEADROOM_BYTES` AND
//! `verified_interval_ms == DECAY_INTERVAL_MS` AND
//! `config_conflicts_delta == 0` (fresh process ⇒ first claim is unconditionally
//! the arm's config).
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r34_10_sparse_decay_gate --features "production alloc-stats bench-internals internals"
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

/// Decay interval used by this gate. 100 ms, NOT the 1000 ms shipped default —
/// see the module doc ("Why decay_interval = 100 ms") for why the stride
/// mechanism is interval-independent and only the "seconds late" axis scales.
const DECAY_INTERVAL_MS: u64 = 100;

/// Wall-clock wait per interval. Must exceed `DECAY_INTERVAL_MS` so a decay
/// tick is genuinely "due" on every interval's first past-headroom call.
const INTERVAL_WAIT_MS: u64 = 150;

/// Headroom: 16 MiB = `LargeCachePolicy::LowHeadroom`, one of the two shipped
/// non-default profiles R32-8's stride fix targets (`src/alloc_core/profile.rs`).
const HEADROOM_BYTES: usize = 16 * 1024 * 1024;

/// One large object's requested size. 2 MiB ≫ `SMALL_MAX` (~253 KiB under plain
/// `production`), so classification as `Large` is unambiguous. 2 MiB + header
/// fits in one 4 MiB `SEGMENT`, so each cached span is exactly 1 segment — clean
/// segment-count math for the accumulation bound.
const OBJ_BYTES: usize = 2 * 1024 * 1024;

/// Base large-cache slot count (`LARGE_CACHE_SLOTS = 8`, no `large-cache-
/// extended` in plain `production`). A full cache holds 8 × 1 segment.
const SLOTS: usize = 8;

/// Consecutive sparse intervals per arm. 40 is within the task's "10-40" range
/// and places the throttled arm's first stride-boundary clock read (at
/// `DECAY_CLOCK_CHECK_STRIDE / calls_per_event` intervals) clearly mid-run for
/// the allocfree profile at 1 event/interval (boundary at interval 32), leaving
/// 8 intervals to observe the gap persisting after the read.
const INTERVALS: usize = 40;

/// Events-per-interval arms.
const EVENTS_ARMS: &[usize] = &[1, 2, 4, 8];

/// The three operation profiles (see module doc).
const PROFILES: &[&str] = &["allocfree", "deallocate", "allocate"];

/// `deallocate` pre-fills this many of the 8 base slots (must be enough that
/// `used > headroom`: 5 × 1 segment = 20 MiB > 16 MiB), leaving 3 free for the
/// growth-phase signal.
const DEALLOC_PREFILL_SLOTS: usize = 5;

/// The shipped stride value, mirrored here for self-documenting assertions and
/// the derive script's headline-ratio checks. Must match
/// `DECAY_CLOCK_CHECK_STRIDE` in `alloc_core_large_cache.rs`.
const DECAY_CLOCK_CHECK_STRIDE: u32 = 64;

// ---------------------------------------------------------------------------
// Workload helpers
// ---------------------------------------------------------------------------

/// Touch one byte per 4 KiB page so the reservation is genuinely committed
/// (otherwise a lazy-commit backend would defer the RSS cost out of the
/// measurement window).
///
/// # Safety
///
/// `p` must be a valid allocation of at least `size` bytes that has not yet
/// been freed.
unsafe fn touch_pages(p: *mut u8, size: usize) {
    let page = 4096usize;
    let mut off = 0usize;
    while off < size {
        p.add(off).write_volatile(0xAB);
        off += page;
    }
}

/// Allocate `n` distinct large objects, touch every page, and return the live
/// pointers. Caller owns them until `dealloc`.
fn alloc_objects(heap: &mut HeapCore, layout: Layout, n: usize) -> Vec<*mut u8> {
    let mut live = Vec::with_capacity(n);
    for _ in 0..n {
        let p = heap.alloc(layout);
        assert!(
            !p.is_null(),
            "large alloc failed (OOM?) — reduce n/OBJ_BYTES"
        );
        // SAFETY: `p` is a fresh allocation of `layout.size()` bytes, not yet
        // freed.
        unsafe { touch_pages(p, layout.size()) };
        live.push(p);
    }
    live
}

/// Free every object in `live` (each deposits its span into the large cache).
///
/// # Safety
///
/// Every pointer in `live` must have been allocated by `heap` with `layout` and
/// not yet freed.
unsafe fn free_objects(heap: &mut HeapCore, layout: Layout, live: &[*mut u8]) {
    for &p in live {
        if !p.is_null() {
            heap.dealloc(p, layout);
        }
    }
}

// ---------------------------------------------------------------------------
// Child / arm mode
// ---------------------------------------------------------------------------

fn parse_env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|e| panic!("{name} env var required in child mode ({e})"))
}

fn parse_env_usize(name: &str) -> usize {
    parse_env(name)
        .parse::<usize>()
        .unwrap_or_else(|e| panic!("{name} not a valid usize ({e})"))
}

fn snapshot_rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn run_child() {
    let profile = parse_env("R34_10_PROFILE");
    let events = parse_env_usize("R34_10_EVENTS");
    let arm = parse_env("R34_10_ARM"); // "throttled" | "unthrottled"
    let intervals = parse_env_usize("R34_10_INTERVALS");
    let rep = parse_env_usize("R34_10_REP");
    let forced = arm == "unthrottled";

    let conflicts_before = config_conflicts_total();

    let heap_ptr = HeapRegistry::claim_with_config(
        LargeCacheConfig::new()
            .headroom_bytes(HEADROOM_BYTES)
            .decay_interval_ms(DECAY_INTERVAL_MS as u32),
    );
    assert!(!heap_ptr.is_null(), "claim_with_config returned null");
    // SAFETY: `heap_ptr` was just returned by `claim_with_config` and is owned
    // by THIS thread until `recycle` below.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    // SELF-VERIFICATION: resolved config matches requested (R26-4 rule).
    let (rate_bp, resolved_interval, resolved_headroom) = heap.dbg_decay_config();
    assert_eq!(
        resolved_headroom, HEADROOM_BYTES,
        "resolved headroom {resolved_headroom} != requested {HEADROOM_BYTES}"
    );
    assert_eq!(
        resolved_interval, DECAY_INTERVAL_MS,
        "resolved interval {resolved_interval} != requested {DECAY_INTERVAL_MS}"
    );

    let layout = Layout::from_size_align(OBJ_BYTES, 8).unwrap();

    // ── Pre-fill the cache (profile-specific) ──────────────────────────────
    //
    // allocfree / allocate: fill all 8 slots (free 8 objects → cache = 8 slots).
    // deallocate: fill only DEALLOC_PREFILL_SLOTS slots (free 5 → 5 cached, 3
    //   held) so the measurement's deposits have room to grow into before the
    //   cache saturates and deposit-eviction masks decay.
    let (prefill_free, pool_held_during_prefill): (usize, Vec<*mut u8>) = match profile.as_str() {
        "allocfree" | "allocate" => {
            let live = alloc_objects(heap, layout, SLOTS);
            // SAFETY: all `SLOTS` pointers freshly allocated by `heap`.
            unsafe { free_objects(heap, layout, &live) };
            (SLOTS, Vec::new())
        }
        "deallocate" => {
            // Allocate the pre-fill objects PLUS the measurement pool up front
            // (one contiguous alloc batch is simpler and faster than two).
            let total = DEALLOC_PREFILL_SLOTS + events * intervals;
            let live = alloc_objects(heap, layout, total);
            let (to_free, to_hold) = live.split_at(DEALLOC_PREFILL_SLOTS);
            // SAFETY: the first `DEALLOC_PREFILL_SLOTS` pointers are freshly
            // allocated by `heap`; freeing them seeds the cache.
            unsafe { free_objects(heap, layout, to_free) };
            (DEALLOC_PREFILL_SLOTS, to_hold.to_vec())
        }
        other => panic!("unknown profile {other:?}"),
    };

    let used_baseline = heap.dbg_large_cache_used();
    let guard_passed_baseline = AllocCore::dbg_maybe_decay_guard_passed_count();
    let released_baseline = AllocCore::dbg_segments_released_total();
    let rss_baseline_kib = snapshot_rss_kib();

    // PATH-ACTIVATION ORACLE piece 1: headroom genuinely crossed.
    let headroom_crossed = used_baseline > HEADROOM_BYTES;
    assert!(
        headroom_crossed,
        "workload precondition violated: used_baseline={used_baseline} must exceed HEADROOM_BYTES={HEADROOM_BYTES}"
    );

    // ── Engage the stride switch ───────────────────────────────────────────
    AllocCore::dbg_set_force_decay_clock_read(forced);

    // Track the deallocate pool free-index and the allocate held-set.
    let mut dealloc_next: usize = 0;
    let mut alloc_held: Vec<*mut u8> = Vec::new();

    // ── Measurement loop: one RESULT line per interval ─────────────────────
    for i in 0..intervals {
        thread::sleep(Duration::from_millis(INTERVAL_WAIT_MS));

        match profile.as_str() {
            "allocfree" => {
                for _ in 0..events {
                    let p = heap.alloc(layout);
                    assert!(!p.is_null(), "alloc failed (OOM?)");
                    // SAFETY: `p` freshly allocated by `heap` with `layout`.
                    unsafe { heap.dealloc(p, layout) };
                }
            }
            "deallocate" => {
                let end = (dealloc_next + events).min(pool_held_during_prefill.len());
                // SAFETY: each pool pointer was allocated by `heap` with
                // `layout`, freed exactly once, in order.
                for &p in &pool_held_during_prefill[dealloc_next..end] {
                    unsafe { heap.dealloc(p, layout) };
                }
                dealloc_next = end;
            }
            "allocate" => {
                for _ in 0..events {
                    let p = heap.alloc(layout);
                    assert!(!p.is_null(), "alloc failed (OOM?)");
                    alloc_held.push(p);
                }
            }
            _ => unreachable!(),
        }

        let used_post = heap.dbg_large_cache_used();
        let guard_passed_cum = AllocCore::dbg_maybe_decay_guard_passed_count();
        let released_cum = AllocCore::dbg_segments_released_total();
        let rss_kib = snapshot_rss_kib();

        println!(
            "RESULT ts=1 interval={i} profile={profile} events={events} arm={arm} rep={rep} \
             used_post={used_post} guard_passed_cum={guard_passed_cum} \
             released_cum={released_cum} rss_kib={rss_kib}"
        );
    }

    // Reset the switch before recycling (defensive — fresh process anyway).
    AllocCore::dbg_set_force_decay_clock_read(false);

    let guard_passed_delta =
        AllocCore::dbg_maybe_decay_guard_passed_count().saturating_sub(guard_passed_baseline);

    // PATH-ACTIVATION ORACLE piece 2: the unthrottled arm must have read the
    // clock at least once during the measurement window.
    let unthrottled_read = if forced {
        guard_passed_delta >= 1
    } else {
        true
    };

    let conflicts_delta = config_conflicts_total().saturating_sub(conflicts_before);

    // Free anything still held so the heap can be recycled cleanly (the
    // `allocate` profile's held set + the `deallocate` pool's unfreed tail).
    // SAFETY: all held pointers were allocated by `heap` with `layout`.
    unsafe {
        for &p in &alloc_held {
            heap.dealloc(p, layout);
        }
        let tail = &pool_held_during_prefill[dealloc_next..];
        for &p in tail {
            heap.dealloc(p, layout);
        }
    }

    // SAFETY: `heap_ptr` returned by `claim_with_config`, not yet recycled.
    unsafe { HeapRegistry::recycle(heap_ptr) };

    // ── Config + oracle evidence line (one per child) ──────────────────────
    println!(
        "RESULT config=1 profile={profile} events={events} arm={arm} rep={rep} \
         intervals={intervals} headroom_bytes={HEADROOM_BYTES} obj_bytes={OBJ_BYTES} \
         decay_interval_ms={DECAY_INTERVAL_MS} decay_rate_bp={rate_bp} \
         stride={DECAY_CLOCK_CHECK_STRIDE} slots={SLOTS} prefill_free={prefill_free} \
         verified_headroom={resolved_headroom} verified_interval_ms={resolved_interval} \
         config_conflicts_delta={conflicts_delta} used_baseline={used_baseline} \
         guard_passed_baseline={guard_passed_baseline} released_baseline={released_baseline} \
         rss_baseline_kib={rss_baseline_kib} guard_passed_delta={guard_passed_delta} \
         headroom_crossed={} unthrottled_read={} process_identity=subprocess",
        u64::from(headroom_crossed),
        u64::from(unthrottled_read),
    );

    let oracle_pass = headroom_crossed && unthrottled_read && conflicts_delta == 0;
    println!(
        "RESULT oracle=1 profile={profile} events={events} arm={arm} rep={rep} \
         oracle_pass={}",
        u64::from(oracle_pass),
    );
}

// ---------------------------------------------------------------------------
// Orchestrator mode
// ---------------------------------------------------------------------------

fn run_one_child(profile: &str, events: usize, arm: &str, intervals: usize, rep: usize) {
    eprintln!(
        "--- profile={profile} events={events} arm={arm} intervals={intervals} rep={rep} ---"
    );
    let exe = std::env::current_exe().unwrap_or_else(|e| panic!("current_exe: {e}"));
    let output = std::process::Command::new(&exe)
        .env("R34_10_PROFILE", profile)
        .env("R34_10_EVENTS", events.to_string())
        .env("R34_10_ARM", arm)
        .env("R34_10_INTERVALS", intervals.to_string())
        .env("R34_10_REP", rep.to_string())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .output()
        .unwrap_or_else(|e| panic!("spawning child: {e}"));
    if !output.status.success() {
        panic!(
            "child (profile={profile}, events={events}, arm={arm}, rep={rep}) failed: {:?}",
            output.status.code()
        );
    }
    // Echo the child's RESULT lines to THIS process's stdout so they land in
    // the captured raw log (the child's stdout was piped, not inherited).
    print!("{}", String::from_utf8_lossy(&output.stdout));
}

fn run_orchestrator() {
    println!(
        "=== R34-10 sparse-decay accumulation gate — throttled (stride={DECAY_CLOCK_CHECK_STRIDE}) \
         vs unthrottled (stride=1), {INTERVALS} consecutive intervals ==="
    );
    println!(
        "profiles: {PROFILES:?} | events: {EVENTS_ARMS:?} | headroom={HEADROOM_BYTES} \
         obj_bytes={OBJ_BYTES} decay_interval_ms={DECAY_INTERVAL_MS} \
         wait_ms={INTERVAL_WAIT_MS} slots={SLOTS}"
    );
    println!();

    for &events in EVENTS_ARMS {
        for &profile in PROFILES {
            for arm in ["throttled", "unthrottled"] {
                run_one_child(profile, events, arm, INTERVALS, 0);
            }
        }
    }

    println!();
    println!(
        "=== all children complete; per-interval time series + config/oracle lines above are the \
         raw per-sample data the derive script (scripts/r34_10_sparse_decay_summary.mjs) turns \
         into the summary CSV + report tables ==="
    );
}

fn main() {
    let _ = bootstrap::ensure();
    let child_mode = std::env::var_os("R34_10_ARM").is_some();
    if child_mode {
        run_child();
    } else {
        run_orchestrator();
    }
}
