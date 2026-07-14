//! UBFIX-7 (M-3, `docs/reviews/2026-07-10-ub-audit-final-synthesis.md`) — the
//! HARDENED intrusive-freelist `next`-pointer validation guard in
//! `AllocCore::pop_free` / `AllocCore::drain_freelist_batch`.
//!
//! ## The defect
//!
//! The small-object substrate free list is intrusive: a freed block's own
//! first word stores the `next` pointer of the chain. That word is inside
//! memory the USER controls for as long as they hold (or, via a
//! use-after-free, still write to) the block. Before this fix, `pop_free` and
//! `drain_freelist_batch` (both `#[cfg(feature = "alloc-runfreelist")]` and
//! the classic branch) trusted `next` unconditionally: they computed
//! `(next as usize - segment as usize) as u32` and stored the result as the
//! new freelist head offset with NO check that `next` actually lies inside
//! `segment`.
//!
//! A UAF write into an already-freed block that overwrites this `next` word
//! with a garbage or out-of-segment pointer therefore produces a garbage
//! `u32` offset. The chain is not immediately broken — that garbage offset is
//! simply stored as the new head. The NEXT pop/drain of that class then feeds
//! this offset straight into `Node::deref` (`segment.add(off)`), an
//! out-of-bounds pointer arithmetic — UB per `node.rs`'s own SAFETY contract
//! — and whatever lands there is handed back out to the caller dressed up as
//! a legitimate free block.
//!
//! ## The fix
//!
//! `hardened`-gated (mimalloc `MI_SECURE`-style): before trusting a non-null
//! `next`, verify `segment_base_of_ptr(next) == segment`. On mismatch, the
//! chain is TRUNCATED at that point (treated as `FREE_LIST_NULL`) instead of
//! being dereferenced — the corrupted tail is dropped, never followed.
//!
//! ## Counterfactual (RED without the guard)
//!
//! Temporarily removing the `#[cfg(feature = "hardened")] let next = if
//! next.is_null() || os::segment_base_of_ptr(next) == segment { next } else {
//! core::ptr::null_mut() };` block in any of the three call sites in
//! `alloc_core_small.rs` (`pop_free`, and both branches of
//! `drain_freelist_batch`) makes the corresponding test below fail: the
//! corrupted `next` is accepted as the new head, and the FOLLOWING alloc
//! either panics (debug assertions / segfault under a real corrupt address)
//! or — with the small in-bounds-but-wrong-segment offset used here — hands
//! back a pointer that is NOT the anchor block, tripping the "wild pointer
//! returned" assertion. With the guard restored, the corrupted tail is
//! dropped and the allocator stays healthy.
//!
//! Gated to `hardened` (implies `fastbin`): only that build compiles the
//! guard and the `dbg_corrupt_freelist_head_next` test hook.

#![cfg(feature = "hardened")]

use core::alloc::Layout;

use sefer_alloc::alloc_core::{AllocCore, SegmentLayout};

const SEGMENT: usize = SegmentLayout::SEGMENT;

/// `pop_free` (the single-block substrate pop, reachable directly from
/// `AllocCore::alloc` on a free-list hit) must not follow a `next` pointer
/// that was corrupted to point outside the owning segment.
#[test]
fn pop_free_rejects_out_of_segment_next() {
    let mut ac = AllocCore::new().expect("primordial reservation");

    // 16 B / 8-align -> the finest small class.
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Build a short free list: alloc two blocks, free both (LIFO), so the
    // head is `b`, and `b.next == a` (the second-oldest free entry).
    let a = ac.alloc(layout);
    let b = ac.alloc(layout);
    assert!(!a.is_null() && !b.is_null());
    assert_ne!(a, b);
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(a, layout) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(b, layout) };

    // Sanity: the free list head is `b` (LIFO).
    assert_eq!(ac.dbg_freelist_head_for(a, 0), {
        let base = a as usize & !(SEGMENT - 1);
        (b as usize - base) as u32
    });

    // Corrupt `b`'s `next` word (the head's chain pointer) with an address
    // computed by flipping the segment-alignment bit of `b` itself — this is
    // guaranteed to resolve to a DIFFERENT segment base than `b`'s own
    // segment (out-of-segment), while still being a plausible-looking
    // pointer value (not a wild address like `0x1`), simulating a realistic
    // UAF corruption.
    let corrupt_next = (b as usize ^ SEGMENT) as *mut u8;
    // SAFETY: the ptr arg is a live allocation owned by the receiver.
    let corrupted = unsafe { ac.dbg_corrupt_freelist_head_next(b, 0, corrupt_next) };
    assert!(
        corrupted,
        "test setup: free list for class 0 must be non-empty"
    );

    // Popping `b` must succeed (the head itself is untouched — only its
    // `next` field was corrupted) and must NOT dereference the corrupted
    // `next`: the guard truncates the chain, so the free list must now be
    // EMPTY (head == FREE_LIST_NULL) rather than pointing at a garbage
    // offset derived from `corrupt_next`.
    let popped = ac.alloc(layout);
    assert_eq!(
        popped, b,
        "pop_free must still return the legitimate head block"
    );

    const FREE_LIST_NULL: u32 = u32::MAX;
    assert_eq!(
        ac.dbg_freelist_head_for(a, 0),
        FREE_LIST_NULL,
        "GUARD BROKEN: pop_free trusted a `next` pointer outside its segment \
         and installed a garbage offset as the new freelist head instead of \
         truncating the corrupted chain"
    );

    // The allocator must remain healthy: further allocations of this class
    // succeed and are never the stale `a` block re-derived from a garbage
    // offset by coincidence being handed out twice, nor null.
    let mut issued = vec![b];
    for _ in 0..64 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "post-corruption alloc returned null");
        issued.push(p);
    }
    let distinct: std::collections::HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE POINTER after next-pointer corruption — the truncated \
         chain leaked or a wild pointer was reissued"
    );

    for p in issued {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}

