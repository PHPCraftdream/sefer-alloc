//! NUMA-node caching, registry-walk/membership accessors, and `(size, align)`
//! classification of [`AllocCore`] (mechanical split of the former flat
//! `alloc_core.rs`; pure code movement, no behavior changed).

use crate::alloc_core::alloc_core::AllocCore;
#[cfg(feature = "numa-aware")]
use crate::alloc_core::numa;
use crate::alloc_core::size_classes::{AllocKind, SizeClasses};

impl AllocCore {
    /// R12-5: number of [`current_node_cached`](Self::current_node_cached)
    /// cache hits between forced re-queries of `numa::current_node()` within
    /// a single registry-slot claim.
    ///
    /// Bounds the staleness introduced by an OS-level thread migration that
    /// happens *mid-claim* (a long-lived, non-pinned thread the scheduler
    /// moves to another NUMA node): without this periodic refresh, R11-5's
    /// cache is invalidated only at the next `claim()`/`recycle()` boundary,
    /// which for a long-lived heap may be unbounded in wall-clock time —
    /// every subsequent allocation would keep steering new segments toward
    /// the stale, now-wrong node indefinitely.
    ///
    /// **Why 128.** The refresh is charged only to
    /// [`current_node_cached`](Self::current_node_cached) call sites, which
    /// are exclusively refill-miss / new-segment-reservation paths
    /// (`find_segment_with_free_impl`, `reserve_small_segment`,
    /// `alloc_large`/`alloc_large_slow`) — never the bump-pointer
    /// alloc/dealloc fast path. Each such call is already paying for a
    /// free-list scan or a fresh OS segment reservation (page-table work,
    /// often a real mmap/VirtualAlloc round-trip), so one extra
    /// `numa::current_node()` call every 128 of them is noise by comparison,
    /// while still bounding staleness to "at most 128 refill-misses behind a
    /// migration" — orders of magnitude tighter than the previous
    /// claim-lifetime bound. 128 sits in the middle of the
    /// 64–256 range this task's review called out, and matches the order of
    /// magnitude of [`crate::alloc_core::segment_directory::DIRECTORY_MISS_FULL_SCAN_PERIOD`]
    /// (64, the sibling "periodic re-validation" cadence already established
    /// for the directory-miss trust window) — reusing a cadence the codebase
    /// already treats as "rare enough to be free, frequent enough to bound
    /// drift" rather than inventing an unrelated third constant. See
    /// `docs/PHASE_NUMA_DESIGN.md` §4.1 "Bounded mid-claim refresh (R12-5)"
    /// for the full rationale and the microbenchmark this choice is checked
    /// against.
    #[cfg(feature = "numa-aware")]
    pub(crate) const NUMA_NODE_REFRESH_PERIOD: u32 = 128;

    /// R11-5 / R12-5: cached NUMA-node accessor — the hot-path replacement
    /// for `numa::current_node()` from every per-miss / per-reservation call
    /// site.
    ///
    /// Returns the cached value if this claim has already queried AND fewer
    /// than [`NUMA_NODE_REFRESH_PERIOD`](Self::NUMA_NODE_REFRESH_PERIOD)
    /// cache hits have been served since the last real query; otherwise
    /// queries `numa::current_node()`, stores the result in
    /// [`cached_numa_node`](Self::cached_numa_node), resets the hit counter,
    /// and returns it. The cache is ALSO invalidated at registry-slot
    /// `claim()` time by
    /// [`invalidate_numa_node_cache`](Self::invalidate_numa_node_cache) so a
    /// recycled slot never hands a stale node to a new owning thread. See
    /// `docs/PHASE_NUMA_DESIGN.md` §4.1 for the full design note, including
    /// the R12-5 bounded mid-claim refresh this periodic re-query adds on
    /// top of R11-5's slot-recycle invalidation.
    ///
    /// Gate: only present under `numa-aware`. Compiled out otherwise (the
    /// call sites are also `#[cfg(feature = "numa-aware")]`, so there is no
    /// caller outside that feature).
    #[cfg(feature = "numa-aware")]
    #[inline]
    pub(crate) fn current_node_cached(&mut self) -> u32 {
        if let Some(n) = self.cached_numa_node {
            if self.numa_node_hits_since_refresh < Self::NUMA_NODE_REFRESH_PERIOD {
                self.numa_node_hits_since_refresh += 1;
                return n;
            }
            // R12-5: hit budget exhausted — force a re-query even though the
            // cache is still `Some`, so a thread that migrated mid-claim is
            // caught within `NUMA_NODE_REFRESH_PERIOD` refill-misses instead
            // of waiting for the next `claim()`.
        }
        let n = numa::current_node();
        self.cached_numa_node = Some(n);
        self.numa_node_hits_since_refresh = 0;
        n
    }

