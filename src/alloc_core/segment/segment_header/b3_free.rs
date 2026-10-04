//! B3 spike (`ph3c-b3s`): per-class masks of non-empty 4 KiB sub-leaves.
#![allow(unsafe_code)] // tier-1 seam: raw mask/bind/cursor words via the node seam.
//!
//! The free set of a Small segment is the existing [`AllocBitmap`]
//! (bit = MIN_BLOCK slot is FREE). This seam adds the owner-only index over it:
//! one bit per 4 KiB sub-leaf (256 slots = exactly 4 bitmap words) per class,
//! plus the first-carve sub-leaf→class binding and a scan cursor. No atomic —
//! single-writer (segment owner), like `bump`.
//!
//! Sub-leaf `s` covers bitmap words `[s*4, s*4+4)`. Class masks live in the
//! segment metadata region carved at [`crate::alloc_core::segment_header::Layout::b3_free_off`].

use crate::alloc_core::node::Node;
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

/// Bytes in one sub-leaf (256 slots × 16 B). `AllocBitmap` covers it with
/// exactly 4 u64 words.
pub(crate) const B3_LEAF_BYTES: usize = 256 * crate::alloc_core::size_classes::MIN_BLOCK;
/// Sub-leaves per segment.
pub(crate) const B3_LEAVES: usize = crate::alloc_core::os::SEGMENT / B3_LEAF_BYTES;
/// u64 words of the alloc bitmap per sub-leaf.
pub(crate) const B3_LEAF_WORDS: usize = 4;
/// u64 mask words per class (one bit per sub-leaf).
const B3_MASK_WORDS: usize = B3_LEAVES / 64;

const _: () = {
    assert!(B3_LEAF_BYTES == 4096);
    assert!(B3_LEAVES == 1024);
    assert!(B3_LEAF_WORDS == 4);
    assert!(B3_LEAVES == B3_MASK_WORDS * 64);
};

/// Owner-only B3 index. Plain (non-atomic) bytes; zero-initialized at segment
/// init and zeroed again on decommit reset.
pub(crate) struct B3Free {
    raw: *mut u8,
}

impl B3Free {
    /// Byte footprint of the region: class masks, leaf bindings, cursors,
    /// 8-byte aligned. Padded UP to 12 KiB: the raw data is ~7.2 KiB, but the
    /// segment-layout invariants pinned by the forced-page tests
    /// (`lazy_initial_commit_forced_page`, `decomp_hooks_forced_page`)
    /// require `small_meta_end() + LAZY_FIRST_CHUNK` to stay a NON-multiple
    /// of every page size above 4 KiB; the natural 20-page meta_end (81920)
    /// is a 16 KiB multiple, the padded 21-page value (86016) is not (and
    /// so are the primordial/hardened values it shifts).
    pub(crate) const FOOTPRINT: usize = {
        let raw = (SMALL_CLASS_COUNT * B3_MASK_WORDS * 8 + B3_LEAVES + SMALL_CLASS_COUNT + 7) & !7;
        assert!(
            raw <= 12288,
            "B3Free raw data exceeds the 12 KiB pad target"
        );
        12288
    };

    pub(crate) fn new(raw: *mut u8) -> Self {
        Self { raw }
    }

    #[inline(always)]
    fn mask_ptr(&self, class: usize, word: usize) -> *mut u64 {
        // SAFETY: class < SMALL_CLASS_COUNT and word < B3_MASK_WORDS are the
        // caller's contract (enforced by the public methods); the region is
        // FOOTPRINT bytes at `raw`, owner-only.
        unsafe { self.raw.add(class * B3_MASK_WORDS * 8 + word * 8) as *mut u64 }
    }

    #[inline(always)]
    fn leaf_class_ptr(&self, leaf: usize) -> *mut u8 {
        // SAFETY: leaf < B3_LEAVES by contract; masks end at
        // SMALL_CLASS_COUNT * B3_MASK_WORDS * 8, bindings follow.
        unsafe { self.raw.add(SMALL_CLASS_COUNT * B3_MASK_WORDS * 8 + leaf) }
    }

    #[inline(always)]
    fn cursor_ptr(&self, class: usize) -> *mut u8 {
        // SAFETY: class < SMALL_CLASS_COUNT by contract; cursors follow the
        // binding array.
        unsafe {
            self.raw
                .add(SMALL_CLASS_COUNT * B3_MASK_WORDS * 8 + B3_LEAVES + class)
        }
    }

