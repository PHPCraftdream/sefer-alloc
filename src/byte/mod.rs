//! Byte / global-allocator descent (Phase 4, `byte` feature).
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).
//! The confined `unsafe` lives in the two submodules below; this file is plain
//! safe re-export glue and stays under the crate-level `#![deny(unsafe_code)]`.

mod byte_allocator;
mod byte_region;

pub use byte_allocator::ByteAllocator;
pub use byte_region::ByteRegion;
