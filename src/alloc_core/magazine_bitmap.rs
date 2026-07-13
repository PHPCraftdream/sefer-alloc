//! [`MagazineBitmap`] — RAD-5 (plan Phase 5-E4), **verdict: GO** (see
//! `docs/perf/IAI_BASELINE.md` §RAD-5 for the measurement). This bitmap IS
//! wired into the production hot path — it is compiled unconditionally (no
//! feature flag) and probed in O(1) on the own-thread free double-free oracle
//! and the cross-class magazine predicate, replacing the O(count) slot scans
//! those paths used before RAD-5.
//!
//! A second, orthogonal per-segment bitmap: one bit per `MIN_BLOCK`-slot,
//! recording whether the block starting at that slot is currently RESIDENT IN
//! THE OWNER'S MAGAZINE (per-thread tcache), as opposed to
//! [`AllocBitmap`](super::alloc_bitmap::AllocBitmap), which records FREE vs
//! ALLOCATED. This bitmap's state is set at magazine push (own-thread free)
//! and cleared at magazine pop (alloc hit / refill issue) or magazine flush.
//!
//! ## Why a second bitmap, not a redefinition of `AllocBitmap`
//!
//! The G1 honest-reject (2026-07-10, `IAI_BASELINE.md`) rejected inverting
//! `AllocBitmap`'s existing semantics ("bit = free") into "bit = not owned by
//! user", because that inversion silently breaks four `mark_alloc` call sites
//! (freelist-drain legs assume "leaves the free list ⇒ handed to caller",
//! false once the destination can be the magazine) and `carve_batch`'s
//! deliberate leave-unset optimization. This bitmap sidesteps that
//! entirely: `AllocBitmap`'s semantics and every one of its call sites are
//! UNCHANGED. This is orthogonal, additional state.
//!
//! ## Semantics
//!
//! - Bit `1` = the block is currently sitting in the owner's per-class
//!   magazine (tcache slot). Bit `0` = not magazine-resident (allocated to
//!   the user, on a BinTable free list, or not a block start).
//! - Mark (`mark_magazine`): magazine push (own-thread free pushes the block
//!   onto `tcache.classes[c].slots`).
//! - Clear (`clear_magazine`): magazine pop (alloc hit, or a refill's final
//!   issue-to-caller pop) OR magazine flush (a flushed block leaves the
//!   magazine for the BinTable free list — flush already calls
//!   `AllocBitmap::mark_free`; this bitmap is cleared in the same breath).
//! - Refill from the BinTable free list into the magazine: `AllocBitmap`
//!   already runs `mark_alloc` (unchanged); this bitmap additionally runs
//!   `mark_magazine` for every block that lands in the magazine (not the one
//!   immediately popped back out to the caller).
//! - Direct bump-carve straight to the caller (never touching the magazine)
//!   does not mark this bitmap at all — mirrors the existing
//!   `carve_batch` leave-unset optimization on `AllocBitmap`.
//!
//! ## Owner-only, no atomics
//!
//! Exactly like `AllocBitmap`: only the segment's owning thread ever writes
//! this bitmap (magazine push/pop/flush/refill are all owner-thread
//! operations by construction — the magazine itself is a per-`HeapCore`,
//! single-writer structure). Cross-thread frees never touch it directly;
//! they are visible to the owner only after a ring-drain, which runs on the
//! owner thread.
//!
//! ## Dedup (task #98 / R4-6)
//!
//! The bitmap MECHANISM (the `bits` field, `FOOTPRINT`, `new`, `init_in_place`,
//! `locate`, bit test / set / clear) is identical to
//! [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) — both are one-bit-per-
//! `MIN_BLOCK`-slot single-writer views — so it lives once in the private
//! [`SegmentBitmap`](super::segment_bitmap::SegmentBitmap). This type is a thin
//! newtype wrapper that exposes ONLY the magazine-residency domain-named methods
//! (`mark_magazine` / `clear_magazine` / `is_in_magazine`), so the two bitmap
//! KINDS cannot be confused at a call site. Every method stays
//! `#[inline(always)]` and forwards trivially, so generated code is unchanged
//! (zero Ir delta on the hot path — see `docs/perf/IAI_BASELINE.md` and
//! `npm run iai`).
//!
//! ## This file is PURE SAFE DATA + ARITHMETIC
//!
//! Every raw memory touch goes through the [`node`](super::node) seam — now via
//! [`SegmentBitmap`](super::segment_bitmap::SegmentBitmap), exactly like
//! [`AllocBitmap`](super::alloc_bitmap::AllocBitmap). There is NO `unsafe` here.

