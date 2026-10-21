//! B4 §11 per-class discovery cursor (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS.md
//! §11): remember the segment the last discovery served each class from,
//! probe it before the next scan, and start that scan after it.

use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL};

use crate::alloc_core::alloc_core::{AllocCore, CLASS_CURSOR_NONE};

impl AllocCore {
    /// Is `base` a non-null small/primordial segment whose header slot id is
    /// `slot`? Validates a cursor hint before its bin is read.
    #[inline]
    fn live_small_slot_at(base: *mut u8, slot: usize) -> bool {
        !base.is_null()
            && matches!(
                SegmentHeader::kind_at(base),
                SegmentKind::Small | SegmentKind::Primordial
            )
            && SegmentHeader::segment_id_at(base) as usize == slot
    }

    /// B4 §11: next scan start = cursor slot + 1, or `None` (scan from 0)
    /// when no cursor is recorded or the slot is past the high-water mark.
    #[inline]
    pub(super) fn class_cursor_scan_start(&self, class_idx: usize, n: usize) -> Option<usize> {
        let slot = self.small_class_cursor[class_idx];
        if slot == CLASS_CURSOR_NONE {
            return None;
        }
        let slot = slot as usize;
        (slot < n).then_some(slot + 1)
    }

    /// B4 §11: remember `base` as the class's cursor after a discovery hit.
    /// Skipped for `small_cur` (tried before any discovery anyway).
    #[inline]
    pub(super) fn record_class_cursor(&mut self, class_idx: usize, base: *mut u8) {
        if base == self.small_cur {
            return;
        }
        // Slot ids are < MAX_SEGMENTS (4096), so they always fit `u16` and
        // never collide with the `u16::MAX` sentinel.
        let slot = SegmentHeader::segment_id_at(base) as usize;
        debug_assert!(slot < usize::from(CLASS_CURSOR_NONE));
        if let Ok(slot) = u16::try_from(slot) {
            self.small_class_cursor[class_idx] = slot;
        }
    }

    /// B4 §11: drop every cursor naming `slot` (release/recycle point).
    #[cfg(feature = "alloc-decommit")]
    #[inline]
    pub(in crate::alloc_core) fn invalidate_class_cursor_for_slot(&mut self, slot: usize) {
        for cursor in self.small_class_cursor.iter_mut() {
            if *cursor as usize == slot {
                *cursor = CLASS_CURSOR_NONE;
            }
        }
    }

    /// B4 §11: clear all cursors (heap (re-)claim, cold-trim directory
    /// release) so a new owner never inherits the previous owner's hints.
    #[cfg_attr(
        not(any(feature = "alloc-global", feature = "alloc-segment-directory")),
        allow(dead_code)
    )]
    #[inline]
    pub(crate) fn invalidate_all_class_cursors(&mut self) {
        self.small_class_cursor.fill(CLASS_CURSOR_NONE);
    }

    /// B4 §11 test-only: this class's cursor slot (`None` = none recorded).
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_class_cursor_slot(&self, class_idx: usize) -> Option<usize> {
        let slot = *self.small_class_cursor.get(class_idx)?;
        (slot != CLASS_CURSOR_NONE).then_some(slot as usize)
    }

    #[cfg(feature = "bench-internals")]
    #[inline]
    fn cursor_hit(&self) {
        crate::alloc_core::B4_CURSOR_HITS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    #[cfg(feature = "bench-internals")]
    #[inline]
    fn cursor_miss(&self) {
        crate::alloc_core::B4_CURSOR_MISSES.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    /// B4 §11: validate this class's cursor segment. `None` = no cursor,
    /// stale (cleared + miss), or drained bin (kept + miss — never a trusted
    /// negative; the caller runs discovery). On `Some`, the segment is
    /// unpooled and its bin is non-empty, ready for a pop/drain.
    #[inline(never)]
    fn cursor_probe_base(&mut self, class_idx: usize) -> Option<*mut u8> {
        let slot = self.small_class_cursor[class_idx];
        if slot == CLASS_CURSOR_NONE {
            return None;
        }
        let slot = slot as usize;
        let base = self.table.base_at(slot);
        let stale = base == self.small_cur || !Self::live_small_slot_at(base, slot);
        if stale {
            self.small_class_cursor[class_idx] = CLASS_CURSOR_NONE;
            #[cfg(feature = "bench-internals")]
            self.cursor_miss();
            return None;
        }
        if SegmentMeta::new(base).bin_table().head(class_idx) == FREE_LIST_NULL {
            // Valid segment, drained bin: keep the cursor (it still anchors the
            // scan start); the caller runs discovery.
            #[cfg(feature = "bench-internals")]
            self.cursor_miss();
            return None;
        }
        // numa-aware: a foreign-node cursor (a discovery fallback) must not
        // bypass discovery's local-node preference.
        #[cfg(feature = "numa-aware")]
        {
            let seg_node = SegmentMeta::new(base).node_id_of();
            if seg_node != self.current_node_cached()
                && seg_node != crate::alloc_core::segment_header::NO_NODE_RAW
            {
                self.small_class_cursor[class_idx] = CLASS_CURSOR_NONE;
                #[cfg(feature = "bench-internals")]
                self.cursor_miss();
                return None;
            }
        }
        #[cfg(feature = "alloc-decommit")]
        self.unpool_if_present(base);
        Some(base)
    }

    /// B4 §11: probe this class's discovery-cursor segment for one free
    /// block. `None` = fall through to discovery. D1′ phase-1b: infallible —
    /// the class-spill OOM is gone with the class leaves.
    #[inline]
    fn pop_from_class_cursor(&mut self, class_idx: usize) -> Option<*mut u8> {
        // Fast path: no cursor recorded — no call into the probe.
        if self.small_class_cursor[class_idx] == CLASS_CURSOR_NONE {
            return None;
        }
        let base = self.cursor_probe_base(class_idx)?;
        let popped = self.pop_free(base, class_idx);
        #[cfg(feature = "bench-internals")]
        if popped.is_some() {
            self.cursor_hit();
        } else {
            self.cursor_miss();
        }
        popped
    }

    /// B4 §11: batch-drain this class's cursor segment's bin into `out`.
    /// Same semantics as [`pop_from_class_cursor`](Self::pop_from_class_cursor).
    /// D1′ phase-1b: infallible — the class-spill OOM is gone.
    #[inline]
    pub(in crate::alloc_core) fn batch_drain_from_class_cursor(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        // Fast path: no cursor recorded — no call into the probe.
        if self.small_class_cursor[class_idx] == CLASS_CURSOR_NONE {
            return 0;
        }
        let Some(base) = self.cursor_probe_base(class_idx) else {
            return 0;
        };
        let n = self.try_drain_freelist_batch(base, class_idx, out);
        #[cfg(feature = "bench-internals")]
        if n != 0 {
            self.cursor_hit();
        } else {
            self.cursor_miss();
        }
        n
    }

    /// B4 §11: shared "cursor probe → discovery → pop" step of the small
    /// scalar path. `Some(ptr)` = block served; `None` = nothing found,
    /// caller carves. D1′ phase-1b: infallible — the class-spill OOM is gone.
    pub(in crate::alloc_core) fn alloc_small_via_discovery(
        &mut self,
        class_idx: usize,
    ) -> Option<*mut u8> {
        if let Some(ptr) = self.pop_from_class_cursor(class_idx) {
            return Some(ptr);
        }
        let seg = self.find_segment_with_free(class_idx)?;
        self.pop_free(seg, class_idx)
    }
}
