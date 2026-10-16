//! Segment-relative continuation geometry cached for one owner-only freelist walk.
use crate::alloc_core::size_classes::SizeClasses;

#[derive(Clone, Copy)]
pub(crate) struct ContinuationBounds {
    lo: usize,
    span: usize,
    divisor: (u64, u64, u32),
}

impl ContinuationBounds {
    #[inline(always)]
    pub(crate) fn new(base: *mut u8, class_idx: usize) -> Self {
        let lo = match crate::alloc_core::segment_header::SegmentHeader::kind_at(base) {
            crate::alloc_core::segment_header::SegmentKind::Primordial => {
                crate::alloc_core::segment_header::Layout::primordial_meta_end()
            }
            _ => crate::alloc_core::segment_header::Layout::small_meta_end(),
        };
        let bump = crate::alloc_core::segment_header::SegmentMeta::new(base).bump_of();
        // Owner-exclusive pop/drain only change bitmap, live credits, and sidecar publication.
        // Neither carves nor resets bump, and no callback permits owner reentry.
        let hi = bump.min(crate::alloc_core::os::SEGMENT);
        if hi < lo {
            crate::alloc_core::fatal::fatal(
                b"node: free-list continuation out of range or misaligned\n",
            );
        }
        Self {
            lo,
            span: hi - lo,
            divisor: SizeClasses::divisor(class_idx),
        }
    }

    #[inline(always)]
    pub(crate) fn contains_aligned(&self, off: usize) -> bool {
        off.wrapping_sub(self.lo) < self.span && SizeClasses::divides_with(self.divisor, off)
    }
}
