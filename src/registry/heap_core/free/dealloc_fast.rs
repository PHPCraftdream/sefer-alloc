//! AS-1: the own-thread small free accept-only fast lane. It either commits
//! exactly the effects of `small_free_guard`'s `Accept` plus the magazine
//! push, or writes nothing and tail-calls the unchanged chain
//! (`dealloc_routing`), which re-derives everything and classifies the reject.

use crate::alloc_core::segment_header::{
    Layout as SegmentLayout, SegmentHeader, SegmentKind, SegmentMeta,
};
use crate::alloc_core::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT, SIZE2CLASS, SMALL_CLASS_COUNT};
use crate::alloc_core::state_byte::StateByte;
use crate::registry::heap_core::state::tcache::{FREE_PARK_CAP, TCACHE_CAP};
use crate::registry::heap_core::HeapCore;
use core::alloc::Layout;

/// Payload start by segment kind (index = `SegmentKind` discriminant).
const PAYLOAD_START: [usize; 2] = [
    SegmentLayout::primordial_meta_end(),
    SegmentLayout::small_meta_end(),
];
const _: () = assert!(SegmentKind::Primordial as usize == 0 && SegmentKind::Small as usize == 1);

impl HeapCore {
    /// Accept an own-thread small free on the tier-1 memo hit, or hand the
    /// free to the unchanged chain. Every exit is a tail call.
    #[inline(always)]
    pub(super) fn dealloc_fast(&mut self, ptr: *mut u8, layout: Layout) {
        let (size, align) = (layout.size(), layout.align());
        // `size - 1` wraps for size 0, which the chain classifies as before.
        let seed = size.wrapping_sub(1) >> MIN_BLOCK_SHIFT;
        if align > MIN_BLOCK || seed >= SIZE2CLASS.len() {
            return self.dealloc_chain(ptr, layout);
        }
        let c = SIZE2CLASS[seed] as usize;
        if c >= SMALL_CLASS_COUNT {
            return self.dealloc_chain(ptr, layout);
        }
        let key = ptr.addr() & !(crate::alloc_core::os::SEGMENT - 1);
        let (memo_key, root) = self.core.tier1_memo();
        if memo_key != key {
            return self.dealloc_chain(ptr, layout);
        }
        // Primordial = 0, Small = 1; Large and Unknown go to the chain.
        let kind = SegmentHeader::kind_byte_at(root) as usize;
        if kind > SegmentKind::Small as usize {
            return self.dealloc_chain(ptr, layout);
        }
        let payload_start = PAYLOAD_START[kind];
        let off = ptr.addr() - root.addr();
        let meta = SegmentMeta::new(root);
        if off < payload_start || off >= meta.bump_of() || off & (MIN_BLOCK - 1) != 0 {
            return self.dealloc_chain(ptr, layout);
        }
        let block = root.wrapping_add(off);
        let off = off as u32;
        let bytes = meta.state_byte();
        if bytes.read(off) != StateByte::encode(c, StateByte::LIVE) {
            return self.dealloc_chain(ptr, layout);
        }
        // Refinement: the lane accepts only what the chain's guard accepts.
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
        if cnt >= FREE_PARK_CAP[c] as usize {
            return self.free_magazine_full_fast(c, cnt, off, root, block);
        }
        bytes.write(off, c, StateByte::MAG);
        self.tcache.classes[c].slots[cnt & (TCACHE_CAP - 1)] = block;
        self.tcache.classes[c].count = (cnt + 1) as u8;
        #[cfg(feature = "virgin-zero-skip")]
        {
            self.tcache.classes[c].virgin_mask &= !(1u16 << cnt);
        }
    }

    /// The unchanged chain, out of line so the lane keeps no frame.
    #[cold]
    #[inline(never)]
    fn dealloc_chain(&mut self, ptr: *mut u8, layout: Layout) {
        self.dealloc_routing(ptr, layout);
    }
}
