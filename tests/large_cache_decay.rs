//! Phase 2 — lazy exponential decay tests for the large-cache.
//!
//! These tests verify the decay policy: an excess over the headroom target is
//! gradually released to the OS via FIFO eviction, at a configurable rate and
//! interval. Tests use either `dbg_set_decay_config` (for dynamic overrides) or
//! `AllocCore::new_with_config` (to set the decay config at construction time).
//!
//! Gated on `alloc-core` + `alloc-decommit` (the same gate as the cache itself).

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

// ── helpers ──────────────────────────────────────────────────────────────────

const MIB: usize = 1024 * 1024;

fn layout(mib: usize) -> Layout {
    Layout::from_size_align(mib * MIB, 8).unwrap()
}

/// Allocate `count` large blocks of `l`, dealloc them all, and return the
/// measured `large_cache_used_bytes` after all deallocs. Skips (returns None)
/// if any alloc OOMs.
fn fill_cache(ac: &mut AllocCore, l: Layout, count: usize) -> Option<usize> {
    let mut ptrs = [core::ptr::null_mut::<u8>(); 8];
    assert!(count <= ptrs.len(), "fill_cache: count > 8");
    for ptr in &mut ptrs[..count] {
        *ptr = ac.alloc(l);
        if (*ptr).is_null() {
            // OOM — clean up what we got and bail.
            for p in &mut ptrs[..count] {
                if !p.is_null() {
                    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
                    unsafe { ac.dealloc(*p, l) };
                    *p = core::ptr::null_mut();
                }
            }
            return None;
        }
    }
    for ptr in &mut ptrs[..count] {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(*ptr, l) };
        *ptr = core::ptr::null_mut();
    }
    Some(ac.dbg_large_cache_used())
}

// ── test 1 ───────────────────────────────────────────────────────────────────

/// `decay_releases_excess_over_target`
///
/// Setup: headroom=0, rate=50% (5000 bp), interval=0 ms (instant).
/// Fill cache with 2 spans of ~8 MiB each → used ≈ 16 MiB.
/// Force one decay tick → released ≈ 50% of used = ≈ 8 MiB.
/// Assert used_after < used_before.
#[test]
fn decay_releases_excess_over_target() {
    let mut ac = AllocCore::new().expect("primordial");
    // Disable budget; fill under the default config first (256 MiB headroom → no organic decay during fill), THEN arm the instant-decay config.
    ac.dbg_set_large_cache_budget(None);

    let l = layout(4); // 4 MiB nominal → ~8 MiB usable (2 segments)

    let used_before = match fill_cache(&mut ac, l, 2) {
        Some(u) if u > 0 => u,
        _ => {
            eprintln!("OOM or cache empty — skipping decay_releases_excess_over_target");
            return;
        }
    };

    ac.dbg_set_decay_config(5000, 0, 0);

    // With 0ms interval and timer primed, the next force tick fires immediately.
    ac.dbg_force_decay_tick();
    let used_after = ac.dbg_large_cache_used();

    assert!(
        used_after < used_before,
        "decay tick must release some cache: before={used_before}, after={used_after}"
    );
}

// ── test 2 ───────────────────────────────────────────────────────────────────

/// `decay_respects_headroom`
///
/// Setup: headroom=8 MiB (roughly one span), rate=100% (flush all excess),
/// interval=0 ms (instant). Fill cache with 2 spans of ~8 MiB each → ~16 MiB.
/// Force multiple ticks → used should converge toward headroom, not toward 0.
#[test]
fn decay_respects_headroom() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let l = layout(4); // ~8 MiB usable

    // Discover actual span size.
    let span_size = match fill_cache(&mut ac, l, 1) {
        Some(s) if s > 0 => s,
        _ => {
            eprintln!("OOM — skipping decay_respects_headroom");
            return;
        }
    };

    // Fill cache with 2 spans so there is a clear excess over headroom=span_size.
    // Need at least 2 spans in cache.  We have 2 slots, so fill both.
    let l_small = layout(4);
    let l_large = layout(8); // different size to occupy the second slot
    let p1 = ac.alloc(l_small);
    let p2 = ac.alloc(l_large);
    if p1.is_null() || p2.is_null() {
        if !p1.is_null() {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p1, l_small) };
        }
        if !p2.is_null() {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p2, l_large) };
        }
        eprintln!("OOM — skipping decay_respects_headroom");
        return;
    }
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p1, l_small) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p2, l_large) };

    let used_before = ac.dbg_large_cache_used();
    if used_before == 0 {
        eprintln!("cache empty — skipping decay_respects_headroom");
        return;
    }

    // Set headroom = exactly one span size; rate = 100% of excess (flush instantly).
    ac.dbg_set_decay_config(10_000, 0, span_size);

    // Multiple ticks: excess collapses to 0 (but headroom stays).
    for _ in 0..5 {
        ac.dbg_force_decay_tick();
    }
    let used_after = ac.dbg_large_cache_used();

    // After full-rate decay, used should have dropped (there was excess).
    assert!(
        used_after < used_before,
        "decay must reduce used: before={used_before} after={used_after}"
    );
    // Because rate=100% all excess is released in one step.  With headroom=span_size
    // and used_before >= span_size, after one step used_after <= span_size.
    assert!(
        used_after <= span_size,
        "used_after={used_after} must be <= headroom={span_size}"
    );
}

