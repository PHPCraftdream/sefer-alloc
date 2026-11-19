//! [`SegmentStateAccount`] — per-state segment accounting record used by
//! [`SegmentStateReconciliation`](super::SegmentStateReconciliation)
//! (mechanical split of the former flat `alloc_core_small_pool.rs`; pure code
//! movement, no behavior changed).
//!
//! R29-4 (task #435) — segment-state reconciliation snapshot types.
//!
//! `SegmentStateAccount` and `SegmentStateReconciliation` are plain-data
//! containers returned by `dbg_segment_state_reconciliation`. Defined here
//! (the pool/decommit cluster) because the method that populates them lives in
//! this file's `impl AllocCore` block; re-exported via `alloc_core::mod.rs` so
//! `examples/` and `tests/` can name the return type. `#[doc(hidden)]` —
//! measurement-only, not stable public API.

/// R29-4 MEASUREMENT-ONLY: per-state accounting for a heap's registered
/// segments (count + committed/reserved bytes).
///
/// Gated `bench-internals`: the only consumer is
/// `dbg_segment_state_reconciliation`, itself gated `alloc-decommit +
/// bench-internals` — an ungated definition here is `dead_code` under plain
/// `cargo clippy --features production -- -D warnings` (caught in the R29
/// readonly review, not by this task's own narrower verification, mirroring
/// R29-5's identical promotion-counter gap fixed in the same round).
#[doc(hidden)]
#[cfg(feature = "bench-internals")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SegmentStateAccount {
    /// Number of registered segments classified into this state.
    pub count: usize,
    /// The OS COMMIT CHARGE implied by each segment's frontier/backend
    /// contract (metadata + committed payload, in virtual bytes
    /// committed) — NOT a measured RSS figure.
    pub committed_bytes: u64,
    /// Total virtual-address reservation bytes for segments in this state.
    pub reserved_bytes: u64,
}
