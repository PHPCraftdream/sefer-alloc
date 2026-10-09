//! The segment substrate — the per-segment metadata "header family"
//! (`segment_header/` and its `#[path]`-moved siblings), the self-hosted
//! [`SegmentTable`](segment_table::SegmentTable) registry, the feature-gated per-class
//! [`segment_directory`] + its always-compiled [`directory_stats`] counters,
//! the user-facing [`segment_layout`]
//! geometry tables, and the per-segment bitmap family (`bitmap/`).
//!
//! All children are re-exported by `alloc_core` (at their original
//! visibility/cfg parity) so every one stays reachable at its existing
//! `alloc_core::<name>` module path.

/// R7-A1: per-class bitmap directory used by segment discovery.
/// Enabled by `alloc-segment-directory`, including the `production` bundle.
/// Defines `SegmentDirectory`, its materialisation threshold, and rebuild.
#[cfg(feature = "alloc-segment-directory")]
pub(crate) mod segment_directory;
/// Group module: the per-segment metadata family — `SegmentHeader`/
/// `PageMap`/`BinTable`/`Layout`/`SegmentMeta` (`mod.rs` +
/// `descriptors.rs` + `layout_asserts.rs`), the `#[path]`-moved
/// field-accessor siblings (`segment_header_layout.rs`,
/// `segment_header_meta_fields.rs`, `segment_header_views.rs`).
#[doc(hidden)]
pub mod segment_header;
#[path = "segment_header/segment_header_layout.rs"]
pub(crate) mod segment_header_layout;
#[path = "segment_header/segment_header_meta_fields.rs"]
pub(crate) mod segment_header_meta_fields;
#[path = "segment_header/segment_header_views.rs"]
pub(crate) mod segment_header_views;
// `directory_stats` is declared HERE (with a `#[path]` into
// `segment_directory/`), not inside the cfg-gated `segment_directory`
// module: its consumers (`alloc_core_core_diag.rs`'s always-on
// `AllocStats::stats()` backing, `alloc_core_small.rs`) are NOT
// feature-gated, so the module must stay compiled in every config exactly
// as when it lived directly in `alloc_core`. Only its FILE moved.
/// Group module: the shared per-segment bitmap mechanism
/// (`segment_bitmap`) and its two wrappers (`alloc_bitmap`,
/// `magazine_bitmap`).
pub(crate) mod bitmap;
/// R7-A0 diagnostic counters for the per-class segment directory
/// (observability phase). Storage is always compiled; per-event increments
/// are gated behind `alloc-stats`. See the module doc for the counter
/// inventory.
#[path = "segment_directory/directory_stats.rs"]
pub(crate) mod directory_stats;
/// Non-intrusive remote-free bitmap ingress used by production Small sidecars.
#[allow(dead_code)]
pub(crate) mod remote_bitmap;
/// The per-segment geometry tables (`SegmentLayout`), moved unchanged.
pub(crate) mod segment_layout;
/// The global registry of all live segments, self-hosted in the primordial
/// segment's payload (mod.rs + the `hash.rs` open-addressing helpers + the
/// test-only `harness.rs`).
pub(crate) mod segment_table;
