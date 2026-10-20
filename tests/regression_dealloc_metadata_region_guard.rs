//! UBFIX-3 (H-1 + M-1, `docs/reviews/2026-07-10-ub-audit-final-synthesis.md`)
//! — the missing payload **lower-bound** guard in the small-free paths.
//!
//! ## The defect
//!
//! `AllocCore::dealloc_small` / `reclaim_offset` / `reclaim_offset_checked` /
//! `flush_run` all guard the block offset against the segment's UPPER bound
//! (`off >= bump`, "past the carved region") but — before this fix — NONE of
//! them guarded the LOWER bound: an offset that lands in the segment's OWN
//! metadata region (header / page map / bin table / state-byte region /
//! remote
//! ring / …), i.e. `off < payload_start`, sailed through every other check
//! (magic OK, kind OK, `block_size`-aligned — `0` is a multiple of every
//! block size, metadata bytes read as the all-zero "never issued" state
//! encoding) straight to `write_next`, which then clobbers the
//! segment's header/page-map/bin-table bytes in place — silent corruption of
//! live allocator state, not merely of user payload.
//!
//! M-1 compounds this: the pre-existing `off >= bump` UPPER guard was itself
//! `#[cfg(feature = "alloc-decommit")]`-gated, so a build without that
//! feature had no bound checking on `off` at all.
//!
//! ## Counterfactual (RED without the fix)
//!
//! Before the fix landed, temporarily commenting out the
//! `if (off as usize) < payload_start { return; }` guard in
//! `AllocCore::dealloc_small` (`src/alloc_core/small/alloc_core_small/`) made
//! this test fail: `dealloc(base+0, ..)` linked the free-list `next` pointer
//! directly into the segment header (byte 0 is the header's `magic` field —
//! see `SegmentHeader`), so `SegmentHeader::magic_at(base)` no longer read
//! `SEGMENT_MAGIC` afterwards, and a subsequent `alloc` on the same segment
//! either returned a pointer inside the header/page-map/bin-table region or
//! the allocator's own bookkeeping (the authoritative per-granule STATE-BYTE
//! region read back by `dbg_state_bytes_for`; B4 removed the dead bitmaps and
//! their dump hook) no longer matched the pristine snapshot taken before the
//! bogus frees. WITH the guard, all three bogus frees are unconditional
//! no-ops: the metadata bytes are byte-for-byte identical before and after,
//! and the allocator continues to hand out valid, distinct pointers.
//!
//! `AllocCore::dealloc` is the public face that routes to `dealloc_small` for
//! a Small/Primordial segment (see `alloc_core.rs`'s `SegmentKind::Small |
//! SegmentKind::Primordial` arm), so driving the counterfactual through it
//! exercises the exact H-1 site under audit without needing `pub(super)`
//! access to `dealloc_small` itself.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::{AllocCore, SegmentLayout};

/// Snapshot the first `state_len` bytes of the STATE-BYTE region (B4 (в)) —
/// the authoritative per-granule record the bogus frees must leave pristine.
/// The bogus metadata offsets `0 / PAGE / 2*PAGE` map to granule (== byte)
/// indices `0 / PAGE>>4 / 2*PAGE>>4`, so a snapshot of that many state bytes
/// covers every granule the corruption would touch; the segment header
/// itself is checked behaviourally — a `write_next` at `off=0` clobbers the
/// header's `magic`/`kind` fields, which makes every subsequent segment
/// lookup/`alloc` on this base fail in a detectable way (part (ii) below).
fn snapshot_state_bytes(ac: &AllocCore, anchor: *mut u8, state_len: usize) -> Vec<u8> {
    let mut state_bytes = vec![0u8; state_len];
    // SAFETY: `anchor` is a live allocation owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(anchor, &mut state_bytes) };
    state_bytes
}

