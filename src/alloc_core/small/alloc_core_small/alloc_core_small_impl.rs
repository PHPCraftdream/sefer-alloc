use crate::alloc_core::node::{Node, NODE_SIZE};
use crate::alloc_core::os::{self, SEGMENT};
// R7-A2: `SegmentHeader::segment_id_at` is consulted here by the
// `alloc-segment-directory`-gated directory-bitmap maintenance arms of
// `pop_free`/`drain_freelist_batch` and, unconditionally, by the Ph3a batch
// commit witnesses (`IssueTransaction::commit_prepared`'s registry index),
// so the import itself is unconditional.
use crate::alloc_core::segment_header::SegmentHeader;
use crate::alloc_core::segment_header::{align_up, SegmentMeta};
// `Layout as SegLayout` is consulted here only by the `alloc-decommit`-gated
// decommit-recommit arms of `carve_block`/`carve_batch` (and only on their
// eager `not(small-segment-lazy-commit)` leg) — gate the import identically
// so non-decommit builds stay warning-clean.
#[cfg(all(feature = "alloc-decommit", not(feature = "small-segment-lazy-commit")))]
use crate::alloc_core::segment_header::Layout as SegLayout;
use crate::alloc_core::segment_header::{B3Free, B3_LEAF_BYTES, B3_LEAF_WORDS};
use crate::alloc_core::size_classes::SizeClasses;

// B3/B2 carve helpers moved verbatim to the `b3_carve` sibling (pure move):
// re-imported here (and re-exported through the existing crate-internal
// re-export chain so external paths like
// `crate::alloc_core::alloc_core_small::LAZY_FIRST_CHUNK` stay valid).
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) use super::b3_carve::{GROW_CHUNK, GROW_COMMIT_COUNT, LAZY_FIRST_CHUNK};
#[cfg(feature = "bench-internals")]
pub(crate) use super::b3_carve::{B3_DRAIN_FROM_FREE, B3P_RUN_BYTES};
#[cfg(all(feature = "bench-internals", feature = "internals"))]
pub(crate) use super::b3_carve::{b3p_leaf_class_of, b3p_run_bytes, b3p_run_leaves};
use super::b3_carve::{
    b3_carve_start, b3_class_block_next_cursor, b3_class_block_plan, b3_locate_free_from,
    b3_slot_offset, B3BlockPlan,
};

use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::segment::segment_table::IssueTransaction;

