use core::mem::size_of;

use crate::alloc_core::node::Node;
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

#[cfg(feature = "page-map-diag")]
use super::PageClass;
use super::{SegmentHeader, PAGES_PER_SEGMENT};

/// Round `n` up to the next multiple of `a`. Works for ANY `a > 0` (not just
/// powers of two) — the size-class table uses 1.25× spacing (rounded to
/// `MIN_BLOCK`), so most block sizes are NOT powers of two. Pure safe integer
/// arithmetic; the `debug_assert` catches a zero/misuse.
pub(crate) fn align_up(n: usize, a: usize) -> usize {
    debug_assert!(a > 0, "align must be non-zero");
    // Ceiling division: `ceil(n / a) * a`. Avoids overflow vs `n + a - 1`.
    let q = n.div_ceil(a);
    q * a
}

/// The per-segment page descriptor table. `PAGES_PER_SEGMENT` entries of one
/// byte each, carved from the segment right after the header.
///
/// Each entry is a [`PageClass`] discriminant byte telling which size class
/// owns the page (or `Free` / `Meta`).
///
/// NOTE (post-Phase 13.3): this table is **NOT load-bearing for class routing**;
/// do NOT derive block classes from it. No production `dealloc` path derives a
/// freed block's class from `PageMap` — the class is carried authoritatively by
/// the caller's `Layout` (own-thread) or recovered by the owner from the route
/// sidecar's owner-issued class map (cross-thread). Deriving a class here would reintroduce the mixed-class /
/// stale-cursor drain-reclaim bug fixed in §13 of `RACE_DRAIN_RECLAIM.md`.
///
/// R12-11 (task #262): an inventory of every call site confirmed the note
/// above — the ONLY reader is the doc-hidden test seam
/// `AllocCore::dbg_page_map_class_for`, which several §13 counterfactual
/// regression tests use as an ORACLE to prove the real dealloc/reclaim paths
/// do NOT consult it (see `tests/phase13_3_dealloc_layout_class.rs`,
/// `tests/phase13_drain_reclaim_layout_class.rs`). Maintaining the table is
/// therefore diagnostic-only work: [`new`](Self::new), [`Self::init_in_place`],
/// [`Self::set_class`], [`Self::set_free`], and [`Self::class_of`] are ALL gated behind the
/// `page-map-diag` feature (see its `Cargo.toml` doc for the full rationale)
/// and elided from the default/`production` build. Only `FOOTPRINT` stays
/// UNCONDITIONAL: it only describes the fixed offset this table occupies in
/// `Layout`, so the segment metadata byte layout is identical in every
/// feature configuration — only the per-segment/per-carve WORK of
/// maintaining/viewing the table's contents is elided, not its place in the
/// layout.
pub(crate) struct PageMap {
    /// Absolute address of the first entry (we store the absolute `*mut u8`
    /// so reads need no segment-base arithmetic).
    #[cfg(feature = "page-map-diag")]
    entries: *mut u8,
}

impl PageMap {
    /// Number of bytes the page map occupies in a segment. Fixed and known at
    /// compile time so the bootstrap can carve it deterministically.
    pub(crate) const FOOTPRINT: usize = PAGES_PER_SEGMENT * size_of::<u8>();

    /// Construct the view over an already-laid-down page map at `entries`.
    /// The bootstrap calls this AFTER writing the entries via [`Self::init_in_place`].
    #[cfg(feature = "page-map-diag")]
    pub(crate) fn new(entries: *mut u8) -> Self {
        Self { entries }
    }

