//! [`LargeCacheConfig`] — a `const`-buildable configuration type for the
//! per-shard large-segment free-cache (feature = `alloc-decommit`).
//!
//! This replaces the environment-variable parsers that used to live in
//! `alloc_core.rs`. The caller constructs a `LargeCacheConfig` value at
//! compile time (or at runtime — the builder is `const` but not *required*
//! to be) and passes it to [`AllocCore::new_with_config`](crate::alloc_core::AllocCore::new_with_config) or
//! [`SeferAlloc::with_config`](crate::SeferAlloc::with_config). If the caller wants to read from the
//! environment, a CLI flag, or a config file, they do that themselves and
//! pass the resolved values here.
//!
//! ## Default values
//!
//! `LargeCacheConfig::DEFAULT` (= `LargeCacheConfig::new()`) applies exactly
//! the same defaults the old env-var parsers used when no variable was set:
//!
//! | Knob | Default | Corresponds to old env var |
//! |---|---|---|
//! | `budget_bytes` | `None` (unbounded) | `SEFER_LARGE_CACHE_BUDGET` unset |
//! | `headroom_bytes` | `256 MiB` | `SEFER_LARGE_CACHE_HEADROOM_BYTES` unset |
//! | `decay_interval_ms` | `1000` ms | `SEFER_LARGE_CACHE_DECAY_INTERVAL_MS` unset |
//! | `decay_rate_percent` | `10` % | `SEFER_LARGE_CACHE_DECAY_RATE` unset |
//! | `mode` | `LargeCacheMode::Lazy` | `SEFER_LARGE_CACHE_MODE` unset |
//!
//! ## Builder contract
//!
//! All setter methods are `const fn` so a `LargeCacheConfig` can be built in
//! a `const` context and placed under a `static`. Validation and clamping
//! happen at *resolution* time (inside `AllocCore::new_with_config`), not at
//! build time, so no `Result` is needed and there is nothing to panic on in a
//! `const` context. Per-setter contracts are documented on each method.

// `LargeCacheMode` lives in the sibling sub-module `alloc_core::large_cache_mode`
// and is re-exported by `alloc_core::mod.rs`.
use crate::alloc_core::config::large_cache_mode::LargeCacheMode;
// `SmallSegmentPoolConfig` (Mechanism 2, task #51) lives in the sibling
// sub-module `alloc_core::small_segment_pool_config`. It is carried as a field
// here — both are `alloc-decommit`-gated construction-time knobs threaded
// through the SAME single-config plumbing, so embedding it avoids adding a
// second config parameter through four API layers (see that type's module
// docs for the wiring rationale).
use crate::alloc_core::config::small_segment_pool_config::SmallSegmentPoolConfig;

// ── Default constants (kept in sync with the old env-parser defaults) ─────────

/// Default headroom: 256 MiB. This is the decay trigger/target for
/// anti-thrashing; FIFO eviction can release whole spans and overshoot it.
pub(crate) const DEFAULT_HEADROOM_BYTES: usize = 256 * 1024 * 1024;

/// Default decay interval: 1000 ms (1 second per due interval).
pub(crate) const DEFAULT_DECAY_INTERVAL_MS: u64 = 1000;

/// Default decay rate: 10 % per tick, expressed as a percentage.
pub(crate) const DEFAULT_DECAY_RATE_PERCENT: u32 = 10;

