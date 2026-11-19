//! Owner-only logical retirement of terminal Small sidecar records.

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::fatal::fatal;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::node::Node;
// Pass 9.1: `kind_at`/`seg_meta` are used in `reclaim_sidecar_record` in every
// alloc-global row (e.g. loom without misuse-checks) — import unconditionally.
#[allow(unused_imports)]
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    any(feature = "misuse-checks", debug_assertions)
))]
use crate::alloc_core::segment_header::Layout as SegLayout;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use crate::alloc_core::segment_header::{SegmentKind, FREE_LIST_NULL};
#[cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    any(feature = "misuse-checks", debug_assertions)
))]
use crate::alloc_core::size_classes::SizeClasses;
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use core::ptr::NonNull;
// Pass 9.1: trait methods (`*`) are used unconditionally inside the
// impl, not only under misuse-checks/debug_assertions.
#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
impl AllocCore {
    /// The owner retains the scan while retiring records and drops it before
    /// reservation finalization. A duplicate or magazine-resident record never
    /// mutates the free list or retires a second credit.
    #[allow(unsafe_code)]
    // P2.1 pass 11: facade removal — direct crate unsafe accessors under documented owner-root contracts.
    #[doc(hidden)] // P2.2 cross-crate boundary: called by the root crate's heap_core_xthread synthetic-record probe.
    ///
    /// # Safety
    /// `base` must be the canonical root of a currently mapped Small/Primordial
    /// segment registered in the caller's owning route table (table/route
    /// ownership protocol), with its reservation live for the whole call;
    /// `offset` must address a published terminal sidecar record of that
    /// segment and `class` its issued class. The function itself re-reads the
    /// header kind at `base` and cannot prove the caller's ownership of the
    /// raw root (R25-1), so the owner-liveness obligation stays on the caller.
    pub unsafe fn reclaim_sidecar_record(base: *mut u8, offset: u32, class: u8) -> bool {
        let off = offset as usize;
        let class_idx = usize::from(class);
        if class_idx >= crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            fatal(b"alloc_core_small_reclaim: small class index out of range\n");
        }
        // SAFETY: `base` is the canonical registered segment root resolved under the allocator's table/route ownership protocol; the header stays mapped and live for this read (`SegmentHeader::kind_at` # Safety).
        let kind = unsafe { crate::alloc_core::segment_header::SegmentHeader::kind_at(base) };
        if !matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            fatal(b"alloc_core_small_reclaim: sidecar record kind is not small\n");
        }
        // SAFETY: `base` is the canonical registered segment root resolved under the allocator's table/route ownership protocol; the header stays mapped and live for this read (`SegmentHeader::kind_at` # Safety).
        let mut meta = unsafe { crate::alloc_core::segment_header::SegmentMeta::new(base) };
        #[cfg(any(feature = "misuse-checks", debug_assertions))]
        let (payload_start, block_size) = (
            if kind == SegmentKind::Primordial {
                SegLayout::primordial_meta_end()
            } else {
                SegLayout::small_meta_end()
            },
            SizeClasses::block_size(class_idx),
        );
        #[cfg(any(feature = "misuse-checks", debug_assertions))]
        if off < payload_start
            || !off.is_multiple_of(block_size)
            || off
                .checked_add(block_size)
                .is_none_or(|end| end > meta.bump_of())
        {
            fatal(b"alloc_core_small_reclaim: sidecar record offset geometry invalid\n");
        }
        // B4 (в): state-byte stale-free and magazine-residency checks (the
        // former alloc-bitmap + magazine-bitmap oracle pair) — by state only,
        // class-agnostic like the old bitmap bits.
        #[cfg(any(feature = "misuse-checks", debug_assertions))]
        {
            // SAFETY: The record geometry above bounds the offset below the owner bump
            // (which never exceeds SEGMENT); the owner scan protects initialized metadata.
            let sb = unsafe { meta.state_byte().read(offset) };
            if (sb >> 6) == crate::alloc_core::state_byte::StateByte::FREE {
                crate::alloc_core::state_byte::StateByte::shadow_reject(
                    b"sefer-alloc: misuse check rejected a remote free in safe mode (reclaim FREE)\n",
                );
                return false;
            }
            #[cfg(feature = "fastbin")]
            if (sb >> 6) == crate::alloc_core::state_byte::StateByte::MAG {
                crate::alloc_core::state_byte::StateByte::shadow_reject(
                    b"sefer-alloc: misuse check rejected a remote free in safe mode (reclaim MAG)\n",
                );
                return false;
            }
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
        // B4 (в): commit LIVE → FREE in the state byte.
        // SAFETY: The owner scan supplies a mapped block start and its routed class;
        // the geometry and class bounds above protect the access. Retiring
        // the record commits LIVE -> FREE.
        unsafe {
            meta.state_byte().write(
                offset,
                class_idx,
                crate::alloc_core::state_byte::StateByte::FREE,
            )
        };
        meta.dec_live();
        true
    }
}
