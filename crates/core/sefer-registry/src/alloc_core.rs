//! Internal compatibility imports for the registry implementation.

#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) use sefer_alloc_core::alloc_core::policy;
#[cfg(any(feature = "fastbin", feature = "medium-classes"))]
pub(crate) use sefer_alloc_core::alloc_core::OwnedBlock;
#[cfg(feature = "alloc-stats")]
pub(crate) use sefer_alloc_core::alloc_core::LARGE_ZERO_PASS_CALLS;
#[cfg(all(
    feature = "fastbin",
    any(feature = "internals", feature = "bench-internals")
))]
pub(crate) use sefer_alloc_core::alloc_core::LOCAL_REJECT_FREES;
#[cfg(feature = "internals")]
pub(crate) use sefer_alloc_core::alloc_core::MEDIUM_PROMOTION_REACHABLE;
#[cfg(all(feature = "alloc-stats", feature = "virgin-zero-skip"))]
pub(crate) use sefer_alloc_core::alloc_core::SMALL_ZERO_PASS_CALLS;
pub(crate) use sefer_alloc_core::alloc_core::{
    fatal, segment_header, size_classes, AllocCore, FOREIGN_OR_UNROUTABLE_FREES,
};
#[cfg(feature = "alloc-decommit")]
pub(crate) use sefer_alloc_core::alloc_core::{large_cache_config, LargeCacheConfig};
pub(crate) use sefer_alloc_core::alloc_core::{
    medium_promotion_reachable, medium_promotion_unreachable,
};
pub use sefer_alloc_core::alloc_core::{record_exit_lock_giveup, segment_route};
#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]
pub(crate) use sefer_alloc_core::alloc_core::{ReservedSmallSegment, SegmentStateReconciliation};
#[cfg(feature = "bench-internals")]
pub(crate) use sefer_alloc_core::alloc_core::{
    B4_FOREIGN_PUBLISH_OK, BACKGROUND_INGRESS_STEP_CALLS, XNULL_NULL_TAIL_GUARD,
};
pub(crate) use sefer_platform::os;
pub(crate) use sefer_segment::segment::bitmap::state_byte;
#[cfg(all(
    feature = "bench-internals",
    any(
        feature = "internals",
        r18_sidecar_scan_bench,
        r19_sidecar_summary_oracle
    )
))]
pub(crate) use sefer_segment::segment::remote_bitmap;
medium_promotion_reachable! {
    #[cfg(feature = "bench-internals")]
    pub(crate) use sefer_alloc_core::alloc_core::{
        promotion_byte_bucket, PROMOTION_BYTES_HIST, PROMOTION_BYTES_MAX, PROMOTION_BYTES_MIN,
        PROMOTION_BYTES_SUM, PROMOTION_COUNT,
    };
}
