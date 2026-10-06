//! GlobalAlloc-face entry points of [`AllocCore`](crate::alloc_core::alloc_core::AllocCore) (mechanical split of the
//! former flat `alloc_core.rs`).
//!
//! Holds two sibling implementation files: `mem_impl.rs` (the `impl AllocCore { .. }` block
//! for `alloc`, `alloc_zeroed`, `dealloc`, `realloc`) and
//! `realloc_fastpath.rs` (the in-place realloc fast-path family). Pure code
//! movement; no behavior changed.

mod mem_impl;
mod realloc_fastpath;
