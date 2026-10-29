//! G1-K7: at the budget ceiling the 16 B class parks floor + overflow up to
//! exactly `u16::MAX` magazines without wrapping `PerClass::parked`, and every
//! block comes back once. Own binary: its own config.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use sefer_alloc::alloc_core::LargeCacheConfig;
use sefer_alloc::registry::{bootstrap, HeapRegistry};
use std::alloc::Layout;
use std::collections::HashSet;

#[test]
fn ceiling_budget_parks_u16_max_magazines_of_16_b() {
    let _ = bootstrap::ensure();
    let cfg = LargeCacheConfig::new().depot_budget_bytes(usize::MAX);
    let mut lease = HeapRegistry::dbg_claim_lease_with_config(cfg).expect("claim");
    let heap = lease.core();
    let (budget, _) = heap.dbg_depot_budget();
    let cap = budget / 256;
    assert_eq!(8 + cap, u16::MAX as usize, "floor + ring fill u16 exactly");
    let layout = Layout::from_size_align(16, 8).unwrap();
    // Live magazine + floor + full ring + one magazine that must be refused.
    let n = 16 * (1 + 8 + cap + 1);
    let blocks: Vec<_> = (0..n).map(|_| heap.alloc(layout)).collect();
    assert!(blocks.iter().all(|p| !p.is_null()));
    for &p in &blocks {
        // SAFETY: distinct live allocations, freed once.
        unsafe { heap.dealloc(p, layout) };
    }
    assert_eq!(heap.dbg_depot_len(0), u16::MAX as usize);
    assert_eq!(heap.dbg_depot_overflow()[0] as usize, budget);
    let again: Vec<_> = (0..n).map(|_| heap.alloc(layout)).collect();
    let got: HashSet<usize> = again.iter().map(|p| *p as usize).collect();
    let freed: HashSet<usize> = blocks.iter().map(|p| *p as usize).collect();
    assert_eq!(got.len(), n);
    assert_eq!(got, freed);
    for &p in &again {
        // SAFETY: as above.
        unsafe { heap.dealloc(p, layout) };
    }
    heap.dbg_flush_all();
    assert_eq!(heap.dbg_depot_overflow()[0], 0);
    drop(lease);
}
