//! Field-specific owner-only accessors for [`SegmentMeta`] (mechanical split
//! of `segment_header.rs`, task R6-CQ-7c).

use super::node::Node;
use super::segment_header::{SegmentHeader, SegmentMeta};

impl SegmentMeta {
    // -------------------------------------------------------------------
    // Phase 35 (M6 decommit) — field-specific owner-only accessors for the
    // `live_count` and `decommitted` fields. Identical discipline to
    // `bump_of`/`set_bump`: a single-word load/store at the field's
    // `offset_of!` offset through the `node` seam, so this file stays
    // `unsafe`-free. Owner-only (the owning thread is the sole mutator of
    // both fields — own-thread alloc/free and the owner-side ring drain;
    // the cross-thread freer never touches them), so a plain field
    // read/write is race-free, exactly as for `bump`.
    // -------------------------------------------------------------------

    /// Read the owner-only `live_count` (number of carved-and-not-free blocks).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn live_count_of(&self) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, live_count);
        Node::read_u32(Node::offset(self.base, off) as *const u32)
    }

    /// Write the owner-only `live_count`.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    fn set_live_count(&mut self, value: u32) {
        let off = core::mem::offset_of!(SegmentHeader, live_count);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, value);
    }

    /// Increment `live_count` (a block was handed to the caller). Saturating so
    /// a corrupt/overflowed counter never wraps to zero and spuriously triggers
    /// a decommit of a non-empty segment (defence-in-depth; a real `live_count`
    /// is bounded by `SEGMENT / MIN_BLOCK` ≪ `u32::MAX`).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn inc_live(&mut self) {
        let v = self.live_count_of();
        self.set_live_count(v.saturating_add(1));
    }

    /// Add `n` to `live_count` in ONE load+store (E1, task W4 — batched carve).
    /// Equivalent to `n` sequential [`inc_live`](Self::inc_live) calls: the
    /// counter is owner-only (single-writer), so the intermediate per-block
    /// values are unobservable and collapsing them to one saturating add is
    /// byte-identical in the final state — the same D1-equivalence argument
    /// `drain_freelist_batch` uses for its batched `inc_live`. Saturating for
    /// the same defence-in-depth reason as `inc_live`.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn add_live(&mut self, n: u32) {
        let v = self.live_count_of();
        self.set_live_count(v.saturating_add(n));
    }

    /// Decrement `live_count` (a block was freed) and return the NEW value.
    /// Saturating at zero: a decrement below zero would indicate a double-free
    /// that slipped past the bitmap guard (it cannot, since the caller checks
    /// `is_free` first), but saturating keeps the counter sane rather than
    /// wrapping to `u32::MAX` and permanently suppressing decommit.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn dec_live(&mut self) -> u32 {
        let v = self.live_count_of();
        let new = v.saturating_sub(1);
        self.set_live_count(new);
        new
    }

    /// Subtract `n` from `live_count` in ONE load+store and return the NEW
    /// value (E3, task W4 — batched flush). Equivalent to `n` sequential
    /// [`dec_live`](Self::dec_live) calls: the counter is owner-only, so the
    /// intermediate per-block values are unobservable and collapsing them to one
    /// saturating sub is byte-identical in the final value. Saturating at zero
    /// for the same defence-in-depth reason as `dec_live` (a real flush never
    /// removes more live blocks than exist).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn sub_live(&mut self, n: u32) -> u32 {
        let v = self.live_count_of();
        let new = v.saturating_sub(n);
        self.set_live_count(new);
        new
    }

    /// Read the owner-only `decommitted` flag (true ⟺ payload pages are
    /// currently returned to the OS).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn is_decommitted(&self) -> bool {
        let off = core::mem::offset_of!(SegmentHeader, decommitted);
        Node::read_u32(Node::offset(self.base, off) as *const u32) != 0
    }

    /// Set/clear the owner-only `decommitted` flag.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn set_decommitted(&mut self, value: bool) {
        let off = core::mem::offset_of!(SegmentHeader, decommitted);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, u32::from(value));
    }

    // -------------------------------------------------------------------
    // RAD-3 (E2, task #56) — field-specific owner-only accessors for the
    // empty-small-segment hysteresis pool's intrusive doubly-linked list
    // (`pool_next`/`pool_prev`). Identical discipline to `live_count`/
    // `decommitted`: a single-word load/store at the field's `offset_of!`
    // offset through the `node` seam. Owner-only: only `AllocCore`'s pool
    // methods (`alloc_core_small_pool.rs`), running exclusively on the
    // segment's owning thread, ever read or write these fields.
    // -------------------------------------------------------------------

    /// Read the owner-only `pool_next` link (the next more-recently-pooled
    /// segment towards the pool's HEAD, or `null` if this is the head / the
    /// segment is not pooled).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn pool_next_of(&self) -> *mut u8 {
        let off = core::mem::offset_of!(SegmentHeader, pool_next);
        Node::read_ptr_mut(Node::offset(self.base, off) as *const *mut u8)
    }

    /// Write the owner-only `pool_next` link.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn set_pool_next(&mut self, value: *mut u8) {
        let off = core::mem::offset_of!(SegmentHeader, pool_next);
        Node::write_ptr_mut(Node::offset(self.base, off) as *mut *mut u8, value);
    }

    /// Read the owner-only `pool_prev` link (the previous — closer to the
    /// pool's TAIL — pooled segment, or `null` if this is the tail / the
    /// segment is not pooled).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn pool_prev_of(&self) -> *mut u8 {
        let off = core::mem::offset_of!(SegmentHeader, pool_prev);
        Node::read_ptr_mut(Node::offset(self.base, off) as *const *mut u8)
    }

    /// Write the owner-only `pool_prev` link.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn set_pool_prev(&mut self, value: *mut u8) {
        let off = core::mem::offset_of!(SegmentHeader, pool_prev);
        Node::write_ptr_mut(Node::offset(self.base, off) as *mut *mut u8, value);
    }

    // -------------------------------------------------------------------
    // Phase B (numa-aware) — field-specific owner-only accessor for the
    // `node_id` field. Identical discipline to `live_count`/`decommitted`:
    // a single-word load/store at the field's `offset_of!` offset through
    // the `node` seam, keeping this file `unsafe`-free. Owner-only (written
    // once at segment init, never mutated thereafter; no cross-thread reader
    // ever touches it). Both accessors are gated on `numa-aware` — without
    // the feature the field is inert dead data.
    //
    // The sentinel value (`NO_NODE_RAW`) matches `numa::NO_NODE` (both are
    // `u32::MAX`); the compile-time assert below enforces this identity so
    // callers can compare `node_id_of(base) != numa::NO_NODE` without risk
    // of the two constants diverging.
    // -------------------------------------------------------------------

    /// Read the NUMA node stored in this segment's header (`NO_NODE_RAW` if
    /// the segment was not bound to a specific node).
    #[cfg(feature = "numa-aware")]
    pub(crate) fn node_id_of(&self) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, node_id);
        Node::read_u32(Node::offset(self.base, off) as *const u32)
    }

    /// Write the NUMA node into this segment's header.  Called once, at
    /// segment-init time, immediately after the full header is written.
    #[cfg(feature = "numa-aware")]
    pub(crate) fn set_node_id(&mut self, node: u32) {
        let off = core::mem::offset_of!(SegmentHeader, node_id);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, node);
    }

    // -------------------------------------------------------------------
    // B1 (R7 Workstream B) — field-specific owner-only accessors for the
    // `committed_payload_end` frontier. Identical discipline to
    // `live_count`/`decommitted`: a single-word load/store at the field's
    // `offset_of!` offset through the `node` seam. Owner-only: only the
    // owning thread reads/writes this field (at segment-init time and,
    // in B2, when growing the commit frontier on a carve that would exceed
    // the current frontier). A plain field read/write is race-free.
    // -------------------------------------------------------------------

    /// Read the owner-only `committed_payload_end` frontier (the byte offset
    /// from the segment base up to which payload pages are committed).
    ///
    /// R12-9 (task #260): gated on `any(...)` of the two split lazy-commit
    /// sub-features — this accessor is shared verbatim by both the
    /// primordial and ordinary-small-segment reservation/carve paths.
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    #[inline(always)]
    pub(crate) fn committed_payload_end_of(&self) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, committed_payload_end);
        Node::read_usize(Node::offset(self.base, off) as *const usize)
    }

    /// Write the owner-only `committed_payload_end` frontier.
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    #[inline(always)]
    pub(crate) fn set_committed_payload_end(&mut self, value: usize) {
        let off = core::mem::offset_of!(SegmentHeader, committed_payload_end);
        Node::write_usize(Node::offset(self.base, off) as *mut usize, value);
    }

    // -------------------------------------------------------------------
    // PERF-PASS-4 (G9/C2, task #52) — field-specific owner-only accessor for
    // the `ring_drain_head` drain-guard cache. Identical discipline to
    // `live_count`/`decommitted`/`node_id`: a single-word load/store at the
    // field's `offset_of!` offset through the `node` seam. Owner-only: only
    // `find_segment_with_free_impl`'s drain guard (running exclusively on the
    // segment's owning thread, the same thread that calls
    // `RemoteFreeRing::drain`) reads or writes this field.
    // -------------------------------------------------------------------

    /// Read the owner-cached `RemoteFreeRing` head, as of the last drain (or
    /// segment init, if never drained).
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn ring_drain_head_of(&self) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, ring_drain_head);
        Node::read_u32(Node::offset(self.base, off) as *const u32)
    }

    /// Write the owner-cached `RemoteFreeRing` head. Called after a drain (real
    /// or skipped) to record the ring's current `head` for the next guard
    /// check.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn set_ring_drain_head(&mut self, value: u32) {
        let off = core::mem::offset_of!(SegmentHeader, ring_drain_head);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, value);
    }

    // -------------------------------------------------------------------
    // R12-10 (task #261, `virgin-zero-skip`) — field-specific owner-only
    // accessors for the `payload_virgin` flag. Identical discipline to
    // `live_count`/`decommitted`: a single-word load/store at the field's
    // `offset_of!` offset through the `node` seam. Owner-only: written only
    // by `reserve_small_segment` (fresh reservation) and
    // `decommit_empty_segment_impl`'s retain leg (defensive clear on
    // in-place decommit); read only by `carve_block`/`carve_batch`. No
    // cross-thread reader or writer ever touches this field — see the
    // field's own doc comment on `SegmentHeader` for the full argument.
    // -------------------------------------------------------------------

    /// Read the owner-only `payload_virgin` flag (true ⟺ a bump-cursor carve
    /// on this segment right now is OS-zero-guaranteed).
    #[cfg(feature = "virgin-zero-skip")]
    #[inline(always)]
    pub(crate) fn payload_virgin_of(&self) -> bool {
        let off = core::mem::offset_of!(SegmentHeader, payload_virgin);
        Node::read_u32(Node::offset(self.base, off) as *const u32) != 0
    }

    /// Set/clear the owner-only `payload_virgin` flag.
    #[cfg(feature = "virgin-zero-skip")]
    #[inline(always)]
    pub(crate) fn set_payload_virgin(&mut self, value: bool) {
        let off = core::mem::offset_of!(SegmentHeader, payload_virgin);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, u32::from(value));
    }

    /// Stamp the `owner_thread_free` field ONLY (not a full-struct
    /// `write_header`). The stamping path runs on the owning thread and writes
    /// the field at most once per segment (when it transitions null → the
    /// heap's inline TFS head address); cross-thread readers use the
    /// field-specific [`SegmentHeader::owner_thread_free_at`]. A single-word
    /// field write here cannot race with a Remote's single-word field read of
    /// a disjoint header field.
    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    pub(crate) fn stamp_owner_thread_free(
        &mut self,
        head: *const core::sync::atomic::AtomicPtr<u8>,
    ) {
        let off = core::mem::offset_of!(SegmentHeader, owner_thread_free);
        Node::write_ptr(
            Node::offset(self.base, off) as *mut *const core::sync::atomic::AtomicPtr<u8>,
            head,
        );
    }
}
