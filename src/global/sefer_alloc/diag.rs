#[cfg(feature = "alloc-decommit")]
use crate::global::tls_heap::current_for_trim;
#[cfg(all(feature = "bench-internals", feature = "internals"))]
use crate::global::tls_heap::CurrentHeap;
use crate::global::AllocStats;

use super::SeferAlloc;

impl SeferAlloc {
    /// A cheap, process-wide diagnostic snapshot of this allocator's internal
    /// counters — cache-hit rates, cross-thread reclaim/overflow counts, and
    /// segment/heap totals. See [`AllocStats`] for what each field means and
    /// which feature flags it depends on.
    ///
    /// **Cost.** Without `alloc-stats` (the default — it is not part of
    /// `production`), `stats()` is O(1): a fixed handful of relaxed atomic
    /// loads, no locks, no allocation, and no segment or heap walk. The two
    /// hit counters (`tcache_hits` / `large_cache_hits`) are compile-time zero
    /// here — their aggregating slot walks are compiled out entirely under
    /// `not(alloc-stats)`, since the per-hit increments they would sum are
    /// themselves absent. Safe to call on a metrics-scrape hot path.
    ///
    /// WITH `alloc-stats`, the two hit counters are aggregated by a walk over
    /// the initialized registry slots (each a Relaxed atomic load of that
    /// slot's counter — still no locks and no segment walk; only registry slot
    /// metadata is touched, never segment payloads). `stats()` is then
    /// O(initialized-slot-count) for those two fields; every other field
    /// remains a single relaxed load. Still safe to poll periodically — just
    /// no longer O(1) with `alloc-stats` on.
    ///
    /// The counters are **process-wide**, not per-`SeferAlloc`-instance: if a
    /// process installs more than one `SeferAlloc` (unusual, but not
    /// forbidden), every instance's `stats()` reads the same process-global
    /// totals.
    ///
    /// ```text
    /// use sefer_alloc::SeferAlloc;
    ///
    /// #[global_allocator]
    /// static A: SeferAlloc = SeferAlloc::new();
    ///
    /// fn report() {
    ///     let stats = A.stats();
    ///     println!(
    ///         "segments live ~= {}, tcache hits = {}, ring overflows = {}",
    ///         stats.segments_reserved_total.saturating_sub(stats.segments_released_total),
    ///         stats.tcache_hits,
    ///         stats.ring_overflows,
    ///     );
    /// }
    /// ```
    ///
    /// Runnable form: `tests/sefer_alloc_examples.rs`.
    #[must_use]
    pub fn stats(&self) -> AllocStats {
        AllocStats {
            #[cfg(feature = "alloc-decommit")]
            large_cache_hits: crate::registry::large_cache_hits_total(),
            #[cfg(not(feature = "alloc-decommit"))]
            large_cache_hits: 0,

            #[cfg(feature = "alloc-decommit")]
            decommit_calls: crate::alloc_core::AllocCore::dbg_decommit_count(),
            #[cfg(not(feature = "alloc-decommit"))]
            decommit_calls: 0,

            #[cfg(feature = "alloc-xthread")]
            large_xthread_reclaimed: crate::registry::DBG_LARGE_XTHREAD_RECLAIMED
                .load(core::sync::atomic::Ordering::Relaxed),
            #[cfg(not(feature = "alloc-xthread"))]
            large_xthread_reclaimed: 0,

            #[cfg(feature = "fastbin")]
            tcache_hits: crate::registry::tcache_hits_total(),
            #[cfg(not(feature = "fastbin"))]
            tcache_hits: 0,

            #[cfg(feature = "alloc-xthread")]
            ring_overflows: crate::alloc_core::remote_free_ring::DBG_RING_OVERFLOW
                .load(core::sync::atomic::Ordering::Relaxed),
            #[cfg(not(feature = "alloc-xthread"))]
            ring_overflows: 0,

            segments_reserved_total: crate::alloc_core::AllocCore::dbg_segments_reserved_total(),
            segments_released_total: crate::alloc_core::AllocCore::dbg_segments_released_total(),

            heaps_claimed_high_water: crate::registry::heaps_claimed_high_water() as u64,

            // Review finding 2.3: the foreign-or-unroutable-free drop counter.
            // Always-present static (backs the `alloc-global`-without-
            // `alloc-xthread` cross-thread-free leak footgun observability);
            // reads 0 unless the per-event increment was compiled in
            // (`alloc-stats`). `stats()` is `alloc-global`-only, so the static
            // is always defined here.
            foreign_or_unroutable_frees:
                crate::alloc_core::AllocCore::dbg_foreign_or_unroutable_frees(),

            #[cfg(feature = "alloc-decommit")]
            config_conflicts: crate::registry::config_conflicts_total(),
            #[cfg(not(feature = "alloc-decommit"))]
            config_conflicts: 0,
        }
    }

