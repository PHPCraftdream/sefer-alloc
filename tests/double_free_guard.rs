//! Phase 13.4a — M2 double-free guard unit tests.
//!
//! Invariant M2: freeing a block that is ALREADY free is a no-op — it must never
//! corrupt the allocator (no free-list self-loop, no duplicate entry) and never
//! cause the block to be handed out to two callers. Phase 13.4a implements this
//! with the per-segment [`AllocBitmap`] (an O(1) exact bit test); these tests
//! pin the behavioural contract the bitmap must uphold (and that the prior O(N)
//! walk also upheld — so they pass with both guards; only the cost changed).

#![cfg(all(feature = "alloc-core", feature = "internals"))]

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

/// F2 (review, B4 state-byte): the substrate double-free guard must reject a
/// FREE state byte REGARDLESS of the block's class field. The base rejected
/// the class-independent `bm.is_free(off)` bit; the class-dependent rewrite
/// (`byte == encode(class_idx, FREE)`) accepted free(p, 64) followed by
/// free(p, 16) — re-pushing `p` onto the 16 B bin with a rewritten `next` and
/// a second `live_count` decrement. Mutant: restore the class-dependent
/// compare in `alloc_core_small/dealloc.rs` → this test goes RED (the first
/// 16 B alloc reissues the already-free `p`).
#[test]
fn wrong_class_free_of_free_block_is_a_noop() {
    let mut ac = AllocCore::new().expect("primordial");
    let l64 = Layout::from_size_align(64, 16).unwrap();
    let l16 = Layout::from_size_align(16, 16).unwrap();

    // Live anchor keeps the segment's bin state non-trivial.
    let anchor = ac.alloc(l16);
    assert!(!anchor.is_null());
    let p = ac.alloc(l64);
    assert!(!p.is_null());

    // Legitimate free: `p` is FREE (its byte carries the 64 B class field).
    // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(p, l64) };

    // Wrong-class free of the already-FREE block: MUST be a no-op (state
    // decode is class-independent), NOT a second push onto the 16 B bin.
    // SAFETY (R6-MS-1/2): `p` is already free — the guard must reject this
    // as a benign no-op; the call itself stays within the dealloc contract
    // being probed (address inside this core's segment, small layout).
    unsafe { ac.dealloc(p, l16) };

    // Behavior: the 16 B class must NOT hand out `p` (it was never freed to
    // the 16 B class).
    for i in 0..64 {
        let q = ac.alloc(l16);
        assert!(!q.is_null(), "16 B alloc {i} null after wrong-class free");
        assert_ne!(
            q, p,
            "wrong-class free re-pushed the FREE block onto the 16 B bin \
             (class-dependent double-free guard)"
        );
    }

    // The block must still be recoverable EXACTLY once, via its own class
    // (LIFO head).
    let r = ac.alloc(l64);
    assert_eq!(r, p, "block lost after rejected wrong-class free");
    // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(r, l64) };
    // The live 16 B anchor frees cleanly at the end.
    // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(anchor, l16) };
}

