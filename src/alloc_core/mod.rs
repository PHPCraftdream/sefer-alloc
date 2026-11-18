//! Phase 8 — the segment substrate + self-hosted metadata (the Membrane
//! Inversion), behind the `alloc-core` feature.
//!
//! P2.2: the implementation moved wholesale to the `sefer-alloc-core` crate
//! (`crates/core/sefer-alloc-core/src/alloc_core`); module tree, file names
//! and `crate::alloc_core::…` paths inside that crate are unchanged. This
//! file is the root conduit: re-exports only, no logic (per the
//! one-export-per-file rule).
//!
//! Visibility parity is preserved exactly: the previously-`pub` names are
//! re-exported `pub` (with their original `#[doc(hidden)]`/`#[cfg]`
//! attributes), and every previously-`pub(crate)` name is re-exported
//! `pub(crate)` — never widened to the root's public surface. The raw-pointer
//! seams (`alloc_core::platform::{node,sidecar}` and `os`'s raw-pointer
//! wrappers) are deliberately NOT re-exported here: root call sites use the
// canonical `sefer-segment` `RootNode` and `sefer-platform` surfaces directly.
//! Cross-crate names the new crate still keeps private (e.g.
//! `alloc_core::alloc_core::counters`, `large::large_cache_extended`,
//! `node::Node`) resolve when the P2.2 visibility pass retargets those call
//! sites.

// ── previously-public surface (unchanged `pub`, same cfg gates) ──
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-decommit")]
pub use sefer_alloc_core::alloc_core::large_cache_config;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-decommit")]
pub use sefer_alloc_core::alloc_core::large_cache_config::LargeCacheConfig;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-decommit", feature = "internals"))]
pub use sefer_alloc_core::alloc_core::large_cache_mode;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-decommit")]
pub use sefer_alloc_core::alloc_core::large_cache_mode::LargeCacheMode;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-decommit")]
pub use sefer_alloc_core::alloc_core::policy;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-decommit")]
pub use sefer_alloc_core::alloc_core::policy::large_thp::{
    LargeThpCurve, LargeThpEstimator, LargeThpGradient, LargeThpMode, LargeThpRequestGroup,
    LargeThpShare,
};
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-decommit")]
pub use sefer_alloc_core::alloc_core::policy::{LargeCachePolicy, Profile, SmallPoolPolicy};
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub use sefer_alloc_core::alloc_core::reserved_small_segment::ReservedSmallSegment;
#[cfg(feature = "alloc-core")]
// Without `alloc-global` no lib code path reaches this `#[doc(hidden)]`
// facade re-export (its only compiled consumers live in the
// `alloc-global`-gated `registry`/`global` modules); the path itself stays
// `alloc-core`-public for alloc-core-only consumers. Scoped allow, not a
// blanket module-wide suppression (R6-T16 discipline).
#[cfg_attr(not(feature = "alloc-global"), allow(unused_imports))]
#[doc(hidden)]
pub use sefer_alloc_core::alloc_core::segment_header;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
#[doc(hidden)]
pub use sefer_alloc_core::alloc_core::segment_route;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-decommit", feature = "internals"))]
pub use sefer_alloc_core::alloc_core::small_segment_pool_config;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-decommit")]
pub use sefer_alloc_core::alloc_core::small_segment_pool_config::SmallSegmentPoolConfig;
#[cfg(feature = "alloc-core")]
pub use sefer_alloc_core::alloc_core::AllocCore;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-global", feature = "internals"))]
#[doc(hidden)]
pub use sefer_alloc_core::alloc_core::LargeReservationState;
#[cfg(feature = "alloc-core")]
pub use sefer_alloc_core::alloc_core::SegmentLayout;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub use sefer_alloc_core::alloc_core::{SegmentStateAccount, SegmentStateReconciliation};
// P2.2: public module-path parity — the module itself was `pub` pre-move.
#[cfg(feature = "alloc-core")]
#[cfg(feature = "numa-aware")]
// Sole consumer of this `#[doc(hidden)]` facade re-export is the
// `internals`-gated `tests/numa_seam.rs`; a `numa-aware`-without-`internals`
// lib build never reads it. The public path is preserved unchanged; the
// scoped allow follows the R6-T16 per-element discipline.
#[cfg_attr(not(feature = "internals"), allow(unused_imports))]
#[doc(hidden)]
pub use sefer_alloc_core::alloc_core::numa;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub use sefer_alloc_core::alloc_core::reserved_small_segment;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "internals")]
#[doc(hidden)]
pub use sefer_alloc_core::alloc_core::SegmentHashHarness;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "internals")]
pub use sefer_alloc_core::alloc_core::{dbg_sidecar_reservation_stats, SidecarReservationStats};

