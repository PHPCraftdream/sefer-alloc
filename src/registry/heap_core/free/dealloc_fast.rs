//! AS-1: the own-thread small free accept-only fast lane. It either commits
//! exactly the effects of `small_free_guard`'s `Accept` plus the magazine
//! push, or writes nothing and tail-calls the unchanged chain
//! (`dealloc_routing`), which re-derives everything and classifies the reject.

use crate::alloc_core::live_tag::LiveTag;
use crate::alloc_core::segment_header::{
    Layout as SegmentLayout, SegmentHeader, SegmentKind, SegmentMeta,
};
use crate::alloc_core::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT, SIZE2CLASS};
use crate::registry::heap_core::state::tcache::{FREE_PARK_CAP, TCACHE_CAP};
use crate::registry::heap_core::HeapCore;
use core::alloc::Layout;

// The lane accepts the small kinds only: Primordial (0) and Small (1).
const _: () = assert!(SegmentKind::Primordial as u8 == 0 && SegmentKind::Small as u8 == 1);

impl HeapCore {
    /// Accept an own-thread small free on the tier-1 memo hit, or hand the
    /// free to the unchanged chain. Every exit is a tail call.
    #[inline(always)]
    pub(super) fn dealloc_fast(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, align) = (layout.size(), layout.align());
        // `size - 1` wraps for size 0, which the chain classifies as before.
        let seed = size.wrapping_sub(1) >> MIN_BLOCK_SHIFT;
        // The last bucket is past SMALL_MAX: `class_for` rejects it.
        if align > MIN_BLOCK || seed >= SIZE2CLASS.len() - 1 {
            return lane_miss(ptr, layout, self);
        }
        // The tag's range bounds `c < SMALL_CLASS_COUNT`.
        let tag = LiveTag::of_seed(seed);
        let c = tag.class();
        let key = ptr.addr() & !(crate::alloc_core::os::SEGMENT - 1);
        let (memo_key, root) = self.core.tier1_memo();
        if memo_key != key {
            return lane_miss(ptr, layout, self);
        }
        // `block == ptr` by address, with the table root's provenance; `ptr`
        // is dead from here, so the exits below hold no extra register.
        let off = ptr.addr().wrapping_sub(root.addr());
        let block = root.wrapping_add(off);
        // A Large payload is user data, not state bytes. Below a small kind's
        // payload the state bytes read 0 (И1), which no LIVE tag matches.
        if SegmentHeader::kind_byte_at(root) > SegmentKind::Small as u8 {
            return lane_miss(block, layout, self);
        }
        // A granule start below the bump; the compare reads `bump` in place.
        let meta = SegmentMeta::new(root);
        if off & (MIN_BLOCK - 1) != 0 || off >= meta.bump_of() {
            return lane_miss(block, layout, self);
        }
        let g = off >> MIN_BLOCK_SHIFT;
        let off = off as u32;
        let bytes = meta.state_byte();
        if bytes.read_granule(g) != tag.byte() {
            return lane_miss(block, layout, self);
        }
        // Refinement: the lane accepts only what the chain's guard accepts.
        debug_assert!(
            off as usize
                >= if SegmentHeader::kind_byte_at(root) == SegmentKind::Small as u8 {
                    SegmentLayout::small_meta_end()
                } else {
                    SegmentLayout::primordial_meta_end()
                },
            "AS-1 lane accepted a granule below the payload"
        );
        debug_assert_eq!(
            crate::alloc_core::size_classes::SizeClasses::class_for(size.max(MIN_BLOCK), align),
            Some(c),
            "AS-1 lane class differs from the chain's"
        );
        debug_assert!(
            matches!(
                super::dealloc_own_base::small_free_guard(root, block, c, layout),
                super::dealloc_own_base::SmallFreeGuard::Accept { off: o } if o == off
            ),
            "AS-1 lane accepted a free the guard rejects"
        );
        #[cfg(feature = "bench-internals")]
        crate::alloc_core::AllocCore::note_tier1_hit();
        let cnt = self.tcache.classes[c].count as usize;
        #[cfg(not(feature = "virgin-zero-skip"))]
        let cap = self.tcache.classes[c].park_cap;
        #[cfg(feature = "virgin-zero-skip")]
        let cap = FREE_PARK_CAP[c];
        debug_assert_eq!(cap, FREE_PARK_CAP[c]);
        if cnt >= cap as usize {
            return self.free_magazine_full_fast(c, root, block);
        }
        bytes.write_granule(g, tag.mag_byte());
        // `cnt < FREE_PARK_CAP[c] <= TCACHE_CAP`, so `i == cnt`.
        let i = cnt & (TCACHE_CAP - 1);
        self.tcache.classes[c].slots[i] = block;
        self.tcache.classes[c].count = (i + 1) as u8;
        #[cfg(feature = "virgin-zero-skip")]
        {
            self.tcache.classes[c].virgin_mask &= !(1u16 << cnt);
        }
    }
}

/// The unchanged chain, out of line so the lane keeps no frame. The heap
/// comes last: `ptr` and `layout` stay in the registers they arrived in.
#[cold]
#[inline(never)]
fn lane_miss(ptr: *mut u8, layout: Layout, heap: &mut HeapCore) {
    heap.dealloc_routing(ptr, layout);
}
