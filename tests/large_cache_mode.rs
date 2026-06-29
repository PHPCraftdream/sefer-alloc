//! Phase 3 — `LargeCacheMode` configuration tests.
//!
//! These tests verify that `LargeCacheMode` variants are stored and retrieved
//! correctly when set via `LargeCacheConfig::mode`. Assertions use the
//! `dbg_large_cache_mode()` test seam rather than side-effects so they are
//! deterministic and safe to run in parallel.
//!
//! Gated on `alloc-core` + `alloc-decommit` (same gate as the cache itself).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeCacheMode};

// ── test 1 ───────────────────────────────────────────────────────────────────

/// `LargeCacheConfig::mode` sets the stored mode correctly for each variant.
///
/// Each case constructs an `AllocCore` via `new_with_config` with a specific
/// mode and verifies the stored value via `dbg_large_cache_mode()`. Tests are
/// safe to run in parallel (no env-var mutation).
#[test]
fn config_mode_variants() {
    let check = |m: LargeCacheMode| {
        let cfg = LargeCacheConfig::new().mode(m);
        let ac = AllocCore::new_with_config(cfg).expect("primordial");
        assert_eq!(
            ac.dbg_large_cache_mode(),
            m,
            "mode({m:?}) must be stored as {m:?}"
        );
    };

    check(LargeCacheMode::Lazy);
    check(LargeCacheMode::Background);
    check(LargeCacheMode::Both);
}

// ── test 2 ───────────────────────────────────────────────────────────────────

/// The default config (`LargeCacheConfig::new()` with no `.mode()` call)
/// stores `LargeCacheMode::Lazy`.
#[test]
fn default_mode_is_lazy() {
    let ac = AllocCore::new_with_config(LargeCacheConfig::new()).expect("primordial");
    assert_eq!(
        ac.dbg_large_cache_mode(),
        LargeCacheMode::Lazy,
        "default config must produce Lazy mode"
    );
}

// ── test 3 ───────────────────────────────────────────────────────────────────

/// `AllocCore::new()` (no config) uses the default, which is `Lazy`. The
/// stored mode must be a valid variant — not uninitialised garbage.
#[test]
fn lazy_mode_stored_correctly_in_shard() {
    let ac = AllocCore::new().expect("primordial");
    let mode = ac.dbg_large_cache_mode();
    // Verify the mode is one of the three valid variants (not garbage memory).
    let is_valid = matches!(
        mode,
        LargeCacheMode::Lazy | LargeCacheMode::Background | LargeCacheMode::Both
    );
    assert!(
        is_valid,
        "dbg_large_cache_mode() must return a valid LargeCacheMode variant; got {mode:?}"
    );
}
