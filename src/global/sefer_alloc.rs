//! Phase 12.3 -- the alloc face: [`SeferAlloc`], an `unsafe impl GlobalAlloc`
//! over the global heap registry (Phase 12.2) via raw-pointer TLS (Phase 12.3).
//!
//! This is the **drop-in face** -- the campaign's victory deliverable. One
//! substrate (the segment-backed, self-hosted, registry-resident heap
//! allocator), two faces: the `Handle` face (typed, generational,
//! relocatable) and this `alloc` face (raw `*mut u8`, drop-in
//! `#[global_allocator]` replacement).
//!
//! ## Phase 12.3 rewiring
//!
//! Previously (Phase 11) this face routed through a now-removed
//! `RefCell<Option<Heap>>` TLS binding. That binding ABORTED under
//! libtest's reentrant harness: `RefCell::try_borrow_mut` returns `Err` on
//! a reentrant borrow → the alloc face returned null → std aborted.
//!
//! Phase 12.3 replaces that with [`tls_heap::current`](super::tls_heap::current):
//! a raw `Cell<*mut HeapCore>` TLS cache (no borrow state to fail) over the
//! global [`HeapRegistry`](crate::registry::HeapRegistry). The heap lives in
//! a registry slot (not in TLS); thread exit recycles the slot (whole-slot
//! reuse — the `HeapCore` stays whole, not dropped). The alloc face is therefore **reentrancy-safe**
//! (M5) and **never-null** (M10): [`current`] returns a non-null pointer in
//! every case (cached slot, fresh claim, or the process-global fallback
//! heap).
//!
//! ## M5 (reentrancy-freedom) -- how it is upheld
//!
//! The whole point (§4 M5, §8 of `ALLOC_PLAN.md`): when WE are the global
//! allocator, ANY use of `Vec`/`Box`/`HashSet`/`std::alloc`/`format!` on the
//! alloc path would recurse infinitely. This module contains NONE of those.
//! `current()` is a plain thread-local load + null check. `bind_slow` claims
//! a registry slot (which bootstraps via the OS aperture, never `std::alloc`);
//! the bind path performs NO `std::alloc` at all: since task H1 (#13), the
//! cross-thread free head (TFS) is a slot-resident `'static AtomicPtr<u8>`
//! (or `FALLBACK_TFS` for the fallback heap), planted by
//! `HeapCore::bind_thread_free` at claim time — before `bind_slow` ever sees
//! the heap pointer, so no per-bind allocation is needed at all (a `Box`
//! there would have recursed into `SeferAlloc::alloc` → `bind_slow` → …; see
//! `registry::heap_core`). The `HeapCore` alloc/dealloc paths are pure safe integer
//! arithmetic + the `node` seam (intrusive pointer r/w). No `std` collection
//! is reachable from here.
//!
//! ## No-panic -- how it is upheld
//!
//! A panic in `#[global_allocator]` aborts the process (§8 of `ALLOC_PLAN.md`).
//! Every entry point here returns null on failure and NEVER panics:
//! - `alloc`: `current()` → `&mut HeapCore` → `HeapCore::alloc` (returns
//!   null on OOM). If `current()` itself yields the fallback (TLS teardown),
//!   the fallback's `with_heap` returns `None` only on true OOM → null.
//! - `dealloc`: `current()` → `HeapCore::dealloc`. If TLS is torn down, the
//!   fallback's `with_heap` deallocs under the spinlock; a torn-down-TLS
//!   dealloc still routes correctly (the segment's owner routes via the
//!   header). On any failure this is a no-op (the block is leaked safely).
//! - `realloc`: an in-place fast path for same-class / compatible growth (C2:
//!   own-thread reallocs delegate to `AllocCore::realloc`, which short-circuits
//!   when the block can stay put), falling back to `alloc` + copy + `dealloc`
//!   otherwise — all null-returning.
//! - `alloc_zeroed`: `alloc` + zero-fill.
//!
//! [`current`]: super::tls_heap::current

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see `src/lib.rs`);
// this is the documented alloc-face seam. `allow` lifts the crate-level
// deny for this file only -- `unsafe` anywhere else in the crate is a hard
// error. The ONLY `unsafe` here is the `unsafe impl GlobalAlloc` (the trait
// is `unsafe`) plus the `// SAFETY:`-annotated pointer handoff to HeapCore.
#![allow(unsafe_code)]

