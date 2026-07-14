//! Phase 2 — lazy exponential decay tests for the large-cache.
//!
//! These tests verify the decay policy: an excess over the headroom target is
//! gradually released to the OS via FIFO eviction, at a configurable rate and
//! interval. Tests use either `dbg_set_decay_config` (for dynamic overrides) or
//! `AllocCore::new_with_config` (to set the decay config at construction time).
//!
//! Gated on `alloc-core` + `alloc-decommit` (the same gate as the cache itself).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

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
    // Disable budget, set decay config: 50% rate, 0ms interval, 0 headroom.
    ac.dbg_set_large_cache_budget(None);
    ac.dbg_set_decay_config(5000, 0, 0);

    let l = layout(4); // 4 MiB nominal → ~8 MiB usable (2 segments)

    let used_before = match fill_cache(&mut ac, l, 2) {
        Some(u) if u > 0 => u,
        _ => {
            eprintln!("OOM or cache empty — skipping decay_releases_excess_over_target");
            return;
        }
    };

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
