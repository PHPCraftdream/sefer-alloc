//! Exclusive cold trim for [`HeapCore`].

use crate::registry::heap_core::HeapCore;

impl HeapCore {
    pub(crate) const BACKGROUND_INGRESS_BUDGET: usize = 64;

    /// Production teardown trim (task #95 / N1): flush every tcache class,
    /// drain the small-segment pool, and evict the entire large cache.
    ///
    /// Called by the TLS `LeaseReleaseGuard::drop` on thread exit, BEFORE the
    /// `HeapLease::drop` CAS flips the slot `LIVE → FREE`. At that
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
        {
            #[cfg_attr(
                not(all(feature = "bench-internals", feature = "fastbin")),
                allow(unused_variables)
            )]
            let reclaimed = self.drain_sidecar_ingress();
            // B4 (b4-provenance): INVALIDATION for the heap-private depot
            // shadow — a remote free was reclaimed here (pending publication
            // consumed); blocks can now leave the shared stream without
            // passing the depot candidate set.
            #[cfg(all(
                feature = "bench-internals",
                feature = "alloc-global",
                feature = "fastbin"
            ))]
            if reclaimed > 0 {
                self.depot_invalidate();
            }
        }
        self.trim_cold_retention();
    }

    /// A worker visit cuts only a bounded ingress prefix, retaining the
    /// independent cold cache/pool policy of the strict trim.
    pub(crate) fn background_maintenance_step(&mut self, budget: usize) -> (usize, usize) {
        let result = self.background_ingress_step(budget);
        self.trim_cold_retention();
        result
    }

    /// Ingress-only worker step for the shared fallback heap (R6-T27 /
    /// R6-CC-7): the fallback heap is shared and can be actively serving
    /// allocations while the worker runs, so the cold-retention trim
    /// (tcache flush, pool drain, evict, munmap) is decided by the caller's
    /// idle policy in `global::maintenance_service` instead of running on
    /// every visit.
    pub(crate) fn background_fallback_step(&mut self, budget: usize) -> (usize, usize) {
        self.background_ingress_step(budget)
    }

    fn background_ingress_step(&mut self, budget: usize) -> (usize, usize) {
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        let result = self
            .core
            .drain_sidecar_ingress_bounded(&mut self.background_cursor, budget);
        #[cfg(not(all(feature = "alloc-global", feature = "alloc-xthread")))]
        let result = (0, 0);
        // B4 (b4-provenance): INVALIDATION for the heap-private depot shadow —
        // same remote-reclaim reason as `trim_for_recycle` above.
        #[cfg(all(
            feature = "bench-internals",
            feature = "alloc-global",
            feature = "fastbin",
            feature = "alloc-xthread"
        ))]
        if result.1 > 0 {
            self.depot_invalidate();
        }
        // Ph4c mutant catcher #12: count every bounded step so a mutant that
        // runs `with_core` (hence this step) twice per maintained slot in
        // `maintenance_pass` becomes observable (measurement-only, Relaxed).
        #[cfg(all(
            feature = "alloc-global",
            feature = "alloc-xthread",
            feature = "bench-internals"
        ))]
        crate::alloc_core::BACKGROUND_INGRESS_STEP_CALLS
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        result
    }

    fn trim_cold_retention(&mut self) {
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
        // B4 (b4-provenance): INVALIDATION for the heap-private depot shadow —
        // cold trim = teardown flush + small-segment pool drain + large-cache
        // evict: segment reclaim/teardown, no depot residency survives.
        #[cfg(all(
            feature = "bench-internals",
            feature = "alloc-global",
            feature = "fastbin"
        ))]
        self.depot_invalidate();
    }
}
