//! Segment layout / offset arithmetic for [`Layout`] (mechanical split of
//! `segment_header.rs`, task R6-CQ-7c).

use super::os::{MAX_REALISTIC_PAGE_SIZE, PAGE};
#[cfg(feature = "hardened")]
use super::segment_header::GEN_TABLE_FOOTPRINT;
use super::segment_header::{align_up_const, BinTable, Layout, PageMap, SegmentHeader};

use core::mem::size_of;

impl Layout {
    /// Offset of the page map (page-aligned past the header).
    pub(crate) const fn page_map_off() -> usize {
        align_up_const(size_of::<SegmentHeader>(), PAGE)
    }
    /// Offset of the bin table (right after the page map).
    pub(crate) const fn bin_table_off() -> usize {
        Self::page_map_off() + PageMap::FOOTPRINT
    }
    /// Offset of the per-segment [`AllocBitmap`](super::alloc_bitmap::AllocBitmap)
    /// — the O(1) double-free guard (Phase 13.4a), one bit per `MIN_BLOCK` slot
    /// of the whole segment. Placed AFTER **two** `BinTable::FOOTPRINT`s, 8-byte
    /// aligned: the second `BinTable` footprint is the slot Phase 13.4b's
    /// two-list (`free` + `local_free`) will occupy. Reserving it now means
    /// 13.4b adds its second head array in place WITHOUT shifting the bitmap /
    /// ring / registry offsets again (the spec's "compute the layout with the
    /// doubled BinTable up front" requirement — §1.2 / §2).
    pub(crate) const fn alloc_bitmap_off() -> usize {
        align_up_const(Self::bin_table_off() + BinTable::FOOTPRINT * 2, 8)
    }
    /// RAD-5 (E4) GO/NO-GO EXPERIMENT — offset of the per-segment
    /// [`MagazineBitmap`](super::magazine_bitmap::MagazineBitmap), an
    /// orthogonal second bitmap recording magazine residency (see its module
    /// doc). Placed immediately after `AllocBitmap`, 8-byte aligned, same
    /// `FOOTPRINT` geometry (one bit per `MIN_BLOCK` slot). See
    /// `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the measured verdict
    /// before assuming this offset is load-bearing for any shipped feature.
    pub(crate) const fn magazine_bitmap_off() -> usize {
        align_up_const(
            Self::alloc_bitmap_off() + super::alloc_bitmap::AllocBitmap::FOOTPRINT,
            8,
        )
    }
    /// Offset of the per-segment `RemoteFreeRing` (the non-intrusive
    /// cross-thread-free MPSC queue of `u32` block-offsets). Lives in segment
    /// metadata right after the magazine bitmap (RAD-5), 4-byte aligned (each
    /// ring slot is a `u32`). Carved alongside the bin table at bootstrap. See
    /// [`crate::alloc_core::remote_free_ring::RemoteFreeRing`] for the protocol.
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    pub(crate) const fn remote_ring_off() -> usize {
        align_up_const(
            Self::magazine_bitmap_off() + super::magazine_bitmap::MagazineBitmap::FOOTPRINT,
            4,
        )
    }
    /// Offset of the per-segment **generation table** (X7 Ф1, task #189) — the
    /// hardened remote-free staleness guard: one `AtomicU8` per `MIN_BLOCK`
    /// granule of the segment, recording the current "life number" of the block
    /// at that granule. Lives in segment metadata right after the remote-free
    /// ring, 1-byte aligned (it is a byte array). Compiled ONLY under
    /// `#[cfg(feature = "hardened")]`; under any other feature config the
    /// generation table does not exist and [`small_meta_end`] is byte-identical
    /// to the pre-X7 layout (the production-judge-neutrality requirement — see
    /// the layout assertions at the bottom of this file). See
    /// [`GEN_TABLE_FOOTPRINT`] / [`gen_at`] / [`bump_gen`].
    #[cfg(feature = "hardened")]
    pub(crate) const fn gen_table_off() -> usize {
        // 1-byte aligned (the table is a byte array). `remote_ring_off` is
        // already 4-byte aligned and the ring footprint is a multiple of 4, so
        // this offset is at least 4-aligned — trivially ≥ 1-aligned.
        Self::remote_ring_off() + super::remote_free_ring::FOOTPRINT
    }
    /// End of the small-segment metadata (page-aligned past the last metadata
    /// region). Payload carving begins here.
    ///
    /// Stacks up to THREE conditional metadata regions in a fixed order, each
    /// composing cleanly on top of the prior so all feature combinations
    /// produce a correct, non-overlapping layout:
    ///
    /// 1. **base** (always): header + page map + bin table + alloc bitmap +
    ///    remote ring (see [`remote_ring_off`] /
    ///    [`super::remote_free_ring::FOOTPRINT`]).
    /// 2. **generation table** (X7 Ф1, `#[cfg(feature = "hardened")]` only):
    ///    one byte per `MIN_BLOCK` granule, 1-byte aligned, immediately after
    ///    the remote ring — see [`gen_table_off`] / [`GEN_TABLE_FOOTPRINT`].
    ///
    /// The final value is page-aligned past the last present region. Under
    /// every feature config the layout is the byte-for-byte composition of the
    /// regions that exist in that config — verified by the ungated
    /// `small_meta_end() + PAGE <= SEGMENT` const assert at the bottom of this
    /// file (X7-Ф1's neutrality argument).
    pub(crate) const fn small_meta_end() -> usize {
        // PLAT-1 (task #205): align to MAX_REALISTIC_PAGE_SIZE (64 KiB), NOT
        // PAGE (4 KiB). This offset is a decommit/recommit boundary (see
        // `alloc_core_small.rs` / `alloc_core_small_pool.rs`); on a 16 KiB- or
        // 64 KiB-page machine a 4 KiB-aligned boundary lands mid-real-page and
        // `madvise`/`VirtualFree` silently round it, reclaiming the wrong byte
        // range. 64 KiB alignment is a superset of every real page size up to
        // 64 KiB. Cannot use the runtime `aligned_vmem::page_size()` here —
        // this is a `const fn` evaluated in true `const` contexts.
        align_up_const(Self::small_meta_end_pre_runstack(), MAX_REALISTIC_PAGE_SIZE)
    }
    /// The end of the small-segment metadata BEFORE final page-alignment — i.e.
    /// the unaligned byte offset just past the last metadata region (the
    /// remote-free ring under non-hardened; the generation table under
    /// `hardened`). The value [`small_meta_end`] page-aligns. Private to this
    /// module; the public surface is [`small_meta_end`].
    const fn small_meta_end_pre_runstack() -> usize {
        #[cfg(feature = "hardened")]
        {
            Self::gen_table_off() + GEN_TABLE_FOOTPRINT
        }
        #[cfg(not(feature = "hardened"))]
        {
            Self::remote_ring_off() + super::remote_free_ring::FOOTPRINT
        }
    }
    /// Offset of the registry array in the primordial segment (page-aligned
    /// past ALL small-segment metadata — header + page map + bin table + alloc
    /// bitmap + remote ring + [gen table under `hardened`]). The registry is
    /// primordial-only; it sits after
    /// `small_meta_end()` so it never overlaps any small-segment metadata
    /// region.
    ///
    /// **X7 Ф3 (task #191) fix:** under `hardened`, `small_meta_end()` includes
    /// the generation table (~256 KiB). The pre-Ф3 code computed this offset
    /// from `remote_ring_off + FOOTPRINT` directly, which SKIPPED the gen table
    /// — so under hardened the registry/hash/free-list were carved ON TOP OF the
    /// gen table region, silently corrupting each other. Using `small_meta_end()`
    /// (which already accounts for the gen table under hardened, and is identical
    /// to the old computation under non-hardened) fixes the overlap. The
    /// non-hardened value is byte-identical to the pre-Ф3 layout (both compute
    /// `align_up(remote_ring_off + FOOTPRINT, PAGE)` — `small_meta_end` IS that
    /// value when the gen table is absent).
    pub(crate) const fn primordial_registry_off() -> usize {
        Self::small_meta_end()
    }
    /// Offset of the open-addressing hash table in the primordial segment
    /// (immediately after the registry array, 8-byte aligned).
    pub(crate) const fn primordial_hash_off() -> usize {
        align_up_const(
            Self::primordial_registry_off() + super::segment_table::REGISTRY_FOOTPRINT,
            8,
        )
    }
    /// Offset of the free-list index-stack array (task #135, Part 1) —
    /// immediately after the hash table, 4-byte aligned (the array holds
    /// `u32` indices).
    pub(crate) const fn primordial_free_list_off() -> usize {
        align_up_const(
            Self::primordial_hash_off() + super::segment_table::HASH_FOOTPRINT,
            4,
        )
    }
    /// Offset of the free-list top-of-stack counter (a single `u32`),
    /// immediately after the free-list array.
    pub(crate) const fn primordial_free_top_off() -> usize {
        Self::primordial_free_list_off() + super::segment_table::FREE_LIST_FOOTPRINT
    }
    /// End of the primordial metadata (page-aligned past the free-list top
    /// counter).
    pub(crate) const fn primordial_meta_end() -> usize {
        // PLAT-1 (task #205): same decommit/recommit-boundary rationale as
        // `small_meta_end` — align to MAX_REALISTIC_PAGE_SIZE, not PAGE.
        align_up_const(Self::primordial_free_top_off() + 4, MAX_REALISTIC_PAGE_SIZE)
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
