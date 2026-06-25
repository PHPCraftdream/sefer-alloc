//! Phase 8 — the segment substrate + self-hosted metadata (the Membrane
//! Inversion), behind the `alloc-core` feature.
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).
//! The two confined-`unsafe` seams are [`os`] and [`node`]; every other file
//! is pure safe code that composes them.
//!
//! [`os`]: self::os
//! [`node`]: self::node

// The file `alloc_core.rs` carries the same name as this module per the
// crate's one-export-per-file convention; silence clippy's module_inception.
#[allow(clippy::module_inception)]
mod alloc_core;
mod bootstrap;
pub(crate) mod node;
pub(crate) mod os;
pub(crate) mod segment_header;
mod segment_layout;
pub(crate) mod segment_table;
pub(crate) mod size_classes;

pub use alloc_core::AllocCore;
pub use segment_layout::SegmentLayout;
