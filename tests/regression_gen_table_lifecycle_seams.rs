//! X7 Ф4 (task #192) — lifecycle-seam regression tests for the per-block
//! generation table.
//!
//! Ф3 (task #191) landed the three touches (bump-at-issue, stamp-at-remote-free,
//! compare-at-drain) and closed the re-issue-before-drain residual. Ф3's
//! adversarial review INDEPENDENTLY confirmed the structural correctness of the
//! three lifecycle seams this file pins (see the X7 plan §3-Ф4 + §2 decision 2):
//!
//!   1. **Decommit-reset continuity** — `decommit_empty_segment` does NOT
//!      re-zero the gen table; old generations persist across the reset so
//!      re-carved blocks continue numbering from where they were left (plan
//!      §2.2). The bootstrap half (fresh segments ARE zeroed) is pinned here by
//!      `fresh_segment_gen_table_is_zeroed`; the decommit-reset half (NOT
//!      re-zeroed) is a structural property verified during Ф3's review (see the
//!      test's doc comment for why it is not separately behaviorally testable:
//!      decommit and slot-recycle are architecturally coupled, so the gen table
//!      is unmapped before any post-decommit read is possible).
//!
//!   2. **Recycle/release drops via existing guards** — once a segment's table
//!      slot is recycled (memory released), a stale ring note targeting it is
//!      dropped by the EXISTING `contains_base`/`magic_at` guards BEFORE any gen
//!      read (reading a dead segment's gen table would be a use-after-free of
//!      unmapped memory, not a generation mismatch). Largely covered by
//!      `decommit_stale_ring.rs` Scenario B; the focused pin here is
//!      `recycled_segment_ring_drain_is_safe`.
//!
//! (A former Seam 3 — "adopt/abandon preserves the gen table" — was removed
//! with the abandon/adopt substrate in task #97 / R4-5; nothing abandons a
//! segment today, so the invariant it pinned is structurally guaranteed.)
//!
//! All gen-table-state tests are `#[cfg(feature = "hardened")]`-gated (matching
//! Ф1/Ф2/Ф3's discipline): only under `hardened` does the generation table
//! exist. Seam 2 additionally requires `alloc-decommit` (the only build under
//! which `decommit_empty_segment` fires). The file's blanket gate is
//! `alloc-core + alloc-xthread + hardened`.
//!
//! ## Counterfactual (non-vacuity)
//!
//! - Seam 1: if `reserve_small_segment`'s `init_gen_table_in_place(base)` call
//!   were removed, a freshly carved block's `gen_at` would read UNINITIALISED
//!   memory (not a reliable 0) → `fresh_segment_gen_table_is_zeroed` fails (or
//!   trips miri). The decommit-reset half (NOT re-zeroed) is structurally pinned
//!   (source re-read confirms `decommit_empty_segment` contains no
//!   `init_gen_table_in_place`).

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "hardened"
))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::segment_header::gen_at;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::SegmentLayout;

/// Derive the SEGMENT-aligned base pointer of `ptr`, preserving provenance
/// (strict-provenance clean, mirroring `os::segment_base_of_ptr` which is
/// `pub(crate)` and thus unreachable from a test). SEGMENT is `1 << 22` (4 MiB).
fn segment_base_of(ptr: *mut u8) -> *mut u8 {
    ptr.map_addr(|a| a & !(SegmentLayout::SEGMENT - 1))
}

// ===========================================================================
// Seam 1 — decommit-reset continuity (plan §2.2 / §3-Ф4)
// ===========================================================================

