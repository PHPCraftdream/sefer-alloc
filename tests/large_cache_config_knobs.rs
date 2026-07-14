//! Gaps 2–6: `LargeCacheConfig` knob tests via `AllocCore::new_with_config`.
//!
//! Each test constructs an `AllocCore` with a specific config knob and verifies
//! the resolved value through the `dbg_*` test seams.

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, LargeCacheConfig};

const MIB: usize = 1024 * 1024;

// ── Gap 2: headroom_bytes ───────────────────────────────────────────────────

/// `.headroom_bytes(64 MiB)` is plumbed through to the decay config.
#[test]
fn headroom_bytes_applied() {
    let cfg = LargeCacheConfig::new().headroom_bytes(64 * MIB);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    let (_rate_bp, _interval_ms, headroom) = ac.dbg_decay_config();
    assert_eq!(
        headroom,
        64 * MIB,
        "headroom_bytes must be 64 MiB, got {headroom}"
    );
}

// ── Gap 3: decay_interval_ms ────────────────────────────────────────────────

/// `.decay_interval_ms(500)` is plumbed through to the decay config.
#[test]
fn decay_interval_ms_applied() {
    let cfg = LargeCacheConfig::new().decay_interval_ms(500);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    let (_rate_bp, interval_ms, _headroom) = ac.dbg_decay_config();
    assert_eq!(
        interval_ms, 500,
        "decay_interval_ms must be 500, got {interval_ms}"
    );
}

// ── Gap 4a: decay_rate_percent clamped from below ───────────────────────────

/// `.decay_rate_percent(0)` clamps to 1 % = 100 basis points.
#[test]
fn decay_rate_percent_clamped_low() {
    let cfg = LargeCacheConfig::new().decay_rate_percent(0);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    let (rate_bp, _interval_ms, _headroom) = ac.dbg_decay_config();
    assert_eq!(
        rate_bp, 100,
        "decay_rate_percent(0) must clamp to 1% = 100 bp, got {rate_bp}"
    );
}

// ── Gap 4b: decay_rate_percent clamped from above ───────────────────────────

/// `.decay_rate_percent(200)` clamps to 100 % = 10000 basis points.
#[test]
fn decay_rate_percent_clamped_high() {
    let cfg = LargeCacheConfig::new().decay_rate_percent(200);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    let (rate_bp, _interval_ms, _headroom) = ac.dbg_decay_config();
    assert_eq!(
        rate_bp, 10_000,
        "decay_rate_percent(200) must clamp to 100% = 10000 bp, got {rate_bp}"
    );
}

// ── Gap 5: budget_bytes(0) → cache disabled (task #136 fix) ────────────────
//
// Before task #136, `.budget_bytes(0)` was silently remapped to `None`
// (unbounded) — the opposite of what `0` intuitively means ("cache
// nothing"). #136 fixed the inversion: `Some(0)` is now stored verbatim, so
// every deposit immediately fails the budget admission check and the span
// is released to the OS instead of entering the cache. Unbounded caching is
// still available — it is simply the *default* (don't call `budget_bytes`
// at all, or see `budget_bytes_absent_is_unbounded` below).

/// `.budget_bytes(0)` now means "cache disabled" — a large span must be
/// released to the OS immediately, not cached (task #136: fixes the
/// pre-#136 inversion where `0` silently meant "unbounded").
#[test]
fn budget_bytes_zero_disables_cache() {
    let cfg = LargeCacheConfig::new().budget_bytes(0);
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");

    // Alloc + dealloc a large span; it must NOT enter the cache.
    let layout = Layout::from_size_align(4 * MIB, 8).unwrap();
    let ptr = ac.alloc(layout);
    assert!(!ptr.is_null(), "allocation must succeed");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(ptr, layout) };

    let used = ac.dbg_large_cache_used();
    assert_eq!(
        used, 0,
        "budget_bytes(0) must disable the cache: span should be released, but used={used}"
    );
}

/// Counterfactual companion: leaving `budget_bytes` unset (the default,
/// `None`) is still unbounded — a large span is cached. This is the
/// behaviour `budget_bytes_zero_disables_cache` would wrongly pass under if
/// the admission check were broken in the other direction (e.g. always
/// rejecting), so it pins down that only `Some(0)` disables the cache, not
/// every config.
#[test]
fn budget_bytes_absent_is_unbounded() {
    let cfg = LargeCacheConfig::new();
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");

    let layout = Layout::from_size_align(4 * MIB, 8).unwrap();
    let ptr = ac.alloc(layout);
    assert!(!ptr.is_null(), "allocation must succeed");
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(ptr, layout) };

    let used = ac.dbg_large_cache_used();
    assert!(
        used > 0,
        "default (unset) budget_bytes must be unbounded: span should be cached, but used={used}"
    );
}

// ── Gap 6: Default::default() == LargeCacheConfig::DEFAULT ──────────────────

/// `LargeCacheConfig::default()` and `LargeCacheConfig::DEFAULT` produce
/// identical observable behaviour (same decay config and mode).
#[test]
fn default_equals_const_default() {
    let ac_default =
        AllocCore::new_with_config(LargeCacheConfig::default()).expect("primordial (default)");
    let ac_const =
        AllocCore::new_with_config(LargeCacheConfig::DEFAULT).expect("primordial (DEFAULT)");

    let cfg_a = ac_default.dbg_decay_config();
    let cfg_b = ac_const.dbg_decay_config();
    assert_eq!(
        cfg_a, cfg_b,
        "default() and DEFAULT must produce identical decay config"
    );

    let mode_a = ac_default.dbg_large_cache_mode();
    let mode_b = ac_const.dbg_large_cache_mode();
    assert_eq!(
        mode_a, mode_b,
        "default() and DEFAULT must produce identical mode"
    );
}

// Note on Gap 7 (slot re-claim preserves first-claim config):
// This gap is intentionally not tested here. It requires thread death +
// re-claim machinery internal to HeapRegistry, which is not observable from
// integration tests without exposing additional test seams. The invariant is
// covered by code inspection and the architectural guarantee that
// `HeapRegistry::claim_with_config` only applies config on generation == 1.
