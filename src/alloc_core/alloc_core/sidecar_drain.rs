//! Owner-only bounded drain of terminal route-sidecar publications.

use super::{AllocCore, LARGE_REMOTE_RETIREMENTS};
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta};

impl AllocCore {
    /// Fixes the table high-water at entry and visits each live slot once.
    /// The owner lease excludes issue/reuse throughout this pass. Each Small
    /// word is exchanged exactly once; post-cut publishers wait for a later
    /// pass and keep their outstanding credits. No dirty hint or producer
    /// quiescence is required. Only stored table roots access reservations.
    pub(crate) fn drain_sidecar_ingress(&mut self) -> usize {
        if !self.table.is_routed() {
            return 0;
        }
        let end = self.table.count() as usize;
        let mut reclaimed = 0;
        for index in 0..end {
            let base = self.table.base_at(index);
            if base.is_null() {
                continue;
            }
            match SegmentHeader::kind_at(base) {
                SegmentKind::Small | SegmentKind::Primordial => {
                    let high_water = SegmentMeta::new(base).bump_of();
                    let mut changed_classes = 0u64;
                    {
                        let Some(mut scan) = self.table.scan_small_route(index, base, high_water)
                        else {
                            std::process::abort();
                        };
                        while let Some(mut cut) = scan.next_cut() {
                            while let Some(record) = cut.pop() {
                                if Self::reclaim_sidecar_record(base, record.offset, record.class) {
                                    changed_classes |= 1u64 << record.class;
                                    reclaimed += 1;
                                }
                            }
                        }
                    }
                    // Every cut and the route borrow are gone before table
                    // removal or payload decommit. A detached or unpublished
                    // valid instance still owns a credit, so only a fully
                    // consumed segment can reach zero here.
                    #[cfg(feature = "alloc-segment-directory")]
                    self.sync_directory_for_segment_classes(base, index, changed_classes);
                    #[cfg(not(feature = "alloc-segment-directory"))]
                    let _ = changed_classes;
                    #[cfg(feature = "alloc-decommit")]
                    if changed_classes != 0
                        && Self::dec_live_and_maybe_decommit(base, self.small_cur)
                    {
                        let _ = self.release_or_pool_empty_segment(base);
                    }
                }
                SegmentKind::Large => {
                    if self.table.claim_large_route(index, base) {
                        self.reclaim_large_segment(base);
                        LARGE_REMOTE_RETIREMENTS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        reclaimed += 1;
                    }
                }
                SegmentKind::Unknown => std::process::abort(),
            }
        }
        reclaimed
    }

    /// Large-only cold scan: the route descriptor must surrender its terminal
    /// obligation before physical retirement. Before publication its instance
    /// credit pins the reservation; afterward a pin retains only the descriptor.
    pub(crate) fn drain_large_sidecar_ingress(&mut self) -> usize {
        if !self.table.is_routed() {
            return 0;
        }
        let end = self.table.count() as usize;
        let mut reclaimed = 0;
        for index in 0..end {
            let base = self.table.base_at(index);
            if !base.is_null()
                && SegmentHeader::kind_at(base) == SegmentKind::Large
                && self.table.claim_large_route(index, base)
            {
                self.reclaim_large_segment(base);
                LARGE_REMOTE_RETIREMENTS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                reclaimed += 1;
            }
        }
        reclaimed
    }
}