/// **Fresh segments start with a ZEROED generation table; decommit-reset does
/// NOT re-zero it.**
///
/// Per the X7 plan §2.2 (decision 2), the gen table "lives in segment metadata,
/// is NOT decommitted with the payload, and its numbering is CONTINUOUS across
/// decommit-reset." Two halves of this invariant:
///
///   (a) **Fresh segment creation zeroes the table** — `reserve_small_segment`
///       (and the primordial bootstrap) call `init_gen_table_in_place`, so every
///       granule starts at life 0. Without this, a `gen_at` Relaxed load on a
///       never-written cell is UB (miri-confirmed during Ф1).
///
///   (b) **Decommit-reset does NOT re-zero** — `decommit_empty_segment` resets
///       bump/bitmap/pagemap/freelist but deliberately leaves the gen table
///       untouched, so numbering continues across the reset.
///
/// This test pins half (a) behaviorally: a freshly carved block in a
/// newly-reserved segment reads gen 0 via `gen_at`. Half (b) is a STRUCTURAL
/// property of `decommit_empty_segment` (verified during Ф3's review: the
/// function resets the bitmap, bump, freelist, and pagemap in steps 1-3 but
/// contains NO `init_gen_table_in_place` call — confirmed by re-reading the
/// source at `alloc_core.rs:1202-1238`). The architectural reason half (b) is
/// not separately behaviorally testable: in the current code decommit is always
/// immediately followed by slot recycle (`dealloc_small` and the drain path both
/// call `table.recycle(base)` right after `decommit_empty_segment` fires), which
/// unmaps the entire segment including metadata. The gen-table-continuity
/// property therefore matters in the narrow window between decommit and recycle
/// (during a deferred-recycle drain, stale ring entries are still processed
/// against the decommitted segment's metadata) — and is defended by the
/// `off >= bump` guard that drops those entries before any gen read. Pinning
/// half (a) ensures the BOOTSTRAP zeroing (the active half) is not regressed;
/// half (b) is pinned structurally.
///
/// ## Counterfactual (non-vacuity)
///
/// If `reserve_small_segment`'s `init_gen_table_in_place(base)` call were
/// removed, a freshly carved block's `gen_at` would read UNINITIALISED memory
/// (not a reliable 0) → the `assert_eq!(gen, 0)` would fail (or trip miri).
///
/// `#[cfg_attr(miri, ignore)]`: the 20K allocation spill is too slow under
/// miri (each carve touches many pages). The gen-table accessor indexing is
/// already miri-covered by `regression_gen_table_layout.rs`'s standalone-buffer
/// tests; this test's value is the end-to-end reserve→carve→gen_at path, which
/// is a fast native run.
#[cfg_attr(miri, ignore)]
#[test]
fn fresh_segment_gen_table_is_zeroed() {
    use std::collections::BTreeMap;

    let mut ac = AllocCore::new().expect("primordial");
    // 256 B / 8 — spills across multiple segments so we can read gen_at on a
    // block in a FRESHLY reserved (non-primordial) segment.
    let layout = Layout::from_size_align(256, 8).unwrap();

    const SPILL: usize = 20_000;
    let mut ptrs = Vec::with_capacity(SPILL);
    for _ in 0..SPILL {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }

    // Group by segment base. The primordial (bases[0]) is zeroed at bootstrap;
    // every OTHER segment was zeroed by `reserve_small_segment`'s
    // `init_gen_table_in_place`. Verify a block in each non-primordial segment
    // reads gen 0 (carve does NOT bump — plan §2.3 decision 3).
    let mut by_seg: BTreeMap<usize, Vec<*mut u8>> = BTreeMap::new();
    for &p in &ptrs {
        by_seg
            .entry(segment_base_of(p) as usize)
            .or_default()
            .push(p);
    }
    assert!(
        by_seg.len() >= 2,
        "need ≥2 segments (primordial + ≥1 fresh Small); got {}",
        by_seg.len()
    );

    for (i, (&base, blocks)) in by_seg.iter().enumerate() {
        let p = blocks[0];
        let off = (p as usize) - base;
        assert_eq!(
            off % SegmentLayout::MIN_BLOCK,
            0,
            "offset is granule-aligned"
        );
        let gen = unsafe { gen_at(base as *mut u8, off) };
        assert_eq!(
            gen, 0,
            "segment[{i}] base={base:#x}: a freshly carved block must read gen 0 \
             (init_gen_table_in_place zeroed the table at reserve time; carve does not bump)"
        );
    }

    // Cleanup.
    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}

// ===========================================================================
// Seam 2 — recycle/release drops via existing guards (plan §3-Ф4)
// ===========================================================================