    /// Initialise a fresh page map at `entries`, marking `meta_pages` low
    /// pages `Meta` and the rest `Free`. Routes every byte write through
    /// [`Node::write_u8`].
    ///
    /// `entries` MUST point to `Self::FOOTPRINT` writable bytes inside the
    /// segment being initialised (caller's contract — the bootstrap).
    ///
    /// R12-11 (task #262, `page-map-diag`): diagnostic-only maintenance —
    /// see the struct doc. Gated out of the default build.
    #[cfg(feature = "page-map-diag")]
    pub(crate) fn init_in_place(entries: *mut u8, meta_pages: usize) {
        for p in 0..PAGES_PER_SEGMENT {
            let byte = if p < meta_pages {
                PageClass::Meta as u8
            } else {
                PageClass::Free as u8
            };
            Node::write_u8(Node::offset(entries, p), byte);
        }
    }

    /// Read the class of page `p` (decoded). Panics (debug) if
    /// `p >= PAGES_PER_SEGMENT`.
    ///
    /// R12-11 (task #262, `page-map-diag`): diagnostic-only — see the struct
    /// doc. The only caller is `AllocCore::dbg_page_map_class_for`, itself
    /// gated behind the same feature.
    #[cfg(feature = "page-map-diag")]
    pub(crate) fn class_of(&self, p: usize) -> Option<usize> {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        let byte = Node::read_u8(self.entries_at_const(p));
        PageClass::decode(byte)
    }

    /// Mark page `p` as owned by size-class `class_idx`.
    ///
    /// R12-11 (task #262, `page-map-diag`): diagnostic-only — see the struct
    /// doc. Gated out of the default build.
    #[cfg(feature = "page-map-diag")]
    pub(crate) fn set_class(&mut self, p: usize, class_idx: usize) {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        Node::write_u8(self.entries_at_const(p), PageClass::encode_class(class_idx));
    }

    /// Mark page `p` as `Free` (uncarved). Phase 35: used by the M6 decommit
    /// reset to return an emptied segment's payload pages to the bump region.
    ///
    /// R12-11 (task #262, `page-map-diag`): diagnostic-only — see the struct
    /// doc. Gated out of the default build (additionally to the pre-existing
    /// `alloc-decommit` gate).
    #[cfg(all(feature = "alloc-decommit", feature = "page-map-diag"))]
    pub(crate) fn set_free(&mut self, p: usize) {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        Node::write_u8(self.entries_at_const(p), PageClass::Free as u8);
    }

    /// Pointer to entry `p`. Caller guarantees `p < PAGES_PER_SEGMENT`.
    #[cfg(feature = "page-map-diag")]
    fn entries_at_const(&self, p: usize) -> *mut u8 {
        // Routed through the `node` seam (`add` is unsafe; the seam documents
        // the in-bounds contract).
        Node::offset(self.entries, p)
    }
}

/// The per-segment per-class free-list head table. One `u32` OFFSET per small
/// class — the segment-relative offset of the head free block of that class,
/// or `FREE_LIST_NULL` if the class's free list is empty.
///
/// Storing offsets (not pointers) keeps the table compact (40 × 4 B = 160 B)
/// and lets the Cartographer reason entirely in safe integers; the conversion
/// to a pointer happens only at the `node` seam when popping.
pub(crate) struct BinTable {
    /// Absolute address of the first `u32` head. `SMALL_CLASS_COUNT` entries.
    heads: *mut u32,
}

/// Sentinel value for "this class's free list is empty". A real offset is
/// always `< SEGMENT`, so `u32::MAX` is unambiguous.
pub(crate) const FREE_LIST_NULL: u32 = u32::MAX;

impl BinTable {
    /// Footprint of the bin table in a segment. Fixed so the bootstrap can
    /// carve it deterministically.
    pub(crate) const FOOTPRINT: usize = SMALL_CLASS_COUNT * size_of::<u32>();

    /// Construct the view over an already-laid-down bin table at `heads`.
    #[inline(always)]
    pub(crate) fn new(heads: *mut u32) -> Self {
        Self { heads }
    }

    /// Initialise a fresh empty bin table at `heads`. Every write routed
    /// through [`Node::write_u32_unaligned`]. `heads` MUST point to
    /// `Self::FOOTPRINT` writable bytes.
    pub(crate) fn init_in_place(heads: *mut u32) {
        for c in 0..SMALL_CLASS_COUNT {
            Node::write_u32_unaligned(
                Node::offset(heads as *mut u8, c * size_of::<u32>()) as *mut u32,
                FREE_LIST_NULL,
            );
        }
    }

