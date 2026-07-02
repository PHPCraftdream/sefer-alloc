//! [`primordial`] — the bootstrap routine that hand-carves the first
//! [`SegmentTable`] from the first segment (the `_mi_heap_main` analogue).
//!
//! This is the primordial bootstrap. It runs ONCE, at `AllocCore::new`, to
//! establish the self-hosting loop: the primordial segment is reserved; its
//! header, page map, bin table, and the registry array are laid down at their
//! fixed [`Layout`] offsets; and the registry's first slot is set to point
//! back at itself. After this, the safe Cartographer can mutate metadata
//! through normal `node`-seam writes with no further bootstrap-time writes.
//!
//! ## This file is PURE SAFE COMPOSITION
//!
//! Every raw memory touch goes through the [`os`](super::os) seam (segment
//! reservation) and the [`node`](super::node) seam (typed writes). The
//! bootstrap composes those already-proven `unsafe` primitives in safe code —
//! there is NO `unsafe` block in this file. So the crate's structural promise
//! ("`unsafe` lives ONLY in `os` + `node`") is upheld by the compiler.

use super::os::Segment;
use super::segment_header::{Layout, SegmentHeader, SegmentKind, SegmentMeta};
use super::segment_table::{self, SegmentTable};

/// The bootstrap outcome: the primordial [`Segment`] (owned) and the
/// [`SegmentTable`] view over the registry carved in its payload. The
/// primordial segment's metadata (header / page map / bin table) is laid down
/// by `primordial()`; `AllocCore::new` reads it back via `SegmentMeta` when it
/// needs to mutate the primordial as a small segment.
pub(crate) struct Primordial {
    pub segment: Segment,
    pub table: SegmentTable,
}

