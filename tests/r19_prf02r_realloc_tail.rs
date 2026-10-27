// The last cfg term is the predicate of `HeapCore::dbg_promotion_compiled`: the promotion
// leg exists only there; under --all-features (exact-span-large + numa-aware) it does not.
// The gate verifier tokenizes the attribute text, so the note stays outside it.
#![cfg(all(
    feature = "internals",
    feature = "bench-internals",
    feature = "fastbin",
    feature = "alloc-global",
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]

use core::alloc::Layout;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

fn total() -> u64 {
    AllocCore::dbg_contains_base_tier1_hits() + AllocCore::dbg_contains_base_tier1_misses()
}

// Run this binary alone, --test-threads=1. No installed Sefer global allocator.
// All measured windows exclude bootstrap, setup, diagnostics, cleanup and prints.
// A reintroduced canonical_block_of in the free body adds exactly one to each
// move/promote window, but none to in-place: this is the negative control.
#[test]
fn realloc_tail_and_inplace_activation() {
    let arm = std::env::var("R19_PRF02R_ARM").unwrap_or_else(|_| "B".to_owned());
    assert!(arm == "A" || arm == "B");
    let tail = if arm == "A" { 1 } else { 0 };
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let small = Layout::from_size_align(16, 8).unwrap();
    let mut blocks = [core::ptr::null_mut(); 64];
    for p in &mut blocks {
        *p = heap.alloc(small);
        assert!(!p.is_null());
    }
    assert_eq!(heap.dbg_tcache_count(0), 0);

    let before = total();
    // SAFETY: uniquely owned live block, exact old layout, same-size resize.
    let inplace = unsafe { heap.realloc(blocks[0], small, 16) };
    let inplace_total = total() - before;
    assert_eq!(inplace, blocks[0]);
    assert_eq!(
        inplace_total, 1,
        "in-place must have ingress only, no free tail"
    );
    assert_eq!(heap.dbg_tcache_count(0), 0);

    // Prime destination magazine so move allocation is a hit. P4 hoists
    // stamping into refill; a hit performs zero mutable ownership lookups.
    let next = Layout::from_size_align(32, 8).unwrap();
    let priming = heap.alloc(next);
    assert!(!priming.is_null());
    // SAFETY: priming is uniquely live with the matching layout.
    unsafe { heap.dealloc(priming, next) };
    let before = total();
    let allocation = heap.alloc(next);
    let alloc_total = total() - before;
    assert_eq!(allocation, priming);
    assert_eq!(alloc_total, 0, "P4 magazine hit must not stamp or resolve");
    // SAFETY: allocation was issued once and remains live.
    unsafe { heap.dealloc(allocation, next) };
    let promotions = AllocCore::dbg_promotion_count();
    let before = total();
    // SAFETY: distinct live old block; valid grow layout, move copies 16 bytes.
    let moved = unsafe { heap.realloc(blocks[1], small, 32) };
    let move_total = total() - before;
    assert!(!moved.is_null());
    assert_ne!(moved, blocks[1]);
    assert_eq!(moved, priming, "must use the primed alloc hit");
    assert_eq!(AllocCore::dbg_promotion_count(), promotions);
    assert_eq!(heap.dbg_tcache_count(0), 1, "old block parked exactly once");
    assert_eq!(move_total.checked_sub(1 + alloc_total), Some(tail));

    // Promotion alloc_large performs no mutable ownership lookup: the explicit
    // owner stamp that used to count here was removed by R6-T01 (1730c1ca), so
    // the window holds the realloc ingress only. Old class-0 cache is below
    // overflow/budget thresholds.
    assert!(sefer_alloc::registry::HeapCore::dbg_promotion_compiled());
    let promotions = AllocCore::dbg_promotion_count();
    let before = total();
    // SAFETY: distinct live old block and exact layout; valid 256 KiB grow.
    let promoted = unsafe { heap.realloc(blocks[2], small, 256 * 1024) };
    let promote_total = total() - before;
    assert!(!promoted.is_null());
    assert_ne!(promoted, blocks[2]);
    assert_eq!(heap.dbg_kind_at_tag(promoted), 2, "physical Large required");
    assert_eq!(AllocCore::dbg_promotion_count() - promotions, 1);
    assert_eq!(heap.dbg_tcache_count(0), 2);
    assert_eq!(promote_total.checked_sub(1), Some(tail));
    let large = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let large_ptr = heap.alloc(large);
    assert!(!large_ptr.is_null());
    assert_eq!(heap.dbg_kind_at_tag(large_ptr), 2);
    let before = total();
    // SAFETY: uniquely live owned Large block, freed once with its exact layout.
    unsafe { heap.dealloc(large_ptr, large) };
    let large_total = total() - before;
    // One lookup: the routing ingress (`owned_block_of`); the Large substrate
    // free reuses that `OwnedBlock` and no longer re-resolves the root.
    assert_eq!(large_total, 1 + tail, "ingress only, plus A body lookup");
    println!("arm={arm} large_free={large_total}");
    println!("arm={arm} inplace={inplace_total} alloc={alloc_total} move={move_total} promote={promote_total} tail={tail}");
    // Deliberately retain LIVE lease and allocations, matching activation fixture;
    // teardown must not contaminate windows or introduce unrelated reclaim work.
    core::mem::forget(lease);
}
