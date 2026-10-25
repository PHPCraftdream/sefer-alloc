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
    /// list. `ptr` identifies the block address; `base` is the allocator's
    /// stored segment root.
    ///
    /// **Free guard (M2 — Phase 13.4a history, B4 (в) state-byte form):**
    /// before pushing, we test the segment's per-granule STATE byte for this
    /// block; see the guard body below for the exact compare (the former
    /// per-granule bitmap bit — and before that the Phase 8 O(free-list-
    /// length) `free_list_contains` walk that made own-thread free O(N²)
    /// under churn, #41 — are gone; the byte is the single-writer owner-
    /// private record, so the read/compare needs no atomics).
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
        // past the 16 B-granular state-byte oracle below (it indexes the START
        // granule, whose byte reads "issued") → `write_next` into mid-block →
        // free-list corruption. Rejected here as a no-op. A `%` by a
        // non-power-of-two `block_size` per small free — a paid check, so
        // `hardened`-gated (default OFF), never on the production hot path.
        // Owner-side cross-thread reclamation uses `reclaim_sidecar_record`:
        // it validates payload start, class alignment and checked block end
        // against current bump before payload dereference; invalid geometry aborts.
        #[cfg(feature = "hardened")]
        if !(off as usize).is_multiple_of(SizeClasses::block_size(class_idx)) {
            return;
        }
        // H-1 (UBFIX-3): reject an offset that lands in the segment's OWN
        // metadata region (header / page map / bin table / …) instead of the
        // payload. A caller passing a foreign/corrupt `ptr` whose computed
        // `off` happens to be small and `block_size`-aligned (e.g. `0`) would
        // otherwise sail past every guard below and `write_next` clobbers
        // live segment metadata in place — corrupting the header, state bytes,
        // or bin table. `payload_start` is the compile-time metadata footprint;
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
        // `small_meta_end()` and the state bytes are zeroed. A late free / a
        // legitimate double-free of a block that lived in the now-decommitted
        // payload would (a) pass the zeroed state-byte guard and (b)
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
        // B4 (в) + F2/F15 (review): O(1) EXACT substrate free guard via the
        // state byte. EVERY internal caller of `dealloc_small` passes a
        // currently-issued block START of exactly `class_idx`:
        // `mem_impl.rs::dealloc_at_base` (the user free face — the block
        // reads `encode(class, LIVE)`) and `find_segment/scan.rs`'s
        // `carve_block_with_refill` (freshly carved refill blocks —
        // `carve_block` commits `encode(class_idx, LIVE)` at first issue).
        // The guard therefore requires the exact issued byte
        // `encode(class_idx, LIVE)` plus a 16 B START alignment
        // (`off & (MIN_BLOCK - 1) == 0`; the byte region is indexed
        // `off >> 4`, so an interior offset `p+1..p+15` indexes the START
        // granule and would otherwise read the start's LIVE byte — the same
        // blind spot F1 closed on the magazine guard). This closes, in the
        // non-route-checked substrate core: the M2 double-free (FREE/MAG
        // state), the interior-pointer free (misalignment or the interior
        // granule's И1-zero byte), the never-issued free (zero byte) and the
        // wrong-class free (class-field mismatch) — all benign no-ops. The
        // former state-only-FREE decode (`byte != 0 && byte >> 6 == FREE`,
        // the `dbg_is_free_for` mold) still accepted zero-byte and
        // wrong-class-LIVE operands; the exact compare is strictly stronger,
        // and the accept path is unchanged for every legitimate free.
        let byte = meta.state_byte().read(off);
        let aligned = (off & (crate::alloc_core::size_classes::MIN_BLOCK as u32 - 1)) == 0;
        if !aligned
            || byte
                != crate::alloc_core::state_byte::StateByte::encode(
                    class_idx,
                    crate::alloc_core::state_byte::StateByte::LIVE,
                )
        {
            return; // Not the caller's issued block start: no-op.
        }
        // The user pointer supplied only the numeric offset. The table-owned
        // root covers the physical block, including its free-list word even
        // when the caller's reborrow covered fewer bytes.
        let block_nn = match NonNull::new(Node::deref(base, off as usize)) {
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
        // B4 (b4-provenance, R4): SUBSTRATE bin-ingress total — this is
        // ONE of the physical Small-block bin-entry points (the others:
        // the `flush_run` splice and `reclaim_sidecar_record`); counting
        // every entry site satisfies exactly
        // `SUBSTRATE_PUSHES == own_overflow + own_batch + remote
        //   + swap_top + teardown + OTHER` with OTHER a computed oracle
        // (examples assert OTHER == 0). See
        // `registry/heap_core/diag/counters.rs`. Relaxed, measurement-only,
        // no behavior change. (Note: swap-top eviction and teardown flush
        // reach the bin via `flush_class_internal` → `dealloc_small`, so
        // they are counted BOTH here and in their provenance counters —
        // the attribution converges.) The `registry::heap_core::diag` path
        // needs `alloc-global`, hence the extra feature gate.
        #[cfg(all(feature = "bench-internals", feature = "alloc-global"))]
        crate::registry::heap_core::diag::DEPOT_BIN_INGRESS_SUBSTRATE_PUSHES
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        // B4 (в): commit LIVE → FREE in the state byte (was bitmap mark_free).
        meta.state_byte().write(
            off,
            class_idx,
            crate::alloc_core::state_byte::StateByte::FREE,
        );
        SegmentMeta::new(base).dec_live();
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
        // committed + registered) or releases it (reset + `table.recycle`).
        // Own-thread deallocation holds no route scan borrow; the release leg
        // can recycle the table entry and reservation after the eligibility check.
        #[cfg(feature = "alloc-decommit")]
        if Self::dec_live_and_maybe_decommit(base, self.small_cur) {
            let _ = self.release_or_pool_empty_segment(base);
        }
    }
}
