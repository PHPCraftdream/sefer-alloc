//! Owner-only numeric candidate enumeration and independent diagnostic census.

use super::{ActiveKindIndex, SegmentTable};
use crate::alloc_core::segment_header::SegmentKind;

#[cfg(all(feature = "internals", feature = "bench-internals"))]
use super::MAX_SEGMENTS;
#[cfg(all(feature = "internals", feature = "bench-internals"))]
use crate::alloc_core::segment_header::SegmentHeader;

impl SegmentTable {
    /// Numeric enumeration only. Resolve every result with a fresh `base_at`.
    #[inline]
    pub(crate) fn next_active(&self, kind: SegmentKind, from: usize) -> Option<usize> {
        let next = ActiveKindIndex::next(self.active_kind, kind, from);
        if next.is_some_and(|i| i >= self.count() as usize) {
            std::process::abort();
        }
        next
    }

    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    pub(crate) fn active_kind_census(&self) -> (usize, usize, bool) {
        let mut small = 0;
        let mut large = 0;
        let mut exact = true;
        for i in 0..MAX_SEGMENTS {
            let base = self.base_at(i);
            let (expect_small, expect_large) = if base.is_null() {
                (false, false)
            } else {
                match SegmentHeader::kind_at(base) {
                    SegmentKind::Small | SegmentKind::Primordial => (true, false),
                    SegmentKind::Large => (false, true),
                    SegmentKind::Unknown => std::process::abort(),
                }
            };
            small += usize::from(expect_small);
            large += usize::from(expect_large);
            exact &=
                ActiveKindIndex::contains(self.active_kind, i, SegmentKind::Small) == expect_small;
            exact &=
                ActiveKindIndex::contains(self.active_kind, i, SegmentKind::Large) == expect_large;
        }
        let mut index_small = 0;
        let mut from = 0;
        while let Some(i) = self.next_active(SegmentKind::Small, from) {
            index_small += 1;
            from = i + 1;
        }
        let mut index_large = 0;
        from = 0;
        while let Some(i) = self.next_active(SegmentKind::Large, from) {
            index_large += 1;
            from = i + 1;
        }
        (
            small,
            large,
            exact && index_small == small && index_large == large,
        )
    }
}
