//! Directory-accelerated small-segment lookup for the small path —
//! `carve_block_with_refill`, the `find_segment_with_free*` scan family,
//! and `validate_directory_candidate` (mechanical split of the former flat
//! `alloc_core_small.rs`; pure code movement, no behavior changed).

use crate::alloc_core::os;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL};

use crate::alloc_core::alloc_core::AllocCore;

/// #1993 (alloc_core review P3-1): the outcome of draining ONE segment's
/// remote-free ring via [`AllocCore::drain_segment_ring`].
#[cfg(feature = "alloc-xthread")]
pub(super) enum RingDrainOutcome {
    /// The pre-drain guard found nothing new since the cached head — no
    /// work was done (ring untouched, `ring_drain_head` NOT refreshed).
    Skipped,
    /// The ring was drained and, as a side effect, the segment was fully
    /// emptied and released/pooled by `release_or_pool_empty_segment`. The
    /// caller MUST treat `base` as gone/unmapped for the rest of this pass
    /// — do not read its BinTable or any other metadata. Only constructed
    /// under `alloc-decommit` (the only feature under which a drain can
    /// trigger a decommit at all).
    #[cfg(feature = "alloc-decommit")]
    Decommitted,
    /// The ring was drained; the segment is still live and
    /// `ring_drain_head` has been refreshed. `changed_classes` is the R8-1
    /// accumulator (bitmask of classes the drain touched) — already used to
    /// sync the directory (if materialised); callers that need it for their
    /// own bookkeeping (e.g. the R9-6 wasted-drain diagnostic, the only
    /// current consumer, and `alloc-stats`-gated) can inspect it too.
    Drained {
        #[cfg_attr(not(feature = "alloc-stats"), allow(dead_code))]
        changed_classes: u64,
    },
}

