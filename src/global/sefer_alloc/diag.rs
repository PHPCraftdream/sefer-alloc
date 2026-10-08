use crate::global::tls_heap::current_for_trim;
#[cfg(all(feature = "bench-internals", feature = "internals"))]
use crate::global::tls_heap::CurrentHeap;
use crate::global::AllocStats;

use super::SeferAlloc;

#[cfg(all(feature = "internals", feature = "bench-internals"))]
impl SeferAlloc {
    /// Snapshot this owner's live allocation's original OS release token.
    ///
    /// # Safety
    /// `ptr` is the exact start of a live allocation in this thread's current
    /// heap. No conflicting owner borrow or segment retirement may occur.
    #[doc(hidden)]
    #[allow(unsafe_code)]
    pub unsafe fn dbg_current_reservation_for_test(
        &self,
        ptr: *mut u8,
    ) -> Option<(usize, usize, usize)> {
        use crate::global::tls_heap::{current_for_dealloc, CurrentHeapForDealloc};

        let CurrentHeapForDealloc::Own(heap) = current_for_dealloc() else {
            return None;
        };
        // SAFETY: caller retains exclusive owner authority and the live
        // allocation; only its canonical table root accesses reservation bytes.
        let core = unsafe { &(*heap).core };
        let root = core.canonical_root_for(ptr)?;
        let header = crate::alloc_core::segment_header::SegmentHeader::read_at(root);
        Some((
            header.reservation.addr(),
            header.reservation_len,
            root.addr(),
        ))
    }

    /// M-C oracle (Ph4c, receipt open question M-C): process-wide
    /// `(full_ingress_drain_calls, ingress_records_consumed)` counters of the
    /// strict trim-path drain ([`AllocCore::drain_sidecar_ingress`] — the
    /// `trim_for_recycle` / re-claim pass; the bounded worker step and the
    /// Large hot/rescue scans are NOT counted). `trim_for_recycle` performs
    /// EXACTLY ONE full drain pass per trim, so:
    /// - `drain_calls` delta = 1 across a single trim / re-claim — a mutant
    ///   consuming ingress twice in one trim yields 2, a suppressed trim
    ///   yields 0, both red under
    ///   `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs`;
    /// - `records_consumed` delta = the number of pending publications —
    ///   distinguishing "exactly one trim consumed exactly the pending set"
    ///   from "no trim ran" (delta 0, the idempotence blindness the receipt
    ///   documented).
    /// MEASUREMENT-ONLY: both counters are pure relaxed-atomic numeric
    /// observers under `bench-internals`; no production logic reads them.
    /// `#[doc(hidden)]` — not part of the public API (the established
    /// test-only export pattern documented in `src/lib.rs`).
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    #[must_use]
    pub fn dbg_sidecar_ingress_stats(&self) -> (u64, u64) {
        use crate::alloc_core::{SIDECAR_INGRESS_DRAIN_CALLS, SIDECAR_INGRESS_RECORDS_CONSUMED};
        use core::sync::atomic::Ordering;
        (
            SIDECAR_INGRESS_DRAIN_CALLS.load(Ordering::Relaxed),
            SIDECAR_INGRESS_RECORDS_CONSUMED.load(Ordering::Relaxed),
        )
    }

    /// Ph4c mutant catcher #12: process-wide count of BOUNDED background
    /// maintenance ingress steps (`HeapCore::background_maintenance_step` —
    /// exactly one per maintained registry slot in
    /// [`crate::registry::HeapRegistry::maintenance_pass`]).
    ///
    /// Why a separate counter: the bounded step is cursor-idempotent, so
    /// `dbg_sidecar_ingress_stats` (which counts ONLY the full trim-path
    /// drain) cannot distinguish one ingress step from two on the maintenance
    /// path. This counter can: a mutant that calls
    /// `MaintenanceLease::with_core` twice per maintained slot doubles the
    /// delta and goes red under
    /// `tests/r11_ph4c_ingress_consume_exactly_once_oracle.rs`.
    /// MEASUREMENT-ONLY (pure relaxed atomic, `bench-internals`-gated);
    /// no production logic reads it. `#[doc(hidden)]` — not public API.
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        feature = "bench-internals"
    ))]
    #[must_use]
    pub fn dbg_background_ingress_step_calls(&self) -> u64 {
        crate::alloc_core::BACKGROUND_INGRESS_STEP_CALLS.load(core::sync::atomic::Ordering::Relaxed)
    }
}

