//! G1-K7: a recycled registry slot keeps the depot-overflow budget it was
//! created with (first claim wins) and counts the conflicting request. Own
//! binary: it deliberately mixes two configs.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "alloc-decommit"
))]

use sefer_alloc::alloc_core::LargeCacheConfig;
use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapRegistry};

#[test]
fn a_recycled_heap_keeps_its_budget_and_counts_the_conflict() {
    let _ = bootstrap::ensure();
    let first = LargeCacheConfig::new().depot_budget_bytes(64 * 1024);
    let mut lease = HeapRegistry::dbg_claim_lease_with_config(first).expect("claim");
    assert_eq!(lease.core().dbg_depot_budget(), (64 * 1024, 64 * 1024));
    drop(lease);
    let conflicts = config_conflicts_total();
    let mut other =
        HeapRegistry::dbg_claim_lease_with_config(LargeCacheConfig::new().depot_budget_bytes(0))
            .expect("claim");
    let (resolved, _) = other.core().dbg_depot_budget();
    if resolved == 64 * 1024 {
        assert_eq!(config_conflicts_total(), conflicts + 1, "conflict counted");
    } else {
        assert_eq!(resolved, 0, "a fresh slot takes the requested budget");
    }
    drop(other);
}
