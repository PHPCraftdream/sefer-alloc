//! [`SegmentHeader`] — the per-segment metadata block that lives at offset 0
//! of every segment, and [`PageMap`] / [`BinTable`] — the per-segment page
//! descriptors and per-size-class free bins, all carved from segment memory.
//!
//! These structures are the **self-hosted metadata** of the Phase 8 substrate
//! (§3 / §5 P8 of `MALLOC_PLAN.md`): they live INSIDE the segments they
//! describe, not in a `Vec`/`HashSet` on the global allocator. This is the
//! Membrane Inversion — the safe slot-table discipline governs OS memory
//! instead of consuming `std` collections.
//!
//! ## This file is PURE SAFE DATA + ARITHMETIC
//!
//! Every raw memory touch goes through the [`node`](super::node) seam. This
//! file declares only `#[repr(C)]` struct layouts, `const` offsets, and
//! methods that compute indices / route reads & writes through `Node`. There
//! is NO `unsafe` here — so the crate's structural promise ("`unsafe` lives
//! ONLY in `os` + `node`") is upheld by the compiler.
//!
//! ## Layout of a small segment
//!
//! ```text
//!   SEGMENT-aligned base
//!   ┌─────────────────────────────────────────────────────────────┐
//!   │ SegmentHeader (fixed-size, page-0)                         │
//!   │  • magic, kind, segment_id                                 │
//!   │  • bump cursor (next uncarved page offset, in bytes)       │
//!   │  • BinTable:  per-class free-list head OFFSETS (u32 each)  │
//!   │  • PageMap:    per-page descriptor (which class, or free)  │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ payload pages (carved bump-allocated into class runs)      │
//!   └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! `segment_of(ptr)` masks the low bits of `ptr` to find the segment base in
//! O(1); the header at offset 0 then tells the Cartographer everything about
//! that segment. Large/huge segments carry only `(size, align)` of their
//! single allocation — no page map (one allocation per segment).

use core::mem::size_of;

use super::node::Node;
use super::os::PAGE;
use super::size_classes::SMALL_CLASS_COUNT;

/// Magic value written to every segment header at creation. Used as a sanity
/// check that a computed segment base really is one of our segments (defence
/// against a foreign pointer being passed to `dealloc`).
pub(crate) const SEGMENT_MAGIC: u32 = 0x5E_F5_E0_01;

/// The number of pages in one segment (`SEGMENT / PAGE` = 1024 for the default
/// 4 MiB / 4 KiB pair). The `PageMap` has exactly this many entries.
pub(crate) const PAGES_PER_SEGMENT: usize = super::os::SEGMENT / PAGE;

/// Kind of a segment. Lives in the header so `segment_of(ptr)` immediately
/// tells the Cartographer how to handle a pointer into this segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SegmentKind {
    /// The primordial segment: hosts the global `SegmentTable` registry in
    /// its early bytes (after the header). Behaves as a small segment for the
    /// remaining payload.
    Primordial = 0,
    /// A small-segment: serves small size-class allocations via per-class free
    /// lists + a bump cursor over its payload pages.
    Small = 1,
    /// A large/huge segment: holds ONE allocation of arbitrary size/align. No
    /// page map; the header records the allocation's layout.
    Large = 2,
}

/// Per-page descriptor: which size class owns this page, or `Free` if the page
/// is uncarved. Encoded as a `u8` (we have ~40 small classes + sentinel
/// values). Pages are dedicated to a single class once carved (simplifies
/// free-list routing — a freed block returns to its page's class free list).
/// This is mimalloc's "page is owned by one size class" rule, which keeps the
/// free path O(1).
pub(crate) enum PageClass {
    /// The page is uncarved (still part of the bump region).
    Free = 0xFF,
    /// The page is metadata (the header / page map / bin table).
    Meta = 0xFE,
}

impl PageClass {
    /// Encode a small-class index as a `PageClass::Class(c)` byte.
    pub(crate) const fn encode_class(c: usize) -> u8 {
        debug_assert!(c < SMALL_CLASS_COUNT, "class_idx out of range");
        c as u8
    }
    /// Decode a page-map byte. Returns `Some(class_idx)` for a class page,
    /// `None` for `Free` / `Meta`.
    pub(crate) fn decode(b: u8) -> Option<usize> {
        match b {
            0xFF | 0xFE => None,
            c => {
                debug_assert!((c as usize) < SMALL_CLASS_COUNT, "corrupt page map entry");
                Some(c as usize)
            }
        }
    }
}