/// R14-5 (task #290), revised R17-9 (task #326): default byte-budget ceiling
/// applied ONLY when the `large-cache-extended` feature is compiled in AND
/// the caller has not explicitly set
/// [`budget_bytes`](LargeCacheConfig::budget_bytes).
///
/// ## Why this exists
///
/// The base 8-slot cache's own default budget is `None` (unbounded) — with
/// only 8 slots, an unbounded budget is self-limiting in practice (at most 8
/// distinct spans can ever be resident, however large). `large-cache-extended`
/// (R13-7, task #277) raises the slot ceiling to 40 (8 base + 32 sidecar), so
/// the SAME `None` default stops being self-limiting: a working set of 40
/// distinct, individually-large spans can now retain multiples of the
/// committed RSS the base cache could ever hold, with no ceiling at all
/// (Round 13 review finding, `docs/perf/R13_WAVE_SUMMARY.md`: "RSS-удержание
/// ... hard budget отключён ... может временно увеличить retained committed
/// memory примерно в 5× относительно базовых 8 слотов").
///
/// ## The chosen policy (R17-9)
///
/// `large-cache-extended` gets its OWN finite default —
/// **`1 * DEFAULT_HEADROOM_BYTES` (1 × 256 MiB = 256 MiB)** — applied only
/// when `budget_bytes` was never called on this config. This is a **per-heap**
/// ceiling: `AllocCore` is owner-only (neither `Send` nor `Sync`), so a
/// thread-per-core server that runs one `AllocCore` per thread multiplies
/// this default by however many heaps are concurrently exercising
/// `large-cache-extended` with a large, diverse working set — there is no
/// process-wide coordination between heaps (see "Rejected: process-global
/// budget" below). An external review of this exact multiplier (R17-9, task
/// #326) flagged that the previous 5x default (1280 MiB/heap) could total
/// tens of GiB of retained committed memory across many concurrently-active
/// heaps in that topology. `1x` (256 MiB/heap) keeps the SAME order of
/// magnitude as the base cache's own anti-thrashing floor — a caller with a
/// large fleet of heaps under this feature now retains, by default, roughly
/// what a single base-cache heap's decay floor already tolerates per heap,
/// not five times that — while still leaving a genuinely useful non-zero
/// cache (this is a default that trades away some of the wide-working-set
/// hit-rate win measured in `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md`
/// §6 for a smaller worst-case footprint; a caller who has measured their own
/// workload and wants the larger cache back calls `.budget_bytes(n)`
/// explicitly — see below).
///
/// **Rejected: the original R14-5 5x multiplier.** The previous default
/// (`5 * DEFAULT_HEADROOM_BYTES` = 1280 MiB) mirrored the `40 / 8 = 5x`
/// slot-count ratio the extension itself introduces — a proportional
/// extrapolation from the base cache's informal "self-limiting via slot
/// count" bound, not a measurement of any real workload's needs. That
/// reasoning is still sound for a SINGLE heap in isolation (see
/// `docs/perf/R14_5_LARGE_CACHE_EXTENDED_HARDENING_GATE.md` §3.2, which
/// measured 1280 MiB/heap neutralising an adversarial single-heap RSS
/// scenario), but it does not account for the multi-heap case: the ratio was
/// chosen to make ONE heap's ceiling proportionate to slot count, with no
/// consideration of how many heaps a process might run concurrently. Rather
/// than re-deriving a new ratio from another proportionality argument, this
/// revision drops back to `1x` — the same constant the base cache already
/// uses as its "how much is normal" floor, now doing double duty as the
/// extended cache's ceiling too, which needs no new argument for why THIS
/// particular multiple is the right one.
///
/// **Rejected: process-global budget.** A shared `AtomicUsize`-based
/// reserve/release protocol across all heaps in a process would directly
/// solve the "N heaps × per-heap limit" multiplication this default cannot
/// address — but no such process-wide accounting infrastructure exists
/// anywhere in this crate today to build on (`AllocCore`'s existing
/// diagnostic counters, e.g. `large_cache_hits` in
/// [`AllocStats`](crate::AllocStats), are per-heap fields collected by the
/// registry, not a shared atomic; see `src/global/alloc_stats.rs`). Adding
/// one would mean a new global CAS-based protocol, its own loom
/// verification, and new cross-heap contention on a path that is currently
/// heap-local by design — real added surface for a P2 finding on a feature
/// that is opt-in and not part of `production`. Left as a candidate future
/// task if a concrete multi-heap workload demonstrates the per-heap default
/// (even at 1x) is still insufficient in aggregate.
///
/// An explicit `.budget_bytes(n)` call — including `.budget_bytes(0)` (cache
/// disabled) or a value larger than this default — always wins; this
/// constant is a FALLBACK for the unset case only, exactly like every other
/// `resolved_*` default in this module. Explicitly calling
/// `.budget_bytes(usize::MAX)` (or any value `>= DEFAULT_EXTENDED_BUDGET_BYTES`
/// that the caller wants larger) recovers the pre-R14-5 unbounded-by-default
/// behaviour for callers who have measured their own workload and want it.
///
/// Base-cache behaviour (feature `large-cache-extended` OFF) is completely
/// unaffected — `resolved_budget_bytes` only branches on this constant when
/// the feature is compiled in, so the base 8-slot cache's `None`-means-
/// unbounded default is byte-for-byte unchanged (the feature-OFF build in
/// `tests/large_cache_extended_off_no_overflow_capacity.rs` continues to
/// pass with the old unbounded-by-default behaviour; the ON-build coverage
/// for THIS constant lives in
/// `tests/large_cache_extended_budget_before_materialization.rs`).
#[cfg(feature = "large-cache-extended")]
pub(crate) const DEFAULT_EXTENDED_BUDGET_BYTES: usize = DEFAULT_HEADROOM_BYTES;