use core::alloc::{GlobalAlloc, Layout};

use super::fallback;
#[cfg(not(feature = "alloc-decommit"))]
use super::tls_heap::current_for_alloc;
#[cfg(feature = "alloc-decommit")]
use super::tls_heap::current_for_alloc_with_config;
#[cfg(feature = "alloc-decommit")]
use super::tls_heap::current_for_trim;
use super::tls_heap::CurrentHeap;
#[cfg(feature = "alloc-xthread")]
use super::tls_heap::{current_for_dealloc, CurrentHeapForDealloc};
use super::AllocStats;

/// The drop-in `GlobalAlloc` face over the `sefer-alloc` segment substrate,
/// routed through the global heap registry (Phase 12.2) via raw-pointer TLS
/// (Phase 12.3).
///
/// Install it as your process's global allocator (simple form — uses all
/// large-cache defaults; requires the `alloc-global` feature; runnable form
/// in `tests/sefer_alloc_examples.rs`):
///
/// ```text
/// use sefer_alloc::SeferAlloc;
///
/// #[global_allocator]
/// static A: SeferAlloc = SeferAlloc::new();
/// ```
///
/// Or configure the large-cache knobs at compile time (requires the
/// `alloc-decommit` feature; runnable end-to-end form in
/// `tests/sefer_alloc_with_config.rs`):
///
/// ```text
/// use sefer_alloc::{SeferAlloc, LargeCacheConfig, LargeCacheMode};
///
/// const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
///     .budget_bytes(512 * 1024 * 1024)
///     .headroom_bytes(64 * 1024 * 1024)
///     .decay_interval_ms(200)
///     .decay_rate_percent(25)
///     .mode(LargeCacheMode::Lazy);
///
/// #[global_allocator]
/// static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);
/// ```
///
/// Each thread gets its own heap slot in the global registry (lazily claimed
/// on first allocation via the raw-pointer TLS binding -- no `RefCell`, no
/// reentrant-borrow failure). `alloc`/`dealloc`/`realloc`/`alloc_zeroed`
/// route through the per-thread heap's segment-centric `BinTable` free lists
/// (the Phase 12.1 hot path). With `alloc-xthread`, cross-thread `dealloc`
/// routes through the Phase 10 Treiber stack, now stamped from the
/// registry-resident heap (12.3 owner stamping).
///
/// Thread exit recycles the slot for whole-slot reuse (Phase 12.5): the
/// `HeapCore` and ALL its segments (plus their remote-free queues) stay
/// whole in the slot, and the next claimer reuses them in full — nothing is
/// abandoned or leaked. A primordial fallback heap (§2.3) serves the
/// pre-TLS / post-teardown windows, so the face is **never-null for a
/// serviceable request** (M10).
///
/// This is the **alloc face** of one substrate; the **handle face**
/// (`Region<T>` / `Handle<T>`) is the typed, generational view over the same
/// governed memory. See `docs/ALLOC_PLAN.md` §3 "The two faces".
///
/// # Multi-thread safety — read this before enabling `alloc-global` alone
///
/// **`alloc-global` without `alloc-xthread` is a footgun in any
/// multi-threaded program.** Without `alloc-xthread` there is no
/// ownership-checked routing path for a cross-thread free (no owner stamp,
/// no per-segment `RemoteFreeRing`): a block allocated on thread A and freed
/// on thread B has nowhere sound to go. In this configuration a
/// cross-thread `dealloc` degrades to a **leak of that block**, not a data
/// race — the fallback/registry paths never write into a foreign thread's
/// private free lists — but any workload that regularly frees on a
/// different thread than it allocated (thread pools, work-stealing queues,
/// producer/consumer channels) will leak monotonically under
/// `alloc-global` alone. The companion `fastbin` feature *requires*
/// `alloc-xthread` for exactly this reason (enforced by a `compile_error!`
/// in `lib.rs` — see `Cargo.toml`'s `fastbin = ["alloc-global",
/// "alloc-xthread"]`).
///
/// For any real multi-threaded deployment, build with at least
/// `["alloc-global", "alloc-xthread"]`, or use the `production` feature
/// bundle (`alloc-global + alloc-xthread + alloc-decommit + fastbin`), which
/// is the combination this crate is tested and tuned for. See
/// `docs/INTEGRATION.md` for the full feature matrix.
///
/// # `std`-only
///
/// `SeferAlloc` (and the whole `alloc-global` / `alloc-core` stack) requires
/// `std`: it uses thread-local storage for the per-thread heap binding and
/// `std::time::Instant` in the large-cache decay clock (`alloc-decommit`).
/// It cannot be used in a `no_std` build. The `Region<T>` / `Handle<T>` core
/// (this crate's other face) is `no_std` + `alloc`-only and unaffected by
/// this restriction — see the crate-level docs.
pub struct SeferAlloc {
    /// Large-cache configuration stored at static-init time. Plumbed into
    /// each per-thread `AllocCore` on the first TLS bind for that thread.
    ///
    /// Only present under `alloc-decommit`; without that feature the struct
    /// remains a ZST.
    #[cfg(feature = "alloc-decommit")]
    config: crate::alloc_core::LargeCacheConfig,
}

