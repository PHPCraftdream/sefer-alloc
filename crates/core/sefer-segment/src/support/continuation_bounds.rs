//! Segment-relative continuation geometry cached for one owner-only freelist walk.
// SAFETY SEAM: the exported constructor reads segment header metadata and is unsafe.
#![allow(unsafe_code)]
use crate::size_classes::MIN_BLOCK;

use crate::segment::segment_header::{Layout, SegmentHeader, SegmentKind, SegmentMeta};

/// Cached numeric bounds for one owner-only free-list continuation walk.
/// Hidden compatibility type; not supported public API.
#[doc(hidden)]
pub struct ContinuationBounds {
    lo: usize,
    span: usize,
}

impl ContinuationBounds {
    /// Build bounds by inspecting the segment header and bump cursor.
    ///
    /// # Safety
    /// `base` must point to a live, mapped segment reservation whose header and
    /// metadata have been initialized, and whose owner is exclusively performing
    /// the free-list walk for the duration of this snapshot.
    #[inline(always)]
    pub unsafe fn from_segment_base(base: *mut u8) -> Self {
        // SAFETY: forwarded caller contract guarantees the live initialized segment.
        unsafe { Self::new(base) }
    }

    /// # Safety
    /// Same live initialized segment and owner-exclusion requirements as the
    /// public `from_segment_base` constructor.
    unsafe fn new(base: *mut u8) -> Self {
        // SAFETY: caller's from_segment_base contract proves this is a live initialized segment.
        let lo = match unsafe { SegmentHeader::kind_at(base) } {
            SegmentKind::Primordial => Layout::primordial_meta_end(),
            _ => Layout::small_meta_end(),
        };
        let bump = unsafe { SegmentMeta::new(base) }.bump_of();
        // Owner-exclusive pop/drain only change bitmap, live credits, and sidecar publication.
        // Neither carves nor resets bump, and no callback permits owner reentry.
        let hi = bump.min(crate::alloc_core::os::SEGMENT);
        if hi < lo {
            crate::alloc_core::fatal::fatal(
                b"node: free-list continuation out of range or misaligned\n",
            );
        }
        Self { lo, span: hi - lo }
    }

    /// In `[meta end, bump)` and `MIN_BLOCK`-aligned. Class-stride divisibility
    /// is checked only by hardened's `valid_free_node` (iai cost, PR #57).
    #[inline(always)]
    pub fn contains_aligned(&self, off: usize) -> bool {
        off.wrapping_sub(self.lo) < self.span && off & (MIN_BLOCK - 1) == 0
    }
}
