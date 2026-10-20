//! [`SegmentBitmap`](crate::alloc_core::segment_bitmap::SegmentBitmap) — the shared *mechanism* underlying the two per-segment
//! bitmaps ([`AllocBitmap`](crate::alloc_core::alloc_bitmap::AllocBitmap) and
//! [`MagazineBitmap`](crate::alloc_core::magazine_bitmap::MagazineBitmap)): one bit per
//! `MIN_BLOCK`-slot of the segment, single-writer (owner-thread-only, so plain
//! non-atomic byte reads/writes — see each wrapper's module doc for the
//! owner-only proof), "pure safe data + arithmetic" routing every raw memory
//! access through the [`node`](crate::alloc_core::node) seam. Zero `unsafe` here.
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

use crate::alloc_core::node::Node;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT};

/// The shared per-segment bitmap *mechanism*: one bit per `MIN_BLOCK`-slot of
/// the segment. A thin view over in-segment metadata carved by the bootstrap;
/// it owns no memory. Identical geometry and arithmetic for both
/// [`AllocBitmap`](crate::alloc_core::alloc_bitmap::AllocBitmap) and
/// [`MagazineBitmap`](crate::alloc_core::magazine_bitmap::MagazineBitmap).
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
    /// O(1): one `u64` word load + one mask (N4: whole-word access instead of a
    /// single byte — same memory representation, wider access width). The
    /// generic mechanism primitive; each wrapper gives it a domain name
    /// (`is_free` / `is_in_magazine`).
    #[inline(always)]
    pub(super) fn test(&self, off: u32) -> bool {
        let (word, mask) = Self::locate(off);
        Self::from_le(Node::read_struct(Self::offset_word(self.bits, word))) & mask != 0
    }

    /// Set the bit for the slot at segment offset `off`. O(1): word load + OR +
    /// store. Domain-named wrappers: `mark_free` / `mark_magazine`.
    #[inline(always)]
    pub(super) fn set(&mut self, off: u32) {
        let (word, mask) = Self::locate(off);
        let p = Self::offset_word(self.bits, word);
        let w = Self::from_le(Node::read_struct(p)) | mask;
        Node::write_struct(p, Self::to_le(w));
    }

    /// Clear the bit for the slot at segment offset `off`. O(1): word load +
    /// AND + store. Domain-named wrappers: `mark_alloc` / `clear_magazine`.
    #[inline(always)]
    pub(super) fn clear(&mut self, off: u32) {
        let (word, mask) = Self::locate(off);
        let p = Self::offset_word(self.bits, word);
        let w = Self::from_le(Node::read_struct(p)) & !mask;
        Node::write_struct(p, Self::to_le(w));
    }

    /// Map a segment offset to `(word_index, bit_mask)` within the bitmap. The
    /// bit index is `off >> MIN_BLOCK_SHIFT` (the `MIN_BLOCK`-slot number);
    /// word = bit / 64, mask = 1 << (bit % 64). Pure arithmetic.
    ///
    /// Bit-numbering compatibility: the byte layout is unchanged from the
    /// byte-addressed predecessor (bit `b` lived in byte `b >> 3` at bit
    /// `b & 7`). On little-endian a `u64` load/store of word `b / 64` addresses
    /// exactly those bytes in that order, so the in-memory representation is
    /// byte-for-byte identical (dump hooks and `init_in_place` — both still
    /// byte-wise — see the same layout). On big-endian the byte-to-bit mapping
    /// inside a loaded word is reversed, so the LE conversion helpers
    /// [`Self::from_le`] / [`Self::to_le`] / [`Self::offset_word`] restore the
    /// identical byte layout. Compile-time guard below pins this.
    #[inline(always)]
    fn locate(off: u32) -> (usize, u64) {
        let bit = (off >> MIN_BLOCK_SHIFT) as usize;
        debug_assert!(bit < Self::FOOTPRINT * 8, "bitmap bit index out of range");
        (bit >> 6, 1u64 << (bit & 63))
    }

    /// Byte offset of word `w` inside the bitmap (= `w * 8`); on big-endian
    /// `u64` scalars must not be stored raw (their byte order differs), so the
    /// word index is adjusted to address the same 8-byte group — which for a
    /// pure `w * 8` stride is the same offset on both endiannesses; the LE
    /// conversions on the VALUE (not the address) do the real work.
    #[inline(always)]
    fn offset_word(bits: *mut u8, word: usize) -> *mut u64 {
        // SAFETY-free arithmetic via Node::offset: `bits + word * 8`, within
        // the same allocation (word < FOOTPRINT / 8).
        Node::offset(bits, word * 8) as *mut u64
    }

    /// Load-side byte-order normalisation (identity on LE; byte-reverses the
    /// raw scalar on BE so bit `i` of the result is byte `i / 8`, bit `i % 8`).
    #[inline(always)]
    #[cfg(target_endian = "little")]
    fn from_le(w: u64) -> u64 {
        w
    }
    #[inline(always)]
    #[cfg(target_endian = "big")]
    fn from_le(w: u64) -> u64 {
        u64::from_le(w)
    }

    /// Store-side byte-order normalisation (inverse of [`Self::from_le`]).
    #[inline(always)]
    #[cfg(target_endian = "little")]
    fn to_le(w: u64) -> u64 {
        w
    }
    #[inline(always)]
    #[cfg(target_endian = "big")]
    fn to_le(w: u64) -> u64 {
        u64::to_le(w)
    }
}

// N4 compile-time guards: the word access is only byte-layout-compatible with
// the byte-addressed predecessor when the endianness handling above is intact.
// (u64::from_le/to_le handle BE; this assert documents the LE fast path.)
const _: () = assert!(
    cfg!(target_endian = "little") || cfg!(target_endian = "big"),
    "unsupported target endianness for SegmentBitmap word access"
);
