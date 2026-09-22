//! `internals`-gated segment-table/registry introspection of [`AllocCore`] —
//! node-id/page-map probes, table counts, hash-diagnostic delegations, and
//! the teardown test seams (`dbg_unregister`/`dbg_recycle`) (mechanical
//! split of the former flat `alloc_core_core_diag.rs`; pure code movement,
//! no behavior changed).

use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::os;
use crate::alloc_core::segment_header::SegmentHeader;
#[cfg(feature = "page-map-diag")]
use crate::alloc_core::segment_header::SegmentKind;
#[cfg(any(feature = "numa-aware", feature = "page-map-diag"))]
use crate::alloc_core::segment_header::SegmentMeta;

/// Sol-F1 (task #563): `internals`-gated — every other `dbg_*` diagnostic
/// hook in this file. See this file's module doc for the full rationale.
#[cfg(feature = "internals")]
impl AllocCore {
    /// TEST-ONLY (Phase B/C): the NUMA `node_id` stored in `ptr`'s segment
    /// header, or `None` if `ptr` is foreign. Returns `u32::MAX` (`NO_NODE_RAW`)
    /// for a segment that was not bound to a specific NUMA node (e.g. on a
    /// non-NUMA platform, or when `numa-aware` is off). The field is present in
    /// EVERY build's layout (layout-stable across feature configs); this accessor
    /// is only compiled under `numa-aware` because the test that reads it is also
    /// gated on that feature.
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer, not `ptr`'s caller-derived
    /// address — see `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    #[cfg(feature = "numa-aware")]
    pub fn dbg_node_id_for(&self, ptr: *mut u8) -> Option<u32> {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self.table.canonical_base_of(candidate)?;
        Some(SegmentMeta::new(base).node_id_of())
    }

    /// TEST-ONLY (Phase 13.3): reveal the size class `page_map` would assign
    /// to `ptr`'s page, so the counterfactual test for "own-thread dealloc
    /// derives the class from `Layout`, not `page_map`" can prove it is
    /// non-vacuous. Returns `None` if `ptr` is foreign, the segment is not
    /// small/primordial, or the page is uncarved. This is the (now-removed)
    /// `page_map`-class derivation the old intrusive-TFS drain used — kept here
    /// as a pure read so the test can prove the Layout-class and page_map-class
    /// genuinely differ on a mixed-class page (the §13 counterfactual).
    /// `#[doc(hidden)] pub` per the established test-only surface.
    ///
    /// R12-11 (task #262): gated behind `page-map-diag` — this is the ONLY
    /// reader of `PageMap::class_of` in the whole codebase, and `PageMap` is
    /// only maintained (written) under that same feature. See
    /// `PageMap`'s struct doc / the feature's `Cargo.toml` doc.
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer, not `ptr`'s caller-derived
    /// address — see `SegmentTable::canonical_base_of`'s doc. The
    /// `page_idx` computation below still legitimately uses the caller's
    /// `ptr` (only for arithmetic against the two addresses — not a
    /// dereference), matching `base`'s address since both denote the same
    /// segment.
    #[doc(hidden)]
    #[cfg(feature = "page-map-diag")]
    pub fn dbg_page_map_class_for(&self, ptr: *mut u8) -> Option<usize> {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self.table.canonical_base_of(candidate)?;
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        let meta = SegmentMeta::new(base);
        let page_idx = (ptr as usize - base as usize) / crate::alloc_core::os::PAGE;
        meta.page_map().class_of(page_idx)
    }

    /// TEST-ONLY (Phase 13.3): the size class the own-thread `dealloc` SHOULD
    /// derive from `layout` (i.e. what `Self::classify` resolves to). Returns
    /// `None` for a Large layout. Exposed so the counterfactual test can
    /// compare the Layout-derived class against the `page_map`-derived class
    /// on a mixed-class page and prove the two genuinely differ (otherwise
    /// the test would be vacuous).
    /// TEST-ONLY (task #135): the segment table's high-water slot count (see
    /// `SegmentTable::count`). Used by `tests/segment_table_o1.rs` to verify
    /// the O(1) free-list actually recycles vacated indices instead of
    /// letting the high-water mark grow unbounded.
    #[doc(hidden)]
    pub fn dbg_table_count(&self) -> u32 {
        self.table.count()
    }

