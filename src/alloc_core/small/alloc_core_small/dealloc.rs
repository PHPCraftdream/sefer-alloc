//! `AllocCore::dealloc_small` — the own-thread small-block free (M2
//! double-free guard + segment metadata resets; mechanical split of the
//! former flat `alloc_core_small.rs`; pure code movement, no behavior changed).

use core::ptr::NonNull;

use crate::alloc_core::node::Node;
use crate::alloc_core::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
// H1 (task #167): `SizeClasses::block_size` is consulted only by the
// `hardened`-gated interior-pointer guard below — gate the import
// identically so non-hardened builds stay warning-clean.
#[cfg(feature = "hardened")]
use crate::alloc_core::size_classes::SizeClasses;

use crate::alloc_core::alloc_core::AllocCore;

impl AllocCore {
    /// Deallocate a small block: push it onto its owning segment's class free
    /// list. `ptr` is the block address; `base` is its segment base (computed
    /// by the caller via `segment_of`).
    ///
    /// **Double-free guard (M2 — Phase 13.4a):** before pushing, we test the
    /// segment's [`AllocBitmap`](crate::alloc_core::alloc_bitmap::AllocBitmap) bit for this
    /// block. If it is already FREE (`is_free` true → the block is on some free
    /// list of this segment), this is a double-free: we no-op (never corrupt the
    /// free list — no self-loop, no duplicate). Otherwise we set the bit
    /// (`mark_free`) and push. This replaces the Phase 8 O(free-list-length)
    /// `free_list_contains` walk — which made own-thread free O(N²) under churn
    /// (#41) — with an O(1) exact bit test. The bitmap is single-writer (owner
    /// only), so the read/modify/write needs no atomics.
    #[inline(always)]
    pub(in crate::alloc_core) fn dealloc_small(
        &mut self,
        base: *mut u8,
        ptr: *mut u8,
        class_idx: usize,
    ) {
        let meta = SegmentMeta::new(base);
        let mut bt = meta.bin_table();
        let off = (ptr as usize - base as usize) as u32;
        // ── H1 (task #167): interior-pointer guard (HARDENED) ───────────────
        // The SAME guard as `HeapCore::dealloc_own_thread_with_base`'s magazine
        // free path, here on the SUBSTRATE own-thread free — the path the
        // direct `AllocCore` free face (`AllocCore::dealloc_small`) any
        // non-magazine substrate user reaches (the
        // magazine guard only covers the `SeferAlloc` face). A real block start
        // of class `class_idx` sits at an `off` that is a whole multiple of
        // `block_size(class_idx)` (carve aligns the bump to `block_size`); an
        // INTERIOR pointer has `off % block_size != 0` and would otherwise slip
        // past the 16 B-granular `is_free` bitmap oracle below (it maps to a
        // DIFFERENT bit that reads "allocated") → `write_next` into mid-block →
        // free-list corruption. Rejected here as a no-op. A `%` by a
        // non-power-of-two `block_size` per small free — a paid check, so
        // `hardened`-gated (default OFF), never on the production hot path. The
        // CROSS-THREAD leg is already covered UNCONDITIONALLY by
        // `reclaim_offset`'s identical `off % block_size` defence-in-depth.
        #[cfg(feature = "hardened")]
        if !(off as usize).is_multiple_of(SizeClasses::block_size(class_idx)) {
            return;
        }
        // H-1 (UBFIX-3): reject an offset that lands in the segment's OWN
        // metadata region (header / page map / bin table / …) instead of the
        // payload. A caller passing a foreign/corrupt `ptr` whose computed
        // `off` happens to be small and `block_size`-aligned (e.g. `0`) would
        // otherwise sail past every guard below and `write_next` clobbers
        // live segment metadata in place — corrupting the header, bitmap, or
        // bin table. `payload_start` is the compile-time metadata footprint;
        // primordial segments carry the extra registry/hash/free-list
        // regions on top of the small footprint, so they use the larger
        // `primordial_meta_end()`.
        let kind = SegmentHeader::kind_at(base);
        let payload_start = if kind == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        };
        if (off as usize) < payload_start {
            return;
        }
        // Phase 35 (M6 decommit) — the post-decommit stale-free guard. When a
        // segment empties it is decommitted AND reset: `bump` returns to
        // `small_meta_end()` and the alloc bitmap is zeroed. A late free / a
        // legitimate double-free of a block that lived in the now-decommitted
        // payload would (a) pass the zeroed bitmap `is_free` check and (b)
        // `write_next` into a DECOMMITTED / unmapped page — a UAF. Every block
        // that was ever carved has `off >= bump` ONLY after such a reset (a live
        // block in a committed segment always has `off < bump`); so rejecting
        // `off >= bump` closes the window with no false positive on a real free.
        // Owner-only `bump` read (single-writer).
        //
        // M-1 (UBFIX-3): previously `#[cfg(feature = "alloc-decommit")]`-only,
        // so non-decommit builds had NO upper bound — a stale/garbled/foreign
        // `off >= bump` value sailed straight through. Corruption containment
        // must not depend on the decommit feature; unconditional now.
        if (off as usize) >= meta.bump_of() {
            return;
        }
        // O(1) exact double-free guard via the alloc bitmap.
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off) {
            return; // Already on a free list (M2 double-free): no-op.
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off);
        bm.mark_free(off);
        // R7-A2: directory bitmap maintenance — the new head is always non-null
        // (we just pushed `off`), so the only transition is empty→non-empty
        // when old_head was FREE_LIST_NULL.
        #[cfg(feature = "alloc-segment-directory")]
        if old_head == FREE_LIST_NULL {
            let slot_idx = SegmentHeader::segment_id_at(base) as usize;
            self.publish_nonempty(base, class_idx, slot_idx);
        }
        // Phase 35 (M6): one fewer live block in this segment; if it just
        // emptied and is not the current carve target, route it through the
        // Mechanism-2 (task #51) pool/release decision. Own-thread free runs on
        // the owner, so the counter stays single-writer.
        // Task #60 (slot recycle) / Mechanism 2: if the segment emptied,
        // `release_or_pool_empty_segment` either retains it in the pool (kept
        // committed + registered) or releases it (reset + `table.recycle`) —
        // `dealloc_small` is NOT inside a ring drain (no stale ring entries
        // arrive here for `base` on the own-thread path), so on the release
        // branch the metadata is readable, the slot can be NULLed, and the OS
        // reservation can be released right away.
        #[cfg(feature = "alloc-decommit")]
        if Self::dec_live_and_maybe_decommit(base, self.small_cur) {
            self.release_or_pool_empty_segment(base);
        }
    }
}
