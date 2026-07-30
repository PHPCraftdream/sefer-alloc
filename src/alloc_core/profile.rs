//! [`Profile`] — a small builder over two independent, named, measured axes
//! ([`SmallPoolPolicy`] and [`LargeCachePolicy`]) that together resolve to a
//! [`LargeCacheConfig`] (feature = `alloc-decommit`).
//!
//! ## Why this exists
//!
//! R27-3/R27-4 (small-pool retention/latency) and R30-6 (large-cache
//! headroom hit-rate/latency) each measured a genuine latency-vs-RSS trade
//! on one knob pair. Both trades were previously reachable only as
//! hand-assembled builder recipes buried in README prose (R29-2, task
//! #433) — a caller had to know both knobs existed, know they must move
//! together (the R27-1 no-op trap, `docs/perf/OPEN_ITEMS_ARCHIVE.md`'s
//! 2026-07-28 R27-1 note: `pool_segments` alone is silently clamped by
//! `pool_byte_cap` via `min(pool_segments, pool_byte_cap / SEGMENT)`), and
//! separately track down the large-cache `headroom_bytes` knob.
//!
//! ## R31-9 (task #473): why this is two axes, not three bundled presets
//!
//! The original R30-7 (task #456) shape shipped one flat, closed enum —
//! `Profile::{Rss, Balanced, Throughput}` — that bundled the small-pool
//! choice and the large-cache choice into a single named value per
//! combination actually measured. Two defects were found in that shape:
//!
//! 1. **`Profile::Rss` did not bound RSS.** `headroom_bytes` is an eventual
//!    decay FLOOR, not an admission limit — `budget_bytes` stays `None`
//!    (unbounded) unless set explicitly, decay is event-driven, and idle
//!    alone never reclaims (R29-13: exactly 0 KiB reclaimed across 36/36
//!    idle-window arms). A burst could leave far more than `headroom_bytes`
//!    resident per heap indefinitely, yet the variant's own name promised an
//!    "RSS" outcome. Fixed by dropping the `Rss` name entirely: the low
//!    large-cache-headroom choice is now [`LargeCachePolicy::LowHeadroom`],
//!    whose own doc states plainly that it is a decay floor, not a cap, and
//!    points to [`LargeCacheConfig::budget_bytes`] for an actual admission
//!    ceiling.
//! 2. **The old `Profile::Throughput` silently lowered the large-cache
//!    window from 256 MiB to 64 MiB with no same-regime evidence** — at the
//!    time it shipped, R30-6's only benefit-side measurement was AT the 64
//!    MiB boundary (a burst that rounds to exactly 64 MiB), not beyond it.
//!    R31-1 (task #464) has since measured beyond that boundary and found a
//!    real, reproducible 12.5-percentage-point large-cache hit-rate cost
//!    (87.5 % vs 100.0 %) once burst occupancy genuinely exceeds 64 MiB —
//!    the exact regime a workload literally named "throughput" is likely to
//!    hit. Splitting the axes means a caller who wants ONLY the small-pool
//!    latency win ([`SmallPoolPolicy::Throughput`]) no longer has to also
//!    accept the large-cache narrowing as a package deal — and the large
//!    cache's policies ([`LargeCachePolicy::LowHeadroom`] /
//!    [`LargeCachePolicy::Trimmed64MiB`] / [`LargeCachePolicy::Default`])
//!    are named and documented for what they measurably cost, not what a
//!    "Throughput"-branded bundle implied.
//!
//! Splitting into two independent axes also fixes the underlying structural
//! problem: bundling meant evidence from ONE workload (small-pool
//! single-thread teardown churn, or large-cache burst/idle) silently set
//! policy for the OTHER, unrelated tier. [`Profile`] now composes
//! [`SmallPoolPolicy`] and [`LargeCachePolicy`] independently — any of the
//! 2×3 combinations is directly constructible, each axis documents its own
//! citations, and a future measured axis value (see
//! [`LargeCachePolicy`]'s doc for the reserved, not-yet-implemented
//! `large-cache-extended` slot) can be added to ONE axis without touching
//! the other.
//!
//! The low-level [`LargeCacheConfig`] / [`SmallSegmentPoolConfig`] builders
//! remain the full-control escape hatch — [`Profile`] is a convenience
//! layer over them, not a replacement: `LargeCacheConfig::new().headroom_bytes(n)…`
//! and `SmallSegmentPoolConfig::new().pool_segments(n)…` are always
//! available for exact manual tuning outside the named axis values.

