//! B4 У1b cached-handle transport (ADR `adr_v2_r4.md` §4.1 (в), chunk U1b) —
//! Miri Stacked-Borrows/TREE-BORROWS acceptance for the ONE new `unsafe`: the
//! dereference of the cached `*const ClassLeaves` handle stored in the
//! heap-private `own_cache` entry (`SegmentTable::cached_class_leaves` →
//! `AllocCore::small_class_authority`).
//!
//! What Miri validates here (run by CI; never locally per project rules):
//! 1. The handle captured by the own_cache fill (`ptr::from_ref` of the live
//!    route's `ClassLeaves`) stays valid for reads across many alloc/free
//!    round-trips — no stack-borrow / tree-borrow violation on the hot read
//!    `(*leaves).encoded(granule)`.
//! 2. Segment churn (alloc → free → re-alloc across distinct classes) does
//!    not produce a stale-handle deref: teardown order (`routes.remove` →
//!    `hash_remove` → `own_cache_clear`) keeps the entry key/handle in
//!    lockstep, and any fallback goes through the safe authoritative lookup.
//! 3. Wrong-class and interior frees still REJECT through the cached-handle
//!    path (the verdict logic itself is pinned by the T1..T9 regression
//!    suite; here we only need the memory accesses to be UB-free).
//!
//! Scope note: `own_cache` is heap-private owner-thread state accessed only
//! through `&mut self`; there is no shared mutable access to the entry, so
//! Loom is NOT applicable to the transport itself (justification in the U1b
//! gate report). The atomics inside `ClassLeaves` keep their own publication
//! harness (`class_leaves_publication_miri.rs`).
#![cfg(all(
    miri,
    feature = "production",
    feature = "small-class-authority",
    feature = "internals"
))]

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::AllocCore;
use std::alloc::Layout;

#[test]
fn cached_handle_deref_is_ub_free_across_class_churn() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();

    // Distinct classes ⇒ distinct leaves; repeat churn so the own_cache entry
    // is filled once and hit on every free (the cached-handle fast path).
    let layouts = [
        Layout::from_size_align(16, 16).unwrap(),
        Layout::from_size_align(64, 16).unwrap(),
        Layout::from_size_align(256, 16).unwrap(),
        Layout::from_size_align(1024, 16).unwrap(),
    ];
    for round in 0..8 {
        for l in layouts {
            let p = heap.alloc(l);
            assert!(!p.is_null());
            // SAFETY: valid routed own Small free of a live block.
            unsafe { heap.dealloc(p, l) };
            // Legit free must be accepted AND re-issued (LIFO magazine top) —
            // proves the cached-handle read matched, not just ran.
            let a = heap.alloc(l);
            assert_eq!(a, p, "round {round}: legit free not re-issued");
            // SAFETY: valid free of the re-issued block.
            unsafe { heap.dealloc(a, l) };
            // Interior pointer (16 B into the block): only for blocks ≥ 64 B
            // (a 16 B block has no interior granule; `a+16` there could be a
            // legitimately issued neighbour of the SAME class and would be
            // ACCEPTED — not the scenario under test). Inside the block the
            // non-start granule's class byte is 0 ⇒ ADR case (б) REJECT via
            // the cached handle — a no-op (block stays live, re-issuable).
            if l.size() >= 64 {
                // SAFETY: pointer arithmetic stays inside the live allocation.
                let interior = unsafe { a.add(16) };
                // SAFETY: probing free; the authority must reject it as a no-op.
                unsafe { heap.dealloc(interior, l) };
                let again = heap.alloc(l);
                assert_eq!(again, a, "interior free was not rejected (no-op broken)");
            }
            // SAFETY: final free of the live block.
            unsafe { heap.dealloc(a, l) };
        }
    }
    drop(lease);
}
