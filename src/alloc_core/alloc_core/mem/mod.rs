//! GlobalAlloc-face entry points of [`AllocCore`] (mechanical split of the
//! former flat `alloc_core.rs`).
//!
//! Holds two sibling modules: [`mem_impl`] (the `impl AllocCore { .. }` block
//! for `alloc`, `alloc_zeroed`, `dealloc`, `realloc`) and
//! [`realloc_fastpath`] (the in-place realloc fast-path family). Pure code
//! movement; no behavior changed.

mod mem_impl;
mod realloc_fastpath;
