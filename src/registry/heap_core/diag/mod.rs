//! `HeapCore` diagnostics / test-hook submodules (decls only): the inspection
//! + sidecar-inspection hooks (`queries`) and the promotion/hardened counters,
//!   `contains_base` family, `unsafe` delegation wrappers, and `dbg_decomp_*`
//!   family (`diag_probes`), and the free-path R1-01 oracle counter bank
//!   (`counters`). `counters` is the one re-exported item: its statics are
//!   incremented from sibling free-path modules, which cannot reach an
//!   inherent method.

#[cfg(feature = "bench-internals")]
mod counters;
mod diag_probes;
mod queries;

#[cfg(feature = "bench-internals")]
pub(crate) use counters::R1_01_CAP_ROUTE_FREES;