/// A fixed-size `SegmentHeader` laid down at offset 0 of every segment.
///
/// `#[repr(C)]` so the layout is deterministic and the bootstrap can compute
/// the page-map / bin-table offsets after it.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct SegmentHeader {
    /// Sanity magic — every segment starts with this. A computed segment base
    /// that does not have this magic is not one of our segments (foreign ptr).
    pub magic: u32,
    /// The segment kind (primordial / small / large). Decides dealloc routing.
    pub kind: SegmentKind,
    /// The segment's index in the global registry. `u32::MAX` until registered
    /// (the primordial segment is index 0).
    pub segment_id: u32,
    /// For small/primordial segments: the bump cursor, in BYTES from the
    /// segment base, of the next uncarved payload byte. The bootstrap sets it
    /// to the end of the metadata region (header + page map + bin table).
    pub bump: usize,
    /// For large/huge segments: the size (bytes) of the single allocation.
    /// Unused for small/primordial (zero).
    pub large_size: usize,
    /// For large/huge segments: the alignment of the single allocation.
    pub large_align: usize,
    /// The start of the OS reservation that produced this segment (may differ
    /// from the segment base due to the over-reserve + trim technique — see
    /// [`super::os`]). Recorded so `AllocCore::drop` can release the WHOLE
    /// reservation by walking the registry (no `Vec<Segment>` needed — this is
    /// part of the self-hosting discipline).
    pub reservation: *mut u8,
    /// The full size of the OS reservation (head + usable + tail). Paired with
    /// `reservation` for the OS free call.
    pub reservation_len: usize,
}

impl SegmentHeader {
    /// Build a fresh small-segment header value (does NOT write it — the
    /// bootstrap writes it through [`Node::write_struct`]). `bump` is where
    /// payload carving may begin (just past the metadata region).
    pub(crate) const fn small(
        segment_id: u32,
        bump: usize,
        reservation: *mut u8,
        reservation_len: usize,
    ) -> Self {
        Self {
            magic: SEGMENT_MAGIC,
            kind: SegmentKind::Small,
            segment_id,
            bump,
            large_size: 0,
            large_align: 0,
            reservation,
            reservation_len,
        }
    }

    /// Build a large/huge header value. The single allocation will live at
    /// the first page-aligned offset past the header.
    pub(crate) const fn large(
        segment_id: u32,
        size: usize,
        align: usize,
        bump: usize,
        reservation: *mut u8,
        reservation_len: usize,
    ) -> Self {
        Self {
            magic: SEGMENT_MAGIC,
            kind: SegmentKind::Large,
            segment_id,
            bump,
            large_size: size,
            large_align: align,
            reservation,
            reservation_len,
        }
    }

    /// Read the header at `base` (segment base, any kind) THROUGH the node
    /// seam. Returns a copy of the header. `base` MUST be a live segment base
    /// with a valid header at offset 0.
    pub(crate) fn read_at(base: *mut u8) -> Self {
        Node::read_struct::<SegmentHeader>(base as *const SegmentHeader)
    }

    /// Read the header's `kind` field only (the hot dealloc-routing path needs
    /// just this). Reads the full header through the seam (a single struct
    /// read is cheaper than one byte read at an offset for a small struct).
    #[allow(dead_code)] // Used by Phase 9+ cross-thread routing; kept for that.
    pub(crate) fn kind_at(base: *mut u8) -> SegmentKind {
        Self::read_at(base).kind
    }
}

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
/// owns the page (or `Free` / `Meta`). The Cartographer consults this on
/// `dealloc` to route a freed block to its page's class free list.
pub(crate) struct PageMap {
    /// Absolute address of the first entry (we store the absolute `*mut u8`
    /// so reads need no segment-base arithmetic).
    entries: *mut u8,
}

impl PageMap {
    /// Number of bytes the page map occupies in a segment. Fixed and known at
    /// compile time so the bootstrap can carve it deterministically.
    pub(crate) const FOOTPRINT: usize = PAGES_PER_SEGMENT * size_of::<u8>();

    /// Construct the view over an already-laid-down page map at `entries`.
    /// The bootstrap calls this AFTER writing the entries via [`init_in_place`].
    pub(crate) fn new(entries: *mut u8) -> Self {
        Self { entries }
    }

    /// Initialise a fresh page map at `entries`, marking `meta_pages` low
    /// pages `Meta` and the rest `Free`. Routes every byte write through
    /// [`Node::write_u8`].
    ///
    /// `entries` MUST point to `Self::FOOTPRINT` writable bytes inside the
    /// segment being initialised (caller's contract — the bootstrap).
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
    pub(crate) fn class_of(&self, p: usize) -> Option<usize> {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        let byte = Node::read_u8(self.entries_at_const(p));
        PageClass::decode(byte)
    }

