use crate::alloc_core::node::Node;
use crate::alloc_core::segment_header::SegmentKind;

use super::MAX_SEGMENTS;

const WORDS: usize = MAX_SEGMENTS / 64;

/// Exact owner-only membership. The backing bytes live in the primordial
/// metadata page; only the pointer is carried by `SegmentTable`.
#[repr(C)]
pub(crate) struct ActiveKindIndex {
    small: [u64; WORDS],
    large: [u64; WORDS],
    small_summary: u64,
    large_summary: u64,
}

const _: () = assert!(MAX_SEGMENTS == 4096 && WORDS == 64);
const _: () = assert!(core::mem::size_of::<ActiveKindIndex>() == 1040);
const _: () = assert!(core::mem::align_of::<ActiveKindIndex>() <= 8);

impl ActiveKindIndex {
    pub(crate) const FOOTPRINT: usize = core::mem::size_of::<Self>();

    #[inline]
    fn offsets(kind: SegmentKind) -> (usize, usize) {
        match kind {
            SegmentKind::Small | SegmentKind::Primordial => (
                core::mem::offset_of!(Self, small),
                core::mem::offset_of!(Self, small_summary),
            ),
            SegmentKind::Large => (
                core::mem::offset_of!(Self, large),
                core::mem::offset_of!(Self, large_summary),
            ),
            SegmentKind::Unknown => std::process::abort(),
        }
    }

    #[inline]
    fn word_at(ptr: *mut Self, off: usize) -> *mut u64 {
        Node::offset(ptr.cast(), off).cast()
    }

    #[inline]
    fn read(ptr: *mut Self, off: usize) -> u64 {
        Node::read_struct(Self::word_at(ptr, off))
    }

    #[inline]
    fn write(ptr: *mut Self, off: usize, value: u64) {
        Node::write_struct(Self::word_at(ptr, off), value);
    }

    /// Caller supplies aligned, writable, exclusively owned primordial bytes.
    pub(crate) fn init_in_place(ptr: *mut Self) {
        for w in 0..WORDS {
            Self::write(ptr, core::mem::offset_of!(Self, small) + w * 8, 0);
            Self::write(ptr, core::mem::offset_of!(Self, large) + w * 8, 0);
        }
        Self::write(ptr, core::mem::offset_of!(Self, small_summary), 0);
        Self::write(ptr, core::mem::offset_of!(Self, large_summary), 0);
        Self::set(ptr, 0, SegmentKind::Primordial);
    }

    pub(super) fn set(ptr: *mut Self, slot: usize, kind: SegmentKind) {
        if slot >= MAX_SEGMENTS {
            std::process::abort();
        }
        let (leaf_off, summary_off) = Self::offsets(kind);
        let w = slot / 64;
        let off = leaf_off + w * 8;
        let bit = 1u64 << (slot % 64);
        let old = Self::read(ptr, off);
        let other_off = if leaf_off == core::mem::offset_of!(Self, small) {
            core::mem::offset_of!(Self, large)
        } else {
            core::mem::offset_of!(Self, small)
        };
        if old & bit != 0 || Self::read(ptr, other_off + w * 8) & bit != 0 {
            std::process::abort();
        }
        Self::write(ptr, off, old | bit);
        if old == 0 {
            let summary = Self::read(ptr, summary_off);
            Self::write(ptr, summary_off, summary | (1u64 << w));
        }
    }

    #[inline]
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(super) fn contains(ptr: *mut Self, slot: usize, kind: SegmentKind) -> bool {
        if slot >= MAX_SEGMENTS {
            return false;
        }
        let (leaf_off, _) = Self::offsets(kind);
        Self::read(ptr, leaf_off + (slot / 64) * 8) & (1u64 << (slot % 64)) != 0
    }

    pub(super) fn clear(ptr: *mut Self, slot: usize, kind: SegmentKind) {
        if slot == 0 || slot >= MAX_SEGMENTS {
            std::process::abort();
        }
        let (leaf_off, summary_off) = Self::offsets(kind);
        let w = slot / 64;
        let off = leaf_off + w * 8;
        let bit = 1u64 << (slot % 64);
        let old = Self::read(ptr, off);
        if old & bit == 0 {
            std::process::abort();
        }
        let new = old & !bit;
        Self::write(ptr, off, new);
        if new == 0 {
            let summary = Self::read(ptr, summary_off);
            Self::write(ptr, summary_off, summary & !(1u64 << w));
        }
    }

    /// Returns the least set slot at or above `from`; no borrow escapes.
    pub(super) fn next(ptr: *mut Self, kind: SegmentKind, from: usize) -> Option<usize> {
        if from >= MAX_SEGMENTS {
            return None;
        }
        let (leaf_off, summary_off) = Self::offsets(kind);
        let word = from / 64;
        let leaf = Self::read(ptr, leaf_off + word * 8) & (u64::MAX << (from % 64));
        if leaf != 0 {
            return Some(word * 64 + leaf.trailing_zeros() as usize);
        }
        if word == WORDS - 1 {
            return None;
        }
        let later = Self::read(ptr, summary_off) & (u64::MAX << (word + 1));
        if later == 0 {
            return None;
        }
        let next_word = later.trailing_zeros() as usize;
        let next_leaf = Self::read(ptr, leaf_off + next_word * 8);
        if next_leaf == 0 {
            std::process::abort();
        }
        Some(next_word * 64 + next_leaf.trailing_zeros() as usize)
    }
}
