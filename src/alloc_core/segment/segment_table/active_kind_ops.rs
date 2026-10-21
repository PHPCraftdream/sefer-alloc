//! Owner-only numeric candidate enumeration and independent diagnostic census.

use super::{ActiveKindIndex, SegmentTable};
use crate::alloc_core::fatal::fatal;
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
            fatal(b"active_kind_ops: active index entry beyond table count\n");
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
                    SegmentKind::Unknown => fatal(b"active_kind_ops: unknown segment kind\n"),
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

// --- D1' phase-1a: the SegmentTable-side start-bit-map seams, moved VERBATIM
// from `segment_table_impl.rs` (which sits at its exact 1000-line cap). Pure
// move; an inherent `impl` may live in any module of the same crate, and the
// `routes` field is `pub(super)` exactly so this sibling file can reach it.
impl super::segment_table_impl::SegmentTable {
    // --- D1′ staged issue (phase-2 OPT) ----------------------------------
    // Open a per-run issue handle: the fatal slot guards (index < cap,
    // occupied slot, root == base, Small sidecar present) plus the cheap
    // `base_at(segment_id_at(base)) == base` integrity check run ONCE here;
    // per-block `SmallIssueRun::issue` / `clear_pending_start_bit` perform
    // only the cheap sidecar operations. Unrouted (standalone) tables get a
    // no-op run, exactly like the former per-issue no-op.
    #[cfg_attr(not(debug_assertions), inline(always))]
    pub(crate) fn small_issue_run(&self, base: *mut u8) -> super::SmallIssueRun<'_> {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &self.routes {
            let index =
                crate::alloc_core::segment_header::SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: issue base is not the segment base\n");
            }
            return routes.small_issue_run(index, base);
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = base;
        super::SmallIssueRun::unrouted()
    }

    /// Reclaim's start-bit check on the published record's start granule.
    /// Unrouted tables make no objection (`true`), matching the SEC-1
    /// convention of `scan_small_route_from`.
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    pub(crate) fn reclaim_start_bit(&self, base: *mut u8, offset: u32) -> bool {
        if let Some(routes) = &self.routes {
            let index =
                crate::alloc_core::segment_header::SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: reclaim base is not the segment base\n");
            }
            return routes.reclaim_start_bit(index, base, offset);
        }
        true
    }

    /// Reclaim's production overlap check over the OPEN interval
    /// `[start, end)`; `true` = a foreign start bit inside the claimed
    /// block (corruption). Unrouted tables make no objection (`false`).
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    pub(crate) fn reclaim_start_overlap(&self, base: *mut u8, start: u32, end: u32) -> bool {
        if let Some(routes) = &self.routes {
            let index =
                crate::alloc_core::segment_header::SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: overlap base is not the segment base\n");
            }
            return routes.reclaim_start_overlap(index, base, start, end);
        }
        false
    }

    /// Retain-reset: atomically clear pending AND start bits over the
    /// granule range `[start_off, end_off)`.
    #[cfg_attr(not(debug_assertions), inline(always))]
    // Only the retain-reset hook (bench-internals + internals +
    // alloc-decommit) reaches this seam; allow the production-only build.
    #[cfg_attr(
        not(all(feature = "alloc-decommit", feature = "bench-internals")),
        allow(dead_code)
    )]
    pub(crate) fn clear_remote_bits(&self, base: *mut u8, start_off: u32, end_off: u32) {
        #[cfg(feature = "alloc-global")]
        if let Some(routes) = &self.routes {
            let index =
                crate::alloc_core::segment_header::SegmentHeader::segment_id_at(base) as usize;
            if self.base_at(index) != base {
                fatal(b"segment_table: reset base is not the segment base\n");
            }
            routes.clear_remote_bits(index, base, start_off, end_off);
        }
        #[cfg(not(feature = "alloc-global"))]
        let _ = (base, start_off, end_off);
    }
}
