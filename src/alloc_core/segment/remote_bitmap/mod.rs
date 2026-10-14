#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
mod prf01_sidecar_oracle;
#[cfg(all(feature = "bench-internals", r19_sidecar_summary_oracle))]
pub(crate) use prf01_sidecar_oracle::Prf01SidecarOracle;
mod bitmap_cut;
mod bitmap_record;
mod bitmap_scan;
mod sidecar_bitmap;
// Atomic seam: the included-source Loom harness (tests/loom_sidecar_bitmap_actual.rs)
// substitutes loom atomics by re-exporting these names from its shim module.
pub(crate) use bitmap_cut::BitmapCut;
pub(crate) use bitmap_record::BitmapRecord;
pub(crate) use bitmap_scan::BitmapScan;
pub(crate) use core::sync::atomic::{AtomicU64, AtomicU8, Ordering};
#[cfg_attr(not(feature = "alloc-global"), allow(unused_imports))]
pub(crate) use sidecar_bitmap::*;
