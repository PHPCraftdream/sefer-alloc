//! Small-path hot cluster of [`AllocCore`] (mechanical split of
//! `alloc_core.rs`).
//!
//! This file holds the `impl AllocCore { .. }` block for the small-object
//! alloc / dealloc / carve / segment-reserve hot path. The cross-thread
//! reclaim, magazine batch, and diagnostics blocks live in their sibling
//! files (`alloc_core_small_reclaim`, `alloc_core_small_magazine`,
//! `alloc_core_small_diag`). Pure code-movement; no behavior changed.

use core::ptr::NonNull;

use super::node::{Node, NODE_SIZE};
#[cfg(feature = "numa-aware")]
use super::numa;
#[cfg(not(any(feature = "numa-aware", feature = "alloc-lazy-commit")))]
use super::os::Segment;
use super::os::{self, SEGMENT};
use super::segment_header::{
    align_up, BinTable, Layout as SegLayout, PageMap, SegmentHeader, SegmentKind, SegmentMeta,
    FREE_LIST_NULL,
};
use super::size_classes::SizeClasses;

use super::alloc_core::{base_add, AllocCore};

/// B2 (R7 Workstream B): process-wide count of successful `commit_pages` calls
/// on the grow-on-carve path. Diagnostic-only (relaxed), gated on
/// `alloc-lazy-commit`. Tests observe this to verify that `carve_batch` does
/// ONE commit per batch (not per block) and that chunk boundary crossings
/// trigger the expected number of commits.
#[cfg(feature = "alloc-lazy-commit")]
pub(crate) static GROW_COMMIT_COUNT: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// B1 (R7 Workstream B): the size of the FIRST committed payload chunk when a
/// small segment is lazily reserved under `alloc-lazy-commit`. Only the
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
#[cfg(feature = "alloc-lazy-commit")]
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
#[cfg(feature = "alloc-lazy-commit")]
pub(crate) const GROW_CHUNK: usize = LAZY_FIRST_CHUNK;