impl AllocCore {
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_r11_scalar_carve_for_test(&mut self, class_idx: usize) -> Option<*mut u8> {
        if class_idx >= crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            return None;
        }
        self.carve_block(class_idx, SizeClasses::block_size(class_idx))
    }

    /// Current owner state: bump, live credits, class head, commit frontier.
    #[cfg(all(feature = "internals", feature = "bench-internals"))]
    #[doc(hidden)]
    pub fn dbg_r11_issue_state_for_test(
        &self,
        class_idx: usize,
    ) -> Option<(usize, u32, u32, usize)> {
        if class_idx >= crate::alloc_core::size_classes::SMALL_CLASS_COUNT {
            return None;
        }
        let meta = SegmentMeta::new(self.small_cur);
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        let frontier = meta.committed_payload_end_of();
        #[cfg(not(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        )))]
        let frontier = SEGMENT;
        Some((
            meta.bump_of(),
            meta.live_count_of(),
            meta.bin_table().head(class_idx),
            frontier,
        ))
    }

    /// Allocate a small block of the given class. Routes through the current
    /// small segment's free list (pop); on a miss, scans ALL owned segments for
    /// one with a non-empty class free list (Phase 12.1: free state lives in
    /// per-segment `BinTable`s, so a freed block in a non-current segment must
    /// be reusable — otherwise non-current segments leak unboundedly); only
    /// then carves a fresh block / reserves a fresh segment. When carving, also
    /// carves a refill batch (Phase 9 amortisation), pushing each extra block
    /// into its OWN segment's `BinTable` via `segment_base_of` (defect A fix:
    /// never a captured "current" pointer).
    ///
    /// Phase 12.5 (shard model): a heap owns its segments exclusively — there
    /// is no adoption hook. On a free-list miss it carves/reserves from its
    /// OWN segments only. Cross-thread frees arrive via the inline TFS and are
    /// drained by `HeapCore::alloc` BEFORE this runs, so they are already on
    /// the per-segment BinTables by the time we scan.
    #[inline(always)]
    pub(in crate::alloc_core) fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        // 1. Try the free list of the current small segment.
        match self.pop_free(self.small_cur, class_idx) {
            Ok(Some(ptr)) => return ptr,
            Err(()) => return core::ptr::null_mut(),
            Ok(None) => {}
        }
        // 2. Current segment's class free list is empty: scan the OTHER owned
        //    segments for one with a non-empty class free list. A freed block
        //    may live in any segment we own (Phase 12.1 segment-centric free
        //    state); without this scan those blocks would leak. O(segments)
        //    only on a free-list miss — acceptable for 12.1 (per-class
        //    segment queues are a Phase 13 speed optimisation, not a 12.1
        //    deliverable). M5-safe: pure arithmetic + head reads via `Node`,
        //    no allocation.
        // Scalar and refill discovery share the same physical metadata guards.
        // Terminal consumption cannot link a free or magazine-resident block
        // onto the substrate a second time.
        if let Some(seg) = self.find_segment_with_free(class_idx) {
            let ptr = match self.pop_free(seg, class_idx) {
                Ok(Some(ptr)) => ptr,
                Err(()) => return core::ptr::null_mut(),
                Ok(None) => core::ptr::null_mut(),
            };
            if !ptr.is_null() {
                debug_assert!(
                    {
                        let base = os::segment_base_of_ptr(ptr);
                        let off = (ptr as usize - base as usize) as u32;
                        !SegmentMeta::new(base).magazine_bitmap().is_in_magazine(off)
                    },
                    "alloc_small: canonical discovery returned a magazine-resident block",
                );
                return ptr;
            }
        }
        // 3. No free block anywhere: carve a FRESH block. On the cold carve
        //    path we also carve a refill batch (Phase 9 amortisation) so the
        //    next allocs pop from the free list instead of carving one-by-one.
        //    Each refilled block is pushed into its OWN segment's BinTable
        //    (via `segment_base_of(ptr)`), never a captured "current" pointer
        //    — defect A fix: `small_cur` may shift mid-batch when a segment
        //    fills, and a captured pointer would then target the wrong
        //    segment, corrupting its BinTable head.
        if !self.prepare_current_carve(class_idx, block_size) {
            return core::ptr::null_mut();
        }
        if let Some(ptr) = self.carve_block_with_refill(class_idx, block_size) {
            return ptr;
        }
        // 4. Current segment is full: reserve a new small segment and retry.
        match self.reserve_small_segment() {
            Some(_) => {
                // Retry once on the fresh segment. Recurse-free: a single
                // direct retry (not a loop that could grow unboundedly).
                match self.pop_free(self.small_cur, class_idx) {
                    Ok(Some(ptr)) => return ptr,
                    Err(()) => return core::ptr::null_mut(),
                    Ok(None) => {}
                }
                // no-panic: a fresh small segment is guaranteed by construction
                // to have room for at least one block of every small class
                // (compile-time sanity: `small_meta_end() + PAGE <= SEGMENT`,
                // and every class block fits in a page). If carve_block returns
                // None here it indicates metadata corruption; we return null
                // (graceful OOM) rather than panicking — the GlobalAlloc face
                // (Phase 11) must never abort.
                self.carve_block_with_refill(class_idx, block_size)
                    .unwrap_or(core::ptr::null_mut())
            }
            None => {
                // R9-8 (task #230): rescue scan before surfacing OOM. The
                // directory-trust fast path (R8-2) may have wrongly cleared a
                // bit for `class_idx`, hiding a real free block and leading us
                // here to a spurious carve that just OOM'd (table full or OS
                // reservation failure). Run ONE forced O(S) scan ignoring the
                // directory-trust: if it finds a segment the directory missed,
                // self-heal the bit (inside the scan) and serve that block
                // instead of OOMing. Gated on the directory feature + a
                // materialised sidecar (otherwise there was no directory-trust
                // hazard to begin with — the normal path already scanned).
                #[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]
                if !self.directory_sidecar.is_null() {
                    if let Some(seg) = self.find_segment_with_free_forced(class_idx) {
                        #[cfg(feature = "alloc-stats")]
                        crate::alloc_core::directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        match self.pop_free(seg, class_idx) {
                            Ok(Some(ptr)) => return ptr,
                            Err(()) => return core::ptr::null_mut(),
                            Ok(None) => {}
                        }
                    }
                }
                core::ptr::null_mut()
            }
        }
    }

    /// R12-10 (task #261, `virgin-zero-skip`): virgin-aware sibling of
    /// [`alloc_small`](Self::alloc_small), consumed ONLY by
    /// [`AllocCore::alloc_zeroed`]'s small arm and (the production win)
    /// `HeapCore::alloc_zeroed`'s small arm. A separate function (not a
    /// changed `alloc_small` signature) per the design docs' Stage-1
    /// recommendation (`docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §11) --
    /// `alloc_small` has call sites that must stay `*mut u8`-only (the plain
    /// uninitialised-memory `alloc` contract must never observe the virgin
    /// bit).
    ///
    /// Returns `(ptr, is_virgin)`. `is_virgin` is `true` **only** when `ptr`
    /// was served by a bump-cursor CARVE (`carve_block`/`carve_batch`, via
    /// `carve_block_with_refill`) on a segment whose `payload_virgin` bit
    /// reads `true`, AND `cfg!(not(miri))`. Every free-list-pop dispatch
    /// (`pop_free`, own-segment or `find_segment_with_free`) yields `false`
    /// unconditionally — a popped block was, by construction, carved and
    /// issued at some strictly earlier point, so it can never be virgin
    /// regardless of the segment's current bit value (the "dispatch
    /// conjunct" from the design docs' formal predicate, §2 in both).
    ///
    /// The bit is read from `self.small_cur` (or, on the reserve-then-retry
    /// branch, the freshly reserved segment) BEFORE the carve call — this is
    /// safe because `carve_block`/`carve_batch` never WRITE the bit (see the
    /// `SegmentHeader::payload_virgin` field doc's reset table: a carve
    /// leaves the segment's lifetime-virginity bit unchanged), so reading it
    /// immediately before or after a successful carve on the SAME segment
    /// observes the same value.
    #[cfg(feature = "virgin-zero-skip")]
    #[inline]
    pub(crate) fn alloc_small_with_virgin(&mut self, class_idx: usize) -> (*mut u8, bool) {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        // 1. Current segment's free list — never virgin (dispatch conjunct).
        match self.pop_free(self.small_cur, class_idx) {
            Ok(Some(ptr)) => return (ptr, false),
            Err(()) => return (core::ptr::null_mut(), false),
            Ok(None) => {}
        }
        // 2. Other owned segments' free lists — never virgin. Use the same
        // metadata-guarded canonical discovery as ordinary scalar allocation.
        if let Some(seg) = self.find_segment_with_free(class_idx) {
            match self.pop_free(seg, class_idx) {
                Ok(Some(ptr)) => return (ptr, false),
                Err(()) => return (core::ptr::null_mut(), false),
                Ok(None) => {}
            }
        }
        // 3. No free block anywhere: carve a FRESH block from the CURRENT
        //    segment. Read the lifetime-virginity bit before carving — the
        //    carve itself never mutates the bit (see this fn's doc).
        let cur_virgin = SegmentMeta::new(self.small_cur).payload_virgin_of();
        if !self.prepare_current_carve(class_idx, block_size) {
            return (core::ptr::null_mut(), false);
        }
        if let Some(ptr) = self.carve_block_with_refill(class_idx, block_size) {
            return (ptr, cur_virgin && cfg!(not(miri)));
        }
        // 4. Current segment is full: reserve a new small segment and retry.
        match self.reserve_small_segment() {
            Some(_) => {
                // A freshly reserved segment's free lists are always empty
                // (nothing has ever been freed on it) — this pop exists only
                // to mirror `alloc_small`'s identical retry shape; it never
                // actually hits in practice, but if it somehow did, a
                // free-list-served block is never virgin regardless.
                match self.pop_free(self.small_cur, class_idx) {
                    Ok(Some(ptr)) => return (ptr, false),
                    Err(()) => return (core::ptr::null_mut(), false),
                    Ok(None) => {}
                }
                let fresh_virgin = SegmentMeta::new(self.small_cur).payload_virgin_of();
                match self.carve_block_with_refill(class_idx, block_size) {
                    Some(ptr) => (ptr, fresh_virgin && cfg!(not(miri))),
                    None => (core::ptr::null_mut(), false),
                }
            }
            None => {
                // R9-8 rescue scan (mirrors `alloc_small`'s identical arm):
                // any block served here comes from a free-list pop — never
                // virgin.
                #[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]
                if !self.directory_sidecar.is_null() {
                    if let Some(seg) = self.find_segment_with_free_forced(class_idx) {
                        #[cfg(feature = "alloc-stats")]
                        crate::alloc_core::directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        match self.pop_free(seg, class_idx) {
                            Ok(Some(ptr)) => return (ptr, false),
                            Err(()) => return (core::ptr::null_mut(), false),
                            Ok(None) => {}
                        }
                    }
                }
                (core::ptr::null_mut(), false)
            }
        }
    }

    fn prepare_current_carve(&self, class_idx: usize, block_size: usize) -> bool {
        let meta = SegmentMeta::new(self.small_cur);
        let bump = meta.bump_of();
        // Base semantics (preserved): "no room in the current segment" must
        // report OK so the caller proceeds to carve → None → reserve path.
        let aligned = if block_size <= B3_LEAF_BYTES {
            // Per-class run cursor (ADR addendum §3): serve from the class's
            // current run, or plan a fresh k-sub-leaf run from the frontier.
            match b3_class_block_plan(
                class_idx,
                block_size,
                bump,
                meta.b3_free().carve_cursor(class_idx),
            ) {
                Some(plan) => plan.off,
                None => return true,
            }
        } else {
            // > 4 KiB span class: unchanged span-from-bump carve, skipping
            // sub-leaves bound to other classes (see `b3_carve_start`).
            match b3_carve_start(
                &meta.b3_free(),
                class_idx,
                block_size,
                align_up(bump, block_size),
            ) {
                Some(off) => off,
                None => return true,
            }
        };
        self.table
            .prepare_small_issue(self.small_cur, aligned as u32, class_idx)
            .is_some()
    }

    /// Pop a free block; `Err` is an uncommitted class-spill OOM.
    ///
    /// B3 spike: the free set is the alloc bitmap indexed by per-class
    /// sub-leaf masks; the intrusive body `next` chain is gone.
    #[inline(always)]
    fn pop_free(&mut self, segment: *mut u8, class_idx: usize) -> Result<Option<*mut u8>, ()> {
        let mut meta = SegmentMeta::new(segment);
        let bm = meta.alloc_bitmap();
        let b3 = meta.b3_free();
        let Some((leaf, wi, bit)) = b3_locate_free_from(&bm, &b3, class_idx, &mut None) else {
            return Ok(None);
        };
        let off = b3_slot_offset(leaf, wi, bit);
        let Some(tx) = self.table.prepare_small_issue(segment, off, class_idx) else {
            return Err(());
        };
        let mut bm = meta.alloc_bitmap();
        let mut b3 = meta.b3_free();
        bm.clear_word_bit(wi, bit);
        // If the sub-leaf emptied, drop its class-mask bit.
        let w0 = leaf * B3_LEAF_WORDS;
        if bm.word(wi) == 0
            && bm.word(w0) == 0
            && bm.word(w0 + 1) == 0
            && bm.word(w0 + 2) == 0
            && bm.word(w0 + 3) == 0
        {
            b3.clear_leaf(class_idx, leaf);
        }
        #[cfg(feature = "alloc-segment-directory")]
        if b3.class_empty(class_idx) {
            let slot_idx = SegmentHeader::segment_id_at(segment) as usize;
            self.publish_empty(segment, class_idx, slot_idx);
        }
        meta.inc_live();
        tx.commit(&self.table);
        let ptr = Node::deref(segment, off as usize);
        Ok(Some(ptr))
    }

    /// Э7 (task #161) — **batch freelist drain**. Pop up to `out.len()` free
    /// blocks of class `class_idx` from `segment`'s `BinTable[class_idx]` in ONE
    /// walk, writing them into `out[..k]` and returning `k` (the number popped,
    /// `0` if the free list was empty). Byte-identical end-state to calling
    /// [`pop_free`] `k` times, but with the per-block round-trip HOISTED:
    ///
    ///   - `head` is read ONCE (not re-read from the `BinTable` per block).
    ///   - `set_head` is written ONCE at the end, to the first UN-popped node
    ///     (or `FREE_LIST_NULL` if the chain was exhausted before `out` filled).
    ///   - `inc_live` is applied ONCE by `k` (under `alloc-decommit`), exactly
    ///     equalling `k` individual `inc_live`s.
    ///
    /// The two per-block costs that MUST stay per-block are kept per-block:
    ///
    ///   - `read_next(block)` — the dependent load that walks the intrusive
    ///     chain. mimalloc pays this too; there is no way to hoist it (each
    ///     `next` lives in the previous block's body). We never WRITE the block
    ///     body on this path (pop doesn't), so reading `next` before advancing
    ///     is hazard-free: nothing overwrites a block between our read of its
    ///     `next` and our recording it.
    ///   - `mark_alloc(off)` — cleared per-block. **Decision: per-block, NOT
    ///     merged.** A freelist is a LIFO push chain, so consecutive popped
    ///     offsets are in general SCATTERED across the bitmap (they do not share
    ///     a byte the way a flush batch of consecutive carves would). Merging
    ///     the RMWs across blocks would only be byte-identical for offsets that
    ///     share a bitmap byte, which is not guaranteed here — so we keep the
    ///     per-block `mark_alloc`, which is trivially identical to `pop_free`'s.
    ///     The batch win is the hoisted `set_head` / `head`-read / `inc_live`,
    ///     NOT the bitmap RMW (which was never the expensive part).
    ///
    /// ## D1 / M2 / set_head correctness
    ///
    ///   - **D1:** exactly `k` blocks leave the free list and are handed out, so
    ///     `inc_live` by `k` == `k` per-block `inc_live`s. No double, no
    ///     under-count.
    ///   - **M2:** every recorded block ends bitmap-ALLOCATED (bit cleared) via
    ///     its own `mark_alloc`, exactly as `pop_free` leaves it. A later
    ///     double-free still hits `is_free` correctly.
    ///   - **set_head:** after the walk, `head` holds either the offset of the
    ///     first un-popped node (chain longer than `out`) or `FREE_LIST_NULL`
    ///     (chain exhausted). We `set_head` to that once. A subsequent
    ///     `pop_free`/drain therefore yields exactly the remaining blocks in the
    ///     same order.
    ///
    /// R7-A2: `&mut self` (upgraded from `&self`) so the directory bitmap can
    /// be maintained at the single choke point. Touches only `segment` metadata
    /// via `SegmentMeta` for the freelist walk, then calls `publish_empty` on
    /// the directory sidecar (if materialised) when the drain empties the list.
    #[inline]
    pub(in crate::alloc_core) fn drain_freelist_batch(
        &mut self,
        segment: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        self.try_drain_freelist_batch(segment, class_idx, out)
            .unwrap_or(0)
    }

    pub(in crate::alloc_core) fn try_drain_freelist_batch(
        &mut self,
        segment: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> Result<usize, ()> {
        if out.is_empty() {
            return Ok(0);
        }
        let mut meta = SegmentMeta::new(segment);
        let index = SegmentHeader::segment_id_at(segment) as usize;
        {
            let bm = meta.alloc_bitmap();
            let b3 = meta.b3_free();
            // Probe phase (OOM-atomicity, mirrors the old chain probe):
            // prepare every spill WITHOUT consuming any bit. The walk below
            // is deterministic and does not mutate, so the commit walk
            // reproduces the identical slot sequence.
            let mut pos = None;
            let mut k = 0usize;
            while k < out.len() {
                let Some(loc) = b3_locate_free_from(&bm, &b3, class_idx, &mut pos) else {
                    break;
                };
                let off = b3_slot_offset(loc.0, loc.1, loc.2);
                if self
                    .table
                    .prepare_small_issue(segment, off, class_idx)
                    .is_none()
                {
                    return Err(());
                }
                k += 1;
            }
            if k == 0 {
                return Ok(0);
            }
            #[cfg(feature = "bench-internals")]
            B3_DRAIN_FROM_FREE.fetch_add(k as u64, core::sync::atomic::Ordering::Relaxed);
            // Commit walk: consume the same slots in the same order.
            let mut bm = meta.alloc_bitmap();
            let mut b3 = meta.b3_free();
            let mut pos = None;
            let mut k = 0usize;
            while k < out.len() {
                let Some((leaf, wi, bit)) = b3_locate_free_from(&bm, &b3, class_idx, &mut pos)
                else {
                    break;
                };
                let off = b3_slot_offset(leaf, wi, bit);
                bm.clear_word_bit(wi, bit);
                let w0 = leaf * B3_LEAF_WORDS;
                if bm.word(w0) == 0
                    && bm.word(w0 + 1) == 0
                    && bm.word(w0 + 2) == 0
                    && bm.word(w0 + 3) == 0
                {
                    b3.clear_leaf(class_idx, leaf);
                }
                IssueTransaction::commit_prepared(&self.table, index, segment, off, class_idx);
                out[k] = Node::deref(segment, off as usize);
                k += 1;
            }
            #[cfg(feature = "alloc-segment-directory")]
            if b3.class_empty(class_idx) {
                let slot_idx = SegmentHeader::segment_id_at(segment) as usize;
                self.publish_empty(segment, class_idx, slot_idx);
            }
            meta.add_live(k as u32);
            Ok(k)
        }
    }

    /// Carve a fresh `block_size`-aligned block from the current small
    /// segment's bump cursor. Returns None if the segment is full.
    ///
    /// On a page boundary crossing, marks the freshly entered page as owned by
    /// `class_idx` in the page map (the page-dedication rule).
    ///
    /// R12-11 (task #262): `class_idx` is used ONLY for that diagnostic-only
    /// page-map marking (see `PageMap`'s struct doc) — it is genuinely unused
    /// without `page-map-diag`.
    #[cfg_attr(not(feature = "page-map-diag"), allow(unused_variables))]
    pub(in crate::alloc_core::small) fn carve_block(
        &mut self,
        class_idx: usize,
        block_size: usize,
    ) -> Option<*mut u8> {
        let segment = self.small_cur;
        let mut meta = SegmentMeta::new(segment);
        // Field-specific bump read/write (task #33 root-cause fix): the Owner
        // touches ONLY the `bump` field, never the cross-thread-read header
        // fields. A full-struct `write_header` here rewrote `magic`/`kind`/
        // `owner_thread_free` too, racing a Remote's full-struct `read_at` in
        // `dealloc_routing` (the §11 data race). `bump` is owner-only (no
        // Remote reads it), so a plain field write is race-free.
        let bump = meta.bump_of();
        let cursor = meta.b3_free().carve_cursor(class_idx);
        // B3 spike: <= 4 KiB classes serve from the per-class run cursor; the
        // > 4 KiB span path skips sub-leaves bound to other classes (see
        // `b3_carve_start`).
        let plan = if block_size <= B3_LEAF_BYTES {
            b3_class_block_plan(class_idx, block_size, bump, cursor)?
        } else {
            let off = b3_carve_start(
                &meta.b3_free(),
                class_idx,
                block_size,
                align_up(bump, block_size),
            )?;
            B3BlockPlan { off, new_run: None }
        };
        let start_off = plan.off;
        // Fallible prepare before any mutation: `?` is the `return None` the
        // old inline guard performed — "segment full" so the caller reserves.
        let tx = self
            .table
            .prepare_small_issue(segment, start_off as u32, class_idx)?;
        // Phase 35 (M6 recommit): if this segment's payload was decommitted (it
        // emptied and we returned its pages to the OS), we are about to write
        // into the payload — recommit and clear the flag BEFORE the bump cursor
        // advances / the page-map / the block is touched.
        #[cfg(feature = "alloc-decommit")]
        if meta.is_decommitted() {
            // B3 (R7 Workstream B): lazy-commit-aware recommit on reuse.
            //
            // Under `small-segment-lazy-commit` (R12-9, task #260: only THIS
            // sub-feature — the decommit/pool lifecycle a decommitted
            // segment came from is structurally `Small`-only, never
            // `Primordial`, see `dec_live_and_maybe_decommit`), the
            // retain-decommit (B3) only decommitted
            // `[meta_end + LAZY_FIRST_CHUNK, SEGMENT)` and kept the initial
            // chunk `[meta_end, meta_end + LAZY_FIRST_CHUNK)` committed. The
            // frontier was already reset to `meta_end + LAZY_FIRST_CHUNK` by
            // the decommit path. So we do NOT need a recommit syscall here —
            // the first chunk is already committed and ready for carving.
            // Just clear the `decommitted` flag and let B2's grow-on-carve
            // logic recommit additional chunks incrementally as the bump
            // cursor advances past the frontier. This is the lazy savings:
            // reuse never touches the upper payload until it is actually
            // needed.
            //
            // On the eager path (feature-OFF), the full-payload recommit is
            // kept: the decommit decommitted the WHOLE payload
            // `[meta_end, SEGMENT)`, so recommit must bring it all back.
            #[cfg(feature = "small-segment-lazy-commit")]
            {
                // The initial chunk is already committed. The frontier was
                // reset by the decommit path. Just clear the flag.
                meta.set_decommitted(false);
            }
            #[cfg(not(feature = "small-segment-lazy-commit"))]
            {
                if !os::recommit_pages(segment, SegLayout::small_decommit_start(), SEGMENT) {
                    // Honest OOM: the OS refused to re-commit the payload
                    // (commit-charge exhaustion). Do NOT clear `decommitted`
                    // and do NOT advance the bump — writing into the
                    // still-reserved page would fault, and clearing the flag
                    // would poison the segment (future carves would skip
                    // recommit and hit the same uncommitted page). Report
                    // "segment full" so the caller falls back (fresh segment
                    // / null), matching the reserve path.
                    return None;
                }
                meta.set_decommitted(false);
            }
        }
        // B2 (R7 Workstream B): incremental grow-on-carve. If this carve
        // would write past the committed frontier, commit the rounded chunk
        // range `[frontier, round_up(carve_end, GROW_CHUNK))` clamped to
        // SEGMENT. Only AFTER a successful commit do we advance the bump
        // cursor, live_count, page map, and hand out the pointer. On failure
        // everything stays unchanged and we return None ("segment full").
        //
        // No-op on the eager path (committed_payload_end == SEGMENT, so the
        // condition is never true) and on Unix/miri (the eager fallback
        // already committed everything). Fires only on the Windows lazy
        // path when carving past the current frontier. R12-9 (task #260):
        // shared between the split sub-features — `segment` here can be
        // EITHER the primordial segment (reused as the first small-carve
        // target, `AllocCore::new_inner`'s `small_cur = primordial_base`) or
        // an ordinary small segment, and both read/write the same
        // `committed_payload_end` field generically.
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        {
            let frontier = meta.committed_payload_end_of();
            // A fresh run hands the whole k-sub-leaf run to the class at
            // once, so the commit frontier must cover `run_end`, not just
            // the first block.
            let carve_end = match plan.new_run {
                Some((_, run_end)) => run_end,
                None => start_off + block_size,
            };
            if carve_end > frontier {
                // `frontier == SEGMENT` means fully committed (eager path or
                // already grown to the end) — the condition above is always
                // false in that case, so we never reach here on the eager path.
                // Round the carve end UP to the next GROW_CHUNK boundary,
                // clamped to SEGMENT (never commit past the segment end).
                let new_frontier = align_up(carve_end, GROW_CHUNK).min(SEGMENT);
                if !os::commit_pages(segment, frontier, new_frontier) {
                    // Commit-charge exhaustion: cannot grow the frontier.
                    // Report "segment full" so the caller falls back (fresh
                    // segment / null), matching the reserve path. Everything
                    // unchanged: bump not moved, committed_payload_end not
                    // moved, live_count unchanged, page map unwritten.
                    return None;
                }
                GROW_COMMIT_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                meta.set_committed_payload_end(new_frontier);
            }
        }
        // B3 spike: apply the plan AFTER every fallible step — for a fresh
        // run, bind all k sub-leaves and advance the bump to `run_end`; then
        // advance (or clear) the per-class carve cursor. Bump and binding are
        // infallible and must not happen on an OOM path.
        if let Some((run_start, run_end)) = plan.new_run {
            let run_leaf = run_start / B3_LEAF_BYTES;
            let k = B3Free::RUN_LEAVES[class_idx] as usize;
            let mut leaf = run_leaf;
            while leaf < run_leaf + k {
                meta.b3_free().bind_leaf(leaf, class_idx);
                leaf += 1;
            }
            // Update ONLY the bump cursor (owner-only plain field write).
            meta.set_bump(run_end);
            #[cfg(feature = "bench-internals")]
            B3P_RUN_BYTES[class_idx].fetch_add((k * B3_LEAF_BYTES) as u64, core::sync::atomic::Ordering::Relaxed);
        }
        // Only leaf classes own a carve cursor; span classes keep it at 0
        // and keep the old bump semantics (advance by one block).
        if block_size <= B3_LEAF_BYTES {
            let next_cursor = b3_class_block_next_cursor(class_idx, &plan, block_size, cursor);
            meta.b3_free().set_carve_cursor(class_idx, next_cursor);
        } else {
            meta.set_bump(start_off + block_size);
        }
        // Phase 35: this carved block is now live (handed to the caller, or — on
        // the refill path — immediately pushed to the free list, which calls
        // `dealloc_small` → `dec_live`, netting zero for refill blocks; the
        // caller's block keeps the +1). Owner-only counter, plain field bump.
        meta.inc_live();
        // Mark the page containing `aligned_bump` as owned by `class_idx`.
        // R12-11 (task #262): diagnostic-only bookkeeping (`PageMap` is NOT
        // load-bearing for class routing — see its struct doc); gated behind
        // `page-map-diag` and elided from the default/production carve path.
        #[cfg(feature = "page-map-diag")]
        {
            let mut pm = meta.page_map();
            let page = start_off / crate::alloc_core::os::PAGE;
            if pm.class_of(page).is_none() {
                // Page was Free or Meta; dedicate it to this class.
                pm.set_class(page, class_idx);
            }
        }
        let ptr = Node::deref(segment, start_off);
        tx.commit(&self.table);
        Some(ptr)
    }

    /// E1 (task W4) — **batched bump-carve**. Carve a RUN of up to `out.len()`
    /// `block_size`-strided blocks from the current small segment's bump cursor
    /// in ONE shot, writing them into `out[..n]` and returning `n` (0 if the
    /// segment cannot fit even one block — the caller reserves a fresh segment,
    /// exactly as it does on `carve_block` → `None`).
    ///
    /// ## Byte-identical to `n` sequential `carve_block`s — what is HOISTED
    ///
    /// A run of `carve_block(class_idx, block_size)` calls, after the FIRST,
    /// always finds `bump` already `block_size`-aligned (the previous carve left
    /// `bump = aligned_prev + block_size`, and every class `block_size` is a
    /// multiple of `MIN_BLOCK`), so `align_up(bump, block_size)` is a TAUTOLOGY
    /// from the second block on. We therefore align ONCE (`aligned_start`), then
    /// stride by `block_size`. The following are hoisted across the run because
    /// none of them can change mid-run (a carve run touches only owner-only
    /// bump/live/page-map state, and no free/decommit runs between carves):
    ///   - `SegmentMeta::new` + `bump_of()` LOAD — once (bump only advances by
    ///     our own writes; we track it locally).
    ///   - `align_up` div — once (tautological after block 0).
    ///   - `set_bump` STORE — once, to `aligned_start + n*block_size` (identical
    ///     to the last sequential carve's final bump).
    ///   - `live += n` — one batched saturating add (D1: exactly `n` handed out,
    ///     byte-identical to `n` `inc_live`s; owner-only counter, intermediate
    ///     states unobservable — same argument as `drain_freelist_batch`).
    ///   - `is_decommitted()` check + recommit — once at run start (the flag is
    ///     set only in the decommit path, which cannot run mid-carve).
    ///
    /// ## What STAYS per-block (NOT tautologies)
    ///   - The page-map `class_of`/`set_class` "first class wins" marking is
    ///     applied per DISTINCT payload page: we compute the page of each block
    ///     and call `set_class` only when the page index CHANGES from the prior
    ///     block (byte-identical to `carve_block`'s per-block "mark only if
    ///     `class_of(page).is_none()`", since within a run the first block to
    ///     enter a page is the one that dedicates it, and later same-page blocks
    ///     find it already `Some` → no-op). For `block_size > PAGE` every block
    ///     lands on a fresh page, so this degrades to per-block correctly.
    ///
    /// ## M2 / D1 / boundary — preserved EXACTLY
    ///   - M2: carve NEVER touches the alloc bitmap (a bump-carved block is
    ///     already bit0=allocated, the M2 convention) — identical to `carve_block`.
    ///   - D1: `+n` for the `n` blocks handed out.
    ///   - Boundary: `n = min(out.len(), room)` where
    ///     `room = (SEGMENT - aligned_start) / block_size`, so
    ///     `aligned_start + n*block_size <= SEGMENT` — the same
    ///     `aligned + block_size > SEGMENT` per-block check, batched.
    // R12-11 (task #262): `class_idx` is used ONLY for the diagnostic-only
    // page-map marking below (see `PageMap`'s struct doc) — it is genuinely
    // unused without `page-map-diag`.
    #[cfg_attr(not(feature = "page-map-diag"), allow(unused_variables))]
    pub(in crate::alloc_core) fn carve_batch(
        &mut self,
        class_idx: usize,
        block_size: usize,
        out: &mut [*mut u8],
    ) -> usize {
        self.try_carve_batch(class_idx, block_size, out)
            .unwrap_or(0)
    }

    pub(in crate::alloc_core) fn try_carve_batch(
        &mut self,
        class_idx: usize,
        block_size: usize,
        out: &mut [*mut u8],
    ) -> Result<usize, ()> {
        if out.is_empty() {
            return Ok(0);
        }
        let segment = self.small_cur;
        let mut meta = SegmentMeta::new(segment);
        let bump = meta.bump_of();
        let b3 = meta.b3_free();
        // B3 spike: <= 4 KiB classes are planned by the per-class run cursor
        // (`b3_class_block_plan`); the plan is a pure function of the packed
        // cursor + bump, so the probe below advances LOCAL copies and the
        // commit half reproduces the identical sequence. Fresh runs advance
        // the local bump by k*4096; a run may end mid-batch and a new one is
        // taken inside the same batch.
        let is_leaf_class = block_size <= B3_LEAF_BYTES;
        let mut n = 0usize;
        let mut batch_end = bump;
        let (mut lcursor, mut lbump) = (b3.carve_cursor(class_idx), bump);
        let mut ls = align_up(bump, block_size); // span-path scan cursor
        {
            while n < out.len() {
                let plan = if is_leaf_class {
                    match b3_class_block_plan(class_idx, block_size, lbump, lcursor) {
                        Some(p) => p,
                        None => break,
                    }
                } else {
                    match b3_carve_start(&b3, class_idx, block_size, align_up(ls, block_size)) {
                        Some(off) => B3BlockPlan { off, new_run: None },
                        None => break,
                    }
                };
                batch_end = match plan.new_run {
                    Some((_, run_end)) => {
                        lbump = run_end;
                        run_end
                    }
                    None => plan.off + block_size,
                };
                if is_leaf_class {
                    lcursor = b3_class_block_next_cursor(class_idx, &plan, block_size, lcursor);
                } else {
                    ls = plan.off + block_size;
                }
                n += 1;
            }
        }
        if n == 0 {
            return Ok(0); // not room for even one block
        }
        // The commit half re-validates this index against `segment`'s stamped
        // id (witness-checked in `IssueTransaction::commit`); `issue_small` re-derives it.
        let index = SegmentHeader::segment_id_at(segment) as usize;
        // A successful promotion is retained even if later OS commit fails;
        // no class code or issue credit changes until the carve commits.
        {
            let mut s = align_up(bump, block_size);
            let mut lcursor = b3.carve_cursor(class_idx);
            let mut lbump = bump;
            for _ in 0..n {
                let plan = if is_leaf_class {
                    match b3_class_block_plan(class_idx, block_size, lbump, lcursor) {
                        Some(p) => p,
                        None => std::process::abort()
                    }
                } else {
                    let cs = match b3_carve_start(&b3, class_idx, block_size, s) {
                        Some(cs) => cs,
                        None => std::process::abort(),
                    };
                    B3BlockPlan { off: cs, new_run: None }
                };
                if self
                    .table
                    .prepare_small_issue(segment, plan.off as u32, class_idx)
                    .is_none()
                {
                    return Err(());
                }
                if let Some((_, run_end)) = plan.new_run {
                    lbump = run_end;
                } else if !is_leaf_class {
                    s = plan.off + block_size;
                }
                if is_leaf_class {
                    lcursor = b3_class_block_next_cursor(class_idx, &plan, block_size, lcursor);
                }
            }
        }
        // Recommit ONCE at run start if the segment's payload was decommitted
        // (identical to `carve_block`'s per-block check — the flag cannot change
        // mid-run, so one check covers the whole run).
        #[cfg(feature = "alloc-decommit")]
        if meta.is_decommitted() {
            #[cfg(feature = "small-segment-lazy-commit")]
            {
                meta.set_decommitted(false);
            }
            #[cfg(not(feature = "small-segment-lazy-commit"))]
            {
                if !os::recommit_pages(segment, SegLayout::small_decommit_start(), SEGMENT) {
                    // Honest OOM (see `carve_block`): leave the segment marked
                    // decommitted, do not advance the bump, and carve nothing
                    // so the caller falls back (fresh segment / null).
                    return Ok(0);
                }
                meta.set_decommitted(false);
            }
        }
        // B2 grow-on-carve: ONE commit covers the WHOLE batch (end = the last
        // computed block end, sub-leaf skips included).
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        {
            let frontier = meta.committed_payload_end_of();
            if batch_end > frontier {
                let new_frontier = align_up(batch_end, GROW_CHUNK).min(SEGMENT);
                if !os::commit_pages(segment, frontier, new_frontier) {
                    return Ok(0);
                }
                GROW_COMMIT_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                meta.set_committed_payload_end(new_frontier);
            }
        }
        // Commit: replay the same start sequence, bind each fresh run's k
        // sub-leaves, advance the bump per fresh run, advance the per-class
        // carve cursor, batch the live credit. (The span path keeps the old
        // single end-of-batch bump write; run cursors move the bump per run.)
        meta.add_live(n as u32);
        {
            let mut s = align_up(bump, block_size);
            let mut lcursor = b3.carve_cursor(class_idx);
            let mut lbump = bump;
            let mut last_leaf = usize::MAX;
            for slot in out[..n].iter_mut() {
                let plan = if is_leaf_class {
                    match b3_class_block_plan(class_idx, block_size, lbump, lcursor) {
                        Some(p) => p,
                        None => std::process::abort()
                    }
                } else {
                    let cs = match b3_carve_start(&b3, class_idx, block_size, s) {
                        Some(cs) => cs,
                        None => std::process::abort(),
                    };
                    B3BlockPlan { off: cs, new_run: None }
                };
                if let Some((run_start, run_end)) = plan.new_run {
                    let run_leaf = run_start / B3_LEAF_BYTES;
                    let k = B3Free::RUN_LEAVES[class_idx] as usize;
                    let mut leaf = run_leaf;
                    while leaf < run_leaf + k {
                        if leaf != last_leaf {
                            meta.b3_free().bind_leaf(leaf, class_idx);
                            last_leaf = leaf;
                        }
                        leaf += 1;
                    }
                    meta.set_bump(run_end);
                    lbump = run_end;
                    #[cfg(feature = "bench-internals")]
                    B3P_RUN_BYTES[class_idx]
                        .fetch_add((k * B3_LEAF_BYTES) as u64, core::sync::atomic::Ordering::Relaxed);
                }
                if is_leaf_class {
                    lcursor = b3_class_block_next_cursor(class_idx, &plan, block_size, lcursor);
                    meta.b3_free().set_carve_cursor(class_idx, lcursor);
                }
                IssueTransaction::commit_prepared(
                    &self.table,
                    index,
                    segment,
                    plan.off as u32,
                    class_idx,
                );
                *slot = Node::deref(segment, plan.off);
                if !is_leaf_class {
                    s = plan.off + block_size;
                }
            }
            if !is_leaf_class {
                meta.set_bump(batch_end);
            }
        }
        Ok(n)
    }
}
