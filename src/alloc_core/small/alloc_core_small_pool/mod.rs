//! Mechanism-2 empty-small-segment pool + M6 decommit cluster of [`AllocCore`](crate::AllocCore)
//! (mechanical split of `alloc_core.rs`).
//!
//! This module holds an additional `impl AllocCore { .. }` block carrying the
//! empty-small-segment hysteresis pool and the decommit/live-count methods,
//! split across its `alloc_core_small_pool_impl` / `decommit` / `decomp_hooks`
//! siblings, plus the `SegmentStateAccount`/`SegmentStateReconciliation`
//! snapshot types (`segment_state_account.rs` / `segment_state_reconciliation.rs`).
//! It is a pure code-movement sibling of `alloc_core.rs`; no behavior changed.
//! The whole module is `alloc-decommit`-gated because every method here is.

// Mechanical-split siblings of the former flat `alloc_core_small_pool.rs`.
// All keep `cfg` gates here (not only on their items) so their compiled-file
// sets stay identical to when the code lived directly in
// `alloc_core_small_pool.rs`: `alloc_core_small_pool_impl` carries the `impl
// AllocCore` block (empty-segment pool + M6 decommit cluster), `decommit`
// carries the decommit-cluster items (every one `alloc-decommit`-gated),
// `decomp_hooks` the measurement-only decomposition hooks (every one
// `internals` + `alloc-decommit` + `bench-internals`-gated — compiling it
// under any narrower gate would only leave its file-level `use` block
// unused), and `segment_state_account` / `segment_state_reconciliation` the
// `bench-internals`-gated snapshot types.
#[cfg(feature = "alloc-decommit")]
mod alloc_core_small_pool_impl;
#[cfg(feature = "alloc-decommit")]
mod decommit;
#[cfg(all(
    feature = "alloc-decommit",
    feature = "bench-internals",
    feature = "internals"
))]
mod decomp_hooks;
#[cfg(feature = "bench-internals")]
mod segment_state_account;
#[cfg(feature = "bench-internals")]
mod segment_state_reconciliation;

#[cfg(feature = "bench-internals")]
pub use segment_state_account::SegmentStateAccount;
#[cfg(feature = "bench-internals")]
pub use segment_state_reconciliation::SegmentStateReconciliation;
