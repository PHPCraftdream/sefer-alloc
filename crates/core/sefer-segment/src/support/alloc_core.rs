//! Compatibility paths used by relocated segment and route implementations.
//!
//! Root-compat facade: consumed from `sefer-alloc`; alias modules here are
//! intentionally import-only.
#![allow(unused_imports)]

pub(crate) mod platform {
    pub(crate) use crate::support::platform::{fatal, os};
    pub(crate) mod node {
        pub(crate) use crate::support::node_impl::{Node, NODE_SIZE};
    }
}
pub(crate) mod fatal {
    pub(crate) use crate::support::platform::fatal::fatal;
}
pub(crate) mod os {
    pub(crate) use crate::support::platform::os::release_segment;
    pub(crate) use crate::support::platform::os::*;
}
pub(crate) mod node {
    pub(crate) use crate::support::node_impl::{Node, NODE_SIZE};
}
pub(crate) mod config {
    pub(crate) use crate::support::continuation_bounds_impl as continuation_bounds;
}
pub(crate) mod large {
    pub(crate) use crate::support::reservation_state_impl as reservation_state;
}
pub(crate) mod reservation_state {
    pub(crate) use crate::support::reservation_state_impl::*;
}
pub(crate) mod alloc_core_small {
    pub(crate) use crate::support::geometry::LAZY_FIRST_CHUNK;
}
pub(crate) mod state_byte {
    pub(crate) use crate::segment::bitmap::state_byte::*;
}
pub(crate) mod live_tag {
    pub(crate) use crate::segment::bitmap::live_tag::*;
}
pub(crate) mod segment_layout {
    pub(crate) use crate::segment::segment_layout::*;
}
pub(crate) mod segment {
    #[cfg(feature = "alloc-segment-directory")]
    pub(crate) use crate::segment::segment_directory;
    #[cfg(feature = "alloc-global")]
    pub(crate) use crate::segment::segment_route;
    pub(crate) use crate::segment::{bitmap, remote_bitmap, segment_header, segment_table};
}
pub(crate) use crate::segment::remote_bitmap;
#[cfg(feature = "alloc-segment-directory")]
pub(crate) use crate::segment::segment_directory;
pub(crate) use crate::segment::segment_header;
#[cfg(feature = "alloc-global")]
pub(crate) use crate::segment::segment_route;
pub(crate) use crate::segment::segment_table;
pub(crate) use crate::support::size_classes_impl as size_classes;
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_platform::process_exit::process_exit_in_progress;
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_platform::record_exit_lock_giveup::record_exit_lock_giveup;
#[cfg(feature = "alloc-decommit")]
pub(crate) use sefer_policy::large_cache_config;
#[cfg(feature = "bench-internals")]
// Historical double path `alloc_core::alloc_core::*` used by root call sites.
#[allow(clippy::module_inception)]
pub(crate) mod alloc_core {
    pub(crate) use crate::support::counters_impl::{
        tier1_bump, B4_DIRECTORY_LOOKUPS, B4_SHARD_LOCK_SPINS, B4_SHARD_LOCK_YIELDS,
        B4_U31_SCAN_HOLDERS_BASE, B4_U31_SCAN_SEGMENTS, CONTAINS_BASE_TIER1_HITS,
        CONTAINS_BASE_TIER1_MISSES,
    };
}
#[cfg(feature = "alloc-global")]
pub(crate) use crate::large_payload_address_matches;
#[cfg(feature = "bench-internals")]
#[doc(hidden)]
pub use crate::support::counters_impl::{
    B4_DIRECTORY_LOOKUPS, B4_SHARD_LOCK_SPINS, B4_SHARD_LOCK_YIELDS, B4_U31_SCAN_HOLDERS_BASE,
    B4_U31_SCAN_SEGMENTS,
};
