//! Byte / global-allocator descent (Phase 4, `byte` feature) and the parallel
//! sharded byte arena (Phase 7d, `byte-sharded` feature).
//!
//! Re-exports only — no logic lives here (per the one-export-per-file rule).
//! The confined `unsafe` lives in the three submodules below; this file is
//! plain safe re-export glue and stays under the crate-level
//! `#![deny(unsafe_code)]`.

mod byte_allocator;
mod byte_region;

#[cfg(feature = "byte-sharded")]
mod sharded_byte_arena;

pub use byte_allocator::ByteAllocator;
pub use byte_region::ByteRegion;

#[cfg(feature = "byte-sharded")]
pub use sharded_byte_arena::ShardedByteArena;
