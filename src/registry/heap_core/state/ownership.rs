//! Canonical owner stamping and exclusive cold trim for [`HeapCore`].

#[cfg(feature = "alloc-global")]
use ::core::sync::atomic::Ordering;

#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::pack_owner;
#[cfg(feature = "alloc-global")]
use crate::alloc_core::segment_header::SegmentMeta;

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    /// Stamp this heap's canonical reservation root with its owner id.
    /// Foreign frees route exclusively through terminal descriptors, never
    /// through an owner-header pointer or an intrusive deferred stack.
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
    pub(crate) fn stamp_segment_owner(&mut self, ptr: *mut u8) {
        use crate::alloc_core::segment_header::{unpack_owner_id, OWNER_STATE_LIVE};
        let Some((base, _)) = self.core.canonical_block_of(ptr) else {
            std::process::abort();
        };

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
                return;
            }
            // Ownership mismatch (e.g., recycled segment): clear the cache
            // and run the slow path.
            self.last_stamped_segment = ::core::ptr::null_mut();
        }

        // -----------------------------------------------------------------------
        // Slow path: full Acquire-load + conditional Release-store.
        // -----------------------------------------------------------------------
        let meta = SegmentMeta::new(base);
        // 1. Stamp owner_state (ownership resolution).
        let owner_atomic = meta.owner_state_atomic();
        let cur = owner_atomic.load(Ordering::Acquire);
        if unpack_owner_id(cur) != self.id {
            let me = pack_owner(OWNER_STATE_LIVE, self.id, 0);
            // Release: a later cross-thread freer's Acquire read of owner_state
            // (to resolve the owning heap) must observe our stamp.
            owner_atomic.store(me, Ordering::Release);
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
        // Terminal sidecars require a full, bounded sweep independent of dirty
        // hints. Retire detached records before flushing magazines or releasing
        // reservations; a pre-publication producer keeps its outstanding credit.
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        let _ = self.drain_sidecar_ingress();
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
            self.core.release_empty_current_small_for_trim();
            self.core.evict_all();
        }
        #[cfg(feature = "alloc-segment-directory")]
        self.core.release_directory_for_cold_trim();
    }
}
