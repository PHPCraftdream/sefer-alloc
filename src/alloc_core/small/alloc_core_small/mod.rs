//! Small-path hot cluster of [`AllocCore`] (mechanical split of
//! `alloc_core.rs`).
//!
//! This file holds the `impl AllocCore { .. }` block for the small-object
//! alloc / dealloc / carve / segment-reserve hot path. The cross-thread
//! reclaim, magazine batch, and diagnostics blocks live in their sibling
//! files (`alloc_core_small_reclaim`, `alloc_core_small_magazine`,
//! `alloc_core_small_diag`). Pure code-movement; no behavior changed.

use core::ptr::NonNull;

use crate::alloc_core::node::{Node, NODE_SIZE};
use crate::alloc_core::os::{self, SEGMENT};
// R7-A2: `SegmentHeader::segment_id_at` is consulted here only by the
// `alloc-segment-directory`-gated directory-bitmap maintenance arms of
// `pop_free`/`drain_freelist_batch` — gate the import identically so
// non-directory builds stay warning-clean.
#[cfg(feature = "alloc-segment-directory")]
use crate::alloc_core::segment_header::SegmentHeader;
use crate::alloc_core::segment_header::{align_up, SegmentMeta, FREE_LIST_NULL};
// `Layout as SegLayout` is consulted here only by the `alloc-decommit`-gated
// decommit-recommit arms of `carve_block`/`carve_batch` (and only on their
// eager `not(small-segment-lazy-commit)` leg) — gate the import identically
// so non-decommit builds stay warning-clean.
#[cfg(all(feature = "alloc-decommit", not(feature = "small-segment-lazy-commit")))]
use crate::alloc_core::segment_header::Layout as SegLayout;
use crate::alloc_core::size_classes::SizeClasses;

use crate::alloc_core::alloc_core::AllocCore;

// Mechanical-split siblings of the former flat `alloc_core_small.rs`.
// `directory` keeps a `cfg` gate here (not only on its items) because the
// WHOLE file is `alloc-segment-directory`-gated content: gating the
// declaration keeps its compiled-file set identical to when the code lived
// directly in `alloc_core_small.rs` (same intrinsic-gate discipline as
// `platform::numa`/`platform::dirty_by_class`).
mod dealloc;
#[cfg(feature = "alloc-segment-directory")]
mod directory;
mod find_segment;
mod reserve;

/// B2 (R7 Workstream B): process-wide count of successful `commit_pages` calls
/// on the grow-on-carve path. Diagnostic-only (relaxed), gated on
/// `any(primordial-lazy-commit, small-segment-lazy-commit)` (R12-9, task
/// #260: the split siblings of the former single `alloc-lazy-commit`, which
/// is now a pure alias for "both together"). Tests observe this to verify
/// that `carve_batch` does ONE commit per batch (not per block) and that
/// chunk boundary crossings trigger the expected number of commits.
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) static GROW_COMMIT_COUNT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// B1 (R7 Workstream B): the size of the FIRST committed payload chunk when a
/// small segment is lazily reserved under `small-segment-lazy-commit`, or
/// the primordial segment under `primordial-lazy-commit` (R12-9, task #260 —
/// this constant is SHARED verbatim by both split policies; see
/// `bootstrap::primordial`'s identical `LAZY_FIRST_CHUNK` use). Only the
/// metadata region `[0, small_meta_end)` plus this chunk are committed at
/// reservation time; the rest of the 4 MiB segment stays reserved-but-
/// uncommitted. B2's grow-on-carve logic commits additional chunks
/// (sized by [`GROW_CHUNK`]) as the bump cursor advances past the frontier.
///
/// 256 KiB is large enough to hold many initial carve batches without faulting
/// (the refill batch is 31 blocks; even at the largest small class of ~8 KiB
/// that is ~248 KiB, comfortably within one chunk). B5 will sweep this value
/// (64/128/256/512 KiB) against the first-heap commit judge; 256 KiB is the
/// conservative default.
///
/// The value MUST be a non-zero multiple of `aligned_vmem::PAGE` (4 KiB).
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) const LAZY_FIRST_CHUNK: usize = 256 * 1024;