// ── The config type ───────────────────────────────────────────────────────────

/// Compile-time-buildable configuration for the per-shard large-segment
/// free-cache.
///
/// Construct with the [`new`](Self::new) associated function (or the
/// [`DEFAULT`](Self::DEFAULT) constant) and chain the setter methods
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
/// Passing `LargeCacheConfig::DEFAULT` (= `LargeCacheConfig::new()`) to
/// [`SeferAlloc::with_config`](crate::SeferAlloc::with_config) produces
/// byte-identical behaviour to [`SeferAlloc::new`](crate::SeferAlloc::new).
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, Debug)]
pub struct LargeCacheConfig {
    /// Per-shard hard ceiling on total cached bytes. `None` = unbounded;
    /// `Some(0)` = cache disabled (every deposit is released to the OS
    /// immediately); `Some(n > 0)` = a finite ceiling.
    ///
    /// When set, FIFO eviction fires before admitting a new span that would
    /// push the total above this limit. If the limit is smaller than the span
    /// being freed, the span is released to the OS immediately (not cached).
    pub(crate) budget_bytes: Option<usize>,

    /// Anti-thrashing decay trigger/target, not a hard floor: whole-span FIFO
    /// eviction can release below this level. `None` uses the default (256 MiB).
    pub(crate) headroom_bytes: Option<usize>,

    /// Elapsed-time period used to count due decay steps, in milliseconds.
    /// This is not a minimum spacing between invocations: bounded catch-up
    /// can process several due steps in one invocation.
    /// `None` uses the default (1000 ms).
    pub(crate) decay_interval_ms: Option<u32>,

    /// Fraction of the excess to release per tick, in integer percent
    /// `[1, 100]`. `None` uses the default (10 %).
    ///
    /// Values outside `[1, 100]` are clamped at resolution time.
    pub(crate) decay_rate_percent: Option<u32>,

    /// Cache operating mode. `None` uses the default (`Lazy`).
    pub(crate) mode: Option<LargeCacheMode>,

    /// Empty-small-segment hysteresis pool config (Mechanism 2, task #51).
    /// Carried here because it is threaded through the SAME construction-time
    /// plumbing as the large-cache knobs; see
    /// [`SmallSegmentPoolConfig`](crate::alloc_core::config::small_segment_pool_config::SmallSegmentPoolConfig)
    /// for the full rationale. Defaults to `SmallSegmentPoolConfig::DEFAULT`
    /// (pool ENABLED, 4 segments / 16 MiB) — so `production` gets pooling on by
    /// default with no explicit opt-in.
    pub(crate) pool: SmallSegmentPoolConfig,

    /// G1-K7: per-heap byte budget of the magazine-depot overflow (blocks
    /// parked above the fixed per-class depot floor). `None` uses
    /// [`DEFAULT_DEPOT_BUDGET_BYTES`]; `Some(0)` disables the overflow.
    pub(crate) depot_budget_bytes: Option<usize>,
}