use super::large_cache_config::LargeCacheConfig;
use super::small_segment_pool_config::SmallSegmentPoolConfig;

/// The small-segment-pool axis: how aggressively to retain empty small
/// segments (`pool_segments` / `pool_byte_cap`) in exchange for avoiding
/// repeated OS reserve/release cycles under segment-boundary churn.
///
/// `#[non_exhaustive]` — a future round may add another measured small-pool
/// point without a breaking change.
#[cfg(feature = "alloc-decommit")]
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum SmallPoolPolicy {
    /// The `production` default: `(4 segments, 16 MiB)`. No additional
    /// small-pool retention cost above what every existing deployment
    /// already pays.
    Default,
    /// `(8 segments, 32 MiB)` — R27-3/R27-4's measured paired candidate.
    ///
    /// **Measured win, on a specific workload shape.** R27-4's single-
    /// threaded, single-shot 1024 B batch-120 churn-with-teardown workload
    /// showed ~22 % lower elapsed time and 9→0 decommit syscalls/run
    /// (`docs/perf/R27_4_REAL_DEFAULT_AB_GATE.md`), at a disclosed ~+8
    /// MiB/heap non-decaying committed-retention cost
    /// (`docs/perf/R27_3_POOL_RETENTION_GATE.md`).
    ///
    /// **Does not generalize to every concurrent shape.** R31-2 (task
    /// #465) swept this exact cap (and 16, and 32) through an 8-thread,
    /// mixed-object-size, continuous-churn server-shaped workload and found
    /// **no reproducible mechanism change at any cap up to 32**
    /// (`decommit_calls_total` bit-identical — 40 — across every cap and
    /// every one of 320 process launches; wall-clock showed no
    /// statistically significant difference at a tight ≈4-5 % minimum-
    /// detectable-effect — `docs/perf/R31_2_POOL_CAP_THRESHOLD_SWEEP_GATE.md`).
    /// This does not invalidate R27-4's original single-threaded finding —
    /// it is a different workload shape — but the win here is
    /// workload-shape-dependent: it applies where segment-boundary churn on
    /// a single heap is the actual bottleneck, not universally to any
    /// multi-threaded server workload.
    Throughput,
}

