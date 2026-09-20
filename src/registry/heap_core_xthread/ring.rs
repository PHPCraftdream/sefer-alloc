//! Heap-overflow ring addressing helpers of the former flat
//! `heap_core_xthread.rs` (reorg fix-up step): the `impl HeapCore` methods
//! that resolve a segment's owning slot / second-chance `HeapOverflow` ring
//! and push into it — the heap-overflow ring addressing helpers shared by
//! the retry-push (`push_with_overflow_retry`, in `overflow.rs`) and the
//! drain path (`drain_heap_overflow`, in `drain.rs`). Pure code-movement
//! sibling file; no behavior changed.

use core::sync::atomic::Ordering;

use crate::alloc_core::segment_header::SegmentMeta;

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// RAD-4b (task #72): resolve `base`'s owning [`HeapSlot`](crate::registry::heap_slot::HeapSlot)
    /// from its `owner_state` header stamp and push `(base, packed)` onto
    /// that slot's [`HeapOverflow`](crate::registry::heap_overflow::HeapOverflow) ring.
    /// Returns `false` if the owner id is out of range (defensive — should
    /// be unreachable for a live, correctly-stamped segment) or the
    /// second-chance ring is itself saturated.
    ///
    /// `owner_state` is read Relaxed: this is the SAME diagnostic-strength
    /// read `dbg_owner_id_for` already performs cross-thread (the id is
    /// written once per segment-lifetime by the owner's `stamp_segment_owner`
    /// and never concurrently mutated by a second writer — the single-writer
    /// invariant on `owner_state` that every other cross-thread reader of
    /// this field already relies on, e.g. `dealloc_foreign_slow`'s own
    /// `owner_thread_free_at` read a few lines above this call site's
    /// caller). A transient stale read (segment recycled and re-stamped
    /// between this load and the array index below) resolves to either the
    /// SAME heap (harmless) or a DIFFERENT live heap's slot (the pushed
    /// entry sits in the wrong heap's overflow ring, drained on ITS next
    /// opportunistic pass — not a correctness hazard: `HeapOverflow::drain`'s
    /// `reclaim_offset(_checked)` call independently re-validates `base`'s
    /// `magic`/`kind`/bounds before touching anything, exactly as the
    /// existing per-segment ring drain already does for the identical class
    /// of stale-entry hazard).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    pub(super) fn push_to_heap_overflow(base: *mut u8, packed: u32) -> bool {
        match Self::resolve_heap_overflow(base) {
            Some(overflow) => overflow.push(base, packed),
            None => false, // Defensive: unstamped/garbled owner id.
        }
    }

    /// R6-OPT-P0-4: factored out of [`push_to_heap_overflow`](
    /// Self::push_to_heap_overflow) so the bounded spin-retry loop in
    /// [`push_with_overflow_retry`](Self::push_with_overflow_retry) can
    /// resolve `base`'s owning [`HeapOverflow`](crate::registry::heap_overflow::HeapOverflow)
    /// ONCE before the loop and reuse the `&'static` reference across up to
    /// [`RETRY_ROUND_SPINS`] × [`RETRY_ROUND_SAFETY_CAP`] poll iterations,
    /// instead of
    /// re-reading the `owner_state` header atomic and re-indexing the
    /// registry's slot array on EVERY poll. The re-resolution cost (an extra
    /// atomic load plus an array index, repeated thousands of times) was
    /// measured to matter under contention: it slows this loop's effective
    /// poll rate enough to visibly increase `DBG_RING_PUSH_RETRY_EXHAUSTED`
    /// flakes on `tests/remote_fanin.rs::remote_fanin_high_contention_
    /// budget_is_sufficient` specifically when the host machine is ALSO under
    /// concurrent CPU load (multiple `cargo`/build processes contending for
    /// cores) — a same-machine, same-code A/B (10 runs each) measured 1/10
    /// baseline-shaped flakes vs. 8/10 with per-iteration re-resolution,
    /// dropping back to a baseline-comparable rate once resolved once here.
    ///
    /// Same staleness argument as [`push_to_heap_overflow`]'s own doc comment
    /// applies UNCHANGED, just amortised across the loop instead of repeated
    /// per iteration: a transient stale read (segment recycled and
    /// re-stamped between this resolution and a later poll inside the loop)
    /// still resolves to either the SAME heap (harmless) or a DIFFERENT live
    /// heap's slot (the pushed entry sits in the wrong heap's overflow ring,
    /// drained on ITS next opportunistic pass — not a correctness hazard, see
    /// that doc comment for the full argument). Returns `None` if the owner
    /// id is out of range (defensive — should be unreachable for a live,
    /// correctly-stamped segment).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    pub(super) fn resolve_heap_overflow(
        base: *mut u8,
    ) -> Option<&'static crate::registry::heap_overflow::HeapOverflow> {
        use crate::alloc_core::segment_header::unpack_owner_id;
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let owner_id = unpack_owner_id(owner_atomic.load(Ordering::Relaxed));
        let reg = crate::registry::bootstrap::ensure();
        let idx = owner_id as usize;
        if idx >= crate::registry::bootstrap::MAX_HEAPS {
            return None; // Defensive: unstamped/garbled owner id.
        }
        // R6-OPT-P0-2: `idx < MAX_HEAPS` just checked; `slot_or_none` resolves
        // it through the chunked slot array (materialising the owning chunk if
        // needed — sound here because this index was read off a LIVE segment's
        // owner stamp, i.e. some earlier `claim()` already materialised this
        // chunk; a fresh materialisation would still be correct, just redundant
        // with that earlier one).
        //
        // R34-15/task #534: `slot_or_none` returns `None` on
        // chunk-materialisation OOM instead of aborting; `map` folds the `None`
        // into this function's existing defensive `None` return (same shape as
        // the garbled-id check above). F-3: see `resolve_dirty_bit_target`'s
        // doc comment for why a garbled-but-in-range id reaching here is
        // harmless for legitimate cross-thread frees.
        reg.slot_or_none(idx).map(|slot| &slot.overflow)
    }

    /// Advisory owner-liveness probe gating
    /// [`push_with_overflow_retry`](Self::push_with_overflow_retry)'s spin
    /// window: `true` iff `base`'s owning registry slot is currently
    /// `STATE_LIVE` — i.e. some thread exists that will (lazily, on its alloc
    /// path) drain this segment's ring, so waiting for that drain is
    /// meaningful. Resolution is the same `owner_state` → `unpack_owner_id` →
    /// `slots[idx]` walk [`push_to_heap_overflow`](Self::push_to_heap_overflow)
    /// performs (see its doc comment for why the Relaxed `owner_state` read is
    /// sound cross-thread).
    ///
    /// **Advisory, not authoritative — both stale outcomes are benign.** The
    /// slot's `state` is read Relaxed with no generation check, so this can
    /// race claim/recycle in either direction:
    /// - stale `LIVE` (owner exited just after the load): ONE free wastes one
    ///   spin budget; the NEXT free re-probes and sees `FREE`. Bounded,
    ///   one-off — not the per-free multiplication this gate exists to stop.
    /// - stale `FREE` (slot re-claimed just after the load): the push skips
    ///   ahead to the `HeapOverflow` ring, whose entries the new claimant
    ///   drains on its own schedule — the same destination those entries had
    ///   anyway. No block is lost that the spin would have saved.
    ///
    /// An out-of-range id (`OWNER_ID_NONE` — an unstamped early segment, or
    /// the process-global fallback heap, whose `id = u32::MAX` masks to
    /// `OWNER_ID_NONE` under `pack_owner`'s 31-bit id field) has no slot to
    /// consult; report "live" to preserve RAD-4's original unconditional spin
    /// there (the fallback heap is process-lived and drains on its own
    /// allocs, so waiting for it is meaningful).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    pub(super) fn owner_slot_is_live(base: *mut u8) -> bool {
        use crate::alloc_core::segment_header::unpack_owner_id;
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let idx = unpack_owner_id(owner_atomic.load(Ordering::Relaxed)) as usize;
        if idx >= crate::registry::bootstrap::MAX_HEAPS {
            return true;
        }
        // R6-OPT-P0-2: `slot_or_none` resolves through the chunked slot array
        // — see `resolve_heap_overflow`'s identical rationale above for why a
        // (redundant, in practice) chunk materialisation here is sound.
        //
        // R34-15/task #534: on chunk-materialisation OOM, `slot_or_none`
        // returns `None` instead of aborting. We report `true` (live) to
        // preserve RAD-4's original unconditional-spin behaviour — the SAME
        // default the out-of-range id branch above uses — so the caller's
        // spin window is not short-circuited by a transient OOM.
        match crate::registry::bootstrap::ensure().slot_or_none(idx) {
            Some(slot) => {
                slot.state.load(Ordering::Relaxed) == crate::registry::heap_slot::STATE_LIVE
            }
            None => true, // Chunk-materialisation OOM — preserve spin (R34-15).
        }
    }
}