    /// Trim the CALLING thread's own heap back to a comparable empty-ish
    /// baseline: flush every tcache class's magazine, drain the
    /// small-segment hysteresis pool, and evict the entire large cache.
    /// Call this when the calling thread KNOWS a burst/phase has ended
    /// (e.g. after finishing a batch of request handling, before an
    /// expected idle period) and wants its retained memory released to the
    /// OS now, rather than waiting for the next allocation-driven decay
    /// tick or thread-exit.
    ///
    /// This does NOT tear down the thread's TLS binding or recycle its
    /// registry slot — the thread keeps using the SAME heap afterward. The
    /// next allocation after this call takes the normal cold
    /// reserve-a-fresh-segment / re-populate-the-large-cache path instead
    /// of reusing whatever this call just released.
    ///
    /// A true no-op — claims NO registry slot and touches no allocator
    /// state — on a thread that has never allocated, on a thread whose TLS
    /// binding is torn down (`TORN`, mid-teardown), and on a thread whose
    /// TLS storage is already destroyed. **Fixed post-R31-10 (task #492):**
    /// an earlier version of this method resolved via the alloc-side
    /// [`current_heap`](SeferAlloc::current_heap), which — for a freshly-bound,
    /// never-allocated thread — would itself claim a fresh registry slot
    /// and bind an (empty) per-thread heap (`global::tls_heap::finish_bind`)
    /// purely as a side effect of asking "is there anything to trim?",
    /// exactly as an ordinary first allocation would. That was harmless
    /// (the slot's `AbandonGuard` was armed, so it still recycled correctly
    /// at thread exit) but wasteful: a monitoring/housekeeping routine that
    /// calls `trim_current_thread()` speculatively across many threads —
    /// some of which never allocate — would claim a registry slot for every
    /// one of them regardless. This method now resolves via
    /// [`tls_heap::current_for_trim`](super::super::tls_heap::current_for_trim), a
    /// **passive** resolver that reports "no live heap yet" instead of
    /// binding one, so a thread with nothing to trim claims nothing.
    ///
    /// Cost: O(live tcache classes + pooled segments + cached large spans)
    /// for THIS thread only — no cross-thread coordination, no lock
    /// contention with any other heap. On a thread with no bound heap the
    /// cost is a single passive TLS read (no bind, no OS call). Safe to
    /// call from a hot request handler's cold "end of batch" branch; NOT
    /// intended to be called on every allocation (it defeats the
    /// warm-cache/warm-pool amortization this project's whole
    /// small-pool/large-cache design exists to provide — see the design
    /// doc, `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` §4.3, for the
    /// mis-use hazard this creates).
    ///
    /// Design: `docs/design/R30_7_TRIM_SCAVENGE_API_DESIGN.md` (R30-7,
    /// task #456). Measured value proposition (benefit side):
    /// `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE.md`. Measured cost
    /// side (trim latency + next-burst cold-start cost): the same report's
    /// later "Cost side" section (task #492).
    #[cfg(feature = "alloc-decommit")]
    pub fn trim_current_thread(&self) {
        if let Some(heap) = current_for_trim() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (same single-writer
            // invariant `alloc`/`dealloc` above rely on) — `current_for_trim`
            // only returns `Some` for an already-bound own-thread slot.
            #[allow(unsafe_code)]
            unsafe {
                (*heap).trim_for_recycle()
            };
        }
    }

