#[cfg(not(feature = "alloc-decommit"))]
use crate::global::tls_heap::current_for_alloc;
#[cfg(feature = "alloc-decommit")]
use crate::global::tls_heap::current_for_alloc_with_config;
use crate::global::tls_heap::CurrentHeap;

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
/// (the Phase 12.1 hot path). `alloc-global` unconditionally implies
/// `alloc-xthread` (R5-01), so a cross-thread `dealloc` is routed by ADDRESS
/// through the process-wide route directory and never borrows another heap:
/// the address lookup pins the block's independently allocated (System-backed)
/// route descriptor; a ready, idle fallback heap reclaims its own block
/// synchronously under its owner lease, and every other case performs ONE
/// terminal publication into that pinned sidecar — an AcqRel pending-bit RMW
/// for Small/Primordial blocks, a LIVE(g) → PENDING(g) CAS on the descriptor's
/// independent Large state word for Large blocks. That publication is the
/// freeing thread's LAST access to the allocation, and exactly one publication
/// is made per freed allocation instance; the owning heap consumes the
/// published records later in a bounded cut/reclaim pass. An address the
/// directory cannot route (or a rejected publication) drops the free and
/// counts `foreign_or_unroutable_frees` — defensive behavior for an invalid
/// pointer, not permission to pass one.
///
/// Thread exit recycles the slot for whole-slot reuse (Phase 12.5): the
/// `HeapCore` and ALL its segments stay
/// whole in the slot, and the next claimer reuses them in full — nothing is
/// abandoned or leaked. A primordial fallback heap (§2.3) serves the
/// pre-TLS / post-teardown windows, so the face is **never-null for a
/// serviceable request** (M10).
///
/// This is `SeferAlloc`'s own segment substrate. The **handle face**
/// (`Region<T>` / `Handle<T>`, in the separate `sefer-region` crate) is an
/// independent typed API over third-party `slotmap` -- it shares no backing
/// memory with this substrate (corrected 2026-08-09 per the static release
/// audit's F5; `docs/ALLOC_PLAN.md` SS3 preserves the original "one substrate,
/// two faces" design intent as history, not current architecture).
///
/// # Multi-thread safety
///
/// `alloc-global` implies `alloc-xthread`, so a plain `--features
/// alloc-global` build routes cross-thread `dealloc` and `realloc` soundly
/// (thread pools, channels and work-stealing queues are fine). Without
/// `alloc-xthread` a cross-thread free leaked its block and a cross-thread
/// `realloc` returned null (R5-01); `lib.rs` now refuses to build that
/// combination.
///
/// The `production` feature bundle
/// (`alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory + primordial-lazy-commit`)
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
///
/// # Fork safety
///
/// There is no `pthread_atfork` handling in this crate (unlike glibc /
/// jemalloc / mimalloc). Single-threaded `fork()` is fine. `fork()` of a
/// multi-threaded process followed immediately by `exec()` is also fine
/// (the POSIX async-signal-safe-only rule for the child). What is **not**
/// safe: allocating or freeing through `SeferAlloc` in the child of a
/// multi-threaded `fork()` before `exec()` — inherited allocator state
/// another thread was mutating at fork time can wedge the child,
/// including on `dealloc`: frozen locks and init/ownership states (the
/// fallback spinlock, a route-directory shard lock, an in-progress
/// primordial/registry-chunk materialisation) are inherited held by a
/// thread that does not survive the fork, and terminal publications
/// left in route sidecars have no surviving owner to drain them in the
/// child. See README.md's "Fork safety" section for the full contract
/// and the exact code sites; tracked in `docs/CORRECTNESS_OPEN_ITEMS.md`.
///
/// ```text
/// // Safe: fork + exec, no allocator use in the child before exec.
/// std::process::Command::new("some-binary").spawn()?;
///
/// // Unsafe: allocating/freeing through SeferAlloc in the child before exec,
/// // from a multi-threaded parent — do not do this.
/// ```
///
/// # Known defect (P1-box)
///
/// Free-list links live inside freed small blocks. If a block is freed through
/// a by-value `Box` whose receiving frame is still executing when the allocator
/// links it into a free list (owner reclaim of a cross-thread free, or a later
/// magazine flush), Miri's Stacked Borrows and Tree Borrows report undefined
/// behaviour (protector violation). No native crash or miscompilation is known;
/// the aliasing models are experimental, and this is not a guarantee that none
/// exists. Re-issuing those bytes while the freeing frame is live is a separate
/// model limit shared by any allocator on a shared reservation. See
/// `docs/CORRECTNESS_OPEN_ITEMS.md` item 164.
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
    /// Idle time alone does not reclaim any of it: large-cache decay is a
    /// per-heap, inline, event-driven mechanism on the owning thread's own
    /// allocation traffic (separate from the opt-in maintenance worker) — so a quiet thread retains its peak
    /// large-cache usage down to this floor indefinitely, until either more of
    /// its own large-alloc/dealloc traffic drives further decay ticks or the
    /// thread exits (the one unconditional reclamation path,
    /// `HeapCore::trim_for_recycle`). A process that additionally wants
    /// ownerless reclamation must explicitly activate
    /// `SeferAlloc::start_maintenance()`: without that activation no
    /// autonomous worker exists; with it, the only guarantee is that
    /// method's own contract — eventual logical reclamation of correctly
    /// published frees in heaps remaining FREE (owning thread exited) and
    /// the idle fallback, under fair scheduling, with no latency SLA. An
    /// OWNED heap whose owner is merely paused is never taken over: the
    /// worker cannot reclaim its state, so its floor persists until that
    /// owner resumes or exits. No idle-owner RSS return is implied by decay
    /// or by the worker.
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
    /// pointer and never re-read the config). A fallback allocation instead
    /// materialises the process-global fallback heap with this same policy.
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
    /// - **Fallback:** the first fallback allocation fixes its process-global
    ///   heap policy. Later incompatible instances cannot reconfigure it;
    ///   each conflicting fallback allocation increments
    ///   [`fallback_config_conflicts`](Self::fallback_config_conflicts).
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
    /// **Detecting the conflict (task #95 / N2):** when a later `claim_lease_with_config`
    /// hits a slot that was already materialised with a *different* config, the
    /// mismatch is no longer fully silent: it is counted in
    /// [`config_conflicts`](crate::global::AllocStats::config_conflicts) (visible via
    /// [`stats()`](Self::stats)). That counter is the only signal, in every
    /// build profile: the conflict is detected on the cold bind path behind
    /// every [`GlobalAlloc`](::core::alloc::GlobalAlloc) method, which must
    /// never panic (R2-08 — a former debug-build `debug_assert!` here
    /// unwound out of `GlobalAlloc::alloc`). The slot's existing config still
    /// wins (this is a detect-and-signal fix, not a reconfigure), but a
    /// non-zero `config_conflicts` is the signature that multiple
    /// incompatible instances are competing for the same registry slots.
    /// Fallback conflicts have their own counter above; `stats()` does not
    /// yet include them.
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
    /// new, opt-in constructor alongside it ([`Profile::DEFAULT`](crate::alloc_core::Profile::DEFAULT) resolves
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
    /// [`current_for_alloc`](crate::global::tls_heap::current_for_alloc); under `alloc-decommit` it passes `&self.config`
    /// (a single pointer load, not a value copy) into
    /// the feature-gated `current_for_alloc_with_config` resolver. The reference avoids
    /// materialising the config on the hot fast path — the dereference
    /// happens only on the cold `bind_slow_tagged_with_config` branch.
    #[inline(always)]
    pub(super) fn current_heap(&self) -> CurrentHeap {
        #[cfg(feature = "alloc-decommit")]
        {
            current_for_alloc_with_config(&self.config)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        {
            current_for_alloc()
        }
    }

    /// Run a fallback allocation under the same policy as this instance.
    #[inline]
    pub(super) fn with_fallback_heap<F, R>(&self, f: F) -> Option<R>
    where
        F: FnOnce(&mut crate::registry::HeapCore) -> R,
    {
        #[cfg(feature = "alloc-decommit")]
        {
            crate::global::fallback::with_heap_config(self.config, f)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        {
            crate::global::fallback::with_heap(f)
        }
    }

    /// Number of fallback policy conflicts. The first fallback initializer's
    /// policy wins; later incompatible requests are counted without panicking.
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn fallback_config_conflicts(&self) -> u64 {
        crate::global::fallback::config_conflicts_total()
    }
}
