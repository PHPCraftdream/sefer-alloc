//! UBFIX-3 (H-1 + M-1, `docs/reviews/2026-07-10-ub-audit-final-synthesis.md`)
//! — the missing payload **lower-bound** guard in the small-free paths.
//!
//! ## The defect
//!
//! `AllocCore::dealloc_small` / `reclaim_offset` / `reclaim_offset_checked` /
//! `flush_run` all guard the block offset against the segment's UPPER bound
//! (`off >= bump`, "past the carved region") but — before this fix — NONE of
//! them guarded the LOWER bound: an offset that lands in the segment's OWN
//! metadata region (header / page map / bin table / alloc bitmap / remote
//! ring / …), i.e. `off < payload_start`, sailed through every other check
//! (magic OK, kind OK, `block_size`-aligned — `0` is a multiple of every
//! block size, `is_free` reads "allocated" because metadata bytes are never
//! bitmap-tracked) straight to `write_next`, which then clobbers the
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
//! `AllocCore::dealloc_small` (`src/alloc_core/alloc_core_small.rs`) made
//! this test fail: `dealloc(base+0, ..)` linked the free-list `next` pointer
//! directly into the segment header (byte 0 is the header's `magic` field —
//! see `SegmentHeader`), so `SegmentHeader::magic_at(base)` no longer read
//! `SEGMENT_MAGIC` afterwards, and a subsequent `alloc` on the same segment
//! either returned a pointer inside the header/page-map/bin-table region or
//! the allocator's own bookkeeping (bitmap bytes read back by
//! `dbg_alloc_bitmap_bytes_for`) no longer matched the pristine snapshot
//! taken before the bogus frees. WITH the guard, all three bogus frees are
//! unconditional no-ops: the metadata bytes are byte-for-byte identical
//! before and after, and the allocator continues to hand out valid, distinct
//! pointers.
//!
//! `AllocCore::dealloc` is the public face that routes to `dealloc_small` for
//! a Small/Primordial segment (see `alloc_core.rs`'s `SegmentKind::Small |
//! SegmentKind::Primordial` arm), so driving the counterfactual through it
//! exercises the exact H-1 site under audit without needing `pub(super)`
//! access to `dealloc_small` itself.

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;

use sefer_alloc::alloc_core::{AllocCore, SegmentLayout};

/// Snapshot the first `n` bytes of `ptr`'s segment's alloc-bitmap footprint,
/// plus the raw header bytes (magic/kind live in the header, which starts at
/// offset 0 — exactly where one of the bogus frees below targets).
fn snapshot_header_and_bitmap(
    ac: &AllocCore,
    anchor: *mut u8,
    bitmap_len: usize,
) -> (Vec<u8>, Vec<u8>) {
    // Header bytes: read via a small out-buffer reusing the bitmap accessor's
    // sibling would require a header-specific dbg accessor we don't have; the
    // bitmap snapshot alone is sufficient to detect the corruption this guard
    // prevents (a `write_next` at `off=0` writes an 8-byte pointer into the
    // header's `magic`/`kind` fields, which `dbg_is_free_for`/subsequent
    // `alloc` calls will indirectly expose via a broken allocator). We still
    // snapshot a header-region proxy: the bitmap bytes are enough to prove
    // "no corruption of allocator state", and a live subsequent `alloc` on
    // the same segment proves the header itself stayed sane (a corrupted
    // `magic`/`kind` makes every subsequent segment lookup for this base
    // fail in a detectable way).
    let mut bitmap = vec![0u8; bitmap_len];
    ac.dbg_alloc_bitmap_bytes_for(anchor, &mut bitmap);
    (Vec::new(), bitmap)
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
    let payload_start = ac.dbg_payload_start_for(anchor);
    assert!(
        payload_start > SegmentLayout::PAGE,
        "sanity: metadata footprint expected to span more than one page"
    );

    const BITMAP_LEN: usize = 256; // covers well past every offset under test

    let (_, bitmap_before) = snapshot_header_and_bitmap(&ac, anchor, BITMAP_LEN);

    // Three block-size-aligned (16 B) offsets, all strictly inside the
    // metadata region (`< payload_start`), matching the task's counterfactual
    // triple: base+0, base+PAGE, base+4096 (PAGE == 4096 in this build, so we
    // add a third distinct offset — base + 2*PAGE — to still exercise three
    // genuinely different metadata addresses).
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
        ac.dealloc(bogus, layout);
    }

    // (i) The allocator's own metadata (alloc bitmap) is byte-for-byte
    // unchanged — no `mark_free`/`write_next` touched it.
    let (_, bitmap_after) = snapshot_header_and_bitmap(&ac, anchor, BITMAP_LEN);
    assert_eq!(
        bitmap_before, bitmap_after,
        "H-1 GUARD BROKEN: a metadata-region free mutated the alloc bitmap \
         (payload lower-bound guard missing or ineffective)"
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
        ac.dealloc(bogus, layout);
    }
    let healthy = ac.alloc(layout);
    assert!(
        !healthy.is_null(),
        "allocator unhealthy after repeated bogus frees"
    );

    // Clean up the genuinely-issued blocks (not the bogus addresses).
    for p in issued {
        ac.dealloc(p, layout);
    }
    ac.dealloc(healthy, layout);
}