/// The large-object-cache axis: the `headroom_bytes` anti-thrashing decay
/// floor for the per-shard large-segment free-cache.
///
/// **Not an RSS bound.** Every value here is a decay FLOOR, not an
/// admission ceiling: `budget_bytes` stays unbounded unless set explicitly
/// via [`LargeCacheConfig::budget_bytes`], decay is event-driven (no
/// background thread), and idle time alone never reclaims below this floor
/// (R29-13 measured exactly 0 KiB reclaimed across 36/36 idle-window arms,
/// every headroom value including the 256 MiB default). A burst can leave
/// far more than any of these floors resident per heap indefinitely. If you
/// need an actual RSS cap, use [`LargeCacheConfig::budget_bytes`] — these
/// values only change how low the cache decays back down to once a decay
/// tick eventually fires.
///
/// `#[non_exhaustive]` — reserved for a future `large-cache-extended`-backed
/// policy (a wider-window, diverse-turnover-oriented preset): R31-3 (task
/// #466) proposed but has NOT been accepted (pending explicit user
/// sign-off) promoting `large-cache-extended` to a shipped default axis
/// value. When/if that proposal is accepted, it slots in here as a new
/// variant without touching [`SmallPoolPolicy`] or breaking existing
/// callers of this enum — not added yet, and not constructible today.
#[cfg(feature = "alloc-decommit")]
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum LargeCachePolicy {
    /// `16 MiB` headroom — the smallest large-cache floor this project has
    /// measured above the near-zero 0 MiB point (R30-6 §2: 0 MiB and 16 MiB
    /// converge to the SAME hit-rate floor in the measured workload, so 16
    /// MiB is chosen over 0 MiB — same disclosed cost, no additional cost).
    ///
    /// **Disclosed cost:** R30-6 measured a real, reproducible
    /// 12.5-percentage-point large-cache hit-rate loss at 16 MiB headroom
    /// versus 64/256 MiB (87.5 % vs 100.0 %, exact at 1/8/32 threads —
    /// `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §0.1). Choose this
    /// policy only when the smaller decay floor matters more than that
    /// measured hit-rate cost — and, per this type's own doc, only after
    /// confirming you don't actually need [`LargeCacheConfig::budget_bytes`]
    /// instead (this is a floor, not a cap).
    LowHeadroom,
    /// `64 MiB` headroom — R30-6's measured parity point, a genuine
    /// reduction from the `production` default's 256 MiB (NOT the same as
    /// [`LargeCachePolicy::Default`] below — this is the smaller, opt-in
    /// value).
    ///
    /// **Measured parity, at a specific working-set regime — not general
    /// throughput equivalence.** R30-6 found 64 MiB ties the 256 MiB
    /// default's 100.0 % hit rate EXACTLY at a burst that rounds to a 64
    /// MiB working set (R31-12, task #476: R30-6's own "48 MiB/burst"
    /// workload rounds up to whole 4 MiB `SEGMENT`s, landing exactly on the
    /// 64 MiB boundary), at ~7× less RSS (~34-37 MiB/heap vs ~238-241
    /// MiB/heap post-drain floor — R29-13). **R31-1 (task #464) has since
    /// measured BEYOND that boundary (128 MiB and 288 MiB bursts) and found
    /// the tie BREAKS: 64 MiB headroom costs the same real, reproducible
    /// 12.5-percentage-point hit-rate loss (87.5 % vs 100.0 %) that 16 MiB
    /// pays, exact and identical at every thread count and both
    /// crossing-regime sizes tested.** This policy is therefore a real
    /// trade, not a free win, for any working set that genuinely exceeds 64
    /// MiB of concurrently-live large-object occupancy per heap — choose it
    /// knowing that regime boundary, not as a blanket throughput default.
    Trimmed64MiB,
    /// The `production` default: `256 MiB` headroom
    /// ([`LargeCacheConfig::DEFAULT`]'s own value, unchanged). Selecting
    /// this on the large-cache axis while opting into
    /// [`SmallPoolPolicy::Throughput`] on the small-pool axis is exactly how
    /// defect #2 (R31-9, task #473) is fixed: the two axes are independent,
    /// so choosing a small-pool win no longer silently narrows the
    /// large-cache window as a package deal the way the old bundled
    /// `Profile::Throughput` enum variant did.
    Default,
}

/// A small builder composing the two independent, named, measured
/// configuration axes: [`SmallPoolPolicy`] (small-segment-pool retention)
/// and [`LargeCachePolicy`] (large-cache headroom).
///
/// Construct with [`Profile::new`] (equivalent to
/// [`Profile::DEFAULT`] — both axes at `Default`, i.e. byte-identical to
/// [`crate::SeferAlloc::new`]) and chain [`small_pool`](Self::small_pool) /
/// [`large_cache`](Self::large_cache) to opt into a measured, named
/// alternative on either axis independently. Consume via
/// [`LargeCacheConfig::for_profile`] or [`crate::SeferAlloc::with_profile`];
/// both are `const fn`, so a profile can be installed directly in a
/// `#[global_allocator]` `static` initialiser (illustrative, not a doctest
/// per this project's "no doctests" rule):
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
/// For exact manual control beyond these named axis points, the low-level
/// builders remain fully available: [`LargeCacheConfig::new`] +
/// [`SmallSegmentPoolConfig::new`] (via [`LargeCacheConfig::pool`]) —
/// `Profile` is a convenience layer over them, not a replacement.
#[cfg(feature = "alloc-decommit")]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Profile {
    small_pool: SmallPoolPolicy,
    large_cache: LargeCachePolicy,
}

