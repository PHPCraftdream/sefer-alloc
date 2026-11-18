//! Root compatibility wrapper over the canonical `sefer-segment` size-class scheme.
// P2.2: root registry/global consumers read these names cross-crate through
// the `sefer_alloc::alloc_core::size_classes` conduit, so the re-exports are
// widened to `#[doc(hidden)] pub` at the new crate boundary. The canonical
// source stays `sefer_segment::support::size_classes_impl`; consumers are
// retargeted there in the P2.2 visibility pass.
#[cfg(any(kani, all(feature = "alloc-global", feature = "fastbin")))]
#[doc(hidden)]
pub use sefer_segment::support::size_classes_impl::SIZE_CLASS_TABLE;
#[doc(hidden)]
pub use sefer_segment::support::size_classes_impl::{
    AllocKind, SizeClasses, MIN_BLOCK, SMALL_CLASS_COUNT,
};
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    not(feature = "hardened")
))]
#[doc(hidden)]
pub use sefer_segment::support::size_classes_impl::{MIN_BLOCK_SHIFT, SIZE2CLASS};
