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
pub(crate) use counters::{
    DEPOT_BIN_INGRESS_OWN_BATCH_BLOCKS, DEPOT_BIN_INGRESS_OWN_BATCH_EVENTS,
    DEPOT_BIN_INGRESS_OWN_OVERFLOW_BLOCKS, DEPOT_BIN_INGRESS_OWN_OVERFLOW_EVENTS,
    DEPOT_BIN_INGRESS_REMOTE_BLOCKS, DEPOT_BIN_INGRESS_REMOTE_EVENTS,
    DEPOT_BIN_INGRESS_SUBSTRATE_PUSHES, DEPOT_FLUSH_SWAP_TOP_BLOCKS, DEPOT_FLUSH_SWAP_TOP_EVENTS,
    DEPOT_FLUSH_TEARDOWN_BLOCKS, DEPOT_FLUSH_TEARDOWN_EVENTS, DEPOT_REFILL_BLOCKS_CARVED,
    DEPOT_REFILL_BLOCKS_DRAINED, DEPOT_REFILL_BLOCKS_TOTAL, DEPOT_REFILL_CARVE_EVENTS,
    DEPOT_REFILL_EVENTS, DEPOT_SHADOW_FLUSH_BLOCKS_ABSORBED, DEPOT_SHADOW_FLUSH_BLOCKS_REAL,
    DEPOT_SHADOW_FLUSH_EVENTS_FULLY_ABSORBED, DEPOT_SHADOW_INVALIDATION_EVENTS,
    DEPOT_SHADOW_REFILL_BLOCKS_DRAIN, DEPOT_SHADOW_REFILL_BLOCKS_FROM_DEPOT,
    DEPOT_SHADOW_REFILL_FULLY_FROM_DEPOT, DEPOT_SHADOW_UNMAPPED_EVENTS, R1_01_CAP_ROUTE_FREES,
};