/// F2 (chunk 3): BOTH wrong-class orders at the substrate double-free guard,
/// each with the state-byte and bin-exactness oracles:
///   * alloc(32) → free(32) → free(16): the stale free(16) of the already-
///     FREE block must be a no-op (state decode is class-INDEPENDENT) — the
///     block's byte is unchanged (still `encode(c32, FREE)`) and the 16 B bin
///     never gains it (a 16 B storm never reissues `p`).
///   * alloc(16) → free(16) → free(32): the mirror order — the free(32) of
///     the FREE 16 B block must be a no-op; the 32 B storm never reissues it.
///
/// Mutant (restore the class-dependent compare
/// `byte == encode(class_idx, FREE)` in `alloc_core_small/dealloc.rs`): in
/// order (a) the free(16) of `p` reads `encode(c32, FREE) != encode(c16,
/// FREE)` and re-pushes `p` onto the 16 B bin → the 16 B storm reissues `p`
/// → RED (same for the mirror order (b)).
#[test]
fn both_wrong_class_free_orders_are_noops() {
    // ── order (a): alloc(32) → free(32) → free(16) ────────────────────────
    {
        let mut ac = AllocCore::new().expect("primordial");
        let l16 = Layout::from_size_align(16, 16).unwrap();
        let l32 = Layout::from_size_align(32, 16).unwrap();

        let anchor16 = ac.alloc(l16);
        assert!(!anchor16.is_null());
        let p = ac.alloc(l32);
        assert!(!p.is_null());

        // Legitimate free: `p` reads encode(c32, FREE).
        // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
        unsafe { ac.dealloc(p, l32) };
        let state_after_legit = ac.dbg_state_byte_state_for(p);
        assert_eq!(state_after_legit, Some(2), "legit free must commit FREE");

        // Wrong-class stale free — MUST be a no-op.
        // SAFETY (R6-MS-1/2): `p` is already free; this is the deliberate
        // wrong-class no-op probe under test.
        unsafe { ac.dealloc(p, l16) };
        assert_eq!(
            ac.dbg_state_byte_state_for(p),
            state_after_legit,
            "wrong-class stale free rewrote the block's state byte"
        );
        // The 16 B storm must never reissue `p`.
        let mut got16 = Vec::new();
        for _ in 0..64 {
            let q = ac.alloc(l16);
            assert!(!q.is_null());
            assert_ne!(
                q, p,
                "free(16) re-pushed the FREE 32B block (class-dependent guard)"
            );
            got16.push(q);
        }
        // The 32 B class still reissues `p` exactly once.
        let r = ac.alloc(l32);
        assert_eq!(r, p, "the block was lost to the rejected wrong-class free");
        // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
        unsafe { ac.dealloc(r, l32) };
        for q in got16 {
            // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
            unsafe { ac.dealloc(q, l16) };
        }
        // SAFETY (R6-MS-1/2): live anchor, matching layout, freed exactly once here.
        unsafe { ac.dealloc(anchor16, l16) };
    }

    // ── order (b): alloc(16) → free(16) → free(32) ────────────────────────
    {
        let mut ac = AllocCore::new().expect("primordial");
        let l16 = Layout::from_size_align(16, 16).unwrap();
        let l32 = Layout::from_size_align(32, 16).unwrap();

        let anchor32 = ac.alloc(l32);
        assert!(!anchor32.is_null());
        let p = ac.alloc(l16);
        assert!(!p.is_null());

        // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
        unsafe { ac.dealloc(p, l16) };
        let state_after_legit = ac.dbg_state_byte_state_for(p);
        assert_eq!(state_after_legit, Some(2), "legit free must commit FREE");

        // SAFETY (R6-MS-1/2): deliberate wrong-class no-op probe on the
        // already-FREE block.
        unsafe { ac.dealloc(p, l32) };
        assert_eq!(
            ac.dbg_state_byte_state_for(p),
            state_after_legit,
            "wrong-class stale free rewrote the block's state byte"
        );
        // The 32 B storm must never reissue `p`.
        let mut got32 = Vec::new();
        for _ in 0..64 {
            let q = ac.alloc(l32);
            assert!(!q.is_null());
            assert_ne!(
                q, p,
                "free(32) re-pushed the FREE 16B block (class-dependent guard)"
            );
            got32.push(q);
        }
        let r = ac.alloc(l16);
        assert_eq!(r, p, "the block was lost to the rejected wrong-class free");
        // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
        unsafe { ac.dealloc(r, l16) };
        for q in got32 {
            // SAFETY (R6-MS-1/2): live, matching layout, freed exactly once here.
            unsafe { ac.dealloc(q, l32) };
        }
        // SAFETY (R6-MS-1/2): live anchor, matching layout, freed exactly once here.
        unsafe { ac.dealloc(anchor32, l32) };
    }
}
