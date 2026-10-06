//! R12-02: every magazine-issued block must satisfy `mask(base) == canonical root`, the
//! invariant `clear_magazine_on_issue` relies on after dropping the table lookup. The debug-only
//! `debug_assert_eq!` in that function checks it on every hit; this test makes sure the hits
//! really happen (`tcache_hits`, an `alloc-stats` counter) while >= 24 Small segments are live.

#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-stats"
))]

use std::alloc::Layout;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn magazine_issue_uses_canonical_segment_root() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let layout = Layout::from_size_align(250_000, 16).unwrap();
    let mut live = Vec::new();

    // A near-maximum small class yields only a handful of blocks per 4 MiB
    // segment. Keep allocations live so each refill must claim more segments.
    for i in 0..500 {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null(), "allocation {i} failed");
        live.push(ptr);
    }
    let (small, _, _) = heap.dbg_active_kind_census();
    assert!(
        small >= 24,
        "expected at least 24 live Small segments, got {small}"
    );

    for ptr in &live {
        unsafe { heap.dealloc(*ptr, layout) };
    }

    let hit_layout = Layout::from_size_align(64, 8).unwrap();
    let initial_count = heap.tcache_hits();
    for _ in 0..16 {
        let ptr = heap.alloc(hit_layout);
        assert!(!ptr.is_null());
        unsafe { heap.dealloc(ptr, hit_layout) };
    }
    assert!(
        heap.tcache_hits() - initial_count >= 8,
        "expected repeated magazine hits"
    );
    drop(lease);
}
