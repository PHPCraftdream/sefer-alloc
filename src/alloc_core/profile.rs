//! [`Profile`] — named, discoverable presets over [`LargeCacheConfig`] +
//! [`SmallSegmentPoolConfig`] (feature = `alloc-decommit`).
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
//! separately track down the large-cache `headroom_bytes` knob. This module
//! turns that tribal knowledge into a genuine, discoverable API surface: one
//! [`Profile`] variant sets BOTH the small-pool pair and the large-cache
//! headroom coherently, from measured data, not guesses.
//!
//! ## What each profile sets, and why (measured, not guessed)
//!
//! - [`Profile::Throughput`]: small pool `(8, 32 MiB)` + large-cache
//!   headroom `64 MiB`. The small-pool pair is R27-4's measured paired
//!   candidate (~22% lower elapsed time, 9→0 decommit calls/run on the
//!   1024 B batch-120 churn-with-teardown workload, at a cost of ~+8
//!   MiB/heap committed retention that does not decay during idle — R27-3).
//!   The 64 MiB headroom is R30-6's measured finding: it ties the 256 MiB
//!   default EXACTLY on hit rate (100.0% at every thread count, on a 48
//!   MiB/burst mixed workload) at ~7x less RSS (~34-37 MiB/heap vs ~238-241
//!   MiB/heap post-drain floor, R29-13) — a genuinely better default for a
//!   throughput-prioritizing profile, not a trade-off at this measured
//!   workload shape.
//! - [`Profile::Rss`]: small pool `(4, 16 MiB)` — the existing
//!   `production` default, already the RSS-conservative end R27-5 §6
//!   recommended keeping — + large-cache headroom `16 MiB`. **Cost,
//!   disclosed, not hidden:** R30-6 measured a real, reproducible
//!   12.5-percentage-point hit-rate loss at 16 MiB vs 64/256 MiB (87.5% vs
//!   100.0%, exact and identical at 1/8/32 threads) on that same 48
//!   MiB/burst workload. A caller choosing this profile is trading that hit
//!   rate for the smallest large-cache floor this project has measured
//!   above the near-zero 0 MiB point (R30-6 §2: 0 MiB and 16 MiB converge
//!   to the SAME 87.5% floor in the measured workload, so 16 MiB is chosen
//!   over 0 MiB — same disclosed cost, no additional cost).
//! - [`Profile::Balanced`]: small pool `(4, 16 MiB)` (the `production`
//!   default — no retention cost above what every existing deployment
//!   already pays) + large-cache headroom `64 MiB` (R30-6's measured
//!   full-hit-rate-parity point). This is the middle ground the measured
//!   data actually supports: it captures the large-cache win at zero
//!   additional small-pool retention cost, without opting into the
//!   small-pool's own measured +8 MiB/heap retention trade — a caller who
//!   also wants the small-pool latency win should choose
//!   [`Profile::Throughput`] instead.
//!
//! None of these profiles change [`LargeCacheConfig::DEFAULT`] /
//! [`SmallSegmentPoolConfig::DEFAULT`] — `SeferAlloc::new()` is byte-for-byte
//! unchanged. A profile is an explicit, named, opt-in alternative
//! constructed via [`LargeCacheConfig::for_profile`] or
//! [`crate::SeferAlloc::with_profile`].

use super::large_cache_config::LargeCacheConfig;
use super::small_segment_pool_config::SmallSegmentPoolConfig;

/// A named, measured configuration preset for [`LargeCacheConfig`] (feature
/// = `alloc-decommit`).
///
/// Each variant sets the small-segment-pool pair
/// (`pool_segments`/`pool_byte_cap`) AND the large-cache `headroom_bytes`
/// together, coherently, from this project's own measured gate reports —
/// see the module docs for the exact numbers and their citations. Consume a
/// variant via [`LargeCacheConfig::for_profile`] or
/// [`crate::SeferAlloc::with_profile`]; both are `const fn`, so a profile
/// can be installed directly in a `#[global_allocator]` `static` initialiser
/// (illustrative, not a doctest per this project's "no doctests" rule):
///
/// ```text
/// use sefer_alloc::{SeferAlloc, Profile};
///
/// #[global_allocator]
/// static GLOBAL: SeferAlloc = SeferAlloc::with_profile(Profile::Throughput);
/// ```
///
/// `#[non_exhaustive]` — a future round may add a profile (e.g. a
/// container/edge-priority preset) without a breaking change; matching
/// [`super::large_cache_mode::LargeCacheMode`]'s own precedent for a
/// small, deliberately-not-closed enum in this config surface.
#[cfg(feature = "alloc-decommit")]
#[non_exhaustive]
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Profile {
    /// Memory-priority: the existing `production` small-pool default `(4
    /// segments, 16 MiB)` + large-cache headroom lowered to `16 MiB`.
    ///
    /// **Disclosed cost:** R30-6 measured a real, reproducible
    /// 12.5-percentage-point large-cache hit-rate loss at 16 MiB headroom
    /// versus 64/256 MiB (87.5% vs 100.0%, exact at 1/8/32 threads, on a 48
    /// MiB/burst mixed small+large workload —
    /// `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §0.1). Choose this
    /// profile only when the smaller large-cache floor matters more than
    /// that measured hit-rate cost.
    Rss,
    /// The middle ground: the `production` small-pool default `(4, 16
    /// MiB)` (no additional small-pool retention cost) + large-cache
    /// headroom `64 MiB` (R30-6's measured full-hit-rate-parity point with
    /// the 256 MiB default, at ~7x less RSS —
    /// `docs/perf/R30_6_LARGE_CACHE_HEADROOM_AB_GATE.md` §0,
    /// `docs/perf/R29_13_LARGE_CACHE_RETENTION_GATE.md` §0).
    Balanced,
    /// Throughput-priority: small pool `(8 segments, 32 MiB)` (R27-3/R27-4's
    /// measured paired candidate — ~22% lower elapsed time, 9→0 decommit
    /// calls/run on the 1024 B batch-120 churn-with-teardown workload, at a
    /// disclosed ~+8 MiB/heap non-decaying retention cost) + large-cache
    /// headroom `64 MiB` (R30-6's full-hit-rate-parity point with the 256
    /// MiB default, at ~7x less RSS).
    Throughput,
}

#[cfg(feature = "alloc-decommit")]
impl Profile {
    /// Small-pool segment count for this profile.
    #[must_use]
    const fn pool_segments(self) -> usize {
        match self {
            Profile::Rss | Profile::Balanced => SmallSegmentPoolConfig::DEFAULT_POOL_SEGMENTS,
            Profile::Throughput => 8,
        }
    }

    /// Small-pool byte cap for this profile.
    #[must_use]
    const fn pool_byte_cap(self) -> usize {
        match self {
            Profile::Rss | Profile::Balanced => SmallSegmentPoolConfig::DEFAULT_POOL_BYTE_CAP,
            Profile::Throughput => 32 * 1024 * 1024,
        }
    }

    /// Large-cache headroom for this profile, in bytes.
    #[must_use]
    const fn headroom_bytes(self) -> usize {
        match self {
            Profile::Rss => 16 * 1024 * 1024,
            Profile::Balanced | Profile::Throughput => 64 * 1024 * 1024,
        }
    }

    /// Resolve this profile into a concrete [`LargeCacheConfig`], setting
    /// the small-pool pair and the large-cache headroom together. Equivalent
    /// to, and used by, [`LargeCacheConfig::for_profile`].
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
