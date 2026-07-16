//! Phase 8 — the segment substrate + self-hosted metadata (the Membrane
//! Inversion), behind the `alloc-core` feature.
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).
//! The confined-`unsafe` seams are `os` and `node`, plus `numa` (a third,
//! feature-gated under `numa-aware`); every other file is pure safe code that
//! composes them.

// The file `alloc_core.rs` carries the same name as this module per the
// crate's one-export-per-file convention; silence clippy's module_inception.
pub(crate) mod alloc_bitmap;
#[allow(clippy::module_inception)]
mod alloc_core;
mod alloc_core_core_diag;
mod alloc_core_large;
#[cfg(feature = "alloc-decommit")]
mod alloc_core_large_cache;
mod alloc_core_small;
mod alloc_core_small_diag;
mod alloc_core_small_magazine;
#[cfg(feature = "alloc-decommit")]
mod alloc_core_small_pool;
mod alloc_core_small_reclaim;
mod bootstrap;
/// The cross-thread deferred-free Treiber stack for Large/huge segments
/// (task A1, extracted for #132). Used by the allocator face
/// (`registry::heap_core::HeapCore`) and any direct `AllocCore` user so the
/// double-push-guarded push/drain logic is not duplicated.
///
/// `pub` (not `pub(crate)`) only because `alloc_core` itself is
/// `#[doc(hidden)]` (see `lib.rs`): `DBG_LARGE_XTHREAD_RECLAIMED` is
/// re-exported (via `registry`) as a `#[doc(hidden)]` test-only diagnostic.
#[doc(hidden)]
pub mod deferred_large;
/// R7-A0 diagnostic counters for the per-class segment directory
/// (observability phase). Storage is always compiled; per-event increments
/// are gated behind `alloc-stats`. See the module doc for the counter
/// inventory.
pub(crate) mod directory_stats;
#[cfg(feature = "alloc-decommit")]
pub mod large_cache_config;
#[cfg(feature = "alloc-decommit")]
pub mod large_cache_mode;
/// RAD-5 (plan Phase 5-E4), verdict GO — the second orthogonal per-segment
/// bitmap (magazine residency), wired into the production hot path. See the
/// module doc for the design and `docs/perf/IAI_BASELINE.md` §RAD-5 for the
/// measurement.
pub(crate) mod magazine_bitmap;
pub(crate) mod node;
/// NUMA OS-seam: NUMA-node detection and segment binding.
/// `pub` (not `pub(crate)`) only because `alloc_core` itself is
/// `#[doc(hidden)]` (see `lib.rs`): the public surface is test-only (the
/// `#[doc(hidden)]` re-export), reachable by the isolated NUMA unit test.
/// Nothing here is stable public API.
#[cfg(feature = "numa-aware")]
#[doc(hidden)]
pub mod numa;
pub(crate) mod os;
/// The per-segment non-intrusive cross-thread-free MPSC ring. Compiled in
/// unconditionally so the segment `Layout` (`segment_header::Layout`, which
/// always reserves the ring's bytes to keep the byte layout uniform across
/// feature configs) can reference `FOOTPRINT`; the `push`/`drain` methods are
/// the only `alloc-xthread`-gated surface.
///
/// `pub` (not `pub(crate)`) only because `alloc_core` itself is
/// `#[doc(hidden)]` (see `lib.rs`): the public surface is test-only (the
/// `#[doc(hidden)] pub` methods on `RemoteFreeRing`), reachable by the
/// isolated ring unit test. Nothing here is stable public API.
#[doc(hidden)]
pub mod remote_free_ring;
/// The shared per-segment bitmap *mechanism* (the bit-test/set/clear
/// arithmetic + `FOOTPRINT`) common to [`alloc_bitmap::AllocBitmap`] and
/// [`magazine_bitmap::MagazineBitmap`]; task #98 / R4-6 dedup of
/// `code_quality_review.md` finding #7. `pub(crate)` only because
/// `alloc_core` itself is `#[doc(hidden)]`; the type is `pub(super)` so neither
/// wrapper nor any other crate code can confuse the two bitmap KINDS at a call
/// site. Nothing here is stable public API.
pub(crate) mod segment_bitmap;
/// The per-segment metadata layout + field-specific header accessors + (X7 Ф1)
/// the generation-table byte-level accessors. `pub` (not `pub(crate)`) only
/// because `alloc_core` itself is `#[doc(hidden)]` (see `lib.rs`): the public
/// surface is test-only (the `#[doc(hidden)] pub` gen-table accessors
/// `gen_at`/`bump_gen`/`GEN_TABLE_FOOTPRINT`/`Layout::gen_table_off`), reachable
/// by the isolated gen-table layout test. Nothing here is stable public API.
#[doc(hidden)]
pub mod segment_header;
/// X7 Ф1 (task #189) generation-table byte-level accessors (`gen_at`/
/// `bump_gen`/`init_gen_table_in_place`) — split out of `segment_header.rs`
/// (task R6-CQ-7c). Compiled only under `hardened` (every item in the file is
/// `#[cfg(feature = "hardened")]`), so the module declaration itself is gated
/// the same way.
#[cfg(feature = "hardened")]
mod segment_header_gen_table;
mod segment_header_layout;
mod segment_header_meta_fields;
mod segment_header_views;
mod segment_layout;
pub(crate) mod segment_table;
pub(crate) mod size_classes;
#[cfg(feature = "alloc-decommit")]
pub mod small_segment_pool_config;

pub use alloc_core::AllocCore;
#[cfg(feature = "alloc-decommit")]
pub use large_cache_config::LargeCacheConfig;
#[cfg(feature = "alloc-decommit")]
pub use large_cache_mode::LargeCacheMode;
pub use segment_layout::SegmentLayout;
/// R4-8/N3 test-only harness for direct exercise of `SegmentTable`'s
/// open-addressing hash (backward-shift deletion). `pub` (not `pub(crate)`)
/// only because `alloc_core` itself is `#[doc(hidden)]`: the public surface is
/// test-only (the `#[doc(hidden)]` property test in `tests/`), reachable by
/// `sefer_alloc::alloc_core::SegmentHashHarness`. Nothing here is stable
/// public API.
#[doc(hidden)]
pub use segment_table::SegmentHashHarness;
#[cfg(feature = "alloc-decommit")]
pub use small_segment_pool_config::SmallSegmentPoolConfig;
