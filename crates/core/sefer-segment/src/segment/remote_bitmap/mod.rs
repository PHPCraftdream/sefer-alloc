mod bitmap_cut;
mod bitmap_record;
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
#[doc(hidden)]
pub use prf01_sidecar_oracle::Prf01SidecarOracle;
mod sidecar_bitmap;
// Atomic seam: the included-source Loom harness (tests/loom_sidecar_bitmap_actual.rs)
// substitutes loom atomics by re-exporting these names from its shim module;
// `leaf_classes` consumes them one module down, through `sidecar_bitmap`.
pub(crate) use bitmap_cut::BitmapCut;
pub(crate) use bitmap_record::BitmapRecord;
// Measurement seam (R18 sidecar scan bench): the routed-scan counters are
// reached from `sefer-alloc-core` (`find_segment/scan.rs`) and the root
// crate (`heap_core::diag::queries`) only under the measurement cfg, so the
// export is public — and `doc(hidden)` — exactly in that cfg, `pub(crate)`
// otherwise (mirrors the `Prf01SidecarOracle` row above).
#[cfg(all(feature = "bench-internals", r18_sidecar_scan_bench))]
#[doc(hidden)]
pub use bitmap_scan::BitmapScan;
#[cfg(not(all(feature = "bench-internals", r18_sidecar_scan_bench)))]
pub(crate) use bitmap_scan::BitmapScan;
pub(crate) use core::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};
#[cfg_attr(not(feature = "alloc-global"), allow(unused_imports))]
pub(crate) use sidecar_bitmap::*;
