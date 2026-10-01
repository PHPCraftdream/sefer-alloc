//! [`NextTable`] — the PG-2 spike's **off-body per-slot freelist `next`**
//! array: one `u32` per `MIN_BLOCK` slot of the segment, holding the offset of
//! the NEXT free block in the same class's chain (or
//! [`NextTable::NULL`] for the tail). This replaces the intrusive
//! `Node::write_next`/`read_next` body access: a retired block's chain
//! pointer lives in SEGMENT METADATA, never in the block body, so neither
//! retire nor pop touches user-visible bytes (ADR
//! `docs/design/2026-10-01-adr-physical-boundary-and-progress.md`, решение п.1).
//!
//! Chain semantics are byte-identical to the intrusive era: per-class LIFO
//! with the head in `BinTable[class]`; the only change is WHERE `next` lives.
//! The table is single-writer (owner-only), like the bitmaps; plain
//! non-atomic `u32` access is race-free. Slots for never-carved offsets stay
//! NULL (fresh OS pages read zero; miri keeps the explicit zero-init, exactly
//! like [`crate::alloc_core::alloc_bitmap::AllocBitmap`]).
//!
//! No `unsafe` here: every raw touch goes through the
//! [`node`](crate::alloc_core::node) seam (`Node::read_u32_unaligned` /
//! `Node::write_u32_unaligned`).

use crate::alloc_core::node::Node;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT};

/// The off-body freelist `next` table. A thin safe view over in-segment
/// metadata carved at
/// [`Layout::next_table_off`](crate::alloc_core::segment_header::Layout::next_table_off).
#[repr(transparent)]
pub(crate) struct NextTable {
    entries: *mut u32,
}

impl NextTable {
    /// The sentinel stored in a tail slot (and in every never-carved slot).
    #[allow(dead_code)] // PG-2 spike: consumed by init_in_place (miri-only)
    pub(crate) const NULL: u32 = u32::MAX;

    /// The byte footprint: one `u32` per `MIN_BLOCK` slot of the whole
    /// segment (1 MiB for the default 4 MiB / 16 B pair). SPIKE NOTE: this is
    /// a naive layout chosen for proof-of-feasibility, not for RSS; a real
    /// design would share slots via a per-class leaf geometry (ADR §Решение).
    pub(crate) const FOOTPRINT: usize = (SEGMENT / MIN_BLOCK) * 4;

    /// Construct the view over the table laid down at `entries`.
    #[inline(always)]
    pub(crate) fn new(entries: *mut u32) -> Self {
        Self { entries }
    }

    /// Initialise a fresh table: ALL NULL ("no free chain anywhere"). Same
    /// virgin-skip discipline as the bitmaps: call sites run this only under
    /// `cfg(miri)` (fresh OS pages read zero).
    #[cfg_attr(not(miri), allow(dead_code))] // miri-only (virgin-skip on real OS pages)
    pub(crate) fn init_in_place(entries: *mut u8) {
        // One `write_bytes(0)` — every zero u32 is the NULL sentinel.
        Node::zero(entries, Self::FOOTPRINT);
    }

    /// Map a block offset to its slot entry pointer.
    #[inline(always)]
    fn slot(&self, off: u32) -> *mut u32 {
        let idx = (off >> MIN_BLOCK_SHIFT) as usize;
        Node::offset(self.entries as *mut u8, idx * 4) as *mut u32
    }

    /// Read the `next` offset for the block at `off` (`NULL` for the tail).
    #[inline(always)]
    pub(crate) fn next(&self, off: u32) -> u32 {
        Node::read_u32_unaligned(self.slot(off))
    }

    /// Store the `next` offset for the block at `off`.
    #[inline(always)]
    pub(crate) fn set_next(&mut self, off: u32, next: u32) {
        Node::write_u32_unaligned(self.slot(off), next);
    }
}