// ── restricted surface (unchanged `pub(crate)`, same cfg gates) ──
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_alloc_core::alloc_core::fatal;
// P2.2 os move: root consumers use the canonical `sefer_platform::os`
// directly (`SEGMENT`, `reserve_depot_overflow`, `segment_base_of_ptr` all
// resolve there), so the root no longer conduits the `alloc_core::os`
// module from `sefer-alloc-core`.
#[cfg(feature = "alloc-core")]
pub(crate) use sefer_alloc_core::alloc_core::medium_promotion_reachable;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_alloc_core::alloc_core::medium_promotion_unreachable;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_alloc_core::alloc_core::record_exit_lock_giveup;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_alloc_core::alloc_core::size_classes;
#[cfg(feature = "alloc-core")]
#[cfg(all(
    feature = "alloc-global",
    any(feature = "fastbin", feature = "medium-classes")
))]
pub(crate) use sefer_alloc_core::alloc_core::OwnedBlock;
#[cfg(feature = "alloc-core")]
#[cfg(any(feature = "alloc-global", feature = "alloc-stats"))]
pub(crate) use sefer_alloc_core::alloc_core::FOREIGN_OR_UNROUTABLE_FREES;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_alloc_core::alloc_core::LARGE_REMOTE_RETIREMENTS;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-stats")]
pub(crate) use sefer_alloc_core::alloc_core::LARGE_ZERO_PASS_CALLS;
#[cfg(feature = "alloc-core")]
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    any(feature = "internals", feature = "bench-internals")
))]
pub(crate) use sefer_alloc_core::alloc_core::LOCAL_REJECT_FREES;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-global", feature = "internals"))]
pub(crate) use sefer_alloc_core::alloc_core::MEDIUM_PROMOTION_REACHABLE;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-stats", feature = "virgin-zero-skip"))]
pub(crate) use sefer_alloc_core::alloc_core::SMALL_ZERO_PASS_CALLS;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-global", feature = "bench-internals"))]
pub(crate) use sefer_alloc_core::alloc_core::XNULL_NULL_TAIL_GUARD;
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-global", feature = "bench-internals"))]
pub(crate) use sefer_alloc_core::alloc_core::{
    B4_CURSOR_HITS, B4_CURSOR_MISSES, B4_DIRECTORY_LOOKUPS, B4_FIND_SEGMENT_CALLS,
    B4_FOREIGN_PUBLISH_OK, B4_SCAN_CALLS, B4_SCAN_WORDS, B4_SHARD_LOCK_SPINS, B4_SHARD_LOCK_YIELDS,
    B4_U31_DEFERRED_SCANS, B4_U31_DISCOVERY_SCANS, B4_U31_SCAN_HOLDERS_BASE,
    B4_U31_SCAN_NO_PENDING, B4_U31_SCAN_SEGMENTS,
};
#[cfg(feature = "alloc-core")]
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "bench-internals"
))]
pub(crate) use sefer_alloc_core::alloc_core::{
    BACKGROUND_INGRESS_STEP_CALLS, SIDECAR_INGRESS_DRAIN_CALLS, SIDECAR_INGRESS_RECORDS_CONSUMED,
};
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_platform::os;
#[cfg(feature = "alloc-core")]
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_segment::segment::bitmap::state_byte;
medium_promotion_reachable! {
    #[cfg(feature = "alloc-core")]
    #[cfg(feature = "bench-internals")]
    pub(crate) use sefer_alloc_core::alloc_core::{
        promotion_byte_bucket, PROMOTION_BYTES_HIST, PROMOTION_BYTES_MAX, PROMOTION_BYTES_MIN,
        PROMOTION_BYTES_SUM, PROMOTION_COUNT,
    };
}

// ── crate-internal `sefer-segment` compatibility shims ──
// P2.2: the former `config`, `large`, `segment::bitmap` and `segment_table`
// root shims were removed — `rg` confirmed no `alloc_core::`-path consumers
// remain in the root tree (registry/global reference the canonical
// `sefer_alloc_core`/`sefer_segment` crates or the explicit re-exports
// above instead).
#[cfg(feature = "alloc-core")]
#[cfg(all(
    feature = "alloc-core",
    feature = "alloc-global",
    feature = "bench-internals"
))]
#[doc(hidden)]
pub(crate) use sefer_segment::segment::remote_bitmap;
