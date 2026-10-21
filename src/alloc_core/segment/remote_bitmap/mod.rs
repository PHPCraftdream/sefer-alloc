mod bitmap_cut;
mod bitmap_scan;
// Source-include tests compile this module without the drain that uses it.
#[cfg(feature = "bench-internals")]
// R6-T16/CQ-9: the strict counters are live under `bench-internals`
// (`sidecar_drain.rs`); the summary-oracle readers are used only with
// `--cfg r19_sidecar_summary_oracle`, so the allow is scoped to its absence.
#[cfg_attr(not(r19_sidecar_summary_oracle), allow(dead_code))]
mod prf01_sidecar_oracle;
#[cfg(feature = "bench-internals")]
#[allow(unused_imports)]
pub(crate) use prf01_sidecar_oracle::Prf01SidecarOracle;
mod sidecar_bitmap;
// Atomic seam: the included-source Loom harness (tests/loom_sidecar_bitmap_actual.rs)
// substitutes loom atomics by re-exporting these names from its shim module;
// `sidecar_bitmap` consumes them from here.
pub(crate) use bitmap_cut::BitmapCut;
pub(crate) use bitmap_scan::BitmapScan;
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};
#[cfg_attr(not(feature = "alloc-global"), allow(unused_imports))]
pub(crate) use sidecar_bitmap::*;