impl SeferAlloc {
    /// Construct the allocator with default large-cache settings. This is a
    /// zero-cost `const` constructor — the per-thread heaps are lazily
    /// claimed on first use (not here), so this can be used in `static`
    /// initialisers without any allocation or OS calls.
    ///
    /// Equivalent to `SeferAlloc::with_config(LargeCacheConfig::DEFAULT)`.
    ///
    /// # Memory policy — read before deploying
    ///
    /// The default large-object cache headroom is 256 MiB **per
    /// materialized heap/shard**, not 256 MiB process-wide — a thread-per-
    /// core server with many concurrently active heaps multiplies this
    /// figure by however many of them have touched a large allocation.
    /// Idle time alone does not reclaim any of it: decay is inline and
    /// event-driven (there is no background thread, by design), so a
    /// quiet thread/process retains its peak large-cache usage down to
    /// this floor indefinitely, until either more large-alloc/dealloc
    /// traffic drives further decay ticks or the thread exits (the one
    /// unconditional reclamation path, `HeapCore::trim_for_recycle`).
    /// Measured in full — including the exact post-drain floor and the
    /// 2-second, zero-byte-reclaimed idle-window result — in
    /// `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md`. If a smaller
    /// per-heap floor fits your deployment better, see
    /// [`with_profile`](Self::with_profile) with
    /// [`LargeCachePolicy::Trimmed64MiB`](crate::alloc_core::LargeCachePolicy::Trimmed64MiB)
    /// or
    /// [`LargeCachePolicy::LowHeadroom`](crate::alloc_core::LargeCachePolicy::LowHeadroom)
    /// for a shipped, one-line alternative — note that policy is a decay
    /// FLOOR, not an RSS cap; see [`LargeCachePolicy`](crate::alloc_core::LargeCachePolicy)'s
    /// doc — or [`with_config`](Self::with_config) to set an exact
    /// [`LargeCacheConfig::headroom_bytes`](crate::alloc_core::LargeCacheConfig::headroom_bytes)
    /// / [`LargeCacheConfig::budget_bytes`](crate::alloc_core::LargeCacheConfig::budget_bytes)
    /// (the actual admission ceiling) directly.
    #[must_use]
    pub const fn new() -> Self {
        #[cfg(feature = "alloc-decommit")]
        {
            Self {
                config: crate::alloc_core::LargeCacheConfig::DEFAULT,
            }
        }
        #[cfg(not(feature = "alloc-decommit"))]
        {
            Self {}
        }
    }

