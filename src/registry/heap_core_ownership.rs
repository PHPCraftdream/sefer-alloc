//! Ownership / binding machinery for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R6-CQ-7b).
//!
//! This file holds the `impl HeapCore { .. }` block for the W3 slot-counter-
//! handle binding (`bind_thread_free`, `bind_overflow`, `thread_free_head`),
//! the OPT-C segment-ownership stamp (`stamp_segment_owner`), and the
//! production teardown-trim primitive (`trim_for_recycle`). Pure
//! code-movement sibling of `heap_core.rs`; no behavior changed.

#[cfg(feature = "alloc-xthread")]
use core::sync::atomic::AtomicPtr;
#[cfg(feature = "alloc-global")]
use core::sync::atomic::Ordering;

#[cfg(feature = "alloc-global")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::pack_owner;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::SegmentMeta;

use super::heap_core::HeapCore;

impl HeapCore {
    /// task H1: plant the stable `&'static` handle to THIS heap's slot-resident
    /// (or fallback-static) cross-thread free-stack head. Called once, right
    /// after the slot / fallback heap is materialised and before any allocation
    /// on this heap runs, by
    /// [`HeapRegistry::claim`](super::heap_registry::HeapRegistry::claim)
    /// (via `bind_slot_counters`) / `fallback::heap_ptr`. Idempotent — on a
    /// slot re-claim the handle already references the same `'static` word, so
    /// re-planting is a harmless no-op store. Same discipline as
    /// [`bind_tcache_hits`](Self::bind_tcache_hits).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn bind_thread_free(&mut self, head: &'static AtomicPtr<u8>) {
        self.thread_free = Some(head);
    }

    /// RAD-4b (task #72): plant the stable `&'static` handle to THIS heap's
    /// slot-resident [`HeapOverflow`](super::heap_overflow::HeapOverflow)
    /// ring. Same discipline as [`bind_thread_free`](Self::bind_thread_free) /
    /// [`bind_tcache_hits`](Self::bind_tcache_hits) — called once, right
    /// after the slot binds, from `bind_slot_counters`.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn bind_overflow(&mut self, overflow: &'static super::heap_overflow::HeapOverflow) {
        self.overflow = Some(overflow);
    }

    /// R7-A4: plant the stable `&'static` handle to THIS heap's slot-resident
    /// `dirty_segments` bitmap. Same discipline as `bind_overflow` / `bind_thread_free`.
    /// Called once, right after the slot binds, from `bind_slot_counters`.
    #[cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]
    pub(crate) fn bind_dirty_segments(
        &mut self,
        ds: &'static [core::sync::atomic::AtomicU64;
                     crate::alloc_core::segment_directory::WORDS_PER_CLASS],
    ) {
        self.core.dirty_segments = Some(ds);
    }

    /// The stable `*const AtomicPtr<u8>` head pointer of this heap's TFS, or
    /// null in the transient pre-bind window (no cross-thread stamping has
    /// happened yet → cross-thread frees to this heap's segments are a safe
    /// no-op). Used by the drain / routing paths on the owning thread. task H1:
    /// resolves to the OWNING slot's `thread_free` word (via the `&'static`
    /// handle), NOT an inline `HeapCore` field — so the returned address is
    /// outside every `&mut HeapCore` retag range.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn thread_free_head(&self) -> *const AtomicPtr<u8> {
        self.thread_free
            .map_or(core::ptr::null(), |h| h as *const AtomicPtr<u8>)
    }

    /// Stamp a segment's header with this heap's ownership. Two parts:
    ///
    /// 1. **`owner_state = LIVE(self.id, 0)`** — the ownership field. Set on
    ///    every alloc so cross-thread free routing can resolve a segment's
    ///    owning heap from its `owner_id`. Idempotent: a segment already
    ///    stamped with our id is left alone.
    /// 2. **(alloc-xthread only) `owner_thread_free` head pointer** — the
    ///    cross-thread free routing target, so a remote freer can find this
    ///    heap's TFS. Idempotent: only stamps if currently null.
    ///
    /// Called on the alloc path after a successful allocation. The segment is
    /// exclusively ours (single-writer invariant from the claim CAS), so the
    /// `owner_state` store is race-free.
    ///
    /// ## OPT-C fast path (task #66)
    ///
    /// `last_stamped_segment` caches the base of the most recently stamped
    /// segment. On a cache hit the function performs only a **Relaxed** load
    /// of `owner_state` and compares it with `self.id`. If they match, we
    /// know the segment is already stamped → return immediately with NO
    /// Release-store (the expensive part on x86 — an `MFENCE`-equivalent).
    /// On a miss or on a ownership mismatch the original slow path runs.
    ///
    /// The Relaxed load is safe because:
    /// - This is the **owning thread** — the single writer of `owner_state`
    ///   on this segment. A Relaxed load cannot race with our own prior
    ///   Release-store (same thread → SC-in-program-order).
    /// - A cache miss (base changed or Relaxed-load mismatch) falls through
    ///   to the slow path which restores the Acquire/Release protocol.
    #[inline(always)]
    pub(super) fn stamp_segment_owner(&mut self, ptr: *mut u8) {
        use crate::alloc_core::segment_header::{unpack_owner_id, OWNER_STATE_LIVE};
        let base = os::segment_base_of_ptr(ptr);

        // -----------------------------------------------------------------------
        // OPT-C fast path: cache-hit check.
        //
        // If the cached segment base matches the current allocation's segment
        // base, do a cheap Relaxed load of `owner_state` to confirm ownership.
        // If ownership is confirmed → early return (no Release-store, no memory
        // fence). If ownership is not confirmed (e.g., segment was recycled and
        // reset to OWNER_ID_NONE) → fall through to the slow path below.
        // -----------------------------------------------------------------------
        if base == self.last_stamped_segment && !self.last_stamped_segment.is_null() {
            // Cache hit: re-check ownership with a cheaper Relaxed load.
            // Owner-only read (we are the sole writer of owner_state on OUR
            // segments), so Relaxed ordering is race-free here.
            let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
            let cur = owner_atomic.load(Ordering::Relaxed);
            if unpack_owner_id(cur) == self.id {
                // Still our segment, already stamped. Skip the Release-store.
                // The alloc-xthread TFS stamp is also idempotent (once
                // stamped it stays); if `last_stamped_segment` is set then
                // the TFS was already written on the slow path below.
                return;
            }
            // Ownership mismatch (e.g., recycled segment): clear the cache
            // and run the slow path.
            self.last_stamped_segment = core::ptr::null_mut();
        }

        // -----------------------------------------------------------------------
        // Slow path: full Acquire-load + conditional Release-store.
        // -----------------------------------------------------------------------
        // `mut` is needed under `alloc-xthread` (the stamp branch below calls
        // `meta.stamp_owner_thread_free(&mut self)`). Silence the unused-mut
        // warning under plain `alloc-global` where the branch is absent.
        #[allow(unused_mut)]
        let mut meta = SegmentMeta::new(base);
        // 1. Stamp owner_state (ownership resolution).
        let owner_atomic = meta.owner_state_atomic();
        let cur = owner_atomic.load(Ordering::Acquire);
        if unpack_owner_id(cur) != self.id {
            let me = pack_owner(OWNER_STATE_LIVE, self.id, 0);
            // Release: a later cross-thread freer's Acquire read of owner_state
            // (to resolve the owning heap) must observe our stamp.
            owner_atomic.store(me, Ordering::Release);
        }
        // 2. (alloc-xthread) Stamp the TFS head for cross-thread routing.
        // Phase 12.5 (shard model): stamped ONCE, when the segment is first
        // allocated from, and NEVER cleared or re-stamped. The inline TFS
        // head's address is stable for the slot's lifetime (it does not
        // change across release→claim), so the stamp remains valid for as
        // long as the slot owns this segment — which is forever in the shard
        // model (segments do not leave their heap).
        //
        // Field-specific write (task #33 root-cause fix): we stamp ONLY the
        // `owner_thread_free` field via `stamp_owner_thread_free`, NOT a
        // full-struct `write_header`. A full-struct RMW here rewrote `bump`
        // and every other field, and — although the stamp itself runs on the
        // owning thread — the struct read it performed (`meta.header()`)
        // raced the Owner's own later `bump` writes is not the issue; the
        // issue is that `write_header` writes `magic`/`kind`/`bump` bytes
        // that a concurrent Remote `dealloc_routing` field-read may observe
        // mid-update. Writing only the `owner_thread_free` word touches bytes
        // disjoint from every field a Remote reads, so there is no race.
        // The single-writer invariant (the slot's owner is the sole writer
        // of its segments' headers) makes the plain field write race-free.
        #[cfg(feature = "alloc-xthread")]
        {
            let cur_head =
                crate::alloc_core::segment_header::SegmentHeader::owner_thread_free_at(base);
            if cur_head.is_null() {
                // Task #142: expose this atomic's provenance so a REMOTE
                // freer can reconstruct a wildcard pointer to it (via
                // `Node::atomic_ptr_ref` → `with_exposed_provenance_mut`)
                // rather than inheriting a reference provenance a concurrent
                // remote write would disable, corrupting other remotes' access
                // (see `Node::atomic_ptr_ref`).
                //
                // task H1: the head is the OWNING SLOT's `thread_free` word,
                // reached through the `&'static` handle planted at claim time
                // — NOT an inline `HeapCore` field. This is the whole point of
                // the H1 hoist: the exposed address is outside every `&mut
                // HeapCore` retag range, so a remote CAS onto it no longer
                // races the owner's `alloc(&mut self)` protector. `handle as
                // *const _` takes the slot field's stable address without any
                // `&mut self`-rooted retag; `expose_provenance` registers it
                // for the paired `with_exposed_provenance_mut`. `None` cannot
                // occur here — stamping runs only after the claim that planted
                // the handle (defensive: skip the stamp if somehow unbound).
                if let Some(handle) = self.thread_free {
                    let tf_ptr = handle as *const AtomicPtr<u8>;
                    let _ = tf_ptr.expose_provenance();
                    meta.stamp_owner_thread_free(tf_ptr as *const _);
                }
            }
        }

        // Slow path succeeded: cache the segment base so the next alloc from
        // the same segment takes the fast path.
        self.last_stamped_segment = base;
    }

    /// Production teardown trim (task #95 / N1): flush every tcache class,
    /// drain the small-segment pool, and evict the entire large cache.
    ///
    /// Called by the TLS `AbandonGuard::drop` on thread exit, BEFORE the
    /// `HeapRegistry::recycle` CAS flips the slot `LIVE → FREE`. At that
    /// point this thread is still the slot's sole owner/writer (same
    /// single-writer window every other mutation relies on), so no
    /// cross-thread quiescence is needed.
    ///
    /// **Why:** without this trim, a wave of short-lived threads leaves
    /// tcache-buffered blocks, pooled small segments (up to 16 MiB each),
    /// and cached large spans pinned on each recycled slot — RSS/commit
    /// stays proportional to the peak thread count, not the current load.
    /// Draining here returns retained memory to the OS on the cold thread-
    /// exit path (never on the alloc/dealloc hot path).
    ///
    /// Each sub-operation carries its own feature gate; in a build without
    /// the relevant feature the corresponding step compiles to nothing.
    pub(crate) fn trim_for_recycle(&mut self) {
        // Flush every tcache class → blocks return to segments → segments
        // may empty → decommit/release or pool.
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        self.flush_all_tcache();
        // Drain the small-segment hysteresis pool → release every pooled
        // segment to the OS. Evict the entire large cache → release every
        // cached span.
        #[cfg(feature = "alloc-decommit")]
        {
            self.core.drain_small_pool();
            self.core.evict_all();
        }
    }
}
