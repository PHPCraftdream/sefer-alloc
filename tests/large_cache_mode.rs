//! `LargeCacheMode` configuration tests.
//!
//! `LargeCacheMode` (feature = `alloc-decommit`) carries a single variant,
//! `Lazy`. Earlier pre-1.0 revisions also had `Background`/`Both` variants
//! that were never implemented — they silently degraded to `Lazy`, then
//! briefly panicked at heap-materialisation time. Round3 (cq#3 / N2, решение
//! №2) removed those variants from the enum entirely ("make invalid states
//! unrepresentable"): there is no longer a runtime rejection to test,
//! because the type system makes the removed variants unnameable —
//! `LargeCacheMode::Background` does not resolve as a path and any reference
//! to it fails to compile (E0599). That compile-time impossibility *is* the
//! regression guard for round3 cq#3; no trybuild-style compile-fail test is
//! added here (the no-doctest rule of `CLAUDE.md` rules out the doctest
//! shape, and the guard is self-evident from the enum definition).
//!
//! What these tests do cover: that `Lazy` (the one real mode) round-trips
//! through `.mode()`, is the default, and is stored correctly in the shard —
//! via the `dbg_large_cache_mode()` test seam.
//!
//! Gated on `alloc-core` + `alloc-decommit` (same gate as the cache itself).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeCacheMode};

// ── test 1 ───────────────────────────────────────────────────────────────────

/// `Lazy` — the only mode — round-trips through `.mode()` and
/// `dbg_large_cache_mode()`.
#[test]
fn lazy_mode_round_trips() {
    let cfg = LargeCacheConfig::new().mode(LargeCacheMode::Lazy);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(
        ac.dbg_large_cache_mode(),
        LargeCacheMode::Lazy,
        "mode(Lazy) must be stored as Lazy"
    );
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
    // `Lazy` is the only constructible variant, but the "not garbage memory"
    // guard remains meaningful.
    assert!(
        matches!(mode, LargeCacheMode::Lazy),
        "dbg_large_cache_mode() must return Lazy for the default config; got {mode:?}"
    );
}
