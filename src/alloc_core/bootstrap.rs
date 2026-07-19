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

#[cfg(all(feature = "alloc-lazy-commit", not(feature = "numa-aware")))]
use super::alloc_core_small::LAZY_FIRST_CHUNK;
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
    //
    // R7-B6 (primordial lazy commit): under `alloc-lazy-commit` AND NOT
    // `numa-aware` (Windows; Unix/miri fall back to the eager, fully-committed
    // path inside `aligned_vmem::reserve_aligned_lazy` itself), commit ONLY
    // `[0, primordial_meta_end() + LAZY_FIRST_CHUNK)` up front instead of the
    // whole 4 MiB segment. `primordial_meta_end()` is the exact byte offset
    // past every region this function writes below (header, page map, bin
    // table, [bitmaps under miri], remote ring, registry array, hash table,
    // free-list array + top) — see `Layout::primordial_meta_end`'s doc and
    // the const-assert in `segment_header.rs` pinning
    // `primordial_meta_end() + LAZY_FIRST_CHUNK <= SEGMENT`. Everything this
    // function writes therefore lands strictly inside the committed prefix by
    // construction — there is no write-before-commit hazard: the whole
    // metadata region is committed BEFORE any of the writes below run, not
    // committed incrementally alongside them. `LAZY_FIRST_CHUNK` beyond
    // `meta_end` additionally covers the FIRST payload carve (the immediate
    // post-bootstrap allocation reuses the primordial as its first small
    // segment, `AllocCore::new_inner`'s `small_cur = primordial_base`), so
    // that allocation does not need a grow-on-carve commit either. Any carve
    // that grows past this initial frontier goes through the SAME
    // `carve_block`/`carve_batch` grow-on-carve path (`alloc_core_small.rs`)
    // that already handles ordinary small segments — that path reads/writes
    // `committed_payload_end` generically over `SegmentKind::Small |
    // SegmentKind::Primordial` (see e.g. `dealloc_small`'s existing
    // Primordial-aware `payload_start` branch), so no new carve-path code is
    // needed here.
    //
    // `numa-aware` exclusion: the primordial reservation itself never goes
    // through `numa::reserve_aligned_on_node` (it predates NUMA awareness —
    // see `AllocCore::new_inner`'s comment), so there is no P2-gate conflict
    // in the same sense `reserve_small_segment` has. This exclusion is kept
    // anyway for two reasons: (1) it matches the `alloc-lazy-commit` feature
    // doc's own blanket statement in `Cargo.toml` ("Under `numa-aware`, the
    // lazy path is disabled ... to preserve NUMA placement"), so the
    // primordial does not silently become the one exception to a documented,
    // crate-wide policy; (2) it keeps `Segment::reserve` (the plain eager
    // path) reachable under `--all-features` (which enables BOTH
    // `alloc-lazy-commit` and `numa-aware` together) — without this
    // exclusion `Segment::reserve` would have no remaining caller in that
    // configuration once the small-segment AND primordial paths both moved to
    // their NUMA-gated lazy/eager arms, tripping `-D warnings`' dead-code lint.
    //
    // On the eager path (feature-OFF, or `numa-aware`), `Segment::reserve` is
    // unchanged — byte-identical to pre-R7-B6 behaviour.
    #[cfg(all(feature = "alloc-lazy-commit", not(feature = "numa-aware")))]
    let segment = {
        let initial_commit = Layout::primordial_meta_end() + LAZY_FIRST_CHUNK;
        // Uphold `reserve_lazy`'s full documented contract (non-zero, PAGE
        // multiple, `<= SEGMENT`), not just the size bound — both hold by
        // construction here (`primordial_meta_end()` is `align_up(_, PAGE)` and
        // `LAZY_FIRST_CHUNK` is a const-asserted PAGE multiple), but assert all
        // three so a future layout change that broke either fails loudly.
        debug_assert!(
            initial_commit != 0
                && initial_commit.is_multiple_of(aligned_vmem::PAGE)
                && initial_commit <= super::os::SEGMENT
        );
        Segment::reserve_lazy(initial_commit)?
    };
    #[cfg(not(all(feature = "alloc-lazy-commit", not(feature = "numa-aware"))))]
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
    //
    // PERF-PASS-2 (G5/C1, task #50): under `cfg(not(miri))` this explicit
    // zero-init is SKIPPED — `base` is the PRIMORDIAL segment, reserved a few
    // lines above via `Segment::reserve` (the ONLY OS allocation primitive on
    // this bootstrap path — see the module doc), never carved or decommitted
    // before this point. The OS guarantees fresh pages are zero (see the
    // matching comment at `AllocCore::reserve_small_segment`'s identical skip,
    // `alloc_core.rs`), so re-zeroing here is a tautology. Under `miri` the
    // fallback aperture (`std::alloc::alloc`) is NOT guaranteed zeroed, so
    // miri keeps the explicit init unconditionally.
    #[cfg(miri)]
    super::alloc_bitmap::AllocBitmap::init_in_place(base_plus(base, Layout::alloc_bitmap_off()));
    // RAD-5 (E4) GO/NO-GO EXPERIMENT: same virgin-skip discipline extended to
    // the second (magazine-residency) bitmap — see
    // `magazine_bitmap.rs`'s module doc and `MagazineBitmap::init_in_place`'s
    // doc comment. Skipped under `cfg(not(miri))` for the identical reason as
    // the line above (fresh OS pages read zero; the target init state is
    // all-zeros).
    #[cfg(miri)]
    super::magazine_bitmap::MagazineBitmap::init_in_place(base_plus(
        base,
        Layout::magazine_bitmap_off(),
    ));
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
    // X7 Ф3 (task #191): zero the per-segment generation table under
    // `hardened`. Compiled ONLY under `hardened`; under any other feature the
    // table does not exist and this call is absent (byte-identical to the
    // pre-X7 build). Without this zeroing, a `gen_at`/`bump_gen` Relaxed load
    // on a never-written cell is UB (miri-confirmed during Ф1) — the carried-
    // over Ф1 gap this call closes. The table is NOT re-zeroed on
    // decommit-reset: the X7 plan §2.2 fixes generation numbering as
    // CONTINUOUS across decommit-reset, so old generations persist intentionally.
    #[cfg(feature = "hardened")]
    {
        // SAFETY: `base` is a live, exclusively-owned segment whose
        // generation table is carved and writable.
        #[allow(unsafe_code)]
        unsafe {
            super::segment_header::init_gen_table_in_place(base)
        };
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

    // B1 (R7 Workstream B) / R7-B6 (primordial lazy commit): stamp the
    // committed-payload frontier, mirroring `reserve_small_segment`'s
    // identical 3-way stamping for ordinary small segments (R8-5, task #218)
    // — this MUST match step 1's reservation gate exactly, since it stamps
    // what step 1 actually committed.
    //
    //   1. `numa-aware` (any platform): `SEGMENT`. The primordial reservation
    //      uses the plain eager `Segment::reserve`, and NUMA reservations
    //      stay fully eager (P2 gate).
    //
    //   2. `alloc-lazy-commit` AND NOT `numa-aware` AND real Windows (not
    //      miri): `meta_end + LAZY_FIRST_CHUNK`. `Segment::reserve_lazy` did
    //      a REAL partial commit via the Windows 2-phase
    //      `VirtualAlloc(MEM_RESERVE)` + `VirtualAlloc(MEM_COMMIT)` prefix,
    //      and the frontier accurately reflects it.
    //
    //   3. `alloc-lazy-commit` AND NOT `numa-aware` AND Unix/miri: `SEGMENT`.
    //      `reserve_aligned_lazy` internally ignores `initial_commit` and
    //      `mmap`s / `alloc`s the WHOLE segment up front (Unix has no separate
    //      reserve/commit distinction; miri models no RSS). Pre-R8-5 the
    //      frontier was understated at `meta_end + LAZY_FIRST_CHUNK` here too
    //      — sound but pointless, since B2's grow-on-carve then ran a
    //      `commit_pages` (a correctness no-op on these platforms) on every
    //      carve past the artificial frontier. R8-5 stamps `SEGMENT`
    //      immediately, matching the OS-level reality and restoring the
    //      feature's zero-cost-when-unneeded property on Unix/miri.
    //
    // The genuine Windows-lazy case (2) still goes through B2's grow-on-carve
    // path on later carves past the frontier; this primordial stamp only
    // changes the frontier's STARTING value on Unix/miri, not the grow-on-
    // carve mechanism.
    #[cfg(all(feature = "alloc-lazy-commit", feature = "numa-aware"))]
    meta.set_committed_payload_end(super::os::SEGMENT);
    #[cfg(all(
        feature = "alloc-lazy-commit",
        not(feature = "numa-aware"),
        windows,
        not(miri)
    ))]
    meta.set_committed_payload_end(meta_end + LAZY_FIRST_CHUNK);
    #[cfg(all(
        feature = "alloc-lazy-commit",
        not(feature = "numa-aware"),
        any(not(windows), miri)
    ))]
    meta.set_committed_payload_end(super::os::SEGMENT);

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
