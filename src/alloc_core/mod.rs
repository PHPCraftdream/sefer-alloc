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

// ── previously-public surface (public paths preserved with `internals`) ──
#[cfg(feature = "alloc-core")]
#[cfg(all(feature = "alloc-decommit", feature = "internals"))]
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
#[cfg(feature = "internals")]
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
#[cfg(feature = "alloc-global")]
pub(crate) use sefer_alloc_core::alloc_core::LARGE_REMOTE_RETIREMENTS;
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
