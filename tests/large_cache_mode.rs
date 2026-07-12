//! Phase 3 / T5 (cleanup#6) — `LargeCacheMode` configuration tests.
//!
//! These tests verify two things:
//! 1. `LargeCacheMode::Lazy` is stored and retrieved correctly when set via
//!    `LargeCacheConfig::mode` (the one mode with implemented behaviour).
//! 2. `LargeCacheMode::Background` / `LargeCacheMode::Both` — previously a
//!    *silent* no-op (stored, then never branched on, degrading to `Lazy`) —
//!    now panic *loudly* at heap-materialisation time
//!    (`AllocCore::new_with_config`), per the resolution-time validation
//!    contract of the `LargeCacheConfig` builder. Before T5 these
//!    `should_panic` tests were RED (no panic fired); after T5 they are GREEN.
//!
//! Assertions use the `dbg_large_cache_mode()` test seam for the `Lazy`
//! round-trip, and `#[should_panic]` for the rejection. All deterministic and
//! safe to run in parallel.
//!
//! Gated on `alloc-core` + `alloc-decommit` (same gate as the cache itself).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use sefer_alloc::{AllocCore, LargeCacheConfig, LargeCacheMode};

// ── test 1 ───────────────────────────────────────────────────────────────────

/// `Lazy` — the only implemented mode — round-trips through `.mode()` and
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

/// `Background` is reserved for an unimplemented scavenger. Materialising a
/// heap with it must panic (not silently degrade to `Lazy`).
///
/// RED before T5: `new_with_config` returned `Some` storing `Background`
/// silently → this `should_panic` test failed ("test did not panic").
/// GREEN after T5: `new_with_config` panics at resolution time.
#[test]
#[should_panic(expected = "not yet implemented")]
fn background_mode_panics_at_materialisation() {
    let cfg = LargeCacheConfig::new().mode(LargeCacheMode::Background);
    // Panics inside `new_with_config` before `.expect` is reached.
    let _ = AllocCore::new_with_config(cfg).expect("primordial");
}

// ── test 3 ───────────────────────────────────────────────────────────────────

/// `Both` (alias for `Background`) is likewise rejected loudly.
///
/// RED before T5 / GREEN after T5 — same rationale as
/// `background_mode_panics_at_materialisation`.
#[test]
#[should_panic(expected = "not yet implemented")]
fn both_mode_panics_at_materialisation() {
    let cfg = LargeCacheConfig::new().mode(LargeCacheMode::Both);
    let _ = AllocCore::new_with_config(cfg).expect("primordial");
}

// ── test 4 ───────────────────────────────────────────────────────────────────

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

// ── test 5 ───────────────────────────────────────────────────────────────────

/// `AllocCore::new()` (no config) uses the default, which is `Lazy`. The
/// stored mode must be a valid variant — not uninitialised garbage.
#[test]
fn lazy_mode_stored_correctly_in_shard() {
    let ac = AllocCore::new().expect("primordial");
    let mode = ac.dbg_large_cache_mode();
    // After T5 only `Lazy` can ever be materialised (Background/Both panic),
    // but the "not garbage memory" guard remains meaningful.
    assert!(
        matches!(mode, LargeCacheMode::Lazy),
        "dbg_large_cache_mode() must return Lazy for the default config; got {mode:?}"
    );
}
