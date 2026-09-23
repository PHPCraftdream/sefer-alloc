//! Small-path hot cluster of [`AllocCore`] (mechanical split of
//! `alloc_core.rs`).
//!
//! This module wires the small-object allocation path and re-exports the
//! implementation in `alloc_core_small_impl.rs`.
//! Free-list operations are grouped in `dealloc.rs`, optional directory
//! maintenance in `directory.rs`, segment lookup in `find_segment.rs`, and
//! reservation logic in `reserve.rs`. Cross-thread reclaim, magazine batch,
//! and diagnostics remain in their sibling modules.

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
mod alloc_core_small_impl;
#[allow(unused_imports)]
pub(crate) use alloc_core_small_impl::*;
