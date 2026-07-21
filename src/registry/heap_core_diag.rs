//! Diagnostics / test-only hooks for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R4-10).
//!
//! This file holds the `impl HeapCore { .. }` block for the inspection
//! test hooks (`dbg_owner_id_for`, `dbg_tcache_count`, etc.).
//! Pure code-movement sibling of `heap_core.rs`; no behavior changed.

use core::alloc::Layout;

use crate::alloc_core::os;
use crate::alloc_core::segment_header::SegmentMeta;
use core::sync::atomic::Ordering;

use super::heap_core::HeapCore;

impl HeapCore {
    /// TEST-ONLY (P4): read the `owner_id` stamped in the segment header of
    /// the segment that contains `ptr`. Returns `None` if `ptr` is not in a
    /// segment owned by this heap's substrate. Used by
    /// `tests/heap_core_tcache_stamp.rs` to verify the stamp-hoist wrote
    /// the correct ownership.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    pub fn dbg_owner_id_for(&self, ptr: *mut u8) -> Option<u32> {
        use crate::alloc_core::segment_header::{unpack_owner_id, SegmentMeta};
        let base = os::segment_base_of_ptr(ptr);
        if !self.core.segment_bases().any(|b| b == base) {
            return None;
        }
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let word = owner_atomic.load(Ordering::Relaxed);
        Some(unpack_owner_id(word))
    }

    /// TEST-ONLY (P4): the cached `last_stamped_segment` base, or null if
    /// no segment has been stamped yet. Allows tests to observe whether the
    /// stamp-cache was updated without re-stamping.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    pub fn dbg_last_stamped_segment(&self) -> *mut u8 {
        self.last_stamped_segment
    }

    /// TEST-ONLY (P7): read the magazine count for class `c`. Widened to
    /// `u16` at this test-only boundary (task #53 shrank the internal
    /// storage to `u8` — see `PerClass::count` — but keeps this accessor's
    /// return type stable for existing callers).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_tcache_count(&self, c: usize) -> u16 {
        self.tcache.classes[c].count as u16
    }

    /// TEST-ONLY (task D3): resolve the size class index for `layout`, the
    /// same classification `alloc` uses to index `tcache.classes[c].slots`/
    /// `.count`.
    /// Delegates to [`AllocCore::dbg_layout_class_for`]; exposed at the
    /// `HeapCore` level because `core` is `pub(crate)` and external
    /// integration tests only see `HeapCore`/`HeapRegistry`.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_class_for(&self, layout: Layout) -> Option<usize> {
        self.core.dbg_layout_class_for(layout)
    }

    /// TEST-ONLY (task D3): the per-class refill amount `alloc`'s
    /// magazine-miss path actually uses for class `c` — i.e.
    /// `super::tcache::refill_n_for_class(SizeClasses::block_size(c))`, the
    /// exact expression `alloc` evaluates. Lets a test assert the byte-budget
    /// clamp fired for a given class without duplicating (and risking
    /// drifting from) the formula.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub fn dbg_refill_n_for_class(&self, c: usize) -> usize {
        super::tcache::refill_n_for_class(crate::alloc_core::size_classes::SizeClasses::block_size(
            c,
        ))
    }

    /// TEST-ONLY (task R2/#154): push `ptr`'s segment-relative offset — packed
    /// with `class_idx` — into its segment's `RemoteFreeRing`, exactly as a
    /// cross-thread freer's `dealloc_routing` Variant-2 push would. Thin
    /// delegation to [`AllocCore::dbg_push_to_ring`]; exposed at the `HeapCore`
    /// level so the ring↔magazine residual-limit pinning test
    /// (`tests/regression_xthread_double_free_residual.rs`) can simulate a
    /// remote free while driving the magazine through `HeapCore`. Returns
    /// `false` if the ring was full or `ptr` is not one of this heap's segments.
    /// Zero production impact: `#[doc(hidden)]`, test-only, delegates to an
    /// existing hook.
    ///
    /// # Safety
    ///
    /// `unsafe fn` (R6-MS-4) for exactly the same reason as
    /// [`AllocCore::dbg_push_to_ring`]: this thin delegation is the producer
    /// side of the cross-thread free simulation, and a safe wrapper would leave
    /// the round5 `memory_safety_review` R5-MS-4 stale-note→double-issue chain
    /// open through `HeapCore` (the residual tests reach the seam through this
    /// very wrapper). The caller must honour the identical contract: `ptr` is a
    /// live block in a segment owned by this heap; this push is at most one
    /// logical remote free (no `dealloc`/`flush_class`/`alloc`-re-issue of `ptr`
    /// between this push and the consuming drain); and `class_idx` is the
    /// block's actual allocated class. See the delegated fn's `# Safety` section
    /// for the full rationale and the defensive-guard caveats.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    #[allow(unsafe_code)] // R6-MS-4: `unsafe fn` boundary (delegation to the unsafe producer).
    pub unsafe fn dbg_push_to_ring(&self, ptr: *mut u8, class_idx: usize) -> bool {
        // SAFETY (R6-MS-4): this method carries the identical `# Safety`
        // contract as the delegated `AllocCore::dbg_push_to_ring` and is itself
        // `unsafe fn`, so the obligation is forwarded to THIS caller verbatim.
        unsafe { self.core.dbg_push_to_ring(ptr, class_idx) }
    }

    /// TEST-ONLY (task R2/#154): drain every owned segment's `RemoteFreeRing`
    /// into its `BinTable`, exactly as the alloc slow path's lazy drain does,
    /// but unconditionally. Task #164: routes through the same magazine
    /// predicate as the production drain, so tests exercise the real
    /// decision path.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_drain_all_rings(&mut self) {
        // Task #164: split borrow — `&self.tcache` (read) + `&mut self.core`
        // (write) are disjoint fields of HeapCore.
        //
        // RAD-5 (E4) GO/NO-GO EXPERIMENT: the magazine predicate is now the
        // O(1) bitmap probe, matching the production `refill_magazine_slow`
        // predicate — see that function's identical replacement for the
        // rationale. `class_idx` is unused by the probe (residency is keyed
        // by offset, not class) but kept in the closure signature to match
        // the `dbg_drain_all_rings_checked`/`reclaim_offset_checked` `F: Fn(*mut
        // u8, usize) -> bool` contract.
        #[cfg(feature = "fastbin")]
        {
            self.core.dbg_drain_all_rings_checked(&|ptr, _class_idx| {
                let pbase = os::segment_base_of_ptr(ptr);
                let poff = (ptr as usize - pbase as usize) as u32;
                SegmentMeta::new(pbase)
                    .magazine_bitmap()
                    .is_in_magazine(poff)
            });
        }
        #[cfg(not(feature = "fastbin"))]
        self.core.dbg_drain_all_rings();
    }

    /// TEST-ONLY (Mechanism 2, task #51): force-drain this heap's
    /// empty-small-segment hysteresis pool (release + recycle every pooled
    /// segment). Forwards to `AllocCore::drain_small_pool` — the production
    /// teardown-trim primitive (see [`trim_for_recycle`](Self::trim_for_recycle)).
    /// Used by decommit tests that run through the `SeferAlloc`/`HeapRegistry` face
    /// (where `claim_with_config` cannot reliably disable the pool on a reused
    /// slot) to deterministically observe the decommit that a pooled segment
    /// would otherwise absorb. Returns the number of segments drained.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    pub fn dbg_drain_small_pool(&mut self) -> usize {
        self.core.drain_small_pool()
    }

    /// TEST-ONLY (R11-2): read a single directory bit for the segment that
    /// contains `ptr`, resolved via the segment header's `segment_id` (so the
    /// caller does not need crate-internal access to compute `slot_idx`).
    /// Returns `None` if the directory is not materialised or `ptr` is foreign.
    /// Thin delegation to `AllocCore::dbg_directory_get_bit` — exposed at the
    /// `HeapCore` level so integration tests driving cross-thread frees through
    /// `HeapCore::dealloc` can observe whether `drain_heap_overflow` synced the
    /// directory after reclaiming an overflow entry.
    #[doc(hidden)]
    #[cfg(feature = "alloc-segment-directory")]
    #[must_use]
    pub fn dbg_directory_bit_for_ptr(&self, ptr: *mut u8, class_idx: usize) -> Option<bool> {
        use crate::alloc_core::segment_header::SegmentHeader;
        let base = os::segment_base_of_ptr(ptr);
        let sid = SegmentHeader::segment_id_at(base) as usize;
        self.core.dbg_directory_get_bit(class_idx, sid)
    }

    /// TEST-ONLY (R11-2): the number of empty small segments currently
    /// retained in this heap's hysteresis pool. Thin delegation to
    /// `AllocCore::dbg_pooled_count` — exposed at the `HeapCore` level so
    /// integration tests can assert that a segment emptied via an overflow-ring
    /// reclaim was actually pooled (not left as an ordinary registered
    /// segment by the pre-R11-2 bug that dropped the pool/release signal).
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_pooled_count(&self) -> usize {
        self.core.dbg_pooled_count()
    }

    /// TEST-ONLY (R11-2): resolve the base address of the segment that
    /// contains `ptr`. Thin delegation to `alloc_core::os::segment_base_of_ptr`
    /// — exposed at the `HeapCore` level because `alloc_core::os` is
    /// `pub(crate)` and integration tests in `tests/` only see the crate's
    /// true `pub` surface. Lets a test verify two pointers share a segment
    /// (a same-segment sanity check on the test's own construction) without
    /// reaching into crate-internal modules.
    #[doc(hidden)]
    #[cfg(feature = "alloc-global")]
    #[must_use]
    pub fn dbg_segment_base_of_ptr(&self, ptr: *mut u8) -> *mut u8 {
        os::segment_base_of_ptr(ptr)
    }

    /// TEST-ONLY (R11-2): the owner-only `live_count` of `ptr`'s segment, or
    /// `None` if `ptr` is foreign / not small/primordial. Thin delegation to
    /// `AllocCore::dbg_live_count_for` — exposed at the `HeapCore` level so an
    /// integration test can drive a segment down to EXACTLY zero live blocks
    /// (reading the exact remaining count at each step) without guessing how
    /// many blocks a given segment holds.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn dbg_live_count_for(&self, ptr: *mut u8) -> Option<u32> {
        self.core.dbg_live_count_for(ptr)
    }
}
