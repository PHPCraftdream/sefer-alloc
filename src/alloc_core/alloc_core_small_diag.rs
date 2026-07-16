//! Diagnostics / test-only hooks for [`AllocCore`] (mechanical split of
//! `alloc_core_small.rs`, task R4-10).
//!
//! This file holds the `impl AllocCore { .. }` block for the inspection and
//! corruption test hooks (`dbg_carve_batch`, `dbg_freelist_head_for`, etc.).
//! Pure code-movement sibling of `alloc_core_small.rs`; no behavior changed.

#[cfg(feature = "hardened")]
use core::ptr::NonNull;

use super::node::Node;
use super::os;
use super::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
use super::size_classes::{SizeClasses, SMALL_CLASS_COUNT};

use super::alloc_core::AllocCore;

impl AllocCore {
    /// TEST-ONLY (E1, task W4): drive [`carve_batch`](Self::carve_batch)
    /// directly (it is a private internal), so the equivalence regression test
    /// can carve a run and inspect the exact block set without going through the
    /// magazine. Returns the number of blocks carved into `out`.
    #[doc(hidden)]
    pub fn dbg_carve_batch(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize {
        let block_size = SizeClasses::block_size(class_idx);
        self.carve_batch(class_idx, block_size, out)
    }

    /// TEST-ONLY (Э7, task #161): the segment-relative offset of the head of
    /// `ptr`'s segment's `BinTable[class_idx]` free list, or `FREE_LIST_NULL`
    /// (`u32::MAX`) if the list is empty. Lets the batch-drain regression test
    /// observe `set_head`'s exact post-drain value directly (partial drain →
    /// remaining head; full drain → NULL).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_freelist_head_for(&self, ptr: *mut u8, class_idx: usize) -> u32 {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return FREE_LIST_NULL;
        }
        // R6-MS-3 (round5 memory_safety_review R5-MS-3): release-mode class-index
        // bounds guard. Belt-and-suspenders alongside `BinTable::head`'s own
        // check — this is a doc-hidden test hook taking a raw caller-controlled
        // `class_idx`, so an out-of-range value must short-circuit here (and not
        // rely on the callee) before any raw `BinTable` access. Under the old
        // `debug_assert!`-only guard this read out of bounds in `production`.
        if class_idx >= SMALL_CLASS_COUNT {
            return FREE_LIST_NULL;
        }
        SegmentMeta::new(base).bin_table().head(class_idx)
    }

    /// TEST-ONLY (Э7, task #161): whether `ptr`'s block is currently marked FREE
    /// (on a free list) in its segment's alloc bitmap — the M2 double-free bit.
    /// `false` ⟺ the block is ALLOCATED (handed out). Lets the batch-drain test
    /// assert every drained block ends bitmap-allocated, exactly as `pop_free`
    /// leaves it.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_is_free_for(&self, ptr: *mut u8) -> bool {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return false;
        }
        let off = (ptr as usize - base as usize) as u32;
        SegmentMeta::new(base).alloc_bitmap().is_free(off)
    }

    /// TEST-ONLY (UBFIX-7, M-3 counterfactual): overwrite the CURRENT freelist
    /// head block's intrusive `next` word for `class_idx` in `ptr`'s segment
    /// with an arbitrary raw pointer, simulating a UAF write into an
    /// already-freed block that corrupts the chain pointer the allocator will
    /// next trust. `next_raw` is written completely unvalidated (unlike the
    /// production `Node::write_next` call sites, which only ever write a
    /// pointer the allocator itself derived) — the caller is responsible for
    /// choosing an out-of-segment value to exercise the hardened guard in
    /// `pop_free`/`drain_freelist_batch`.
    ///
    /// Mirrors the established `dbg_stamp_*`-style field-corruption pattern
    /// (see `AllocCore::dbg_stamp_segment_id`/`dbg_stamp_kind_byte` in
    /// `alloc_core.rs`), applied to a freelist node's `next` word instead of a
    /// header field. Returns `false` (no-op) if the class's free list is
    /// currently empty (nothing to corrupt).
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer whose
    /// segment base is a real, mapped segment owned by this `AllocCore`. The
    /// callee computes `base` from `ptr` and writes an arbitrary `next_raw` into
    /// the head free-list node WITHOUT a membership check; passing an invalid,
    /// interior, stale or foreign `ptr` corrupts allocator metadata or triggers
    /// undefined behaviour.
    #[doc(hidden)]
    #[cfg(feature = "hardened")]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_corrupt_freelist_head_next(
        &self,
        ptr: *mut u8,
        class_idx: usize,
        next_raw: *mut u8,
    ) -> bool {
        let base = os::segment_base_of_ptr(ptr);
        let head_off = SegmentMeta::new(base).bin_table().head(class_idx);
        if head_off == FREE_LIST_NULL {
            return false;
        }
        let block_ptr = Node::deref(base, head_off as usize);
        let Some(block_nn) = NonNull::new(block_ptr) else {
            return false;
        };
        Node::write_next(block_nn, next_raw);
        true
    }

    /// TEST-ONLY (Э7, task #161): drive `drain_freelist_batch` directly on
    /// `ptr`'s segment so a regression test can observe partial/full-drain
    /// behaviour (return count, resulting `set_head`, per-block bitmap state) in
    /// isolation from the surrounding `refill_class_bump` carve logic.
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer whose
    /// segment is owned by this `AllocCore`. The callee computes `base` from
    /// `ptr` and mutates the free list WITHOUT a membership check; an invalid,
    /// interior, stale or foreign `ptr` corrupts allocator metadata or triggers
    /// undefined behaviour.
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_drain_freelist_batch(
        &mut self,
        ptr: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        let base = os::segment_base_of_ptr(ptr);
        self.drain_freelist_batch(base, class_idx, out)
    }

    /// TEST-ONLY (PERF-PASS-2, task #50): read `out.len()` raw bytes starting
    /// at `ptr`'s segment's `AllocBitmap` base, byte-for-byte, with NO
    /// interpretation (unlike `dbg_is_free_for`, which decodes a single bit
    /// for a specific block offset). Exists for the sub-part 2 (G5/C1)
    /// virgin-init-elision poison-then-assert counterfactual
    /// (`tests/regression_virgin_bitmap_skip.rs`): the test needs to inspect
    /// the WHOLE bitmap footprint of a freshly-reserved segment (including
    /// byte ranges no `alloc`/`dealloc` call has touched) to prove the OS
    /// handed back genuinely zeroed pages, not just that one class's bit
    /// happens to read as allocated (which `dbg_is_free_for` alone cannot
    /// distinguish from "never written" vs "explicitly zeroed").
    ///
    /// `out.len()` MUST be `<= AllocBitmap::FOOTPRINT` (release-asserted); the
    /// caller is responsible for not reading past the bitmap's own footprint
    /// (reading further would spill into the next metadata region, which this
    /// accessor does not guard against — test-only, not a production API).
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer whose
    /// segment is owned by this `AllocCore`. The callee computes `base` from
    /// `ptr` and reads raw bitmap bytes WITHOUT a membership check.
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_alloc_bitmap_bytes_for(&self, ptr: *mut u8, out: &mut [u8]) {
        assert!(
            out.len() <= super::alloc_bitmap::AllocBitmap::FOOTPRINT,
            "dbg_alloc_bitmap_bytes_for: out.len() exceeds AllocBitmap::FOOTPRINT"
        );
        let base = os::segment_base_of_ptr(ptr);
        let bitmap_base = Node::offset(base, SegLayout::alloc_bitmap_off());
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = Node::read_u8(Node::offset(bitmap_base, i));
        }
    }

    /// RAD-5 (E4) GO/NO-GO EXPERIMENT, TEST-ONLY: same byte-for-byte raw read
    /// as `dbg_alloc_bitmap_bytes_for`, over the second (magazine-residency)
    /// bitmap instead. Exists for the poison-then-assert counterfactual
    /// extension (`tests/regression_virgin_bitmap_skip.rs`) that proves the
    /// virgin-init skip is sound for this bitmap too, not just `AllocBitmap`.
    ///
    /// # Safety
    ///
    /// Same as [`dbg_alloc_bitmap_bytes_for`](Self::dbg_alloc_bitmap_bytes_for#safety):
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer.
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_magazine_bitmap_bytes_for(&self, ptr: *mut u8, out: &mut [u8]) {
        assert!(
            out.len() <= super::magazine_bitmap::MagazineBitmap::FOOTPRINT,
            "dbg_magazine_bitmap_bytes_for: out.len() exceeds MagazineBitmap::FOOTPRINT"
        );
        let base = os::segment_base_of_ptr(ptr);
        let bitmap_base = Node::offset(base, SegLayout::magazine_bitmap_off());
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = Node::read_u8(Node::offset(bitmap_base, i));
        }
    }

    /// TEST-ONLY (B1, R7 Workstream B): the `committed_payload_end` frontier
    /// of `ptr`'s segment — the byte offset from the segment base up to which
    /// payload pages are committed. Returns `None` if `ptr` is foreign or not
    /// a small/primordial segment. Returns `SEGMENT` on the eager path
    /// (feature-OFF, Unix, miri, or `numa-aware`); on the lazy path returns
    /// `small_meta_end() + LAZY_FIRST_CHUNK` for a fresh segment.
    #[doc(hidden)]
    #[must_use]
    #[cfg(feature = "alloc-lazy-commit")]
    pub fn dbg_committed_payload_end_for(&self, ptr: *mut u8) -> Option<usize> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).committed_payload_end_of())
    }

    /// TEST-ONLY (B2, R7 Workstream B): snapshot of the process-wide
    /// grow-commit counter. Returns the number of successful `commit_pages`
    /// calls on the grow-on-carve path since process start.
    #[doc(hidden)]
    #[must_use]
    #[cfg(feature = "alloc-lazy-commit")]
    pub fn dbg_grow_commit_count(&self) -> u64 {
        super::alloc_core_small::GROW_COMMIT_COUNT.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (B2, R7 Workstream B): arm the commit-failure fault injector.
    /// The next `n` calls to `os::commit_pages` will return `false` without
    /// touching the OS, simulating commit-charge exhaustion. After `n` failures
    /// subsequent calls proceed normally.
    #[doc(hidden)]
    #[cfg(feature = "alloc-lazy-commit")]
    pub fn dbg_arm_commit_fail(&self, n: u32) {
        os::COMMIT_FAIL_ARMED.store(n, core::sync::atomic::Ordering::Relaxed);
    }

    /// TEST-ONLY (B2, R7 Workstream B): the `GROW_CHUNK` constant (bytes).
    /// Exposed so tests can compute expected frontier values without
    /// hardcoding the crate's private constant.
    #[doc(hidden)]
    #[must_use]
    #[cfg(feature = "alloc-lazy-commit")]
    pub fn dbg_grow_chunk(&self) -> usize {
        super::alloc_core_small::GROW_CHUNK
    }

    /// TEST-ONLY (UBFIX-3, H-1/M-1 counterfactual): the segment-relative
    /// payload lower bound for `ptr`'s segment — the same `payload_start`
    /// (`Layout::primordial_meta_end()` for a primordial segment, else
    /// `Layout::small_meta_end()`) the H-1 guard in `dealloc_small`/
    /// `reclaim_offset`/`reclaim_offset_checked`/`flush_run` rejects offsets
    /// below. Exposed so a regression test can construct a metadata-region
    /// address (`base + k` for `k < payload_start`) without hardcoding the
    /// crate's private layout constants (`segment_header::Layout` is
    /// `pub(crate)`, unreachable from `tests/`).
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer whose
    /// segment is owned by this `AllocCore`. The callee reads the segment kind
    /// byte at the computed `base` WITHOUT a membership check; a dangling or
    /// foreign `ptr` triggers undefined behaviour.
    #[doc(hidden)]
    #[must_use]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_payload_start_for(&self, ptr: *mut u8) -> usize {
        let base = os::segment_base_of_ptr(ptr);
        if SegmentHeader::kind_at(base) == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        }
    }
}
