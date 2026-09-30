//! Budget-infeasible deposits must be rejected before the lazy sidecar is
//! materialised. The base-eight counterfactual holds eight equal spans live
//! until deposit, then uses one request larger than their actual used bytes.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended",
    feature = "internals"
))]

#[path = "support/r6_bounded_large_cache.rs"]
mod bounded;

use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout};

#[test]
fn zero_budget_never_materialises_extension_even_past_base_eight() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(Some(0));
    let bytes = bounded::large_request();
    let live = bounded::allocate_live(&mut ac, &[bytes; 9]);
    for (p, l) in live {
        // SAFETY: p is live from allocate_live and l is its matching layout.
        unsafe { ac.dealloc(p, l) };
        assert_eq!(ac.dbg_large_cache_used(), 0);
        assert!(!ac.dbg_large_cache_extension_materialised());
    }
    assert_eq!(ac.dbg_large_cache_total_slots(), 8);
}

#[test]
fn budget_infeasible_deposit_after_base_eight_full_never_materialises_extension() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);
    let bytes = bounded::large_request();
    let live = bounded::allocate_live(&mut ac, &[bytes; 8]);
    bounded::deposit_all(&mut ac, live);
    assert_eq!(bounded::occupied(&ac), (8, 0));
    assert!(!ac.dbg_large_cache_extension_materialised());

    let tight_budget = ac.dbg_large_cache_used();
    ac.dbg_set_large_cache_budget(Some(tight_budget));
    let ninth = tight_budget + SegmentLayout::PAGE;
    assert!(ninth > tight_budget);
    let live = bounded::allocate_live(&mut ac, &[ninth]);
    bounded::deposit_all(&mut ac, live);

    assert!(!ac.dbg_large_cache_extension_materialised());
    assert_eq!(bounded::occupied(&ac), (8, 0));
    assert_eq!(ac.dbg_large_cache_used(), tight_budget);
    bounded::assert_used(&ac);
}

#[test]
fn budget_smaller_than_every_span_never_materialises_extension() {
    let mut ac = AllocCore::new().expect("primordial");
    let bytes = bounded::large_request();
    ac.dbg_set_large_cache_budget(Some(bytes - 1));
    let live = bounded::allocate_live(&mut ac, &[bytes; 9]);
    for (p, l) in live {
        // SAFETY: p is live from allocate_live and l is its matching layout.
        unsafe { ac.dealloc(p, l) };
        assert_eq!(ac.dbg_large_cache_used(), 0);
        assert!(!ac.dbg_large_cache_extension_materialised());
    }
}

#[test]
fn effectively_unbounded_budget_still_materialises_extension_on_overflow() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(Some(usize::MAX));
    let live = bounded::allocate_live(&mut ac, &[bounded::large_request(); 9]);
    bounded::deposit_all(&mut ac, live);
    assert!(ac.dbg_large_cache_extension_materialised());
    assert_eq!(bounded::occupied(&ac), (8, 1));
}

#[test]
fn default_config_resolves_finite_budget_when_extension_compiled_in() {
    let ac = AllocCore::new().expect("primordial");
    assert_eq!(ac.dbg_large_cache_budget(), Some(256 * 1024 * 1024));
}

#[test]
fn explicit_budget_bytes_overrides_the_extended_default() {
    let cfg = LargeCacheConfig::new().budget_bytes(usize::MAX);
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(ac.dbg_large_cache_budget(), Some(usize::MAX));

    let cfg_small = LargeCacheConfig::new().budget_bytes(4096);
    let ac_small = AllocCore::new_with_config(cfg_small).expect("primordial");
    assert_eq!(ac_small.dbg_large_cache_budget(), Some(4096));
}
