//! Doc-hidden registry facade. The implementation and nominal types live in
//! `sefer-registry`; this module preserves the root paths and feature gates.

#![cfg_attr(not(feature = "internals"), allow(unused_imports))]

#[doc(hidden)]
pub mod bootstrap {
    #[cfg(feature = "internals")]
    pub use sefer_registry::registry::bootstrap::{
        count_for_test, dbg_num_chunks, dbg_rollback_chunk_sentinel_reenterable,
        dbg_set_inject_chunk_oom, dbg_slot_or_none, FreeSlotHint, RollbackProbe,
    };
    pub use sefer_registry::registry::bootstrap::{ensure, Registry, MAX_HEAPS};
}

#[doc(hidden)]
pub mod heap_core {
    pub use sefer_registry::registry::heap_core::HeapCore;
}

#[doc(hidden)]
pub mod heap_registry {
    #[cfg(feature = "alloc-decommit")]
    pub use super::large_cache_hits_total;
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "alloc-decommit"
    ))]
    pub use super::tcache_and_large_cache_hits_total;
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    pub use super::tcache_hits_total;
    pub use super::{config_conflicts_total, heaps_claimed_high_water, HeapRegistry};
    #[cfg(feature = "internals")]
    pub use sefer_registry::registry::heap_registry::{
        dbg_bump_count_without_materialising, dbg_claim_then_simulate_oom, dbg_slot_initialised,
    };
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub use sefer_registry::registry::heap_registry::{
        pick_with_hint_spill, pick_with_saturation, FreeSlotHint, HeapLease, SaturationHint,
    };
}

#[cfg(feature = "alloc-global")]
#[doc(hidden)]
pub use crate::alloc_core::segment_route;
#[doc(hidden)]
pub use sefer_registry::registry::heap_slot;

#[cfg(feature = "internals")]
#[doc(hidden)]
pub use sefer_registry::registry::heap_registry::HeapLease;
#[cfg(feature = "alloc-decommit")]
#[doc(hidden)]
pub use sefer_registry::registry::large_cache_hits_total;
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]
#[doc(hidden)]
pub use sefer_registry::registry::tcache_and_large_cache_hits_total;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
pub use sefer_registry::registry::tcache_hits_total;
#[doc(hidden)]
pub use sefer_registry::registry::{config_conflicts_total, heaps_claimed_high_water};
#[doc(hidden)]
pub use sefer_registry::registry::{HeapCore, HeapRegistry, HeapSlot};
