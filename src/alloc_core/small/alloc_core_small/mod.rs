//! Small-path hot cluster of [`AllocCore`] (mechanical split of
//! `alloc_core.rs`).
//!
//! This file holds the `impl AllocCore { .. }` block for the small-object
//! alloc / dealloc / carve / segment-reserve hot path. The cross-thread
//! reclaim, magazine batch, and diagnostics blocks live in their sibling
//! files (`alloc_core_small_reclaim`, `alloc_core_small_magazine`,
//! `alloc_core_small_diag`). Pure code-movement; no behavior changed.

// Mechanical-split siblings of the former flat `alloc_core_small.rs`.
// `directory` keeps a `cfg` gate here (not only on its items) because the
// WHOLE file is `alloc-segment-directory`-gated content: gating the
// declaration keeps its compiled-file set identical to when the code lived
// directly in `alloc_core_small.rs` (same intrinsic-gate discipline as
// `platform::numa`/`platform::dirty_by_class`).
#[path = "dealloc.rs"]
mod dealloc;

#[cfg(feature = "alloc-segment-directory")]
#[path = "directory.rs"]
mod directory;

#[path = "find_segment.rs"]
mod find_segment;

#[path = "reserve.rs"]
mod reserve;

#[path = "alloc_core_small_impl.rs"]
mod implementation;
#[allow(unused_imports)]
pub(crate) use implementation::*;
