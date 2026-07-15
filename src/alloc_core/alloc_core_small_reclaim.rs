//! Cross-thread reclaim + ring-drain diagnostics for [`AllocCore`] (mechanical
//! split of `alloc_core_small.rs`, task R4-10).
//!
//! This file holds the `impl AllocCore { .. }` block for the non-intrusive
//! cross-thread reclaim path (`reclaim_offset` / `reclaim_offset_checked`) and
//! the ring-related test hooks (`dbg_push_to_ring`, `dbg_drain_all_rings*`).
//! Pure code-movement sibling of `alloc_core_small.rs`; no behavior changed.

use core::ptr::NonNull;

use super::node::Node;
use super::os;
use super::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
use super::size_classes::SizeClasses;

use super::alloc_core::AllocCore;

impl AllocCore {
    /// Reclaim a cross-thread-freed block identified by its **segment-relative
    /// offset** back into its owning segment's `BinTable`. This is the
    /// non-intrusive reclaim path (Variant-2): the block's offset arrived via
    /// the segment's `RemoteFreeRing` (the block's own bytes were never touched
    /// by the cross-thread freer), so we turn the offset back into a pointer
    /// and route it through the same `dealloc_small` path as an own-thread free.
    ///
    /// **Self-less** (an associated function, not `&mut self`): it touches ONLY
    /// segment metadata reachable from `base` via `SegmentMeta` (header, page
    /// map, bin table) — never the `AllocCore` registry. This lets the
    /// `find_segment_with_free` drain call it while iterating `&self.table`
    /// without an aliasing conflict, and keeps the single-consumer reclaim
    /// uniform with the own-thread path. The caller MUST be the segment's sole
    /// `BinTable` writer (the slot's owner) — the same invariant `dealloc_small`
    /// relies on.
    ///
    /// **Class is carried in the ring entry, NOT derived from `page_map`.** The
    /// segment has ONE bump cursor shared by all size classes, so a page can
    /// host blocks of several classes (the page-dedication rule records only
    /// the FIRST class to touch a page). Deriving the class from `page_map`
    /// therefore returns the wrong class for any later block of a different
    /// class in the same page, and reclaim would link the free-list `next` at a
    /// mis-aligned address, corrupting a neighbour (the §13 root cause). The
    /// cross-thread freer has the original `Layout`, so it packs
    /// `class_idx = classify(layout)` into the high bits of the ring entry;
    /// here we unpack it and use it directly.
    ///
    /// `packed` layout: `off = packed & OFF_MASK` (low 22 bits, since
    /// `SEGMENT = 1 << 22` so every offset is `< 2^22`), `class_idx = packed >>
    /// OFF_BITS` (high bits; `SMALL_CLASS_COUNT` is 49 normally, 55 under
    /// `medium-classes` (R6-OPT-P0-3a) — either way `≪ 2^10`, so it fits; see
    /// `remote_free_ring.rs`'s compile-time pin on this field).
    ///
    /// Safe: a foreign segment (magic mismatch), a large segment, or an offset
    /// that is not `block_size`-aligned is a no-op (defence-in-depth). Applies
    /// the M2 double-free guard.
    /// Task #164: variant of `reclaim_offset` that consults an `is_in_magazine`
    /// predicate AFTER all existing guards and BEFORE `write_next`. If the
    /// predicate returns `true` (block is magazine-resident), the ring entry is
    /// a duplicate free → return `false` without linking (no `write_next`, no
    /// `mark_free`, no `dec_live`). The magazine copy remains the sole canonical
    /// reference. Closes the in-magazine leg of the ring↔magazine cross-thread
    /// double-free residual (task #164, §5 fallback (a)-closure).
    ///
    /// `F` receives `(ptr: *mut u8, class_idx: usize)` and must return `true`
    /// if the block is currently resident in the owner's magazine for that class.
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    #[cfg_attr(not(feature = "alloc-decommit"), allow(unused_variables))]
    pub(crate) fn reclaim_offset_checked<F: Fn(*mut u8, usize) -> bool>(
        base: *mut u8,
        packed: u32,
        small_cur: *mut u8,
        is_in_magazine: &F,
    ) -> bool {
        // X7 Ф3 (task #191): under `hardened` the ring entry was packed with
        // `pack_entry_hardened` (touch (b) in `dealloc_routing`), so it must be
        // unpacked with the matching `unpack_entry_hardened` — which also
        // recovers the stamped generation byte. Under non-hardened the entry
        // was packed with the untouched `pack_entry`, so the unpack is the
        // untouched `unpack_entry` (byte-identical to the pre-X7 code,
        // verified by construction — the `cfg(not)` branch IS the pre-existing
        // code). Sibling-block discipline mirrors `Layout::small_meta_end()`.
        //
        // `stamped_gen` is consulted AFTER all existing guards (the load-bearing
        // ordering: magic/kind/align/bump/is_free/X2-magazine, THEN gen) and
        // BEFORE `write_next` — see the comment below.
        #[cfg(feature = "hardened")]
        let (stamped_gen, class_idx_raw, off_raw) =
            super::remote_free_ring::unpack_entry_hardened(packed);
        #[cfg(not(feature = "hardened"))]
        let (off_raw, class_idx_raw) = super::remote_free_ring::unpack_entry(packed);
        let off = off_raw as usize;
        let class_idx = class_idx_raw as usize;
        if class_idx >= super::size_classes::SMALL_CLASS_COUNT {
            return false;
        }
        let ptr = Node::deref(base, off);
        if SegmentHeader::magic_at(base) != super::segment_header::SEGMENT_MAGIC {
            return false;
        }
        let kind = SegmentHeader::kind_at(base);
        if !matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            return false;
        }
        let bs = SizeClasses::block_size(class_idx) as u32;
        if !(off as u32).is_multiple_of(bs) {
            return false;
        }
        // H-1 (UBFIX-3): reject an offset that lands in the segment's OWN
        // metadata region (header / page map / bin table / …) instead of the
        // payload. Without this guard a garbled/attacker-controlled ring
        // entry with a small, `block_size`-aligned `off` (e.g. `0`) sails
        // past every other guard and `write_next` below clobbers live
        // segment metadata — corrupting the header, bitmap, or bin table
        // in place. `payload_start` is the compile-time metadata footprint,
        // primordial segments carry the extra registry/hash/free-list
        // regions on top of the small footprint, so they use the larger
        // `primordial_meta_end()`.
        let payload_start = if kind == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        };
        if off < payload_start {
            return false;
        }
        let meta = SegmentMeta::new(base);
        // M-1 (UBFIX-3): this guard was previously `#[cfg(feature =
        // "alloc-decommit")]`-only, so builds without that feature had NO
        // upper bound at all — a stale/garbled offset `>= bump` (uncarved
        // payload) would sail through. Corruption containment must not
        // depend on the decommit feature, so the compare is unconditional.
        if off >= meta.bump_of() {
            return false;
        }
        let mut bt = meta.bin_table();
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off as u32) {
            return false;
        }
        // Task #164 (§5 fallback (a)-closure): the block's bitmap reads
        // "allocated". Before linking it onto the freelist (which would
        // clobber its word0 via `write_next`), consult the magazine. If the
        // block IS magazine-resident, this ring entry is the duplicate leg of
        // a cross-thread double-free — DROP it (keep the magazine copy, no
        // link, no mark_free, no dec_live).
        if is_in_magazine(ptr, class_idx) {
            return false;
        }
        // X7 Ф3 (task #191) touch (c): the GENERATIONAL guard. Under
        // `hardened`, AFTER all existing guards (magic/kind/align/bump/
        // is_free/X2-magazine — that ordering is load-bearing, do NOT reorder)
        // and BEFORE `write_next`/`mark_free`: compare the generation stamped
        // in the ring note (touch (b)) against the block's CURRENT generation.
        // A mismatch means the block has been RE-ISSUED since the note was
        // stamped (its life counter advanced via `bump_gen` at the issue pop),
        // so honouring this note would double-free / corrupt the CURRENT
        // occupant — DROP it (return false: no link, no mark_free, no
        // dec_live), exactly like the `is_in_magazine` drop above. This closes
        // the re-issue-before-drain leg (residual leg 3): a note that
        // "survived" a re-issue in the ring is identified by its stale
        // generation and discarded. Compiled ONLY under `hardened`; under
        // non-hardened this block is absent (byte-identical to pre-X7).
        //
        // Wrap 1/256 (X7 §2.5): after 256 re-issues-without-drain a stale note
        // coincides with the current generation and is wrongly honoured — a
        // probabilistic residual accepted by design, pinned by a Ф5 boundary
        // test, not fixable without doubling the ring footprint.
        #[cfg(feature = "hardened")]
        {
            // SAFETY: `base` is a live, exclusively-owned segment; `off` is a
            // MIN_BLOCK-aligned offset of a live block in it.
            #[allow(unsafe_code)]
            let current_gen = unsafe { super::segment_header::gen_at(base, off) };
            if stamped_gen != current_gen {
                return false;
            }
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return false,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off as u32);
        bm.mark_free(off as u32);
        #[cfg(feature = "alloc-decommit")]
        {
            Self::dec_live_and_maybe_decommit(base, small_cur)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        false
    }

    #[cfg(feature = "alloc-xthread")]
    // `small_cur` is consumed only by the `alloc-decommit` dec-then-decommit
    // step; without that feature the reclaim path does no live-count bookkeeping.
    // Under fastbin, `reclaim_offset_checked` is used instead (dead_code expected).
    #[cfg_attr(feature = "fastbin", allow(dead_code))]
    #[cfg_attr(not(feature = "alloc-decommit"), allow(unused_variables))]
    pub(crate) fn reclaim_offset(base: *mut u8, packed: u32, small_cur: *mut u8) -> bool {
        // Unpack the offset and the class the cross-thread freer stamped.
        let (off, class_idx) = super::remote_free_ring::unpack_entry(packed);
        let off = off as usize;
        let class_idx = class_idx as usize;
        // Contract (see this fn's docs: "defence-in-depth against a garbled ring
        // value — no abort, just skip"): the ring entry's class field physically
        // carries 10 bits (0..1023), but only `SMALL_CLASS_COUNT` classes exist.
        // A garbled entry (e.g. a user heap-overflow writing into this segment's
        // metadata region) can present `class_idx >= SMALL_CLASS_COUNT`, which
        // would index `SIZE_CLASS_TABLE` out of bounds in `block_size` below and
        // panic inside the global allocator → process abort. Bounds-check FIRST
        // and no-op (return the skip signal) instead, honouring the no-panic
        // alloc-path discipline.
        if class_idx >= super::size_classes::SMALL_CLASS_COUNT {
            return false;
        }
        let ptr = Node::deref(base, off);
        // Field-specific reads: this runs on the Owner's alloc path
        // (find_segment_with_free's lazy ring drain), concurrent with a
        // Remote's `dealloc_routing` field reads. A full-struct
        // `SegmentHeader::read_at` here would race them; reading individual
        // fields via their offsets touches bytes disjoint from any racing
        // writer, so there is no data race.
        if SegmentHeader::magic_at(base) != super::segment_header::SEGMENT_MAGIC {
            return false;
        }
        let kind = SegmentHeader::kind_at(base);
        if !matches!(kind, SegmentKind::Small | SegmentKind::Primordial) {
            return false;
        }
        // Sanity: the offset must be a whole number of `block_size` units. carve
        // aligns the bump to `block_size`, so a real block offset is always a
        // multiple of its class's block_size. A mis-aligned offset would write
        // the free-list `next` into the middle of a block — the §13 corruption.
        // This never fires for a correctly-packed entry; it is defence-in-depth
        // against a garbled ring value (no abort — just skip, matching the
        // defensive `dealloc` contract).
        let bs = SizeClasses::block_size(class_idx) as u32;
        if !(off as u32).is_multiple_of(bs) {
            return false;
        }
        // H-1 (UBFIX-3): reject an offset in the segment's OWN metadata
        // region (header / page map / bin table / …) rather than payload —
        // see `reclaim_offset_checked`'s identical guard for the full
        // rationale. Primordial segments have the larger footprint (extra
        // registry/hash/free-list regions), so they use
        // `primordial_meta_end()`.
        let payload_start = if kind == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        };
        if off < payload_start {
            return false;
        }
        let meta = SegmentMeta::new(base);
        // Phase 35 (M6 decommit) — the STALE-RING-INTO-DECOMMITTED-SEGMENT guard.
        // When a segment empties it is decommitted AND reset: its `bump` returns
        // to `small_meta_end()` and its alloc bitmap is zeroed. A ring entry that
        // arrives (or lingers) for an offset in the now-decommitted payload would
        // pass the bitmap `is_free` check (the reset cleared every bit), and the
        // reclaim below would `write_next` into a DECOMMITTED page — a UAF / write
        // to unmapped memory. The bump guard closes this: a real, currently-carved
        // block always has `off < bump`; an offset `>= bump` is either uncarved or
        // (post-reset) in the decommitted region — no-op, never touch the page.
        // This is the concrete realization of design §1.3 ("reclaim does a no-op
        // BEFORE touching the block on a stale entry") for the reset bitmap. The
        // owner is the sole `bump` writer, and reclaim runs owner-side, so this
        // field read is consistent (no concurrent bump write).
        //
        // M-1 (UBFIX-3): previously `#[cfg(feature = "alloc-decommit")]`-only,
        // so non-decommit builds had NO upper bound — a stale/garbled
        // `off >= bump` value sailed straight through. Corruption containment
        // must not depend on the decommit feature; unconditional now.
        if off >= meta.bump_of() {
            return false;
        }
        // Inline of `dealloc_small` (self-less): double-free guard + push to
        // BinTable. We cannot call the `&mut self` method from here (this fn is
        // an associated function), so we replicate the body. The replication is
        // small and the invariant is identical.
        let mut bt = meta.bin_table();
        // O(1) exact double-free guard (Phase 13.4a): test the segment's alloc
        // bitmap instead of walking the free list. The owner is the bitmap's
        // sole writer (reclaim runs on the owner — see this fn's docs), so the
        // read/modify/write needs no atomics. Replaces the former inline O(N)
        // `free_list_contains` walk that gave reclaim the same O(N²) regression
        // as own-thread free.
        let mut bm = meta.alloc_bitmap();
        if bm.is_free(off as u32) {
            return false; // Already on a free list (M2 double-free): no-op.
        }
        let block_nn = match NonNull::new(ptr) {
            Some(nn) => nn,
            None => return false,
        };
        let old_head = bt.head(class_idx);
        let old_head_ptr = if old_head == FREE_LIST_NULL {
            core::ptr::null_mut()
        } else {
            Node::deref(base, old_head as usize)
        };
        Node::write_next(block_nn, old_head_ptr);
        bt.set_head(class_idx, off as u32);
        bm.mark_free(off as u32);
        // Phase 35 (M6): a cross-thread-freed block is now back on the free list
        // → one fewer live block. The owner-side drain runs this, so the
        // owner-only counter is single-writer (the cross-thread freer NEVER
        // touched it — it only pushed the offset into the ring). If the segment
        // is now empty AND not the carve target, return its payload to the OS.
        // Returns true if decommit fired (caller should call recycle after drain).
        #[cfg(feature = "alloc-decommit")]
        {
            Self::dec_live_and_maybe_decommit(base, small_cur)
        }
        #[cfg(not(feature = "alloc-decommit"))]
        false
    }

    /// TEST-ONLY: push `ptr`'s segment-relative offset — packed with its
    /// `class_idx` in the high bits — into its segment's `RemoteFreeRing`,
    /// exactly as a cross-thread freer would. Lets a single-threaded test
    /// exercise the ring→reclaim path (which the public own-thread `dealloc`
    /// bypasses) and isolate `reclaim_offset` logic from concurrency. The caller
    /// supplies `class_idx` (the class it allocated the block under) because the
    /// reclaim contract carries the class in the ring entry — the owner must
    /// never re-derive it from `page_map` (the §13 root cause).
    ///
    /// # Safety
    ///
    /// Pushing a ring note is the **producer** side of the cross-thread free
    /// simulation; the note is later consumed by a drain
    /// ([`dbg_drain_all_rings`](Self::dbg_drain_all_rings) / the production
    /// `find_segment_with_free` lazy drain), which reclaims the block back into
    /// its segment's `BinTable` (`write_next` + `set_head` + `mark_free`). The
    /// same reasoning that made [`dealloc`](AllocCore::dealloc)/`realloc`
    /// (R6-MS-1/2) and [`flush_class`](AllocCore::flush_class) (R6-MS-3)
    /// `unsafe fn` applies to this producer: a SAFE entry point accepting a
    /// caller-controlled `ptr` was a soundness gap (round5
    /// `memory_safety_review` R5-MS-4). Fully-safe Rust could push a "remote
    /// free" note for a LIVE block, then `dealloc` it and `alloc`-re-issue the
    /// same address before the drain consumed the stale note — at drain the
    /// block's bitmap reads "allocated" (the re-issue set it), the magazine
    /// predicate is always-false on a bare `AllocCore`, and the generational
    /// guard is compiled out under `production`, so `reclaim_offset` would
    /// `write_next`/`mark_free` the LIVE re-issue, yielding two live owners of
    /// one range — with NO `unsafe` block on the caller side.
    ///
    /// The caller must guarantee:
    ///
    /// - `ptr` is the exact start of a block in a segment owned by THIS
    ///   `AllocCore`. (The function's own `contains_base_ro` check enforces this
    ///   and returns `false` otherwise, so a foreign/recycled-segment pointer is
    ///   a safe no-op rather than UB; this bullet states the obligation for the
    ///   case where the push SUCCEEDS, i.e. a note is actually created.) The
    ///   derived `off = ptr - base` is then a valid in-segment offset.
    /// - This push represents AT MOST ONE logical remote free of `ptr`. Between
    ///   this push and the drain that consumes this note, `ptr` must NOT be
    ///   freed via any other path ([`dealloc`](AllocCore::dealloc) /
    ///   [`flush_class`](AllocCore::flush_class)) and must NOT be re-issued (a
    ///   subsequent [`alloc`](AllocCore::alloc) returning the same address).
    ///   Equivalently: treat `ptr` as consumed by this push (logically freed)
    ///   and do not touch it again until after the drain has processed this note
    ///   and the block is re-issued as a fresh allocation. This is the
    ///   load-bearing obligation: it is what prevents the stale-note→double-issue
    ///   chain above.
    /// - `class_idx` is the block's actual allocated size class, so the note is
    ///   honoured by drain at the correct `block_size`. (`reclaim_offset`
    ///   bounds-checks `class_idx < SMALL_CLASS_COUNT` and rejects an out-of-
    ///   range value as a no-op — defence-in-depth, matching the "garbled ring
    ///   entry" contract documented on `reclaim_offset`; pushing an out-of-range
    ///   class purely to exercise that guard is permitted and is NOT
    ///   memory-unsafe, since such a note causes no free at drain.)
    ///
    /// The drain's own guards (the `is_free` bitmap check, the magazine
    /// predicate under `fastbin`, the generational guard under `hardened`)
    /// degrade several CONTRACT VIOLATIONS benignly as defence-in-depth — but
    /// they are NOT a substitute for honouring this contract, and under the
    /// `production` feature set (no `hardened`, bare-`AllocCore` always-false
    /// magazine predicate) they are insufficient, which is exactly why this
    /// entry point is `unsafe fn`. Under `hardened` this function additionally
    /// reads the block's stamped generation (so the drain's generational guard
    /// compares against a real gen); that read is sound given `ptr` is a live
    /// in-segment block per the first bullet.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    #[allow(unsafe_code)] // R6-MS-4: `unsafe fn` boundary (remote-free-note producer contract).
    pub unsafe fn dbg_push_to_ring(&self, ptr: *mut u8, class_idx: usize) -> bool {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return false;
        }
        let off = (ptr as usize - base as usize) as u32;
        // X7 Ф3 (task #191): mirror `dealloc_routing`'s touch (b). Under
        // `hardened` the drain unpacks with `unpack_entry_hardened`, so the
        // push MUST pack with `pack_entry_hardened` and stamp the current
        // generation — otherwise the gen-check at drain would compare against
        // an unstamped (zero) gen and false-mismatch on every entry. Under
        // non-hardened the untouched `pack_entry` is used (byte-identical).
        // Sibling-block discipline mirrors `dealloc_routing`'s Variant-2 block.
        #[cfg(feature = "hardened")]
        {
            // SAFETY: `base` is a live, exclusively-owned segment; `off` is a
            // MIN_BLOCK-aligned offset of a live block.
            #[allow(unsafe_code)]
            let gen = unsafe { super::segment_header::gen_at(base, off as usize) };
            let packed = super::remote_free_ring::pack_entry_hardened(gen, class_idx as u32, off);
            let ring = SegmentMeta::new(base).remote_ring();
            ring.push(packed).is_ok()
        }
        #[cfg(not(feature = "hardened"))]
        {
            let packed = super::remote_free_ring::pack_entry(off, class_idx as u32);
            let ring = SegmentMeta::new(base).remote_ring();
            ring.push(packed).is_ok()
        }
    }

    /// TEST-ONLY (task #37): drain every owned segment's ring into its
    /// `BinTable`, exactly as `find_segment_with_free` does, but unconditionally.
    #[doc(hidden)]
    #[cfg(feature = "alloc-xthread")]
    pub fn dbg_drain_all_rings(&mut self) {
        // Default: no magazine predicate (non-fastbin callers, or
        // AllocCore-level tests that have no magazine).
        self.dbg_drain_all_rings_impl(&|_, _| false);
    }

    /// Task #164: variant with an explicit magazine predicate, called from
    /// `HeapCore::dbg_drain_all_rings` to exercise the production decision path.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    pub fn dbg_drain_all_rings_checked<F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        is_in_magazine: &F,
    ) {
        self.dbg_drain_all_rings_impl(is_in_magazine);
    }

    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "fastbin"), allow(unused_variables))]
    #[inline]
    fn dbg_drain_all_rings_impl<F: Fn(*mut u8, usize) -> bool>(&mut self, is_in_magazine: &F) {
        // Index-driven scan (task #126), mirroring `find_segment_with_free`:
        // `base_at(i)` is a self-contained read with no borrow tied to the
        // loop, so it can be freely interleaved with `self.table.recycle`
        // below without a pre-collect buffer.
        let n = self.table.count() as usize;
        for i in 0..n {
            let base = self.table.base_at(i);
            if base.is_null() {
                continue;
            }
            let hdr = SegmentHeader::read_at(base);
            if !matches!(hdr.kind, SegmentKind::Small | SegmentKind::Primordial) {
                continue;
            }
            let ring = SegmentMeta::new(base).remote_ring();
            let small_cur = self.small_cur;
            #[cfg(feature = "alloc-decommit")]
            let mut decommit_happened = false;
            ring.drain(|off| {
                #[cfg(feature = "fastbin")]
                let reclaimed = Self::reclaim_offset_checked(base, off, small_cur, is_in_magazine);
                #[cfg(not(feature = "fastbin"))]
                let reclaimed = Self::reclaim_offset(base, off, small_cur);
                #[cfg(feature = "alloc-decommit")]
                if reclaimed {
                    decommit_happened = true;
                }
                #[cfg(not(feature = "alloc-decommit"))]
                {
                    let _ = reclaimed;
                }
            });
            #[cfg(feature = "alloc-decommit")]
            if decommit_happened {
                // Mechanism 2 (task #51): same pool-or-release routing as the
                // production `find_segment_with_free_impl` drain site, so this
                // test seam exercises the identical decision path.
                self.release_or_pool_empty_segment(base);
            }
        }
    }
}