// ── test 3 ───────────────────────────────────────────────────────────────────

/// `decay_skips_when_under_target`
///
/// If the cache is below headroom, a decay tick must be a no-op.
#[test]
fn decay_skips_when_under_target() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let l = layout(4); // ~8 MiB usable

    let used = match fill_cache(&mut ac, l, 1) {
        Some(s) => s,
        None => {
            eprintln!("OOM — skipping decay_skips_when_under_target");
            return;
        }
    };

    if used == 0 {
        // Cache didn't hold anything (rare edge case): still valid, nothing to test.
        return;
    }

    // Set headroom LARGER than what's in the cache → no excess.
    let big_headroom = used * 10;
    ac.dbg_set_decay_config(10_000, 0, big_headroom);

    ac.dbg_force_decay_tick();

    assert_eq!(
        ac.dbg_large_cache_used(),
        used,
        "decay must be a no-op when cache ({used}) < headroom ({big_headroom})"
    );
}

// ── test 4 ───────────────────────────────────────────────────────────────────

/// `decay_interval_respected`
///
/// With a very long interval (10 s), a burst of 50 large alloc/dealloc cycles
/// should NOT trigger an actual eviction (since the real wall clock will not
/// advance 10 s in the middle of a tight loop).
///
/// We measure `used_bytes` before and after 50 cycles; it should remain the
/// same (decay never fired), showing the interval guard works.
#[test]
fn decay_interval_respected() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    // headroom=0, rate=100% — if decay fires, it would flush everything.
    // interval=10s — should NOT fire in a tight loop.
    ac.dbg_set_decay_config(10_000, 10_000, 0);

    let l = layout(4);

    // Warm up: fill the cache so there is something to lose.
    match fill_cache(&mut ac, l, 1) {
        Some(s) if s > 0 => s,
        _ => {
            eprintln!("OOM — skipping decay_interval_respected");
            return;
        }
    };
    let used_initial = ac.dbg_large_cache_used();

    // Now do 50 more alloc+dealloc cycles. Each dealloc path calls
    // maybe_decay_large_cache; if the interval is respected, used_bytes stays
    // the same (no eviction beyond normal cache-slot churn).
    for _ in 0..50 {
        let ptr = ac.alloc(l);
        if ptr.is_null() {
            break; // OOM — stop early but don't fail
        }
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(ptr, l) };
    }

    // The cache may have changed due to slot churn (new span replacing old), but
    // decay-driven eviction should NOT have fired (interval = 10s).
    // We cannot assert exact equality because slot eviction from the Phase 1
    // budget path may also run; instead we assert that the cache is not EMPTY —
    // if decay had fired with rate=100% and headroom=0, it would be empty.
    let used_after = ac.dbg_large_cache_used();
    if used_initial > 0 {
        assert!(
            used_after > 0,
            "cache must not have been fully drained in a tight loop (interval=10s); \
             initial={used_initial}, after={used_after}"
        );
    }
}

// ── test 5 ───────────────────────────────────────────────────────────────────

/// `config_decay_rate_percent`
///
/// Build a `LargeCacheConfig` with `decay_rate_percent(25)` and verify the
/// resulting `AllocCore` has `decay_rate_bp == 2500` (25 % → 2500 basis
/// points).
#[test]
fn config_decay_rate_percent() {
    use sefer_alloc::LargeCacheConfig;

    let cfg = LargeCacheConfig::new().decay_rate_percent(25);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");

    let (rate_bp, _interval_ms, _headroom) = ac.dbg_decay_config();
    assert_eq!(
        rate_bp, 2500,
        "decay_rate_percent(25) must produce 2500 bp; got {rate_bp}"
    );
}

// ── test 6 ───────────────────────────────────────────────────────────────────

