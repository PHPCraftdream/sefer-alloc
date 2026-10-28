//! G1-K7: a recycled registry slot keeps the depot-overflow budget it was
//! created with (first claim wins), counts the conflicting request, and its
//! overflow — filled before the recycle — works again after it. Own binary:
//! it deliberately mixes two configs; one test, so the second claim pops the
//! slot the first one recycled.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use sefer_alloc::alloc_core::LargeCacheConfig;
use sefer_alloc::registry::{bootstrap, config_conflicts_total, HeapCore, HeapRegistry};
use std::alloc::Layout;

const BUDGET: usize = 64 * 1024;
const PARKS: usize = 1;
const BYTES: usize = 0;

fn cycle(heap: &mut HeapCore) {
    let l = Layout::from_size_align(64, 8).unwrap();
    let v: Vec<_> = (0..16 * (1 + 8 + 4)).map(|_| heap.alloc(l)).collect();
    assert!(v.iter().all(|p| !p.is_null()));
    for &p in &v {
        // SAFETY: each pointer is a distinct live allocation of 64 B.
        unsafe { heap.dealloc(p, l) };
    }
}

#[test]
fn a_recycled_heap_keeps_its_budget_and_counts_the_conflict() {
    let _ = bootstrap::ensure();
    let first = LargeCacheConfig::new().depot_budget_bytes(BUDGET);
    let mut lease = HeapRegistry::dbg_claim_lease_with_config(first).expect("claim");
    let slot = lease.slot_index();
    assert_eq!(lease.core().dbg_depot_budget(), (BUDGET, BUDGET));
    let p0 = lease.core().dbg_depot_overflow()[PARKS];
    cycle(lease.core());
    assert_eq!(
        lease.core().dbg_depot_overflow()[PARKS] - p0,
        4,
        "overflow filled"
    );
    drop(lease);
    let conflicts = config_conflicts_total();
    let mut other =
        HeapRegistry::dbg_claim_lease_with_config(LargeCacheConfig::new().depot_budget_bytes(0))
            .expect("claim");
    assert_eq!(other.slot_index(), slot, "the recycled slot is reused");
    assert_eq!(
        other.core().dbg_depot_budget(),
        (BUDGET, BUDGET),
        "first claim wins"
    );
    assert_eq!(config_conflicts_total(), conflicts + 1, "conflict counted");
    let s = other.core().dbg_depot_overflow();
    assert_eq!(s[BYTES], 0, "recycle drained the overflow");
    cycle(other.core());
    assert_eq!(
        other.core().dbg_depot_overflow()[PARKS] - s[PARKS],
        4,
        "reused after recycle"
    );
    other.core().dbg_flush_all();
    drop(other);
}
