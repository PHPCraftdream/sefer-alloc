//! Root compatibility wrapper over the canonical `sefer-segment` size-class scheme.
#[cfg(any(kani, all(feature = "alloc-global", feature = "fastbin")))]
pub(crate) use sefer_segment::support::size_classes_impl::SIZE_CLASS_TABLE;
pub(crate) use sefer_segment::support::size_classes_impl::{
    AllocKind, SizeClasses, MIN_BLOCK, SMALL_CLASS_COUNT,
};
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    not(feature = "hardened")
))]
pub(crate) use sefer_segment::support::size_classes_impl::{MIN_BLOCK_SHIFT, SIZE2CLASS};
