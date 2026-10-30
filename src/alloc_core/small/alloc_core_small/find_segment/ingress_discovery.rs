//! SC-6 A-exact owner half of the remote-free ingress (P23): this heap's
//! notification cell, taking notified slots, re-arming and fully draining
//! them. Discovery (`scan.rs`) trusts a negative answer of an armed heap.

#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    not(feature = "numa-aware"),
    feature = "alloc-decommit"
))]
use super::super::sidecar_drain_outcome::SidecarDrainOutcome;
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    not(feature = "numa-aware")
))]
use super::scan::DirectoryCandidateContext;
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    not(feature = "numa-aware")
))]
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL};
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    not(feature = "numa-aware")
))]
use crate::registry::segment_route::{HeapIngress, INGRESS_CELLS, INGRESS_WORDS};

// The ingress cell geometry matches the table and the registry it serves.
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    not(feature = "numa-aware")
))]
const _: () = {
    assert!(crate::alloc_core::segment_table::MAX_SEGMENTS == 64 * INGRESS_WORDS);
    assert!(crate::alloc_core::segment_header::MAX_HEAPS == INGRESS_CELLS);
};

impl AllocCore {
    /// SC-6 A-exact: this heap's ingress cell (routed registry heaps only).
    #[cfg(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        not(feature = "numa-aware")
    ))]
    #[inline]
    pub(super) fn ingress(&self) -> Option<&'static HeapIngress> {
        self.table.route_owner().and_then(HeapIngress::of)
    }

    /// SC-6: whether a negative discovery answer is trusted (armed heap), so
    /// the OOM rescue must run even without a materialised directory.
    #[cfg(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        not(feature = "numa-aware")
    ))]
    #[cfg_attr(
        any(feature = "numa-aware", not(feature = "alloc-segment-directory")),
        allow(dead_code)
    )]
    #[inline]
    pub(crate) fn ingress_armed(&self) -> bool {
        self.ingress().is_some()
    }
    #[cfg(not(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        not(feature = "numa-aware")
    )))]
    #[cfg_attr(
        any(feature = "numa-aware", not(feature = "alloc-segment-directory")),
        allow(dead_code)
    )]
    #[inline]
    pub(crate) fn ingress_armed(&self) -> bool {
        false
    }

    /// SC-6 owner half: take every notified slot, re-arm it, fully drain it;
    /// the first segment whose bin for `class_idx` is then non-empty is the hit.
    #[cfg(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        not(feature = "numa-aware")
    ))]
    pub(super) fn process_ingress(
        &mut self,
        class_idx: usize,
        ingress: &'static HeapIngress,
    ) -> Option<*mut u8> {
        let mut hit = None;
        ingress.take(|i| {
            if let Some(base) = self.drain_notified_slot(i, class_idx) {
                hit.get_or_insert(base);
            }
        });
        if let Some(base) = hit {
            self.finalize_hit(
                base,
                class_idx,
                DirectoryCandidateContext {
                    #[cfg(feature = "alloc-stats")]
                    routed_scan_was_nonempty: None,
                    #[cfg(feature = "alloc-segment-directory")]
                    periodic_revalidation_active: false,
                    #[cfg(feature = "alloc-segment-directory")]
                    rescue: false,
                },
            );
        }
        hit
    }

    #[cfg(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        not(feature = "numa-aware")
    ))]
    fn drain_notified_slot(&mut self, i: usize, class_idx: usize) -> Option<*mut u8> {
        if i >= self.table.count() as usize {
            return None;
        }
        let base = self.table.base_at(i);
        if base.is_null()
            || !matches!(
                SegmentHeader::kind_at(base),
                SegmentKind::Small | SegmentKind::Primordial
            )
        {
            return None;
        }
        // Arm (Release + SeqCst fence) BEFORE the full drain: a producer that
        // published before the fence is seen by the drain, one after it sees
        // ARMED and notifies again.
        self.table.arm_small_route(i, base);
        match self.drain_segment_sidecar(base) {
            #[cfg(feature = "alloc-decommit")]
            SidecarDrainOutcome::Decommitted { pooled: false } => return None,
            _ => {}
        }
        (SegmentMeta::new(base).bin_table().head(class_idx) != FREE_LIST_NULL).then_some(base)
    }
}
