//! R6-T11 — correctness backstop for `SegmentTable`'s pair-layout own_cache.
//!
//! The Tier-1 own_cache is now one array of `(key, root)` pairs with an
//! EMPTY sentinel key of `usize::MAX`, so a hit is a single key comparison
//! with NO separate `!is_null()` root check. That removal makes the sentinel
//! choice load-bearing: an EMPTY slot must have a key that can never equal a
//! real lookup key. `usize::MAX` is not SEGMENT-aligned, so it can match
//! neither a real SEGMENT-aligned key nor address 0 (null) — the previous
//! `0`-initialised key design could, if the `is_null` guard were dropped.
//!
//! These tests drive the REAL `SegmentTable` cache through the
//! `SegmentHashHarness` test seam (synthetic SEGMENT-aligned pointer values,
//! never dereferenced). They assert the three properties that matter: an
//! EMPTY cache never false-hits (including for a null key), unregistered or
//! evicted keys miss after clear/eviction, and a hit returns the STORED
//! canonical root, not the lookup key.
//!
//! Per the short-scenario policy (`CLAUDE.md`): 3 focused cases.
#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::alloc_core::SegmentHashHarness;

/// A fresh cache must not report a hit for a NULL lookup key. Counterfactual:
/// with the previous design's sentinel key `0` — and the single-comparison
/// hit the pair layout enables (no `is_null` guard) — a null key whose cache
/// index hashes to 0 would read a slot whose sentinel key is `0` and report a
/// FALSE hit (`Some(_)`/`true`) for a base that was never registered. With
/// `usize::MAX` the empty slot can never match any real SEGMENT-aligned key
/// nor address 0.
#[test]
fn empty_cache_no_false_hit_for_null_key() {
    let mut harness = SegmentHashHarness::new();
    assert!(harness.canonical(core::ptr::null_mut()).is_none());
    assert!(!harness.contains_cached(core::ptr::null_mut()));
}

/// An unregistered SEGMENT-aligned key must miss, and a removed key must
/// keep missing after its cache slot is cleared. Counterfactual: if
/// `own_cache_clear` left the OLD key in place, a subsequent probe of that
/// key would be a stale hit routing an unregistered base as own.
///
/// `base_for_index` yields DISTINCT values for distinct indices (each value
/// encodes its hash index; see the harness), so `other` (index 7) can never
/// collide with the inserted base (index 2).
#[test]
fn unregistered_segment_aligned_key_misses() {
    let mut harness = SegmentHashHarness::new();
    let other = SegmentHashHarness::base_for_index(7);
    let inserted_base = SegmentHashHarness::base_for_index(2);
    harness.insert(inserted_base);

    // The never-registered key misses through both lookup paths...
    assert!(harness.canonical(other).is_none());
    assert!(!harness.contains_cached(other));
    // ...while the registered base is found via the hash path, and a cached
    // lookup fills its Tier-1 slot.
    assert!(harness.contains(inserted_base));
    assert!(harness.contains_cached(inserted_base));
    assert_eq!(harness.canonical(inserted_base), Some(inserted_base));

    // After removal (hash delete + own_cache_clear) the evicted key is a
    // MISS, not a stale hit, through both lookup paths.
    harness.remove(inserted_base);
    assert!(!harness.contains_cached(inserted_base));
    assert!(harness.canonical(inserted_base).is_none());
}

/// A hit returns the STORED canonical root, not the lookup key. `canonical`
/// is read-only (hash path); `contains_cached` fills the slot, so the last
/// `canonical` read goes through the cache HIT path.
///
/// Indices 3 and 4 give distinct SEGMENT-aligned values (see
/// `base_for_index`), so `key` and `root` cannot be confused.
#[test]
fn cache_hit_returns_registered_root() {
    let mut harness = SegmentHashHarness::new();
    let key = SegmentHashHarness::base_for_index(3);
    let root = SegmentHashHarness::base_for_index(4);
    harness.insert_root_for_key(key, root);

    // Hash path, then fill, then cache hit.
    assert_eq!(harness.canonical(key), Some(root));
    assert!(harness.contains_cached(key));
    // The hit is pointer-equal to the STORED root, proving the cache returns
    // the registered canonical entry rather than the caller's lookup key.
    assert!(core::ptr::eq(harness.canonical(key).unwrap(), root));
}
