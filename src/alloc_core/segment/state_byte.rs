//! B4 Шаг S-v (`small-state-byte`, variant (в) of
//! `.rush/art/b4/state_byte_design.md`): ONE byte of owner-private STATE per
//! `MIN_BLOCK` granule, stored in the SEGMENT METADATA (not in per-leaf
//! `ClassLeaves`), addressed as `root + Layout::state_byte_off() + (off >> 4)`
//! through the existing safe [`Node::read_u8`]/[`Node::write_u8`] seam —
//! exactly like the two bitmaps. ZERO `unsafe`.
//!
//! ## Encoding (state_byte_design.md §2.1)
//!
//! ```text
//! byte = (class + 1) | (state << 6)
//! ```
//!
//! - class bits: `class + 1 ∈ [1, SMALL_CLASS_COUNT]`, `SMALL_CLASS_COUNT ≤
//!   63` (const-asserted below), so the class never bleeds into the state
//!   bits and `byte == 0 ⇔ (byte & 0x3F) == 0` (invariant И1: every owner
//!   store writes nonzero class bits, so a ZERO byte always means
//!   "metadata / interior of a live block / never issued" — reject).
//! - state bits (2): [`StateByte::LIVE`] = 0 (block at the user, or in a
//!   batch's in-flight output), [`StateByte::MAG`] = 1 (block resident in a
//!   tcache magazine), [`StateByte::FREE`] = 2 (block on a segment bin's
//!   free list). `state == 3` is not produced by any writer.
//! - Bit 7 stays free for the item-246 1-bit-epoch candidate (the same
//!   reasoning that let `б″` keep it — §6.5 Q2).
//!
//! ## Writers / protocol (§2.1 И2–И4)
//!
//! - LIVE: substrate issue to the user — `pop_free`, `drain`, `carve_block`,
//!   `carve_batch` (the same sites as `AllocBitmap::mark_alloc`), and the
//!   heap issue path `HeapCore::clear_magazine_on_issue` (MAG → LIVE).
//! - MAG: magazine push on free (scalar + batch) and the refill tail
//!   (`finish_magazine_refill`, alongside the `mark_magazine` loop that
//!   stays — the magazine bitmap still has readers: drain / reclaim /
//!   `class_cursor` scans).
//! - FREE: bin insertion — `dealloc_small` (which every `flush_class`
//!   staging path funnels through), `flush_run`, `reclaim`.
//!
//! The bytes are owner-private and are NEVER read by producers / other
//! threads (unlike the `ClassLeaves` leaves, whose protocol is untouched —
//! `small_sidecar.rs` keeps its own class bytes). The array lives in
//! metadata BEHIND every published-protocol region (`+256 KiB` per 4 MiB
//! segment ⇒ capacity −6.25 % brutto / ≈−4.7 % net of the touched small
//! payload — the RSS verdict accepted by the owner for S-v).
//!
//! Fresh segments: the OS hands back zero pages, which is exactly the
//! "never issued" encoding; under `miri` (and in the decommit reset, which
//! re-initialises the bitmaps explicitly) the array is zeroed explicitly,
//! mirroring `AllocBitmap::init_in_place`'s discipline.

use crate::alloc_core::node::Node;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::segment_header::Layout;
use crate::alloc_core::size_classes::MIN_BLOCK;

/// `SMALL_CLASS_COUNT ≤ 63` — the class field of the state byte is 6 bits
/// wide (state_byte_design.md §2.1 / TS5).
const _: () = assert!(crate::alloc_core::size_classes::SMALL_CLASS_COUNT <= 63);

/// One byte of state per `MIN_BLOCK` granule of the segment.
#[derive(Clone, Copy)]
pub(crate) struct StateByte {
    /// First byte of the per-segment state array
    /// (`base + Layout::state_byte_off()`).
    bytes: *mut u8,
}

impl StateByte {
    /// Granule is at the user (or in a batch's in-flight output).
    pub(crate) const LIVE: u8 = 0;
    /// Granule is resident in a tcache magazine.
    pub(crate) const MAG: u8 = 1;
    /// Granule is on a bin free list.
    pub(crate) const FREE: u8 = 2;

    /// Byte footprint: one byte per `MIN_BLOCK` granule.
    pub(crate) const FOOTPRINT: usize = SEGMENT / MIN_BLOCK;

    /// Encode a full state byte for `class_idx` in `state`.
    #[inline(always)]
    pub(crate) const fn encode(class_idx: usize, state: u8) -> u8 {
        (class_idx as u8 + 1) | (state << 6)
    }

    /// View over the state bytes of the segment whose base is `base`.
    #[inline(always)]
    pub(crate) fn new(base: *mut u8) -> Self {
        Self {
            bytes: Node::offset(base, Layout::state_byte_off()),
        }
    }

    /// Read the state byte of the granule at payload offset `off`
    /// (`off` need not be aligned: the index is `off >> MIN_BLOCK_SHIFT`,
    /// so a misaligned free reads its start granule's byte — the guard's
    /// separate alignment conjunct rejects it).
    #[inline(always)]
    pub(crate) fn read(&self, off: u32) -> u8 {
        Node::read_u8(Node::offset(self.bytes, (off >> 4) as usize))
    }

    /// Write the state byte of the granule at payload offset `off`.
    #[inline(always)]
    pub(crate) fn write(&self, off: u32, class_idx: usize, state: u8) {
        Node::write_u8(
            Node::offset(self.bytes, (off >> 4) as usize),
            Self::encode(class_idx, state),
        );
    }

    /// Rewrite ONLY the state bits of the granule's byte, preserving the
    /// class field. Used where the caller has no class index in hand
    /// (`clear_magazine_on_issue`): the block was issued under its own
    /// class, so the byte's class bits are already correct (a LIVE/MAG
    /// byte is never zero — invariant И1 — and the class never changes for
    /// a given block).
    #[inline(always)]
    pub(crate) fn set_state(&self, off: u32, state: u8) {
        let p = Node::offset(self.bytes, (off >> 4) as usize);
        let byte = Node::read_u8(p);
        Node::write_u8(p, (byte & 0x3F) | (state << 6));
    }

    /// Zero the whole array (explicit initialisation where the OS-zero
    /// guarantee does not hold: `miri`, and mirrors of the bitmaps' own
    /// `init_in_place` discipline).
    pub(crate) fn init_in_place(&self) {
        let mut i = 0;
        while i < Self::FOOTPRINT {
            Node::write_u8(Node::offset(self.bytes, i), 0);
            i += 1;
        }
    }
}
