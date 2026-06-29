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

// ── Gap 5: budget_bytes(0) → unbounded ──────────────────────────────────────

/// `.budget_bytes(0)` means unbounded — a large span must be cached, not rejected.
#[test]
fn budget_bytes_zero_is_unbounded() {
    let cfg = LargeCacheConfig::new().budget_bytes(0);
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");

    // Alloc + dealloc a large span; it should enter the cache.
    let layout = Layout::from_size_align(4 * MIB, 8).unwrap();
    let ptr = ac.alloc(layout);
    assert!(!ptr.is_null(), "allocation must succeed");
    ac.dealloc(ptr, layout);

    let used = ac.dbg_large_cache_used();
    assert!(
        used > 0,
        "budget_bytes(0) must be unbounded: span should be cached, but used={used}"
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
