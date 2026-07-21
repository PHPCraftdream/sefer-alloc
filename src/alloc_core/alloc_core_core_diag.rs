//! Core/general diagnostics for [`AllocCore`] (mechanical split of
//! `alloc_core.rs`, task R6-CQ-7a).
//!
//! This file holds the `impl AllocCore { .. }` block for the `dbg_*`
//! diagnostic/test-only hooks that are NOT specific to the small-allocator
//! subsystem (see `alloc_core_small_diag.rs` for that cluster): segment
//! reservation/release counters, NUMA node lookup, page-map/layout-class
//! introspection, segment-id/kind-byte read+corrupt hooks, and the
//! table/registry teardown test seams (`dbg_unregister`/`dbg_recycle`).
//! Pure code-movement sibling of `alloc_core.rs`; no behavior changed.

use core::alloc::Layout;

use super::node::Node;
use super::os;
use super::segment_header::{SegmentHeader, SegmentKind, SegmentMeta};
use super::size_classes::{AllocKind, SizeClasses};

use super::alloc_core::{AllocCore, FOREIGN_OR_UNROUTABLE_FREES};
use super::directory_stats;

impl AllocCore {
    /// DIAGNOSTIC (review finding 2.3): process-wide count of `dealloc` calls
    /// that hit the foreign-or-unroutable no-op branch (a `ptr` not in any of
    /// this heap's registered segments — silently dropped). Backs
    /// [`AllocStats::foreign_or_unroutable_frees`](crate::AllocStats::foreign_or_unroutable_frees).
    /// See [`FOREIGN_OR_UNROUTABLE_FREES`] for the full rationale (the
    /// `alloc-global`-without-`alloc-xthread` cross-thread-free leak footgun).
    /// A plain relaxed atomic load — diagnostic only, no ordering obligation.
    /// Reads `0` unless the per-event increment was compiled in (`alloc-stats`).
    #[doc(hidden)]
    #[cfg(feature = "alloc-core")]
    pub fn dbg_foreign_or_unroutable_frees() -> u64 {
        FOREIGN_OR_UNROUTABLE_FREES.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// reservations since process start (every `os::Segment::reserve`
    /// success plus NUMA-pinned reservations). Monotonic, relaxed — pairs
    /// with [`AllocCore::dbg_segments_released_total`]; the difference is
    /// the current process-wide live segment count. Always compiled (not
    /// feature-gated) — every build reserves segments via `os::Segment::reserve`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_reserved_total() -> u64 {
        super::os::segments_reserved_total()
    }

    /// DIAGNOSTIC (task E1): process-wide count of successful OS segment
    /// releases since process start. Monotonic, relaxed. See
    /// [`AllocCore::dbg_segments_reserved_total`].
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_segments_released_total() -> u64 {
        super::os::segments_released_total()
    }