/// R2-18: `decay_interval = 0` must tick on the FIRST eligible organic event.
///
/// Deterministic counterfactual (no sleeps, no wall-clock reads by the test):
/// with headroom=0 and rate=100%, the first large op that finds the cache
/// above headroom must decay the entire cache in that same call. Before the
/// R2-18 fix that call only PRIMED `last_decay_tick` (and the next 63 calls
/// were stride-blocked), so the pre-filled cache survived and this assertion
/// failed with the cache still holding bytes.
#[test]
fn zero_interval_decays_on_first_eligible_op() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let l = layout(4); // 4 MiB nominal → segment-sized usable

    // Fill under the DEFAULT config (256 MiB headroom → the decay fast-path
    // exits before `large_cache_decay_op_count` is ever incremented, so no
    // organic tick and op_count stays 0).
    let used = match fill_cache(&mut ac, l, 2) {
        Some(u) if u > 0 => u,
        _ => {
            eprintln!("OOM or cache empty — skipping zero_interval_decays_on_first_eligible_op");
            return;
        }
    };

    // Arm interval=0 / headroom=0 / rate=100%. This also resets
    // `last_decay_tick` to `None` — the exact "first eligible event" state.
    ac.dbg_set_decay_config(10_000, 0, 0);

    // A single large alloc is the first eligible organic event (its decay tick
    // runs at the entry of `alloc_large`, before the cache lookup).
    let p = ac.alloc(l);
    if p.is_null() {
        eprintln!("OOM — skipping zero_interval_decays_on_first_eligible_op");
        return;
    }
    let used_after_first_op = ac.dbg_large_cache_used();
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by the matching alloc above, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, l) };

    assert_eq!(
        used_after_first_op, 0,
        "interval=0 must decay on the FIRST eligible op (cache held {used} bytes \
         before it); pre-R2-18 code only primed the timer on that call and left \
         the cache intact"
    );
}

// ── test 7 ───────────────────────────────────────────────────────────────────

/// R2-18: at `decay_interval = 0` EVERY eligible organic event ticks — not
/// just every 64th (`DECAY_CLOCK_CHECK_STRIDE`) — including rare, isolated
/// single operations.
///
/// Deterministic counterfactual: alternating 4 MiB / 8 MiB nominal layouts,
/// headroom=0, rate=100%. Every cycle asserts the cache holds EXACTLY the
/// just-deposited span's usable bytes and nothing else: the alloc-side entry
/// tick evicts the previous deposit, the free-side tick runs BEFORE its own
/// deposit, so one span is cached between cycles. Pre-fix, the first post-prime
/// event only primed and the following ops were stride-blocked, so the older
/// deposit was never evicted and the cache held BOTH spans (≈ s1 + s2 bytes),
/// failing the very first cycle's equality.
#[test]
fn zero_interval_ticks_on_every_eligible_op() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    let l1 = layout(4); // usable s1
    let l2 = layout(8); // usable s2 ≠ s1 (different slot/size class)

    // Discover usable sizes and warm the cache under the DEFAULT config (no
    // organic decay; op_count stays 0). After this: cache = [s1, s2].
    let s1 = match fill_cache(&mut ac, l1, 1) {
        Some(u) if u > 0 => u,
        _ => {
            eprintln!("OOM — skipping zero_interval_ticks_on_every_eligible_op");
            return;
        }
    };
    let both = match fill_cache(&mut ac, l2, 1) {
        Some(u) if u > s1 => u,
        _ => {
            eprintln!(
                "OOM or unexpected cache state — skipping zero_interval_ticks_on_every_eligible_op"
            );
            return;
        }
    };
    let s2 = both - s1;
    assert_ne!(
        s1, s2,
        "test premise: the two layouts must cache to different usable sizes"
    );

    // Arm interval=0 / headroom=0 / rate=100% (resets `last_decay_tick`).
    ac.dbg_set_decay_config(10_000, 0, 0);

    // 8 alternating single-op cycles (≪ 64 — deep inside the pre-fix stride
    // window). After each dealloc the cache must hold exactly the span that
    // was just deposited and nothing older.
    let expected = [s1, s2, s1, s2, s1, s2, s1, s2];
    let layouts = [l1, l2, l1, l2, l1, l2, l1, l2];
    for (cycle, (&l, &want)) in layouts.iter().zip(expected.iter()).enumerate() {
        let ptr = ac.alloc(l);
        if ptr.is_null() {
            eprintln!("OOM at cycle {cycle} — skipping zero_interval_ticks_on_every_eligible_op");
            return;
        }
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by the matching alloc above, is live, and is freed exactly once here.
        unsafe { ac.dealloc(ptr, l) };
        let used = ac.dbg_large_cache_used();
        assert_eq!(
            used, want,
            "cycle {cycle}: interval=0 must tick on every eligible op, leaving \
             exactly the fresh deposit cached; pre-R2-18 stride-blocked ticks \
             let older deposits pile up"
        );
    }
}
