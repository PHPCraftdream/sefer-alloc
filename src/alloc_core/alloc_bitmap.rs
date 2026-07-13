//! [`AllocBitmap`] — the per-segment **O(1) exact double-free guard**: one bit
//! per `MIN_BLOCK`-slot of the segment, recording whether the block starting at
//! that slot is currently FREE (sitting in one of the segment's free lists) or
//! ALLOCATED / not-a-block-start.
//!
//! ## Why a bitmap (Phase 13.4a)
//!
//! The Phase 8 double-free guard walked the class free list on every own-thread
//! free (`free_list_contains`) — O(free-list length). On a churn workload that
//! frees N blocks of one class into one segment the free list grows 0→N, so the
//! walk is **O(N²)** (the bench regression #41: 16 B churn ballooned to ~1.9 ms
//! vs mimalloc's ~11 µs). This bitmap makes the guard **O(1) and exact**: a
//! single bit test/set per free. Unlike a block canary it never false-positives
//! (user data can equal any canary), so it satisfies M2 precisely: a double-free
//! is a no-op, never a self-loop / double-issue / corruption.
//!
//! ## Semantics
//!
//! - Bit `b = off >> MIN_BLOCK_SHIFT` covers the `MIN_BLOCK`-slot at segment
//!   offset `off`. Block starts are always `MIN_BLOCK`-aligned (carve aligns the
//!   bump to `block_size`, a multiple of `MIN_BLOCK`), so each block maps to a
//!   unique bit. The bitmap covers the WHOLE segment (including its metadata
//!   region) — the metadata bits are simply never touched (no block starts
//!   there), which avoids any payload-start subtraction.
//! - Bit `1` = FREE (in some free list of this segment: `free` / `local_free` /
//!   reclaimed). Bit `0` = allocated, or not a block start. Fresh init is all
//!   zeros ("everything allocated / not-a-block").
//!
//! ## Dedup (task #98 / R4-6)
//!
//! The bitmap MECHANISM (the `bits` field, `FOOTPRINT`, `new`, `init_in_place`,
//! `locate`, bit test / set / clear) is identical to
//! [`MagazineBitmap`](super::magazine_bitmap::MagazineBitmap) — both are one-bit-
//! per-`MIN_BLOCK`-slot single-writer views — so it lives once in the private
//! [`SegmentBitmap`](super::segment_bitmap::SegmentBitmap). This type is a thin
//! newtype wrapper that exposes ONLY the free-vs-allocated domain-named methods
//! (`is_free` / `mark_free` / `mark_alloc`), so the two bitmap KINDS cannot be
//! confused at a call site. Every method stays `#[inline(always)]` and forwards
//! trivially, so generated code is unchanged (zero Ir delta on the hot path —
//! see `docs/perf/IAI_BASELINE.md` and `npm run iai`).
//!
//! ## This file is PURE SAFE DATA + ARITHMETIC
//!
//! Every raw memory touch goes through the [`node`](super::node) seam (exactly
//! like [`PageMap`](super::segment_header::PageMap) /
//! [`BinTable`](super::segment_header::BinTable)) — now via
//! [`SegmentBitmap`](super::segment_bitmap::SegmentBitmap). There is NO `unsafe`
//! here.
//!
//! ## No atomics (single-writer)
//!
//! A segment's bitmap is written ONLY by the segment's owner: own-thread frees
//! and the owner-side `reclaim_offset` drain both run on the owner. Cross-thread
//! frees never touch the bitmap — they go through the
//! [`RemoteFreeRing`](super::remote_free_ring::RemoteFreeRing) (offsets only)
//! and the owner sets the bit when it drains. So plain (non-atomic) byte
//! reads/writes are race-free, matching the `bump`-cursor single-writer rule.

use super::segment_bitmap::SegmentBitmap;

/// The per-segment allocation/free bitmap view: one bit per `MIN_BLOCK`-slot of
/// the segment. A thin newtype over the shared
/// [`SegmentBitmap`](super::segment_bitmap::SegmentBitmap) mechanism; it owns no
/// memory. Carved by the bootstrap at
/// [`Layout::alloc_bitmap_off`](super::segment_header::Layout::alloc_bitmap_off).
#[repr(transparent)]
pub(crate) struct AllocBitmap(SegmentBitmap);

impl AllocBitmap {
    /// The byte footprint of the bitmap in a segment. Re-exported from
    /// [`SegmentBitmap::FOOTPRINT`] so call sites keep using
    /// `AllocBitmap::FOOTPRINT` unchanged.
    pub(crate) const FOOTPRINT: usize = SegmentBitmap::FOOTPRINT;

    /// Construct the view over an already-laid-down bitmap at `bits`. The
    /// bootstrap calls this AFTER zeroing the bytes via [`init_in_place`].
    ///
    /// [`init_in_place`]: Self::init_in_place
    #[inline(always)]
    pub(crate) fn new(bits: *mut u8) -> Self {
        Self(SegmentBitmap::new(bits))
    }

    /// Initialise a fresh bitmap at `bits`: ALL ZEROS (every slot
    /// "allocated / not-a-block-start"). `bits` MUST point to
    /// [`FOOTPRINT`](Self::FOOTPRINT) writable bytes inside the segment being
    /// initialised (caller's contract — the bootstrap).
    ///
    /// PERF-PASS-2 (G5/C1, task #50): the two virgin-reserve call sites
    /// (`bootstrap::primordial`, `AllocCore::reserve_small_segment`) now skip
    /// calling this under `cfg(not(miri))` — see their doc comments — so under
    /// a non-miri build WITHOUT `alloc-decommit` (whose
    /// `decommit_empty_segment_impl` full-reset is the only remaining
    /// unconditional caller) this function is legitimately unreachable. The
    /// `cfg_attr` below silences that specific, expected case; under `miri` or
    /// `alloc-decommit` it IS called and the lint stays live.
    #[cfg_attr(all(not(miri), not(feature = "alloc-decommit")), allow(dead_code))]
    pub(crate) fn init_in_place(bits: *mut u8) {
        SegmentBitmap::init_in_place(bits)
    }

    /// Whether the block at segment offset `off` is currently marked FREE
    /// (its bit is set). O(1): one byte load + one mask. This is the M2
    /// double-free test — `true` means the block is already on a free list.
    #[inline(always)]
    pub(crate) fn is_free(&self, off: u32) -> bool {
        self.0.test(off)
    }

    /// Mark the block at segment offset `off` as FREE (set its bit). Called when
    /// the block is pushed onto a free list. O(1): byte load + OR + store.
    #[inline(always)]
    pub(crate) fn mark_free(&mut self, off: u32) {
        self.0.set(off)
    }

    /// Mark the block at segment offset `off` as ALLOCATED (clear its bit).
    /// Called when the block is popped from a free list and handed out. O(1):
    /// byte load + AND + store.
    #[inline(always)]
    pub(crate) fn mark_alloc(&mut self, off: u32) {
        self.0.clear(off)
    }
}