    /// TEST-ONLY (Phase B/C): the NUMA `node_id` stored in `ptr`'s segment
    /// header, or `None` if `ptr` is foreign. Returns `u32::MAX` (`NO_NODE_RAW`)
    /// for a segment that was not bound to a specific NUMA node (e.g. on a
    /// non-NUMA platform, or when `numa-aware` is off). The field is present in
    /// EVERY build's layout (layout-stable across feature configs); this accessor
    /// is only compiled under `numa-aware` because the test that reads it is also
    /// gated on that feature.
    #[doc(hidden)]
    #[cfg(feature = "numa-aware")]
    pub fn dbg_node_id_for(&self, ptr: *mut u8) -> Option<u32> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
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
    #[doc(hidden)]
    pub fn dbg_page_map_class_for(&self, ptr: *mut u8) -> Option<usize> {
        let base = os::segment_base_of_ptr(ptr);
        if !self.table.contains_base_ro(base) {
            return None;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        let meta = SegmentMeta::new(base);
        let page_idx = (ptr as usize - base as usize) / super::os::PAGE;
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

    /// TEST-ONLY (task #135): read the stamped `segment_id` field of `ptr`'s
    /// segment (field-specific read, mirrors what
    /// `SegmentTable::unregister`/`recycle` now use internally for their O(1)
    /// slot lookup).
    #[doc(hidden)]
    pub fn dbg_segment_id_of(&self, ptr: *mut u8) -> u32 {
        let base = os::segment_base_of_ptr(ptr);
        // R2-3: release-surviving membership guard (replaces a debug-only
        // debug_assert! that compiled out in release, leaving the raw header
        // read unguarded). This module is #![forbid(unsafe_code)], so the
        // heap_registry-style `unsafe fn` discipline does not apply — a real
        // runtime guard is the soundness fix here.
        assert!(
            self.table.contains_base_ro(base),
            "dbg_segment_id_of: ptr's segment is not owned by this AllocCore"
        );
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

    /// TEST-ONLY (L-5, UBFIX-11): read the RAW `kind` discriminant byte of
    /// `ptr`'s segment header (not decoded through `SegmentHeader::kind_at` —
    /// the exact byte at the `kind` field's offset). Lets a test capture the
    /// legitimate byte before corrupting it, and confirm the corruption
    /// actually landed.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_kind_byte_of(&self, ptr: *mut u8) -> u8 {
        let base = os::segment_base_of_ptr(ptr);
        assert!(
            self.table.contains_base_ro(base),
            "dbg_kind_byte_of: ptr's segment is not owned by this AllocCore"
        );
        let off = core::mem::offset_of!(SegmentHeader, kind);
        Node::read_u8(Node::offset(base, off) as *const u8)
    }

    /// TEST-ONLY (L-5, UBFIX-11): overwrite the RAW `kind` discriminant byte
    /// of `ptr`'s segment header with an arbitrary value — including bytes
    /// that are NOT one of the three legitimate `SegmentKind` discriminants
    /// (0/1/2), simulating a corrupted/garbled header byte (a wild write from
    /// an unrelated bug, or the aftermath of an H-1-class defect before its
    /// fix). Used to construct the corrupted-kind scenario exercised by
    /// `kind_at_rejects_corrupt_discriminant` — proves `SegmentHeader::
    /// kind_at`'s strict decode maps any byte outside {0,1,2} to
    /// `SegmentKind::Unknown` rather than silently defaulting to `Small`.
    ///
    /// Mirrors `dbg_stamp_segment_id`'s established test-only field-corruption
    /// pattern (`offset_of!` + `Node::write_*`), applied to the `kind` byte
    /// instead of `segment_id`.
    ///
    /// # Safety
    ///
    /// The stamped `kind` discriminant byte is load-bearing allocator metadata:
    /// `dealloc` / `realloc` / `Drop` decode it via `SegmentHeader::kind_at` and
    /// route the segment down the matching `Small` / `Large` / `Primordial`
    /// path. A `raw` value inconsistent with the segment's true kind mis-routes
    /// the segment — e.g. a `Large` segment whose `kind` byte is stamped to the
    /// `Small` discriminant gets freed down the `Small` path, writing a
    /// `BinTable` / free-list header into the live `Large` payload. (Any byte
    /// outside {0,1,2} decodes to `Unknown`, whose `dealloc` arm is a documented
    /// no-op — see [`dealloc`](AllocCore::dealloc) — so stamping such a byte and
    /// then `dealloc`-ing exercises that no-op path, not a mis-route.) This is
    /// the same "writes raw / load-bearing metadata" class that made
    /// [`dbg_unregister`](Self::dbg_unregister) /
    /// [`dbg_recycle`](Self::dbg_recycle) (task #101 / R4-MS-3) and
    /// [`dbg_push_to_ring`](Self::dbg_push_to_ring) (R6-MS-4) `unsafe fn` in
    /// this file: `#[doc(hidden)]` only hides from generated docs, it does NOT
    /// restrict Rust reachability, so a fully-safe call could overwrite the byte
    /// with an arbitrary value (round5 `code_quality_review` R6-CQ-2,
    /// CRITICAL). The `contains_base_ro` assert below only proves the segment
    /// BELONGS to this `AllocCore`; it does NOT preserve the byte's invariant.
    ///
    /// The caller must guarantee that, between this stamp and the byte being
    /// restored to the segment's true `kind` discriminant, NO safe
    /// `alloc` / `dealloc` / `realloc` / `Drop` call routes the segment on the
    /// stamped value — i.e. one of:
    ///
    /// - the stamped byte is restored to the segment's true discriminant
    ///   (captured beforehand via [`dbg_kind_byte_of`](Self::dbg_kind_byte_of))
    ///   before any routing allocator operation touches the segment (read-only
    ///   `dbg_*` accessors that do not route, such as
    ///   [`dbg_kind_byte_of`](Self::dbg_kind_byte_of) /
    ///   [`dbg_kind_at_tag`](Self::dbg_kind_at_tag), may run while the byte is
    ///   corrupted); OR
    /// - the only routing operation run while the byte is corrupted is one the
    ///   allocator performs as a documented no-op regardless of the value, such
    ///   as [`dealloc`](AllocCore::dealloc)'s `SegmentKind::Unknown => {}` arm;
    ///   OR
    /// - the segment is consumed ONLY by a `#[doc(hidden)]` test-only teardown
    ///   seam ([`dbg_unregister`](Self::dbg_unregister) /
    ///   [`dbg_recycle`](Self::dbg_recycle)) that does NOT route on the stamped
    ///   byte being correct.
    #[doc(hidden)]
    #[allow(unsafe_code)] // R6-CQ-2: `unsafe fn` boundary (raw metadata write).
    pub unsafe fn dbg_stamp_kind_byte(&self, ptr: *mut u8, raw: u8) {
        let base = os::segment_base_of_ptr(ptr);
        assert!(
            self.table.contains_base_ro(base),
            "dbg_stamp_kind_byte: ptr's segment is not owned by this AllocCore"
        );
        let off = core::mem::offset_of!(SegmentHeader, kind);
        Node::write_u8(Node::offset(base, off), raw);
    }

    /// TEST-ONLY (L-5, UBFIX-11): the DECODED `SegmentKind` of `ptr`'s
    /// segment, as `SegmentHeader::kind_at` (the strict decode this task
    /// hardened) resolves it — returned as a small tag so `tests/` (which
    /// cannot see the `pub(crate)` `SegmentKind` enum) can assert on it:
    /// `0` = `Primordial`, `1` = `Small`, `2` = `Large`, `3` = `Unknown` (the
    /// L-5 reject sentinel for any byte outside {0,1,2}). Distinct from
    /// [`dbg_kind_byte_of`](Self::dbg_kind_byte_of), which reads the RAW byte
    /// without going through `kind_at`'s decode at all — this accessor is
    /// what actually proves the decode's behaviour.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_kind_at_tag(&self, ptr: *mut u8) -> u8 {
        let base = os::segment_base_of_ptr(ptr);
        assert!(
            self.table.contains_base_ro(base),
            "dbg_kind_at_tag: ptr's segment is not owned by this AllocCore"
        );
        match SegmentHeader::kind_at(base) {
            SegmentKind::Primordial => 0,
            SegmentKind::Small => 1,
            SegmentKind::Large => 2,
            SegmentKind::Unknown => 3,
        }
    }

    /// TEST-ONLY (OPT-G regression): read the `large_size` field from the
    /// header of `ptr`'s segment. Uses a direct field read (same pattern as
    /// `large_size_at` but without the `alloc-xthread` feature gate) so
    /// integration tests can verify the stored value after an in-place realloc.
    #[doc(hidden)]
    pub fn dbg_large_size_of(&self, ptr: *mut u8) -> usize {
        let base = os::segment_base_of_ptr(ptr);
        assert!(
            self.table.contains_base_ro(base),
            "dbg_large_size_of: ptr's segment is not owned by this AllocCore"
        );
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::read_usize(Node::offset(base, off) as *const usize)
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

    /// TEST-ONLY (E2, task W4): the `block_size` of a small class, so the
    /// `refill_n` LUT-vs-formula equivalence test can feed the same input to
    /// both without needing `pub(crate)` access to `SIZE_CLASS_TABLE`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_block_size(class_idx: usize) -> usize {
        SizeClasses::block_size(class_idx)
    }

    /// TEST-ONLY (E2, task W4): number of small size classes.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_small_class_count() -> usize {
        super::size_classes::SMALL_CLASS_COUNT
    }

