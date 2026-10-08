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
//! `drain_freelist_batch` (the classic branch) trusted `next` unconditionally: they computed
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
//! `hardened`-gated: before dereferencing a head or continuation, validate its
//! segment range, payload geometry, class alignment, bump frontier, free bitmap,
//! and magazine residency. Invalid state rejects the pop or entire batch without
//! mutating the head, bitmap, live credits, or output. The non-hardened hot path
//! retains its existing checks and arithmetic.
//!
//! ## Counterfactual (RED without the guard)
//!
//! Removing `valid_free_node` from `pop_free` and `try_drain_freelist_batch`, or
//! removing `free_continuation`'s range/free-state validation, lets an
//! out-of-segment or still-allocated link be installed or followed. The scalar
//! tests then return a non-null block or change the head/bitmap instead of
//! rejecting; the batch tests require a one-slot output so a later head check
//! cannot mask a missing continuation check. With the guards present, rejection
//! leaves the chain unchanged, and restoring a valid tail permits the original
//! free blocks to be issued exactly once.
//!
//! The corruption hooks are gated to `hardened` (which implies `fastbin`) and
//! `internals`. Corruption scenarios model invalid prior access/state, not a
//! supported safe-caller behavior.

#![cfg(all(feature = "hardened", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::{AllocCore, SegmentLayout};

const SEGMENT: usize = SegmentLayout::SEGMENT;

/// `pop_free` (the single-block substrate pop, reachable directly from
/// `AllocCore::alloc` on a free-list hit) must reject an out-of-segment
/// continuation without mutating the head or bitmap.
#[test]
fn pop_free_rejects_out_of_segment_next() {
    let mut ac = AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let anchor = ac.alloc(layout);
    let a = ac.alloc(layout);
    let b = ac.alloc(layout);
    assert!(!anchor.is_null() && !a.is_null() && !b.is_null());
    let base = SegmentLayout::segment_base_of(anchor.addr());
    assert_eq!(SegmentLayout::segment_base_of(a.addr()), base);
    assert_eq!(SegmentLayout::segment_base_of(b.addr()), base);

    // The initial refill may leave a pre-existing tail behind a and b.
    let tail_before = ac.dbg_freelist_head_for(anchor, 0);

    // SAFETY: a and b are live allocations from this core with matching layouts.
    unsafe {
        ac.dealloc(a, layout);
        ac.dealloc(b, layout);
    }
    let head_before = ac.dbg_freelist_head_for(anchor, 0);
    let credits_before = ac.dbg_live_count_for(anchor);
    assert_eq!(head_before, (b.addr() - base) as u32);
    assert!(ac.dbg_is_free_for(a) && ac.dbg_is_free_for(b));
    assert!(!ac.dbg_is_free_for(anchor));

    let corrupt_next = (b.addr() ^ SEGMENT) as *mut u8;
    // SAFETY: anchor is a live allocation used only to identify its owned segment.
    assert!(unsafe { ac.dbg_corrupt_freelist_head_next(anchor, 0, corrupt_next) });

    assert!(
        ac.alloc(layout).is_null(),
        "pop_free must reject an untrusted continuation before issuing the head"
    );
    assert_eq!(ac.dbg_freelist_head_for(anchor, 0), head_before);
    assert_eq!(ac.dbg_live_count_for(anchor), credits_before);
    assert!(ac.dbg_is_free_for(a) && ac.dbg_is_free_for(b));

    // SAFETY: anchor is live; restore the free head's original valid link.
    assert!(unsafe { ac.dbg_corrupt_freelist_head_next(anchor, 0, a) });
    let popped_b = ac.alloc(layout);
    let popped_a = ac.alloc(layout);
    assert_eq!(popped_b, b);
    assert_eq!(popped_a, a);
    assert_eq!(ac.dbg_freelist_head_for(anchor, 0), tail_before);

    let mut issued = vec![anchor, popped_b, popped_a];
    for _ in 0..64 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "post-corruption alloc returned null");
        issued.push(p);
    }
    let distinct: std::collections::HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(distinct.len(), issued.len());
    for p in issued {
        // SAFETY: every pointer in issued is live exactly once with this layout.
        unsafe { ac.dealloc(p, layout) };
    }
}

