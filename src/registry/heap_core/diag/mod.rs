//! `HeapCore` diagnostics / test-hook submodules (decls only): the inspection
//! + ring-simulation hooks (`queries`) and the promotion/hardened counters,
//!   `contains_base` family, `unsafe` delegation wrappers, and `dbg_decomp_*`
//!   family (`diag_probes`). No re-exports: every item is an inherent
//!   `HeapCore` method, reached via method resolution like the rest of the
//!   `heap_core/` split.

mod diag_probes;
mod queries;