impl AllocCore {
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
    pub(super) fn carve_block_with_refill(
        &mut self,
        class_idx: usize,
        block_size: usize,
    ) -> Option<*mut u8> {
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

    /// #1993 (alloc_core review P3-1): the shared ring-drain body factored
    /// out of what were three near-identical copies (the linear-scan
    /// fallback below, [`validate_directory_candidate`](Self::validate_directory_candidate),
    /// and `directory::drain_dirty_segments`) — "guarded pre-drain check →
    /// `ring.drain(...)` reclaiming each entry → directory sync →
    /// decommit/pool hysteresis → `ring_drain_head` refresh". The drift this
    /// closes was not hypothetical: the R10-3 fix ("gate the class bit on
    /// `reclaimed`", see the comment inline below) had been reasoned through
    /// once and mechanically mirrored twice, which is exactly the shape that
    /// silently diverges on the next edit.
    ///
    /// PERF-PASS-4 (G9/C2, task #52): pre-drain empty-guard. Compares a
    /// cheap Relaxed `tail` load against this segment's owner-cached `head`
    /// (persisted across calls in the segment's OWN header — see
    /// `SegmentHeader::ring_drain_head`'s doc comment for why the cache
    /// lives there and not in `SegmentTable`, and
    /// `RemoteFreeRing::tail_relaxed`'s doc comment for the full soundness
    /// argument). If they match, no producer has reserved a slot since the
    /// last drain (real or guarded) — skip `drain()` entirely, INCLUDING the
    /// unconditional `head.store(_, Release)` it would otherwise perform,
    /// for a ring that has nothing new to report ([`RingDrainOutcome::Skipped`]).
    /// A push landing after this check is exactly as deferred as one landing
    /// after an unconditional drain finishes — the "later drain picks it
    /// up" contract (`remote_free_ring.rs` module docs) is unchanged.
    ///
    /// Slot recycle integration (task #60, `alloc-decommit`): a decommit
    /// this call triggers (via `dec_live_and_maybe_decommit`/
    /// `release_or_pool_empty_segment`) happens AFTER the drain for `base`
    /// is complete — a partially-drained ring still has entries that
    /// `reclaim_offset` processes by reading the segment's metadata (which
    /// stays committed until this call decides to release it).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    pub(super) fn drain_segment_ring<#[cfg(feature = "fastbin")] F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        base: *mut u8,
        #[cfg(feature = "fastbin")] is_in_magazine: &F,
    ) -> RingDrainOutcome {
        let mut meta_for_ring = SegmentMeta::new(base);
        let ring = meta_for_ring.remote_ring();
        let cached_head = meta_for_ring.ring_drain_head_of();
        if ring.tail_relaxed() == cached_head {
            return RingDrainOutcome::Skipped;
        }
        #[cfg(feature = "alloc-decommit")]
        let small_cur = self.small_cur;
        #[cfg(feature = "alloc-decommit")]
        let mut decommit_happened = false;
        // R8-1 (task #214): accumulate the set of classes this drain pass
        // touches, for an O(popcount) post-drain directory sync.
        let mut changed_classes: u64 = 0;
        let new_head = ring.drain(|off| {
            // Task #164: when a magazine exists (fastbin), use the checked
            // variant that consults the magazine predicate before
            // `write_next`, closing the in-magazine leg of the
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
                // R10-3: gate the class bit on `reclaimed` — a rejected
                // entry never mutated the BinTable for its class (every
                // early `return false` in `reclaim_offset[_checked]`
                // precedes `set_head`/`mark_free`), so recording it would
                // (a) cause a spurious directory sync for an unchanged
                // class and (b) make the R9-6 `WASTED_DIRTY_DRAINS` metric
                // under-count: a drain that rejected every entry of the
                // sought class would still look "not wasted".
                changed_classes |=
                    1u64 << crate::alloc_core::remote_free_ring::entry_class_idx(off);
            }
        });
        // R7-A2: sync the directory for this segment after the drain
        // completed. R8-1: only the classes the drain touched are
        // inspected (O(popcount(changed_classes))), not all
        // SMALL_CLASS_COUNT. No-op (and compiled out) without the
        // directory feature.
        #[cfg(feature = "alloc-segment-directory")]
        {
            let slot_idx = SegmentHeader::segment_id_at(base) as usize;
            self.sync_directory_for_segment_classes(base, slot_idx, changed_classes);
        }
        // Mechanism 2 (task #51): now that the drain is complete, an
        // emptied segment is routed through the pool/release decision —
        // caller must skip any further metadata read of `base` this pass.
        #[cfg(feature = "alloc-decommit")]
        if decommit_happened {
            self.release_or_pool_empty_segment(base);
            return RingDrainOutcome::Decommitted;
        }
        // Refresh the cache with the drain's actual final head — NOT
        // `ring.tail_relaxed()`'s pre-drain snapshot, so a producer that
        // reserved (but had not yet published) a slot at drain time is
        // correctly NOT counted as "seen" (see the module doc's "later
        // drain picks it up" contract).
        meta_for_ring.set_ring_drain_head(new_head);
        RingDrainOutcome::Drained { changed_classes }
    }

    #[cfg_attr(
        all(feature = "alloc-xthread", not(feature = "fastbin")),
        allow(unused_variables)
    )]
    #[inline]
    #[allow(unsafe_code)] // R17-2 (task #319): calls the `unsafe fn`s
                          // `os::read_directory_node_bucket` and
                          // `os::read_directory_class_words`. Sound: both call
                          // sites are guarded by the
                          // `!self.directory_sidecar.is_null()` check above (the
                          // pointer is produced only by
                          // `reserve_directory_sidecar`, which fully initialises
                          // it), and `AllocCore`'s owner-only discipline (neither
                          // `Send` nor `Sync`) rules out a concurrent writer. The
                          // reads are by-value; no reference to the sidecar
                          // escapes either call.
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
        // P2 (NUMA two-pass preference) — R11-6 UPDATE: the paragraph that
        // used to be here (pre-R11-6) said the directory-driven lookup is
        // DISABLED under `numa-aware` and that the directory was a
        // write-only index until a future round added node-aware queries.
        // That round happened: R11-6 (task #234) added the node-indexed
        // `class_nonempty_by_node` bitmap and wired the per-bucket scan below
        // (`buckets`/`n_buckets`, built from `os::read_directory_node_bucket`)
        // so the directory-driven lookup IS active under `numa-aware` too —
        // it visits the caller's own node bucket first, then the shared
        // unknown bucket, then every foreign real-node bucket in ascending
        // order, implementing the same local-first / foreign-fallback
        // two-pass preference the linear-scan fallback below independently
        // provides for when the directory is not materialised (below the
        // threshold) or misses. Both paths honour the preference; they are
        // not "directory disabled, linear scan does NUMA" as this comment
        // used to claim — they are two independently NUMA-aware
        // implementations of the same preference, used depending on whether
        // the directory sidecar is materialised.
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
            // R12-1 (task #252): do NOT hold a live `&'static SegmentDirectory`
            // across this loop. `validate_directory_candidate` below can call
            // `publish_empty` / `sync_directory_for_segment_classes`, which
            // materialise a `&'static mut SegmentDirectory` on the SAME
            // allocation (via `deref_directory_sidecar_mut`) to self-heal a
            // stale bit. Holding a live shared reference across that call is
            // aliasing UB under Stacked/Tree Borrows (`&T` and `&mut T` both
            // live over one allocation), independent of the single-threaded
            // owner discipline that makes it data-race-free. Each word-array
            // is instead read BY VALUE via `os::read_directory_class_words`
            // (a raw-pointer `.read()`, no reference retained) immediately
            // before it is scanned, so no directory reference is ever live
            // across `validate_directory_candidate`. Every bit read this way
            // is already just a CANDIDATE — re-validated (base non-null,
            // kind, BinTable head) below — so reading a possibly-one-word-
            // stale snapshot changes nothing observable.

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
            #[cfg(feature = "numa-aware")]
            let (buckets, n_buckets): (
                [usize; crate::alloc_core::segment_directory::NODE_BITMAPS],
                usize,
            ) = {
                let mut buckets = [0usize; crate::alloc_core::segment_directory::NODE_BITMAPS];
                let mut n = 0usize;
                // R12-2: read-by-value (no live `&SegmentDirectory` retained
                // — see `os::read_directory_node_bucket`'s doc comment,
                // mirroring the R12-1 discipline `read_directory_class_words`
                // established for the word-array reads below).
                // SAFETY (R17-2, task #319): `self.directory_sidecar` is
                // non-null (guarded by the `!self.directory_sidecar.is_null()`
                // check above) and was produced by
                // `reserve_directory_sidecar`, which fully initialises it.
                // `AllocCore`'s owner-only discipline (neither `Send` nor
                // `Sync`) rules out a concurrent writer; the read is by-value,
                // so no reference to the sidecar escapes.
                let my_bucket =
                    unsafe { os::read_directory_node_bucket(self.directory_sidecar, my_node) };
                buckets[n] = my_bucket;
                n += 1;
                // Unknown bucket — local-equivalent (R10-6 §3.2 "treated as
                // acceptable/local-equivalent, matching today's scan"). Scanned
                // BEFORE foreign so a NO_NODE segment is never deprioritised
                // below a foreign one.
                let unknown = crate::alloc_core::segment_directory::MAX_NODES;
                if unknown != my_bucket {
                    buckets[n] = unknown;
                    n += 1;
                }
                // Foreign real-node buckets, ascending.
                for nb in 0..crate::alloc_core::segment_directory::MAX_NODES {
                    if nb != my_bucket {
                        buckets[n] = nb;
                        n += 1;
                    }
                }
                (buckets, n)
            };
            // Single bucket [0] (pre-R11-6 layout) — no mutation needed, so
            // `buckets`/`n_buckets` are bound directly in their final state
            // (avoids an unused `mut` / dead initial-write warning under
            // `not(numa-aware))`, which is the production default).
            #[cfg(not(feature = "numa-aware"))]
            let (buckets, n_buckets): (
                [usize; crate::alloc_core::segment_directory::NODE_BITMAPS],
                usize,
            ) = ([0; crate::alloc_core::segment_directory::NODE_BITMAPS], 1);

            for &nb in buckets.iter().take(n_buckets) {
                // R12-1: read this bucket/class's word-array BY VALUE — no
                // `&SegmentDirectory` reference is retained across the
                // `validate_directory_candidate` calls below (see the doc
                // comment above and on `os::read_directory_class_words`).
                // SAFETY (R17-2, task #319): `self.directory_sidecar` is
                // non-null (guarded by the
                // `!self.directory_sidecar.is_null()` check above) and was
                // produced by `reserve_directory_sidecar`, which fully
                // initialises it. `AllocCore`'s owner-only discipline (neither
                // `Send` nor `Sync`) rules out a concurrent writer; the read is
                // by-value, so no reference to the sidecar escapes.
                let words = unsafe {
                    os::read_directory_class_words(self.directory_sidecar, nb, class_idx)
                };

                for (w, &word_val) in words.iter().enumerate() {
                    let mut bits = word_val;
                    if bits == 0 {
                        continue;
                    }
                    // R7-A0: count each word examined by the directory scan.
                    #[cfg(feature = "alloc-stats")]
                    crate::alloc_core::directory_stats::DIRECTORY_WORDS_EXAMINED
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
                        < crate::alloc_core::segment_directory::DIRECTORY_MISS_FULL_SCAN_PERIOD
                    {
                        // R8-2: trust the directory. Skip the O(S) scan entirely.
                        #[cfg(feature = "alloc-stats")]
                        crate::alloc_core::directory_stats::DIRECTORY_AUTHORITATIVE_MISS
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
                crate::alloc_core::directory_stats::DIRECTORY_FALLBACK_SCANS
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
            crate::alloc_core::directory_stats::FULL_SCAN_SLOTS_EXAMINED
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
            // inspecting its BinTable — see
            // [`drain_segment_ring`](Self::drain_segment_ring)'s doc for the
            // guard/drain/sync/decommit mechanism this delegates to.
            // Cross-thread frees that targeted THIS segment (a segment we
            // own but are not currently allocating from) are sitting in its
            // ring; without this drain they would never reach the BinTable
            // and the scan would miss them.
            #[cfg(feature = "alloc-xthread")]
            match self.drain_segment_ring(
                base,
                #[cfg(feature = "fastbin")]
                is_in_magazine,
            ) {
                // Decommitted: `base` is unmapped or pooled — skip the
                // BinTable check for it in THIS scan (see the doc comment on
                // `drain_segment_ring` for the released-vs-pooled rationale).
                #[cfg(feature = "alloc-decommit")]
                RingDrainOutcome::Decommitted => continue,
                RingDrainOutcome::Skipped | RingDrainOutcome::Drained { .. } => {}
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
                    if seg_node != my_node
                        && seg_node != crate::alloc_core::segment_header::NO_NODE_RAW
                    {
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
                            crate::alloc_core::directory_stats::DIRECTORY_MISS_SELF_HEAL
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
                            crate::alloc_core::directory_stats::DIRECTORY_MISS_SELF_HEAL
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
                        crate::alloc_core::directory_stats::DIRECTORY_MISS_SELF_HEAL
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
            crate::alloc_core::directory_stats::DIRECTORY_STALE_HITS
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
            crate::alloc_core::directory_stats::DIRECTORY_STALE_HITS
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }

        // P1-a: lazily drain this segment's remote-free ring BEFORE
        // inspecting BinTable — exactly as the linear scan does, via the
        // shared [`drain_segment_ring`](Self::drain_segment_ring). Cross-thread
        // frees sitting in the ring are invisible to the BinTable until
        // drained.
        #[cfg(feature = "alloc-xthread")]
        match self.drain_segment_ring(
            base,
            #[cfg(feature = "fastbin")]
            is_in_magazine,
        ) {
            // P1-b: decommit/pool hysteresis — a decommitted segment must be
            // skipped; try the next candidate.
            #[cfg(feature = "alloc-decommit")]
            RingDrainOutcome::Decommitted => return None,
            RingDrainOutcome::Skipped | RingDrainOutcome::Drained { .. } => {}
        }

        // Validation step 3: BinTable head STILL non-null?
        let meta = SegmentMeta::new(base);
        let bt = meta.bin_table();
        if bt.head(class_idx) == FREE_LIST_NULL {
            // Stale positive: drain may have emptied the class, or a concurrent
            // transition cleared it. Clear the bit and try the next candidate.
            self.publish_empty(base, class_idx, slot_idx);
            #[cfg(feature = "alloc-stats")]
            crate::alloc_core::directory_stats::DIRECTORY_STALE_HITS
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return None;
        }

        // Valid hit — do the SAME work as the linear scan on hit.
        // P1-c: un-pool if this segment was retained in the pool.
        #[cfg(feature = "alloc-decommit")]
        self.unpool_if_present(base);

        #[cfg(feature = "alloc-stats")]
        crate::alloc_core::directory_stats::DIRECTORY_HITS
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);

        Some(base)
    }
}
