//! GlobalAlloc-face entry points of [`AllocCore`](crate::alloc_core::alloc_core::AllocCore) (mechanical split of the
//! former flat `alloc_core.rs`).
//!
//! Holds two sibling implementation files: `mem_impl.rs` (the `impl AllocCore { .. }` block
//! for `alloc`, `alloc_zeroed`, `dealloc`, `realloc`) and
//! `realloc_fastpath.rs` (the in-place realloc fast-path family). Pure code
//! movement; no behavior changed.

mod mem_impl;
#[cfg(feature = "alloc-global")]
mod owned_block;
mod realloc_fastpath;

#[cfg(feature = "alloc-global")]
pub(crate) use owned_block::OwnedBlock;
// B4 У1 (`small-class-authority`): tri-state free-path class-authority
// verdict defined in `mem_impl.rs` (next to its producer,
// `AllocCore::small_class_authority`); consumed by the registry's
// `small_free_guard` callers via `alloc_core::alloc_core::ClassAuthority`.
#[cfg(feature = "small-class-authority")]
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(crate) use mem_impl::ClassAuthority;
