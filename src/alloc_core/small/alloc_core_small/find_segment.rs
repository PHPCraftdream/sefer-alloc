//! Directory-accelerated small-segment lookup for the small path —
//! `carve_block_with_refill`, the `find_segment_with_free*` scan family,
//! and `validate_directory_candidate` (mechanical split of the former flat
//! `alloc_core_small.rs`; pure code movement, no behavior changed).

use crate::alloc_core::os;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL};

use crate::alloc_core::alloc_core::AllocCore;

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
use super::sidecar_drain_outcome::SidecarDrainOutcome;

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
    /// Each terminal route cut ends before this reservation can be finalized.
    /// Logical retirement checks physical free/magazine state inside the owner
    /// primitive; callers do not supply a second residency oracle.
    pub(crate) fn find_segment_with_free(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free_impl(
            class_idx,
            #[cfg(feature = "alloc-segment-directory")]
            false,
        )
    }

    /// Test-only entry to the real canonical-root discovery path.
    #[cfg(all(feature = "alloc-xthread", feature = "bench-internals"))]
    #[cfg(feature = "internals")]
    #[doc(hidden)]
    pub fn dbg_find_segment_with_free_for_test(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free(class_idx)
    }

    /// R9-8 (task #230): forced "rescue scan" — runs the full O(S) linear
    /// scan BYPASSING the R8-2 directory-trust fast path, so a stale-negative
    /// directory bit cannot hide a real free block. Self-heals any bit the
    /// scan finds the directory had wrongly cleared (same mechanism as the
    /// periodic re-validation path). Called as a last resort from the
    /// small-allocation OOM path (where `reserve_small_segment` returned
    /// `None`) to avoid a spurious OOM a directory bug could otherwise cause.
    /// Available under the directory feature without NUMA. The explicit rescue
    /// mode bypasses standalone negative trust and repairs missing directory bits.
    #[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]
    pub(crate) fn find_segment_with_free_forced(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free_impl(class_idx, true)
    }

    /// Consume one canonical Small root, dropping every cut and route borrow
    /// before reclaim finalization can release its reservation. Called only
    /// on actual free-list/refill misses, never on a magazine hit.
    #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
    #[inline]
    pub(super) fn drain_segment_sidecar(&mut self, base: *mut u8) -> SidecarDrainOutcome {
        if !self.table.is_routed() {
            return SidecarDrainOutcome::Skipped;
        }
        let index = SegmentHeader::segment_id_at(base) as usize;
        let high_water = SegmentMeta::new(base).bump_of();
        let mut changed_classes = 0u64;
        {
            let Some(mut scan) = self.table.scan_small_route(index, base, high_water) else {
                std::process::abort();
            };
            while let Some(mut cut) = scan.next_cut() {
                while let Some(record) = cut.pop() {
                    if Self::reclaim_sidecar_record(base, record.offset, record.class) {
                        changed_classes |= 1u64 << record.class;
                    }
                }
            }
        }
        if changed_classes == 0 {
            return SidecarDrainOutcome::Skipped;
        }
        #[cfg(feature = "alloc-segment-directory")]
        self.sync_directory_for_segment_classes(base, index, changed_classes);
        #[cfg(feature = "alloc-decommit")]
        if Self::dec_live_and_maybe_decommit(base, self.small_cur) {
            let pooled = self.release_or_pool_empty_segment(base);
            return SidecarDrainOutcome::Decommitted { pooled };
        }
        SidecarDrainOutcome::Drained
    }

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
    fn find_segment_with_free_impl(
        &mut self,
        class_idx: usize,
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
                        if let Some(base) = self.validate_directory_candidate(class_idx, slot_idx) {
                            return Some(base);
                        }
                    }
                }
            }
            // Only an instance with terminal publication capability must look
            // past a negative owner directory. Standalone cores retain their
            // authoritative-negative cadence even in a production build.
            #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
            let trust_negative = !self.table.is_routed();
            #[cfg(not(all(feature = "alloc-global", feature = "alloc-xthread")))]
            let trust_negative = true;
            if !trust_negative {
                periodic_revalidation_active = !rescue;
            } else if !rescue {
                self.directory_miss_streak[class_idx] =
                    self.directory_miss_streak[class_idx].saturating_add(1);
                if u32::from(self.directory_miss_streak[class_idx])
                    < crate::alloc_core::segment_directory::DIRECTORY_MISS_FULL_SCAN_PERIOD
                {
                    #[cfg(feature = "alloc-stats")]
                    crate::alloc_core::directory_stats::DIRECTORY_AUTHORITATIVE_MISS
                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    return None;
                }
                self.directory_miss_streak[class_idx] = 0;
                periodic_revalidation_active = true;
            }
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
            // Consume only this canonical candidate before inspecting its bins.
            #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
            match self.drain_segment_sidecar(base) {
                // Decommitted+released: `base` is unmapped — skip the
                // BinTable check for it in THIS scan. Decommitted+pooled:
                // `base` is still live/registered/committed (R1-03, src
                // review round 1) — fall through to the SAME BinTable check
                // every other live segment gets below, so a segment that just
                // emptied (and therefore has every one of its blocks free)
                // can be reused as a hit in this very scan, not only on a
                // later call (see `RingDrainOutcome::Decommitted`'s doc).
                #[cfg(feature = "alloc-decommit")]
                SidecarDrainOutcome::Decommitted { pooled: false } => continue,
                #[cfg(feature = "alloc-decommit")]
                SidecarDrainOutcome::Decommitted { pooled: true } => {}
                SidecarDrainOutcome::Skipped | SidecarDrainOutcome::Drained => {}
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
                        if fallback.is_none_or(|fb| !self.table.contains_base_ro(fb)) {
                            fallback = Some(base);
                        }
                        continue;
                    }
                    // Local or unknown node — use it immediately.
                    self.finalize_hit(
                        base,
                        class_idx,
                        #[cfg(feature = "alloc-segment-directory")]
                        periodic_revalidation_active,
                        #[cfg(feature = "alloc-segment-directory")]
                        rescue,
                    );
                    return Some(base);
                }
                // Without numa-aware: same as before — return the first match.
                #[cfg(not(feature = "numa-aware"))]
                {
                    self.finalize_hit(
                        base,
                        class_idx,
                        #[cfg(feature = "alloc-segment-directory")]
                        periodic_revalidation_active,
                        #[cfg(feature = "alloc-segment-directory")]
                        rescue,
                    );
                    return Some(base);
                }
            }
        }
        // First pass found no local segment with a free block; fall back to
        // the first foreign-node segment we recorded (or None if everything is
        // empty / all recycled).
        #[cfg(feature = "numa-aware")]
        {
            // Later drains may evict an earlier fully empty pooled candidate.
            // Revalidate without touching its reservation before finalizing it.
            let fallback = fallback.filter(|&fb| self.table.contains_base_ro(fb));
            if let Some(fb) = fallback {
                self.finalize_hit(
                    fb,
                    class_idx,
                    #[cfg(feature = "alloc-segment-directory")]
                    periodic_revalidation_active,
                    #[cfg(feature = "alloc-segment-directory")]
                    rescue,
                );
            }
            fallback
        }
        #[cfg(not(feature = "numa-aware"))]
        None
    }

    /// #1994 (alloc_core review): shared "hit finalize" step reused by the
    /// three near-identical directory-self-heal blocks in the linear scan
    /// above (the numa-aware local/unknown-node hit, the plain non-numa hit,
    /// and the numa-aware foreign-node fallback) — un-pool `base` if it was
    /// retained in the pool (Mechanism 2, task #51), then self-heal a stale
    /// directory MISS if this scan reached `base` via the periodic
    /// re-validation or rescue path (R8-2, task #215 / R9-8, task #230).
    #[allow(unused_variables)]
    #[inline]
    fn finalize_hit(
        &mut self,
        base: *mut u8,
        class_idx: usize,
        #[cfg(feature = "alloc-segment-directory")] periodic_revalidation_active: bool,
        #[cfg(feature = "alloc-segment-directory")] rescue: bool,
    ) {
        // Mechanism 2 (task #51): if this segment was RETAINED in the pool
        // (empty, committed), it is now being reused — remove it from the
        // pool so it is not later re-pooled a second time (a double-entry
        // that would double-recycle its base). This is the hysteresis WIN:
        // the emptied segment's free blocks are re-served here with no OS
        // reserve/release round-trip.
        #[cfg(feature = "alloc-decommit")]
        self.unpool_if_present(base);
        // R8-2 (task #215): if this scan reached `base` via the periodic
        // re-validation branch (directory feature ON and streak hit the
        // period) or the R9-8 rescue path, the directory's MISS was wrong —
        // this segment actually has a free block. Self-heal the bit in-place
        // and bump the canary counter.
        #[cfg(feature = "alloc-segment-directory")]
        if periodic_revalidation_active || rescue {
            let slot_idx = SegmentHeader::segment_id_at(base) as usize;
            self.publish_nonempty(base, class_idx, slot_idx);
            // R9-8: only the PERIODIC re-validation path bumps the
            // `DIRECTORY_MISS_SELF_HEAL` canary; the rescue path is counted
            // separately by `DIRECTORY_RESCUE_OOM_AVOIDED` at its caller, so
            // the two drift signals stay distinguishable in diagnostics.
            #[cfg(feature = "alloc-stats")]
            if periodic_revalidation_active {
                crate::alloc_core::directory_stats::DIRECTORY_MISS_SELF_HEAL
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
        }
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
    fn validate_directory_candidate(
        &mut self,
        class_idx: usize,
        slot_idx: usize,
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

        // The route scan and all detached cuts end before this reservation
        // can be pooled/released or its free-list head can be served.
        #[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
        match self.drain_segment_sidecar(base) {
            // P1-b: decommit/pool hysteresis — a RELEASED segment must be
            // skipped (try the next candidate); a POOLED segment (R1-03, src
            // review round 1) is still live/registered/committed, so fall
            // through to the SAME validation step 3 below every other live
            // candidate gets, letting a segment that emptied DURING this
            // drain be recognised as a hit in this very call.
            #[cfg(feature = "alloc-decommit")]
            SidecarDrainOutcome::Decommitted { pooled: false } => return None,
            #[cfg(feature = "alloc-decommit")]
            SidecarDrainOutcome::Decommitted { pooled: true } => {}
            SidecarDrainOutcome::Skipped | SidecarDrainOutcome::Drained => {}
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
