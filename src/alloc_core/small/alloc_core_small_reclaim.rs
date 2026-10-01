//! Owner-only logical retirement of terminal Small sidecar records.

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment::next_table::NextTable;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
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
        // PG-2 spike: retire WITHOUT touching the block body. The block is
        // marked free only in the off-body `alloc_bitmap`; `bin_table.head`
        // is kept as the non-empty flag.
        // PG-2 spike: the LIFO push writes `next` into the segment's
        // off-body NextTable metadata — the block body is never touched.
        let mut bins = meta.bin_table();
        let old_head = bins.head(class_idx);
        meta.next_table().set_next(
            offset,
            if old_head == FREE_LIST_NULL {
                NextTable::NULL
            } else {
                old_head
            },
        );
        bins.set_head(class_idx, offset);
        bitmap.mark_free(offset);
        meta.dec_live();
        true
    }
}