    #[doc(hidden)]
    pub fn dbg_layout_class_for(&self, layout: Layout) -> Option<usize> {
        let size = layout.size().max(super::size_classes::MIN_BLOCK);
        match Self::classify(size, layout.align()) {
            AllocKind::Small { class_idx } => Some(class_idx),
            AllocKind::Large => None,
        }
    }

    /// TEST-ONLY (R9-1, task #221 follow-up): process-wide count of explicit
    /// `Node::zero` passes on the Large-classified `alloc_zeroed` path (both
    /// the `AllocCore` and `HeapCore` faces bump the same counter). Lets
    /// `tests/alloc_zeroed_fresh_large_skip.rs` assert the fresh-reservation
    /// SKIP actually fires (delta 0 on a fresh alloc under a real OS) and that
    /// the explicit zero actually runs where it must (cache hit; any alloc
    /// under miri). Relaxed load — diagnostic only. Reads 0 unless `alloc-stats`
    /// is on (the increment sites are gated); the accessor is always compiled
    /// so callers need no `#[cfg]`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_large_zero_pass_count() -> u64 {
        super::alloc_core::LARGE_ZERO_PASS_CALLS.load(core::sync::atomic::Ordering::Relaxed)
    }

    // ── R7-A0: directory diagnostic counter accessors ───────────────────────
    //
    // Process-wide counters (Relaxed loads -- diagnostic only, no ordering).
    // Storage is always compiled; per-event increments are `alloc-stats`-gated.
    // Reads 0 when the increment was not compiled in. See
    // `directory_stats.rs` for the counter inventory.

    /// R7-A0: process-wide count of directory lookup hits (A3). Reads 0 until
    /// A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_hits() -> u64 {
        directory_stats::DIRECTORY_HITS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of stale directory hits (A3). Reads 0 until
    /// A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_stale_hits() -> u64 {
        directory_stats::DIRECTORY_STALE_HITS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of directory fallback scans (A3). Reads 0
    /// until A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_fallback_scans() -> u64 {
        directory_stats::DIRECTORY_FALLBACK_SCANS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of directory bitmap words examined (A3).
    /// Reads 0 until A3 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_words_examined() -> u64 {
        directory_stats::DIRECTORY_WORDS_EXAMINED.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of dirty segments drained (A4). Reads 0
    /// until A4 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_dirty_segments_drained() -> u64 {
        directory_stats::DIRTY_SEGMENTS_DRAINED.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R9-6 (class-aware dirty routing judge): process-wide count of
    /// `drain_dirty_segments` visits where the segment's ring, once drained in
    /// response to a `find_segment_with_free_impl(class_idx)` call, produced
    /// ZERO reclaimed blocks of the sought `class_idx` — i.e. wasted work from
    /// THAT caller's perspective that per-(segment,class) dirty routing would
    /// have avoided. The denominator is `dbg_dirty_segments_drained()`. The
    /// ratio wasted/total directly characterises the O(D) vs O(D_class) gap
    /// the review flagged. Diagnostic only; reads 0 unless `alloc-stats` is on.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_wasted_dirty_drains() -> u64 {
        directory_stats::WASTED_DIRTY_DRAINS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R7-A0: process-wide count of slots examined by
    /// `find_segment_with_free_impl` (the linear scan). This is the primary
    /// scan-cost counter -- it is LIVE in A0 (incremented per slot visited
    /// under `alloc-stats`).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_full_scan_slots_examined() -> u64 {
        directory_stats::FULL_SCAN_SLOTS_EXAMINED.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R8-2 (task #215): process-wide count of genuine directory misses where
    /// the directory was TRUSTED authoritative and the O(S) linear-scan
    /// fallback was SKIPPED. Reads 0 until R8-2 wires the increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_authoritative_miss() -> u64 {
        directory_stats::DIRECTORY_AUTHORITATIVE_MISS.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R8-2 (task #215): process-wide count of periodic re-validation full
    /// scans that found a segment the directory had missed and repaired its
    /// bit in-place. Expected to stay 0 in normal operation; a nonzero value
    /// is a canary for a directory-tracking bug. Reads 0 until R8-2 wires the
    /// increment.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_miss_self_heal() -> u64 {
        directory_stats::DIRECTORY_MISS_SELF_HEAL.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// R9-8 (task #230): count of forced OOM-rescue scans that found a real
    /// free block the directory had hidden (and thus avoided a spurious OOM).
    /// Distinguished from `dbg_directory_miss_self_heal` (the periodic
    /// re-validation's routine self-heals). Reads 0 unless `alloc-stats` is on
    /// and `alloc-segment-directory` is active.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_rescue_oom_avoided() -> u64 {
        directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED.load(core::sync::atomic::Ordering::Relaxed)
    }

    // ── R7-A1: directory sidecar introspection ────────────────────────────

    /// R7-A1: whether the per-class segment directory sidecar has been
    /// materialised for this `AllocCore`. `true` iff the sidecar pointer is
    /// non-null (the threshold was crossed and the OS reservation succeeded).
    /// Always returns `false` when `alloc-segment-directory` is OFF.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_is_materialised(&self) -> bool {
        #[cfg(feature = "alloc-segment-directory")]
        {
            !self.directory_sidecar.is_null()
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            false
        }
    }

    /// R7-A1: read a single bit from the materialised directory sidecar.
    /// Returns `None` if the directory is not materialised (below threshold,
    /// feature OFF, or sidecar OOM). Returns `Some(true/false)` otherwise.
    ///
    /// R11-6: under `numa-aware`, this ORs across ALL node buckets (returns
    /// `true` if ANY node has the bit set for this class/slot). For per-node
    /// verification use `dbg_directory_get_bit_for_node`.
    ///
    /// Test-only — lets integration tests verify the rebuilt bitmap matches
    /// the actual `BinTable` state.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_get_bit(&self, class_idx: usize, slot_idx: usize) -> Option<bool> {
        #[cfg(feature = "alloc-segment-directory")]
        {
            self.directory().map(|dir| {
                let word = slot_idx / 64;
                let bit_mask = 1u64 << (slot_idx % 64);
                (0..super::segment_directory::NODE_BITMAPS)
                    .any(|nb| dir.class_nonempty_by_node[nb][class_idx][word] & bit_mask != 0)
            })
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = (class_idx, slot_idx);
            None
        }
    }

    /// R11-6 TEST-ONLY: read a single bit from a SPECIFIC node bucket. Returns
    /// `None` if the directory is not materialised. Used by the NUMA
    /// local-first/foreign-fallback test and the per-node oracle to verify
    /// that bits are placed in the correct node bucket.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-segment-directory", feature = "numa-aware"))]
    #[must_use]
    pub fn dbg_directory_get_bit_for_node(
        &self,
        node_id: u32,
        class_idx: usize,
        slot_idx: usize,
    ) -> Option<bool> {
        self.directory()
            .map(|dir| dir.get_bit(node_id, class_idx, slot_idx))
    }

    /// R11-6 TEST-ONLY: return the number of node buckets in the directory
    /// (`NODE_BITMAPS`). Used by the NUMA oracle to iterate all buckets.
    #[doc(hidden)]
    #[cfg(feature = "alloc-segment-directory")]
    #[must_use]
    pub fn dbg_directory_node_bitmaps() -> usize {
        super::segment_directory::NODE_BITMAPS
    }

    /// R11-6 TEST-ONLY: read the bit for `(node_bucket, class_idx, slot_idx)`
    /// directly by bucket index (not node_id). Used by the NUMA oracle to
    /// iterate all buckets and compare incremental vs rebuild per-bucket.
    #[doc(hidden)]
    #[cfg(feature = "alloc-segment-directory")]
    #[must_use]
    pub fn dbg_directory_get_bit_bucket(
        &self,
        bucket: usize,
        class_idx: usize,
        slot_idx: usize,
    ) -> Option<bool> {
        self.directory().map(|dir| {
            let word = slot_idx / 64;
            let bit_mask = 1u64 << (slot_idx % 64);
            dir.class_nonempty_by_node[bucket][class_idx][word] & bit_mask != 0
        })
    }

    /// R11-6 TEST-ONLY: directly invoke `find_segment_with_free(class_idx)`,
    /// BYPASSING `alloc_small`'s step-1 `pop_free(small_cur)` fast path. This
    /// forces the directory-driven lookup (or the linear scan fallback) to be
    /// the deciding factor, without depending on incidental `small_cur` state.
    ///
    /// Returns `Some(base)` (a segment-base pointer whose `BinTable[class_idx]`
    /// is non-empty) or `None` (no segment has a free block for this class).
    ///
    /// Used by `tests/segment_directory_numa.rs`'s local-first /
    /// foreign-fallback test to exercise the directory's node-bucket scan order
    /// in isolation, rather than depending on which segment `small_cur` points
    /// at after a mixed-node workload (which could make the decisive alloc
    /// resolve via `pop_free(small_cur)` before the directory is ever
    /// consulted — making the test vacuous with respect to the bucket order).
    ///
    /// `#[doc(hidden)] pub` per the established test-only-export pattern
    /// (CLAUDE.md "File and module structure" sanctioned exception 1). Not
    /// stable public API.
    #[doc(hidden)]
    pub fn dbg_find_segment_with_free(&mut self, class_idx: usize) -> Option<*mut u8> {
        self.find_segment_with_free(class_idx)
    }

    /// R8-2 (task #215) TEST-ONLY: directly clear a single bit in the
    /// materialised directory sidecar, BYPASSING the normal invariant (which
    /// only clears a bit in response to a real non-empty→empty transition).
    /// This is a test tool to SIMULATE directory drift — manufacture a
    /// directory bit that is stale-clear while the underlying `BinTable` still
    /// has a free block — so the R8-2 self-heal path (periodic re-validation
    /// full scan finds a segment the directory missed) can be exercised by a
    /// test. No real production code path should ever call this: every other
    /// call site of `publish_empty` is gated on an actual BinTable head
    /// transition to `FREE_LIST_NULL`. Returns `true` if the directory was
    /// materialised (and the bit cleared), `false` otherwise.
    #[doc(hidden)]
    pub fn dbg_directory_force_clear_bit(&mut self, class_idx: usize, slot_idx: usize) -> bool {
        #[cfg(feature = "alloc-segment-directory")]
        {
            if self.directory_sidecar.is_null() {
                return false;
            }
            // R11-6: derive the base from the table so publish_empty routes
            // to the correct node bucket (or clears all buckets if null).
            let base = self.table.base_at(slot_idx);
            self.publish_empty(base, class_idx, slot_idx);
            true
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = (class_idx, slot_idx);
            false
        }
    }

    /// R9-8 (task #230) TEST-ONLY: invoke the OOM-rescue scan directly for
    /// `class_idx` — a faithful mirror of what `alloc_small`'s step-4 `None`
    /// branch (and the magazine-refill equivalent) does right before surfacing
    /// an OOM to the user. Runs ONE forced O(S) linear scan that bypasses the
    /// R8-2 directory-trust fast path; if it finds a real free block the
    /// directory had hidden, self-heals the bit (inside the scan) and bumps
    /// `DIRECTORY_RESCUE_OOM_AVOIDED`, returning the segment base. Returns
    /// `None` if nothing was found (genuine OOM) or the directory is off /
    /// not materialised / `numa-aware`.
    ///
    /// This exists because reaching a real OOM (`MAX_SEGMENTS` table full or OS
    /// reservation failure) is impractical in a unit test (would require
    /// ~1024 live 4 MiB segments). The hook lets a test exercise the EXACT
    /// rescue code path the production OOM branches call, against a
    /// manufactured directory drift, without driving the table to capacity.
    #[doc(hidden)]
    pub fn dbg_directory_rescue_scan(&mut self, class_idx: usize) -> Option<*mut u8> {
        #[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]
        {
            if self.directory_sidecar.is_null() {
                return None;
            }
            let seg = self.find_segment_with_free_forced(class_idx);
            if seg.is_some() {
                #[cfg(feature = "alloc-stats")]
                super::directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED
                    .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            }
            seg
        }
        #[cfg(not(all(feature = "alloc-segment-directory", not(feature = "numa-aware"))))]
        {
            let _ = class_idx;
            None
        }
    }

    /// R7-A1: the materialisation threshold constant (test introspection).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_materialize_threshold() -> u32 {
        #[cfg(feature = "alloc-segment-directory")]
        {
            super::segment_directory::DIRECTORY_MATERIALIZE_THRESHOLD
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            0
        }
    }

    /// R8-2 (task #215) / R9-8 (task #230) TEST-ONLY: reset the per-instance
    /// `directory_miss_streak` counters to 0 for ALL classes. The streak is
    /// internal optimisation state (now PER-CLASS since R9-8) that
    /// `push_past_threshold` (and any other alloc sequence) leaves in an
    /// unknown residual value; tests that need to assert on the periodic
    /// re-validation boundary behaviour call this to put the streaks in a known
    /// state before driving misses. No production code path touches the streak
    /// outside `find_segment_with_free_impl`'s directory-miss branch.
    #[doc(hidden)]
    pub fn dbg_directory_reset_miss_streak(&mut self) {
        #[cfg(feature = "alloc-segment-directory")]
        {
            for c in self.directory_miss_streak.iter_mut() {
                *c = 0;
            }
        }
    }

    /// R9-8 (task #230) TEST-ONLY: read the per-instance `directory_miss_streak`
    /// counter for a SINGLE class — lets a test assert a specific class's streak
    /// value directly (the per-class decoupling proof checks that one class's
    /// misses do NOT advance another class's streak).
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_miss_streak_for_class(&self, class_idx: usize) -> u32 {
        #[cfg(feature = "alloc-segment-directory")]
        {
            self.directory_miss_streak
                .get(class_idx)
                .copied()
                .map_or(0, |v| v as u32)
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = class_idx;
            0
        }
    }

    /// R9-8 (task #230) TEST-ONLY: SET the per-instance `directory_miss_streak`
    /// counter for a SINGLE class to `value`. Lets a test place a class's streak
    /// at an arbitrary point (e.g. `period - 1`) WITHOUT driving polluting
    /// carves — used by the periodic-self-heal test to position class_x one miss
    /// shy of the re-validation boundary. No production code path sets the
    /// streak outside `find_segment_with_free_impl`'s directory-miss branch.
    #[doc(hidden)]
    pub fn dbg_directory_set_miss_streak_for_class(&mut self, class_idx: usize, value: u8) {
        #[cfg(feature = "alloc-segment-directory")]
        {
            if let Some(c) = self.directory_miss_streak.get_mut(class_idx) {
                *c = value;
            }
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            let _ = (class_idx, value);
        }
    }

    /// R8-2 (task #215): the periodic re-validation full-scan period constant
    /// (test introspection) — the streak length after which a genuine
    /// directory miss runs the full linear scan as a re-validation pass.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_directory_miss_full_scan_period() -> u32 {
        #[cfg(feature = "alloc-segment-directory")]
        {
            super::segment_directory::DIRECTORY_MISS_FULL_SCAN_PERIOD
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            0
        }
    }

    /// R7-A1 TEST-ONLY: re-run the full rebuild of the directory sidecar
    /// from the current `SegmentTable` state. Returns `true` if the
    /// directory was materialised (and thus rebuilt), `false` if it has not
    /// been materialised yet. This lets a test: (a) allocate enough to
    /// cross the threshold (directory materialised with all-zero BinTable
    /// heads), (b) free some blocks (creating non-empty BinTable entries),
    /// (c) call this to rebuild, (d) verify the bits match.
    #[doc(hidden)]
    pub fn dbg_rebuild_directory(&mut self) -> bool {
        #[cfg(feature = "alloc-segment-directory")]
        {
            let ptr = self.directory_sidecar;
            if ptr.is_null() {
                return false;
            }
            // Obtain a `&mut SegmentDirectory` from the raw pointer — the
            // same dereference `directory_mut()` does, but done BEFORE
            // borrowing `self.table` so the borrow checker sees two
            // disjoint borrows (the sidecar memory is heap-allocated, not
            // a field of `self`).
            let dir = os::deref_directory_sidecar_mut(ptr);
            // Zero out all bits first, then rebuild from scratch.
            // R11-6: iterate all node buckets.
            for nb in 0..super::segment_directory::NODE_BITMAPS {
                for c in 0..super::size_classes::SMALL_CLASS_COUNT {
                    for w in 0..super::segment_directory::WORDS_PER_CLASS {
                        dir.class_nonempty_by_node[nb][c][w] = 0;
                    }
                }
            }
            // R12-2: deliberately do NOT reset `node_ids` here. The dense
            // node-id -> bucket mapping is established ONCE, at first
            // materialisation (`maybe_materialize_directory`'s call to
            // `init_node_ids` + `rebuild_from_table`), and is APPEND-ONLY
            // from then on (new nodes may still claim free slots via
            // `node_bucket_mut`, but existing claims never move) — exactly
            // matching the incremental `set_bit`/`clear_bit` path's
            // discipline. Resetting `node_ids` here and re-deriving
            // "first-seen in TABLE-SLOT order" would silently pick a
            // DIFFERENT node->bucket assignment than "first-seen in
            // REAL-TIME class-transition order" (segment N being created
            // before segment M does not imply N's class transitions
            // empty->non-empty before M's — a segment fully consumed by the
            // time of materialisation contributes NO bits and registers NO
            // bucket until it is later freed into). Resetting here broke the
            // §7.3 item 1 per-bucket oracle
            // (`segment_directory_numa::per_node_oracle_holds_after_mixed_node_workload`):
            // it compared this test-only "rebuild from scratch" against
            // live incremental state bucket-for-bucket, and a reassigned
            // mapping made an otherwise-correct bit appear in the "wrong"
            // bucket. Preserving `node_ids` across rebuilds keeps bucket
            // identity stable, so only the BITS are re-derived (which is
            // the whole point of a self-healing rebuild) — matching what
            // the production self-heal call sites
            // (`publish_empty`/`sync_directory_for_segment_classes`) already
            // do (they never touch `node_ids` either).
            dir.rebuild_from_table(&self.table);
            true
        }
        #[cfg(not(feature = "alloc-segment-directory"))]
        {
            false
        }
    }
}
