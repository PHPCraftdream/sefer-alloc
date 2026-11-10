//! `large_thp_runtime` — the resolved large-THP tuple, its packed word form,
//! the pure advice geometry, the budget ledger, and (under the decommit,
//! non-NUMA mechanism gate) its publication cell and process statics.

pub mod thp_advice;
#[path = "state/thp_advice_outcome.rs"]
pub mod thp_advice_outcome;
pub mod thp_budget_ledger;
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
#[path = "state/thp_learning_cell.rs"]
pub(crate) mod thp_learning_cell;
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) mod thp_runtime;
pub mod thp_tuple;
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) mod thp_tuple_cell;

// Consumed in-crate by the mechanism-gated fresh-path advice (ThpAdvice/
// ThpAdviceOutcome/ThpRuntime) and the config resolvers (ThpTuple);
// `ThpBudgetLedger` is re-exported for the internals test surface.
pub use thp_advice::ThpAdvice;
pub use thp_advice_outcome::ThpAdviceOutcome;
// Re-exported for the internals test surface only; no in-crate consumer uses
// this re-export path.
#[allow(unused_imports)]
pub use thp_budget_ledger::ThpBudgetLedger;
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) use thp_runtime::ThpRuntime;
pub use thp_tuple::ThpTuple;

// Atomics through `super` so the actual-source concurrency model can bind.
// Ungated: the ungated ledger module takes its `AtomicU64`/`Ordering` here.
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) use core::sync::atomic::AtomicU8;
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};
