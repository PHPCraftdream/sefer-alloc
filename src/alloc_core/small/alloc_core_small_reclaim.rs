//! Owner-only logical retirement of terminal Small sidecar records.

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta,
};
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::size_classes::SizeClasses;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
impl AllocCore {
    /// The owner retains the scan while retiring records and drops it before
    /// reservation finalization. A duplicate or magazine-resident record never
    /// mutates the free list or retires a second credit.
    pub(crate) fn reclaim_sidecar_record(base: *mut u8, offset: u32, class: u8) -> bool {
        let off = offset as usize;
        let class_idx = usize::from(class);
        if class_idx >= crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            std::process::abort();
        }
        let kind = SegmentHeader::kind_at(base);
        if !matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            std::process::abort();
        }
        let payload_start = if kind == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        };
        let block_size = SizeClasses::block_size(class_idx);
        let mut meta = SegmentMeta::new(base);
        if off < payload_start
            || !off.is_multiple_of(block_size)
            || off
                .checked_add(block_size)
                .is_none_or(|end| end > meta.bump_of())
        {
            std::process::abort();
        }
        let mut bitmap = meta.alloc_bitmap();
        if bitmap.is_free(offset) {
            return false;
        }
        #[cfg(feature = "fastbin")]
        if meta.magazine_bitmap().is_in_magazine(offset) {
            return false;
        }
        // B3 spike: retire = one bitmap bit + the sub-leaf class-mask bit.
        // No body write, no BinTable head.
        bitmap.mark_free(offset);
        meta.b3_free().set_leaf(
            class_idx,
            off / crate::alloc_core::segment_header::B3_LEAF_BYTES,
        );
        meta.dec_live();
        true
    }
}