use super::segment_bitmap::SegmentBitmap;

/// The per-segment magazine-residency bitmap view: one bit per `MIN_BLOCK`
/// slot of the segment. A thin newtype over the shared
/// [`SegmentBitmap`](super::segment_bitmap::SegmentBitmap) mechanism; it owns no
/// memory. Carved at
/// [`Layout::magazine_bitmap_off`](super::segment_header::Layout::magazine_bitmap_off);
/// mirrors [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) with orthogonal
/// semantics (see module doc).
#[repr(transparent)]
pub(crate) struct MagazineBitmap(SegmentBitmap);

impl MagazineBitmap {
    /// The byte footprint of the bitmap in a segment. Re-exported from
    /// [`SegmentBitmap::FOOTPRINT`] so call sites keep using
    /// `MagazineBitmap::FOOTPRINT` unchanged. Same geometry as
    /// `AllocBitmap::FOOTPRINT`: one bit per `MIN_BLOCK` slot of the whole
    /// segment, rounded to whole bytes. 32 768 bytes (8 pages) for the default
    /// 4 MiB / 16 B pair.
    pub(crate) const FOOTPRINT: usize = SegmentBitmap::FOOTPRINT;

    /// Construct the view over an already-laid-down bitmap at `bits`.
    #[inline(always)]
    pub(crate) fn new(bits: *mut u8) -> Self {
        Self(SegmentBitmap::new(bits))
    }

    /// Initialise a fresh bitmap at `bits`: ALL ZEROS ("not magazine-resident").
    /// `bits` MUST point to [`FOOTPRINT`](Self::FOOTPRINT) writable bytes
    /// inside the segment being initialised (caller's contract — the
    /// bootstrap / decommit-reset).
    ///
    /// RAD-5: mirrors `AllocBitmap::init_in_place`'s virgin-skip discipline —
    /// see the call sites in `bootstrap.rs` / `alloc_core_small.rs` /
    /// `alloc_core_small_pool.rs` for the `cfg(not(miri))` elision on
    /// freshly-reserved (never touched) segments, and the unconditional call
    /// on decommit-reset (a non-virgin segment).
    #[cfg_attr(all(not(miri), not(feature = "alloc-decommit")), allow(dead_code))]
    pub(crate) fn init_in_place(bits: *mut u8) {
        SegmentBitmap::init_in_place(bits)
    }

    /// Whether the block at segment offset `off` is currently magazine-resident.
    /// O(1): one byte load + one mask. This is the O(1) replacement for the
    /// O(count) in-magazine scan (own-thread free path) / cross-class scan
    /// (`reclaim_offset_checked`'s `is_in_magazine` predicate).
    #[inline(always)]
    pub(crate) fn is_in_magazine(&self, off: u32) -> bool {
        self.0.test(off)
    }

    /// Mark the block at segment offset `off` as magazine-resident (set its
    /// bit). Called on magazine push (own-thread free) and on refill for
    /// every block landing in the magazine (not the one immediately issued).
    #[inline(always)]
    pub(crate) fn mark_magazine(&mut self, off: u32) {
        self.0.set(off)
    }

    /// Clear the block at segment offset `off`'s magazine-resident bit.
    /// Called on magazine pop (alloc hit / refill issue) and magazine flush.
    #[inline(always)]
    pub(crate) fn clear_magazine(&mut self, off: u32) {
        self.0.clear(off)
    }
}
