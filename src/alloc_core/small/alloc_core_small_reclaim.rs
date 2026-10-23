//! Owner-only logical retirement of terminal Small sidecar records.
//!
//! D1′ phase-1a: the class is taken FROM THE OWNER-PRIVATE STATE BYTE (the
//! sidecar leaf map is no longer read here). Only `encode(c, LIVE)` is
//! accepted; a correct FREE/MAG byte is a quiet no-op reject counted by the
//! internals/bench-internals owner counters — in EVERY build (the MAG
//! reject is not `fastbin`-gated). Fatal lanes (ALL builds, in order): a
//! zero byte at a start, state 3, class 0/≥ COUNT, a published record
//! whose start bit is 0 (checked BEFORE the quiet FREE/MAG returns),
//! invalid geometry, and the production overlap check over the open
//! interval `(off, off + bs)`.

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::fatal::fatal;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::node::Node;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::size_classes::SizeClasses;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use core::ptr::NonNull;

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
impl AllocCore {
    /// D1′ phase-1a: crate-private read-only seam for the registry-side
    /// synthetic reclaim probe
    /// (`HeapCore::dbg_reclaim_sidecar_record_for_test`), which needs the
    /// guarded route-table checks but lives outside `alloc_core`.
    // Only the internals+bench-internals registry probe reaches this seam.
    #[cfg_attr(
        not(all(feature = "internals", feature = "bench-internals")),
        allow(dead_code)
    )]
    pub(crate) fn sidecar_table(&self) -> &crate::alloc_core::segment_table::SegmentTable {
        &self.table
    }

    /// D1′ phase-1a numeric observers: (REMOTE_REJECT_UNISSUED,
    /// REMOTE_REJECT_FREE, REMOTE_REJECT_MAG, CARVE_CLEAR_BITS,
    /// CARVE_CLEAR_WORD_LOADS). Diagnostic deltas only — these count
    /// OBSERVED rejected pending records/bits, not misuse calls (a
    /// `fetch_or` collapses duplicate publications; carve-clear drops bits
    /// without a reclaim).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_oneclass_counters() -> (u64, u64, u64, u64, u64) {
        use core::sync::atomic::Ordering::Relaxed;
        (
            crate::alloc_core::REMOTE_REJECT_UNISSUED.load(Relaxed),
            crate::alloc_core::REMOTE_REJECT_FREE.load(Relaxed),
            crate::alloc_core::REMOTE_REJECT_MAG.load(Relaxed),
            crate::alloc_core::CARVE_CLEAR_BITS.load(Relaxed),
            crate::alloc_core::CARVE_CLEAR_WORD_LOADS.load(Relaxed),
        )
    }

    /// D1′ M-h oracle seam (test-only): the pause arm for the
    /// `try_carve_batch` early-publication window. `true` parks the NEXT
    /// batch carve inside `[set_bump, first per-block issue]` until
    /// [`Self::dbg_carve_batch_resume`]; `false` disarms (the default).
    /// Numeric flag store only — no pointer, no allocator state.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_carve_batch_pause(pause: bool) {
        crate::alloc_core::CARVE_BATCH_PAUSE.store(pause, core::sync::atomic::Ordering::Release);
    }

    /// D1′ M-h oracle seam (test-only): release the owner parked by
    /// [`Self::dbg_carve_batch_pause`] inside the batch carve. One flag
    /// store — no pointer, no allocator state.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_carve_batch_resume() {
        crate::alloc_core::CARVE_BATCH_RESUME.store(true, core::sync::atomic::Ordering::Release);
    }

    /// D1′ M-h oracle seam (test-only): the batch-carve stage — `0` outside
    /// a batch carve, otherwise the number of blocks the in-flight
    /// `try_carve_batch` is about to issue (owner parked inside the
    /// early-publication window). Read-only Acquire load of a numeric flag.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_carve_batch_stage() -> usize {
        crate::alloc_core::CARVE_BATCH_STAGE.load(core::sync::atomic::Ordering::Acquire)
    }

    /// The owner retains the scan while retiring records and drops it before
    /// reservation finalization. A duplicate or magazine-resident record never
    /// mutates the free list or retires a second credit.
    ///
    /// D1′ phase-1a: returns `Some(class)` of the accepted LIVE block (the
    /// class comes from the state byte), `None` for a quiet FREE/MAG reject.
    /// MEASUREMENT-ONLY (D1′ round-2 P2#6 overlap isolation):
    /// `#[inline(never)]` callgrind `--toggle-collect` boundary around the
    /// production reclaim overlap check. It calls the SAME production
    /// `SegmentTable::reclaim_start_overlap` (start-bits bank scan via
    /// `bank_bits_between`/`for_words_in_range`) with the same arguments;
    /// toggling collection on this symbol therefore measures EXACTLY the
    /// production overlap-check path per record — separate from the
    /// pending-bank scan (`bitmap_scan.rs`/`bitmap_cut.rs`). No behavior
    /// change: the `bench-internals`-off build keeps the direct call.
    #[cfg(feature = "bench-internals")]
    #[doc(hidden)]
    #[inline(never)]
    fn overlap_check_probe_call(
        table: &crate::alloc_core::segment_table::SegmentTable,
        base: *mut u8,
        start: u32,
        end: u32,
    ) -> bool {
        table.reclaim_start_overlap(base, start, end)
    }

    pub(crate) fn reclaim_sidecar_record(
        table: &crate::alloc_core::segment_table::SegmentTable,
        base: *mut u8,
        offset: u32,
    ) -> Option<u8> {
        let off = offset as usize;
        let kind = SegmentHeader::kind_at(base);
        if !matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            fatal(b"alloc_core_small_reclaim: sidecar record kind is not small\n");
        }
        let payload_start = if kind == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        };
        // B4 (в) / D1′: the state byte is the SINGLE class source now.
        let sb = SegmentMeta::new(base).state_byte().read(offset);
        if sb == 0 {
            #[cfg(any(feature = "internals", feature = "bench-internals"))]
            crate::alloc_core::REMOTE_REJECT_UNISSUED
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            fatal(b"alloc_core_small_reclaim: zero state byte at a start\n");
        }
        let state = sb >> 6;
        if state == 3 {
            fatal(b"alloc_core_small_reclaim: state byte carries the unused state 3\n");
        }
        let class_code = usize::from(sb & 0x3F);
        if class_code == 0 || class_code > crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            fatal(b"alloc_core_small_reclaim: small class index out of range\n");
        }
        let class_idx = class_code - 1;
        // Fatal lane ordering (round D1′ fix): the published record's start
        // granule MUST carry its start bit - checked BEFORE the quiet
        // FREE/MAG no-op returns, so a pending record whose start bit was
        // lost is a fatal in EVERY build, never a silent reject.
        if !table.reclaim_start_bit(base, offset) {
            fatal(b"alloc_core_small_reclaim: published record has start bit 0\n");
        }
        if state == crate::alloc_core::state_byte::StateByte::FREE {
            #[cfg(any(feature = "internals", feature = "bench-internals"))]
            crate::alloc_core::REMOTE_REJECT_FREE
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }
        // Round D1′ fix: MAG residency is a quiet counted no-op in EVERY
        // build (a correct MAG byte must never fatal and never accept);
        // builds without `fastbin` simply never produce state 1.
        if state == crate::alloc_core::state_byte::StateByte::MAG {
            #[cfg(any(feature = "internals", feature = "bench-internals"))]
            crate::alloc_core::REMOTE_REJECT_MAG
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }
        // Geometry stays PRODUCTION (the anti-weakening condition): the byte
        // now decides the bin class, so a corrupted class code must not
        // redirect a reclaim outside the segment's committed payload.
        let block_size = SizeClasses::block_size(class_idx);
        let mut meta = SegmentMeta::new(base);
        if off < payload_start
            || !off.is_multiple_of(block_size)
            || off
                .checked_add(block_size)
                .is_none_or(|end| end > meta.bump_of())
        {
            fatal(b"alloc_core_small_reclaim: sidecar record offset geometry invalid\n");
        }
        // D1′ production start-bit checks: the record's own start bit was
        // validated above (before the quiet FREE/MAG returns); the open
        // interval `(off, off + bs)` must contain NO start bits (no issued
        // neighbour may be spanned by a claimed block).
        let block_end = (off + block_size) as u32;
        #[cfg(feature = "bench-internals")]
        if Self::overlap_check_probe_call(
            table,
            base,
            offset + crate::alloc_core::size_classes::MIN_BLOCK as u32,
            block_end,
        ) {
            fatal(b"alloc_core_small_reclaim: claimed block overlaps an issued start\n");
        }
        #[cfg(not(feature = "bench-internals"))]
        if table.reclaim_start_overlap(
            base,
            offset + crate::alloc_core::size_classes::MIN_BLOCK as u32,
            block_end,
        ) {
            fatal(b"alloc_core_small_reclaim: claimed block overlaps an issued start\n");
        }
        let ptr = Node::deref(base, off);
        let mut bins = meta.bin_table();
        let old_head = bins.head(class_idx);
        let next = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        let block = NonNull::new(ptr).unwrap_or_else(|| {
            fatal(b"alloc_core_small_reclaim: reclaimed record resolves to null\n")
        });
        Node::write_next(block, next);
        bins.set_head(class_idx, offset);
        // B4 (в): commit LIVE → FREE in the state byte.
        meta.state_byte().write(
            offset,
            class_idx,
            crate::alloc_core::state_byte::StateByte::FREE,
        );
        meta.dec_live();
        Some(class_idx as u8)
    }
}
