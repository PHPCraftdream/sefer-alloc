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
/// `#[non_exhaustive]` — a future measured axis point can still be added
/// without a breaking change.
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
    ///
    /// **Second disclosed cost (R32-8, task #499): a per-large-op wall-clock
    /// read.** This policy's whole point — keeping a heap's working set
    /// resident ABOVE this 16 MiB floor during normal operation — puts every
    /// large alloc/free on the "guard fails" side of
    /// `AllocCore::maybe_decay_large_cache`'s internal fast-path guard,
    /// which otherwise skips a `std::time::Instant::now()` read (a
    /// `QueryPerformanceCounter` syscall on Windows) when the cache sits at
    /// or below headroom. Measured, confound-free
    /// (`docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE.md`): **~75-138
    /// ns per `maybe_decay_large_cache` call** (two calls per steady-state
    /// alloc+free cycle) in the raw, unthrottled shape. A structural fix
    /// shipped in the SAME task (a monotonic op-counter that only actually
    /// reads the clock every ~64th call once past headroom) reduces this
    /// specific function's own elapsed contribution by ~62-73 % in the
    /// above-headroom regime this policy targets, at the cost of decay
    /// ticks firing up to ~63 large ops later than before (never earlier,
    /// never more aggressively) — see that report §4 for the exact
    /// trade. The residual cost after the fix is smaller but NOT zero: this
    /// policy still pays materially more clock reads than
    /// [`LargeCachePolicy::Default`] at 256 MiB headroom, whose working set
    /// in most measured workloads never crosses the floor at all.
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
    ///
    /// **Third disclosed cost (R32-8, task #499): the same per-large-op
    /// wall-clock read documented on [`LargeCachePolicy::LowHeadroom`]
    /// above applies here too, and for the identical structural reason** —
    /// any working set that persists above this 64 MiB floor (the exact
    /// regime R31-1 found the hit-rate parity breaks in) is, by the same
    /// token, on the "guard fails" side of
    /// `AllocCore::maybe_decay_large_cache`'s fast-path guard for its
    /// entire above-floor duration. See `LowHeadroom`'s doc immediately
    /// above for the measured magnitude, the structural fix that reduces
    /// (but does not eliminate) it, and the citation
    /// (`docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE.md`) — not
    /// repeated verbatim here to avoid drift between the two copies.
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
    /// **Requires the `large-cache-extended` Cargo feature to do anything —
    /// EXPERIMENTAL, opt-in, not in `production`.** `256 MiB` headroom (same
    /// numeric floor as `Default` above); the behavioural difference this
    /// variant selects for comes entirely from whether the caller's build
    /// also has `large-cache-extended` compiled in, which widens the
    /// large-segment free-cache from 8 to 40 slots (8 base + 32 lazily-
    /// materialised sidecar slots) at COMPILE time. Without that feature
    /// compiled in, this variant currently resolves identically to
    /// `Default` — it exists as a distinct, named value so a caller who has
    /// opted into `large-cache-extended` can express "I want the
    /// diverse-turnover-oriented headroom" without hand-picking a number,
    /// but it does not itself turn the sidecar on (Cargo features cannot be
    /// selected at runtime).
    ///
    /// This is the named opt-in policy R31-3 (task #466) proposed and task
    /// #491 shipped, per an explicit user request that promotion NOT be a
    /// blanket `production`/`Profile::Default` change — see the three
    /// bullets below and `docs/perf/OPEN_ITEMS.md` item 30 for the full
    /// evidence trail before choosing this policy for a workload.
    ///
    /// **Choose this ONLY for a workload with genuinely diverse, repeatedly-
    /// reused Large-object sizes (more than the base cache's 8 slots) —
    /// this is a real, measured trade, not a free upgrade over `Default`:**
    ///
    /// - **Turnover win (the reason to choose this):** on a workload
    ///   cycling through more than 8 repeatedly-reused distinct Large sizes,
    ///   the base 8-slot cache's FIFO eviction thrashes — measured hit rate
    ///   33.3 % (1600/4800) with `large-cache-extended` OFF vs **100 %**
    ///   (4800/4800) ON, a large, reproducible win (paired n=20, `t =
    ///   127.776`, sign 20/20, mean ~385.7 µs/op faster in the measured
    ///   workload). See
    ///   `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` §2
    ///   for full methodology.
    /// - **Narrow-working-set cost (real, measured, NOT free — CLAUDE.md's
    ///   same-regime cost/benefit rule):** on a working set that does NOT
    ///   need the wider cache, the widened O(40) scan bound costs
    ///   something real, not negligible. A real-process A/B at N=1/2/4
    ///   reused sizes found `large-cache-extended` ON measurably,
    ///   reproducibly SLOWER (t = −11.6 / −7.8 / −13.5, all past
    ///   crit = 2.101, clean noise-floor controls); a scan-isolated
    ///   microjudge attributes this to the best-fit scan loop itself
    ///   (5.01× ns/round, 8 vs 40 slots, n=20 paired t = −29.3). Magnitude
    ///   is small in absolute per-operation terms (roughly 100–500 ns per
    ///   alloc+dealloc pair at these N) but real, not noise. See
    ///   `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` §8
    ///   for full methodology.
    /// - **Per-heap RSS retention (real, PER-HEAP, NOT process-wide
    ///   bounded):** the 256 MiB budget enforces at ~248 MiB retained PER
    ///   HEAP in the measured workload (vs ~432 MiB/heap for the
    ///   unbounded-by-default base cache), scaling near-perfectly LINEARLY
    ///   across 1/8/32 concurrently-claimed heaps —
    ///   `docs/perf/R31_3_LARGE_CACHE_EXTENDED_REVERIFICATION_GATE.md` §4.
    ///   **This is a predictable PER-HEAP ceiling, not a safe PROCESS-WIDE
    ///   one**: `AllocCore` is owner-only (neither `Send` nor `Sync`), so a
    ///   thread-per-core server running one heap per thread multiplies this
    ///   default by however many heaps concurrently exercise a large,
    ///   diverse working set under this policy — e.g. 32 such heaps ≈ 32 ×
    ///   248 MiB ≈ **7.75 GiB** of retained committed memory in the
    ///   measured workload shape, with NO cross-heap coordination bounding
    ///   the process-wide total. A process-wide shared budget was
    ///   considered (task #491) and deliberately NOT built: it would be a
    ///   brand-new cross-heap synchronization point on a path that
    ///   currently has none, and its own contention/coordination cost would
    ///   need the same rigor of measurement this crate already requires for
    ///   every other perf claim — a second gate-report-sized undertaking
    ///   this task did not have standing evidence to justify building
    ///   speculatively. If you run many large-working-set heaps
    ///   concurrently under this policy, compute your own worst case as
    ///   `(concurrently-active heap count) × (256 MiB)` and set
    ///   [`LargeCacheConfig::budget_bytes`] explicitly per heap (a smaller
    ///   per-heap cap, or `0` to disable) if that total is more than your
    ///   deployment can afford — this policy does not do that arithmetic
    ///   for you.
    ///
    /// A user choosing this policy is choosing the turnover win AND both
    /// disclosed costs together — not getting a free lunch on any axis.
    DiverseTurnover,
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
            // Same numeric floor as `Default` — see `DiverseTurnover`'s own
            // doc comment for why this variant's real effect comes from the
            // `large-cache-extended` Cargo feature (a compile-time slot-count
            // change `Profile` cannot express), not from a different
            // `headroom_bytes` value.
            LargeCachePolicy::DiverseTurnover => super::large_cache_config::DEFAULT_HEADROOM_BYTES,
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
