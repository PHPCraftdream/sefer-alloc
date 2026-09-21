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
/// This is `SeferAlloc`'s own segment substrate. The **handle face**
/// (`Region<T>` / `Handle<T>`, in the separate `sefer-region` crate) is an
/// independent typed API over third-party `slotmap` -- it shares no backing
/// memory with this substrate (corrected 2026-08-09 per the static release
/// audit's F5; `docs/ALLOC_PLAN.md` SS3 preserves the original "one substrate,
/// two faces" design intent as history, not current architecture).
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
/// bundle (`alloc-global + alloc-xthread + alloc-decommit + fastbin + alloc-segment-directory + primordial-lazy-commit + class-aware-dirty`), which
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
    /// (a single pointer load, not a value copy) into
    /// [`current_for_alloc_with_config`]. The reference avoids
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
}
