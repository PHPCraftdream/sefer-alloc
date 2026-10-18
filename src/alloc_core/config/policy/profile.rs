//! [`Profile`](crate::Profile) — a small builder over two independent,
//! named, measured axes ([`SmallPoolPolicy`](crate::SmallPoolPolicy) and
//! [`LargeCachePolicy`](crate::LargeCachePolicy)) that together resolve to a
//! [`LargeCacheConfig`](crate::LargeCacheConfig) (feature = `alloc-decommit`).
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
//!    large-cache-headroom choice is now
//!    [`LargeCachePolicy::LowHeadroom`](crate::LargeCachePolicy::LowHeadroom),
//!    whose own doc states plainly that it is a decay floor, not a cap, and
//!    points to [`LargeCacheConfig::budget_bytes`](crate::LargeCacheConfig::budget_bytes)
//!    for an actual admission
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
//!    latency win ([`SmallPoolPolicy::Throughput`](crate::SmallPoolPolicy::Throughput))
//!    no longer has to also
//!    accept the large-cache narrowing as a package deal — and the large
//!    cache's policies
//!    ([`LargeCachePolicy::LowHeadroom`](crate::LargeCachePolicy::LowHeadroom) /
//!    [`LargeCachePolicy::Trimmed64MiB`](crate::LargeCachePolicy::Trimmed64MiB) /
//!    [`LargeCachePolicy::Default`](crate::LargeCachePolicy::Default))
//!    are named and documented for what they measurably cost, not what a
//!    "Throughput"-branded bundle implied.
//!
//! Splitting into two independent axes also fixes the underlying structural
//! problem: bundling meant evidence from ONE workload (small-pool
//! single-thread teardown churn, or large-cache burst/idle) silently set
//! policy for the OTHER, unrelated tier. [`Profile`](crate::Profile) now composes
//! [`SmallPoolPolicy`](crate::SmallPoolPolicy) and
//! [`LargeCachePolicy`](crate::LargeCachePolicy) independently — any of the
//! 2×4 combinations is directly constructible, each axis documents its own
//! citations, and a future measured axis value can be added to ONE axis
//! without touching the other.
//!
//! ## R31-3/task #491: `LargeCachePolicy::DiverseTurnover` — a named,
//! explicitly opt-in policy for `large-cache-extended`
//!
//! R31-3 (task #466) re-verified `large-cache-extended`'s turnover win on
//! current `HEAD` (hit rate 33.3 % → 100 %, `t = 127.776`, sign 20/20) and
//! ALSO closed two open questions in the opposite direction: a real,
//! reproducible narrow-working-set scan cost (task #488, `t` up to −13.5),
//! and a per-heap (not process-wide) RSS retention ceiling that scales
//! LINEARLY with concurrently-active heap count (~248 MiB/heap × N heaps,
//! no cross-heap coordination — `AllocCore` is owner-only, neither `Send`
//! nor `Sync`). Task #491 weighed building a process-wide shared budget to
//! bound the multi-heap total and explicitly declined: it would be a new
//! cross-heap synchronization point on a path that has none today, and its
//! own contention cost would need the same measurement rigor as every other
//! perf claim in this crate — a second gate-report-sized undertaking with
//! no standing evidence yet to justify building speculatively (see
//! `docs/perf/OPEN_ITEMS.md` item 30's `large-cache-extended` sub-thread and
//! `LargeCachePolicy::DiverseTurnover`'s own doc comment for the full
//! evidence trail). Instead, task #491 shipped `DiverseTurnover` as a named,
//! explicitly opt-in axis value that states all three costs/benefits inline
//! — a caller choosing it is choosing a measured trade, not getting a
//! silent default change. **`large-cache-extended` remains OUT of
//! `production`'s feature list and `Profile::DEFAULT` remains
//! `LargeCachePolicy::Default` — this is additive, not a default change.**
//!
//! The low-level [`LargeCacheConfig`](crate::LargeCacheConfig) /
//! [`SmallSegmentPoolConfig`](crate::SmallSegmentPoolConfig) builders
//! remain the full-control escape hatch — [`Profile`](crate::Profile) is a convenience
//! layer over them, not a replacement: `LargeCacheConfig::new().headroom_bytes(n)…`
//! and `SmallSegmentPoolConfig::new().pool_segments(n)…` are always
//! available for exact manual tuning outside the named axis values.
use super::large_cache_policy::LargeCachePolicy;
use super::small_pool_policy::SmallPoolPolicy;
use crate::alloc_core::config::large_cache_config::LargeCacheConfig;
use crate::alloc_core::config::small_segment_pool_config::SmallSegmentPoolConfig;

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
            LargeCachePolicy::Default => {
                crate::alloc_core::config::large_cache_config::DEFAULT_HEADROOM_BYTES
            }
            // Same numeric floor as `Default` — see `DiverseTurnover`'s own
            // doc comment for why this variant's real effect comes from the
            // `large-cache-extended` Cargo feature (a compile-time slot-count
            // change `Profile` cannot express), not from a different
            // `headroom_bytes` value.
            LargeCachePolicy::DiverseTurnover => {
                crate::alloc_core::config::large_cache_config::DEFAULT_HEADROOM_BYTES
            }
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
