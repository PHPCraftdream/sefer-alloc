//! [`LargeCacheConfig`] — a `const`-buildable configuration type for the
//! per-shard large-segment free-cache (feature = `alloc-decommit`).
//!
//! This replaces the environment-variable parsers that used to live in
//! `alloc_core.rs`. The caller constructs a `LargeCacheConfig` value at
//! compile time (or at runtime — the builder is `const` but not *required*
//! to be) and passes it to [`AllocCore::new_with_config`] or
//! [`SeferMalloc::with_config`]. If the caller wants to read from the
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

// `LargeCacheMode` is defined in `alloc_core.rs` (the sibling sub-module
// `alloc_core::alloc_core`) and re-exported by `alloc_core::mod.rs`.
use super::alloc_core::LargeCacheMode;

// ── Default constants (kept in sync with the old env-parser defaults) ─────────

/// Default headroom: 256 MiB. The cache does not decay below this level,
/// providing an anti-thrashing floor.
pub(crate) const DEFAULT_HEADROOM_BYTES: usize = 256 * 1024 * 1024;

/// Default decay interval: 1000 ms (1 second between ticks).
pub(crate) const DEFAULT_DECAY_INTERVAL_MS: u64 = 1000;

/// Default decay rate: 10 % per tick, expressed as a percentage.
pub(crate) const DEFAULT_DECAY_RATE_PERCENT: u32 = 10;

// ── The config type ───────────────────────────────────────────────────────────

/// Compile-time-buildable configuration for the per-shard large-segment
/// free-cache.
///
/// Construct with the [`new`](Self::new) associated function (or the
/// [`DEFAULT`](Self::DEFAULT) constant) and chain the setter methods:
///
/// ```rust
/// # #[cfg(all(feature = "alloc-core", feature = "alloc-decommit", feature = "alloc-global"))]
/// # {
/// use sefer_alloc::{SeferMalloc, LargeCacheConfig, LargeCacheMode};
///
/// const CONFIG: LargeCacheConfig = LargeCacheConfig::new()
///     .budget_bytes(512 * 1024 * 1024)
///     .headroom_bytes(64 * 1024 * 1024)
///     .decay_interval_ms(200)
///     .decay_rate_percent(25)
///     .mode(LargeCacheMode::Lazy);
///
/// #[global_allocator]
/// static GLOBAL: SeferMalloc = SeferMalloc::with_config(CONFIG);
/// # }
/// ```
///
/// Passing `LargeCacheConfig::DEFAULT` (= `LargeCacheConfig::new()`) to
/// [`SeferMalloc::with_config`] produces byte-identical behaviour to
/// [`SeferMalloc::new`].
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, Debug)]
pub struct LargeCacheConfig {
    /// Per-shard hard ceiling on total cached bytes. `None` = unbounded.
    ///
    /// When set, FIFO eviction fires before admitting a new span that would
    /// push the total above this limit. If the limit is smaller than the span
    /// being freed, the span is released to the OS immediately (not cached).
    pub(crate) budget_bytes: Option<usize>,

    /// Anti-thrashing floor: the decay step does **not** release bytes below
    /// this level. `None` uses the default (256 MiB).
    pub(crate) headroom_bytes: Option<usize>,

    /// Minimum wall-clock milliseconds between consecutive decay ticks.
    /// `None` uses the default (1000 ms).
    pub(crate) decay_interval_ms: Option<u32>,

    /// Fraction of the excess to release per tick, in integer percent
    /// `[1, 100]`. `None` uses the default (10 %).
    ///
    /// Values outside `[1, 100]` are clamped at resolution time.
    pub(crate) decay_rate_percent: Option<u32>,

    /// Cache operating mode. `None` uses the default (`Lazy`).
    pub(crate) mode: Option<LargeCacheMode>,
}

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
    /// Chain setter calls to override individual knobs:
    ///
    /// ```rust
    /// # #[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
    /// # {
    /// use sefer_alloc::{LargeCacheConfig, LargeCacheMode};
    /// let cfg = LargeCacheConfig::new()
    ///     .budget_bytes(512 * 1024 * 1024)
    ///     .mode(LargeCacheMode::Lazy);
    /// # let _ = cfg;
    /// # }
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            budget_bytes: None,
            headroom_bytes: None,
            decay_interval_ms: None,
            decay_rate_percent: None,
            mode: None,
        }
    }

    /// Set the per-shard byte budget (hard ceiling on total cached bytes).
    ///
    /// When the cache would exceed this limit on a new deposit, the oldest
    /// cached slot is FIFO-evicted before admitting the new span. If `bytes`
    /// is smaller than the incoming span's usable size, the span is released
    /// to the OS immediately.
    ///
    /// Pass `0` to express "unlimited" — internally treated as `None`
    /// (unbounded). Any non-zero value sets a finite ceiling.
    ///
    /// Default: `None` (unbounded — any span is admissible).
    #[must_use]
    pub const fn budget_bytes(mut self, bytes: usize) -> Self {
        self.budget_bytes = if bytes == 0 { None } else { Some(bytes) };
        self
    }

    /// Set the headroom floor in bytes.
    ///
    /// The decay step does not release bytes below this level. A higher
    /// headroom means the cache retains more memory between ticks (less
    /// aggressive trimming).
    ///
    /// Default: 256 MiB.
    #[must_use]
    pub const fn headroom_bytes(mut self, bytes: usize) -> Self {
        self.headroom_bytes = Some(bytes);
        self
    }

    /// Set the minimum wall-clock interval between decay ticks, in
    /// milliseconds.
    ///
    /// A value of `0` means "tick on every large alloc/free" (useful for
    /// testing; zero ms is accepted). Higher values reduce the frequency of
    /// decay checks and OS calls.
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
    /// - `LargeCacheMode::Lazy` (default): event-driven — a decay tick fires
    ///   inline on the next large alloc/free after the interval has elapsed.
    ///   No background thread; idle processes pay nothing.
    /// - `LargeCacheMode::Background` / `LargeCacheMode::Both`: reserved for
    ///   a future background scavenger thread. Currently behaves identically
    ///   to `Lazy` (the scavenger is not yet implemented).
    ///
    /// Default: `LargeCacheMode::Lazy`.
    #[must_use]
    pub const fn mode(mut self, m: LargeCacheMode) -> Self {
        self.mode = Some(m);
        self
    }

    // ── Resolution helpers (pub(crate)) ──────────────────────────────────────

    /// Resolve the byte budget. `None` = unbounded (no admission limit).
    #[must_use]
    pub(crate) const fn resolved_budget_bytes(&self) -> Option<usize> {
        self.budget_bytes
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
}
