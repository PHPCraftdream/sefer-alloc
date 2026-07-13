//! [`SegmentBitmap`] — the shared *mechanism* underlying the two per-segment
//! bitmaps ([`AllocBitmap`](super::alloc_bitmap::AllocBitmap) and
//! [`MagazineBitmap`](super::magazine_bitmap::MagazineBitmap)): one bit per
//! `MIN_BLOCK`-slot of the segment, single-writer (owner-thread-only, so plain
//! non-atomic byte reads/writes — see each wrapper's module doc for the
//! owner-only proof), "pure safe data + arithmetic" routing every raw memory
//! access through the [`node`](super::node) seam. Zero `unsafe` here.
//!
//! Both wrappers are byte-for-byte identical in MECHANISM (the `bits` field,
//! the `FOOTPRINT` byte-footprint, `new`, `init_in_place`, `locate`, bit test /
//! set / clear); they differ ONLY in domain SEMANTICS and method naming (free
//! vs magazine-resident). This type is the single copy of that mechanism (task
//! #98 / R4-6, dedup of `code_quality_review.md` finding #7). It is intentionally
//! private to `alloc_core` and uses generic mechanism names (`test`/`set`/
//! `clear`); the two public wrappers layer domain-specific semantics and method
//! names on top so the two bitmap KINDS still cannot be confused at a call
//! site (which is the point of keeping them as distinct newtypes rather than
//! merging outright).
//!
//! ## HOT PATH — zero codegen change
//!
//! Every method is `#[inline(always)]` exactly as aggressively as the original
//! duplicated copies, and each wrapper's domain-named method is itself
//! `#[inline(always)]` and forwards trivially, so the generated code is
//! byte-for-byte identical to the pre-dedup state (verified by `npm run iai`:
//! zero Ir delta on every hot-path bench). No indirection, no `dyn`, no closure.

use super::node::Node;
use super::os::SEGMENT;
use super::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT};

/// The shared per-segment bitmap *mechanism*: one bit per `MIN_BLOCK`-slot of
/// the segment. A thin view over in-segment metadata carved by the bootstrap;
/// it owns no memory. Identical geometry and arithmetic for both
/// [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) and
/// [`MagazineBitmap`](super::magazine_bitmap::MagazineBitmap).
#[repr(transparent)]
pub(super) struct SegmentBitmap {
    /// Absolute address of the first bitmap byte (stored absolute so reads need
    /// no segment-base arithmetic, like `PageMap`/`BinTable`).
    bits: *mut u8,
}

impl SegmentBitmap {
    /// The byte footprint of the bitmap in a segment: one bit per `MIN_BLOCK`
    /// slot of the whole segment, rounded to whole bytes. For the default
    /// 4 MiB / 16 B pair this is `4 MiB / 16 / 8 = 32 768` bytes (8 pages).
    /// **Computed from the constants** so it cannot drift if `SEGMENT` /
    /// `MIN_BLOCK` change.
    pub(super) const FOOTPRINT: usize = SEGMENT / MIN_BLOCK / 8;

    /// Construct the view over an already-laid-down bitmap at `bits`. The
    /// bootstrap calls this AFTER zeroing the bytes via [`init_in_place`].
    ///
    /// [`init_in_place`]: Self::init_in_place
    #[inline(always)]
    pub(super) fn new(bits: *mut u8) -> Self {
        Self { bits }
    }

    /// Initialise a fresh bitmap at `bits`: ALL ZEROS ("nothing set"). Routes
    /// every byte write through [`Node::write_u8`]. `bits` MUST point to
    /// [`FOOTPRINT`](Self::FOOTPRINT) writable bytes inside the segment being
    /// initialised (caller's contract — the bootstrap / decommit-reset).
    ///
    /// Not `#[inline(always)]` (matching the original duplicated copies): this
    /// is a cold init path, never the free/alloc fast path. Whether it inlines
    /// at its (few) call sites is unchanged by the dedup — both wrappers forward
    /// through this one body.
    #[cfg_attr(all(not(miri), not(feature = "alloc-decommit")), allow(dead_code))]
    pub(super) fn init_in_place(bits: *mut u8) {
        let mut i = 0;
        while i < Self::FOOTPRINT {
            Node::write_u8(Node::offset(bits, i), 0);
            i += 1;
        }
    }

    /// Whether the bit for the `MIN_BLOCK`-slot at segment offset `off` is set.
    /// O(1): one byte load + one mask. The generic mechanism primitive; each
    /// wrapper gives it a domain name (`is_free` / `is_in_magazine`).
    #[inline(always)]
    pub(super) fn test(&self, off: u32) -> bool {
        let (byte_idx, mask) = Self::locate(off);
        let byte = Node::read_u8(Node::offset(self.bits, byte_idx));
        (byte & mask) != 0
    }

    /// Set the bit for the slot at segment offset `off`. O(1): byte load + OR +
    /// store. Domain-named wrappers: `mark_free` / `mark_magazine`.
    #[inline(always)]
    pub(super) fn set(&mut self, off: u32) {
        let (byte_idx, mask) = Self::locate(off);
        let p = Node::offset(self.bits, byte_idx);
        let byte = Node::read_u8(p);
        Node::write_u8(p, byte | mask);
    }

    /// Clear the bit for the slot at segment offset `off`. O(1): byte load +
    /// AND + store. Domain-named wrappers: `mark_alloc` / `clear_magazine`.
    #[inline(always)]
    pub(super) fn clear(&mut self, off: u32) {
        let (byte_idx, mask) = Self::locate(off);
        let p = Node::offset(self.bits, byte_idx);
        let byte = Node::read_u8(p);
        Node::write_u8(p, byte & !mask);
    }

    /// Map a segment offset to `(byte_index, bit_mask)` within the bitmap. The
    /// bit index is `off >> MIN_BLOCK_SHIFT` (the `MIN_BLOCK`-slot number);
    /// byte = bit / 8, mask = 1 << (bit % 8). Pure arithmetic.
    #[inline(always)]
    fn locate(off: u32) -> (usize, u8) {
        let bit = (off >> MIN_BLOCK_SHIFT) as usize;
        debug_assert!(bit < Self::FOOTPRINT * 8, "bitmap bit index out of range");
        (bit >> 3, 1u8 << (bit & 7))
    }
}
