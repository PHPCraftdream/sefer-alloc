//! Owner-only logical retirement of terminal Small sidecar records.

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::fatal::fatal;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::node::Node;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::size_classes::SizeClasses;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use core::ptr::NonNull;

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
impl AllocCore {
    /// The owner retains the scan while retiring records and drops it before
    /// reservation finalization. A duplicate or magazine-resident record never
    /// mutates the free list or retires a second credit.
    pub(crate) fn reclaim_sidecar_record(base: *mut u8, offset: u32, class: u8) -> bool {
        let off = offset as usize;
        let class_idx = usize::from(class);
        if class_idx >= crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            fatal(b"alloc_core_small_reclaim: small class index out of range\n");
        }
        let kind = SegmentHeader::kind_at(base);
        if !matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            fatal(b"alloc_core_small_reclaim: sidecar record kind is not small\n");
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
            fatal(b"alloc_core_small_reclaim: sidecar record offset geometry invalid\n");
        }
        let mut bitmap = meta.alloc_bitmap();
        if bitmap.is_free(offset) {
            return false;
        }
        #[cfg(feature = "fastbin")]
        if meta.magazine_bitmap().is_in_magazine(offset) {
            return false;
        }
        let ptr = Node::deref(base, off);
        let mut bins = meta.bin_table();
        let old_head = bins.head(class_idx);
        let next = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        let block = NonNull::new(ptr).unwrap_or_else(|| {
            fatal(b"alloc_core_small_reclaim: reclaimed record resolves to null\n")
        });
        Node::write_next(block, next);
        bins.set_head(class_idx, offset);
        bitmap.mark_free(offset);
        // B4 Шаг S-v: reclaimed into the bin → FREE.
        #[cfg(feature = "small-state-byte")]
        meta.state_byte().write(
            offset,
            class_idx,
            crate::alloc_core::state_byte::StateByte::FREE,
        );
        meta.dec_live();
        true
    }
}