/// B2 (R7 Workstream B): the chunk size used when GROWING the commit frontier
/// past its initial value during bump-carve. When a `carve_block` or
/// `carve_batch` would write past `committed_payload_end`, the grow logic
/// commits `[frontier, round_up(carve_end, GROW_CHUNK))` (clamped to
/// `SEGMENT`) and advances the frontier.
///
/// Set equal to `LAZY_FIRST_CHUNK` (256 KiB): both are B5-swept constants
/// (64/128/256/512 KiB); using the same value for initial and grow chunks
/// keeps the model simple and the sweep surface small. A separate value is
/// a trivial constant rename if B5 data motivates it.
///
/// The value MUST be a non-zero multiple of `aligned_vmem::PAGE` (4 KiB).
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
pub(crate) const GROW_CHUNK: usize = LAZY_FIRST_CHUNK;

// B1/B2: compile-time sanity — the initial commit (metadata + first chunk) must
// fit within one segment, and both chunk constants must be page-aligned and
// non-zero.
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
const _: () = {
    assert!(
        LAZY_FIRST_CHUNK > 0 && LAZY_FIRST_CHUNK.is_multiple_of(crate::alloc_core::os::PAGE),
        "LAZY_FIRST_CHUNK must be a non-zero multiple of PAGE"
    );
    // Task #1074: the page-rounded `lazy_initial_commit` adds at most one
    // MAX_REALISTIC_PAGE_SIZE (64 KiB) of rounding on top of the tight sum,
    // so the slack is part of the bound: this pins the ROUNDED initial
    // commit within one SEGMENT for every page size this crate supports.
    assert!(
        crate::alloc_core::segment_header::Layout::small_meta_end()
            + LAZY_FIRST_CHUNK
            + crate::alloc_core::os::MAX_REALISTIC_PAGE_SIZE
            <= crate::alloc_core::os::SEGMENT,
        "metadata + LAZY_FIRST_CHUNK + one MAX_REALISTIC_PAGE_SIZE of page-rounding \
         slack must fit within one SEGMENT"
    );
    assert!(
        GROW_CHUNK > 0 && GROW_CHUNK.is_multiple_of(crate::alloc_core::os::PAGE),
        "GROW_CHUNK must be a non-zero multiple of PAGE"
    );
};