    /// TEST/BENCH-ONLY alias for
    /// [`trim_current_thread`](Self::trim_current_thread).
    ///
    /// **Why this exists.** `benches/global_alloc.rs`'s `criterion_main!`
    /// invokes all registered `benchmark_group()` functions in the SAME
    /// process, on the SAME thread. Every `SeferAlloc::new()` call inside
    /// each group resolves to the SAME underlying per-thread `HeapCore` (TLS
    /// caches the first-claimed heap for the thread's lifetime — see
    /// `global::tls_heap`'s fast path), so segment high-water marks, tcache
    /// occupancy, the small-segment pool, and the large cache from an
    /// EARLIER group are still resident when a LATER group starts. Calling
    /// this hook between `benchmark_group()` calls resets that shared state
    /// to a comparable baseline without paying for a fresh subprocess per
    /// group (cargo bench harness re-init, criterion setup) on every one of
    /// the 7 groups this bench registers.
    ///
    /// `#[doc(hidden)]` — not part of the public API; the established
    /// test-only export pattern documented in `src/lib.rs`'s `#[doc(hidden)]`
    /// notes.
    ///
    /// **Deliberately UNCONDITIONAL, not a bare `alloc-decommit`-gated
    /// delegation to [`trim_current_thread`](Self::trim_current_thread)**
    /// (fixed post-R31-10/task #474 — an independent review, R32, caught
    /// that an earlier version of this hook, gated purely on
    /// `alloc-decommit`, silently stopped flushing tcache magazines under
    /// `alloc-global + fastbin` builds that don't also enable
    /// `alloc-decommit`: `HeapCore::trim_for_recycle`'s own body flushes
    /// tcache under `fastbin` independently of `alloc-decommit`, so gating
    /// the WHOLE call on `alloc-decommit` silently dropped real work in that
    /// configuration — exactly the class of regression
    /// `benches/global_alloc.rs`'s cross-group isolation depends on this
    /// hook for). This body calls `trim_for_recycle` directly, in every
    /// feature configuration `HeapCore` compiles under, so it always
    /// performs whatever subset of the trim work that build actually
    /// supports (see [`trim_current_thread`](Self::trim_current_thread)'s
    /// own body for the identical logic the public method uses when
    /// `alloc-decommit` is enabled).
    ///
    /// I5 (task #583, `docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`
    /// finding F5): gated `bench-internals` + `internals` — this hook has no
    /// production caller (CLAUDE.md's benchmark-hook rule 2), and was found
    /// reachable from a plain `--features production` downstream build with
    /// neither gate present, the same class of gap Sol-F1/H2 close for
    /// `AllocCore::dbg_*`. This gate is orthogonal to the "deliberately
    /// UNCONDITIONAL" note above: that note is about NOT gating the BODY on a
    /// mechanism feature like `alloc-decommit` (which previously caused a
    /// real regression by silently skipping trim work); it says nothing
    /// about ACCESS to the method itself, which this `#[cfg]` controls.
    #[doc(hidden)]
    #[cfg(all(feature = "bench-internals", feature = "internals"))]
    pub fn dbg_trim_current_thread(&self) {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (same single-writer
            // invariant `alloc`/`dealloc` above rely on) — `current_heap()`
            // just resolved it for the calling thread.
            #[allow(unsafe_code)]
            unsafe {
                (*heap).trim_for_recycle()
            };
        }
    }