/// G1-K7: default depot-overflow budget per heap (1 MiB of parked blocks).
pub(crate) const DEFAULT_DEPOT_BUDGET_BYTES: usize = 1024 * 1024;
/// G1-K7: bytes of the smallest full magazine (16 blocks of 16 B); the
/// budget resolves to a multiple of it.
pub(crate) const DEPOT_BUDGET_GRAIN: usize = 256;
/// G1-K7: budget ceiling — `(u16::MAX - DEPOT_K_MAX) * 256`, so a class's
/// `PerClass::parked: u16` holds its floor plus the whole overflow even for
/// 16 B blocks (pinned against the registry constants in `state/depot.rs`).
pub(crate) const DEPOT_BUDGET_MAX: usize = (u16::MAX as usize - 8) * DEPOT_BUDGET_GRAIN;

#[cfg(feature = "alloc-decommit")]
impl Default for LargeCacheConfig {
    /// Returns [`LargeCacheConfig::DEFAULT`] — all knobs at their documented
    /// defaults.
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(feature = "alloc-decommit")]
impl LargeCacheConfig {
    /// The default configuration — all fields `None`, meaning "use the built-in
    /// default for every knob". Equivalent to `LargeCacheConfig::new()`.
    ///
    /// Behaviour when passed to `AllocCore::new_with_config` is byte-identical
    /// to the previous env-var path when no environment variables were set.
    pub const DEFAULT: Self = Self::new();