/// `drain_freelist_batch` must reject the whole batch when the head's
/// continuation is out of segment, preserving all free-list state.
#[test]
fn drain_freelist_batch_rejects_out_of_segment_next() {
    let mut ac = AllocCore::new().expect("primordial reservation");
    let class_idx = 0usize;
    let layout = Layout::from_size_align(16, 8).unwrap();
    let mut carved = [core::ptr::null_mut::<u8>(); 4];
    assert_eq!(ac.dbg_carve_batch(class_idx, &mut carved), 4);
    let [anchor, a, b, c] = carved;
    let base = SegmentLayout::segment_base_of(anchor.addr());
    assert!([a, b, c]
        .into_iter()
        .all(|p| SegmentLayout::segment_base_of(p.addr()) == base));

    // SAFETY: these three blocks are live allocations from this core.
    unsafe {
        ac.dealloc(a, layout);
        ac.dealloc(b, layout);
        ac.dealloc(c, layout);
    }
    let head_before = ac.dbg_freelist_head_for(anchor, class_idx);
    let credits_before = ac
        .dbg_live_count_for(anchor)
        .expect("live count for owned segment");
    assert_eq!(head_before, (c.addr() - base) as u32);
    assert!(ac.dbg_is_free_for(a) && ac.dbg_is_free_for(b) && ac.dbg_is_free_for(c));
    assert!(!ac.dbg_is_free_for(anchor));

    let corrupt_next = (c.addr() ^ SEGMENT) as *mut u8;
    // SAFETY: anchor is live and identifies this exclusively-owned segment.
    assert!(unsafe { ac.dbg_corrupt_freelist_head_next(anchor, class_idx, corrupt_next) });
    let mut rejected = [core::ptr::null_mut::<u8>(); 1];
    // SAFETY: anchor is a live allocation in this core's owned segment.
    let drained = unsafe { ac.dbg_drain_freelist_batch(anchor, class_idx, &mut rejected) };
    assert_eq!(drained, 0, "invalid continuation must reject the batch");
    assert_eq!(rejected, [core::ptr::null_mut()]);
    assert_eq!(ac.dbg_freelist_head_for(anchor, class_idx), head_before);
    assert_eq!(ac.dbg_live_count_for(anchor), Some(credits_before));
    assert!(ac.dbg_is_free_for(a) && ac.dbg_is_free_for(b) && ac.dbg_is_free_for(c));

    // SAFETY: anchor is live; restore the head's original valid link.
    assert!(unsafe { ac.dbg_corrupt_freelist_head_next(anchor, class_idx, b) });
    let mut out = [core::ptr::null_mut::<u8>(); 8];
    // SAFETY: anchor is a live allocation in this core's owned segment.
    let popped = unsafe { ac.dbg_drain_freelist_batch(anchor, class_idx, &mut out) };
    assert_eq!(popped, 3);
    assert_eq!(&out[..3], &[c, b, a]);
    assert_eq!(ac.dbg_freelist_head_for(anchor, class_idx), u32::MAX);
    assert!(!ac.dbg_is_free_for(a) && !ac.dbg_is_free_for(b) && !ac.dbg_is_free_for(c));
    assert_eq!(ac.dbg_live_count_for(anchor), Some(credits_before + 3));

    let mut issued = vec![anchor, a, b, c];
    for _ in 0..64 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "post-corruption alloc returned null");
        issued.push(p);
    }
    let distinct: std::collections::HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(distinct.len(), issued.len());
    for p in issued {
        // SAFETY: every pointer in issued is live exactly once with this layout.
        unsafe { ac.dealloc(p, layout) };
    }
}
/// A same-segment continuation may still be invalid because it is allocated,
/// or cyclic because it points back to the free head. Range-only validation
/// misses both; hardened validation must reject before changing allocator state.
fn continuation_is_rejected(batch: bool, self_cycle: bool) {
    let mut ac = AllocCore::new().expect("primordial reservation");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let class_idx = 0;
    let mut blocks = [core::ptr::null_mut(); 2];
    assert_eq!(ac.dbg_carve_batch(class_idx, &mut blocks), 2);
    let [live, head] = blocks;
    assert_eq!(
        SegmentLayout::segment_base_of(live.addr()),
        SegmentLayout::segment_base_of(head.addr())
    );

    // SAFETY: head is live with this layout and is freed exactly once below.
    unsafe { ac.dealloc(head, layout) };
    let head_before = ac.dbg_freelist_head_for(live, class_idx);
    let credits_before = ac.dbg_live_count_for(live);
    assert!(ac.dbg_is_free_for(head));
    assert!(!ac.dbg_is_free_for(live));

    // SAFETY: live anchors this owned segment; the hook writes the supplied
    // continuation into the valid free-list head.
    let next = if self_cycle { head } else { live };
    assert!(unsafe { ac.dbg_corrupt_freelist_head_next(live, class_idx, next) });
    if batch {
        // One slot makes the continuation check itself observable; with two,
        // a later head check could mask a missing continuation guard.
        let mut out = [core::ptr::null_mut(); 1];
        // SAFETY: live anchors this exclusively-owned segment.
        let drained = unsafe { ac.dbg_drain_freelist_batch(live, class_idx, &mut out) };
        assert_eq!(drained, 0, "invalid continuation must reject the batch");
        assert_eq!(
            out,
            [core::ptr::null_mut()],
            "rejection must preserve output"
        );
    } else {
        assert!(
            ac.alloc(layout).is_null(),
            "invalid continuation must reject scalar pop"
        );
    }

    assert_eq!(ac.dbg_freelist_head_for(live, class_idx), head_before);
    assert_eq!(ac.dbg_live_count_for(live), credits_before);
    assert!(ac.dbg_is_free_for(head));
    assert!(!ac.dbg_is_free_for(live));

    // SAFETY: live anchors the still-valid head; restore its original null tail.
    assert!(unsafe { ac.dbg_corrupt_freelist_head_next(live, class_idx, core::ptr::null_mut()) });
    assert_eq!(
        ac.alloc(layout),
        head,
        "valid control must reissue the head"
    );
    assert!(!ac.dbg_is_free_for(head));
    assert_eq!(ac.dbg_freelist_head_for(live, class_idx), u32::MAX);
    // SAFETY: both allocations are live with this layout and freed exactly once.
    unsafe {
        ac.dealloc(head, layout);
        ac.dealloc(live, layout);
    }
}

#[test]
fn pop_rejects_allocated_continuation_without_mutation() {
    continuation_is_rejected(false, false);
}

#[test]
fn drain_rejects_allocated_continuation_without_mutation() {
    continuation_is_rejected(true, false);
}

#[test]
fn drain_rejects_self_cycle_without_mutation() {
    continuation_is_rejected(true, true);
}

#[test]
fn pop_rejects_self_cycle_without_mutation() {
    continuation_is_rejected(false, true);
}
