//! `internals`-gated segment-header introspection of [`AllocCore`] — raw
//! field byte reads/stamps, kind decode tag, size/zero-pass counters, and
//! the virgin-bit probe (mechanical split of the former flat
//! `alloc_core_core_diag.rs`; pure code movement, no behavior changed).

use core::alloc::Layout;

use crate::alloc_core::alloc_core::AllocCore;
use crate::alloc_core::node::Node;
use crate::alloc_core::os;
#[cfg(feature = "virgin-zero-skip")]
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};
use crate::alloc_core::size_classes::{AllocKind, SizeClasses};

/// Sol-F1 (task #563): `internals`-gated — every other `dbg_*` diagnostic
/// hook in this file. See this file's module doc for the full rationale.
#[cfg(feature = "internals")]
impl AllocCore {
    /// TEST-ONLY (L-5, UBFIX-11): read the RAW `kind` discriminant byte of
    /// `ptr`'s segment header (not decoded through `SegmentHeader::kind_at` —
    /// the exact byte at the `kind` field's offset). Lets a test capture the
    /// legitimate byte before corrupting it, and confirm the corruption
    /// actually landed.
    ///
    /// R2-05 (independent src review round 2, task #2007): the raw read below
    /// goes through the table's own STORED (canonical) pointer, not `ptr`'s
    /// caller-derived address — see `SegmentTable::canonical_base_of`'s doc
    /// for why matching an address alone does not grant provenance to read
    /// through it.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_kind_byte_of(&self, ptr: *mut u8) -> u8 {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self
            .table
            .canonical_base_of(candidate)
            .expect("dbg_kind_byte_of: ptr's segment is not owned by this AllocCore");
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
    ///
    /// R2-05 (independent src review round 2, task #2007): decodes through
    /// the table's own STORED (canonical) pointer, not `ptr`'s caller-derived
    /// address — see `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_kind_at_tag(&self, ptr: *mut u8) -> u8 {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self
            .table
            .canonical_base_of(candidate)
            .expect("dbg_kind_at_tag: ptr's segment is not owned by this AllocCore");
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
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer — see
    /// `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    pub fn dbg_large_size_of(&self, ptr: *mut u8) -> usize {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self
            .table
            .canonical_base_of(candidate)
            .expect("dbg_large_size_of: ptr's segment is not owned by this AllocCore");
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// TEST-ONLY (R12-3): read the `span_usable` field from the header of
    /// `ptr`'s segment — the segment's PHYSICAL committed byte span (see
    /// `SegmentHeader::span_usable`'s doc). Same field-read pattern as
    /// `dbg_large_size_of`. Lets integration tests verify the `exact-span-large`
    /// feature actually shrinks the physical reservation below a whole
    /// `SEGMENT` (4 MiB), instead of only inferring it indirectly from RSS.
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer — see
    /// `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    pub fn dbg_span_usable_of(&self, ptr: *mut u8) -> usize {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self
            .table
            .canonical_base_of(candidate)
            .expect("dbg_span_usable_of: ptr's segment is not owned by this AllocCore");
        let off = core::mem::offset_of!(SegmentHeader, span_usable);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// TEST-ONLY (R12-4): read the `reserved_capacity` field from the header
    /// of `ptr`'s segment — the segment's total RESERVED VA span (see
    /// `SegmentHeader::reserved_capacity`'s doc). Same field-read pattern as
    /// `dbg_span_usable_of`. Lets integration tests verify the
    /// `large-reserved-capacity` feature actually reserves extra VA beyond
    /// the committed `span_usable`, and that a growing `realloc` commits
    /// into it without moving the allocation.
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer — see
    /// `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    pub fn dbg_reserved_capacity_of(&self, ptr: *mut u8) -> usize {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self
            .table
            .canonical_base_of(candidate)
            .expect("dbg_reserved_capacity_of: ptr's segment is not owned by this AllocCore");
        let off = core::mem::offset_of!(SegmentHeader, reserved_capacity);
        Node::read_usize(Node::offset(base, off) as *const usize)
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
        crate::alloc_core::size_classes::SMALL_CLASS_COUNT
    }

    #[doc(hidden)]
    pub fn dbg_layout_class_for(&self, layout: Layout) -> Option<usize> {
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
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
        crate::alloc_core::alloc_core::counters::LARGE_ZERO_PASS_CALLS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (R12-10, task #261, `virgin-zero-skip`): process-wide count
    /// of explicit `Node::zero` passes on the Small-classified `alloc_zeroed`
    /// path (both the `AllocCore` and `HeapCore` faces bump the same
    /// counter). Mirrors [`dbg_large_zero_pass_count`](Self::dbg_large_zero_pass_count)
    /// exactly. Lets `tests/alloc_zeroed_virgin_small_skip.rs` assert the
    /// virgin-carve SKIP actually fires (delta 0 on a genuinely fresh carve
    /// under a real OS) and that the explicit zero actually runs where it
    /// must (free-list reuse; any alloc under miri). Relaxed load —
    /// diagnostic only. Reads 0 unless BOTH `virgin-zero-skip` AND
    /// `alloc-stats` are on; the accessor is always compiled so callers need
    /// no `#[cfg]`.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_small_zero_pass_count() -> u64 {
        crate::alloc_core::alloc_core::counters::SMALL_ZERO_PASS_CALLS
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (R12-10, task #261, `virgin-zero-skip`): read the owner-only
    /// `payload_virgin` bit of `ptr`'s segment, or `None` if `ptr` is foreign
    /// / not a small or primordial segment. Lets tests assert the bit's state
    /// directly (e.g. after a fresh reservation, or after forcing the
    /// decommit-retain regression path via
    /// [`dbg_force_decommit_retain`](Self::dbg_force_decommit_retain)).
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer, not `ptr`'s caller-derived
    /// address — see `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    #[cfg(feature = "virgin-zero-skip")]
    #[must_use]
    pub fn dbg_payload_virgin_for(&self, ptr: *mut u8) -> Option<bool> {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self.table.canonical_base_of(candidate)?;
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).payload_virgin_of())
    }
}
