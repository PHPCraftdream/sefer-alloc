use core::ptr::NonNull;

use crate::alloc_core::node::{Node, NODE_SIZE};
#[cfg(any(
    feature = "alloc-decommit",
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit"
))]
use crate::alloc_core::os;
use crate::alloc_core::os::SEGMENT;
// R7-A2 + D1′ phase-1b: `SegmentHeader::segment_id_at` is consulted here by
// the `alloc-segment-directory`-gated directory-bitmap maintenance arms of
// `pop_free`/`drain_freelist_batch` and by the `hardened` freelist
// validation — gate the import on the same pair so builds without either
// stay warning-clean (D1′ phase-1b removed the other consumers).
#[cfg(any(feature = "hardened", feature = "alloc-segment-directory"))]
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
use crate::alloc_core::state_byte::StateByte;

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
    // MAX_REALISTIC_PAGE_SIZE (64 KiB) of rounding, so the slack is part of
    // the bound: the ROUNDED initial commit stays within one SEGMENT.
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
    /// per-segment `BinTable`s — a freed block in a non-current segment must
    /// stay reusable); only then carves a fresh block / reserves a fresh
    /// segment, plus a refill batch (Phase 9 amortisation) pushed into each
    /// block's OWN segment's `BinTable` via `segment_base_of` (defect A fix:
    /// never a captured "current" pointer).
    ///
    /// Phase 12.5 (shard model): a heap owns its segments exclusively — no
    /// adoption hook; carves/reserves from its OWN segments only. Foreign
    /// frees publish to pinned route sidecars; discovery/reclamation run on
    /// allocation miss paths.
    #[inline(always)]
    pub(in crate::alloc_core) fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        // 1. Try the free list of the current small segment.
        if let Some(ptr) = self.pop_free(self.small_cur, class_idx) {
            return ptr;
        }
        // 2. Current segment's class free list is empty: probe the per-class
        //    discovery cursor, then scan the OTHER owned segments for one
        //    with a non-empty class free list (B4 §11: the scan starts after
        //    the cursor's slot and wraps; see
        //    docs/perf/B4_MT_COLLAPSE_DIAGNOSIS.md §11). Scalar and refill
        //    discovery share the same physical metadata guards; terminal
        //    consumption cannot link a free or magazine-resident block onto
        //    the substrate a second time. D1′ phase-1b: discovery is
        //    infallible — the class-leaf spill OOM is gone with the class
        //    leaves, so no `Err` arm remains (main's OOM probe lived in the
        //    removed `prepare_for` class-spill path).
        if let Some(ptr) = self.alloc_small_via_discovery(class_idx) {
            return ptr;
        }
        // 3. No free block anywhere: carve a FRESH block. On the cold carve
        //    path we also carve a refill batch (Phase 9 amortisation) so the
        //    next allocs pop from the free list instead of carving one-by-one.
        //    Each refilled block is pushed into its OWN segment's BinTable
        //    (via `segment_base_of(ptr)`), never a captured "current" pointer
        //    (defect A fix: `small_cur` may shift mid-batch when a segment fills).
        //    D1′ phase-1b: no fallible prepare before the carve — issue paths
        //    are infallible (the only OOM left is an honest OS commit/reserve
        //    refusal, handled inside the carve itself). Main's
        //    `prepare_current_carve` probe is removed with the class leaves.
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
    /// `alloc` contract must never observe the virgin bit).
    ///
    /// Returns `(ptr, is_virgin)`. `is_virgin` is `true` **only** when `ptr`
    /// was served by a bump-cursor CARVE (`carve_block`/`carve_batch`, via
    /// `carve_block_with_refill`) on a segment whose `payload_virgin` bit
    /// reads `true`, AND `cfg!(not(miri))`. Every free-list-pop dispatch
    /// yields `false` unconditionally — a popped block was carved and issued
    /// strictly earlier, so it can never be virgin ("dispatch conjunct",
    /// design docs §2 in both).
    ///
    /// The bit is read from `self.small_cur` (or the freshly reserved
    /// segment) BEFORE the carve — safe because a carve never WRITEs the bit
    /// (`SegmentHeader::payload_virgin` field doc's reset table), so reading
    /// before or after a successful carve on the SAME segment is equivalent.
    #[cfg(feature = "virgin-zero-skip")]
    #[inline]
    pub(crate) fn alloc_small_with_virgin(&mut self, class_idx: usize) -> (*mut u8, bool) {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        // 1. Current segment's free list — never virgin (dispatch conjunct).
        if let Some(ptr) = self.pop_free(self.small_cur, class_idx) {
            return (ptr, false);
        }
        // 2. Other owned segments' free lists — never virgin (a popped block
        //    was carved and issued earlier). Same cursor-aware discovery step
        //    as `alloc_small` (B4 §11).
        if let Some(ptr) = self.alloc_small_via_discovery(class_idx) {
            return (ptr, false);
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

    #[cfg(feature = "hardened")]
    #[inline(always)]
    fn valid_free_node(segment: *mut u8, off: u32, class_idx: usize) -> bool {
        let offset = off as usize;
        let meta = SegmentMeta::new(segment);
        let start = match SegmentHeader::kind_at(segment) {
            crate::alloc_core::segment_header::SegmentKind::Primordial => {
                crate::alloc_core::segment_header::Layout::primordial_meta_end()
            }
            crate::alloc_core::segment_header::SegmentKind::Small => {
                crate::alloc_core::segment_header::Layout::small_meta_end()
            }
            _ => return false,
        };
        let size = SizeClasses::block_size(class_idx);
        offset >= start
            && offset < SEGMENT
            && offset.is_multiple_of(size)
            && offset
                .checked_add(size)
                .is_some_and(|end| end <= meta.bump_of() && end <= SEGMENT)
            && meta.state_byte().read(off) == StateByte::encode(class_idx, StateByte::FREE)
    }

    #[cfg(feature = "hardened")]
    #[cold]
    #[inline(never)]
    fn free_list_corrupted() -> ! {
        // Corrupt allocator-owned state cannot be recovered as an OOM. Abort
        // deliberately skips unwinding, destructors, and the panic hook.
        crate::alloc_core::fatal::fatal(
            b"sefer-alloc: hardened: Small free-list corruption detected; aborting\n",
        )
    }
    #[cfg(feature = "hardened")]
    #[inline(always)]
    fn free_continuation(segment: *mut u8, off: u32, class_idx: usize, next: *mut u8) -> u32 {
        if next.is_null() {
            return FREE_LIST_NULL;
        }
        let next_off =
            Node::continuation_offset(segment, next).unwrap_or_else(|| Self::free_list_corrupted());
        if next_off == off || !Self::valid_free_node(segment, next_off, class_idx) {
            Self::free_list_corrupted();
        }
        next_off
    }

    /// Pop a free block. D1′ phase-1b: infallible — the former transactional
    /// prepare (and its class-leaf spill OOM) is gone; the only remaining
    /// owner-side OOM is an honest OS commit/reserve refusal, which this
    /// path never performs. Hardened corruption aborts.
    #[inline(always)]
    pub(in crate::alloc_core) fn pop_free(
        &mut self,
        segment: *mut u8,
        class_idx: usize,
    ) -> Option<*mut u8> {
        let mut meta = SegmentMeta::new(segment);
        let mut bt = meta.bin_table();
        let head_off = bt.head(class_idx);
        if head_off == FREE_LIST_NULL {
            return None;
        }
        #[cfg(feature = "hardened")]
        if !Self::valid_free_node(segment, head_off, class_idx) {
            Self::free_list_corrupted();
        }
        let block_ptr = Node::deref(segment, head_off as usize);
        let block_nn = NonNull::new(block_ptr)?;
        let next = Node::read_next(block_nn);
        #[cfg(feature = "hardened")]
        let new_head = Self::free_continuation(segment, head_off, class_idx, next);
        #[cfg(not(feature = "hardened"))]
        let new_head = if next.is_null() {
            FREE_LIST_NULL
        } else {
            let bounds =
                crate::alloc_core::config::continuation_bounds::ContinuationBounds::new(segment);
            Node::continuation_offset_or_fatal(segment, next, bounds)
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
        // B4 (в) F3-fix: assert the byte's PRIOR value BEFORE the issue
        // commits LIVE.
        debug_assert_eq!(
            meta.state_byte().read(head_off),
            StateByte::encode(class_idx, StateByte::FREE),
            "pop_free: free-list head must carry its exact class FREE byte",
        );
        // B4 (в): issue = commit FREE → LIVE in the state byte.
        meta.state_byte()
            .write(head_off, class_idx, StateByte::LIVE);
        // Phase 35 (M6): a block left the free list → one more live block.
        // A popped block always comes from a COMMITTED payload (a decommitted
        // segment's free list is empty), so no recommit is needed here — only
        // `carve_block` writes fresh payload and thus recommits.
        meta.inc_live();
        // D1′ phase-1b: set the record's "start was issued" bit exactly at the
        // former `tx.commit` point (infallible; the class lives only in the
        // byte written above). Staged: the slot guards open once here (one
        // block per `pop_free` call — same guard set, one handle).
        self.table.small_issue_run(segment).issue(head_off);
        Some(block_ptr)
    }

    /// Drain up to `out.len()` free blocks of one class into `out`; returns
    /// the number drained. D1′ phase-1b: the former transactional
    /// `prepare_for` probe (class-leaf spill OOM) is gone.
    ///
    /// - Production: ONE pass — validate the node, read its continuation,
    ///   commit the LIVE byte, store into `out[k]`. The continuation checks
    ///   (`continuation_offset_or_fatal` / hardened `free_continuation`)
    ///   are MOVED here, not deleted.
    /// - Hardened: stays two-pass — a Floyd/`valid_free_node` walk validates
    ///   the whole chain BEFORE any mutation, then the single-pass commit
    ///   walk runs on the validated chain.
    ///
    /// A cheap per-run route-slot integrity check
    /// (`base_at(segment_id_at(base)) == base`) replaces the former
    /// per-issue fatal guards.
    ///
    /// The two per-block costs that MUST stay per-block are kept per-block:
    ///
    ///   - `read_next(block)` — the dependent load that walks the intrusive
    ///     chain. mimalloc pays this too; there is no way to hoist it (each
    ///     `next` lives in the previous block's body). We never WRITE the block
    ///     body on this path (pop doesn't), so reading `next` before advancing
    ///     is hazard-free: nothing overwrites a block between our read of its
    ///     `next` and our recording it.
    ///   - the start-bit set (`issue(off)`) — cleared/committed per-block.
    ///     **Decision: per-block, NOT merged.** A freelist is a LIFO push
    ///     chain, so consecutive popped offsets are in general SCATTERED
    ///     across the sidecar (they do not share a word the way a flush batch
    ///     of consecutive carves would). The batch win is the hoisted
    ///     `set_head` / `head`-read / `add_live`, NOT the per-block sidecar
    ///     ops (which were never the expensive part).
    ///
    /// ## D1 / M2 / set_head correctness
    ///
    ///   - **D1:** exactly `k` blocks leave the free list and are handed out, so
    ///     `add_live` by `k` == `k` per-block `inc_live`s. No double, no
    ///     under-count.
    ///   - **M2:** every recorded block ends with its own state byte
    ///     (`issue_state`) written, exactly as `pop_free` leaves it. A later
    ///     double-free still hits the state check correctly.
    ///   - **set_head:** after the walk, `head` holds either the offset of the
    ///     first un-popped node (chain longer than `out`) or `FREE_LIST_NULL`
    ///     (chain exhausted). We `set_head` to that once. A subsequent
    ///     `pop_free`/drain therefore yields exactly the remaining blocks in the
    ///     same order.
    ///
    /// R7-A2: `&mut self` (upgraded from `&self`) so the directory bitmap can
    /// be maintained at the single choke point. Touches only `segment`
    /// metadata via `SegmentMeta` for the freelist walk, then calls
    /// `publish_empty` on the directory sidecar (if materialised) when the
    /// drain empties the list.
    #[inline]
    #[cfg(feature = "internals")]
    pub(in crate::alloc_core) fn drain_freelist_batch(
        &mut self,
        segment: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        self.try_drain_freelist_batch(segment, class_idx, out, StateByte::LIVE)
    }

    /// `issue_state` = state byte committed per popped block — LIVE for
    /// batch paths, MAG for the magazine refill (parks n−1 blocks; S1).
    /// Infallible since D1′ phase-1b: the former fallible `prepare_for`
    /// class-spill probe is gone with the class leaves, so main's
    /// `Result<usize, ()>` signature degraded to a plain `usize` (0 =
    /// nothing drained); hardened validation failures abort rather than
    /// being converted to an empty drain.
    pub(in crate::alloc_core) fn try_drain_freelist_batch(
        &mut self,
        segment: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
        issue_state: u8,
    ) -> usize {
        if out.is_empty() {
            return 0;
        }
        // D1′ staged (OPT): the fatal slot guards (index < cap, occupied
        // slot, root == base, sidecar present) plus the cheap per-run
        // integrity check `base_at(segment_id_at(base)) == base` — all
        // hoisted into ONE `small_issue_run` open per run (design §2.2);
        // the per-block loop below does only the cheap sidecar operations.
        let issue_run = self.table.small_issue_run(segment);
        let mut meta = SegmentMeta::new(segment);
        let mut bt = meta.bin_table();
        // Read the head ONCE.
        let mut head_off = bt.head(class_idx);
        if head_off == FREE_LIST_NULL {
            return 0;
        }
        // Run-level protocol (B4-R3), D1′ form: the fatal route guards run
        // once in `small_issue_run` (opened above); nothing per-block is
        // fallible any more (main's per-block `prepare_for` leaf-spill probe
        // is gone with the class leaves).
        // Hardened pre-pass: validate the chain (Floyd cycle detection +
        // per-node FREE-byte/geometry checks) BEFORE any mutation. No
        // `prepare_for` — nothing fallible remains.
        #[cfg(feature = "hardened")]
        {
            let mut probe = head_off;
            let mut slow = head_off;
            let mut probed = 0usize;
            while probed < out.len() && probe != FREE_LIST_NULL {
                if !Self::valid_free_node(segment, probe, class_idx) {
                    Self::free_list_corrupted();
                }
                let block = Node::deref(segment, probe as usize);
                let Some(nn) = NonNull::new(block) else {
                    break;
                };
                let next = Node::read_next(nn);
                let next_off = Self::free_continuation(segment, probe, class_idx, next);
                if next_off == bt.head(class_idx) {
                    Self::free_list_corrupted();
                }
                probe = next_off;
                if probed % 2 == 1 {
                    let slow_block = NonNull::new(Node::deref(segment, slow as usize))
                        .unwrap_or_else(|| Self::free_list_corrupted());
                    slow = Self::free_continuation(
                        segment,
                        slow,
                        class_idx,
                        Node::read_next(slow_block),
                    );
                }
                if probe != FREE_LIST_NULL && probe == slow {
                    Self::free_list_corrupted();
                }
                probed += 1;
            }
        }
        #[cfg(not(feature = "hardened"))]
        let bounds =
            crate::alloc_core::config::continuation_bounds::ContinuationBounds::new(segment);
        // Single mutation pass: validate → read_next → LIVE byte → out[k].
        let mut k = 0usize;
        while k < out.len() && head_off != FREE_LIST_NULL {
            let block_ptr = Node::deref(segment, head_off as usize);
            let block_nn = match NonNull::new(block_ptr) {
                Some(nn) => nn,
                // Corrupt offset only: commit what we have (defence-in-depth);
                // `head` stays on this node so nothing is lost.
                None => break,
            };
            // Dependent load: read this block's `next` BEFORE recording it. The
            // block body is never written on the pop path, so this is race-free
            // against ourselves.
            let next = Node::read_next(block_nn);
            // Continuation check (MOVED from the former probe pass, never
            // deleted): resolve the next node's offset through the same
            // guarded path as before mutating this node's state byte.
            let next_off = if next.is_null() {
                FREE_LIST_NULL
            } else {
                #[cfg(feature = "hardened")]
                {
                    let next_off = Self::free_continuation(segment, head_off, class_idx, next);
                    if next_off == head_off {
                        Self::free_list_corrupted();
                    }
                    next_off
                }
                #[cfg(not(feature = "hardened"))]
                {
                    Node::continuation_offset_or_fatal(segment, next, bounds)
                }
            };
            // B4 (в)/S1: issue = commit `issue_state` (LIVE for batch paths,
            // MAG when the caller parks the batch in a magazine).
            meta.state_byte().write(head_off, class_idx, issue_state);
            // D1′ phase-1b: set the start bit at the former `run.commit`
            // point — per block, never merged (d1p_rev finding 3).
            issue_run.issue(head_off);
            out[k] = block_ptr;
            k += 1;
            head_off = next_off;
        }
        // Write the new head ONCE: the first un-popped node, or NULL.
        bt.set_head(class_idx, head_off);
        // B4-M (P1-2): MAG state-byte writes of this drain, one add per batch.
        #[cfg(feature = "bench-internals")]
        if issue_state == StateByte::MAG {
            crate::alloc_core::alloc_core::S1_MAG_BYTE_WRITES
                .fetch_add(k as u64, core::sync::atomic::Ordering::Relaxed);
        }
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
        meta.add_live(k as u32);
        k
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
        // Read/write only the owner-only `bump` field, never the whole header.
        // Terminal foreign frees use route descriptors and independent sidecars;
        // they do not read this bump cursor.
        let bump = meta.bump_of();
        let aligned_bump = align_up(bump, block_size);
        if aligned_bump + block_size > SEGMENT {
            return None;
        }
        // D1′ phase-1b: no fallible prepare before the mutation — issue
        // paths are infallible; the segment-full check above still sends a
        // full segment to the reserve path.
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
        // D1' phase-1a carve-clear (design section 2.4): after ALL fallible
        // steps and BEFORE the LIVE write, drop the fresh block's start
        // granule's pending bit (load Acquire -> fetch_and !mask AcqRel, RMW
        // only) so a garbage pending bit on a never-issued granule cannot
        // survive into a live block. Counted by CARVE_CLEAR_BITS /
        // CARVE_CLEAR_WORD_LOADS. Staged: the per-run slot guards open once
        // here (`small_issue_run`); the clear and the per-block start-bit
        // set below are the cheap sidecar operations.
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
            let page = aligned_bump / crate::alloc_core::os::PAGE;
            if pm.class_of(page).is_none() {
                // Page was Free or Meta; dedicate it to this class.
                pm.set_class(page, class_idx);
            }
        }
        let ptr = Node::deref(segment, aligned_bump);
        // D1′ staged: open the per-run handle once for this single-block
        // carve (same fatal guards); carve-clear, then the fresh block's
        // start bit set at exactly the former `tx.commit` point — both
        // before the LIVE write (infallible).
        let issue_run = self.table.small_issue_run(segment);
        issue_run.clear_pending_start_bit(aligned_bump as u32);
        issue_run.issue(aligned_bump as u32);
        // B4 (в): a freshly carved block is issued LIVE at first touch.
        meta.state_byte()
            .write(aligned_bump as u32, class_idx, StateByte::LIVE);
        Some(ptr)
    }

    /// S1: `issue_state` — LIVE (batch paths) or MAG (magazine refill);
    /// see `try_drain_freelist_batch`.
    pub(in crate::alloc_core) fn try_carve_batch(
        &mut self,
        class_idx: usize,
        block_size: usize,
        out: &mut [*mut u8],
        issue_state: u8,
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
        let room = (SEGMENT - aligned_start) / block_size;
        let n = out.len().min(room);
        // D1′ phase-1b: no per-block fallible prepare (main's per-block
        // `run.prepare_for` leaf-spill probe is gone with the class leaves) —
        // the start-bit set happens per block below, at the former
        // `run.commit` point.
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
        // How many blocks fit from `aligned_start` to the segment end,
        // capped by the caller's slice. Computed ONCE (#1994: previously
        // recomputed a second time, identically, inside the lazy-commit
        // growth check below) — shared by that check's `batch_end` and the
        // final carve's bump advance.
        #[cfg(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        ))]
        {
            let frontier = meta.committed_payload_end_of();
            let batch_end = aligned_start + n * block_size;
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
        // Advance the bump cursor ONCE to just past the last carved block —
        // byte-identical to the final `set_bump` of the n-th sequential carve.
        meta.set_bump(aligned_start + n * block_size);
        // D1′ staged (OPT): open the per-run issue handle ONCE — the fatal
        // slot guards run here, not per block (design §2.2).
        let issue_run = self.table.small_issue_run(segment);
        // Batched live increment (D1): exactly `n` blocks handed out.
        meta.add_live(n as u32);
        // D1′ M-h oracle seam (test-only, compiled out of every build without
        // `internals`/`bench-internals`): expose the defect window
        // `[set_bump + add_live, first per-block issue]`. Production issue
        // order below is PER BLOCK — `clear_pending_start_bit(off)` →
        // `issue(off)` (start bit) → LIVE byte — so while this stage value
        // is `n`, NO block of the batch may yet show a start bit to a
        // concurrent publisher. An early mass publication of all batch
        // start bits here (d1p_rev M-h) is visible exactly inside this
        // window; the test `oneclass_carve_batch_early_publication` parks
        // the owner here and probes the sidecar start-bit bank.
        #[cfg(any(feature = "internals", feature = "bench-internals"))]
        {
            crate::alloc_core::CARVE_BATCH_STAGE.store(n, core::sync::atomic::Ordering::Release);
            if crate::alloc_core::CARVE_BATCH_PAUSE.load(core::sync::atomic::Ordering::Acquire) {
                while !crate::alloc_core::CARVE_BATCH_RESUME
                    .swap(false, core::sync::atomic::Ordering::AcqRel)
                {
                    core::hint::spin_loop();
                }
            }
        }
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
            let ptr = Node::deref(segment, off);
            // D1' phase-1a carve-clear: PER BLOCK, at the block's own
            // per-block commit point (never one merged mask after
            // `set_bump` - d1p_rev finding 3), before its LIVE write.
            // D1′ phase-1b: the start bit is set at EXACTLY the former
            // per-block `run.commit` point — never merged into one store
            // after `set_bump` (d1p_rev finding 3). Infallible. D1′
            // equivalent of main's `run.commit(off)`.
            issue_run.clear_pending_start_bit(off as u32);
            issue_run.issue(off as u32);
            // B4 (в)/S1: freshly carved blocks commit `issue_state` (LIVE,
            // or MAG when the caller parks the batch in a magazine).
            meta.state_byte().write(off as u32, class_idx, issue_state);
            *slot = ptr;
        }
        // D1′ M-h oracle seam (test-only): the batch is fully issued — the
        // early-publication window is closed.
        #[cfg(any(feature = "internals", feature = "bench-internals"))]
        crate::alloc_core::CARVE_BATCH_STAGE.store(0, core::sync::atomic::Ordering::Release);
        // B4-M (P1-2): MAG state-byte writes of this carve, one add per batch.
        #[cfg(feature = "bench-internals")]
        if issue_state == StateByte::MAG {
            crate::alloc_core::alloc_core::S1_MAG_BYTE_WRITES
                .fetch_add(n as u64, core::sync::atomic::Ordering::Relaxed);
        }
        n
    }
}
