//! `sefer-segment`: the segment and route substrate of `sefer-alloc`.
//!
//! The mutually dependent segment and route protocols intentionally share
//! one crate; no dependency points back to `sefer-alloc`.
#![deny(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

// R2-17 rationale: mirrors the pointer-width gate in the root `src/lib.rs` —
// the segment substrate relies on the same 64-bit-only layout premise.
#[cfg(all(feature = "alloc-core", not(target_pointer_width = "64")))]
compile_error!(
    "sefer-alloc: allocator features require a 64-bit target (R2-17 support \
     decision): `alloc-core` and every feature built on it (`alloc-global`, \
     `production`, ...) require metadata/magazine layout bounds, AtomicU64 and \
     pointer-provenance rules that have NOT been audited on 32-bit pointer \
     widths. Rebuild for a \
     64-bit target, or drop the allocator features (the region-only \
     `Region<T>` surface still works on 32-bit). See README.md \
     \"Target support\"."
);

#[path = "support/alloc_core.rs"]
pub(crate) mod alloc_core;
#[doc(hidden)]
pub mod support;
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) use support::geometry::LAZY_FIRST_CHUNK;
pub(crate) mod node {
    pub(crate) use crate::support::node_impl::*;
}
#[cfg(any(feature = "alloc-decommit", feature = "bench-internals"))]
pub(crate) mod reservation_state {
    // P2.1: unused under cfg combinations where the consumer is compiled out.
    #[allow(unused_imports)]
    pub(crate) use crate::support::reservation_state_impl::LargeReservationState;
}
pub(crate) mod size_classes {
    pub(crate) use crate::support::size_classes_impl::*;
}
pub mod segment;

pub use sefer_ghost::ghost;

/// Stability: this hidden pure helper is not supported API.
#[doc(hidden)]
#[inline(always)]
pub fn large_payload_address_matches(address: usize, base: usize, payload_offset: usize) -> bool {
    base.checked_add(payload_offset) == Some(address)
}

pub use segment::remote_bitmap;
pub use segment::segment_header;
pub use segment::segment_layout::SegmentLayout;
#[cfg(feature = "alloc-global")]
pub use segment::segment_route;
pub use segment::segment_table;