    /// R11-5: invalidate the cached NUMA node. Called by
    /// `HeapRegistry::claim` / `claim_with_config` immediately before
    /// handing a freshly-claimed `*mut HeapCore` to the caller, so the next
    /// `current_node_cached()` call re-queries `numa::current_node()` instead
    /// of returning the previous owner's stale value. Soundness argument
    /// (why a plain write is sufficient — no atomic, no fence beyond what
    /// `claim`'s state-CAS already establishes) lives in
    /// `docs/PHASE_NUMA_DESIGN.md` §4.1.
    ///
    /// R12-5: also resets the refresh-hit counter. Not strictly required for
    /// correctness (the very next call is a miss regardless of the counter's
    /// value, since `cached_numa_node` is `None`), but keeps the two fields
    /// consistent with each other so a future reader of `dbg_cached_numa_node`
    /// alongside a hypothetical debug accessor for the counter never observes
    /// a stale non-zero count paired with a `None` cache.
    ///
    /// Gate: only present under `numa-aware`.
    #[cfg(feature = "numa-aware")]
    #[inline]
    pub(crate) fn invalidate_numa_node_cache(&mut self) {
        self.cached_numa_node = None;
        self.numa_node_hits_since_refresh = 0;
    }

    /// R11-5 test-only: read the cached NUMA-node value without populating
    /// it. Returns `None` if no call to `current_node_cached` has fired on
    /// this `AllocCore` since its most recent invalidation, otherwise the
    /// cached value. The `tests/numa_cache_invalidation.rs` slot-recycle
    /// regression uses this to assert the newly-claimed slot's cached value
    /// reflects the *new* mock node, not the stale one from before
    /// recycling — the exact bug `invalidate_numa_node_cache` exists to
    /// prevent.
    ///
    /// `#[doc(hidden)] pub` per the established test-only-export pattern
    /// (CLAUDE.md "File and module structure" sanctioned exception 1): a
    /// test hook reaching an otherwise-internal field, not stable public
    /// API.
    #[cfg(all(feature = "numa-aware", feature = "internals"))]
    #[doc(hidden)]
    pub fn dbg_cached_numa_node(&self) -> Option<u32> {
        self.cached_numa_node
    }

    /// R11-5 bench/test-only: invoke the cached accessor from external
    /// consumers (the criterion microbenchmark in
    /// `benches/numa_current_node_cache.rs` and the HeapCore test hook
    /// `dbg_populate_numa_cache_for_test`). Delegates to
    /// [`current_node_cached`](Self::current_node_cached) verbatim.
    ///
    /// `#[doc(hidden)] pub` per the established test/bench-only-export
    /// pattern (CLAUDE.md "File and module structure" sanctioned exception
    /// 1). Not stable public API.
    #[cfg(all(feature = "numa-aware", feature = "internals"))]
    #[doc(hidden)]
    pub fn dbg_current_node_cached(&mut self) -> u32 {
        self.current_node_cached()
    }

    /// R11-6 test-only: invalidate the cached NUMA node so the next
    /// `current_node_cached()` call re-queries `numa::current_node()`. Used by
    /// the NUMA directory local-first/foreign-fallback test to create segments
    /// stamped with DIFFERENT node ids within a single `AllocCore` (script the
    /// mock to node A, allocate, invalidate, script to node B, allocate).
    ///
    /// `#[doc(hidden)] pub` per the established test-only-export pattern
    /// (CLAUDE.md "File and module structure" sanctioned exception 1). Not
    /// stable public API.
    #[cfg(all(feature = "numa-aware", feature = "internals"))]
    #[doc(hidden)]
    pub fn dbg_invalidate_numa_node_cache(&mut self) {
        self.invalidate_numa_node_cache();
    }