    /// Construct a config with all knobs at their defaults.
    ///
    /// Chain setter calls to override individual knobs (runnable form in
    /// `tests/large_cache_config_knobs.rs`):
    ///
    /// ```text
    /// use sefer_alloc::{LargeCacheConfig, LargeCacheMode};
    /// let cfg = LargeCacheConfig::new()
    ///     .budget_bytes(512 * 1024 * 1024)
    ///     .mode(LargeCacheMode::Lazy);
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            budget_bytes: None,
            headroom_bytes: None,
            decay_interval_ms: None,
            decay_rate_percent: None,
            mode: None,
            pool: SmallSegmentPoolConfig::DEFAULT,
            depot_budget_bytes: None,
        }
    }

    /// Set the per-shard byte budget (hard ceiling on total cached bytes).
    ///
    /// When the cache would exceed this limit on a new deposit, the oldest
    /// cached slot is FIFO-evicted before admitting the new span. If `bytes`
    /// is smaller than the incoming span's usable size, the span is released
    /// to the OS immediately.
    ///
    /// `0` is a valid, least-surprising finite ceiling: it means "cache
    /// nothing" — every deposit immediately fails the budget check and the
    /// span is released to the OS instead of cached (large-cache disabled).
    /// This is a stored `Some(0)`, distinct from the default `None`. The
    /// default's RESOLVED meaning is feature-dependent: with
    /// `large-cache-extended` OFF, `None` resolves to unbounded (any span
    /// admissible); with `large-cache-extended` ON, an unset budget resolves
    /// to the FINITE `DEFAULT_EXTENDED_BUDGET_BYTES` (see the
    /// `resolved_budget_bytes` doc and R14-5), so under that feature the
    /// cache is always budget-bounded and *unbounded* caching is not
    /// available.
    ///
    /// (Before task #136 this method treated `0` as an alias for `None`
    /// — i.e. "unlimited" — which is the opposite of what `0` intuitively
    /// suggests. That inversion was fixed prior to the first publish of the
    /// `LargeCacheConfig` API, so it is not a breaking change for any
    /// released version.)
    ///
    /// Default: stored `None` — resolves to unbounded only with
    /// `large-cache-extended` OFF (see above for the feature-dependent
    /// resolved meaning).
    #[must_use]
    pub const fn budget_bytes(mut self, bytes: usize) -> Self {
        self.budget_bytes = Some(bytes);
        self
    }

    /// Set the decay headroom target in bytes.
    ///
    /// This is the decay step's trigger/target, NOT a hard floor the cache is
    /// never released below. A tick fires only when cached bytes EXCEED this
    /// level, and then releases `decay_rate_percent` of the excess — but
    /// eviction releases WHOLE FIFO-oldest spans, so a tick can overshoot and
    /// leave the cache BELOW this level, and other paths may take it lower
    /// still (a deposit that fails the budget check evicts to fit; teardown
    /// drains the cache entirely). A higher headroom means the cache retains
    /// more memory between ticks (less aggressive trimming).
    ///
    /// Default: 256 MiB.
    #[must_use]
    pub const fn headroom_bytes(mut self, bytes: usize) -> Self {
        self.headroom_bytes = Some(bytes);
        self
    }

    /// Set the elapsed-time period used to count due decay steps, in
    /// milliseconds, measured with a monotonic clock.
    ///
    /// For a nonzero interval, the first eligible call primes the timer.
    /// Subsequent eligible clock checks process `due = min(elapsed / interval, 8)`
    /// steps inline. While excess remains after the batch, the timer advances
    /// by `due * interval`, retaining fractional and capped whole-interval debt.
    /// A batch returning the cache to headroom instead sets the timer to the
    /// current time and discards debt; full cache trim resets timer and stride.
    /// Catch-up is not a minimum spacing guarantee while excess remains.
    /// Calls at or below headroom still leave accounting unchanged: the first
    /// batch after below-headroom idle can still take up to eight steps
    /// (limiting that first batch is deferred as V5).
    /// See `docs/perf/R34_11_CATCHUP_DECAY_GATE.md` §1 for the accepted design.
    ///
    /// A value of `0` means "tick on every eligible large alloc/free" (useful
    /// for testing; zero ms is accepted): every event that finds the cache
    /// above its headroom floor decays in that same call — the first eligible
    /// event included (no timer-priming pass) — and the clock-read stride
    /// throttle described on [`LargeCacheMode::Lazy`] is bypassed entirely.
    /// Higher values mean fewer due intervals for the same elapsed time;
    /// they do not change the clock-read stride.
    ///
    /// Default: 1000 ms (1 second).
    #[must_use]
    pub const fn decay_interval_ms(mut self, ms: u32) -> Self {
        self.decay_interval_ms = Some(ms);
        self
    }

    /// Set the fraction of the excess to release per tick, as an integer
    /// percent in `[1, 100]`.
    ///
    /// Values below 1 are clamped to 1; values above 100 are clamped to 100
    /// at resolution time.
    ///
    /// - `10` → release 10 % of `(cached − headroom)` per tick (default).
    /// - `100` → flush all excess in a single tick.
    ///
    /// Default: 10 %.
    #[must_use]
    pub const fn decay_rate_percent(mut self, pct: u32) -> Self {
        self.decay_rate_percent = Some(pct);
        self
    }

    /// Set the cache operating mode.
    ///
    /// - `LargeCacheMode::Lazy` (default and currently the only variant):
    ///   event-driven — due decay steps run inline on a large alloc/free.
    ///   To limit clock reads, once the cache is above headroom and the timer
    ///   is primed, the clock is consulted every 64th such event. Each check
    ///   processes up to eight due intervals using the bounded catch-up policy
    ///   described on [`decay_interval_ms`](Self::decay_interval_ms); remaining
    ///   debt can trigger another batch while excess remains without a fresh
    ///   full interval. A batch reaching headroom discards debt; full trim resets
    ///   the timer. A zero `decay_interval_ms` configuration bypasses the stride
    ///   and runs one step on every eligible event, the first one included.
    ///   No background thread; idle processes pay nothing.
    ///
    /// `LargeCacheMode` is `#[non_exhaustive]`, leaving room for a future
    /// background-scavenger mode to be added as a non-breaking change; today
    /// only `Lazy` exists and is constructible.
    ///
    /// Default: `LargeCacheMode::Lazy`.
    #[must_use]
    pub const fn mode(mut self, m: LargeCacheMode) -> Self {
        self.mode = Some(m);
        self
    }

    /// Build a config from a [`Profile`](crate::alloc_core::config::policy::profile::Profile) — a small
    /// builder over two independent, named, measured axes
    /// ([`SmallPoolPolicy`](crate::alloc_core::config::policy::small_pool_policy::SmallPoolPolicy) for the
    /// small-pool pair, [`LargeCachePolicy`](crate::alloc_core::config::policy::large_cache_policy::LargeCachePolicy)
    /// for the large-cache `headroom_bytes`). See [`Profile`](crate::alloc_core::config::policy::profile::Profile)'s
    /// own docs for exactly what each axis sets and the measured gate reports
    /// (R27-3/R27-4/R30-6/R31-1/R31-2) each value is drawn from.
    ///
    /// Equivalent to (illustrative, not a doctest per this project's "no
    /// doctests" rule):
    ///
    /// ```text
    /// use sefer_alloc::{LargeCacheConfig, Profile, SmallPoolPolicy, SmallSegmentPoolConfig};
    ///
    /// let cfg = LargeCacheConfig::new()
    ///     .headroom_bytes(64 * 1024 * 1024)
    ///     .pool(SmallSegmentPoolConfig::new().pool_segments(8).pool_byte_cap(32 * 1024 * 1024));
    /// // is the same resolved config as:
    /// let cfg2 = LargeCacheConfig::for_profile(Profile::new().small_pool(SmallPoolPolicy::Throughput));
    /// ```
    ///
    /// Every other knob (`budget_bytes`, `decay_interval_ms`,
    /// `decay_rate_percent`, `mode`) stays at its own built-in default —
    /// chain further setter calls on the result to override them.
    #[must_use]
    pub const fn for_profile(profile: crate::alloc_core::config::policy::profile::Profile) -> Self {
        profile.to_config()
    }

    /// Set the empty-small-segment hysteresis pool config (Mechanism 2, task
    /// #51).
    ///
    /// Default: [`SmallSegmentPoolConfig::DEFAULT`](crate::alloc_core::config::small_segment_pool_config::SmallSegmentPoolConfig::DEFAULT)
    /// (pool enabled, 4 segments / 16 MiB). Pass
    /// `SmallSegmentPoolConfig::new().pool_segments(0)` to disable pooling
    /// (immediate release of every empty small segment — the pre-Mechanism-2
    /// behaviour).
    #[must_use]
    pub const fn pool(mut self, pool: SmallSegmentPoolConfig) -> Self {
        self.pool = pool;
        self
    }

    /// Set the per-thread byte budget of the magazine-depot overflow (G1-K7).
    ///
    /// Each thread's heap keeps freed small blocks (16..1024 B) in its
    /// magazines and a fixed per-class depot (241,408 B at most). When that
    /// fills, up to `bytes` more of freed blocks stay parked in the depot
    /// overflow instead of going back to their segments, so a later burst of
    /// allocations of the same class is served by copying pointers rather
    /// than walking free lists. Larger values speed up alloc/free cycles over
    /// large cohorts (e.g. 16 384 blocks of 64 B) and hold more memory per
    /// thread between bursts; parked blocks also keep their segments from
    /// being released. Overflow that stays unused decays back over the
    /// heap's next cold events; `trim`, thread exit and slot recycle empty
    /// it. Default 1 MiB; `0` disables the
    /// overflow; values are rounded down to 256 B and capped at
    /// 16,774,912 B. Applied when the thread's heap is first created (a
    /// recycled heap keeps the budget it was created with — see
    /// `config_conflicts_total`). Measurements: README "Depot overflow
    /// budget".
    #[must_use]
    pub const fn depot_budget_bytes(mut self, bytes: usize) -> Self {
        self.depot_budget_bytes = Some(bytes);
        self
    }

    /// TEST-ONLY (G1-K7): the resolved depot-overflow budget of this config.
    #[doc(hidden)]
    #[cfg(feature = "internals")]
    #[must_use]
    pub const fn dbg_resolved_depot_budget(&self) -> usize {
        self.resolved_depot_budget_bytes()
    }

    // ── Resolution helpers (pub(crate)) ──────────────────────────────────────

    /// Resolve the depot-overflow budget (G1-K7): default when unset, then
    /// capped at [`DEPOT_BUDGET_MAX`] and rounded down to
    /// [`DEPOT_BUDGET_GRAIN`].
    #[must_use]
    pub(crate) const fn resolved_depot_budget_bytes(&self) -> usize {
        let b = match self.depot_budget_bytes {
            Some(v) => v,
            None => DEFAULT_DEPOT_BUDGET_BYTES,
        };
        let b = if b > DEPOT_BUDGET_MAX {
            DEPOT_BUDGET_MAX
        } else {
            b
        };
        b / DEPOT_BUDGET_GRAIN * DEPOT_BUDGET_GRAIN
    }

    /// Resolve the byte budget. `None` = unbounded (no admission limit).
    ///
    /// R14-5 (task #290): when `large-cache-extended` is compiled in AND the
    /// caller never called [`budget_bytes`](Self::budget_bytes) (the stored
    /// field is `None`), this resolves to
    /// [`DEFAULT_EXTENDED_BUDGET_BYTES`] rather than `None` — see that
    /// constant's doc for the full rationale (RSS-retention hazard the
    /// extension introduces once slot count alone stops being a meaningful
    /// bound). An explicit `.budget_bytes(n)` call (any `n`, including `0`)
    /// always wins over this fallback, exactly like every other resolver in
    /// this module. Base-cache builds (feature OFF) are unaffected: the
    /// stored `None` resolves to `None` exactly as before this task.
    #[must_use]
    pub(crate) const fn resolved_budget_bytes(&self) -> Option<usize> {
        #[cfg(feature = "large-cache-extended")]
        {
            match self.budget_bytes {
                Some(v) => Some(v),
                None => Some(DEFAULT_EXTENDED_BUDGET_BYTES),
            }
        }
        #[cfg(not(feature = "large-cache-extended"))]
        {
            self.budget_bytes
        }
    }

    /// Resolve the headroom floor in bytes. Falls back to the default (256 MiB)
    /// when unset.
    #[must_use]
    pub(crate) const fn resolved_headroom_bytes(&self) -> usize {
        match self.headroom_bytes {
            Some(v) => v,
            None => DEFAULT_HEADROOM_BYTES,
        }
    }

    /// Resolve the decay interval as a `Duration`. Falls back to the default
    /// (1 000 ms) when unset.
    #[must_use]
    pub(crate) fn resolved_decay_interval(&self) -> core::time::Duration {
        let ms = match self.decay_interval_ms {
            Some(v) => v as u64,
            None => DEFAULT_DECAY_INTERVAL_MS,
        };
        core::time::Duration::from_millis(ms)
    }

    /// Resolve the decay rate as basis points. The stored percent value is
    /// clamped to `[1, 100]` and converted (`pct * 100 = basis points`).
    /// Falls back to the default (10 % → 1000 bp) when unset.
    #[must_use]
    pub(crate) const fn resolved_decay_rate_bp(&self) -> u32 {
        let pct = match self.decay_rate_percent {
            Some(p) => p,
            None => DEFAULT_DECAY_RATE_PERCENT,
        };
        // Clamp to [1, 100].
        let clamped = if pct < 1 {
            1
        } else if pct > 100 {
            100
        } else {
            pct
        };
        clamped * 100 // percent → basis points
    }

    /// Resolve the cache operating mode. Falls back to `Lazy` when unset.
    #[must_use]
    pub(crate) const fn resolved_mode(&self) -> LargeCacheMode {
        match self.mode {
            Some(m) => m,
            None => LargeCacheMode::Lazy,
        }
    }

    /// Resolve the empty-small-segment pool config (Mechanism 2, task #51).
    #[must_use]
    pub(crate) const fn resolved_pool(&self) -> SmallSegmentPoolConfig {
        self.pool
    }
}
