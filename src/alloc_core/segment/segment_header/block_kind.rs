//! Ph3b: the single private witness for a block's PHYSICAL kind.
//!
//! The free side historically had two independent answers to "what kind of
//! block is this": the caller-supplied `Layout` (re-derived through
//! `SizeClasses::class_for` at every entry) and the segment header's `kind`
//! byte (`SegmentHeader::kind_at`), which records how the block was actually
//! backed. They disagree exactly where it matters — R14-4's medium→Large
//! promotion (task #289) grows a block IN PLACE inside its Large segment, so
//! its dealloc layout still classifies Small — and every entry that derived
//! its route from the `Layout` alone misrouted that case.
//!
//! [`BlockKind`] settles it with one read: the physical kind comes from
//! [`SegmentHeader::kind_at`] only; the caller-derived class is at most a
//! payload (and a reject when it is absent), never an authority. The Small /
//! Large algorithms themselves stay exactly as they were — this type only
//! decides WHICH of them a block belongs to.

use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};

/// Physical kind of a block, as recorded by its segment header.
///
/// `Small` carries the size class the CALLER resolved from its `Layout`
/// (`SizeClasses::class_for(size.max(MIN_BLOCK), align)`) — the header has no
/// class field, and the free paths already need that class for the magazine
/// push that follows, so it is passed in instead of being recomputed here.
/// `class_for` stays the caller's business; this witness stays the
/// header's.
pub(crate) enum BlockKind {
    /// The block lives in the primordial segment — Small semantics for every
    /// free path, with no distinct handling of its own.
    Primordial,
    /// The block lives in a Small segment and the caller's layout resolved to
    /// class `class_idx`.
    Small { class_idx: usize },
    /// The block lives in a Large segment: one allocation per segment, freed
    /// through the substrate regardless of what the caller's layout
    /// classifies as.
    Large,
    /// The header's `kind` byte is a reject sentinel, or the block is in a
    /// Small segment whose layout does not resolve a class. No free path may
    /// act on a block whose physical kind it cannot trust.
    Unknown,
}

impl BlockKind {
    /// Resolve `base`'s physical kind. `class` is the caller's
    /// `SizeClasses::class_for(size.max(MIN_BLOCK), align)` result, or `None`
    /// when that layout does not classify as Small — a `Small` header with
    /// `None` degrades to [`BlockKind::Unknown`] (the same reject the
    /// former `classify`-based routing produced), so no call site has to
    /// special-case it.
    ///
    /// `of` does NOT call `class_for`: that decision stays with the caller,
    /// which already has the clamped size / align in hand for its magazine
    /// push. This function's only authority is the header's `kind` byte.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn of(base: *mut u8, class: Option<usize>) -> Self {
        match SegmentHeader::kind_at(base) {
            SegmentKind::Primordial => Self::Primordial,
            SegmentKind::Large => Self::Large,
            SegmentKind::Unknown => Self::Unknown,
            SegmentKind::Small => match class {
                Some(class_idx) => Self::Small { class_idx },
                None => Self::Unknown,
            },
        }
    }
}
