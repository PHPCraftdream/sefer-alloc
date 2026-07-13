//! Miri coverage for the PERF-PASS-4 (G9/C2, task #52) ring-drain empty
//! guard — the pre-drain check in `find_segment_with_free_impl` that skips
//! `RemoteFreeRing::drain` (Relaxed `tail` load vs the owner-cached
//! `SegmentHeader::ring_drain_head`) when nothing new has landed since the
//! last drain.
//!
//! Unlike `reclaim_offset_unit.rs` (which forces every scan through
//! `dbg_drain_all_rings` — an UNCONDITIONAL force-drain that deliberately
//! bypasses the guard), this test exercises the REAL guarded path: a genuine
//! free-list-miss scan via `AllocCore::alloc` → `alloc_small` →
//! `find_segment_with_free`. Single-threaded (no concurrency —
//! `dbg_push_to_ring` stands in for "a remote thread already pushed this
//! before we got here", the same technique `reclaim_offset_unit.rs` and
//! `regression_xthread_double_free_residual.rs` use), so miri's
//! aliasing/provenance checker validates the new
//! `SegmentHeader::ring_drain_head` field's read/write discipline (a plain,
//! non-atomic field access through the `node` seam — exactly like `bump`/
//! `live_count`) under strict provenance.
//!
//! ## Forcing a REAL scan (not a local-freelist hit)
//!
//! `AllocCore::alloc_small`'s carve path (`carve_block_with_refill`) always
//! refills a batch of extra free blocks into the CURRENT segment's own
//! `BinTable` on a cold carve. Left alone, that means `small_cur`'s own
//! local free list is never actually empty by the time this test wants to
//! force a scan. To get a genuine `find_segment_with_free` miss-then-scan,
//! this test drains `small_cur`'s own class free list down to empty (via
//! `dbg_drain_freelist_batch`, an owner-thread BinTable pop with no ring
//! involvement) before pushing the cross-segment offset and re-allocating.
//!
//! ## What a guard bug would look like under miri
//!
//! - **Missed reclaim** (guard wrongly skips a real drain): the segment-A
//!   pointer freed via `dbg_push_to_ring` would never come back from the
//!   next `alloc()` — it would instead carve a brand-new block, and the
//!   assertion that the returned pointer equals the pushed pointer fails.
//! - **UB in the cache field itself**: an out-of-bounds or torn `u32`
//!   read/write on `ring_drain_head` (e.g. an `offset_of!` mistake landing
//!   the field on `SegmentHeader` padding/boundary garbage) is exactly the
//!   class of bug miri's strict-provenance pointer-arithmetic checker
//!   catches on every `Node::read_u32`/`write_u32` call this test drives.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use core::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::alloc_core::AllocCore;

fn seg_of(p: *mut u8) -> usize {
    (p as usize) & !((1usize << 22) - 1)
}

/// A block size close to `SMALL_MAX` (~253 KiB) so a 4 MiB segment holds only
/// a handful of blocks — crossing a segment boundary takes few allocations,
/// keeping this test fast under miri's interpreter.
const BLOCK_SIZE: usize = 200_000;
const BLOCK_ALIGN: usize = 8;

