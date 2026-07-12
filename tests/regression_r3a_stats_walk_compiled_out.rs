//! R3-A (round3 finding N1) — `stats()`'s two hit-counter aggregators
//! (`tcache_hits_total` / `large_cache_hits_total`) used to walk up to
//! `heaps_claimed_high_water` registry slots reading each slot's counter —
//! but those counters are ONLY ever incremented under `alloc-stats`, which is
//! NOT part of `production`. In a plain `production` build the counters were
//! compile-time zero, so the walk summed zeros yet still ran on every call,
//! contradicting `stats()`'s "no segment or heap walk, safe on a hot path"
//! doc. R3-A compiles the WALK out under `not(alloc-stats)` too (returning 0
//! with no loop), making `stats()` genuinely O(1) in production as its doc
//! now promises.
//!
//! This regression pins the VALUE CONTRACT that `stats()` exposes: under
//! `production` WITHOUT `alloc-stats`, the two hit-counter fields are exactly
//! 0 even after traffic that WOULD have bumped them had `alloc-stats` been
//! compiled in. See `stats_hit_counters_are_zero_without_alloc_stats`'s doc
//! comment for an explicit statement of what this can and cannot structurally
//! prove.
//!
//! ## The matching "gate isn't accidentally inverted" check
//!
//! The counter-check — that WITH `alloc-stats` the counters DO aggregate to
//! something `> 0` after tcache/large-cache-hitting traffic — is ALREADY
//! covered by `tests/stats_reflects_activity.rs::stats_reflects_activity`'s
//! `#[cfg(… "alloc-stats")]` assertions on `tcache_hits` / `large_cache_hits`.
//! If the R3-A cfg were accidentally flipped backwards (walk under
//! `not(alloc-stats)`, return 0 under `alloc-stats`), that existing test would
//! fail under `--features "production alloc-stats"` because the counters would
//! read 0 instead of advancing. It is therefore not duplicated here.
//!
//! File-level gate: every assertion below is specific to the
//! `not(alloc-stats)` build (it asserts the counters are 0), so the whole
//! file is excluded when `alloc-stats` is compiled in.

#![cfg(all(feature = "alloc-global", not(feature = "alloc-stats")))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

/// Drive traffic that, under `alloc-stats`, would bump the tcache
/// (`tcache_hits`) and large-cache (`large_cache_hits`) hit counters: prime
/// the per-thread magazine then reuse it (tcache hits), and run an
/// alloc/free/alloc cycle on a large object (large-cache hits under
/// `alloc-decommit`). Under `not(alloc-stats)` these same code paths run but
/// the per-hit increments are absent, so every slot's counter stays 0.
fn drive_cache_hitting_traffic(a: &SeferAlloc) {
    // Small-object churn: fill the magazine, drain it, then re-allocate the
    // same class — the second wave is served from the primed magazine
    // (tcache hits) under `fastbin`.
    let small = Layout::from_size_align(48, 8).unwrap();
    let mut live = Vec::new();
    for _ in 0..1024 {
        // SAFETY: valid non-zero layout.
        let p = unsafe { a.alloc(small) };
        assert!(!p.is_null());
        live.push(p);
    }
    for p in live.drain(..) {
        // SAFETY: allocated with `small`, still live.
        unsafe { a.dealloc(p, small) };
    }
    for _ in 0..1024 {
        // SAFETY: valid layout; magazine is primed → a tcache hit under fastbin.
        let p = unsafe { a.alloc(small) };
        assert!(!p.is_null());
        // SAFETY: just allocated with `small`.
        unsafe { a.dealloc(p, small) };
    }

    // Large-object alloc/free/alloc cycle: the second alloc should reuse the
    // large_cache slot (a cache hit) under `alloc-decommit`.
    let large = Layout::from_size_align(1 << 20, 8).unwrap(); // 1 MiB
    for _ in 0..4 {
        // SAFETY: valid layout.
        let p = unsafe { a.alloc(large) };
        assert!(!p.is_null());
        // SAFETY: allocated with `large`.
        unsafe { a.dealloc(p, large) };
    }
}

/// Under `production` WITHOUT `alloc-stats`, the two hit-counter fields of
/// `stats()` are exactly 0 even after traffic that would bump them under
/// `alloc-stats`. This is the honest runtime-observable proxy for "the
/// aggregating walk is compiled out": without `alloc-stats` the per-hit
/// increment is absent, so every slot's counter is structurally 0, so the
/// (now-compiled-out) walk would sum 0 — and the value we observe must be 0.
///
/// ## What this can and cannot structurally prove
///
/// This assertion pins the VALUE CONTRACT (`stats()` exposes 0 for these
/// fields without `alloc-stats`), NOT the walk-elimination itself. A
/// hypothetical regression that re-introduced the unconditional walk (summing
/// the same always-zero counters) would STILL pass this test, because
/// `0 == 0` either way — the two cases are indistinguishable from outside the
/// process. The walk-elimination claim is therefore not unit-testable via
/// `cargo test`; it is a compile-time cost property better judged by an
/// instruction-count tool (`npm run iai`), which R3-A's verification also ran.
/// The value-based test here is the correct regression guard for what
/// `stats()` PROMISES (honest zeros), and the `alloc-stats`-gated assertions
/// in `stats_reflects_activity.rs` guard against the gate being inverted (a
/// flipped gate WOULD make these fields non-zero under `alloc-stats` and fail
/// there).
#[test]
fn stats_hit_counters_are_zero_without_alloc_stats() {
    let a = SeferAlloc::new();
    // Drive traffic that WOULD bump the hit counters under `alloc-stats`.
    drive_cache_hitting_traffic(&a);

    let s = a.stats();

    // `tcache_hits` calls `tcache_hits_total()` under `fastbin` (which
    // `production` includes); without `alloc-stats` it must be exactly 0.
    #[cfg(feature = "fastbin")]
    assert_eq!(
        s.tcache_hits, 0,
        "tcache_hits must be 0 without alloc-stats (walk compiled out, \
         increment absent) — got {}",
        s.tcache_hits,
    );

    // `large_cache_hits` calls `large_cache_hits_total()` under
    // `alloc-decommit` (which `production` includes); without `alloc-stats`
    // it must be exactly 0.
    #[cfg(feature = "alloc-decommit")]
    assert_eq!(
        s.large_cache_hits, 0,
        "large_cache_hits must be 0 without alloc-stats (walk compiled out, \
         increment absent) — got {}",
        s.large_cache_hits,
    );
}