    /// Construct the allocator with a user-supplied large-cache configuration.
    ///
    /// This is a `const fn` so it can be used in a `static` initialiser
    /// (runnable end-to-end form in `tests/sefer_alloc_with_config.rs`):
    ///
    /// ```text
    /// use sefer_alloc::{SeferAlloc, LargeCacheConfig, LargeCacheMode};
    ///
    /// const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
    ///     .budget_bytes(512 * 1024 * 1024)
    ///     .headroom_bytes(64 * 1024 * 1024)
    ///     .decay_interval_ms(200)
    ///     .decay_rate_percent(25)
    ///     .mode(LargeCacheMode::Lazy);
    ///
    /// #[global_allocator]
    /// static GLOBAL: SeferAlloc = SeferAlloc::with_config(CONFIG);
    /// ```
    ///
    /// The config is stored in the `SeferAlloc` struct and plumbed into a
    /// per-thread heap when that heap's registry slot is **first
    /// materialised** — which happens on the thread's first allocation (the
    /// cold TLS `bind_slow` path; subsequent allocations hit the cached TLS
    /// pointer and never re-read the config).
    ///
    /// # Binding semantics — single instance vs. multiple instances
    ///
    /// The binding has two layers, and both are "first to bind wins":
    ///
    /// - **Per slot (registry):** a registry slot is configured exactly once,
    ///   at its first materialisation. Slots are never de-initialised — when a
    ///   thread exits, its slot is recycled *whole* (same `HeapCore`, same
    ///   config) and reused as-is by whichever thread claims it next. So the
    ///   config of a slot is fixed by the first `SeferAlloc` instance to
    ///   materialise it, for the slot's entire process lifetime.
    /// - **Per thread (TLS):** the first allocation on a thread caches the
    ///   heap pointer in TLS; every later allocation reuses that cached
    ///   pointer. The config is consulted only on the cold first-bind branch.
    ///
    /// For the normal, supported usage — **one** `#[global_allocator]` `static`
    /// `SeferAlloc` per process — this is consistent and correct: every thread
    /// materialises its slot through that single instance, so every thread's
    /// heap carries that one config. (The `static` initialiser also runs
    /// before `main`, before any thread is started, so there is no
    /// "pre-init thread" window.)
    ///
    /// Installing **multiple** `SeferAlloc` instances with *different* configs
    /// in one process is unusual and effectively unsupported: whichever
    /// instance first materialises a given slot / first binds a given thread
    /// wins, and the other instances' configs are silently ignored for that
    /// slot/thread. There is no cross-instance config independence. If you
    /// need distinct large-cache configs, run separate processes — do not
    /// rely on per-instance config under a single global registry.
    ///
    /// **Detecting the conflict (task #95 / N2):** when a later `claim_with_config`
    /// hits a slot that was already materialised with a *different* config, the
    /// mismatch is no longer fully silent: it is counted in
    /// [`config_conflicts`](AllocStats::config_conflicts) (visible via
    /// [`stats()`](Self::stats)), and a `debug_assert!` fires in debug builds.
    /// The slot's existing config still wins (this is a detect-and-signal
    /// fix, not a reconfigure), but a non-zero `config_conflicts` is the
    /// signature that multiple incompatible instances are competing for the
    /// same registry slots.
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub const fn with_config(config: crate::alloc_core::LargeCacheConfig) -> Self {
        Self { config }
    }

    /// Construct the allocator with a [`Profile`](crate::alloc_core::Profile)
    /// — a small builder over two independent, named, measured axes
    /// (R30-7, task #456; reworked into two axes R31-9, task #473):
    /// [`SmallPoolPolicy`](crate::alloc_core::SmallPoolPolicy) (small-pool
    /// `pool_segments`/`pool_byte_cap`) and
    /// [`LargeCachePolicy`](crate::alloc_core::LargeCachePolicy) (large-cache
    /// `headroom_bytes`), each settable independently. See
    /// [`Profile`](crate::alloc_core::Profile)'s own docs for exactly what
    /// each axis sets, why, and — importantly for the large-cache axis — why
    /// it is a decay floor, not an RSS bound.
    ///
    /// This is a thin, discoverable wrapper over
    /// [`LargeCacheConfig::for_profile`](crate::alloc_core::LargeCacheConfig::for_profile)
    /// and [`with_config`](Self::with_config); it is `const fn`, so a
    /// profile can be installed directly in a `#[global_allocator]` `static`
    /// initialiser (illustrative, not a doctest per this project's "no
    /// doctests" rule):
    ///
    /// ```text
    /// use sefer_alloc::{SeferAlloc, Profile, SmallPoolPolicy, LargeCachePolicy};
    ///
    /// #[global_allocator]
    /// static GLOBAL: SeferAlloc = SeferAlloc::with_profile(
    ///     Profile::new()
    ///         .small_pool(SmallPoolPolicy::Throughput)
    ///         .large_cache(LargeCachePolicy::Trimmed64MiB),
    /// );
    /// ```
    ///
    /// See [`with_config`](Self::with_config)'s doc for the binding
    /// semantics (per-slot / per-thread "first to materialise wins") — the
    /// same rules apply here, since this is `with_config` under a named
    /// preset. Does NOT change [`SeferAlloc::new`]'s defaults — this is a
    /// new, opt-in constructor alongside it ([`Profile::DEFAULT`] resolves
    /// to byte-identical settings to [`SeferAlloc::new`] should you want the
    /// same starting point to chain axis overrides from).
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub const fn with_profile(profile: crate::alloc_core::Profile) -> Self {
        Self::with_config(crate::alloc_core::LargeCacheConfig::for_profile(profile))
    }
}