#[test]
fn dealloc_metadata_region_offsets_are_noop() {
    let mut ac = AllocCore::new().expect("primordial segment bootstrap");

    // 16 B / 8-align → class 0 (the finest small class).
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Establish a live anchor block so we have a real segment base to target.
    let anchor = ac.alloc(layout);
    assert!(!anchor.is_null(), "anchor alloc returned null");

    // Derive the segment base as a pointer offset FROM `anchor` (never via an
    // integer-to-pointer cast) so this test stays clean under
    // `-Zmiri-strict-provenance` — see `regression_realloc_cross_class_shrink`
    // (in the miri MATRIX) for the same discipline. `usize` arithmetic to
    // compute the OFFSET is fine; only forming a `*mut` from a bare integer is
    // the strict-provenance violation.
    let anchor_addr = anchor as usize;
    let base_addr = SegmentLayout::segment_base_of(anchor_addr);
    assert_eq!(base_addr, anchor_addr & !(SegmentLayout::SEGMENT - 1));
    let anchor_off_from_base = anchor_addr - base_addr;
    let base = unsafe { anchor.byte_sub(anchor_off_from_base) };

    // The compile-time payload lower bound for THIS segment's kind (primordial
    // for a fresh `AllocCore`'s first segment, per `Layout::primordial_meta_end()`
    // vs `Layout::small_meta_end()` — the exact split the fix branches on).
    // SAFETY: `anchor` is a live allocation owned by `ac`.
    let payload_start = unsafe { ac.dbg_payload_start_for(anchor) };
    assert!(
        payload_start > SegmentLayout::PAGE,
        "sanity: metadata footprint expected to span more than one page"
    );

    // B4 (в): state-byte snapshot covering the granule indices the bogus
    // offsets map to (0, PAGE>>4, 2·PAGE>>4 = 0/128/256 for PAGE = 4096).
    const STATE_LEN: usize = 1024;

    let state_before = snapshot_state_bytes(&ac, anchor, STATE_LEN);

    // Three block-size-aligned (16 B) offsets, all strictly inside the
    // metadata region (`< payload_start`), matching the task's counterfactual
    // triple: base+0, base+PAGE, base+2*PAGE (PAGE is allocator's compile-time
    // geometry constant, not necessarily the host runtime page size).
    let bogus_offsets: [usize; 3] = [0, SegmentLayout::PAGE, 2 * SegmentLayout::PAGE];
    for &off in &bogus_offsets {
        assert!(
            off < payload_start,
            "test setup: offset {off} must be inside the metadata region (< {payload_start})"
        );
        assert_eq!(off % 16, 0, "offset must be class-0 block-size aligned");
    }

    for &off in &bogus_offsets {
        let bogus = unsafe { base.add(off) };
        // The hazardous free: an in-segment, block-aligned address that was
        // NEVER carved (it points into the segment's own metadata). The H-1
        // guard must make this an unconditional no-op.
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(bogus, layout) };
    }

    // (i) The allocator's own metadata (the authoritative per-granule state
    // bytes) is byte-for-byte unchanged — no `write_next`/state-store
    // touched it.
    let state_after = snapshot_state_bytes(&ac, anchor, STATE_LEN);
    assert_eq!(
        state_before, state_after,
        "H-1 GUARD BROKEN: a metadata-region free mutated the STATE-BYTE \
         region (payload lower-bound guard missing or ineffective)"
    );
    assert!(
        state_after.iter().all(|&b| b == 0),
        "STATE-BYTE region low end not all-zero after bogus metadata frees \
         (first non-zero granule at {:?})",
        state_after.iter().position(|&b| b != 0)
    );

    // (ii) The segment must still be healthy: further allocations succeed,
    // are non-null, and are distinct from both the anchor and every bogus
    // address (a corrupted bin-table head could otherwise hand one of the
    // bogus in-metadata addresses back out as a "free" block).
    let mut issued = vec![anchor];
    for _ in 0..64 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "post-bogus-free alloc returned null");
        for &bogus_off in &bogus_offsets {
            assert_ne!(
                p as usize,
                base as usize + bogus_off,
                "METADATA-REGION ADDRESS ISSUED: a bogus in-metadata free was \
                 linked onto the free list and handed back out by alloc \
                 (H-1 guard missing)"
            );
        }
        issued.push(p);
    }
    let distinct: std::collections::HashSet<usize> = issued.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        issued.len(),
        "DUPLICATE POINTER after metadata-region bogus frees — allocator \
         state corrupted"
    );

    // (iii) Re-freeing the same bogus addresses stays idempotent (still a
    // no-op) — no crash, no double-free artefact.
    for &off in &bogus_offsets {
        let bogus = unsafe { base.add(off) };
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(bogus, layout) };
    }
    let healthy = ac.alloc(layout);
    assert!(
        !healthy.is_null(),
        "allocator unhealthy after repeated bogus frees"
    );

    // Clean up the genuinely-issued blocks (not the bogus addresses).
    for p in issued {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(healthy, layout) };
}