#[test]
fn guarded_scan_reclaims_cross_segment_push_and_skips_when_empty() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(BLOCK_SIZE, BLOCK_ALIGN).unwrap();
    let class_idx = ac
        .dbg_layout_class_for(layout)
        .expect("BLOCK_SIZE must resolve to a small class");

    // Phase 1: allocate enough blocks to span at least two segments — this
    // guarantees `find_segment_with_free_impl`'s scan loop has more than one
    // owned segment to iterate (segment A, no longer `small_cur`; segment B,
    // the current `small_cur`). Keep every handed-out pointer live (own it in
    // `ptrs`) EXCEPT the one segment-A pointer this test frees via the ring.
    let mut ptrs = Vec::new();
    let seg_a = {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "first alloc failed");
        ptrs.push(p);
        seg_of(p)
    };
    let seg_a_ptr = ptrs[0];
    let seg_b = loop {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "alloc failed while spanning segments");
        let s = seg_of(p);
        ptrs.push(p);
        if s != seg_a {
            break s;
        }
        assert!(ptrs.len() < 64, "did not cross a segment boundary in time");
    };
    assert_ne!(seg_b, seg_a, "must have crossed into a second segment");
    // A real, live pointer INTO segment B — the `ptrs` element that
    // triggered the `s != seg_a` break above. Used only to address segment B
    // for the `dbg_*` accessors below (`segment_base_of_ptr` masks it down
    // to the segment base); it is never freed via this alias.
    let seg_b_anchor = *ptrs.last().unwrap();

    // Phase 2: drain `small_cur`'s (segment B's) OWN class free list to
    // empty, own-thread (no ring involved) — `carve_block_with_refill`'s
    // internal batch-refill left extra free blocks sitting in segment B's
    // BinTable; without draining them, the next `alloc()` would satisfy
    // itself locally (`pop_free(small_cur, ...)`) and never reach
    // `find_segment_with_free` at all. `dbg_drain_freelist_batch` leaves each
    // drained block bitmap-ALLOCATED (mirroring `pop_free`'s contract) — keep
    // them in `ptrs` (own-thread-live, freed in phase 6) rather than
    // re-freeing them, which would just refill the very list this phase is
    // emptying.
    let mut drain_buf = vec![std::ptr::null_mut::<u8>(); 64];
    loop {
        // SAFETY: the first arg is a live allocation owned by the receiver.
        let n = unsafe { ac.dbg_drain_freelist_batch(seg_b_anchor, class_idx, &mut drain_buf) };
        if n == 0 {
            break;
        }
        ptrs.extend_from_slice(&drain_buf[..n]);
    }
    assert_eq!(
        ac.dbg_freelist_head_for(seg_b_anchor, class_idx),
        free_list_null(),
        "segment B's class free list must be empty before forcing the scan"
    );

    // Phase 3: simulate a cross-thread free of the segment-A block — push its
    // offset into segment A's ring (segment A's local BinTable free list is
    // otherwise empty; the only route back to this block is the ring).
    let pushed = ac.dbg_push_to_ring(seg_a_ptr, class_idx);
    assert!(pushed, "dbg_push_to_ring failed for the segment-A pointer");

    // Phase 4: a genuine free-list-miss alloc call. `small_cur` (segment B)
    // has an empty class free list (phase 2), so `alloc_small` falls through
    // to `find_segment_with_free`, which scans owned segments — INCLUDING
    // segment A. The guard's `tail_relaxed() != cached_head` branch must
    // fire here (a real push landed since segment A's cache was last set at
    // segment-A's own creation, cache == 0), running a REAL drain that
    // reclaims `seg_a_ptr`'s offset back into segment A's BinTable, which
    // `find_segment_with_free` then reports as a hit.
    let reclaimed = ac.alloc(layout);
    assert!(!reclaimed.is_null(), "post-push alloc returned null");
    assert_eq!(
        reclaimed, seg_a_ptr,
        "guarded scan did not reclaim the cross-thread-pushed segment-A block \
         (expected the drain guard's real-drain branch to fire and hand back \
         exactly the pushed pointer)"
    );

    // Phase 5: a SECOND free-list-miss alloc call with NOTHING new pushed.
    // Segment B's class free list is empty again (the phase-4 `alloc()`
    // consumed the sole reclaimed block and nothing refilled it — a small
    // class with no carve headroom left in segment B falls straight to
    // `find_segment_with_free` again). The guard must now see
    // `tail_relaxed() == cached_head` for segment A (nothing changed since
    // phase 4's drain refreshed the cache) and skip the drain entirely. This
    // must be indistinguishable in outcome from an unconditional drain: no
    // double-reclaim of `seg_a_ptr` (already live, out of every free list),
    // no crash, no UB — the call falls through to carving a FRESH block (a
    // pointer distinct from everything allocated so far).
    let fresh = ac.alloc(layout);
    assert!(!fresh.is_null(), "post-skip alloc returned null");
    let mut all_ptrs: Vec<*mut u8> = ptrs.clone();
    all_ptrs.push(reclaimed);
    let existing: HashSet<usize> = all_ptrs.iter().map(|&p| p as usize).collect();
    assert!(
        !existing.contains(&(fresh as usize)),
        "guard-skip alloc returned a DUPLICATE of an already-live pointer \
         (guard incorrectly skipped a drain it needed, or double-reclaimed)"
    );

    // Phase 6: free everything own-thread (no more cross-thread pushes) —
    // own-thread dealloc does not touch the ring at all, so this just
    // confirms no residual corruption from the guarded scan.
    for &p in &ptrs {
        if p != seg_a_ptr {
            ac.dealloc(p, layout);
        }
    }
    ac.dealloc(reclaimed, layout);
    ac.dealloc(fresh, layout);
}

/// `FREE_LIST_NULL`'s value, mirrored here (the constant is `pub(crate)` in
/// `alloc_core.rs`, not reachable from an integration test). Matches the
/// sentinel documented at `BinTable::head`'s call sites: `u32::MAX`.
fn free_list_null() -> u32 {
    u32::MAX
}
