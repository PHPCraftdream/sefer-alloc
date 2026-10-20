//! Bench/diagnostic static counters of the `HeapCore` free path
//! (R1-01 magazine byte-budget investigation). Mirrors the B4_* bank in
//! `alloc_core::alloc_core::counters`: statics are defined here, incremented
//! at their call sites (feature-gated there), and re-exported via the parent
//! `diag/mod.rs` for cross-module consumers and `dbg_*` exposure.

use core::sync::atomic::AtomicU64;

/// R1-01 (P1-box / item 164) path-activation oracle: number of frees that
/// hit the `cnt == cap < TCACHE_CAP` branch of the small-free tcache route —
/// i.e. frees where the per-refill byte budget (`FREE_PARK_CAP[c]`) is
/// already exhausted and the O(1) swap-top policy evicts the magazine's top
/// block to the substrate instead of parking the current block.
///
/// Process-global (all heaps/threads), monotone within a run. Used by
/// `tests/support/r8_global_box_witness.rs::r1_01_cap_frame` as an oracle
/// that the witness scenario actually exercised this branch (the scenario's
/// earlier phase performs at most 8 pushes, so the branch cannot fire before
/// the 9th drop).
///
/// Diagnostic only: `AtomicU64` + `Relaxed` (no ordering guarantees needed —
/// the counter is never used for synchronization, only observed for
/// "did the path execute at all"). Gated behind `bench-internals` so the
/// release build pays nothing; the increment lives under the same gate at
/// the call site in `dealloc_own_base.rs`.
#[cfg(feature = "bench-internals")]
pub(crate) static R1_01_CAP_ROUTE_FREES: AtomicU64 = AtomicU64::new(0);