    /// Mark page `p` as owned by size-class `class_idx`.
    pub(crate) fn set_class(&mut self, p: usize, class_idx: usize) {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        Node::write_u8(self.entries_at_const(p), PageClass::encode_class(class_idx));
    }

    /// Pointer to entry `p`. Caller guarantees `p < PAGES_PER_SEGMENT`.
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
    pub(crate) fn new(heads: *mut u32) -> Self {
        Self { heads }
    }

    /// Initialise a fresh empty bin table at `heads`. Every write routed
    /// through [`Node::write_u32_unaligned`]. `heads` MUST point to
    /// `Self::FOOTPRINT` writable bytes.
    pub(crate) fn init_in_place(heads: *mut u32) {
        for c in 0..SMALL_CLASS_COUNT {
            Node::write_u32_unaligned(Node::offset(heads as *mut u8, c * size_of::<u32>()) as *mut u32, FREE_LIST_NULL);
        }
    }

    /// The segment-relative offset of the head free block of class `c`, or
    /// `FREE_LIST_NULL` if empty.
    pub(crate) fn head(&self, c: usize) -> u32 {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        Node::read_u32_unaligned(self.heads_at_const(c))
    }

    /// Set the head of class `c`'s free list to `off`.
    pub(crate) fn set_head(&mut self, c: usize, off: u32) {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        Node::write_u32_unaligned(self.heads_at_const(c), off);
    }

    fn heads_at_const(&self, c: usize) -> *mut u32 {
        Node::offset(self.heads as *mut u8, c * size_of::<u32>()) as *mut u32
    }
}

/// The metadata footprint of a small segment: header + page map + bin table,
/// each laid out at fixed offsets (see [`Layout::small`]). This does NOT
/// include the registry array (which lives only in the primordial segment).
#[allow(dead_code)] // Compile-time sanity only; consumed by the `const _` asserts below.
pub(crate) const SMALL_META_FOOTPRINT: usize = Layout::small_meta_end();

/// The fixed layout of in-segment metadata: offsets of header / page map /
/// bin table. Centralised so the bootstrap and `SegmentMeta` agree.
pub(crate) struct Layout;
impl Layout {
    /// Offset of the page map (page-aligned past the header).
    pub(crate) const fn page_map_off() -> usize {
        align_up_const(size_of::<SegmentHeader>(), PAGE)
    }
    /// Offset of the bin table (right after the page map).
    pub(crate) const fn bin_table_off() -> usize {
        Self::page_map_off() + PageMap::FOOTPRINT
    }
    /// End of the small-segment metadata (page-aligned past the bin table).
    /// Payload carving begins here.
    pub(crate) const fn small_meta_end() -> usize {
        align_up_const(Self::bin_table_off() + BinTable::FOOTPRINT, PAGE)
    }
    /// Offset of the registry array in the primordial segment (page-aligned
    /// past the bin table — the registry is primordial-only).
    pub(crate) const fn primordial_registry_off() -> usize {
        align_up_const(Self::bin_table_off() + BinTable::FOOTPRINT, PAGE)
    }
    /// End of the primordial metadata (page-aligned past the registry).
    pub(crate) const fn primordial_meta_end() -> usize {
        align_up_const(
            Self::primordial_registry_off() + super::segment_table::REGISTRY_FOOTPRINT,
            PAGE,
        )
    }
    /// Number of metadata pages in a small segment.
    pub(crate) const fn small_meta_pages() -> usize {
        Self::small_meta_end() / PAGE
    }
    /// Number of metadata pages in the primordial segment.
    pub(crate) const fn primordial_meta_pages() -> usize {
        Self::primordial_meta_end() / PAGE
    }
}

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

    /// The page-map view.
    pub(crate) fn page_map(&self) -> PageMap {
        PageMap::new(Node::offset(self.base, Layout::page_map_off()))
    }

    /// The bin-table view.
    pub(crate) fn bin_table(&self) -> BinTable {
        BinTable::new(Node::offset(self.base, Layout::bin_table_off()) as *mut u32)
    }
}

const fn align_up_const(n: usize, a: usize) -> usize {
    let mask = a - 1;
    (n + mask) & !mask
}

// Compile-time sanity: the metadata footprints must fit in one segment with
// room for at least one payload page, and the smallest size class must hold a
// free-list node.
const _: () = assert!(Layout::primordial_meta_end() + PAGE <= super::os::SEGMENT);
const _: () = assert!(Layout::small_meta_end() + PAGE <= super::os::SEGMENT);
const _: () = assert!(super::size_classes::MIN_BLOCK >= super::node::NODE_SIZE);
