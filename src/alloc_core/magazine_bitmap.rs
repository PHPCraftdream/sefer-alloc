//! [`MagazineBitmap`] — RAD-5 (E4) GO/NO-GO EXPERIMENT. **Not wired into the
//! production hot path if this ledger entry records a NO-GO** — see
//! `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the measured verdict before
//! reusing anything in this file.
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
//! ## This file is PURE SAFE DATA + ARITHMETIC
//!
//! Every raw memory touch goes through the [`node`](super::node) seam,
//! exactly like [`AllocBitmap`](super::alloc_bitmap::AllocBitmap). There is
//! NO `unsafe` here.

use super::node::Node;
use super::os::SEGMENT;
use super::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT};

/// The per-segment magazine-residency bitmap view: one bit per `MIN_BLOCK`
/// slot of the segment. A thin view over in-segment metadata carved at
/// [`Layout::magazine_bitmap_off`](super::segment_header::Layout::magazine_bitmap_off);
/// it owns no memory. Mirrors [`AllocBitmap`](super::alloc_bitmap::AllocBitmap)
/// exactly, with orthogonal semantics (see module doc).
pub(crate) struct MagazineBitmap {
    /// Absolute address of the first bitmap byte.
    bits: *mut u8,
}

impl MagazineBitmap {
    /// Same geometry as `AllocBitmap::FOOTPRINT`: one bit per `MIN_BLOCK`
    /// slot of the whole segment, rounded to whole bytes. 32 768 bytes (8
    /// pages) for the default 4 MiB / 16 B pair.
    pub(crate) const FOOTPRINT: usize = SEGMENT / MIN_BLOCK / 8;

    /// Construct the view over an already-laid-down bitmap at `bits`.
    #[inline(always)]
    pub(crate) fn new(bits: *mut u8) -> Self {
        Self { bits }
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
        let mut i = 0;
        while i < Self::FOOTPRINT {
            Node::write_u8(Node::offset(bits, i), 0);
            i += 1;
        }
    }

    /// Whether the block at segment offset `off` is currently magazine-resident.
    /// O(1): one byte load + one mask. This is the O(1) replacement for the
    /// O(count) in-magazine scan (own-thread free path) / cross-class scan
    /// (`reclaim_offset_checked`'s `is_in_magazine` predicate).
    #[inline(always)]
    pub(crate) fn is_in_magazine(&self, off: u32) -> bool {
        let (byte_idx, mask) = Self::locate(off);
        let byte = Node::read_u8(Node::offset(self.bits, byte_idx));
        (byte & mask) != 0
    }

    /// Mark the block at segment offset `off` as magazine-resident (set its
    /// bit). Called on magazine push (own-thread free) and on refill for
    /// every block landing in the magazine (not the one immediately issued).
    #[inline(always)]
    pub(crate) fn mark_magazine(&mut self, off: u32) {
        let (byte_idx, mask) = Self::locate(off);
        let p = Node::offset(self.bits, byte_idx);
        let byte = Node::read_u8(p);
        Node::write_u8(p, byte | mask);
    }

    /// Clear the block at segment offset `off`'s magazine-resident bit.
    /// Called on magazine pop (alloc hit / refill issue) and magazine flush.
    #[inline(always)]
    pub(crate) fn clear_magazine(&mut self, off: u32) {
        let (byte_idx, mask) = Self::locate(off);
        let p = Node::offset(self.bits, byte_idx);
        let byte = Node::read_u8(p);
        Node::write_u8(p, byte & !mask);
    }

    /// Map a segment offset to `(byte_index, bit_mask)`. Identical arithmetic
    /// to `AllocBitmap::locate`.
    #[inline(always)]
    fn locate(off: u32) -> (usize, u8) {
        let bit = (off >> MIN_BLOCK_SHIFT) as usize;
        debug_assert!(bit < Self::FOOTPRINT * 8, "bitmap bit index out of range");
        (bit >> 3, 1u8 << (bit & 7))
    }
}