    /// The segment-relative offset of the head free block of class `c`, or
    /// `FREE_LIST_NULL` if empty.
    ///
    /// R6-MS-3 (round5 memory_safety_review R5-MS-3): an out-of-range `c`
    /// (`>= SMALL_CLASS_COUNT`) is a RELEASE-MODE no-op returning
    /// `FREE_LIST_NULL`, NOT merely a `debug_assert!`. The previous guard
    /// compiled out under the `production` profile, so a caller-controlled
    /// `class_idx` (e.g. via `flush_class`/`dbg_freelist_head_for`) could raw-
    /// read `heads + c * 4` out of bounds. The check lives here, inside the
    /// lowest-level accessor, so EVERY caller (production `dealloc_small`/
    /// `flush_run`, and the doc-hidden `dbg_*` seams) is protected uniformly;
    /// the `debug_assert!` is retained as a debug-mode tripwire.
    #[inline(always)]
    pub(crate) fn head(&self, c: usize) -> u32 {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        if c >= SMALL_CLASS_COUNT {
            return FREE_LIST_NULL;
        }
        Node::read_u32_unaligned(self.heads_at_const(c))
    }

    /// Set the head of class `c`'s free list to `off`.
    ///
    /// R6-MS-3: an out-of-range `c` (`>= SMALL_CLASS_COUNT`) is a RELEASE-MODE
    /// no-op, NOT merely a `debug_assert!` (same rationale as [`head`](Self::head)).
    #[inline(always)]
    pub(crate) fn set_head(&mut self, c: usize, off: u32) {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        if c >= SMALL_CLASS_COUNT {
            return;
        }
        Node::write_u32_unaligned(self.heads_at_const(c), off);
    }

    #[inline(always)]
    fn heads_at_const(&self, c: usize) -> *mut u32 {
        Node::offset(self.heads as *mut u8, c * size_of::<u32>()) as *mut u32
    }
}

/// The metadata footprint of a small segment: header + page map + bin table,
/// each laid out at fixed offsets (see [`Layout::small_meta_end`]). This does NOT
/// include the registry array (which lives only in the primordial segment).
#[allow(dead_code)] // Compile-time sanity only; consumed by the `const _` asserts below.
pub(crate) const SMALL_META_FOOTPRINT: usize = Layout::small_meta_end();

/// The fixed layout of in-segment metadata: offsets of header / page map /
/// bin table. Centralised so the bootstrap and `SegmentMeta` agree.
pub(crate) struct Layout;

/// Accessor triple for the in-segment metadata of a small/primordial segment.
/// The bootstrap / `AllocCore` use this to obtain typed views over the header,
/// page map, and bin table of a segment given its base pointer.
pub(crate) struct SegmentMeta {
    pub base: *mut u8,
}

impl SegmentMeta {
    /// Construct the metadata view for a small/primordial segment whose base
    /// is `base` and whose header / page map / bin table are laid down at
    /// their [`Layout`] offsets.
    #[inline(always)]
    pub(crate) fn new(base: *mut u8) -> Self {
        Self { base }
    }

    /// Read the segment header (a copy).
    pub(crate) fn header(&self) -> SegmentHeader {
        SegmentHeader::read_at(self.base)
    }

    /// Write the segment header through the node seam.
    pub(crate) fn write_header(&mut self, hdr: SegmentHeader) {
        Node::write_struct(self.base as *mut SegmentHeader, hdr);
    }

    // Field-specific owner accesses avoid copying unaffected metadata.

