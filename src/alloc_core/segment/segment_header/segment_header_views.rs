//! Field-specific view accessors for [`SegmentHeader`] (mechanical split of
//! `segment_header.rs`, task R6-CQ-7c).

use crate::alloc_core::node::Node;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};

impl SegmentHeader {
    /// Owner-only, field-specific kind read from the canonical table root.
    #[allow(dead_code)] // Used by Phase 9+ cross-thread routing; kept for that.
    #[inline(always)]
    pub(crate) fn kind_at(base: *mut u8) -> SegmentKind {
        let off = core::mem::offset_of!(SegmentHeader, kind);
        // The `SegmentKind` discriminant is one byte at `base + off`; read it
        // via the node seam and transcribe the raw byte back to the enum.
        let b = Node::read_u8(Node::offset(base, off) as *const u8);
        // L-5 (UBFIX-11): STRICT decode — the byte was laid down by
        // `SegmentHeader::small`/`large` as a valid `SegmentKind` discriminant
        // (`#[repr(u8)]`) and the header is otherwise immutable in this
        // field, so in the well-formed case the byte is always one of
        // {0,1,2}. Previously any OTHER byte (a corrupted/garbled kind — a
        // wild write from an unrelated bug, or the aftermath of an
        // H-1-class defect before its fix) fell through to a `_ => Small`
        // default: a corrupt/unexpected byte was silently treated as a
        // VALID, specific segment kind — amplifying the corruption instead
        // of containing it (e.g. a Large segment with a corrupted kind byte
        // would be misrouted onto the Small free path, and a
        // Small-specific free would write a BinTable/free-list header into
        // a live Large payload). `magic_at` (checked by the caller first on
        // the cross-thread path) rejects a non-sefer BASE, but does nothing
        // to validate the `kind` BYTE of a base that IS ours but has been
        // corrupted in place — so this decode must reject on its own.
        // Every unexpected byte now maps to `SegmentKind::Unknown`, a
        // sentinel no constructor ever writes; every caller of `kind_at`
        // tests for a SPECIFIC expected kind via `==`/`matches!` (never an
        // exhaustive match with an implicit catch-all — see the callers
        // inventory in this task's audit), so `Unknown` naturally fails
        // every such check and is routed to that call site's existing
        // "not this kind" no-op/reject branch.
        match b {
            0 => SegmentKind::Primordial,
            1 => SegmentKind::Small,
            2 => SegmentKind::Large,
            _ => SegmentKind::Unknown,
        }
    }