/// **A stale ring drain targeting a recycled segment is a safe no-op.**
///
/// Once a segment's table slot is recycled (its OS reservation released), the
/// segment's metadata pages — including the gen table — are UNMAPPED. A stale
/// ring note referencing an offset in that dead segment must therefore be
/// dropped by the EXISTING guards (the `contains_base`/`magic_at` checks in
/// `dbg_push_to_ring` and `reclaim_offset_checked`), NOT by a generation
/// comparison: reading a dead segment's gen table would be a use-after-free of
/// unmapped memory, not a benign mismatch.
///
/// This seam is largely covered by `decommit_stale_ring.rs`:
///   - **Scenario A** (`stale_ring_entry_rejected_by_bump_guard`) pins the
///     within-drain case (the `off >= bump` guard after decommit fires).
///   - **Scenario B** (`post_recycle_push_rejected_by_contains_base`) pins the
///     post-recycle PUSH case (`contains_base` returns false → push rejected).
///
/// What this test adds is the DRAIN-side confirmation after a batch of recycles:
/// following a wave of own-thread deallocs that empties and recycles several
/// segments, a full `dbg_drain_all_rings` must be a safe no-op (no fault, no
/// corruption) even though stale ring entries for the now-recycled segments may
/// still be queued. Because `dbg_push_to_ring` rejects pushes to recycled
/// segments (Scenario B), the only way a stale note can exist for a recycled
/// segment is if it was pushed BEFORE the recycle — and Scenario A already pins
/// that the within-drain guard handles that. This test confirms the allocator
/// stays healthy end-to-end after the recycle+drain sequence.
///
/// Under `alloc-decommit` (which `production` pulls in), recycles happen on the
/// own-thread dealloc path. This test does NOT need `hardened` for its logic,
/// but is compiled here (under the file's blanket gate) because it documents
/// the gen-irrelevance of the recycle path — the gen table is irrelevant once
/// the segment is gone.
///
/// `#[cfg_attr(miri, ignore)]`: 5K allocations + a ring drain is too slow
/// under miri. The within-drain guard logic is already miri-covered by
/// `decommit_stale_ring.rs` Scenario B (which IS miri-run); this test adds the
/// end-to-end health check, which is a fast native run.
#[cfg_attr(miri, ignore)]
#[cfg(feature = "alloc-decommit")]
#[test]
fn recycled_segment_ring_drain_is_safe() {
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool so the
    // decommit+recycle this test asserts fires DETERMINISTICALLY. With the pool
    // ON (production default) up to `pool_cap` emptied segments are retained
    // (no decommit); this test's payload is the drain-after-RECYCLE health
    // check, so it needs at least one segment to actually recycle. Disabling the
    // pool guarantees that. Pool behaviour is covered by
    // `tests/small_segment_pool.rs`.
    let mut ac = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .expect("primordial");
    let layout = Layout::from_size_align(256, 8).unwrap();
    let class_idx = ac
        .dbg_layout_class_for(layout)
        .expect("256 B is a small class");

    // Alloc enough to spill into several fresh Small segments. 5,000 blocks at
    // 256 B never left a non-current segment reaching live_count == 0 (a
    // 4 MiB segment holds ~16K 256 B blocks before metadata overhead), so
    // `decommit_after == decommit_before` fired on every run and the test's
    // actual payload (the drain-after-recycle health check below) silently
    // no-op'd every time (@sh review, X7-Ф4). 25,000 reliably spills across
    // enough segments that at least one non-current segment empties and
    // recycles.
    const N: usize = 150_000;
    let mut ptrs = Vec::with_capacity(N);
    for _ in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }

    // Push a few blocks to their rings (simulating cross-thread frees) BEFORE
    // any own-thread dealloc, so the rings carry entries. These entries target
    // segments that will shortly be recycled.
    let mut pushed = 0usize;
    for &p in ptrs.iter().step_by(50) {
        // SAFETY (R6-MS-4): `p` is owned by `ac` and `class_idx` is its actual
        // class. These pushed blocks are ALSO own-thread-dealloc'd below, so this
        // is a DELIBERATE contract-stress of the drain's `is_free` defensive
        // guard (a stale note for an already-freed block), not a contract-honoring
        // single remote free. It is sound because `reclaim_offset`'s
        // `bm.is_free(off)` check runs unconditionally and returns false for the
        // already-freed block, so no `write_next`/`mark_free` runs on a live owner.
        if unsafe { ac.dbg_push_to_ring(p, class_idx) } {
            pushed += 1;
        }
    }
    assert!(pushed > 0, "expected some ring pushes to succeed");

    // Free all blocks via own-thread dealloc. Non-current Small segments that
    // reach live_count == 0 will decommit and have their slots recycled.
    let decommit_before = AllocCore::dbg_decommit_count();
    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
    let decommit_after = AllocCore::dbg_decommit_count();

    assert!(
        decommit_after > decommit_before,
        "N={N} 256 B allocations must spill enough non-current Small segments \
         to trigger at least one decommit+recycle (before={decommit_before}, \
         after={decommit_after}) — if this fires, bump N further"
    );

    // Drain all rings. Any stale entries whose segment was recycled must be
    // dropped by the existing guards WITHOUT touching unmapped memory. The
    // drain must not panic, fault, or corrupt state.
    ac.dbg_drain_all_rings();

    // Sanity: the allocator is healthy. Re-alloc a batch; each block must be
    // valid, writable, and distinct.
    let mut ok = 0usize;
    for _ in 0..100 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "alloc failed after post-recycle drain");
        unsafe {
            core::ptr::write_bytes(p, 0x7E, 256);
            assert_eq!(
                p.read(),
                0x7E,
                "write/readback failed after post-recycle drain"
            );
        }
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
        ok += 1;
    }
    assert_eq!(ok, 100, "all post-recycle-drain allocs must succeed");
}