impl Default for SeferAlloc {
    fn default() -> Self {
        Self::new()
    }
}

impl SeferAlloc {
    /// Resolve the current heap for an allocation, threading the config into
    /// the TLS bind slow path when `alloc-decommit` is active.
    ///
    /// Under `not(alloc-decommit)` this delegates to the config-free
    /// [`current_for_alloc`]; under `alloc-decommit` it passes `&self.config`
    /// (a single pointer load, not a ~40-byte value copy) into
    /// [`current_for_alloc_with_config`]. The reference avoids
    /// materialising the config on the hot fast path — the dereference
    /// happens only on the cold `bind_slow` branch.
    #[inline(always)]
    fn current_heap(&self) -> CurrentHeap {
        #[cfg(feature = "alloc-decommit")]
        {
            current_for_alloc_with_config(&self.config)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        {
            current_for_alloc()
        }
    }

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
    /// [`current_heap`](Self::current_heap), which — for a freshly-bound,
    /// never-allocated thread — would itself claim a fresh registry slot
    /// and bind an (empty) per-thread heap (`global::tls_heap::finish_bind`)
    /// purely as a side effect of asking "is there anything to trim?",
    /// exactly as an ordinary first allocation would. That was harmless
    /// (the slot's `AbandonGuard` was armed, so it still recycled correctly
    /// at thread exit) but wasteful: a monitoring/housekeeping routine that
    /// calls `trim_current_thread()` speculatively across many threads —
    /// some of which never allocate — would claim a registry slot for every
    /// one of them regardless. This method now resolves via
    /// [`tls_heap::current_for_trim`](super::tls_heap::current_for_trim), a
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
            unsafe { (*heap).trim_for_recycle() };
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
    #[doc(hidden)]
    pub fn dbg_trim_current_thread(&self) {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (same single-writer
            // invariant `alloc`/`dealloc` above rely on) — `current_heap()`
            // just resolved it for the calling thread.
            unsafe { (*heap).trim_for_recycle() };
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
    /// `src/alloc_core/remote_free_ring.rs` — is the mechanism actually under
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
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-xthread", feature = "bench-internals"))]
    pub fn dbg_drain_current_thread_rings(&self) {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (same single-writer
            // invariant `alloc`/`dealloc` above rely on) — `current_heap()`
            // just resolved it for the calling thread; `dbg_drain_all_rings`
            // requires `&mut HeapCore`, sound here because we are the sole
            // writer (single-consumer-per-heap, matching every other
            // `dbg_*` mutator in this file's identical `unsafe` shape).
            unsafe { (*heap).dbg_drain_all_rings() };
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
    /// bound heap via [`current_heap`](Self::current_heap) (never claims a
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
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-decommit",
        feature = "bench-internals",
        feature = "large-cache-extended"
    ))]
    #[must_use]
    pub fn dbg_current_large_cache_extension_materialised(&self) -> bool {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (same single-writer
            // invariant `alloc`/`dealloc` rely on) — `current_heap()` just
            // resolved it for the calling thread. `dbg_large_cache_extension_materialised`
            // is a read-only `&self` accessor.
            unsafe { (*heap).dbg_large_cache_extension_materialised() }
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
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_current_large_cache_total_slots(&self) -> usize {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread — same justification as
            // `dbg_current_large_cache_extension_materialised` above.
            unsafe { (*heap).dbg_large_cache_total_slots() }
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
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_current_large_cache_budget(&self) -> Option<usize> {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread — same justification as
            // `dbg_current_large_cache_extension_materialised` above.
            unsafe { (*heap).dbg_large_cache_budget() }
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
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
    #[must_use]
    pub fn dbg_current_large_cache_used_bytes(&self) -> usize {
        if let CurrentHeap::Own(heap) = self.current_heap() {
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread — same justification as
            // `dbg_current_large_cache_extension_materialised` above.
            unsafe { (*heap).dbg_large_cache_used() }
        } else {
            0
        }
    }

    /// R10-7 (Part 2) — **tcache-aware batch allocation** wrapper.
    ///
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// This API has NO semver guarantees. It may change signature, behavior,
    /// or be removed entirely in any release without a major version bump,
    /// for as long as the `batch-api` feature (which requires
    /// `experimental`) remains unstable. Use at your own risk in production
    /// code.
    ///
    /// # API boundary — `batch-api` Cargo feature (R10-7 follow-up)
    ///
    /// `#[doc(hidden)]` alone is NOT a real API boundary: it hides the item
    /// from rustdoc but leaves it on the public semver/ABI surface (external
    /// code can still call it, and a signature change would still be a
    /// breaking change). This method (and `dealloc_batch` below) is
    /// additionally gated behind the **`batch-api` Cargo feature** (which
    /// itself requires `experimental` — R12-12), which is NOT part of
    /// `production` or any default-on bundle. Downstream code cannot reach
    /// this surface at all without explicitly opting in, so the signature can
    /// evolve freely without semver consequences for the vast majority of
    /// users (who build with `production` alone). Chosen over `pub(crate)` +
    /// an adapter because the existing bench/test consumers
    /// (`benches/global_alloc.rs`'s `batch_tcache` arm, `tests/batch_tcache.rs`,
    /// and the new `tests/r10_7_alloc_batch_xthread_double_free.rs`) live
    /// OUTSIDE the crate and need a `pub` path — a feature gate preserves
    /// their access pattern while adding the hard semver boundary the review
    /// asked for. `#[doc(hidden)]` was dropped (R12-12): hiding a stable-
    /// looking signature from rustdoc is not the same as marking it
    /// unstable — a user who enables `batch-api` and finds these functions
    /// via IDE autocomplete or the source deserves a visible warning, not a
    /// silently-absent one.
    ///
    /// Resolves the per-thread heap ONCE (one TLS lookup for the whole batch,
    /// vs N for N scalar `alloc` calls), then delegates to
    /// [`HeapCore::alloc_batch`], which drains the warm magazine and
    /// batch-refills only the remainder. Returns the number of slots filled
    /// (0 only on true OOM); `out[filled..]` is left uninitialised and MUST
    /// NOT be used by the caller.
    ///
    /// # Safety
    /// Same contract as [`GlobalAlloc::alloc`]: `layout` must be a non-zero-size
    /// valid `Layout`. Every returned non-null pointer is a live allocation owned
    /// by this allocator and must be freed exactly once via [`dealloc_batch`] (or
    /// N scalar `dealloc` calls). Null entries (on partial fill / OOM) must not
    /// be freed.
    ///
    /// [`dealloc_batch`]: Self::dealloc_batch
    #[cfg(feature = "batch-api")]
    pub unsafe fn alloc_batch(&self, layout: Layout, out: &mut [*mut u8]) -> usize {
        match self.current_heap() {
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc_batch(layout, out)).unwrap_or(0)
            }
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
            // registry slot owned by THIS thread (single-writer invariant) —
            // `current_heap()` just resolved it for the calling thread.
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc_batch(layout, out) },
        }
    }

    /// R10-7 (Part 2); batched by R11-4 — **batch deallocation** wrapper.
    ///
    /// # ⚠ EXPERIMENTAL / UNSTABLE
    ///
    /// This API has NO semver guarantees. It may change signature, behavior,
    /// or be removed entirely in any release without a major version bump,
    /// for as long as the `batch-api` feature (which requires
    /// `experimental`) remains unstable. Use at your own risk in production
    /// code.
    ///
    /// Same `batch-api` feature boundary as [`alloc_batch`] (see that
    /// method's API-boundary doc section). Resolves the per-thread heap
    /// ONCE, then delegates to [`HeapCore::dealloc_batch`], which partitions
    /// `blocks` into a this-heap-owned Small-classified fast subset (batched
    /// magazine-fill + `flush_class` overflow — see that method's doc
    /// comment for the full mechanism and the stated magazine-warmth
    /// trade-off) and a scalar fallback for everything else (foreign,
    /// cross-thread-owned, Large-classified, null). Null entries are always
    /// skipped (matching the per-block contract).
    ///
    /// # Safety
    /// Same contract as [`GlobalAlloc::dealloc`]: every non-null `blocks[i]`
    /// must be the exact start pointer of a currently-live allocation made by
    /// this allocator, with `layout` matching its allocation, and freed at most
    /// once. Null entries are always safe (skipped).
    ///
    /// [`alloc_batch`]: Self::alloc_batch
    #[cfg(feature = "batch-api")]
    pub unsafe fn dealloc_batch(&self, layout: Layout, blocks: &[*mut u8]) {
        match self.current_heap() {
            CurrentHeap::Fallback => {
                // SAFETY: caller upholds the dealloc-batch contract for
                // every non-null entry of `blocks`.
                let _ = fallback::with_heap(|h| unsafe { h.dealloc_batch(layout, blocks) });
            }
            // SAFETY: `heap` is non-null and points to a live `HeapCore` owned
            // by THIS thread (single-writer invariant); `HeapCore::dealloc_batch`
            // upholds the same per-block contract as scalar `dealloc` for every
            // entry it does not route through its batched fast path.
            CurrentHeap::Own(heap) => unsafe { (*heap).dealloc_batch(layout, blocks) },
        }
    }
}

// SAFETY (the trait obligation): `GlobalAlloc` requires that `alloc`/
// `alloc_zeroed`/`realloc` return valid memory for the requested `Layout`
// (or null on failure), and that `dealloc` receives a pointer previously
// returned by an allocating method. We delegate to `HeapCore::alloc`/
// `dealloc`/`realloc`/`alloc_zeroed`, which uphold M1 (validity), M3 (no
// overlap), and M4 (alignment/size fidelity) -- verified by the Phase 8/9
// differential proptests and miri. `HeapCore` returns null on OOM (never
// panics -- the substrate panic sites were hardened in Phase 11). If the
// TLS heap is unavailable (thread teardown), `current()` returns the
// process-global fallback heap (never null); `dealloc` on the fallback is
// sound under the fallback's spinlock. M10 (never-null for serviceable
// requests) is upheld: the only null return is true OOM.
unsafe impl GlobalAlloc for SeferAlloc {
    #[inline(always)]
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        match self.current_heap() {
            // Fallback path (TLS torn down, registry exhausted, or true
            // fallback OOM): route through the fallback's spinlock-guarded
            // `with_heap`. `with_heap` returns `None` only on true OOM → we
            // surface null.
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc(layout)).unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: `heap` is non-null and points to a live `HeapCore` in
            // a registry slot. `current_heap` returned it for THIS thread;
            // the single-writer invariant (the CAS-won slot owner) makes
            // `&mut` access exclusive. `HeapCore::alloc` upholds the
            // GlobalAlloc contract (returns valid memory or null).
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc(layout) },
        }
    }