    /// Owner-only logical Large size, read through the canonical table root.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "medium-classes",
        any(
            not(feature = "exact-span-large"),
            all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
        )
    ))]
    #[inline(always)]
    pub(crate) fn large_size_at(base: *mut u8) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// Owner-only alignment for the current Large allocation.
    #[cfg(all(
        feature = "alloc-global",
        feature = "fastbin",
        feature = "medium-classes",
        any(
            not(feature = "exact-span-large"),
            all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
        )
    ))]
    #[inline(always)]
    pub(crate) fn large_align_at(base: *mut u8) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, large_align);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// Read the header's `segment_id` field only (field-specific `u32` load).
    /// Used by [`SegmentTable::unregister`](crate::alloc_core::segment_table::SegmentTable::unregister)
    /// / [`SegmentTable::recycle`](crate::alloc_core::segment_table::SegmentTable::recycle)
    /// (task #135) to locate a segment's registry slot index in O(1), instead
    /// of scanning the table for a matching base pointer. `segment_id` is
    /// written ONCE, at registration time, as part of the freshly-built header
    /// value passed to a full-struct `Node::write_struct` (`alloc_large_slow`,
    /// the large-cache-hit path, `register_segment`'s caller) — never mutated
    /// in place thereafter — so a field read here does not race with the
    /// owner's `bump` field writes on a disjoint field (same discipline as
    /// `magic_at`/`kind_at`). Present in EVERY build's layout (like `magic`),
    /// so this accessor is not feature-gated.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    #[inline(always)]
    pub(crate) fn segment_id_at(base: *mut u8) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, segment_id);
        Node::read_u32(Node::offset(base, off) as *const u32)
    }

    /// Read the header's `span_usable` field only (field-specific `usize`
    /// load). For large segments this is the PHYSICAL committed usable span
    /// (the full OS reservation rounded to whole segments). Used by
    /// `AllocCore::realloc` to decide whether an in-place Large grow fits
    /// without reallocation.
    ///
    /// `span_usable` is written once at segment construction
    /// (`SegmentHeader::large`) or carried forward verbatim on a cache-hit
    /// reuse — never mutated in place field-by-field — so a field-specific
    /// read here does not race with the owner's disjoint `bump` writes (same
    /// discipline as `kind_at`/`large_size_at`).
    #[inline(always)]
    pub(crate) fn span_usable_at(base: *mut u8) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, span_usable);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// Overwrite the header's `large_size` field only (field-specific `usize`
    /// store, mirroring `large_size_at`'s read). Used by `AllocCore::realloc`
    /// to update the logical allocation size after an in-place Large grow
    /// (the segment's physical span is unchanged — only the recorded size
    /// advances).
    ///
    /// Safety discipline: called ONLY by the owning thread's `realloc` path,
    /// which is the single writer for this segment. The field sits at a fixed
    /// offset disjoint from `bump` / `owner_state`, so no cross-field race.
    #[inline(always)]
    pub(crate) fn set_large_size_at(base: *mut u8, size: usize) {
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::write_usize(Node::offset(base, off) as *mut usize, size);
    }

    /// F12 (task #498): overwrite the header's `large_align` field only
    /// (field-specific `usize` store, mirrors [`set_large_size_at`](Self::set_large_size_at)).
    /// Used by `AllocCore::alloc_large`'s large-cache HIT arm to update the
    /// logical allocation alignment for the new occupant, as one of a
    /// targeted set of field writes replacing a full-struct `write_struct`
    /// (see that call site's own comment for the full UBFIX-6 safety
    /// restatement covering the whole targeted-write group).
    ///
    /// Safety discipline: called only while `base` is still UNREGISTERED (not
    /// yet inserted into `SegmentTable`'s `contains_base` hash table by
    /// `register()`), so no cross-thread reader can address this segment at
    /// all — same precondition as `set_large_size_at`/`set_bump_at`/
    /// `set_magic_at` at this call site.
    ///
    /// `cfg_attr(not(alloc-decommit))`: the sole call site (the large-cache
    /// hit arm) lives entirely inside `AllocCore::alloc_large`'s
    /// `#[cfg(feature = "alloc-decommit")]` block (the large_cache mechanism
    /// itself does not exist without that feature), so this accessor has no
    /// caller — and would otherwise be dead code — when the feature is off.
    #[cfg_attr(not(feature = "alloc-decommit"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn set_large_align_at(base: *mut u8, align: usize) {
        let off = core::mem::offset_of!(SegmentHeader, large_align);
        Node::write_usize(Node::offset(base, off) as *mut usize, align);
    }

    /// F12 (task #498): overwrite the header's `bump` field only
    /// (field-specific `usize` store). `bump`'s usual owner-only accessor is
    /// [`SegmentMeta::set_bump`](crate::alloc_core::segment_header::SegmentMeta::set_bump),
    /// which takes `&mut SegmentMeta`; this `*_at(base)` sibling exists
    /// because `AllocCore::alloc_large`'s large-cache HIT arm only has a raw
    /// `slot.base` pointer at this point (the segment is not yet registered,
    /// so no `SegmentMeta` handle has been constructed for it) — same
    /// `offset_of!`-through-`Node` discipline, same field, same owner-only
    /// write.
    ///
    /// Safety discipline: called only while `base` is still UNREGISTERED —
    /// see [`set_large_align_at`](Self::set_large_align_at)'s doc for the
    /// shared precondition.
    #[cfg_attr(not(feature = "alloc-decommit"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn set_bump_at(base: *mut u8, bump: usize) {
        let off = core::mem::offset_of!(SegmentHeader, bump);
        Node::write_usize(Node::offset(base, off) as *mut usize, bump);
    }

    /// F12 (task #498): overwrite the header's `magic` field only
    /// (field-specific `u32` store) with `SEGMENT_MAGIC`, re-establishing the
    /// sanity magic that the large-cache deposit path atomically zeroed (see
    /// [`magic_at`](Self::magic_at)'s doc for that Release-store writer).
    ///
    /// **Deliberately a PLAIN store, not an atomic one**, even though
    /// `magic_at`'s cross-thread reader pairs an atomic Release writer with
    /// its Acquire load in the STEADY-STATE case (a registered, live segment
    /// whose `magic` a remote thread may concurrently zero on deposit). This
    /// call site is different: it runs strictly BEFORE `register()` makes
    /// `slot.base` reachable via `contains_base`, so no concurrent reader —
    /// atomic-aware or otherwise — can observe this write at all; the
    /// unregistered-window argument that lets the pre-existing full-struct
    /// `Node::write_struct` (also a plain, non-atomic write) be sound here
    /// applies identically to this narrower field-wise write. See the
    /// UBFIX-6 comment at this accessor's only call site
    /// (`AllocCore::alloc_large`'s large-cache hit arm,
    /// `alloc_core_large.rs`) for the restated argument in full.
    #[cfg_attr(not(feature = "alloc-decommit"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn set_magic_at(base: *mut u8, magic: u32) {
        let off = core::mem::offset_of!(SegmentHeader, magic);
        Node::write_u32(Node::offset(base, off) as *mut u32, magic);
    }

    /// R12-4 (feature `large-reserved-capacity`): read the header's
    /// `reserved_capacity` field only (field-specific `usize` load, mirrors
    /// `span_usable_at`'s read pattern). Used by the OPT-G grow path to
    /// decide whether a growing `realloc` that no longer fits within the
    /// COMMITTED `span_usable` can still grow in place by committing more of
    /// the already-RESERVED `reserved_capacity`, instead of falling back to
    /// the slow alloc+copy+free path.
    ///
    /// `reserved_capacity` is written once at segment construction
    /// (`SegmentHeader::large`) or carried forward verbatim on a cache-hit
    /// reuse (same bug-#134-shaped discipline as `span_usable` — see that
    /// field's doc) — never mutated in place field-by-field except by
    /// [`set_span_usable_at`](Self::set_span_usable_at) growing the
    /// COMMITTED frontier below it, so a field-specific read here does not
    /// race with the owner's disjoint `bump` writes (same discipline as
    /// `span_usable_at`).
    #[cfg_attr(not(feature = "large-reserved-capacity"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn reserved_capacity_at(base: *mut u8) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, reserved_capacity);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// R12-4 (feature `large-reserved-capacity`): overwrite the header's
    /// `span_usable` field only (field-specific `usize` store, mirroring
    /// `span_usable_at`'s read). Used by the OPT-G grow path to advance the
    /// COMMITTED frontier after successfully committing
    /// `[old span_usable, new span_usable)` via
    /// [`crate::alloc_core::os::commit_pages`] — `span_usable`'s own meaning (the
    /// segment's TRUE COMMITTED span) is UNCHANGED by this addition; this is
    /// simply the first production write site for a field that was
    /// previously write-once (see [`SegmentHeader::span_usable`]'s doc,
    /// which this call site's own doc extends rather than contradicts).
    ///
    /// Safety discipline: called ONLY by the owning thread's `realloc` path,
    /// immediately after a successful `commit_pages` call for the exact same
    /// range — the field sits at a fixed offset disjoint from `bump` /
    /// `owner_state`, so no cross-field race, and no cross-thread reader ever
    /// races the OWNER growing its own segment's committed frontier (the
    /// cross-thread reads of `span_usable_at`/`safe_payload_read_span` only
    /// ever need a LOWER BOUND on the committed span to stay sound — see
    /// `AllocCore::safe_payload_read_span`'s doc — and this call only ever
    /// GROWS the value, never shrinks it, so a stale (pre-grow) read is
    /// merely conservative, never unsound).
    #[cfg_attr(not(feature = "large-reserved-capacity"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn set_span_usable_at(base: *mut u8, span_usable: usize) {
        let off = core::mem::offset_of!(SegmentHeader, span_usable);
        Node::write_usize(Node::offset(base, off) as *mut usize, span_usable);
    }

    /// TEST-ONLY (task #135): overwrite the header's `segment_id` field only
    /// (field-specific write, mirroring `segment_id_at`'s read). Used by
    /// `AllocCore::dbg_stamp_segment_id` to exercise `SegmentTable::unregister`'s
    /// defensive `slots[id] == base` guard against a corrupted `segment_id`
    /// (see `tests/segment_table_o1.rs`). Never called on any production path.
    #[allow(dead_code)] // TEST-ONLY hook, see `///` doc above (task #135)
    pub(crate) fn set_segment_id_at(base: *mut u8, id: u32) {
        let off = core::mem::offset_of!(SegmentHeader, segment_id);
        Node::write_u32(Node::offset(base, off) as *mut u32, id);
    }
}
