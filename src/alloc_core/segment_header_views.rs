//! Field-specific view accessors for [`SegmentHeader`] (mechanical split of
//! `segment_header.rs`, task R6-CQ-7c).

use super::node::Node;
use super::segment_header::{SegmentHeader, SegmentKind};

impl SegmentHeader {
    /// Read the header's `kind` field only (field-specific read: a single
    /// byte load at the field's offset, NOT a full-struct read). The
    /// dealloc-routing hot path needs just this together with `magic` and
    /// `owner_thread_free`; reading each field individually avoids the
    /// full-struct `read_at` that raced with the owner's `bump` field writes
    /// (the §11 root cause — `kind`/`owner_thread_free` are written once at
    /// init/stamp time and only read cross-thread thereafter, with no atomic
    /// writer anywhere, so a plain field read of either does not race the
    /// owner's `bump` writes on a disjoint field. `magic` is read separately
    /// by `magic_at` as an ATOMIC load because it IS atomically zeroed on
    /// recycle — see that accessor and the R6-MS-5 audit note in the block
    /// comment above `bump_of`).
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

    /// Read the header's `magic` field only (field-specific ATOMIC `u32`
    /// load). Used by the cross-thread dealloc-routing path to validate the
    /// segment base without reading the whole mutable header.
    ///
    /// `magic` is laid down as `SEGMENT_MAGIC` at segment construction (via a
    /// full-struct `Node::write_struct`) and is then ATOMICALLY zeroed by the
    /// large-object recycle/reclaim paths when a segment is returned to the
    /// OS-reservation cache — `AllocCore::dealloc`'s Large-cache-deposit
    /// branch (`alloc_core.rs`) and `AllocCore::alloc_large`'s eviction branch
    /// (`alloc_core_large.rs`) both write it through
    /// `Node::atomic_u32_at(base, off).store(0, Ordering::Release)`. A PLAIN
    /// (non-atomic) read here, racing that atomic store from another thread,
    /// is a data race under Rust's memory model (R6-MS-5 / U-R5-1) — and the
    /// defensive-free contract exists precisely to stay safe under caller
    /// misuse (a stale/duplicate remote free), which is exactly the misuse
    /// that can interleave this read with the recycler's atomic zeroing store,
    /// so this validation route must not itself become a data-race source.
    ///
    /// We therefore read through the same `&AtomicU32` view at `magic`'s
    /// `offset_of!` offset the writers use, with `Ordering::Acquire` to pair
    /// their `Release` store: this field is the FIRST thing the cross-thread
    /// dealloc-routing path reads before touching any further header state
    /// (`kind_at`/`owner_thread_free_at`/`large_size_at`), and an Acquire load
    /// keeps those subsequent reads ordered after the header-write they
    /// describe (a load observing `SEGMENT_MAGIC` sees a live, fully-
    /// constructed header; a load observing `0` has synchronized-with the
    /// recycler's Release and routes the base to the foreign/no-op branch). On
    /// x86_64 an Acquire `u32` load compiles to a plain `mov` (no fence), so
    /// this is not a pessimization of the hot free path — confirmed by the iai
    /// before/after in the R6-MS-5 commit (Ir unchanged on the recycle bench).
    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn magic_at(base: *mut u8) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, magic);
        Node::atomic_u32_at(base, off).load(core::sync::atomic::Ordering::Acquire)
    }

    /// Read the header's `owner_thread_free` field only (field-specific pointer
    /// load). Used by the cross-thread dealloc-routing path to find the owning
    /// heap's TFS head without reading the whole mutable header. The field is
    /// written ONCE at stamp time (by the owning thread) and only read
    /// cross-thread thereafter, so a field read does not race with the owner's
    /// `bump` writes on a disjoint field.
    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn owner_thread_free_at(base: *mut u8) -> *const core::sync::atomic::AtomicPtr<u8> {
        let off = core::mem::offset_of!(SegmentHeader, owner_thread_free);
        Node::read_ptr(Node::offset(base, off) as *const *const core::sync::atomic::AtomicPtr<u8>)
    }

    /// Read the header's `large_size` field only (field-specific `usize`
    /// load). 0.3.0 (task #138, A1 post-reuse mitigation): used by the
    /// cross-thread Large-free routing paths (`HeapCore::dealloc_routing`,
    /// `Heap::dealloc_any_thread`) to sanity-check that the freeing layout is
    /// consistent with the CURRENT occupant of the segment before queuing it
    /// onto the owner's deferred-free stack — see the mitigation's doc
    /// comment on [`push_large_deferred_free`](crate::alloc_core::deferred_large::push_large_deferred_free)
    /// for the full rationale and its documented residual limit.
    ///
    /// `large_size` is written once at segment construction
    /// (`SegmentHeader::large`) and on every large-cache-hit reuse
    /// (`AllocCore::alloc_large`'s hit path rewrites the WHOLE header via
    /// `Node::write_struct` before the segment is handed to a new caller —
    /// never mutated in place field-by-field), so a field-specific read here
    /// does not race the owner's disjoint `bump` writes (same discipline as
    /// `kind_at`/`magic_at`/`owner_thread_free_at`). It IS, by design, able to
    /// observe a DIFFERENT value than the one the freeing thread's stale
    /// `Layout` was allocated against, if the segment has already been
    /// reclaimed and reused for a new allocation between the free and this
    /// read — that race is exactly what this check exists to catch (a
    /// mismatch here means "this is not a free of the CURRENT occupant").
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn large_size_at(base: *mut u8) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// Read the header's `segment_id` field only (field-specific `u32` load).
    /// Used by [`SegmentTable::unregister`](super::segment_table::SegmentTable::unregister)
    /// / [`SegmentTable::recycle`](super::segment_table::SegmentTable::recycle)
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
