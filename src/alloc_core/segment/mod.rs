//! The segment substrate — the per-segment metadata "header family"
//! (`segment_header/` and its `#[path]`-moved siblings), the self-hosted
//! [`SegmentTable`] registry, the experimental per-class
//! [`segment_directory`] + its always-compiled [`directory_stats`] counters,
//! the per-segment [`RemoteFreeRing`], the user-facing [`segment_layout`]
//! geometry tables, and the per-segment bitmap family (`bitmap/`).
//!
//! All children are re-exported by `alloc_core` (at their original
//! visibility/cfg parity) so every one stays reachable at its existing
//! `alloc_core::<name>` module path.

/// R7-A1: per-class `class_nonempty` bitmap sidecar for O(1)
/// directory-driven segment lookup. Feature-gated behind
/// `alloc-segment-directory` (experimental, off by default). The module
/// defines the `SegmentDirectory` struct, the materialisation threshold
/// constant, and the one-time rebuild routine. Lookup wiring is A3 scope.
#[cfg(feature = "alloc-segment-directory")]
pub(crate) mod segment_directory;
/// Group module: the per-segment metadata family — [`SegmentHeader`]/
/// [`PageMap`]/[`BinTable`]/[`Layout`]/[`SegmentMeta`] (`mod.rs` +
/// `descriptors.rs` + `layout_asserts.rs`), the `#[path]`-moved
/// field-accessor siblings (`segment_header_layout.rs`,
/// `segment_header_meta_fields.rs`, `segment_header_views.rs`), and the
/// `hardened`-gated generation-table accessors
/// (`segment_header_gen_table.rs`).
#[doc(hidden)]
pub mod segment_header;
/// X7 Ф1 (task #189) generation-table byte-level accessors (`gen_at`/
/// `bump_gen`/`init_gen_table_in_place`) — split out of `segment_header.rs`
/// (task R6-CQ-7c). Compiled only under `hardened` (every item in the file is
/// `#[cfg(feature = "hardened")]`), so the module declaration itself is gated
/// the same way.
#[cfg(feature = "hardened")]
#[path = "segment_header/segment_header_gen_table.rs"]
pub(crate) mod segment_header_gen_table;
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
/// The per-segment geometry tables (`SegmentLayout`), moved unchanged.
pub(crate) mod segment_layout;
/// The global registry of all live segments, self-hosted in the primordial
/// segment's payload (mod.rs + the `hash.rs` open-addressing helpers + the
/// test-only `harness.rs`).
pub(crate) mod segment_table;
