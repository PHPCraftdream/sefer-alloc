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
pub(crate) mod alloc_bitmap;
#[allow(clippy::module_inception)]
mod alloc_core;
mod bootstrap;
pub(crate) mod node;
/// NUMA OS-seam: NUMA-node detection and segment binding.
/// `pub` (not `pub(crate)`) only because `alloc_core` itself is
/// `#[doc(hidden)]` (see `lib.rs`): the public surface is test-only (the
/// `#[doc(hidden)]` re-export), reachable by the isolated NUMA unit test.
/// Nothing here is stable public API.
#[cfg(feature = "numa-aware")]
#[doc(hidden)]
pub mod numa;
pub(crate) mod os;
/// The per-segment non-intrusive cross-thread-free MPSC ring. Compiled in
/// unconditionally so the segment [`Layout`](segment_header::Layout) (which
/// always reserves the ring's bytes to keep the byte layout uniform across
/// feature configs) can reference `FOOTPRINT`; the `push`/`drain` methods are
/// the only `alloc-xthread`-gated surface.
///
/// `pub` (not `pub(crate)`) only because `alloc_core` itself is
/// `#[doc(hidden)]` (see `lib.rs`): the public surface is test-only (the
/// `#[doc(hidden)] pub` methods on `RemoteFreeRing`), reachable by the
/// isolated ring unit test. Nothing here is stable public API.
#[doc(hidden)]
pub mod remote_free_ring;
pub(crate) mod segment_header;
mod segment_layout;
pub(crate) mod segment_table;
pub(crate) mod size_classes;

pub use alloc_core::AllocCore;
#[cfg(feature = "alloc-decommit")]
pub use alloc_core::LargeCacheMode;
pub use segment_layout::SegmentLayout;