/// `drain_freelist_batch` (the batch pop feeding the magazine refill path)
/// must apply the identical guard: a corrupted `next` mid-chain truncates the
/// walk instead of being dereferenced on the following iteration.
#[test]
fn drain_freelist_batch_rejects_out_of_segment_next() {
    let mut ac = AllocCore::new().expect("primordial reservation");
    let class_idx = 0usize; // 16 B class, matches dbg_carve_batch's direct class-0 carve

    // Carve three blocks directly (bypassing the magazine) and free all three
    // in order a, b, c -> LIFO free list head is c, then b, then a.
    let mut carved = [core::ptr::null_mut::<u8>(); 3];
    let n = ac.dbg_carve_batch(class_idx, &mut carved);
    assert_eq!(n, 3, "expected to carve exactly 3 fresh blocks");
    let [a, b, c] = carved;
    assert!(!a.is_null() && !b.is_null() && !c.is_null());

    let layout = Layout::from_size_align(16, 8).unwrap();
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(a, layout) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(b, layout) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(c, layout) };

    // Head is `c`; chain is c -> b -> a -> NULL. Corrupt `c`'s `next` (which
    // currently points at `b`) to an out-of-segment address.
    let corrupt_next = (c as usize ^ SEGMENT) as *mut u8;
    // SAFETY: the ptr arg is a live allocation owned by the receiver.
    let corrupted = unsafe { ac.dbg_corrupt_freelist_head_next(c, class_idx, corrupt_next) };
    assert!(corrupted, "test setup: free list must be non-empty");

    // Drain up to 8 blocks (more than the 3 available) — the walk must stop
    // at `c` (the only block whose `next` is trustworthy) rather than
    // dereferencing the corrupted `next` to "reach" a bogus second entry.
    let mut out: [*mut u8; 8] = [core::ptr::null_mut(); 8];
    // SAFETY: the first arg is a live allocation owned by the receiver.
    let popped = unsafe { ac.dbg_drain_freelist_batch(c, class_idx, &mut out) };

    assert_eq!(
        popped, 1,
        "GUARD BROKEN: drain_freelist_batch followed a corrupted out-of-\
         segment `next` pointer instead of truncating the chain at the \
         first untrustworthy link"
    );
    assert_eq!(out[0], c);

    // The freelist must now be empty (truncated), NOT pointing at a garbage
    // offset derived from `corrupt_next` — `b` and `a` are or­phaned from the
    // linked list by the truncation (a known, documented containment
    // trade-off: corruption drops the REST of the chain rather than risking
    // an OOB deref), but the allocator itself must stay healthy: it must not
    // crash and must not hand out any wild/duplicate pointer afterward.
    const FREE_LIST_NULL: u32 = u32::MAX;
    assert_eq!(
        ac.dbg_freelist_head_for(c, class_idx),
        FREE_LIST_NULL,
        "freelist head must be NULL after the corrupted chain was truncated"
    );

    // Allocator stays healthy: further allocs succeed, are distinct, and
    // none of them is a wild pointer derived from `corrupt_next`.
    let mut issued = vec![c];
    for _ in 0..64 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "post-corruption alloc returned null");
        assert_ne!(
            p, corrupt_next,
            "a wild pointer derived from the corrupted `next` was issued"
        );
        issued.push(p);
    }
    let distinct: std::collections::HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE POINTER after next-pointer corruption in drain_freelist_batch"
    );

    for p in issued {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}
