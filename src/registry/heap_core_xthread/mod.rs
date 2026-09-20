//! Cross-thread free routing for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R4-10).
//!
//! This file holds the `impl HeapCore { .. }` block for the cross-thread
//! deferred-free drain and the foreign-thread dealloc routing protocol.
//! All methods are `#[cfg(feature = "alloc-xthread")]`.
//! Pure code-movement sibling of `heap_core.rs`; no behavior changed.

mod drain;
mod overflow;
mod ring;
mod routing;
mod stall;
