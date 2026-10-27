//! U31 (docs/perf/OPEN_ITEMS.md item 90): owner-only deferral of the routed
//! linear scan's sidecar drains after a scan that reclaimed nothing.
//!
//! Without it a growing heap drains every segment's pending sidecar from the
//! payload start to the bump high water on EVERY refill miss, so the work is
//! quadratic in the carved range. After a scan whose drains all came back
//! `Skipped`, a window `(start, end)` of `small_cur`'s bump is recorded:
//! `start` = the bump at that scan, `end` = `start + drained range /
//! 2^SIDECAR_DEFER_SHIFT`. Drains are deferred only while the bump is
//! strictly past `start` (the owner has carved since the empty scan — a
//! repeated discovery with no carve in between always drains) and below
//! `end`, and `small_cur` can still carve one block. The window lives in
//! that segment's own header, so a segment change closes it by
//! construction. Producers are untouched: a publication made meanwhile stays
//! pending and is drained by the first scan after the window.

use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::segment_header::{align_up, SegmentMeta};

use crate::alloc_core::alloc_core::AllocCore;

/// Window length = drained payload range >> this.
pub(in crate::alloc_core) const SIDECAR_DEFER_SHIFT: u32 = 3;

/// Window offsets are stored in units of this many bytes: `SEGMENT / UNIT`
/// = 32 768 fits `u16`, so both ends pack into the header's one `u32`.
const UNIT: usize = 128;
const _: () = assert!(SEGMENT / UNIT <= u16::MAX as usize);

impl AllocCore {
    /// True while `small_cur`'s window is open: bump in `(start, end)` and
    /// room for one `block_size` block, so a skipped drain always ends in a
    /// carve that advances the bump.
    #[inline]
    pub(super) fn sidecar_drains_deferred(&self, block_size: usize) -> bool {
        let meta = SegmentMeta::new(self.small_cur);
        let window = meta.sidecar_defer_window_of() as usize;
        let (start, end) = ((window & 0xffff) * UNIT, (window >> 16) * UNIT);
        let bump = meta.bump_of();
        start < bump && bump < end && align_up(bump, block_size) + block_size <= SEGMENT
    }

    /// Open `small_cur`'s window after a linear scan that drained
    /// `drained_bytes` of sidecar payload range and reclaimed nothing.
    /// `start` rounds up and `end` down, so rounding only narrows the window.
    #[inline]
    pub(super) fn open_sidecar_defer_window(&mut self, drained_bytes: usize) {
        let mut meta = SegmentMeta::new(self.small_cur);
        let bump = meta.bump_of();
        let start = bump.div_ceil(UNIT);
        let end = (bump
            .saturating_add(drained_bytes >> SIDECAR_DEFER_SHIFT)
            .min(SEGMENT))
            / UNIT;
        let window = if end > start {
            // Both are <= SEGMENT / UNIT, which fits `u16` (const assert above).
            (start | (end << 16)) as u32
        } else {
            0
        };
        meta.set_sidecar_defer_window(window);
    }

    /// U31 test-only: `(bump, window start, window end)` of `small_cur` in
    /// bytes (`end == 0` = no window; open while `start < bump < end`).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_sidecar_defer_window(&self) -> (usize, usize, usize) {
        let meta = SegmentMeta::new(self.small_cur);
        let window = meta.sidecar_defer_window_of() as usize;
        (
            meta.bump_of(),
            (window & 0xffff) * UNIT,
            (window >> 16) * UNIT,
        )
    }

    /// U31 test-only: close `small_cur`'s window so the next discovery drains.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_close_sidecar_defer_window(&mut self) {
        SegmentMeta::new(self.small_cur).set_sidecar_defer_window(0);
    }
}
