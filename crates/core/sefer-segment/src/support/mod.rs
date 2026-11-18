//! Grouped helpers whose defining implementations are owned by this crate.

// Loom seam for `reservation_state_impl` (see its header comment): the crate
// itself always binds `core` atomics; an actual-source loom harness mounts the
// file under its own shim module whose `super` provides loom's atomics.
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};

#[path = "facade/geometry.rs"]
pub mod geometry;

#[path = "continuation_bounds.rs"]
#[doc(hidden)]
pub mod continuation_bounds_impl;
/// Counters are shared identities so root call sites update the very same atomics.
#[path = "facade/counters.rs"]
#[doc(hidden)]
pub mod counters_impl;
#[path = "../segment/support/segment_directory/directory_stats.rs"]
pub mod directory_stats;
#[path = "node.rs"]
#[doc(hidden)]
pub mod node_impl;
#[path = "facade/platform.rs"]
pub(crate) mod platform;
#[path = "reservation_state.rs"]
pub mod reservation_state_impl;
#[path = "size_classes.rs"]
#[doc(hidden)]
pub mod size_classes_impl;
// Root-compat aliases: each binds a historical name to the *_impl module.
#[allow(unused_imports)]
pub(crate) use crate::alloc_core as alloc_core_impl;
#[allow(unused_imports)]
pub(crate) use continuation_bounds_impl as continuation_bounds;
#[allow(unused_imports)]
pub(crate) use counters_impl as counters;
#[allow(unused_imports)]
pub(crate) use node_impl as node;
#[allow(unused_imports)]
pub(crate) use reservation_state_impl as reservation_state;
#[allow(unused_imports)]
pub(crate) use size_classes_impl as size_classes;

// P2.1: the root compatibility wrapper for continuation bounds is
// `#[derive(Clone, Copy)]` over this type (the root `config/continuation_bounds.rs`
// pre-split type was `Copy`; the drain loop passes the snapshot by value per
// link). Two-`usize` value type: Copy is layout/behavior-neutral and adds no
// new API surface.
impl Clone for crate::support::continuation_bounds_impl::ContinuationBounds {
    fn clone(&self) -> Self {
        *self
    }
}
impl Copy for crate::support::continuation_bounds_impl::ContinuationBounds {}

/// Doc-hidden directory counter accessors (same atomics as `directory_stats`).
#[doc(hidden)]
pub mod directory_counters {
    use crate::support::directory_stats as ds;
    use core::sync::atomic::AtomicU64;
    pub const fn directory_hits() -> &'static AtomicU64 {
        &ds::DIRECTORY_HITS
    }
    pub const fn directory_stale_hits() -> &'static AtomicU64 {
        &ds::DIRECTORY_STALE_HITS
    }
    pub const fn directory_fallback_scans() -> &'static AtomicU64 {
        &ds::DIRECTORY_FALLBACK_SCANS
    }
    pub const fn directory_words_examined() -> &'static AtomicU64 {
        &ds::DIRECTORY_WORDS_EXAMINED
    }
    pub const fn full_scan_slots_examined() -> &'static AtomicU64 {
        &ds::FULL_SCAN_SLOTS_EXAMINED
    }
    pub const fn directory_authoritative_miss() -> &'static AtomicU64 {
        &ds::DIRECTORY_AUTHORITATIVE_MISS
    }
    pub const fn directory_miss_self_heal() -> &'static AtomicU64 {
        &ds::DIRECTORY_MISS_SELF_HEAL
    }
    pub const fn directory_rescue_oom_avoided() -> &'static AtomicU64 {
        &ds::DIRECTORY_RESCUE_OOM_AVOIDED
    }
    pub const fn routed_miss_scans() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCANS
    }
    pub const fn routed_miss_scan_drain_created_free() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCAN_DRAIN_CREATED_FREE
    }
    pub const fn routed_miss_scan_bin_already_nonempty() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCAN_BIN_ALREADY_NONEMPTY
    }
    pub const fn routed_miss_scan_nothing() -> &'static AtomicU64 {
        &ds::ROUTED_MISS_SCAN_NOTHING
    }
}
