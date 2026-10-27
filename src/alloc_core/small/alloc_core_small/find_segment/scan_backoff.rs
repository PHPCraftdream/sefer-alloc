//! U31 (docs/perf/OPEN_ITEMS.md item 90): owner-only deferral of the routed
//! linear scan's sidecar drains after scans that reclaimed nothing.
//!
//! Without it a growing heap drains every segment's pending sidecar from the
//! payload start to the bump high water on EVERY refill miss, so the work is
//! quadratic in the carved range. Each scan whose drains all came back
//! `Skipped` extends an empty-scan streak; a reclaiming drain resets it.
//! Once the streak reaches `SIDECAR_DEFER_STREAK`, a window `(start, end)` of
//! `small_cur`'s bump is recorded: `start` = the bump at that scan, `end` =
//! `start + drained range >> SIDECAR_DEFER_SHIFT`. Drains are deferred only
//! while the bump is strictly past `start` (the owner has carved since the
//! empty scan — a repeated discovery with no carve in between always drains)
//! and below `end`, and `small_cur` can still carve one block. The streak
//! keeps the window shut while remote frees keep arriving (scans that
//! reclaim interleave with empty ones), so pending frees are not left behind
//! while the owner carves. The state lives in `small_cur`'s own header, so a
//! segment change starts a fresh streak. Producers are untouched: a
//! publication made meanwhile stays pending and is drained by the first scan
//! after the window.

use crate::alloc_core::os::SEGMENT;
use crate::alloc_core::segment_header::{align_up, SegmentMeta};

use crate::alloc_core::alloc_core::AllocCore;

/// Window length = drained payload range >> this.
pub(in crate::alloc_core) const SIDECAR_DEFER_SHIFT: u32 = 4;

/// Consecutive empty scans before a window opens (saturating 2-bit counter).
pub(in crate::alloc_core) const SIDECAR_DEFER_STREAK: u32 = 3;

/// Window offsets are stored in units of this many bytes: `SEGMENT / UNIT`
/// = 16 384 fits 15 bits, so both ends and the streak pack into the
/// header's one `u32` as `start | end << 15 | streak << 30`.
const UNIT: usize = 256;
const OFF_BITS: u32 = 15;
const OFF_MASK: u32 = (1 << OFF_BITS) - 1;
const STREAK_SHIFT: u32 = 2 * OFF_BITS;
const _: () = assert!(SEGMENT / UNIT <= OFF_MASK as usize);
const _: () = assert!(SIDECAR_DEFER_STREAK <= 3);

/// `(start, end, streak)` of a packed window, offsets in bytes.
#[inline]
fn unpack(window: u32) -> (usize, usize, u32) {
    (
        (window & OFF_MASK) as usize * UNIT,
        ((window >> OFF_BITS) & OFF_MASK) as usize * UNIT,
        window >> STREAK_SHIFT,
    )
}

impl AllocCore {
    /// True while `small_cur`'s window is open: bump in `(start, end)` and
    /// room for one `block_size` block, so a skipped drain always ends in a
    /// carve that advances the bump.
    #[inline]
    pub(super) fn sidecar_drains_deferred(&self, block_size: usize) -> bool {
        let meta = SegmentMeta::new(self.small_cur);
        let (start, end, _) = unpack(meta.sidecar_defer_window_of());
        let bump = meta.bump_of();
        start < bump && bump < end && align_up(bump, block_size) + block_size <= SEGMENT
    }

    /// Record a linear scan that drained `drained_bytes` of sidecar payload
    /// range and reclaimed nothing: extend the streak and, once it reaches
    /// `SIDECAR_DEFER_STREAK`, open the window. `start` rounds up and `end`
    /// down, so rounding only narrows the window.
    #[inline]
    pub(super) fn note_empty_sidecar_scan(&mut self, drained_bytes: usize) {
        let mut meta = SegmentMeta::new(self.small_cur);
        let (_, _, streak) = unpack(meta.sidecar_defer_window_of());
        let streak = (streak + 1).min(3);
        let mut window = streak << STREAK_SHIFT;
        if streak >= SIDECAR_DEFER_STREAK {
            let bump = meta.bump_of();
            let start = bump.div_ceil(UNIT);
            let end = (bump
                .saturating_add(drained_bytes >> SIDECAR_DEFER_SHIFT)
                .min(SEGMENT))
                / UNIT;
            if end > start {
                // Both are <= SEGMENT / UNIT <= OFF_MASK (const assert above).
                window |= start as u32 | (end as u32) << OFF_BITS;
            }
        }
        meta.set_sidecar_defer_window(window);
    }

    /// A drain reclaimed something: reset the streak and close the window.
    #[inline]
    pub(super) fn reset_sidecar_defer_streak(&mut self) {
        let mut meta = SegmentMeta::new(self.small_cur);
        if meta.sidecar_defer_window_of() != 0 {
            meta.set_sidecar_defer_window(0);
        }
    }

    /// U31 test-only: `(bump, window start, window end, streak)` of
    /// `small_cur` in bytes (`end == 0` = no window; open while
    /// `start < bump < end`).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_sidecar_defer_window(&self) -> (usize, usize, usize, u32) {
        let meta = SegmentMeta::new(self.small_cur);
        let (start, end, streak) = unpack(meta.sidecar_defer_window_of());
        (meta.bump_of(), start, end, streak)
    }

    /// U31 test-only: close `small_cur`'s window and reset its streak so the
    /// next discovery drains.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_close_sidecar_defer_window(&mut self) {
        SegmentMeta::new(self.small_cur).set_sidecar_defer_window(0);
    }
}
