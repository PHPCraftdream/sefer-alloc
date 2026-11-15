//! `SmallPoolPolicy` — the small-segment-pool configuration-policy axis.
//! The full rationale and measured gate-report citations live in
//! [`policy::profile`](super::profile).

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