    /// F10 (task #502) TEST/BENCH-ONLY: force-drain every `RemoteFreeRing`
    /// owned by the CALLING thread's own heap into its `BinTable`, via
    /// [`HeapCore::dbg_drain_all_rings`](crate::registry::HeapCore::dbg_drain_all_rings).
    ///
    /// **Why this exists.** `HeapCore`'s normal small-alloc drain (the
    /// "lazily drain this segment's remote-free ring before inspecting its
    /// BinTable" step documented in `AllocCore::alloc_small`) fires only on a
    /// free-list MISS on the current bump segment — it is not reachable on
    /// demand from outside the allocator, and a harness that needs to force a
    /// ring drain on a KNOWN cadence (e.g. `examples/r32_11_remote_ring_shadow_head_gate.rs`'s
    /// favorable-regime owner thread, which must keep `RemoteFreeRing::push`'s
    /// target ring far from capacity so the shadow-head fast path — F10,
    /// `src/alloc_core/segment/remote_free_ring/` — is the mechanism actually under
    /// measurement, not an accident of allocation-pattern side effects) needs
    /// a direct hook, not a hoped-for side effect. Mirrors
    /// [`dbg_trim_current_thread`](Self::dbg_trim_current_thread)'s exact
    /// pattern: resolve the calling thread's ALREADY-BOUND heap via
    /// `current_heap()` (never claims a new slot — the same heap
    /// `alloc`/`dealloc` on this thread already use) and delegate.
    ///
    /// `#[doc(hidden)]` — not part of the public API; the established
    /// test-only export pattern documented in `src/lib.rs`'s `#[doc(hidden)]`
    /// notes. `bench-internals`-gated per CLAUDE.md's benchmark-hook rule
    /// (no production caller) — additionally gated on `alloc-xthread` (rings
    /// do not exist otherwise; matches `HeapCore::dbg_drain_all_rings`'s own
    /// gate).
    ///
    /// Sol-F1 (task #563): additionally gated `internals` — the delegated
    /// `HeapCore::dbg_drain_all_rings` (`registry::heap_core::diag::queries`) is now
    /// `internals`-gated too (a hard transitive compile dependency, since it
    /// in turn delegates to `AllocCore::dbg_drain_all_rings[_checked]`,
    /// moved behind `internals` in `alloc_core_small_reclaim.rs`).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-xthread",
        feature = "bench-internals",
        feature = "internals"
    ))]
    pub fn dbg_drain_current_thread_rings(&self) {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (same single-writer
            // invariant `alloc`/`dealloc` above rely on) — `current_heap()`
            // just resolved it for the calling thread; `dbg_drain_all_rings`
            // requires `&mut HeapCore`, sound here because we are the sole
            // writer (single-consumer-per-heap, matching every other
            // `dbg_*` mutator in this file's identical `unsafe` shape).
            #[allow(unsafe_code)]
            unsafe {
                (*heap).dbg_drain_all_rings()
            };
        }
    }

    /// R31-4 (task #487) MEASUREMENT-ONLY: whether the CALLING thread's own
    /// heap's large-cache extension sidecar has materialised. A gate driving
    /// the real `#[global_allocator]` (e.g.
    /// `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`)
    /// only ever has a `SeferAlloc`/`HeapCore` in hand, never a bare
    /// `AllocCore` — this is the same in-run materialisation-oracle evidence
    /// [`AllocCore::dbg_large_cache_extension_materialised`](crate::AllocCore::dbg_large_cache_extension_materialised)
    /// gives a bare-`AllocCore` caller, resolved for the calling thread's own
    /// bound heap via [`current_heap`](SeferAlloc::current_heap) (never claims a
    /// NEW slot — reads back whatever heap this thread already resolved to,
    /// binding one via the normal first-touch path if this is the very first
    /// call on this thread). Returns `false` on the fallback heap (TLS torn
    /// down, or the registry is exhausted) — there is no per-thread slot to
    /// report on. `#[doc(hidden)]` — not part of the public API; the
    /// established test-only export pattern documented in `src/lib.rs`'s
    /// `#[doc(hidden)]` notes. `bench-internals`-gated (no production caller
    /// → CLAUDE.md's benchmark-hook rule 2), additionally gated on
    /// `large-cache-extended` (matching the delegated `HeapCore`/`AllocCore`
    /// methods' own gate — the sidecar does not exist otherwise).
    ///
    /// H2 (task #572): additionally gated `internals` — a hard transitive
    /// compile dependency, since this delegates to
    /// `HeapCore::dbg_large_cache_extension_materialised`, moved behind
    /// `internals` (`alloc_core/large/alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "large-cache-extended",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_current_large_cache_extension_materialised(&self) -> bool {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (same single-writer
            // invariant `alloc`/`dealloc` rely on) — `current_heap()` just
            // resolved it for the calling thread. `dbg_large_cache_extension_materialised`
            // is a read-only `&self` accessor.
            #[allow(unsafe_code)]
            unsafe {
                (*heap).dbg_large_cache_extension_materialised()
            }
        } else {
            false
        }
    }

    /// R31-4 (task #487) MEASUREMENT-ONLY: the CALLING thread's own heap's
    /// current combined base+extension addressable large-cache slot count (8
    /// if the extension has not materialised, 40 once it has). Same
    /// current-thread-resolution shape as
    /// [`dbg_current_large_cache_extension_materialised`](Self::dbg_current_large_cache_extension_materialised)
    /// immediately above — see that method's doc for the full rationale.
    /// Returns `0` on the fallback heap (TLS torn down, or the registry is
    /// exhausted) — there is no per-thread slot to report on; `0` is
    /// distinguishable from every real resolved value (always `8` or `40`).
    /// `#[doc(hidden)]` — not part of the public API. `bench-internals`-gated
    /// (no production caller → CLAUDE.md's benchmark-hook rule 2). Unlike
    /// [`dbg_current_large_cache_extension_materialised`](Self::dbg_current_large_cache_extension_materialised),
    /// NOT additionally gated on `large-cache-extended` — matching
    /// [`HeapCore::dbg_large_cache_total_slots`](crate::registry::HeapCore::dbg_large_cache_total_slots)'s
    /// own gate exactly (it reads `8` when the extension feature/sidecar is
    /// absent).
    ///
    /// H2 (task #572): additionally gated `internals` — a hard transitive
    /// compile dependency, since this delegates to
    /// `HeapCore::dbg_large_cache_total_slots`, moved behind `internals`
    /// (`alloc_core/large/alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_current_large_cache_total_slots(&self) -> usize {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread — same justification as
            // `dbg_current_large_cache_extension_materialised` above.
            #[allow(unsafe_code)]
            unsafe {
                (*heap).dbg_large_cache_total_slots()
            }
        } else {
            0
        }
    }

    /// R31-4 (task #487) MEASUREMENT-ONLY: the CALLING thread's own heap's
    /// resolved large-cache byte budget (`None` = unbounded, base cache;
    /// `Some(n)` = the finite ceiling this heap resolved at first
    /// materialisation — e.g. 256 MiB, `large-cache-extended`'s R17-9
    /// default). Same current-thread-resolution shape as
    /// [`dbg_current_large_cache_extension_materialised`](Self::dbg_current_large_cache_extension_materialised)
    /// above — see that method's doc for the full rationale. Returns `None`
    /// on the fallback heap (TLS torn down, or the registry is exhausted) —
    /// indistinguishable from a real unbounded resolution by design (this
    /// hook is meant for a gate that has already established, via its own
    /// `CurrentHeap::Own`-only workload shape, that it is running on a real
    /// per-thread slot, not the fallback). `#[doc(hidden)]` — not part of the
    /// public API. `bench-internals`-gated (no production caller →
    /// CLAUDE.md's benchmark-hook rule 2).
    ///
    /// H2 (task #572): additionally gated `internals` — a hard transitive
    /// compile dependency, since this delegates to
    /// `HeapCore::dbg_large_cache_budget`, moved behind `internals`
    /// (`alloc_core/large/alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_current_large_cache_budget(&self) -> Option<usize> {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread — same justification as
            // `dbg_current_large_cache_extension_materialised` above.
            #[allow(unsafe_code)]
            unsafe {
                (*heap).dbg_large_cache_budget()
            }
        } else {
            None
        }
    }

    /// R31-8 (task #488) MEASUREMENT-ONLY: the CALLING thread's own heap's
    /// current large-cache running-used-bytes sum (thin delegation to
    /// [`HeapCore::dbg_large_cache_used`](crate::registry::HeapCore::dbg_large_cache_used),
    /// which itself delegates to
    /// [`AllocCore::dbg_large_cache_used`](crate::AllocCore::dbg_large_cache_used)).
    /// Same current-thread-resolution shape as
    /// [`dbg_current_large_cache_extension_materialised`](Self::dbg_current_large_cache_extension_materialised)
    /// above — see that method's doc for the full rationale. Returns `0` on
    /// the fallback heap (TLS torn down, or the registry is exhausted) —
    /// indistinguishable from a real empty cache, same caveat as
    /// [`dbg_current_large_cache_total_slots`](Self::dbg_current_large_cache_total_slots)'s
    /// `0` sentinel. Added for the R31-8 narrow-gate re-measurement's
    /// matched-state proof (CLAUDE.md's per-arm state-matching evidence
    /// rule): both arms must show the same resident large-cache used-bytes
    /// total immediately before the timed region is trusted. `#[doc(hidden)]`
    /// — not part of the public API. `bench-internals`-gated (no production
    /// caller → CLAUDE.md's benchmark-hook rule 2).
    ///
    /// H2 (task #572): additionally gated `internals` — a hard transitive
    /// compile dependency, since this delegates to
    /// `HeapCore::dbg_large_cache_used`, moved behind `internals`
    /// (`alloc_core/large/alloc_core_large_cache.rs`'s module doc).
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "internals"
    ))]
    #[must_use]
    pub fn dbg_current_large_cache_used_bytes(&self) -> usize {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread — same justification as
            // `dbg_current_large_cache_extension_materialised` above.
            #[allow(unsafe_code)]
            unsafe {
                (*heap).dbg_large_cache_used()
            }
        } else {
            0
        }
    }
}