    /// Iterate over all registered segment bases (read-only). A registry-walk
    /// primitive used by cross-thread-free routing and (historically) the
    /// Phase 12.4 abandonment walk (now removed — task #97 / R4-5).
    ///
    /// `#[doc(hidden)]` (task #136): `AllocCore` itself is re-exported as
    /// stable public API (unlike most of `alloc_core`), but this iterator is
    /// an internal registry-walk primitive, not something an external
    /// caller is expected to use directly — it leaked into the visible
    /// public surface only because `AllocCore` is public. Kept `pub` (not
    /// `pub(crate)`) because `registry::heap_core::HeapCore::segment_bases`
    /// delegates to it across the crate boundary between `alloc_core` and
    /// `registry`.
    #[cfg(any(feature = "alloc-global", feature = "alloc-xthread"))]
    #[doc(hidden)]
    pub fn segment_bases(&self) -> impl Iterator<Item = *mut u8> {
        self.table.bases()
    }

    /// O(1) membership test: is `base` one of THIS substrate's registered,
    /// LIVE (non-NULL) segment bases? Thin delegation to
    /// `SegmentTable::contains_base` (the OPT-B open-addressing hash table).
    ///
    /// Task #135 (Part 2/3): exposes the table's existing O(1) check at the
    /// `AllocCore` level so `HeapCore::realloc` (own-segment ownership test)
    /// and `HeapCore::dealloc_routing` (M2 hardening — see its doc comment)
    /// no longer need to fall back to the O(count) `segment_bases().any(...)`
    /// scan.
    ///
    /// Gated on `alloc-global` only (not also `alloc-xthread`): both call
    /// sites live in `registry::heap_core::HeapCore`, and the entire
    /// `registry` module is itself `#[cfg(feature = "alloc-global")]`-gated
    /// at the crate root (`src/lib.rs`) — `alloc-xthread` alone (without
    /// `alloc-global`) does not compile `HeapCore` at all, so a wider gate
    /// here would leave this method genuinely unused under that combination.
    #[cfg(feature = "alloc-global")]
    #[inline(always)]
    pub(crate) fn contains_base(&mut self, base: *mut u8) -> bool {
        self.table.contains_base(base)
    }

    /// RAD-4b (task #72): the current small segment's base, for callers
    /// outside this module that need to pass it into
    /// [`reclaim_offset`](Self::reclaim_offset) /
    /// [`reclaim_offset_checked`](Self::reclaim_offset_checked) (both of
    /// which take `small_cur` as a plain argument rather than reading `self`,
    /// since they are associate functions, not methods — see their doc
    /// comments). `small_cur` itself is `pub(super)` (module-private); this
    /// thin `pub(crate)` accessor is the sole reason `HeapCore::
    /// drain_heap_overflow` (`src/registry/heap_core.rs`, `registry` module,
    /// outside `alloc_core`) needs to exist.
    ///
    /// Gated on `all(alloc-xthread, alloc-decommit)`, not `alloc-xthread`
    /// alone: the sole call site (`heap_core_xthread.rs::drain_heap_overflow`)
    /// only reads `small_cur` inside its own `#[cfg(feature =
    /// "alloc-decommit")]` block (it feeds `dec_live_and_maybe_decommit`,
    /// which exists only under that feature) — `alloc-xthread` without
    /// `alloc-decommit` (e.g. `hardened medium-classes`) left this method
    /// genuinely unused (R23-5, task #374).
    #[cfg(all(feature = "alloc-xthread", feature = "alloc-decommit"))]
    #[must_use]
    pub(crate) fn small_cur(&self) -> *mut u8 {
        self.small_cur
    }

    // Э4 (task #145) "classify once" wrappers `alloc_small_class` /
    // `dealloc_small_class` were RETIRED in P3 (task #147): their only callers
    // were the P7 alloc-side and dealloc-side bulk bypasses, both removed here.
    // The classify-once win survives where it still has a live caller
    // (`HeapCore::dealloc_own_thread` already resolves the class once); these
    // one-line pass-throughs are trivially re-addable if a future path needs
    // a class-resolved single-block primitive again.

    // -----------------------------------------------------------------------
    // Internals — the safe Cartographer. All raw memory touches go through
    // `Node`; no `Vec`/`Box`/`HashSet`/`std::alloc`.
    // -----------------------------------------------------------------------

    /// Classify a `(size, align)` request as Small or Large.
    #[inline]
    pub(super) fn classify(size: usize, align: usize) -> AllocKind {
        match SizeClasses::class_for(size, align) {
            Some(class_idx) => AllocKind::Small { class_idx },
            None => AllocKind::Large,
        }
    }
}
