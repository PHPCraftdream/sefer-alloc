//! B4 variant (в): ONE byte of owner-private STATE per `MIN_BLOCK` granule,
//! stored in the SEGMENT METADATA, addressed as
//! `base + Layout::state_byte_off() + (off >> MIN_BLOCK_SHIFT)` through the
//! existing safe [`Node::read_u8`]/[`Node::write_u8`] seam — ZERO `unsafe`
//! in this file. The byte REPLACED the former per-segment bitmaps
//! (`AllocBitmap`/`MagazineBitmap`, removed with B4) as the authoritative
//! small-segment state record; the state-byte region occupies their slot in
//! the metadata geometry (`Layout::state_byte_off()`, the former
//! `alloc_bitmap_off()`).

/// One byte of state per `MIN_BLOCK` granule of a small or primordial
/// segment. Owner-private: only the segment owner thread reads/writes it;
/// producers and other threads NEVER touch it (unlike the remote sidecar /
/// `ClassLeaves`, whose protocol is untouched here).
///
/// ## Encoding
///
/// ```text
/// byte = (class + 1) | (state << 6)
/// ```
///
/// - class bits: `class + 1 ∈ [1, SMALL_CLASS_COUNT]`, `SMALL_CLASS_COUNT ≤
///   63` (const-asserted below), so the class never bleeds into the state
///   bits and `byte == 0 ⇔ (byte & 0x3F) == 0`.
/// - state bits (2): [`StateByte::LIVE`] = 0 (issued to the caller, incl.
///   remote-pending), [`StateByte::MAG`] = 1 (resident in a tcache
///   magazine), [`StateByte::FREE`] = 2 (on a bin / free list). `state == 3`
///   is never produced.
/// - Bit 7 remains available to the remote `ClassLeaves` protocol. The separate
///   owner state array consumes its own bit 7 for `FREE`; leaf entries remain
///   encoded as `class + 1` and do not consume that bit.
///
/// ## Formal invariants
///
/// - **И1 (zero = never issued / metadata/interior):** every non-start
///   granule and every never-issued granule reads `0`. Equivalently, a byte
///   is nonzero iff its granule is the 16 B-aligned start of a block this
///   segment has handed out at least once since init. Any state-store
///   writes nonzero class bits, so `byte == 0 ⇔ (byte & 0x3F) == 0`.
/// - **И2 (LIVE exactness at operation boundaries):** for a nonzero state
///   byte, `state == LIVE` exactly on granules currently issued to the caller
///   (including remote-pending blocks), and nowhere else. A zero byte is
///   never LIVE; this invariant applies at completed-operation boundaries.
/// - **И3 (MAG exactness at operation boundaries):** for a nonzero state byte,
///   `state == MAG` exactly on granules currently resident in a tcache
///   magazine, and nowhere else; this invariant applies at completed-operation
///   boundaries.
/// - **И4 (FREE exactness at operation boundaries):** for a nonzero state byte,
///   `state == FREE` exactly on granules currently on a bin / free list of the
///   segment, and nowhere else; this invariant applies at completed-operation
///   boundaries.
/// - **И5 (transition exactness at operation boundaries):** every state
///   transition is committed by the owner before the operation that causes it
///   returns: alloc-issue commits MAG→LIVE, magazine push commits LIVE→MAG,
///   flush/free-to-bin commits LIVE|MAG→FREE, reclaim/carve commits FREE→LIVE
///   (via issue) or fresh-carve writes LIVE at first issue.
use crate::alloc_core::node::Node;
use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::segment_header::Layout;
use crate::alloc_core::size_classes::MIN_BLOCK;

/// `SMALL_CLASS_COUNT ≤ 63` — the class field of the state byte is 6 bits
/// wide.
const _: () = assert!(crate::alloc_core::size_classes::SMALL_CLASS_COUNT <= 63);

/// Shift from a payload offset to its granule index (`MIN_BLOCK == 16`).
const GRANULE_SHIFT: u32 = MIN_BLOCK.trailing_zeros();

#[derive(Clone, Copy)]
pub(crate) struct StateByte {
    /// First byte of the per-segment state array
    /// (`base + Layout::state_byte_off()`).
    bytes: *mut u8,
}

impl StateByte {
    /// Granule is at the user (or in a batch's in-flight output).
    pub(crate) const LIVE: u8 = 0;
    /// Granule is resident in a tcache magazine. Only the magazine
    /// (`alloc-global` + `fastbin`) writes or tests it.
    #[cfg_attr(
        not(all(feature = "alloc-global", feature = "fastbin")),
        allow(dead_code)
    )]
    pub(crate) const MAG: u8 = 1;
    /// Granule is on a bin free list.
    pub(crate) const FREE: u8 = 2;

    /// Byte footprint: one byte per `MIN_BLOCK` granule (256 KiB for the
    /// default 4 MiB segment).
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

    /// Read the state byte of the granule at payload offset `off`. A
    /// misaligned `off` maps to its START granule's byte — callers reject
    /// misalignment separately.
    #[inline(always)]
    pub(crate) fn read(&self, off: u32) -> u8 {
        Node::read_u8(Node::offset(self.bytes, (off >> GRANULE_SHIFT) as usize))
    }

    /// Write the state byte of the granule at payload offset `off`.
    #[inline(always)]
    pub(crate) fn write(&self, off: u32, class_idx: usize, state: u8) {
        Node::write_u8(
            Node::offset(self.bytes, (off >> GRANULE_SHIFT) as usize),
            Self::encode(class_idx, state),
        );
    }

    /// Rewrite ONLY the state bits of the granule's byte, preserving the
    /// class field. Used where the caller has no class index in hand
    /// (alloc-issue: the block was stored under its own class, and a
    /// LIVE/MAG byte is never zero — invariant И1 — so the class bits are
    /// already correct).
    // Single caller is `clear_magazine_on_issue`, gated behind `fastbin`;
    // without the feature this method is dead by design.
    #[cfg_attr(
        not(all(feature = "alloc-global", feature = "fastbin")),
        allow(dead_code)
    )]
    #[inline(always)]
    pub(crate) fn set_state(&self, off: u32, state: u8) {
        let p = Node::offset(self.bytes, (off >> GRANULE_SHIFT) as usize);
        let byte = Node::read_u8(p);
        Node::write_u8(p, (byte & 0x3F) | (state << 6));
    }

    /// Zero the whole array (the miri-only explicit init at the virgin
    /// bootstrap/reserve sites; the decommit-reset path zeroes a SCOPED
    /// range instead — see `decommit_empty_segment_impl`). Under
    /// `cfg(not(miri))` no caller remains (the virgin-reserve sites rely on
    /// the OS zero-page guarantee), so the method is cfg-gated out.
    #[cfg(miri)]
    pub(crate) fn init_in_place(&self) {
        let mut i = 0;
        while i < Self::FOOTPRINT {
            Node::write_u8(Node::offset(self.bytes, i), 0);
            i += 1;
        }
    }
}