    #[inline(always)]
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        // R6-OPT-P0-1: under `alloc-xthread`, resolve via the DEALLOC-ONLY
        // `current_for_dealloc` — NOT `self.current_heap()` — so a thread
        // whose TLS is null (never allocated anything itself) or `TORN`
        // (already exited) does not pay to claim a registry slot or take the
        // fallback spinlock just to free one foreign pointer. See
        // `tls_heap::current_for_dealloc`'s doc comment for the full
        // rationale, and the "TORN + fallback-owned" trade-off note below.
        #[cfg(feature = "alloc-xthread")]
        {
            match current_for_dealloc() {
                CurrentHeapForDealloc::Own(heap) => {
                    // SAFETY: as the `Own` arm below — `heap` is non-null and
                    // points to a live `HeapCore` in a registry slot this
                    // thread owns (single-writer invariant).
                    unsafe { (*heap).dealloc(ptr, layout) };
                }
                CurrentHeapForDealloc::ForeignNoBind => {
                    // This thread never bound a heap, or its heap's slot was
                    // already recycled (TORN), or its TLS is torn down. Every
                    // valid pointer reaching `dealloc` here is foreign BY
                    // CONSTRUCTION (see `current_for_dealloc`'s doc comment):
                    // route it directly through the heap-instance-independent
                    // cross-thread routing tail, WITHOUT claiming a registry
                    // slot and WITHOUT constructing or dereferencing any
                    // `*mut HeapCore` at all.
                    //
                    // Deliberate, documented trade-off (verified sound — see
                    // `HeapCore::dealloc_foreign_routing`'s doc comment and
                    // the R6-OPT-P0-1 task report): for the TORN case
                    // specifically, the OLD code routed through
                    // `fallback::with_heap`, which checked the FALLBACK
                    // heap's OWN `contains_base` FIRST — so a pointer that
                    // genuinely belongs to the fallback's own segments took
                    // the direct free path under the lock. This shortcut has
                    // no fallback `HeapCore` instance to consult, so it
                    // ALWAYS treats a TORN thread's dealloc as foreign-by-
                    // header, pushing onto whatever ring the header says
                    // owns it — for a fallback-owned pointer, that means the
                    // fallback's OWN ring instead of a direct free. This is
                    // NOT a correctness bug: pushing to a ring is safe for
                    // ANY live segment regardless of caller identity (see
                    // `dealloc_foreign_routing`'s doc comment), and the
                    // fallback drains its own ring lazily on its next
                    // `with_heap` call exactly like any other segment's
                    // owner — it is a narrow efficiency trade-off in an
                    // already-rare corner case (TORN AND fallback-owned),
                    // traded for removing the claim/lock cost in the
                    // overwhelmingly common case this task targets.
                    //
                    // SAFETY: `ptr`/`layout` are the caller-bound
                    // `GlobalAlloc::dealloc` contract pair (this whole fn is
                    // `unsafe fn dealloc`); `dealloc_foreign_routing` applies
                    // the SAME null-base and magic-mismatch guards
                    // `dealloc_foreign_slow` already uses before touching any
                    // segment memory, so a dangling/garbage `ptr` cannot
                    // fault here either.
                    let base = crate::alloc_core::os::segment_base_of_ptr(ptr);
                    crate::registry::HeapCore::dealloc_foreign_routing(ptr, base, layout, None);
                }
            }
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            // Without `alloc-xthread` there is no heap-instance-independent
            // routing concept (no owner stamp, no per-segment
            // `RemoteFreeRing`) — fall back to the OLD behavior: resolve via
            // `current_heap()` (bind/fallback as before). Do not attempt the
            // P0-1 shortcut when cross-thread routing does not exist.
            match self.current_heap() {
                CurrentHeap::Fallback => {
                    // Fallback path: dealloc under the spinlock. A failure
                    // here (true OOM at fallback init) is a safe no-op — the
                    // block is leaked, never corrupted.
                    //
                    // SAFETY: `ptr`/`layout` are the caller-bound GlobalAlloc
                    // contract pair (this whole fn is `unsafe fn dealloc`);
                    // the closure forwards them to the fallback heap's
                    // `HeapCore::dealloc`.
                    let _ = fallback::with_heap(|h| unsafe { h.dealloc(ptr, layout) });
                }
                // SAFETY: as above. For a LIVE/MAPPED pointer this routes
                // correctly regardless of which thread allocated it
                // (own-thread only, without `alloc-xthread`), and the M2
                // double-free guard makes a repeated free of a still-mapped
                // block a no-op. This is NOT a blanket "safe on any
                // foreign/dangling pointer" claim: a dangling pointer into an
                // already-RELEASED, unmapped segment is fundamentally UB —
                // not calling `dealloc` on an already-freed pointer is the
                // caller's baseline `GlobalAlloc` obligation (a basic trait
                // contract, not something M2 relaxes); M2 hardens the
                // live-block case, it does not extend the contract to
                // released memory.
                CurrentHeap::Own(heap) => unsafe { (*heap).dealloc(ptr, layout) },
            }
        }
    }

    #[inline]
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        match self.current_heap() {
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| h.alloc_zeroed(layout)).unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: as in `alloc`.
            CurrentHeap::Own(heap) => unsafe { (*heap).alloc_zeroed(layout) },
        }
    }

    #[inline]
    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        match self.current_heap() {
            // SAFETY: `ptr`/`old_layout` are the caller-bound GlobalAlloc
            // contract pair (this whole fn is `unsafe fn realloc`); the
            // closure forwards them to the fallback heap's `HeapCore::realloc`.
            CurrentHeap::Fallback => {
                fallback::with_heap(|h| unsafe { h.realloc(ptr, old_layout, new_size) })
                    .unwrap_or(core::ptr::null_mut())
            }
            // SAFETY: as in `alloc`. `realloc` takes the C2 in-place fast path
            // for a same-class / compatible resize of an own-thread block, and
            // otherwise falls back to alloc-new + copy + dealloc-old, leaving
            // the old allocation intact on OOM.
            CurrentHeap::Own(heap) => unsafe { (*heap).realloc(ptr, old_layout, new_size) },
        }
    }
}