    /// Read the owner-only `bump` cursor (the next uncarved payload byte
    /// offset). Owner-only: the owning thread is the sole reader/writer of
    /// `bump`; a plain field read is race-free (no Remote ever reads it).
    #[inline(always)]
    pub(crate) fn bump_of(&self) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, bump);
        Node::read_usize(Node::offset(self.base, off) as *const usize)
    }

    /// Write the owner-only bump cursor without copying the whole header.
    #[inline(always)]
    pub(crate) fn set_bump(&mut self, value: usize) {
        let off = core::mem::offset_of!(SegmentHeader, bump);
        Node::write_usize(Node::offset(self.base, off) as *mut usize, value);
    }

    /// The page-map view.
    ///
    /// R12-11 (task #262): diagnostic-only — see `PageMap`'s struct doc.
    /// Gated behind `page-map-diag`.
    #[cfg(feature = "page-map-diag")]
    pub(crate) fn page_map(&self) -> PageMap {
        PageMap::new(Node::offset(self.base, Layout::page_map_off()))
    }

    /// The bin-table view.
    #[inline(always)]
    pub(crate) fn bin_table(&self) -> BinTable {
        BinTable::new(Node::offset(self.base, Layout::bin_table_off()) as *mut u32)
    }

    /// The alloc-bitmap view (the Phase 13.4a O(1) double-free guard). The
    /// bitmap bytes are carved at [`Layout::alloc_bitmap_off`] and zeroed at
    /// bootstrap; this returns the typed view over them.
    #[inline(always)]
    pub(crate) fn alloc_bitmap(&self) -> crate::alloc_core::alloc_bitmap::AllocBitmap {
        crate::alloc_core::alloc_bitmap::AllocBitmap::new(Node::offset(
            self.base,
            Layout::alloc_bitmap_off(),
        ))
    }

    /// RAD-5 (E4) GO/NO-GO EXPERIMENT — the magazine-residency bitmap view.
    /// The bitmap bytes are carved at [`Layout::magazine_bitmap_off`] and
    /// zeroed at bootstrap (mirroring `alloc_bitmap`); this returns the typed
    /// view over them. See `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the
    /// measured verdict.
    #[inline(always)]
    pub(crate) fn magazine_bitmap(&self) -> crate::alloc_core::magazine_bitmap::MagazineBitmap {
        crate::alloc_core::magazine_bitmap::MagazineBitmap::new(Node::offset(
            self.base,
            Layout::magazine_bitmap_off(),
        ))
    }

    // -------------------------------------------------------------------
    // Atomic view over the owner identity field.
    // return `&AtomicU64` at the field's fixed offset so a cross-thread
    // read/store is a genuine atomic operation (NOT a non-atomic struct
    // field read, which would be a data race under concurrency). The single
    // `unsafe` dereference lives in the [`node`](crate::alloc_core::node) seam
    // (`Node::atomic_u64_at`); the field offset is computed by the safe
    // `core::mem::offset_of!` macro on the `#[repr(C)]` header, so this file
    // stays unsafe-free, as it has been since Phase 8.
    // -------------------------------------------------------------------

    /// A `&AtomicU64` view over this segment's `owner_state` field.
    /// The owner-stamp path (`stamp_segment_owner`) writes it with a
    /// Release store; the owner's stamp fast path and diagnostic probes
    /// read it. Foreign free routing resolves the owning heap from the
    /// route descriptor, not from this field. The view aliases the
    /// header byte range; access is atomic so there is no data race with a
    /// concurrent header read.
    ///
    /// # Caller's contract
    ///
    /// `self.base` MUST be a live small/primordial segment base with a valid
    /// header at offset 0 (the caller — cross-thread free routing / owner
    /// stamping — guarantees this; the segment is registered and has a valid
    /// header).
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn owner_state_atomic(&self) -> &'static core::sync::atomic::AtomicU64 {
        // `offset_of!` is a safe macro (address arithmetic on a
        // `#[repr(C)]` type); the atomic-view dereference is delegated to
        // the `node` seam.
        let off = core::mem::offset_of!(SegmentHeader, owner_state);
        Node::atomic_u64_at(self.base, off)
    }
}
