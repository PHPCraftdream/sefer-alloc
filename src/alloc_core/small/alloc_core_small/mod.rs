//! Small-path hot cluster of
//! [`AllocCore`](crate::alloc_core::alloc_core::AllocCore) (mechanical split of
//! `alloc_core.rs`).
//!
//! This module wires the small-object allocation path and re-exports the
//! implementation in `alloc_core_small_impl.rs`.
//! Free-list operations are grouped in `dealloc.rs`, optional directory
//! maintenance in `directory.rs`, segment lookup in `find_segment/`, and
//! reservation logic in `reserve.rs`. Cross-thread reclaim, magazine batch,
//! and diagnostics remain in their sibling modules.

// Mechanical-split siblings of the former flat `alloc_core_small.rs`.
// `directory` keeps a `cfg` gate here (not only on its items) because the
// WHOLE file is `alloc-segment-directory`-gated content: gating the
// declaration keeps its compiled-file set identical to when the code lived
// directly in `alloc_core_small.rs` (same intrinsic-gate discipline as
// `platform::numa`).
mod dealloc;

#[cfg(feature = "alloc-segment-directory")]
mod directory;

mod find_segment;

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
mod sidecar_drain_outcome;

mod reserve;
#[cfg(all(feature = "bench-internals", not(feature = "numa-aware")))]
pub(crate) use reserve::{SMALL_SEGMENTS_RESERVED, THP_HINTED_SMALL_SEGMENTS};

mod alloc_core_small_impl;
#[allow(unused_imports)]
pub(crate) use alloc_core_small_impl::*;