impl AllocCore {
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
        if let Some(ptr) = self.pop_free(self.small_cur, class_idx) {
            return ptr;
        }
        // 2. Current segment's class free list is empty: scan the OTHER owned
        //    segments for one with a non-empty class free list. A freed block
        //    may live in any segment we own (Phase 12.1 segment-centric free
        //    state); without this scan those blocks would leak. O(segments)
        //    only on a free-list miss — acceptable for 12.1 (per-class
        //    segment queues are a Phase 13 speed optimisation, not a 12.1
        //    deliverable). M5-safe: pure arithmetic + head reads via `Node`,
        //    no allocation.
        // UBFIX-8 (M-4 audit finding, docs/reviews/2026-07-10-ub-audit-final-
        // synthesis.md): this scan uses the UNCHECKED `find_segment_with_free`
        // (no magazine-membership predicate), unlike the production fastbin
        // refill path (`refill_class_bump_impl`), which passes
        // `find_segment_with_free_checked` guarded by an `is_in_magazine`
        // closure so a magazine-resident block is never handed out a second
        // time via the free-list drain.
        //
        // Reachability analysis (traced, not assumed): `alloc_small` has
        // exactly two callers in this crate — `AllocCore::alloc` (the plain
        // substrate entry point) and the legacy `refill_class` (test-only;
        // grep confirms its only non-doc callers are `tests/alloc_core_batch.rs`
        // / `tests/regression_batch_flush.rs`, never `HeapCore`). The ONLY
        // production entry point is `SeferAlloc::alloc` → `HeapCore::alloc`.
        // `HeapCore::alloc` gates its magazine block on
        // `#[cfg(all(feature = "alloc-global", feature = "fastbin"))]`; inside
        // that block, EVERY small class (`class_for` returns `Some`) is routed
        // through the magazine (hit → return, miss → `refill_magazine_slow` →
        // `refill_class_bump_checked`, the CHECKED variant) and returns before
        // reaching `self.core.alloc(layout)`. `self.core.alloc` — the only path
        // that reaches `AllocCore::alloc_small` — is taken ONLY when `class` is
        // `None` (a Large request) whenever fastbin is compiled in, i.e. this
        // step-2 scan never runs for a small class in a fastbin build. And
        // `fastbin = ["alloc-global", "alloc-xthread"]` in Cargo.toml — feature
        // unification means fastbin is NEVER active without a magazine also
        // being wired in. In builds WITHOUT fastbin there is no magazine at all
        // (the `mark_magazine`/`clear_magazine` call sites all live inside the
        // fastbin-gated code in `heap_core.rs`), so no block can be
        // magazine-resident there either. Net: whenever this scan can reach a
        // magazine-tagged block, it cannot run; whenever it runs, no block is
        // magazine-tagged. Pinned below so a future refactor that opens a path
        // from `HeapCore`'s fastbin magazine block into `AllocCore::alloc_small`
        // fails loudly under any debug-assertions build instead of silently
        // double-issuing a block.
        if let Some(seg) = self.find_segment_with_free(class_idx) {
            if let Some(ptr) = self.pop_free(seg, class_idx) {
                debug_assert!(
                    {
                        let base = os::segment_base_of_ptr(ptr);
                        let off = (ptr as usize - base as usize) as u32;
                        !SegmentMeta::new(base).magazine_bitmap().is_in_magazine(off)
                    },
                    "alloc_small: find_segment_with_free (unchecked) returned a \
                     magazine-resident block — this path was believed unreachable \
                     under fastbin (see the doc comment above); a refactor has \
                     opened a double-issue hazard",
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
        if let Some(ptr) = self.carve_block_with_refill(class_idx, block_size) {
            return ptr;
        }
        // 4. Current segment is full: reserve a new small segment and retry.
        match self.reserve_small_segment() {
            Some(_) => {
                // Retry once on the fresh segment. Recurse-free: a single
                // direct retry (not a loop that could grow unboundedly).
                if let Some(ptr) = self.pop_free(self.small_cur, class_idx) {
                    return ptr;
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
                        if let Some(ptr) = self.pop_free(seg, class_idx) {
                            return ptr;
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
        if let Some(ptr) = self.pop_free(self.small_cur, class_idx) {
            return (ptr, false);
        }
        // 2. Other owned segments' free lists — never virgin (dispatch
        //    conjunct). Same unchecked scan `alloc_small` uses; see that
        //    function's doc for the reachability argument (this substrate
        //    entry point is reached only outside the fastbin magazine, same
        //    as `alloc_small` itself).
        if let Some(seg) = self.find_segment_with_free(class_idx) {
            if let Some(ptr) = self.pop_free(seg, class_idx) {
                return (ptr, false);
            }
        }
        // 3. No free block anywhere: carve a FRESH block from the CURRENT
        //    segment. Read the lifetime-virginity bit before carving — the
        //    carve itself never mutates the bit (see this fn's doc).
        let cur_virgin = SegmentMeta::new(self.small_cur).payload_virgin_of();
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
                if let Some(ptr) = self.pop_free(self.small_cur, class_idx) {
                    return (ptr, false);
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
                        if let Some(ptr) = self.pop_free(seg, class_idx) {
                            return (ptr, false);
                        }
                    }
                }
                (core::ptr::null_mut(), false)
            }
        }
    }

    /// Pop a free block of `class_idx` from `segment`'s bin table. Returns
    /// null if the free list is empty. Writes the block's `next` word to null
    /// (it becomes the new head) via the node seam.
    #[inline(always)]
    fn pop_free(&mut self, segment: *mut u8, class_idx: usize) -> Option<*mut u8> {
        #[cfg(feature = "alloc-decommit")]
        let mut meta = SegmentMeta::new(segment);
        #[cfg(not(feature = "alloc-decommit"))]
        let meta = SegmentMeta::new(segment);
        let mut bt = meta.bin_table();
        let head_off = bt.head(class_idx);
        if head_off == FREE_LIST_NULL {
            return None;
        }
        let block_ptr = Node::deref(segment, head_off as usize);
        let block_nn = NonNull::new(block_ptr)?;
        let next = Node::read_next(block_nn);
        // UBFIX-7 (M-3, `docs/reviews/2026-07-10-ub-audit-final-synthesis.md`):
        // the intrusive freelist `next` word lives INSIDE the block itself, so
        // it is writable by the user for as long as the block is (legitimately
        // or via a use-after-free) in their hands. Before this guard, a
        // corrupted `next` — e.g. left over from a UAF write into an
        // already-freed block — was trusted unconditionally: the very next
        // line turned it into a segment-relative offset via raw pointer
        // subtraction, which is only sound if `next` actually lies inside
        // `segment`. A `next` pointing outside the segment produces a garbage
        // `u32` offset (wrapping/overflowing arithmetic), and the NEXT
        // `pop_free`/`drain_freelist_batch` call derefs THAT offset via
        // `Node::deref` (`segment.add(off)`), an out-of-bounds `add` — UB per
        // `node.rs`'s SAFETY contract — and hands the caller a wild pointer
        // dressed up as a legitimate block.
        //
        // `hardened`-gated (mimalloc `MI_SECURE`-style): validate `next` is
        // either null or resolves to THIS segment's base before trusting it as
        // a chain continuation; a mismatch TRUNCATES the chain here (treated
        // as `FREE_LIST_NULL`) rather than being dereferenced. This never runs
        // on the production (non-hardened) hot path — zero added instructions
        // there, byte-identical to the pre-fix code under `cfg(not(hardened))`.
        #[cfg(feature = "hardened")]
        let next = if next.is_null() || os::segment_base_of_ptr(next) == segment {
            next
        } else {
            core::ptr::null_mut()
        };
        let new_head = if next.is_null() {
            FREE_LIST_NULL
        } else {
            // Compute the offset of `next` relative to this segment. `next`
            // is an absolute pointer into the same segment (free lists are
            // per-segment), so offset = next - segment.
            (next as usize - segment as usize) as u32
        };
        bt.set_head(class_idx, new_head);
        // R7-A2: directory bitmap maintenance — the old head was non-null
        // (we passed the FREE_LIST_NULL guard above), so the only transition
        // is non-empty→empty when new_head is FREE_LIST_NULL.
        #[cfg(feature = "alloc-segment-directory")]
        if new_head == FREE_LIST_NULL {
            let slot_idx = SegmentHeader::segment_id_at(segment) as usize;
            self.publish_empty(segment, class_idx, slot_idx);
        }
        // Phase 13.4a: clear the block's bitmap bit — it leaves the free list
        // and is handed to the caller, so a subsequent free must NOT see it as
        // already-free (and the next legitimate free must be able to re-mark it).
        meta.alloc_bitmap().mark_alloc(head_off);
        // Phase 35 (M6): a block left the free list and is handed to the caller
        // → one more live block in this segment. Owner-only counter. A popped
        // block always comes from a COMMITTED payload (a decommitted segment was
        // reset to an empty free list, so `pop_free` finds nothing there), so no
        // recommit is needed on this path — only `carve_block` writes fresh
        // payload and thus recommits.
        #[cfg(feature = "alloc-decommit")]
        meta.inc_live();
        // X7 Ф3 (task #191) touch (a): bump the generation at ISSUE. `pop_free`
        // hands a block directly to the caller (it is the non-magazine substrate
        // pop, reachable from `alloc_small`). Under `hardened` (which implies
        // `fastbin`), `HeapCore::alloc` routes small blocks through the magazine
        // and never reaches here — but a direct `AllocCore` consumer (or a future
        // config change) could, so the bump is placed at this issue point for
        // correctness and defense-in-depth. The magazine refill path uses
        // `drain_freelist_batch` (which fills `out`, NOT issuing to a caller), so
        // blocks pulled into the magazine are NOT bumped here — they are bumped
        // on their later magazine pop. Compiled ONLY under `hardened`.
        #[cfg(feature = "hardened")]
        {
            // SAFETY: `segment` is a live, exclusively-owned segment;
            // `head_off` is a MIN_BLOCK-aligned offset of a live block.
            #[allow(unsafe_code)]
            unsafe {
                crate::alloc_core::segment_header::bump_gen(segment, head_off as usize)
            };
        }
        Some(block_ptr)
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
        if out.is_empty() {
            return 0;
        }
        #[cfg(feature = "alloc-decommit")]
        let mut meta = SegmentMeta::new(segment);
        #[cfg(not(feature = "alloc-decommit"))]
        let meta = SegmentMeta::new(segment);
        let mut bt = meta.bin_table();

        {
            // Read the head ONCE.
            let mut head_off = bt.head(class_idx);
            if head_off == FREE_LIST_NULL {
                return 0;
            }
            let mut bm = meta.alloc_bitmap();
            let mut k = 0usize;
            while k < out.len() && head_off != FREE_LIST_NULL {
                let block_ptr = Node::deref(segment, head_off as usize);
                let block_nn = match NonNull::new(block_ptr) {
                    Some(nn) => nn,
                    // A null-deref would only arise from a corrupt offset; stop the
                    // walk here and commit what we have (defence-in-depth). `head`
                    // is left pointing at this node so nothing is lost.
                    None => break,
                };
                // Dependent load: read this block's `next` BEFORE recording it. The
                // block body is never written on the pop path, so this is race-free
                // against ourselves.
                let next = Node::read_next(block_nn);
                // UBFIX-7 (M-3): validate `next` before trusting it as a chain
                // continuation — see `pop_free`'s identical guard for the full
                // rationale. `hardened`-gated; a mismatch truncates the chain
                // (this iteration's `head_off` becomes NULL below, which the
                // loop condition then exits on) instead of being dereferenced.
                #[cfg(feature = "hardened")]
                let next = if next.is_null() || os::segment_base_of_ptr(next) == segment {
                    next
                } else {
                    core::ptr::null_mut()
                };
                // Clear this block's bitmap bit — it leaves the free list and is
                // handed out (per-block, byte-identical to `pop_free`).
                bm.mark_alloc(head_off);
                out[k] = block_ptr;
                k += 1;
                head_off = if next.is_null() {
                    FREE_LIST_NULL
                } else {
                    // `next` is an absolute pointer into the SAME segment (free
                    // lists are per-segment), so offset = next - segment.
                    (next as usize - segment as usize) as u32
                };
            }
            // Write the new head ONCE: the first un-popped node, or NULL.
            bt.set_head(class_idx, head_off);
            // R7-A2: directory bitmap maintenance — the old head was non-null
            // (early return above), so the only transition is non-empty→empty
            // when the drain exhausted the chain (head_off == FREE_LIST_NULL).
            #[cfg(feature = "alloc-segment-directory")]
            if head_off == FREE_LIST_NULL {
                let slot_idx = SegmentHeader::segment_id_at(segment) as usize;
                self.publish_empty(segment, class_idx, slot_idx);
            }
            // `inc_live` ONCE by `k` (D1): exactly `k` blocks were handed out. A
            // popped block always comes from a COMMITTED payload (a decommitted
            // segment was reset to an empty free list, so the drain finds nothing
            // there), so no recommit is needed on this path. Applied via the
            // batch `add_live(k)` primitive (byte-identical to `k` per-block
            // `inc_live`s — see `add_live`'s D1-equivalence note).
            #[cfg(feature = "alloc-decommit")]
            meta.add_live(k as u32);
            k
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
    pub(super) fn carve_block(&mut self, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        let segment = self.small_cur;
        let mut meta = SegmentMeta::new(segment);
        // Field-specific bump read/write (task #33 root-cause fix): the Owner
        // touches ONLY the `bump` field, never the cross-thread-read header
        // fields. A full-struct `write_header` here rewrote `magic`/`kind`/
        // `owner_thread_free` too, racing a Remote's full-struct `read_at` in
        // `dealloc_routing` (the §11 data race). `bump` is owner-only (no
        // Remote reads it), so a plain field write is race-free.
        let bump = meta.bump_of();
        let aligned_bump = align_up(bump, block_size);
        if aligned_bump + block_size > SEGMENT {
            return None;
        }
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
            let carve_end = aligned_bump + block_size;
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
        // Update ONLY the bump cursor.
        meta.set_bump(aligned_bump + block_size);
        // Phase 35: this carved block is now live (handed to the caller, or — on
        // the refill path — immediately pushed to the free list, which calls
        // `dealloc_small` → `dec_live`, netting zero for refill blocks; the
        // caller's block keeps the +1). Owner-only counter, plain field bump.
        #[cfg(feature = "alloc-decommit")]
        meta.inc_live();
        // Mark the page containing `aligned_bump` as owned by `class_idx`.
        // R12-11 (task #262): diagnostic-only bookkeeping (`PageMap` is NOT
        // load-bearing for class routing — see its struct doc); gated behind
        // `page-map-diag` and elided from the default/production carve path.
        #[cfg(feature = "page-map-diag")]
        {
            let mut pm = meta.page_map();
            let page = aligned_bump / crate::alloc_core::os::PAGE;
            if pm.class_of(page).is_none() {
                // Page was Free or Meta; dedicate it to this class.
                pm.set_class(page, class_idx);
            }
        }
        let ptr = Node::deref(segment, aligned_bump);
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
        if out.is_empty() {
            return 0;
        }
        let segment = self.small_cur;
        let mut meta = SegmentMeta::new(segment);
        let bump = meta.bump_of();
        let aligned_start = align_up(bump, block_size);
        if aligned_start + block_size > SEGMENT {
            return 0; // not room for even one block
        }
        // Recommit ONCE at run start if the segment's payload was decommitted
        // (identical to `carve_block`'s per-block check — the flag cannot change
        // mid-run, so one check covers the whole run).
        #[cfg(feature = "alloc-decommit")]
        if meta.is_decommitted() {
            // B3: lazy-commit-aware recommit on reuse (see `carve_block`'s
            // identical block for the full rationale). R12-9 (task #260):
            // `small-segment-lazy-commit` specifically — decommit only ever
            // happens to `Small` segments, never `Primordial`.
            #[cfg(feature = "small-segment-lazy-commit")]
            {
                meta.set_decommitted(false);
            }
            #[cfg(not(feature = "small-segment-lazy-commit"))]
            {
                if !os::recommit_pages(segment, SegLayout::small_decommit_start(), SEGMENT) {
                    // Honest OOM (see `carve_block`): leave the segment marked
                    // decommitted, do not advance the bump, and carve nothing
                    // so the caller falls back (fresh segment / null) instead
                    // of writing into a still-reserved page.
                    return 0;
                }
                meta.set_decommitted(false);
            }
        }
        // B2 (R7 Workstream B): incremental grow-on-carve for the batched
        // path. ONE commit covers the WHOLE batch — not a syscall per block.
        // Compute the batch's final end, commit once up to the rounded chunk
        // boundary that covers it, then carve the batch. On commit failure
        // everything stays unchanged and we return 0.
        //
        // No-op on the eager path (committed_payload_end == SEGMENT — the
        // condition is always false). Fires only on the Windows lazy path.
        // R12-9 (task #260): shared between the split sub-features, same as
        // `carve_block`'s identical block.
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        {
            let frontier = meta.committed_payload_end_of();
            let batch_room = (SEGMENT - aligned_start) / block_size;
            let batch_n = out.len().min(batch_room);
            let batch_end = aligned_start + batch_n * block_size;
            if batch_end > frontier {
                // Round the batch end UP to the next GROW_CHUNK boundary,
                // clamped to SEGMENT (never commit past the segment end).
                let new_frontier = align_up(batch_end, GROW_CHUNK).min(SEGMENT);
                if !os::commit_pages(segment, frontier, new_frontier) {
                    // Commit-charge exhaustion: cannot grow the frontier.
                    // Everything unchanged: bump not moved, live_count
                    // unchanged, page map unwritten, no blocks handed out.
                    return 0;
                }
                GROW_COMMIT_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                meta.set_committed_payload_end(new_frontier);
            }
        }
        // How many blocks fit from `aligned_start` to the segment end, capped by
        // the caller's slice.
        let room = (SEGMENT - aligned_start) / block_size;
        let n = out.len().min(room);
        // Advance the bump cursor ONCE to just past the last carved block —
        // byte-identical to the final `set_bump` of the n-th sequential carve.
        meta.set_bump(aligned_start + n * block_size);
        // Batched live increment (D1): exactly `n` blocks handed out.
        #[cfg(feature = "alloc-decommit")]
        meta.add_live(n as u32);
        // Page-map "first class wins", applied once per DISTINCT page entered by
        // this run. `carve_block` marks a page iff it was not already owned; the
        // first block to land on a page is the one that dedicates it, so calling
        // `set_class` on each page-index CHANGE reproduces that exactly.
        //
        // R12-11 (task #262): diagnostic-only bookkeeping (`PageMap` is NOT
        // load-bearing for class routing — see its struct doc); gated behind
        // `page-map-diag` and elided from the default/production carve path.
        #[cfg(feature = "page-map-diag")]
        let mut pm = meta.page_map();
        #[cfg(feature = "page-map-diag")]
        let mut prev_page = usize::MAX;
        for (i, slot) in out[..n].iter_mut().enumerate() {
            let off = aligned_start + i * block_size;
            #[cfg(feature = "page-map-diag")]
            {
                let page = off / crate::alloc_core::os::PAGE;
                if page != prev_page {
                    if pm.class_of(page).is_none() {
                        pm.set_class(page, class_idx);
                    }
                    prev_page = page;
                }
            }
            *slot = Node::deref(segment, off);
        }
        n
    }
}