    /// TEST-ONLY (task #135): public wrapper over `AllocCore::contains_base`
    /// for integration tests (which cannot see the `pub(crate)` version, nor
    /// the `pub(crate)` `os::segment_base_of_ptr` needed to derive a segment
    /// base from an arbitrary in-segment pointer). Takes any pointer
    /// previously returned by `alloc`/`alloc_large` (not necessarily the
    /// segment base itself) and derives the base internally, matching the
    /// convention of the other `dbg_*_for` accessors in this file.
    #[doc(hidden)]
    pub fn dbg_contains_base(&self, ptr: *mut u8) -> bool {
        self.table.contains_base_ro(os::segment_base_of_ptr(ptr))
    }

    /// R23-6 (task #375) DIAGNOSTIC ONLY: process-wide high-water mark of
    /// hash-slot probe steps any SINGLE `SegmentTable::hash_remove` call has
    /// taken since the last [`AllocCore::dbg_reset_hash_remove_max_scan_steps`]
    /// — see
    /// [`segment_table::HASH_REMOVE_MAX_SCAN_STEPS`](crate::alloc_core::segment_table::HASH_REMOVE_MAX_SCAN_STEPS)
    /// for the full rationale. Deterministic replacement for the wall-clock
    /// "no single dealloc is a dramatic outlier" check in
    /// `tests/regression_segment_table_tombstone_rebuild.rs`. Reads 0 unless
    /// `alloc-stats` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_hash_remove_max_scan_steps() -> u64 {
        crate::alloc_core::segment_table::HASH_REMOVE_MAX_SCAN_STEPS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R23-6 (task #375): reset the high-water mark read by
    /// [`AllocCore::dbg_hash_remove_max_scan_steps`] to 0. Test hook only —
    /// lets a measurement window (e.g. one wave of a threshold-boundary test)
    /// start clean instead of accumulating across the whole process.
    #[doc(hidden)]
    pub fn dbg_reset_hash_remove_max_scan_steps() {
        crate::alloc_core::segment_table::reset_hash_remove_max_scan_steps();
    }

    /// MEASUREMENT-ONLY (R23-3, task #372): thin delegation to
    /// `SegmentTable::dbg_hash_contains_only` — see that method's doc comment
    /// for why this exists (isolating Tier-2's cost deterministically,
    /// since the Tier-1 direct-mapped cache's hit/miss behaviour depends on
    /// OS-assigned segment addresses, not on anything a benchmark controls).
    /// Takes an already-computed segment BASE (unlike `dbg_contains_base`,
    /// which derives it from an arbitrary in-segment pointer) so
    /// `benches/perf_gate_iai.rs` can pair it with the SAME
    /// `dbg_segment_base_of_ptr`-derived base its other probe arms already
    /// use, keeping every probe arm's shape identical apart from which
    /// tier of `contains_base` each one measures.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_hash_contains_only(&self, base: *mut u8) -> bool {
        self.table.dbg_hash_contains_only(base)
    }

