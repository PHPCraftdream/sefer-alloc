//! GlobalAlloc-face entry points of [`AllocCore`](crate::alloc_core::alloc_core::AllocCore) (mechanical split of the
//! former flat `alloc_core.rs`).
//!
//! Holds two sibling implementation files: `mem_impl.rs` (the `impl AllocCore { .. }` block
//! for `alloc`, `alloc_zeroed`, `dealloc`, `realloc`) and
//! `realloc_fastpath.rs` (the in-place realloc fast-path family). The Large
//! mapping's advice orchestrator and terminal-release funnel sit beside them.

#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
mod advise_fresh_large;
mod mem_impl;
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
mod observe_large_control;
#[cfg(feature = "alloc-global")]
mod owned_block;
mod realloc_fastpath;
mod release_large_mapping;

#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) use advise_fresh_large::advise_fresh_large;
#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) use observe_large_control::observe_large_control;
#[cfg(feature = "alloc-global")]
pub(crate) use owned_block::OwnedBlock;
pub(crate) use release_large_mapping::release_large_mapping;