    /// Zero the whole region (segment init / decommit reset).
    pub(crate) fn init_in_place(raw: *mut u8) {
        // SAFETY: the caller grants FOOTPRINT writable zeroed-on-return bytes
        // inside the segment being initialized.
        Node::zero(raw, Self::FOOTPRINT);
    }

    /// The bound class of sub-leaf `leaf` (`class + 1`), or 0 = virgin.
    #[inline(always)]
    pub(crate) fn leaf_class(&self, leaf: usize) -> u8 {
        debug_assert!(leaf < B3_LEAVES);
        // SAFETY: in-bounds binding byte, owner-only.
        Node::read_u8(self.leaf_class_ptr(leaf))
    }

    /// Bind a virgin sub-leaf to `class` at its first carve.
    #[inline(always)]
    pub(crate) fn bind_leaf(&mut self, leaf: usize, class: usize) {
        debug_assert!(leaf < B3_LEAVES && class < SMALL_CLASS_COUNT);
        // SAFETY: in-bounds binding byte, owner-only single writer.
        Node::write_u8(self.leaf_class_ptr(leaf), class as u8 + 1);
    }

    #[inline(always)]
    pub(crate) fn set_leaf(&mut self, class: usize, leaf: usize) {
        debug_assert!(class < SMALL_CLASS_COUNT && leaf < B3_LEAVES);
        // SAFETY: in-bounds mask word, owner-only single writer.
        let p = self.mask_ptr(class, leaf / 64);
        Node::write_u64(p, Node::read_u64(p) | (1u64 << (leaf % 64)));
    }

    #[inline(always)]
    pub(crate) fn clear_leaf(&mut self, class: usize, leaf: usize) {
        debug_assert!(class < SMALL_CLASS_COUNT && leaf < B3_LEAVES);
        // SAFETY: in-bounds mask word, owner-only single writer.
        let p = self.mask_ptr(class, leaf / 64);
        Node::write_u64(p, Node::read_u64(p) & !(1u64 << (leaf % 64)));
    }

    /// OR `val` into mask word `word` of `class` (flush-run accumulation:
    /// one RMW per touched word per run).
    #[inline(always)]
    pub(crate) fn or_mask_word(&self, class: usize, word: usize, val: u64) {
        debug_assert!(class < SMALL_CLASS_COUNT && word < B3_MASK_WORDS && val != 0);
        // SAFETY: in-bounds mask word, owner-only single writer.
        let p = self.mask_ptr(class, word);
        Node::write_u64(p, Node::read_u64(p) | val);
    }

    #[inline(always)]
    pub(crate) fn mask_word(&self, class: usize, word: usize) -> u64 {
        debug_assert!(class < SMALL_CLASS_COUNT && word < B3_MASK_WORDS);
        // SAFETY: in-bounds mask word, owner-only.
        Node::read_u64(self.mask_ptr(class, word))
    }

    /// Whether class `class` has no sub-leaf with free slots.
    #[inline(always)]
    pub(crate) fn class_empty(&self, class: usize) -> bool {
        debug_assert!(class < SMALL_CLASS_COUNT);
        let mut w = 0;
        while w < B3_MASK_WORDS {
            if self.mask_word(class, w) != 0 {
                return false;
            }
            w += 1;
        }
        true
    }

    /// First sub-leaf of `class` with a set mask bit, scanning from the
    /// cursor with wraparound; advances the cursor to the hit word.
    #[inline(always)]
    pub(crate) fn first_leaf(&self, class: usize) -> Option<usize> {
        debug_assert!(class < SMALL_CLASS_COUNT);
        // SAFETY: in-bounds cursor byte, owner-only; written through `self`
        // via the shared raw pointer (interior mutability by contract).
        let start = Node::read_u8(self.cursor_ptr(class)) as usize % B3_MASK_WORDS;
        let mut i = 0;
        while i < B3_MASK_WORDS {
            let w = (start + i) % B3_MASK_WORDS;
            let bits = self.mask_word(class, w);
            if bits != 0 {
                // SAFETY: same cursor byte as the read above; single owner.
                Node::write_u8(self.cursor_ptr(class), w as u8);
                return Some(w * 64 + bits.trailing_zeros() as usize);
            }
            i += 1;
        }
        None
    }
}