#[cfg(feature = "alloc-decommit")]
impl Default for Profile {
    /// Returns [`Profile::DEFAULT`].
    fn default() -> Self {
        Self::DEFAULT
    }
}

#[cfg(feature = "alloc-decommit")]
impl Profile {
    /// Both axes at `Default` — byte-identical to [`crate::SeferAlloc::new`].
    /// Equivalent to [`Profile::new`].
    pub const DEFAULT: Self = Self::new();

    /// Construct a profile with both axes at their `production` defaults
    /// (`SmallPoolPolicy::Default` + `LargeCachePolicy::Default`, i.e. `(4,
    /// 16 MiB)` small pool + `256 MiB` large-cache headroom — the SAME
    /// defaults [`crate::SeferAlloc::new`] already uses). Chain
    /// [`small_pool`](Self::small_pool) / [`large_cache`](Self::large_cache)
    /// to opt into a named alternative on either axis.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            small_pool: SmallPoolPolicy::Default,
            large_cache: LargeCachePolicy::Default,
        }
    }

    /// Set the small-segment-pool axis. See [`SmallPoolPolicy`] for what
    /// each value sets and the measured gate reports it is drawn from.
    ///
    /// Default: `SmallPoolPolicy::Default`.
    #[must_use]
    pub const fn small_pool(mut self, policy: SmallPoolPolicy) -> Self {
        self.small_pool = policy;
        self
    }

    /// Set the large-cache-headroom axis. See [`LargeCachePolicy`] for what
    /// each value sets, the measured gate reports it is drawn from, and —
    /// importantly — why this is a decay floor, not an RSS bound.
    ///
    /// Default: `LargeCachePolicy::Default`.
    #[must_use]
    pub const fn large_cache(mut self, policy: LargeCachePolicy) -> Self {
        self.large_cache = policy;
        self
    }

    /// Small-pool segment count for the currently-set [`SmallPoolPolicy`].
    #[must_use]
    const fn pool_segments(self) -> usize {
        match self.small_pool {
            SmallPoolPolicy::Default => SmallSegmentPoolConfig::DEFAULT_POOL_SEGMENTS,
            SmallPoolPolicy::Throughput => 8,
        }
    }

    /// Small-pool byte cap for the currently-set [`SmallPoolPolicy`].
    #[must_use]
    const fn pool_byte_cap(self) -> usize {
        match self.small_pool {
            SmallPoolPolicy::Default => SmallSegmentPoolConfig::DEFAULT_POOL_BYTE_CAP,
            SmallPoolPolicy::Throughput => 32 * 1024 * 1024,
        }
    }

    /// Large-cache headroom, in bytes, for the currently-set
    /// [`LargeCachePolicy`].
    #[must_use]
    const fn headroom_bytes(self) -> usize {
        match self.large_cache {
            LargeCachePolicy::LowHeadroom => 16 * 1024 * 1024,
            LargeCachePolicy::Trimmed64MiB => 64 * 1024 * 1024,
            LargeCachePolicy::Default => super::large_cache_config::DEFAULT_HEADROOM_BYTES,
        }
    }

    /// Resolve this profile into a concrete [`LargeCacheConfig`], setting
    /// the small-pool pair and the large-cache headroom from each axis's
    /// currently-set policy. Equivalent to, and used by,
    /// [`LargeCacheConfig::for_profile`].
    #[must_use]
    pub(crate) const fn to_config(self) -> LargeCacheConfig {
        LargeCacheConfig::new()
            .headroom_bytes(self.headroom_bytes())
            .pool(
                SmallSegmentPoolConfig::new()
                    .pool_segments(self.pool_segments())
                    .pool_byte_cap(self.pool_byte_cap()),
            )
    }
}