    /// MEASUREMENT-ONLY (R32-10, task #501, F2): process-wide count of
    /// `SegmentTable::contains_base` calls that HIT the Tier-1 direct-mapped
    /// `own_cache` — see
    /// [`CONTAINS_BASE_TIER1_HITS`](crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_HITS)'s
    /// own doc for the full rationale (the missing path-activation oracle
    /// `docs/perf/OPEN_ITEMS.md` item 1's own text left as an open clause).
    /// Reads 0 unless `bench-internals` is on.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    #[must_use]
    pub fn dbg_contains_base_tier1_hits() -> u64 {
        crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_HITS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// MEASUREMENT-ONLY (R32-10, task #501, F2): the Tier-2-fallback
    /// complement of [`AllocCore::dbg_contains_base_tier1_hits`] — see
    /// [`CONTAINS_BASE_TIER1_MISSES`](crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_MISSES)'s
    /// own doc. Reads 0 unless `bench-internals` is on.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    #[must_use]
    pub fn dbg_contains_base_tier1_misses() -> u64 {
        crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_MISSES
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// MEASUREMENT-ONLY (R32-10, task #501, F2): reset both
    /// [`CONTAINS_BASE_TIER1_HITS`](crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_HITS)
    /// and
    /// [`CONTAINS_BASE_TIER1_MISSES`](crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_MISSES)
    /// to 0. Test/bench hook only — lets a measurement window start from a
    /// clean count instead of accumulating across the whole process
    /// lifetime, mirroring [`AllocCore::dbg_reset_hash_remove_max_scan_steps`]'s
    /// established reset-hook convention.
    #[doc(hidden)]
    #[cfg(feature = "bench-internals")]
    pub fn dbg_reset_contains_base_tier1_counters() {
        crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_HITS
            .store(0, core::sync::atomic::Ordering::Relaxed);
        crate::alloc_core::alloc_core::counters::CONTAINS_BASE_TIER1_MISSES
            .store(0, core::sync::atomic::Ordering::Relaxed);
    }

    /// TEST-ONLY (task #135): read the stamped `segment_id` field of `ptr`'s
    /// segment (field-specific read, mirrors what
    /// `SegmentTable::unregister`/`recycle` now use internally for their O(1)
    /// slot lookup).
    ///
    /// R2-05 (independent src review round 2, task #2007): the guard below
    /// now returns (not just checks) the table's own STORED (canonical)
    /// pointer, and the header read goes through THAT pointer, not `ptr`'s
    /// caller-derived address — see `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    pub fn dbg_segment_id_of(&self, ptr: *mut u8) -> u32 {
        let candidate = os::segment_base_of_ptr(ptr);
        // R2-3: release-surviving membership guard (replaces a debug-only
        // debug_assert! that compiled out in release, leaving the raw header
        // read unguarded). This module is #![forbid(unsafe_code)], so the
        // heap_registry-style `unsafe fn` discipline does not apply — a real
        // runtime guard is the soundness fix here.
        let base = self
            .table
            .canonical_base_of(candidate)
            .expect("dbg_segment_id_of: ptr's segment is not owned by this AllocCore");
        SegmentHeader::segment_id_at(base)
    }

    /// TEST-ONLY (task #135): overwrite the stamped `segment_id` field of
    /// `ptr`'s segment (field-specific write). Used to construct the
    /// corrupted-id scenario exercised by
    /// `unregister_defends_against_mismatched_segment_id`.
    ///
    /// # Safety
    ///
    /// The stamped `segment_id` is load-bearing allocator metadata:
    /// `SegmentTable`'s O(1) slot lookup indexes `slots[segment_id]` with it
    /// (`unregister` / `recycle` / the hash/own-cache probes behind
    /// `contains_base`), so an `id` inconsistent with the segment's true slot
    /// can make a later lookup land on the WRONG slot — corrupting the O(1)
    /// lookup for BOTH the stamped segment and the segment whose id was
    /// borrowed. This is the same "writes raw / load-bearing metadata" class
    /// that made [`dbg_unregister`](Self::dbg_unregister) /
    /// [`dbg_recycle`](Self::dbg_recycle) (task #101 / R4-MS-3) and
    /// [`dbg_push_to_ring`](Self::dbg_push_to_ring) (R6-MS-4) `unsafe fn` in
    /// this file: `#[doc(hidden)]` only hides from generated docs, it does NOT
    /// restrict Rust reachability, so a fully-safe call could overwrite the
    /// field with an arbitrary value (round5 `code_quality_review` R6-CQ-2,
    /// CRITICAL). The `contains_base_ro` assert below only proves the segment
    /// BELONGS to this `AllocCore`; it does NOT preserve the field's invariant.
    ///
    /// The caller must guarantee that, between this stamp and the field being
    /// restored to the segment's true `segment_id`, NO safe
    /// `alloc` / `dealloc` / `realloc` / `Drop` call routes the segment on the
    /// stamped value — i.e. one of:
    ///
    /// - the stamped `id` is restored to the segment's true value (captured
    ///   beforehand via [`dbg_segment_id_of`](Self::dbg_segment_id_of)) before
    ///   any further allocator operation touches the segment; OR
    /// - the segment is consumed ONLY by a `#[doc(hidden)]` test-only teardown
    ///   seam ([`dbg_unregister`](Self::dbg_unregister) /
    ///   [`dbg_recycle`](Self::dbg_recycle)) whose `slots[id] == base`
    ///   defensive guard rejects the corrupted id as a no-op / defensive tail
    ///   and does NOT route on the stamped field being correct.
    #[doc(hidden)]
    #[allow(unsafe_code)] // R6-CQ-2: `unsafe fn` boundary (raw metadata write).
    pub unsafe fn dbg_stamp_segment_id(&self, ptr: *mut u8, id: u32) {
        let base = os::segment_base_of_ptr(ptr);
        assert!(
            self.table.contains_base_ro(base),
            "dbg_stamp_segment_id: ptr's segment is not owned by this AllocCore"
        );
        SegmentHeader::set_segment_id_at(base, id);
    }

    /// TEST-ONLY (task #135): directly invoke `SegmentTable::unregister` for
    /// `ptr`'s segment, for a public integration test (which cannot call the
    /// `pub(crate)` version). Exercises the O(1) `segment_id`-indexed lookup
    /// and its defensive `slots[id] == base` guard in isolation from any
    /// surrounding dealloc bookkeeping (the caller is responsible for
    /// whatever cleanup the test scenario needs afterwards).
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live allocation pointer whose segment is owned by
    /// this `AllocCore`. The callee computes `base` from `ptr` and mutates the
    /// segment table WITHOUT a membership check; an invalid, stale or foreign
    /// `ptr` may corrupt the segment table or trigger undefined behaviour.
    #[doc(hidden)]
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_unregister(&mut self, ptr: *mut u8) {
        self.table.unregister(os::segment_base_of_ptr(ptr));
    }

    /// TEST-ONLY (L-3, UBFIX-11): directly invoke `SegmentTable::recycle` for
    /// `ptr`'s segment, for a public integration test (which cannot call the
    /// `pub(crate)` version). Exercises the O(1) `segment_id`-indexed slot
    /// lookup AND its defensive mismatch tail (`slots[id] != base` /
    /// `id >= count`) in isolation — mirrors `dbg_unregister`'s role for
    /// `SegmentTable::unregister`. The caller is responsible for constructing
    /// whatever corrupted-`segment_id` scenario the test needs beforehand
    /// (e.g. via `dbg_stamp_segment_id`) and for any cleanup afterwards.
    ///
    /// # Safety contract mirrors `SegmentTable::recycle`'s caller contract
    ///
    /// After this call returns, `ptr`'s segment's OS reservation has been
    /// released (defensive tail) or released-and-slot-NULLed (main path) —
    /// either way the caller MUST NOT dereference `ptr`/`base` afterwards.
    ///
    /// `ptr` MUST be a valid, live allocation pointer whose segment is owned by
    /// this `AllocCore`. The callee computes `base` from `ptr` and releases the
    /// OS reservation WITHOUT a membership check; an invalid, stale or foreign
    /// `ptr` may corrupt the segment table or release the wrong reservation.
    #[doc(hidden)]
    #[cfg(feature = "alloc-decommit")]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_recycle(&mut self, ptr: *mut u8) {
        let base = os::segment_base_of_ptr(ptr);
        // R7-A2: clear directory bits before the slot is recycled.
        #[cfg(feature = "alloc-segment-directory")]
        {
            let slot_idx = SegmentHeader::segment_id_at(base) as usize;
            self.clear_segment_directory(slot_idx);
        }
        self.table.recycle(base);
    }

    /// R12-14 (task #265) TEST-ONLY: the process-wide cap on simultaneously
    /// live segments (`segment_table::MAX_SEGMENTS`). Lets a multi-workload
    /// test (e.g. a fixed allocation count fanned out across several NUMA
    /// nodes) size itself so the TOTAL segment demand it generates stays
    /// within capacity, instead of hardcoding a per-node allocation count
    /// that was only ever measured safe under one feature combination — a
    /// class whose per-segment block density varies sharply across the
    /// feature matrix (e.g. `SegmentLayout::SMALL_MAX`, which is one block
    /// per segment under `medium-classes-wide` vs. ~16 under `production`)
    /// can silently overflow this cap when the same literal count is reused
    /// under a denser feature combination it was never tuned for.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_max_segments() -> usize {
        crate::alloc_core::segment_table::MAX_SEGMENTS
    }
}