impl SeferAlloc {
    /// A cheap, process-wide diagnostic snapshot of this allocator's internal
    /// counters — cache hits, Large cross-thread retirements, dropped frees,
    /// and segment/heap totals. See [`AllocStats`] for what each field means and
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
    /// all MINTED registry slots (`0..count`, the high-water mark of slots
    /// ever claimed) — each slot costs one Acquire load of
    /// `HeapSlot::initialised`, skipped immediately if `false`, else one
    /// further Relaxed atomic load of that slot's counter; still no locks and
    /// no segment walk, only registry slot metadata is touched, never segment
    /// payloads. When both counters are compiled in (`alloc-decommit` +
    /// `fastbin`, both present under `production`), this is a SINGLE fused
    /// walk (#1986) — one pass over the slot array summing both counters,
    /// not two independent passes. `stats()` is then
    /// O(minted-slot-count) for those two fields; every other field remains
    /// a single relaxed load. Still safe to poll periodically — just no
    /// longer O(1) with `alloc-stats` on.
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
    /// static GLOBAL: SeferAlloc = SeferAlloc::new();
    ///
    /// fn report() {
    ///     let stats = GLOBAL.stats();
    ///     println!(
    ///         "segments live ~= {}, tcache hits = {}, dropped/unroutable frees = {}",
    ///         stats
    ///             .segments_reserved_total
    ///             .saturating_sub(stats.segments_released_total),
    ///         stats.tcache_hits,
    ///         stats.foreign_or_unroutable_frees,
    ///     );
    /// }
    /// ```
    ///
    /// Runnable form: `tests/sefer_alloc_examples.rs`.
    #[must_use]
    pub fn stats(&self) -> AllocStats {
        // #1986: fuse the two hit-counter registry walks into one pass when
        // both are compiled in (`alloc-decommit` + `fastbin`, both present
        // under `production`) — halves the per-slot walk `stats()` does
        // under `alloc-stats`. Each counter's own feature gate is preserved
        // exactly (see the four mutually-exclusive arms below); only the
        // WALK is fused when both apply, never the feature surface.
        #[cfg(all(feature = "alloc-decommit", feature = "fastbin"))]
        let (tcache_hits, large_cache_hits) = crate::registry::tcache_and_large_cache_hits_total();
        #[cfg(all(feature = "alloc-decommit", not(feature = "fastbin")))]
        let (tcache_hits, large_cache_hits) = (0u64, crate::registry::large_cache_hits_total());
        #[cfg(all(not(feature = "alloc-decommit"), feature = "fastbin"))]
        let (tcache_hits, large_cache_hits) = (crate::registry::tcache_hits_total(), 0u64);
        #[cfg(all(not(feature = "alloc-decommit"), not(feature = "fastbin")))]
        let (tcache_hits, large_cache_hits) = (0u64, 0u64);

        AllocStats {
            large_cache_hits,

            #[cfg(feature = "alloc-decommit")]
            decommit_calls: crate::alloc_core::AllocCore::dbg_decommit_count(),
            #[cfg(not(feature = "alloc-decommit"))]
            decommit_calls: 0,

            large_xthread_reclaimed: crate::alloc_core::LARGE_REMOTE_RETIREMENTS
                .load(core::sync::atomic::Ordering::Relaxed),

            tcache_hits,

            segments_reserved_total: crate::alloc_core::AllocCore::dbg_segments_reserved_total(),
            segments_released_total: crate::alloc_core::AllocCore::dbg_segments_released_total(),

            heaps_claimed_high_water: crate::registry::heaps_claimed_high_water() as u64,

            // Dropped-free counter (see `FOREIGN_OR_UNROUTABLE_FREES`).
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
    /// `current_heap`, which — for a freshly-bound,
    /// never-allocated thread — would itself claim a fresh registry slot
    /// and bind an (empty) per-thread heap (`global::tls_heap::finish_bind`)
    /// purely as a side effect of asking "is there anything to trim?",
    /// exactly as an ordinary first allocation would. That was harmless
    /// (the slot's `AbandonGuard` was armed, so it still recycled correctly
    /// at thread exit) but wasteful: a monitoring/housekeeping routine that
    /// calls `trim_current_thread()` speculatively across many threads —
    /// some of which never allocate — would claim a registry slot for every
    /// one of them regardless. This method now resolves via
    /// `tls_heap::current_for_trim`, a
    /// **passive** resolver that reports "no live heap yet" instead of
    /// binding one, so a thread with nothing to trim claims nothing.
    ///
    /// Cost: O(table high-water + issued Small bitmap words + detached records +
    /// live tcache classes + pooled segments + cached large spans) for THIS
    /// thread only. The sidecar sweep never waits for producer quiescence.
    /// On a thread with no bound heap, cost is a single passive TLS read (no bind,
    /// no OS call). Safe to
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
    /// # Terminal route publications
    ///
    /// Available with `alloc-global`, including without `fastbin` or
    /// `alloc-decommit`: terminal sidecar publications completed before entry
    /// are logically retired before return. Each word is cut once; concurrent
    /// post-cut publications can wait for the next call. Reservation release
    /// still follows the build's cache/decommit policy.
    ///
    /// Foreign frees publish through route-directory pins into terminal
    /// sidecars; the owner performs a bounded cut per word and reclaims the
    /// detached records without waiting for producers. This is not autonomous
    /// ownerless reclamation. Trim still skips fallback.
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