// B1/B2: compile-time sanity — the initial commit (metadata + first chunk) must
// fit within one segment, and both chunk constants must be page-aligned and
// non-zero.
#[cfg(feature = "alloc-lazy-commit")]
const _: () = {
    assert!(
        LAZY_FIRST_CHUNK > 0 && LAZY_FIRST_CHUNK.is_multiple_of(super::os::PAGE),
        "LAZY_FIRST_CHUNK must be a non-zero multiple of PAGE"
    );
    assert!(
        super::segment_header::Layout::small_meta_end() + LAZY_FIRST_CHUNK <= super::os::SEGMENT,
        "metadata + LAZY_FIRST_CHUNK must fit within one SEGMENT"
    );
    assert!(
        GROW_CHUNK > 0 && GROW_CHUNK.is_multiple_of(super::os::PAGE),
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
    pub(super) fn alloc_small(&mut self, class_idx: usize) -> *mut u8 {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        // 1. Try the free list of the current small segment.
        if let Some(ptr) = self.pop_free(self.small_cur, class_idx, block_size) {
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
            if let Some(ptr) = self.pop_free(seg, class_idx, block_size) {
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
                if let Some(ptr) = self.pop_free(self.small_cur, class_idx, block_size) {
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
                        super::directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        if let Some(ptr) = self.pop_free(seg, class_idx, block_size) {
                            return ptr;
                        }
                    }
                }
                core::ptr::null_mut()
            }
        }
    }

    /// Carve one fresh block of `class_idx` for the caller, plus a refill
    /// batch of extra blocks that are pushed onto their OWN segments'
    /// `BinTable[class_idx]` (Phase 9 amortisation, Phase 12.1 segment-centric
    /// free state). Each extra block's owning segment is derived per-block via
    /// `segment_base_of(ptr)` — `small_cur` may shift mid-batch when the
    /// current segment fills, so a captured pointer would corrupt the wrong
    /// segment's BinTable head (defect A).
    ///
    /// Returns the first carved block (for the caller), or `None` if the
    /// current segment cannot fit even one block (caller reserves a fresh
    /// segment and retries).
    fn carve_block_with_refill(&mut self, class_idx: usize, block_size: usize) -> Option<*mut u8> {
        // Carve the caller's block first.
        let first = self.carve_block(class_idx, block_size)?;
        // Refill batch: carve extra blocks and push each into its OWN segment.
        // `carve_block` returns None when the current segment is full; we stop
        // the batch there (the next alloc will reserve a fresh segment).
        //
        // Size chosen by measurement (Phase 13.5, task #29). Swept
        // {31, 63, 127, 255, 511} over the MT macro-bench (larson + mstress,
        // T=1/2/4 ops/sec — the load where refill actually bites) and the
        // single-threaded fixed-size churn micro-bench. Result: 31 is the
        // throughput winner. Larger batches do NOT help — they monotonically
        // HURT larson (working-set churn): T1/T2 larson fell from ~21–25 M to
        // ~14–18 M at 127–511, because a free-list miss now does up to 8×–16×
        // more upfront carve work (page faults, page-map writes) that the
        // steady-state churn never amortises. mstress was within noise and the
        // single-threaded churn was flat (~23–24 µs at every value — it pops
        // from the free list and never re-enters the cold carve). The §3.5
        // "raise toward a page of blocks (256–512)" hypothesis did not hold
        // under measurement; 31 stays. (Bigger upfront carve = worse locality
        // for the churn pattern, not better.)
        const REFILL_BATCH: usize = 31;
        for _ in 0..REFILL_BATCH {
            let Some(extra) = self.carve_block(class_idx, block_size) else {
                break;
            };
            let base = os::segment_base_of_ptr(extra);
            self.dealloc_small(base, extra, class_idx);
        }
        Some(first)
    }

    /// Scan all owned SMALL/PRIMORDIAL segments and return the base of the
    /// first one whose `BinTable[class_idx]` is non-empty. Used by
    /// [`alloc_small`] on a current-segment miss to reuse freed blocks in
    /// non-current segments (Phase 12.1: free state lives in per-segment
    /// `BinTable`s).
    ///
    /// **Large segments are excluded:** a large segment has no `BinTable`
    /// (only a header), so reading its `bin_table()` would dereference
    /// garbage and could return a bogus non-null head — leading `pop_free`
    /// to read a junk block and compute an out-of-segment `next` pointer
    /// (overflow/UAF). We read each candidate's header `kind` and skip
    /// non-small/primordial segments.
    ///
    /// Returns `None` if no owned small segment has a free block of this
    /// class.
    ///
    /// ## Slot recycle integration (task #60, `alloc-decommit`)
    ///
    /// Under `alloc-xthread` + `alloc-decommit`, the ring drain inside this
    /// function may trigger `dec_live_and_maybe_decommit` (via `reclaim_offset`)
    /// which decommits an empty segment. Slot recycling — `self.table.recycle(base)`
    /// — is deferred until AFTER the drain for that `base` is complete. This is
    /// critical: a partially-drained ring still has ring entries that
    /// `reclaim_offset` processes by reading the segment's metadata (which stays
    /// committed). Recycling before the drain ends would release the OS
    /// reservation prematurely — the metadata read in `magic_at` / `kind_at`
    /// would UAF. By recycling after the drain, we ensure:
    ///   a. All ring entries for `base` are processed (or safely skipped via
    ///      the `off >= bump` guard — bump was reset by decommit).
    ///   b. The OS release + slot NULL happen atomically in `recycle`, with no
    ///      window where the slot is non-NULL but the OS segment is gone.
    pub(crate) fn find_segment_with_free(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free_impl(
            class_idx,
            #[cfg(feature = "alloc-xthread")]
            &|_, _| false,
            #[cfg(feature = "alloc-segment-directory")]
            false,
        )
    }

    /// Task #164: variant with magazine predicate, called from
    /// `refill_class_bump` when the magazine is accessible.
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    pub(crate) fn find_segment_with_free_checked<F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        class_idx: usize,
        is_in_magazine: &F,
    ) -> Option<*mut u8> {
        self.find_segment_with_free_impl(
            class_idx,
            is_in_magazine,
            #[cfg(feature = "alloc-segment-directory")]
            false,
        )
    }

    /// R9-8 (task #230): forced "rescue scan" — runs the full O(S) linear
    /// scan BYPASSING the R8-2 directory-trust fast path, so a stale-negative
    /// directory bit cannot hide a real free block. Self-heals any bit the
    /// scan finds the directory had wrongly cleared (same mechanism as the
    /// periodic re-validation path). Called as a last resort from the
    /// small-allocation OOM path (where `reserve_small_segment` returned
    /// `None`) to avoid a spurious OOM a directory bug could otherwise cause.
    /// Exists only under the directory feature + not-`numa-aware` (under
    /// `numa-aware` the directory is never trusted for lookups, so there is no
    /// hazard to rescue from). This unchecked variant is used by `alloc_small`
    /// (whose step-2 scan is proven magazine-unreachable under `fastbin`).
    #[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]
    pub(crate) fn find_segment_with_free_forced(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free_impl(
            class_idx,
            #[cfg(feature = "alloc-xthread")]
            &|_, _| false,
            true,
        )
    }

    /// R9-8 (task #230): checked variant of the rescue scan, used by the
    /// magazine-refill OOM path (`refill_class_bump_impl`'s step-4 `None`
    /// branch) so a cross-thread-freed magazine-resident block in a drained
    /// ring is NOT reclaimed (avoiding a double-issue). See
    /// `find_segment_with_free_forced` for the rescue semantics.
    #[cfg(all(
        feature = "alloc-segment-directory",
        feature = "alloc-xthread",
        feature = "fastbin",
        not(feature = "numa-aware")
    ))]
    pub(crate) fn find_segment_with_free_checked_forced<F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        class_idx: usize,
        is_in_magazine: &F,
    ) -> Option<*mut u8> {
        self.find_segment_with_free_impl(class_idx, is_in_magazine, true)
    }

    #[cfg_attr(
        all(feature = "alloc-xthread", not(feature = "fastbin")),
        allow(unused_variables)
    )]
    #[inline]
    fn find_segment_with_free_impl<
        #[cfg(feature = "alloc-xthread")] F: Fn(*mut u8, usize) -> bool,
    >(
        &mut self,
        class_idx: usize,
        #[cfg(feature = "alloc-xthread")] is_in_magazine: &F,
        // R9-8 (task #230): when `true`, this call is a forced "rescue scan"
        // run as a last resort before the small path surfaces an OOM. It
        // BYPASSES the R8-2 directory-trust fast path (so a stale-negative
        // directory bit cannot hide a real free block) and self-heals any bit
        // the scan finds the directory had wrongly cleared (via the same
        // mechanism the periodic re-validation path uses). Cf-gated to the
        // directory feature: only the rescue wrappers (below) ever pass `true`,
        // and they exist solely under `alloc-segment-directory` + not-`numa-aware`.
        #[cfg(feature = "alloc-segment-directory")] rescue: bool,
    ) -> Option<*mut u8> {
        // R8-2 (task #215): set to `true` ONLY when the periodic
        // re-validation branch below actually runs (directory feature ON,
        // sidecar materialised, streak hit the period). The linear-scan
        // success paths read it to decide whether to self-heal a directory
        // bit — distinguishing "scan ran as periodic re-validation" (heal)
        // from "scan ran because the directory feature is OFF / sidecar not
        // materialised / `numa-aware` skipped the directory path" (no heal,
        // since there is no miss to repair and healing under `numa-aware`
        // would muddy the `DIRECTORY_MISS_SELF_HEAL` canary signal).
        // Under `numa-aware` the directory-driven lookup block below (which is
        // the ONLY site that writes `true`) is compiled out entirely (gated
        // `not(feature = "numa-aware")`), so this binding is read-only in that
        // configuration — `mut` would be a warning under `--all-features`.
        #[cfg(feature = "alloc-segment-directory")]
        let mut periodic_revalidation_active = false;
        // R11-6: compute my_node ONCE for both the directory-driven lookup
        // (node-bucket preference order) and the linear-scan NUMA two-pass
        // logic below. Moved here from the linear-scan prologue so the
        // directory path can use it.
        #[cfg(feature = "numa-aware")]
        let my_node = self.current_node_cached();
        // ── R7-A3: directory-accelerated path ──────────────────────────────
        //
        // When `alloc-segment-directory` is compiled in AND the sidecar is
        // materialised (table.count() >= threshold), query the per-class
        // bitmap for set bits. Each set bit is a CANDIDATE segment whose
        // BinTable *was* non-empty for `class_idx` at the last transition
        // event. Because the bitmap can lag (a drain that empties the class
        // between the set and our read), every candidate is VALIDATED:
        //   1. base_at(slot) != null
        //   2. kind is Small/Primordial
        //   3. BinTable head for class_idx is STILL non-null
        //
        // Before inspecting the BinTable, the candidate's remote-free ring
        // is drained (P1-a), preserving the Variant-2 drain + decommit /
        // pool hysteresis (P1-b) + ring_drain_head refresh (P1-d).
        // On a valid hit, `unpool_if_present(base)` is called (P1-c).
        //
        // On a directory MISS (no set bit yields a valid hit), R8-2 (task
        // #215) makes the directory AUTHORITATIVE in the common case: the
        // miss is trusted and we return `None` immediately (no O(S) scan),
        // so the caller carves a fresh segment as if the directory's "empty
        // for this class" were the truth. Every
        // `DIRECTORY_MISS_FULL_SCAN_PERIOD` misses a periodic re-validation
        // full scan still runs as a safety net (and self-heals any drift it
        // finds — see the success path of the linear scan below).
        //
        // P2 (NUMA two-pass preference): when `numa-aware` is active,
        // the directory-driven lookup is DISABLED — the linear-scan
        // fallback below handles the two-pass local-first /
        // foreign-fallback NUMA preference. The directory bitmap is
        // still maintained (A1/A2 helpers fire normally), so the
        // sidecar stays consistent for a future round that adds
        // node-aware bit selection. This preserves `--all-features`
        // compatibility: both features compile together, but the
        // directory is a write-only index under `numa-aware` until
        // node-aware queries are implemented.
        // ── R7-A4: dirty-segment drain ──────────────────────────────────────
        //
        // Before querying the directory, drain ALL dirty segments' rings.
        // This ensures the directory bits reflect the latest cross-thread
        // frees: a producer that set a dirty bit after publishing a ring
        // entry has its entry drained HERE, and the directory is updated
        // accordingly (sync_directory_for_segment_classes inside drain_dirty_segments).
        // After this, the directory lookup below can skip the per-candidate
        // ring drain for segments that were already drained in this pass.
        #[cfg(all(feature = "alloc-segment-directory", feature = "alloc-xthread"))]
        {
            #[cfg(feature = "fastbin")]
            self.drain_dirty_segments(class_idx, is_in_magazine);
            #[cfg(not(feature = "fastbin"))]
            self.drain_dirty_segments(class_idx);
        }

        #[cfg(feature = "alloc-segment-directory")]
        if !self.directory_sidecar.is_null() {
            let dir = os::deref_directory_sidecar(self.directory_sidecar);

            // R11-6: scan the per-node bitmaps in NUMA preference order.
            //
            // Non-numa-aware: a single bucket [0] — byte-for-byte the
            // pre-R11-6 flat-bitmap scan (the outer loop runs once).
            //
            // Numa-aware: LOCAL bucket first (matches `my_node`), then the
            // UNKNOWN bucket (`NO_NODE_RAW` / out-of-range segments, treated
            // as local-equivalent — mirroring the linear scan's binding
            // `seg_node != my_node && seg_node != NO_NODE_RAW` semantics
            // precisely: a NO_NODE segment is preferred over any foreign
            // segment, exactly as today), then FOREIGN real-node buckets in
            // ascending order. This preserves the two-pass local-first /
            // foreign-fallback preference the R7 plan (R10-6 §3.1) mandates.
            let mut buckets: [usize; super::segment_directory::NODE_BITMAPS] =
                [0; super::segment_directory::NODE_BITMAPS];
            let mut n_buckets: usize = 0;
            #[cfg(feature = "numa-aware")]
            {
                let my_bucket = super::segment_directory::node_bucket(my_node);
                buckets[n_buckets] = my_bucket;
                n_buckets += 1;
                // Unknown bucket — local-equivalent (R10-6 §3.2 "treated as
                // acceptable/local-equivalent, matching today's scan"). Scanned
                // BEFORE foreign so a NO_NODE segment is never deprioritised
                // below a foreign one.
                let unknown = super::segment_directory::MAX_NODES;
                if unknown != my_bucket {
                    buckets[n_buckets] = unknown;
                    n_buckets += 1;
                }
                // Foreign real-node buckets, ascending.
                for nb in 0..super::segment_directory::MAX_NODES {
                    if nb != my_bucket {
                        buckets[n_buckets] = nb;
                        n_buckets += 1;
                    }
                }
            }
            #[cfg(not(feature = "numa-aware"))]
            {
                // Single bucket [0] (pre-R11-6 layout).
                n_buckets = 1;
            }

            for &nb in buckets.iter().take(n_buckets) {
                let words = &dir.class_nonempty_by_node[nb][class_idx];

                for (w, &word_val) in words.iter().enumerate() {
                    let mut bits = word_val;
                    if bits == 0 {
                        continue;
                    }
                    // R7-A0: count each word examined by the directory scan.
                    #[cfg(feature = "alloc-stats")]
                    super::directory_stats::DIRECTORY_WORDS_EXAMINED
                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);

                    while bits != 0 {
                        let j = bits.trailing_zeros() as usize;
                        bits &= bits - 1; // clear lowest set bit
                        let slot_idx = w * 64 + j;

                        // Validate this candidate (base, kind, ring drain,
                        // BinTable head) — the SINGLE choke point shared with
                        // the non-NUMA path so the criteria are byte-for-byte
                        // identical.
                        if let Some(base) = self.validate_directory_candidate(
                            class_idx,
                            slot_idx,
                            #[cfg(feature = "alloc-xthread")]
                            is_in_magazine,
                        ) {
                            return Some(base);
                        }
                    }
                }
            }
            // (fall through to the directory-miss handling below — control
            // only reaches here if no candidate in ANY node bucket validated)
            // Directory miss: no set bit yielded a valid hit.
            //
            // R8-2 (task #215): in the common case, TRUST the directory — the
            // incremental-sync invariants (task #214, proven by the
            // `assert_directory_equals_rebuild` oracle across the directory
            // test suite) mean a genuine miss is authoritative and the O(S)
            // scan below is unnecessary defense. Every
            // `DIRECTORY_MISS_FULL_SCAN_PERIOD` misses for THIS class, run the
            // full scan anyway as a periodic re-validation safety net: if it
            // finds something the directory missed, the success-path self-heal
            // (below in the linear scan) repairs the bit in-place and bumps
            // `DIRECTORY_MISS_SELF_HEAL` as a canary counter.
            //
            // R9-8 (task #230): the streak is PER-CLASS (indexed by
            // `class_idx`), so a drift-affected class trips its OWN rescan
            // independent of how often other (healthy) classes miss — directly
            // bounding the worst case of a directory-invariant violation to
            // `DIRECTORY_MISS_FULL_SCAN_PERIOD` misses of the drifted class.
            //
            // R9-8 rescue mode (`rescue == true`): this call is the forced
            // last-resort scan before the small path surfaces an OOM. SKIP the
            // trust-the-miss return entirely (a stale-negative bit is exactly
            // what we are trying to see past) and fall straight through to the
            // linear scan with the self-heal armed. The streak is NOT touched
            // (rescue is a one-shot backstop, orthogonal to the periodic
            // cadence). The caller bumps `DIRECTORY_RESCUE_OOM_AVOIDED` if the
            // scan finds something.
            #[cfg(feature = "alloc-segment-directory")]
            {
                if rescue {
                    // Rescue: force the linear scan + self-heal, bypass trust.
                    // `rescue` is consulted directly at the heal sites below.
                } else {
                    self.directory_miss_streak[class_idx] =
                        self.directory_miss_streak[class_idx].saturating_add(1);
                    if u32::from(self.directory_miss_streak[class_idx])
                        < super::segment_directory::DIRECTORY_MISS_FULL_SCAN_PERIOD
                    {
                        // R8-2: trust the directory. Skip the O(S) scan entirely.
                        #[cfg(feature = "alloc-stats")]
                        super::directory_stats::DIRECTORY_AUTHORITATIVE_MISS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        return None;
                    }
                    // Periodic re-validation: run the full scan below anyway,
                    // and reset this class's streak regardless of whether it
                    // finds anything.
                    self.directory_miss_streak[class_idx] = 0;
                    periodic_revalidation_active = true;
                }
            }
            // `DIRECTORY_FALLBACK_SCANS` still means "the periodic
            // re-validation pass is about to run the fallback scan" — gated
            // off the rescue path (R9-8), whose entry is counted separately by
            // `DIRECTORY_RESCUE_OOM_AVOIDED` at the caller, so the periodic
            // and rescue entry counts stay distinguishable.
            #[cfg(feature = "alloc-stats")]
            if !rescue {
                super::directory_stats::DIRECTORY_FALLBACK_SCANS
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }

        // ── Guarded linear-scan fallback (the existing scan) ───────────────
        //
        // When the directory feature is OFF, or the sidecar is not
        // materialised (count < threshold), or R8-2's periodic
        // re-validation pass fires (every `DIRECTORY_MISS_FULL_SCAN_PERIOD`
        // consecutive misses), this scan runs. It is byte-for-byte
        // semantically identical to the pre-A3 scan body; R8-2 only ADDED an
        // early `return None` branch above for the trusted-miss common case.

        // Index-driven scan (task #126): walk slots `[0, count)` by index via
        // `SegmentTable::base_at`, instead of pre-collecting every live base
        // into an 8 KiB `[*mut u8; MAX_SEGMENTS]` stack buffer on every
        // free-list miss. `base_at` performs a single self-contained pointer
        // read (no borrow of `self.table` outlives the call), so it can be
        // freely interleaved with `self.table.recycle(base)` below — unlike
        // `self.table.bases()`, whose returned `impl Iterator` captures the
        // elided `&self` lifetime and would keep `self.table` borrowed for the
        // life of the loop, conflicting with the `&mut self.table.recycle`
        // call needed when a segment empties out mid-scan.
        //
        // This makes recycle UNBOUNDED within a single scan: however many
        // segments empty out (drained ring → decommit) during this call, each
        // is recycled the moment it is discovered — there is no fixed-size
        // buffer to overflow and no deferred/lost recycle (task #126 redo of
        // the Phase C attempt, which used a CAP=32 deferred-recycle ring that
        // silently dropped recycles for the 33rd+ emptied segment in one scan).
        let n = self.table.count() as usize;

        // Phase C (numa-aware): on the first pass we prefer segments whose
        // node_id matches the calling thread's NUMA node; we collect segments
        // from foreign nodes in `fallback` and return the first one only if
        // the first pass found nothing.
        //
        // Strategy (a) — "ignore migration": we consult the cached
        // current_node() value (R11-5: see `current_node_cached` and
        // `PHASE_NUMA_DESIGN.md` §4.1) on every find_segment_with_free
        // invocation. Only the FIRST call after a slot claim queries the OS;
        // subsequent calls on the same claim return the cached value. If the
        // thread migrated between nodes mid-claim, we may prefer a now-wrong
        // segment — that is the accepted MVP trade-off (§4 / §4.1 of
        // PHASE_NUMA_DESIGN.md), a small extension of the per-segment lag
        // §4 already documents to a per-claim lag on the query itself.
        //
        // R11-6: `my_node` is now computed at the top of this function (before
        // the directory-driven lookup) and reused here — see the binding above.
        // A single fallback slot: the first segment from a foreign node that has
        // a free block.  On a single-NUMA machine (or when numa-aware is off)
        // this path is never taken — all segments have node_id == my_node (or
        // NO_NODE_RAW, which is treated as "acceptable" / unknown).
        #[cfg(feature = "numa-aware")]
        let mut fallback: Option<*mut u8> = None;

        for i in 0..n {
            // R7-A0: count every slot visited by the linear scan (including
            // null/skipped slots) so the baseline has a live scan-cost counter.
            // Gated behind `alloc-stats` so feature-OFF builds are unchanged.
            #[cfg(feature = "alloc-stats")]
            super::directory_stats::FULL_SCAN_SLOTS_EXAMINED
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            let base = self.table.base_at(i);
            if base.is_null() {
                // Recycled (NULL) slot — skip. `base_at` also returns NULL for
                // an out-of-range index, but `i < n == self.table.count()`
                // here, so a NULL here always means "recycled slot", never
                // "out of range".
                continue;
            }
            // Skip large/huge segments: they have no BinTable. Field-specific
            // `kind` read (task #33): this is the Owner's alloc path,
            // concurrent with a Remote's `dealloc_routing` field reads — a
            // full-struct `read_at` here would race them. `kind_at` reads only
            // the `kind` byte, disjoint from any writer.
            if !matches!(
                SegmentHeader::kind_at(base),
                SegmentKind::Small | SegmentKind::Primordial
            ) {
                continue;
            }
            // Variant-2: lazily drain this segment's remote-free ring before
            // inspecting its BinTable. Cross-thread frees that targeted THIS
            // segment (a segment we own but are not currently allocating from)
            // are sitting in its ring; without this drain they would never
            // reach the BinTable and the scan would miss them.
            //
            // Under `alloc-decommit`, if draining empties the segment it is
            // decommitted inside `reclaim_offset`. We track whether a decommit
            // fired via the `decommit_happened` flag, then recycle the slot
            // AFTER the drain completes — not during — so that any remaining
            // ring entries for `base` can still safely read the (still-committed)
            // metadata via `magic_at`/`kind_at`/`bump_of`.
            #[cfg(feature = "alloc-xthread")]
            {
                let mut meta_for_ring = SegmentMeta::new(base);
                // PERF-PASS-4 (G9/C2, task #52): pre-drain empty-guard. Compare
                // a cheap Relaxed `tail` load against this segment's
                // owner-cached `head` (persisted across calls in the segment's
                // OWN header — see `SegmentHeader::ring_drain_head`'s doc
                // comment for why the cache lives there and not in
                // `SegmentTable`, and `RemoteFreeRing::tail_relaxed`'s doc
                // comment for the full soundness argument). If they match, no
                // producer has reserved a slot since the last drain (real or
                // guarded) — skip `drain()` entirely, INCLUDING the
                // unconditional `head.store(_, Release)` it would otherwise
                // perform, for a ring that has nothing new to report. A push
                // landing after this check is exactly as deferred as one
                // landing after today's unconditional drain finishes — the
                // "later drain picks it up" contract (remote_free_ring.rs
                // module docs) is unchanged.
                let ring = meta_for_ring.remote_ring();
                let cached_head = meta_for_ring.ring_drain_head_of();
                if ring.tail_relaxed() != cached_head {
                    let small_cur = self.small_cur;
                    #[cfg(feature = "alloc-decommit")]
                    let mut decommit_happened = false;
                    // R8-1 (task #214): accumulate the set of classes this
                    // drain pass touches, for an O(popcount) post-drain
                    // directory sync.
                    let mut changed_classes: u64 = 0;
                    let new_head = ring.drain(|off| {
                        // Task #164: when a magazine exists (fastbin), use the
                        // checked variant that consults the magazine predicate
                        // before `write_next`, closing the in-magazine leg of the
                        // ring↔magazine cross-thread double-free residual.
                        #[cfg(feature = "fastbin")]
                        let reclaimed = Self::reclaim_offset_checked(base, off, &is_in_magazine);
                        #[cfg(not(feature = "fastbin"))]
                        let reclaimed = Self::reclaim_offset(base, off);
                        if reclaimed {
                            #[cfg(feature = "alloc-decommit")]
                            if Self::dec_live_and_maybe_decommit(base, small_cur) {
                                decommit_happened = true;
                            }
                            changed_classes |=
                                1u64 << super::remote_free_ring::entry_class_idx(off);
                        }
                    });
                    // R7-A2: sync the directory for this segment after the drain
                    // completed. The drain may have reclaimed blocks into
                    // multiple classes, creating empty→non-empty transitions.
                    // R8-1: only the classes the drain touched are inspected
                    // (O(popcount(changed_classes))), not all SMALL_CLASS_COUNT.
                    #[cfg(feature = "alloc-segment-directory")]
                    {
                        let slot_idx = SegmentHeader::segment_id_at(base) as usize;
                        self.sync_directory_for_segment_classes(base, slot_idx, changed_classes);
                    }
                    // Mechanism 2 (task #51): now that the drain is complete, the
                    // emptied segment is routed through the pool/release
                    // decision — either RETAINED in the pool (kept registered +
                    // committed, free-lists intact) or RELEASED (OS reservation
                    // freed, slot NULLed). EITHER way we `continue` past the
                    // BinTable check for this base in THIS scan:
                    //   - released → base is unmapped, MUST be skipped;
                    //   - pooled   → the segment JUST emptied on the LAST ring
                    //     entry of this drain; skipping it here (rather than
                    //     immediately handing it back) simply defers its reuse to
                    //     a LATER `find_segment_with_free`, which will find its
                    //     free blocks and `unpool_if_present` it — that later
                    //     free-list reuse (no OS reserve/release) is the
                    //     hysteresis win. Handing it back in the SAME scan is
                    //     unnecessary and keeps the drain loop simple.
                    // Any stale ring entries were already processed (guarded by
                    // the bitmap `is_free` check; and on the release branch the
                    // release-follows reset's `off >= bump` guard covers
                    // subsequent same-drain entries). The decision is deferred to
                    // here — NOT taken mid-drain inside `reclaim_offset` — so the
                    // whole ring is fully drained against still-committed
                    // metadata first.
                    #[cfg(feature = "alloc-decommit")]
                    if decommit_happened {
                        self.release_or_pool_empty_segment(base);
                        continue;
                    }
                    // Refresh the cache with the drain's actual final head —
                    // NOT `ring.tail_relaxed()`'s pre-drain snapshot, so a
                    // producer that reserved (but had not yet published) a
                    // slot at drain time is correctly NOT counted as "seen"
                    // (the drain stopped at the unpublished slot; the next
                    // guard check must still observe `tail != new_head` and
                    // drain again to pick it up — see the module doc's
                    // "later drain picks it up" contract).
                    meta_for_ring.set_ring_drain_head(new_head);
                }
            }
            let meta = SegmentMeta::new(base);
            let bt = meta.bin_table();
            if bt.head(class_idx) != FREE_LIST_NULL {
                // Phase C (numa-aware): check whether this segment belongs to
                // our NUMA node.  Segments with node_id == NO_NODE_RAW are
                // "unknown" — treat them as local (no penalty, and on platforms
                // without NUMA they all carry NO_NODE_RAW so this degrades
                // gracefully to the pre-NUMA single-pass behaviour).
                #[cfg(feature = "numa-aware")]
                {
                    let seg_node = meta.node_id_of();
                    if seg_node != my_node && seg_node != super::segment_header::NO_NODE_RAW {
                        // Foreign-node segment with a free block.  Remember as
                        // fallback if we find nothing local, then keep scanning.
                        if fallback.is_none() {
                            fallback = Some(base);
                        }
                        continue;
                    }
                    // Local or unknown node — use it immediately.
                    // Mechanism 2 (task #51): if this segment was RETAINED in the
                    // pool (empty, committed), it is now being reused — remove it
                    // from the pool so it is not later re-pooled a second time (a
                    // double-entry that would double-recycle its base). This is
                    // the hysteresis WIN: the emptied segment's free blocks are
                    // re-served here with no OS reserve/release round-trip.
                    #[cfg(feature = "alloc-decommit")]
                    self.unpool_if_present(base);
                    // R8-2 (task #215): if we reached the linear scan via the
                    // periodic re-validation branch (directory feature ON and
                    // streak hit the period), the directory's MISS was wrong —
                    // this segment actually has a free block. Self-heal the bit
                    // in-place and bump the canary counter.
                    #[cfg(feature = "alloc-segment-directory")]
                    if periodic_revalidation_active || rescue {
                        let slot_idx = SegmentHeader::segment_id_at(base) as usize;
                        self.publish_nonempty(base, class_idx, slot_idx);
                        // R9-8: only the PERIODIC re-validation path bumps the
                        // `DIRECTORY_MISS_SELF_HEAL` canary; the rescue path is
                        // counted separately by `DIRECTORY_RESCUE_OOM_AVOIDED`
                        // at its caller, so the two drift signals stay
                        // distinguishable in diagnostics.
                        #[cfg(feature = "alloc-stats")]
                        if periodic_revalidation_active {
                            super::directory_stats::DIRECTORY_MISS_SELF_HEAL
                                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    return Some(base);
                }
                // Without numa-aware: same as before — return the first match.
                #[cfg(not(feature = "numa-aware"))]
                {
                    // Mechanism 2 (task #51): un-pool on reuse (see the
                    // numa-aware arm above for the double-pool rationale).
                    #[cfg(feature = "alloc-decommit")]
                    self.unpool_if_present(base);
                    // R8-2 (task #215): periodic-re-validation self-heal —
                    // see the numa-aware arm above for the full rationale.
                    #[cfg(feature = "alloc-segment-directory")]
                    if periodic_revalidation_active || rescue {
                        let slot_idx = SegmentHeader::segment_id_at(base) as usize;
                        self.publish_nonempty(base, class_idx, slot_idx);
                        // R9-8: only the PERIODIC re-validation path bumps the
                        // `DIRECTORY_MISS_SELF_HEAL` canary; the rescue path is
                        // counted separately by `DIRECTORY_RESCUE_OOM_AVOIDED`
                        // at its caller, so the two drift signals stay
                        // distinguishable in diagnostics.
                        #[cfg(feature = "alloc-stats")]
                        if periodic_revalidation_active {
                            super::directory_stats::DIRECTORY_MISS_SELF_HEAL
                                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    return Some(base);
                }
            }
        }
        // First pass found no local segment with a free block; fall back to
        // the first foreign-node segment we recorded (or None if everything is
        // empty / all recycled).
        #[cfg(feature = "numa-aware")]
        {
            // Mechanism 2 (task #51): un-pool the fallback on reuse too.
            #[cfg(feature = "alloc-decommit")]
            if let Some(fb) = fallback {
                self.unpool_if_present(fb);
            }
            // R8-2 (task #215): periodic-re-validation self-heal — see the
            // local-hit arm above for the full rationale. Fires on the
            // foreign-fallback success path under the same condition.
            #[cfg(feature = "alloc-segment-directory")]
            if periodic_revalidation_active || rescue {
                if let Some(fb) = fallback {
                    let slot_idx = SegmentHeader::segment_id_at(fb) as usize;
                    self.publish_nonempty(fb, class_idx, slot_idx);
                    #[cfg(feature = "alloc-stats")]
                    if periodic_revalidation_active {
                        super::directory_stats::DIRECTORY_MISS_SELF_HEAL
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                }
            }
            fallback
        }
        #[cfg(not(feature = "numa-aware"))]
        None
    }

    /// R7-A3 / R11-6: validate ONE directory candidate (segment-table slot
    /// `slot_idx`) for `class_idx`. This is the SINGLE choke point for
    /// candidate validation — called by both the flat (non-NUMA) and
    /// node-indexed (NUMA) directory scans so the validation criteria are
    /// byte-for-byte identical (base non-null, Small/Primordial kind, ring
    /// drain, BinTable head non-null). Returns `Some(base)` on a valid hit;
    /// returns `None` (after self-healing any stale bit) if the candidate is
    /// stale/empty/decommitted, so the caller continues to the next candidate.
    #[cfg(feature = "alloc-segment-directory")]
    #[inline]
    fn validate_directory_candidate<
        #[cfg(feature = "alloc-xthread")] F: Fn(*mut u8, usize) -> bool,
    >(
        &mut self,
        class_idx: usize,
        slot_idx: usize,
        #[cfg(feature = "alloc-xthread")] is_in_magazine: &F,
    ) -> Option<*mut u8> {
        // Validation step 1: base must be non-null.
        let base = self.table.base_at(slot_idx);
        if base.is_null() {
            // Stale bit for a recycled slot — clear it (publish_empty with a
            // null base clears across ALL node buckets).
            self.publish_empty(base, class_idx, slot_idx);
            #[cfg(feature = "alloc-stats")]
            super::directory_stats::DIRECTORY_STALE_HITS
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }

        // Validation step 2: must be Small/Primordial.
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            // A large segment somehow had a stale bit — clear.
            self.publish_empty(base, class_idx, slot_idx);
            #[cfg(feature = "alloc-stats")]
            super::directory_stats::DIRECTORY_STALE_HITS
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }

        // P1-a: lazily drain this segment's remote-free ring BEFORE inspecting
        // BinTable — exactly as the linear scan does. Cross-thread frees
        // sitting in the ring are invisible to the BinTable until drained.
        #[cfg(feature = "alloc-xthread")]
        {
            let mut meta_for_ring = SegmentMeta::new(base);
            let ring = meta_for_ring.remote_ring();
            let cached_head = meta_for_ring.ring_drain_head_of();
            if ring.tail_relaxed() != cached_head {
                let small_cur = self.small_cur;
                #[cfg(feature = "alloc-decommit")]
                let mut decommit_happened = false;
                // R8-1 (task #214): accumulate the set of classes this drain
                // pass touches, for an O(popcount) post-drain directory sync.
                let mut changed_classes: u64 = 0;
                let new_head = ring.drain(|off| {
                    #[cfg(feature = "fastbin")]
                    let reclaimed = Self::reclaim_offset_checked(base, off, &is_in_magazine);
                    #[cfg(not(feature = "fastbin"))]
                    let reclaimed = Self::reclaim_offset(base, off);
                    if reclaimed {
                        #[cfg(feature = "alloc-decommit")]
                        if Self::dec_live_and_maybe_decommit(base, small_cur) {
                            decommit_happened = true;
                        }
                        changed_classes |= 1u64 << super::remote_free_ring::entry_class_idx(off);
                    }
                });
                // A2 post-drain directory sync.
                {
                    let sid = SegmentHeader::segment_id_at(base) as usize;
                    self.sync_directory_for_segment_classes(base, sid, changed_classes);
                }
                // P1-b: decommit/pool hysteresis — same as the linear scan. A
                // decommitted segment must be skipped.
                #[cfg(feature = "alloc-decommit")]
                if decommit_happened {
                    self.release_or_pool_empty_segment(base);
                    return None;
                }
                // P1-d: refresh the ring_drain_head cache.
                meta_for_ring.set_ring_drain_head(new_head);
            }
        }

        // Validation step 3: BinTable head STILL non-null?
        let meta = SegmentMeta::new(base);
        let bt = meta.bin_table();
        if bt.head(class_idx) == FREE_LIST_NULL {
            // Stale positive: drain may have emptied the class, or a concurrent
            // transition cleared it. Clear the bit and try the next candidate.
            self.publish_empty(base, class_idx, slot_idx);
            #[cfg(feature = "alloc-stats")]
            super::directory_stats::DIRECTORY_STALE_HITS
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }

        // Valid hit — do the SAME work as the linear scan on hit.
        // P1-c: un-pool if this segment was retained in the pool.
        #[cfg(feature = "alloc-decommit")]
        self.unpool_if_present(base);

        #[cfg(feature = "alloc-stats")]
        super::directory_stats::DIRECTORY_HITS.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        Some(base)
    }

    /// Pop a free block of `class_idx` from `segment`'s bin table. Returns
    /// null if the free list is empty. Writes the block's `next` word to null
    /// (it becomes the new head) via the node seam.
    #[inline(always)]
    fn pop_free(
        &mut self,
        segment: *mut u8,
        class_idx: usize,
        block_size: usize,
    ) -> Option<*mut u8> {
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
                super::segment_header::bump_gen(segment, head_off as usize)
            };
        }
        let _ = block_size; // block_size is the caller's invariant; not needed here.
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
    pub(super) fn drain_freelist_batch(
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
    fn carve_block(&mut self, class_idx: usize, block_size: usize) -> Option<*mut u8> {
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
            // Under `alloc-lazy-commit`, the retain-decommit (B3) only
            // decommitted `[meta_end + LAZY_FIRST_CHUNK, SEGMENT)` and kept
            // the initial chunk `[meta_end, meta_end + LAZY_FIRST_CHUNK)`
            // committed. The frontier was already reset to
            // `meta_end + LAZY_FIRST_CHUNK` by the decommit path. So we do
            // NOT need a recommit syscall here — the first chunk is already
            // committed and ready for carving. Just clear the `decommitted`
            // flag and let B2's grow-on-carve logic recommit additional
            // chunks incrementally as the bump cursor advances past the
            // frontier. This is the lazy savings: reuse never touches the
            // upper payload until it is actually needed.
            //
            // On the eager path (feature-OFF), the full-payload recommit is
            // kept: the decommit decommitted the WHOLE payload
            // `[meta_end, SEGMENT)`, so recommit must bring it all back.
            #[cfg(feature = "alloc-lazy-commit")]
            {
                // The initial chunk is already committed. The frontier was
                // reset by the decommit path. Just clear the flag.
                meta.set_decommitted(false);
            }
            #[cfg(not(feature = "alloc-lazy-commit"))]
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
        // path when carving past the current frontier.
        #[cfg(feature = "alloc-lazy-commit")]
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
        let mut pm = meta.page_map();
        let page = aligned_bump / super::os::PAGE;
        if pm.class_of(page).is_none() {
            // Page was Free or Meta; dedicate it to this class.
            pm.set_class(page, class_idx);
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
    pub(super) fn carve_batch(
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
            // identical block for the full rationale).
            #[cfg(feature = "alloc-lazy-commit")]
            {
                meta.set_decommitted(false);
            }
            #[cfg(not(feature = "alloc-lazy-commit"))]
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
        #[cfg(feature = "alloc-lazy-commit")]
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
        let mut pm = meta.page_map();
        let mut prev_page = usize::MAX;
        for (i, slot) in out[..n].iter_mut().enumerate() {
            let off = aligned_start + i * block_size;
            let page = off / super::os::PAGE;
            if page != prev_page {
                if pm.class_of(page).is_none() {
                    pm.set_class(page, class_idx);
                }
                prev_page = page;
            }
            *slot = Node::deref(segment, off);
        }
        n
    }

    /// Deallocate a small block: push it onto its owning segment's class free
    /// list. `ptr` is the block address; `base` is its segment base (computed
    /// by the caller via `segment_of`).
    ///
    /// **Double-free guard (M2 — Phase 13.4a):** before pushing, we test the
    /// segment's [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) bit for this
    /// block. If it is already FREE (`is_free` true → the block is on some free
    /// list of this segment), this is a double-free: we no-op (never corrupt the
    /// free list — no self-loop, no duplicate). Otherwise we set the bit
    /// (`mark_free`) and push. This replaces the Phase 8 O(free-list-length)
    /// `free_list_contains` walk — which made own-thread free O(N²) under churn
    /// (#41) — with an O(1) exact bit test. The bitmap is single-writer (owner
    /// only), so the read/modify/write needs no atomics.
    #[inline(always)]
    pub(super) fn dealloc_small(&mut self, base: *mut u8, ptr: *mut u8, class_idx: usize) {
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

    // ── end Phase 3 ──────────────────────────────────────────────────────────

    /// Reserve a fresh small segment, initialise its metadata, register it,
    /// and set it as the current small segment. Returns its base.
    pub(super) fn reserve_small_segment(&mut self) -> Option<*mut u8> {
        // Mechanism 2 (task #51): this path is reached only when NO registered
        // segment — including any POOLED empty segment — has a free block of the
        // requested class (`find_segment_with_free` already scanned them all,
        // pooled included, and REMOVED any it reused from the pool). A pooled
        // segment is fully-carved (bump near `SEGMENT` end), so it cannot serve
        // as a FRESH carve target for a class its free list lacks — that is why
        // the pool is drawn from via `find_segment_with_free`'s free-list reuse
        // (the hysteresis win: the emptied segment's blocks are re-served with
        // no OS work), NOT popped as `small_cur` here.
        //
        // R8-10 (task #223): this holds identically under `alloc-lazy-commit`.
        // A prior design (B3, R7 Workstream B) popped a pooled segment here as
        // a "clean carve target", relying on pool admission having decommit-
        // reset it (bump at payload_start, free lists cleared,
        // `is_decommitted=true`). That reset was itself the 50-75× regression
        // (see `release_or_pool_empty_segment`'s doc comment) — with admission
        // fixed to never reset, a pooled segment under lazy-commit is a
        // partially-carved segment with a live free list, indistinguishable
        // from the eager leg's pooled segment. It is therefore reused the same
        // way: via `find_segment_with_free`'s free-list path, never popped here.
        //
        // This is the cold small-path clock edge, so trim any stale pooled
        // segment here (cheap: fast early-exit when the pool is empty; one
        // `Instant::now()` at most, only when something is pooled).
        #[cfg(feature = "alloc-decommit")]
        self.maybe_decay_small_pool();

        // Phase C (numa-aware): determine the calling thread's NUMA node
        // BEFORE the reservation so we can pass it to `reserve_aligned_on_node`
        // (Windows requires the node at reserve-time via VirtualAllocExNuma;
        // Linux can bind post-mmap, but we unify the paths here). R11-5: this
        // is the cached accessor — typically the same value the most recent
        // `find_segment_with_free` already populated.
        #[cfg(feature = "numa-aware")]
        let my_node = self.current_node_cached();

        // Reserve one SEGMENT's worth of virtual address space.
        // Under numa-aware we call the NUMA-steering path; otherwise the plain
        // OS path.  The returned triple always provides (base, reservation,
        // reservation_len) with the same semantics as Segment::reserve.
        #[cfg(feature = "numa-aware")]
        let (base, reservation, reservation_len) = {
            let reserved = numa::reserve_aligned_on_node(SEGMENT, my_node);
            // Mechanism 2 (task #51 / follow-up): pool-drain-and-retry on OS
            // reservation failure, mirroring `alloc_large`'s identical guard
            // — the pool is a reclaimable soft reserve, never a hard pin
            // under memory pressure, even for a plain small-segment reserve.
            #[cfg(feature = "alloc-decommit")]
            let reserved = match reserved {
                Some(t) => Some(t),
                None if self.pooled_count > 0 => {
                    self.drain_small_pool();
                    numa::reserve_aligned_on_node(SEGMENT, my_node)
                }
                None => None,
            };
            let (b, r, rl) = reserved?;
            (b.as_ptr(), r, rl)
        };
        #[cfg(not(feature = "numa-aware"))]
        let (base, reservation, reservation_len) = {
            // B1 (R7 Workstream B): under `alloc-lazy-commit` (Windows-only
            // lazy path; Unix/miri eager fallback), reserve the 4 MiB segment
            // WITHOUT committing the whole payload. Only the metadata region
            // and the first payload chunk are committed; the rest stays
            // reserved-but-uncommitted. On the eager path (feature-OFF) or
            // under miri/Unix, `reserve_aligned_lazy` falls back to the eager
            // `reserve_aligned` internally, so the observable behavior is
            // identical.
            //
            // The NUMA path (above) always uses the eager
            // `reserve_aligned_on_node` — NUMA reservations go through
            // VirtualAllocExNuma and must not be disturbed (P2 gate).
            #[cfg(feature = "alloc-lazy-commit")]
            let mut seg = {
                let meta_end = SegLayout::small_meta_end();
                // initial_commit = metadata pages + first payload chunk.
                // Both `meta_end` and `LAZY_FIRST_CHUNK` are page-aligned,
                // so their sum is page-aligned (vmem contract).
                let initial_commit = meta_end + LAZY_FIRST_CHUNK;
                // `initial_commit <= SEGMENT` by construction: meta_end is
                // ~80 KiB (non-hardened) or ~336 KiB (hardened), and
                // LAZY_FIRST_CHUNK is 256 KiB, so their sum is well under
                // 4 MiB. The const-assert below pins this.
                debug_assert!(initial_commit <= SEGMENT);
                aligned_vmem::reserve_aligned_lazy(SEGMENT, SEGMENT, initial_commit).inspect(|_| {
                    os::SEGMENTS_RESERVED_TOTAL.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                })
            };
            #[cfg(not(feature = "alloc-lazy-commit"))]
            let mut seg = Segment::reserve(SEGMENT);
            // Mechanism 2 (task #51 / follow-up): same pool-drain-and-retry
            // guard as the numa-aware arm above.
            #[cfg(feature = "alloc-decommit")]
            if seg.is_none() && self.pooled_count > 0 {
                self.drain_small_pool();
                #[cfg(feature = "alloc-lazy-commit")]
                {
                    let meta_end = SegLayout::small_meta_end();
                    let initial_commit = meta_end + LAZY_FIRST_CHUNK;
                    seg = aligned_vmem::reserve_aligned_lazy(SEGMENT, SEGMENT, initial_commit)
                        .inspect(|_| {
                            os::SEGMENTS_RESERVED_TOTAL
                                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        });
                }
                #[cfg(not(feature = "alloc-lazy-commit"))]
                {
                    seg = Segment::reserve(SEGMENT);
                }
            }
            #[cfg(feature = "alloc-lazy-commit")]
            {
                let reservation = seg?;
                let b = reservation.as_ptr();
                // `reservation_ptr()` is always non-null per the
                // aligned_vmem::Reservation contract (checked at construction).
                // Use `new` + `?` to propagate as OOM rather than panic.
                let r = core::ptr::NonNull::new(reservation.reservation_ptr())?;
                let rl = reservation.reservation_len();
                core::mem::forget(reservation);
                (b, r, rl)
            }
            #[cfg(not(feature = "alloc-lazy-commit"))]
            {
                let segment = seg?;
                let b = segment.as_ptr();
                let r = segment.reservation();
                let rl = segment.reservation_len();
                core::mem::forget(segment);
                (b, r, rl)
            }
        };

        // no-panic: register returns None if the segment table is full. We
        // must release the reservation we just made before returning None.
        let id = match self.table.register(base) {
            Some(id) => id,
            None => {
                // Release the reservation we just made (we own it now).
                os::release_segment(reservation.as_ptr(), reservation_len);
                return None;
            }
        };
        // Lay down the small header + page map + bin table at the fixed
        // offsets. `bump` starts at the small-meta end (past the metadata).
        let meta_end = SegLayout::small_meta_end();
        let meta_pages = SegLayout::small_meta_pages();
        let mut meta = SegmentMeta::new(base);
        meta.write_header(SegmentHeader::small(
            id,
            meta_end,
            reservation.as_ptr(),
            reservation_len,
        ));
        // Phase C (numa-aware): stamp the NUMA node into the header NOW,
        // immediately after writing it. The header constructor set node_id to
        // NO_NODE_RAW; we overwrite it with the actual node. This must happen
        // BEFORE any carve/alloc so that find_segment_with_free sees the real
        // node on the very first scan that includes this segment.
        #[cfg(feature = "numa-aware")]
        meta.set_node_id(my_node);

        // B1 (R7 Workstream B): stamp the committed-payload frontier.
        //
        // Three-way split, mirroring the THREE platform-specific
        // `reserve_aligned_lazy_raw` implementations in
        // `crates/vmem/src/lib.rs` (the OS-level reservation this function
        // just completed via `Segment::reserve_lazy`):
        //
        //   1. `numa-aware` (any platform): `SEGMENT`. NUMA reservations go
        //      through `numa::reserve_aligned_on_node`, which is ALWAYS eager
        //      (P2 gate — `VirtualAllocExNuma` reservations must not be
        //      disturbed by a later partial commit).
        //
        //   2. `alloc-lazy-commit` AND NOT `numa-aware` AND real Windows (not
        //      miri): `meta_end + LAZY_FIRST_CHUNK`. This is the ONLY platform
        //      where `reserve_aligned_lazy_raw` is GENUINELY lazy — a real
        //      2-phase `VirtualAlloc(MEM_RESERVE)` then
        //      `VirtualAlloc(MEM_COMMIT)` on the metadata + first chunk
        //      prefix. The frontier accurately reflects that partial commit.
        //
        //   3. `alloc-lazy-commit` AND NOT `numa-aware` AND Unix/miri:
        //      `SEGMENT`. Here `reserve_aligned_lazy_raw` ignores
        //      `_initial_commit` and `mmap`s / `alloc`s the WHOLE segment up
        //      front (Unix has no separate reserve/commit distinction the way
        //      Windows does; miri models no RSS). Understating the frontier
        //      at `meta_end + LAZY_FIRST_CHUNK` here used to be SOUND but
        //      POINTLESS (R8-5, task #218): every carve past the artificial
        //      frontier still ran through B2's grow-on-carve path (bounds
        //      check + a `commit_pages` call that is a correctness no-op on
        //      these platforms per `crates/vmem`'s own `commit_range_impl`
        //      for unix/miri + an atomic `GROW_COMMIT_COUNT` bump) for zero
        //      behavioral benefit. Stamping `SEGMENT` immediately restores
        //      the `alloc-lazy-commit` feature's promised zero-cost-when-
        //      unneeded property on Unix/miri.
        //
        // B2 wired: grow-on-carve is still live on the genuine Windows-lazy
        // path (case 2) — when a carve would exceed `committed_payload_end`,
        // the carve path commits the next chunk(s) and advances the frontier
        // before advancing bump. The first chunk (LAZY_FIRST_CHUNK) covers
        // initial carving; subsequent chunks are grown incrementally
        // (GROW_CHUNK) by carve_block/carve_batch. B3 will reset the frontier
        // after a decommit; B5 sweeps the chunk sizes.
        #[cfg(feature = "alloc-lazy-commit")]
        {
            #[cfg(feature = "numa-aware")]
            meta.set_committed_payload_end(SEGMENT);
            #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
            meta.set_committed_payload_end(meta_end + LAZY_FIRST_CHUNK);
            #[cfg(all(not(feature = "numa-aware"), any(not(windows), miri)))]
            meta.set_committed_payload_end(SEGMENT);
        }

        PageMap::init_in_place(base_add(base, SegLayout::page_map_off()), meta_pages);
        BinTable::init_in_place(base_add(base, SegLayout::bin_table_off()) as *mut u32);
        // Initialise the per-segment alloc-bitmap (Phase 13.4a double-free
        // guard) to all-zeros; bits flip to FREE as blocks are pushed.
        //
        // PERF-PASS-2 (G5/C1, task #50): under `cfg(not(miri))` this init is
        // SKIPPED — `base` is a segment JUST reserved fresh from the OS via
        // `Segment::reserve`/`numa::reserve_aligned_on_node` a few lines above
        // (never carved, never decommit-reset), and the OS guarantees fresh
        // pages read as zero (Windows `MEM_COMMIT` demand-zero; POSIX
        // anonymous `mmap` zero-fill — see `crates/vmem/src/lib.rs`'s reserve
        // paths). `AllocBitmap::init_in_place`'s target state is ALL ZEROS
        // (see its doc comment), so writing zero over memory the OS already
        // handed back as zero is a tautology. Skipping it avoids dirtying
        // `AllocBitmap::FOOTPRINT` (32 KiB / 8 pages for the default
        // SEGMENT/MIN_BLOCK pair) of metadata pages that would otherwise fault
        // in eagerly instead of lazily.
        //
        // Under `miri` this is NOT skipped: `crates/vmem/src/lib.rs`'s miri
        // fallback aperture is `std::alloc::alloc`, which is NOT guaranteed
        // zeroed — so miri keeps the explicit zero-init, exactly as before.
        //
        // This is NOT the rejected P4(b) `alloc_zeroed` virgin-skip (that NO-GO
        // was about *user-visible payload* virginity, where macOS
        // `MADV_DONTNEED` laziness on a RECYCLED (not freshly-reserved) mapping
        // makes "recycled == zero" an unsound assumption). Here the virgin
        // signal is exact — this function only ever runs immediately after a
        // fresh OS reservation, never on a decommit-reused segment (that path
        // is `decommit_empty_segment_impl`'s `release_follows=false` full
        // reset, which keeps its own explicit `AllocBitmap::init_in_place`
        // call unconditionally — see PERF-PASS-2 report / task #50) — and it
        // is metadata, not payload the user could have observed/mutated.
        #[cfg(miri)]
        super::alloc_bitmap::AllocBitmap::init_in_place(base_add(
            base,
            SegLayout::alloc_bitmap_off(),
        ));
        // RAD-5 (E4) GO/NO-GO EXPERIMENT: same virgin-skip discipline extended
        // to the second (magazine-residency) bitmap — see
        // `magazine_bitmap.rs`'s module doc. Skipped under `cfg(not(miri))`
        // for the identical reason as the line above.
        #[cfg(miri)]
        super::magazine_bitmap::MagazineBitmap::init_in_place(base_add(
            base,
            SegLayout::magazine_bitmap_off(),
        ));
        // Initialise the per-segment remote-free ring (Variant-2 fix). Only
        // under `alloc-xthread`; the Layout always reserves the bytes.
        #[cfg(feature = "alloc-xthread")]
        {
            super::remote_free_ring::RemoteFreeRing::init_in_place(
                base,
                SegLayout::remote_ring_off(),
            );
        }
        // X7 Ф3 (task #191): zero the per-segment generation table under
        // `hardened`. Compiled ONLY under `hardened`; under any other feature
        // the table does not exist and this call is absent (byte-identical to
        // the pre-X7 build). Closes the carried-over Ф1 gap: without this
        // zeroing, a `gen_at`/`bump_gen` Relaxed load on a never-written cell
        // is UB. NOT re-zeroed on decommit-reset (plan §2.2: generation
        // numbering is continuous across decommit-reset by design).
        #[cfg(feature = "hardened")]
        {
            // SAFETY: `base` is a live, exclusively-owned segment whose
            // generation table is carved and writable.
            #[allow(unsafe_code)]
            unsafe {
                super::segment_header::init_gen_table_in_place(base)
            };
        }
        // R7-A1: check whether the segment count has crossed the directory
        // materialisation threshold. If so, materialize the sidecar and do
        // the one-time rebuild. This is a lazy, one-shot operation: once the
        // pointer is non-null, subsequent calls are a single null-check.
        #[cfg(feature = "alloc-segment-directory")]
        self.maybe_materialize_directory();

        self.small_cur = base;
        Some(base)
    }
}

// ── R7-A1: directory sidecar materialisation ────────────────────────────────

/// R11-6: read the NUMA `node_id` from a segment header for directory bitmap
/// routing. Under `numa-aware`, reads `SegmentMeta::node_id_of()`. Under
/// non-`numa-aware`, returns 0 unconditionally (the single-bucket directory
/// ignores the value; `node_id_of` is cfg-gated out). Returns 0 for a null
/// base (callers that clear stale bits handle the null case separately via
/// `clear_bit_all_nodes`).
#[cfg(feature = "alloc-segment-directory")]
#[inline]
fn directory_node_id_of(base: *mut u8) -> u32 {
    #[cfg(feature = "numa-aware")]
    {
        if base.is_null() {
            0
        } else {
            SegmentMeta::new(base).node_id_of()
        }
    }
    #[cfg(not(feature = "numa-aware"))]
    {
        let _ = base;
        0
    }
}

#[cfg(feature = "alloc-segment-directory")]
impl AllocCore {
    /// Check whether the directory sidecar should be materialised and, if
    /// so, reserve it and rebuild the bitmap from the current segment table.
    ///
    /// Called after every successful `table.register()` on the small-segment
    /// path. Fast path (already materialised OR below threshold): one
    /// null-check + one u32 comparison. Slow path (first materialisation):
    /// one OS VM reservation + one full-table-scan rebuild.
    ///
    /// Sidecar OOM is NOT allocator OOM: on reserve failure, the pointer
    /// stays null and the mechanism is simply off (the linear scan fallback
    /// is used, unchanged from today). Never abort.
    pub(super) fn maybe_materialize_directory(&mut self) {
        // Fast path: already materialised.
        if !self.directory_sidecar.is_null() {
            return;
        }
        // Below threshold: not worth materialising yet.
        if self.table.count() < super::segment_directory::DIRECTORY_MATERIALIZE_THRESHOLD {
            return;
        }
        // Slow path: reserve the sidecar via direct OS VM (M5-clean).
        let ptr = match os::reserve_directory_sidecar() {
            Some(p) => p,
            None => return, // OOM — mechanism stays off, not an error.
        };
        // One-time rebuild: walk every registered small/primordial segment,
        // read each class's BinTable head, set the exact class_nonempty bits.
        // The sidecar was OS-zeroed (all bits clear), so only non-empty heads
        // need to be SET.
        let dir = os::deref_directory_sidecar_mut(ptr);
        dir.rebuild_from_table(&self.table);

        self.directory_sidecar = ptr;
    }

    /// Return a shared reference to the materialised directory sidecar, or
    /// `None` if not yet materialised.
    #[inline]
    pub(super) fn directory(&self) -> Option<&super::segment_directory::SegmentDirectory> {
        if self.directory_sidecar.is_null() {
            None
        } else {
            Some(os::deref_directory_sidecar(self.directory_sidecar))
        }
    }

    /// Return a mutable reference to the materialised directory sidecar, or
    /// `None` if not yet materialised.
    #[inline]
    pub(super) fn directory_mut(
        &mut self,
    ) -> Option<&mut super::segment_directory::SegmentDirectory> {
        if self.directory_sidecar.is_null() {
            None
        } else {
            Some(os::deref_directory_sidecar_mut(self.directory_sidecar))
        }
    }

    // ── R7-A2: centralized empty↔non-empty transition helpers ──────────────
    //
    // These are the SINGLE choke point for directory bitmap maintenance. Every
    // site that mutates a per-class BinTable head calls one of these three
    // helpers to keep the directory in sync. The helpers are cheap no-ops when
    // the sidecar is not materialised (below threshold, OOM-disabled, or
    // feature OFF).
    //
    // R11-6: the helpers derive the segment's NUMA node from `base` and route
    // the bit-set/clear to the correct per-node bucket under `numa-aware`.
    // Under non-`numa-aware` the node dimension is inert (single bucket).

    /// R7-A2 / R11-6: notify the directory that class `class_idx` in segment
    /// slot `slot_idx` (segment base `base`) transitioned from empty to
    /// non-empty (old_head was FREE_LIST_NULL, new_head is not). Sets the
    /// corresponding bit in the segment's node bucket.
    ///
    /// No-op if the directory is not materialised.
    #[inline]
    pub(super) fn publish_nonempty(&mut self, base: *mut u8, class_idx: usize, slot_idx: usize) {
        if let Some(dir) = self.directory_mut() {
            dir.set_bit(directory_node_id_of(base), class_idx, slot_idx);
        }
    }

    /// R7-A2 / R11-6: notify the directory that class `class_idx` in segment
    /// slot `slot_idx` (segment base `base`) transitioned from non-empty to
    /// empty (old_head was not FREE_LIST_NULL, new_head is FREE_LIST_NULL).
    /// Clears the corresponding bit in the segment's node bucket.
    ///
    /// If `base` is null (the stale-bit-clearing path in directory validation,
    /// where the segment was already recycled), the node is unknowable so the
    /// bit is cleared across ALL node buckets (`clear_bit_all_nodes`).
    ///
    /// No-op if the directory is not materialised.
    #[inline]
    pub(super) fn publish_empty(&mut self, base: *mut u8, class_idx: usize, slot_idx: usize) {
        if let Some(dir) = self.directory_mut() {
            if base.is_null() {
                dir.clear_bit_all_nodes(class_idx, slot_idx);
            } else {
                dir.clear_bit(directory_node_id_of(base), class_idx, slot_idx);
            }
        }
    }

    /// R7-A2: clear ALL class bits for segment slot `slot_idx`. Called on
    /// segment recycle/release so a reused slot does not inherit stale bits
    /// from the old segment lifetime.
    ///
    /// No-op if the directory is not materialised.
    #[inline]
    pub(super) fn clear_segment_directory(&mut self, slot_idx: usize) {
        if let Some(dir) = self.directory_mut() {
            dir.clear_slot(slot_idx);
        }
    }

    /// R8-1 (task #214): sync the directory for segment at `base` / `slot_idx`
    /// by inspecting ONLY the classes whose bit is set in `changed_classes` —
    /// the set of classes a ring-drain pass actually touched — instead of
    /// sweeping all `SMALL_CLASS_COUNT` classes. O(popcount(`changed_classes`))
    /// reads instead of O(`SMALL_CLASS_COUNT`). This closes the
    /// O(D × SMALL_CLASS_COUNT) remote-dirty directory-sync regression: a
    /// drain that reclaims blocks of (say) 2 classes does 2 reads, not 49.
    ///
    /// `changed_classes` is accumulated by a ring-drain closure via
    /// `changed_classes |= 1u64 << entry_class_idx(off)` for every drained
    /// entry, so it carries every distinct class the pass touched regardless of
    /// how many entries each class contributed.
    ///
    /// R11-6: derives the node bucket from `base` and routes each set/clear to
    /// the correct per-node bucket.
    ///
    /// No-op if the directory is not materialised or `changed_classes == 0`.
    pub(crate) fn sync_directory_for_segment_classes(
        &mut self,
        base: *mut u8,
        slot_idx: usize,
        changed_classes: u64,
    ) {
        if changed_classes == 0 {
            return;
        }
        if let Some(dir) = self.directory_mut() {
            let node_id = directory_node_id_of(base);
            let meta = SegmentMeta::new(base);
            let bt = meta.bin_table();
            let mut bits = changed_classes;
            while bits != 0 {
                let c = bits.trailing_zeros() as usize;
                bits &= bits - 1; // clear lowest set bit
                if bt.head(c) != FREE_LIST_NULL {
                    dir.set_bit(node_id, c, slot_idx);
                } else {
                    dir.clear_bit(node_id, c, slot_idx);
                }
            }
        }
    }

    /// R7-A4: drain all dirty segments' remote-free rings. Called at the top
    /// of `find_segment_with_free_impl` BEFORE the directory scan, so the
    /// directory bits reflect the latest cross-thread frees.
    ///
    /// For each dirty word, `swap(0, Acquire)`. For each set bit:
    ///   1. `base_at(slot_idx)` — skip if null (recycled slot, stale bit).
    ///   2. Validate kind is Small/Primordial.
    ///   3. Validate `segment_id_at(base) == slot_idx` (revalidation: a slot
    ///      may have been recycled and reused for a different segment since
    ///      the producer set the bit).
    ///   4. Drain the segment's remote-free ring (REUSING the existing
    ///      drain body from the directory-hit path — P1-compliant).
    ///   5. `sync_directory_for_segment_classes` to publish reclaimed blocks
    ///      into the directory (R8-1: only the classes the drain touched).
    ///   6. Handle decommit/pool hysteresis.
    ///   7. Refresh the `ring_drain_head` cache.
    ///
    /// Increments the `dirty_segments_drained` A0 counter per drained segment.
    ///
    /// R9-6 (class-aware dirty routing judge, measurement-only): additionally
    /// increments `wasted_dirty_drains` for each drained segment whose ring,
    /// once drained, produced ZERO reclaimed blocks of the `class_idx` the
    /// caller is searching for — those are drains that per-(segment,class)
    /// dirty routing would have avoided entirely. This is purely diagnostic
    /// (no algorithmic change); the comparison happens once per successful
    /// drain against the R8-1 `changed_classes` bitmap the loop already
    /// accumulates.
    ///
    /// No-op if `dirty_segments` is not bound (pre-bind window) or the
    /// directory sidecar is not materialised.
    #[cfg(feature = "alloc-xthread")]
    pub(super) fn drain_dirty_segments<
        #[cfg(feature = "fastbin")] F: Fn(*mut u8, usize) -> bool,
    >(
        &mut self,
        #[cfg_attr(not(feature = "alloc-stats"), allow(unused_variables))] class_idx: usize,
        #[cfg(feature = "fastbin")] is_in_magazine: &F,
    ) {
        let ds = match self.dirty_segments {
            Some(ds) => ds,
            None => return, // Pre-bind: no dirty bitmap.
        };
        // The directory must be materialised for dirty routing to be useful.
        if self.directory_sidecar.is_null() {
            return;
        }
        let small_cur = self.small_cur;

        for (w, ds_word) in ds.iter().enumerate() {
            // Acquire: pairs with the producer's Release fetch_or.
            let dirty = ds_word.swap(0, core::sync::atomic::Ordering::Acquire);
            if dirty == 0 {
                continue;
            }
            let mut bits = dirty;
            while bits != 0 {
                let j = bits.trailing_zeros() as usize;
                bits &= bits - 1; // clear lowest set bit
                let slot_idx = w * 64 + j;

                // Validation 1: base must be non-null.
                let base = self.table.base_at(slot_idx);
                if base.is_null() {
                    continue; // Recycled slot, stale dirty bit.
                }

                // Validation 2: must be Small/Primordial.
                if !matches!(
                    SegmentHeader::kind_at(base),
                    SegmentKind::Small | SegmentKind::Primordial
                ) {
                    continue;
                }

                // Validation 3: segment_id must match slot_idx (revalidation
                // against slot recycle — a recycled-then-reused slot may hold
                // a different segment whose segment_id != the old slot_idx
                // the producer saw when setting the bit).
                if SegmentHeader::segment_id_at(base) as usize != slot_idx {
                    continue;
                }

                // REUSE the existing A3/scan drain body (P1-compliant).
                let mut meta_for_ring = SegmentMeta::new(base);
                let ring = meta_for_ring.remote_ring();
                let cached_head = meta_for_ring.ring_drain_head_of();
                if ring.tail_relaxed() != cached_head {
                    #[cfg(feature = "alloc-decommit")]
                    let mut decommit_happened = false;
                    // R8-1 (task #214): accumulate the set of classes this drain
                    // pass touches, so the post-drain directory sync inspects
                    // ONLY those classes (O(popcount)) instead of re-sweeping
                    // all SMALL_CLASS_COUNT classes.
                    let mut changed_classes: u64 = 0;
                    let new_head = ring.drain(|off| {
                        #[cfg(feature = "fastbin")]
                        let reclaimed = Self::reclaim_offset_checked(base, off, &is_in_magazine);
                        #[cfg(not(feature = "fastbin"))]
                        let reclaimed = Self::reclaim_offset(base, off);
                        if reclaimed {
                            #[cfg(feature = "alloc-decommit")]
                            if Self::dec_live_and_maybe_decommit(base, small_cur) {
                                decommit_happened = true;
                            }
                            // R10-3: gate the class bit on `reclaimed` — a
                            // rejected entry never mutated the BinTable for its
                            // class (every early `return false` in
                            // reclaim_offset[_checked] precedes `set_head`/
                            // `mark_free`), so recording it would (a) cause a
                            // spurious directory sync for an unchanged class
                            // and (b) make the R9-6 WASTED_DIRTY_DRAINS metric
                            // under-count: a drain that rejected every entry of
                            // the sought class still looked "not wasted".
                            changed_classes |=
                                1u64 << super::remote_free_ring::entry_class_idx(off);
                        }
                    });
                    // A2 post-drain directory sync.
                    {
                        let sid = SegmentHeader::segment_id_at(base) as usize;
                        self.sync_directory_for_segment_classes(base, sid, changed_classes);
                    }
                    // R9-6 (class-aware dirty routing judge, measurement-only):
                    // if this drain — triggered by a `find_segment_with_free_impl(class_idx)`
                    // call — produced ZERO reclaimed blocks of the sought class
                    // (the sought class's bit is NOT in `changed_classes`), it
                    // was wasted work from THAT caller's perspective. Per-(segment,
                    // class) dirty routing would have avoided visiting this segment
                    // entirely. Purely diagnostic — no algorithmic effect.
                    #[cfg(feature = "alloc-stats")]
                    if changed_classes & (1u64 << class_idx) == 0 {
                        super::directory_stats::WASTED_DIRTY_DRAINS
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                    // P1-b: decommit/pool hysteresis.
                    #[cfg(feature = "alloc-decommit")]
                    if decommit_happened {
                        self.release_or_pool_empty_segment(base);
                        // The segment is now released/pooled; skip the head
                        // refresh (the segment may be unmapped).
                        // R7-A0: count this dirty segment as drained.
                        #[cfg(feature = "alloc-stats")]
                        super::directory_stats::DIRTY_SEGMENTS_DRAINED
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        continue;
                    }
                    // P1-d: refresh the ring_drain_head cache.
                    meta_for_ring.set_ring_drain_head(new_head);
                }

                // R7-A0: count this dirty segment as drained.
                #[cfg(feature = "alloc-stats")]
                super::directory_stats::DIRTY_SEGMENTS_DRAINED
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
    }
}
