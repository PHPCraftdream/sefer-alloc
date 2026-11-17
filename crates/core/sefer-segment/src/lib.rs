//! `sefer-segment`: the segment and route substrate of `sefer-alloc`.
//!
//! The mutually dependent segment and route protocols intentionally share
//! one crate; no dependency points back to `sefer-alloc`.
#![deny(unsafe_code)]
#![cfg_attr(docsrs, feature(doc_cfg))]

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
#[macro_use]
extern crate sefer_ghost;

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
