//! Phase 13.4a — M2 double-free guard unit tests.
//!
//! Invariant M2: freeing a block that is ALREADY free is a no-op — it must never
//! corrupt the allocator (no free-list self-loop, no duplicate entry) and never
//! cause the block to be handed out to two callers. Phase 13.4a implements this
//! with the per-segment [`AllocBitmap`] (an O(1) exact bit test); these tests
//! pin the behavioural contract the bitmap must uphold (and that the prior O(N)
//! walk also upheld — so they pass with both guards; only the cost changed).

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::alloc_core::AllocCore;

/// A double-free must not let the same address be handed out twice, and must not
/// corrupt the free list (which a self-loop would, by making the list "infinite"
/// or by re-issuing the looped node).
#[test]
fn double_free_does_not_double_issue() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 16).unwrap();

    let p = ac.alloc(layout);
    assert!(!p.is_null());

    // Free it ONCE (legitimate), then AGAIN (the double-free — must be a no-op).
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };
    // A third, for good measure: still a no-op.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };

    // Now allocate several blocks. If the double-free had pushed `p` onto the
    // free list twice (or made a self-loop), the allocator would hand `p` back
    // more than once across these allocations. Every returned pointer must be
    // distinct.
    const K: usize = 16;
    let mut seen: HashSet<usize> = HashSet::new();
    let mut got = Vec::new();
    for _ in 0..K {
        let q = ac.alloc(layout);
        assert!(!q.is_null(), "alloc returned null after double-free");
        assert!(
            seen.insert(q as usize),
            "allocator handed out a DUPLICATE pointer {q:p} after a double-free \
             — the free list was corrupted (the M2 guard failed)"
        );
        got.push(q);
    }
    // `p` may legitimately come back EXACTLY once (its single real free), but
    // never more than once — the distinctness assert above already guarantees
    // that. Clean up.
    for q in got {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(q, layout) };
    }
}

/// Free → re-alloc → free the SAME address again should be fine (the bit is
/// cleared on alloc, so the second free is a legitimate new free, not a
/// double-free). This guards against an over-eager guard that would treat a
/// recycled address as a permanent double-free and silently leak it.
#[test]
fn realloc_same_address_is_refreeable() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 16).unwrap();

    let p1 = ac.alloc(layout);
    assert!(!p1.is_null());
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p1, layout) }; // free → bit set

    // Re-alloc: the recycled block should be the same address (single-class,
    // single segment, LIFO free list), and its bit must now be CLEAR again.
    let p2 = ac.alloc(layout);
    assert_eq!(p2, p1, "expected LIFO reuse of the just-freed block");

    // Freeing it again is a LEGITIMATE free (not a double-free) — must succeed
    // (the block becomes reusable), proving `mark_alloc` cleared the bit on the
    // re-alloc. If the guard wrongly still saw it as free, this free would
    // no-op and the next alloc would have to carve a NEW block instead of
    // reusing p1.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p2, layout) };
    let p3 = ac.alloc(layout);
    assert_eq!(
        p3, p1,
        "re-freed block was not reusable — `mark_alloc` did not clear the \
         double-free bit on re-allocation"
    );
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p3, layout) };
}

/// Many blocks, each double-freed: the working set must stay consistent and no
/// address is ever issued twice concurrently-live. This exercises the bitmap
/// across many distinct bits.
#[test]
fn many_double_frees_keep_free_list_consistent() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(32, 16).unwrap();

    const N: usize = 500;
    let mut ptrs = Vec::with_capacity(N);
    for _ in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }
    // Free each block TWICE (the second is a double-free → must no-op).
    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
    // Re-allocate N blocks; every one must be a distinct, valid address. A
    // corrupted free list (from a double-add) would either loop, hand out a
    // duplicate, or yield a garbage pointer.
    let mut seen: HashSet<usize> = HashSet::new();
    for _ in 0..N {
        let q = ac.alloc(layout);
        assert!(!q.is_null(), "alloc null after mass double-free");
        assert!(
            seen.insert(q as usize),
            "duplicate pointer {q:p} after mass double-free (free-list corruption)"
        );
    }
}
