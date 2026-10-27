//! U31 (docs/perf/OPEN_ITEMS.md item 90): owner-only deferral of the routed
//! linear scan's sidecar drains after a scan that reclaimed nothing.
//!
//! Without it a growing heap drains every segment's pending sidecar from the
//! payload start to the bump high water on EVERY refill miss, so the work is
//! quadratic in the carved range. After a scan whose drains all came back
//! `Skipped`, the drains are deferred until the current segment has carved
//! `drained bytes >> SIDECAR_DEFER_SHIFT` more, fills, or stops being
//! `small_cur`. The threshold lives in that segment's own header
//! (`SegmentHeader::sidecar_defer_until`), so a segment change closes the
//! window by construction. Producers are untouched: a publication made
//! meanwhile stays pending and is drained by the first scan after the window.

use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::segment_header::{align_up, SegmentMeta};

use crate::alloc_core::alloc_core::AllocCore;

/// Window length = sum of the drained segments' bump high water >> this.
pub(in crate::alloc_core) const SIDECAR_DEFER_SHIFT: u32 = 3;

impl AllocCore {
    /// True while `small_cur`'s window is open AND it can still carve one
    /// `block_size` block, so a skipped drain always ends in a carve that
    /// advances the bump.
    #[inline]
    pub(super) fn sidecar_drains_deferred(&self, block_size: usize) -> bool {
        let meta = SegmentMeta::new(self.small_cur);
        let bump = meta.bump_of();
        bump < meta.sidecar_defer_until_of() && align_up(bump, block_size) + block_size <= SEGMENT
    }

    /// Open `small_cur`'s window after a linear scan that drained
    /// `drained_bytes` of sidecar range and reclaimed nothing.
    #[inline]
    pub(super) fn open_sidecar_defer_window(&mut self, drained_bytes: usize) {
        let mut meta = SegmentMeta::new(self.small_cur);
        let until = meta
            .bump_of()
            .saturating_add(drained_bytes >> SIDECAR_DEFER_SHIFT)
            .min(SEGMENT);
        // SEGMENT (4 MiB) fits `u32`.
        meta.set_sidecar_defer_until(until as u32);
    }

    /// U31 test-only: `small_cur`'s deferral threshold (0 = no window).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_sidecar_defer_until(&self) -> usize {
        SegmentMeta::new(self.small_cur).sidecar_defer_until_of()
    }
}