/// Reserve the primordial segment and hand-carve its self-hosted metadata.
///
/// This is the analogue of mimalloc's `_mi_heap_main` init: it establishes the
/// segment that hosts the registry, after which the allocator is self-hosting.
///
/// Returns `None` only if the OS refuses the primordial reservation (OOM at
/// startup — unrecoverable for an allocator, but we propagate rather than
/// abort so a caller may fall back).
pub(crate) fn primordial() -> Option<Primordial> {
    // 1. Reserve the primordial segment (SEGMENT-aligned, SEGMENT bytes). This
    //    is the ONLY OS allocation primitive on the bootstrap path; everything
    //    else is safe composition over the segment's bytes.
    let segment = Segment::reserve(super::os::SEGMENT)?;
    let base = segment.as_ptr();
    let reservation = segment.reservation();
    let reservation_len = segment.reservation_len();

    // 2. Lay down the small-segment header at offset 0 via the node seam. We
    //    write `bump = 0` here and fix it up after computing the metadata end.
    let mut meta = SegmentMeta::new(base);
    meta.write_header(SegmentHeader::small(
        0,
        0,
        reservation.as_ptr(),
        reservation_len,
    ));

    // 3. Initialise the page map and bin table at their fixed offsets. The
    //    `Node::write_*` primitives (called from `init_in_place`) do the raw
    //    writes; this code only computes offsets.
    let pm_off = Layout::page_map_off();
    let bt_off = Layout::bin_table_off();
    let reg_off = Layout::primordial_registry_off();
    let meta_end = Layout::primordial_meta_end();
    let meta_pages = Layout::primordial_meta_pages();

    // SAFETY (caller-side reasoning, encoded as the `init_in_place` contract):
    // `base + pm_off` is within the freshly-reserved segment (compile-time
    // checked: `Layout::primordial_meta_end() + PAGE <= SEGMENT`), and the
    // segment is exclusively owned (single-threaded bootstrap). Each
    // `init_in_place` call writes only its `FOOTPRINT` bytes via `Node`.
    super::segment_header::PageMap::init_in_place(base_plus(base, pm_off), meta_pages);
    super::segment_header::BinTable::init_in_place(base_plus(base, bt_off) as *mut u32);
    // Initialise the per-segment alloc-bitmap (Phase 13.4a O(1) double-free
    // guard) to all-zeros ("everything allocated / not-a-block"). Carved at the
    // fixed `alloc_bitmap_off`; the bits are flipped to FREE as blocks are
    // pushed onto free lists.
    super::alloc_bitmap::AllocBitmap::init_in_place(base_plus(base, Layout::alloc_bitmap_off()));
    // Initialise the per-segment non-intrusive cross-thread-free ring (the
    // Variant-2 fix: queues carry offsets, never poison the block). Only under
    // `alloc-xthread`; without it the ring metadata is reserved (the Layout
    // always carves it, to keep the byte layout uniform) but left uninitialised
    // — it is never read on the single-thread path.
    #[cfg(feature = "alloc-xthread")]
    {
        let ring_off = Layout::remote_ring_off();
        super::remote_free_ring::RemoteFreeRing::init_in_place(base, ring_off);
    }

    // 4. Lay down the registry array at `reg_off`. Slot 0 is the primordial
    //    segment's own base (self-reference). The write goes through `Node`.
    let reg_slots = base_plus(base, reg_off) as *mut *mut u8;
    super::node::Node::write_struct::<*mut u8>(reg_slots, base);

    // 4b. OPT-B: Initialise the open-addressing hash table at `hash_off`.
    //     Zero-fill all HASH_CAPACITY slots (null_mut() = empty). Then insert
    //     the primordial base (slot 0's value) so `contains_base` works from
    //     the very first allocation.
    let hash_off = Layout::primordial_hash_off();
    let hash_slots = base_plus(base, hash_off) as *mut *mut u8;
    // Zero-fill: each slot must start as null_mut() (= "empty").
    for i in 0..segment_table::HASH_CAPACITY {
        let slot =
            super::node::Node::offset(hash_slots as *mut u8, i * core::mem::size_of::<*mut u8>())
                as *mut *mut u8;
        super::node::Node::write_struct::<*mut u8>(slot, core::ptr::null_mut());
    }
    // Insert the primordial base into the hash table (mirrors slot 0 write).
    // The hash table starts empty, so we hand-probe: start at hash_index(base),
    // find the first empty slot, and write `base`. Since the table is freshly
    // zeroed, slot hash_index(base) is guaranteed empty.
    {
        let start_idx =
            (base as usize >> segment_table::SEGMENT_SHIFT) & (segment_table::HASH_CAPACITY - 1);
        let hash_slot = super::node::Node::offset(
            hash_slots as *mut u8,
            start_idx * core::mem::size_of::<*mut u8>(),
        ) as *mut *mut u8;
        super::node::Node::write_struct::<*mut u8>(hash_slot, base);
    }

    // 4c. Task #135 (Part 1): initialise the free-list index-stack (recycled
    //     slot indices) and its top-of-stack counter. The stack starts EMPTY
    //     (top = 0) — slot 0 (primordial) is live and never recyclable, and
    //     no other slot has been registered yet, so there is nothing to
    //     recycle. Zero-fill the index array defensively (only entries
    //     `[0, top)` are ever read, but a clean zero state keeps the layout
    //     inspectable/debuggable).
    let free_list_off = Layout::primordial_free_list_off();
    let free_top_off = Layout::primordial_free_top_off();
    let free_list_slots = base_plus(base, free_list_off) as *mut u32;
    for i in 0..segment_table::FREE_LIST_CAPACITY {
        let slot =
            super::node::Node::offset(free_list_slots as *mut u8, i * core::mem::size_of::<u32>())
                as *mut u32;
        super::node::Node::write_u32(slot, 0);
    }
    let free_top_ptr = base_plus(base, free_top_off) as *mut u32;
    super::node::Node::write_u32(free_top_ptr, 0);

    // 5. Fix up the header: kind = Primordial, bump = meta_end (where payload
    //    carving begins). Mark the page map / bin table / registry pages Meta
    //    in the page map we just wrote.
    let mut hdr = meta.header();
    hdr.kind = SegmentKind::Primordial;
    hdr.bump = meta_end;
    meta.write_header(hdr);

    // 6. Construct the SegmentTable view. `from_primordial` is safe (it
    //    performs no memory operation — just wraps the pointer + count); the
    //    contract that slot 0 was written is the bootstrap's invariant.
    let table =
        SegmentTable::from_primordial(reg_slots, 1, hash_slots, free_list_slots, free_top_ptr);

    Some(Primordial { segment, table })
}

/// `base + off` as a `*mut u8`, routed through the `node` seam (`add` is
/// unsafe; the seam documents the in-bounds contract). The result is
/// dereferenced later only by code that has proven `off` is within the segment
/// bounds.
fn base_plus(base: *mut u8, off: usize) -> *mut u8 {
    super::node::Node::offset(base, off)
}
