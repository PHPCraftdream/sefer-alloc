//! R8-1 (task #214) correctness test — incremental directory sync across a
//! multi-class ring-drain pass.
//!
//! The incremental directory sync (`sync_directory_for_segment_classes`) reads
//! a `u64` bitmask of classes a drain pass touched (accumulated in the drain
//! closure via `changed_classes |= 1u64 << entry_class_idx(off)`) instead of
//! re-sweeping all `SMALL_CLASS_COUNT` classes. The load-bearing scenario that
//! bitmask MUST get right — and that NO pre-existing directory test exercises —
//! is a SINGLE `dbg_drain_all_rings` call that reclaims blocks of TWO OR MORE
//! DIFFERENT classes into the SAME segment. The existing tests only ever touch
//! the directory via the own-thread `dealloc`/`flush_class` paths, which update
//! the directory immediately at the mutation site (never through this
//! ring-drain-then-sync path with several classes in flight at once); a buggy
//! implementation that tracked only the LAST drained class instead of
//! accumulating the full bitmask would pass every existing test and fail here.
//!
//! ## Why the free lists are pre-drained
//!
//! `carve_block_with_refill` (the non-magazine alloc path) carves a refill
//! batch of 31 extra blocks and immediately `dealloc_small`s them, which calls
//! `publish_nonempty` and SETS the directory bit for the carved class — so a
//! freshly-allocated-to class already has its bit set before this test's ring
//! drain runs. That would MASK the bug (the buggy sync's failure to set a
//! class's bit is invisible when the bit is already set). To make the
//! clear→set transition observable, each target class's free list is fully
//! drained first via the test-only `dbg_drain_freelist_batch` (which calls
//! `publish_empty` when it exhausts the chain, CLEARING the bit), then ONE
//! drained block of each class is pushed to the ring and reclaimed by the
//! drain — an empty→non-empty BinTable transition the sync MUST publish.
//!
//! Deterministic and single-threaded: the cross-thread free is SIMULATED via
//! the `dbg_push_to_ring` producer hook (the exact ring-producer side of a real
//! cross-thread free, without touching the BinTable/directory yet), so the test
//! needs no OS threads.
//!
//! Feature-gated behind `alloc-xthread` (the ring/drain path) PLUS
//! `alloc-segment-directory` (the directory sync under test).

#![cfg(all(feature = "alloc-xthread", feature = "alloc-segment-directory"))]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

// ── helpers (copied verbatim from segment_directory_a2.rs — the established
//    correctness oracle in this repo) ─────────────────────────────────────

/// Allocate until `table.count() > threshold`, returning pointers + class.
fn push_past_threshold(core: &mut AllocCore) -> (Vec<*mut u8>, usize) {
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let small_max = SegmentLayout::SMALL_MAX;
    let layout = Layout::from_size_align(small_max, 1).unwrap();
    let class_idx =
        SegmentLayout::class_for(small_max, 1).expect("SMALL_MAX must resolve to a small class");

    let mut ptrs: Vec<*mut u8> = Vec::new();
    let max_allocs = (threshold as usize + 5) * 20;
    for _ in 0..max_allocs {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "alloc returned null");
        ptrs.push(p);
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(
        core.dbg_table_count() > threshold,
        "failed to push table count past threshold"
    );
    (ptrs, class_idx)
}

/// Assert that the incremental directory bitmap equals a fresh rebuild for
/// ALL (class, slot) pairs. Panics with a detailed message on mismatch.
fn assert_directory_equals_rebuild(core: &mut AllocCore) {
    // Take a snapshot of the current incremental directory.
    let class_count = AllocCore::dbg_small_class_count();
    let mut incremental = vec![vec![false; 1024]; class_count];
    for (c, row) in incremental.iter_mut().enumerate() {
        for (s, cell) in row.iter_mut().enumerate() {
            *cell = core.dbg_directory_get_bit(c, s).unwrap_or(false);
        }
    }

    // Rebuild from scratch.
    let rebuilt = core.dbg_rebuild_directory();
    assert!(
        rebuilt,
        "directory should be materialised for this assertion"
    );

    // Compare.
    for (c, row) in incremental.iter().enumerate() {
        for (s, &inc_val) in row.iter().enumerate() {
            let fresh = core.dbg_directory_get_bit(c, s).unwrap_or(false);
            assert_eq!(
                inc_val, fresh,
                "directory mismatch at class={c} slot={s}: \
                 incremental={inc_val}, rebuild={fresh}",
            );
        }
    }
}

// ── tests ────────────────────────────────────────────────────────────────

/// The load-bearing R8-1 scenario: ONE ring-drain pass that reclaims blocks of
/// TWO different small classes into the SAME segment must set BOTH classes'
/// directory bits. The `changed_classes` bitmask accumulated in the drain
/// closure must OR in every distinct class — an implementation that overwrote
/// it with only the last drained class would leave the other class's bit stale
/// (clear), diverging from a fresh rebuild.
#[test]
fn multi_class_drain_sets_all_class_bits() {
    let mut core = AllocCore::new().unwrap();

    // Materialise the directory (the A2 oracle requires it).
    let (_threshold_ptrs, _) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Two distinct small classes (same pair segment_directory_a2's
    // `multiple_classes_one_segment` uses).
    let size_a = 16;
    let size_b = 64;
    let class_a = SegmentLayout::class_for(size_a, 1).unwrap();
    let class_b = SegmentLayout::class_for(size_b, 1).unwrap();
    assert_ne!(class_a, class_b, "need two different classes");

    let layout_a = Layout::from_size_align(size_a, 1).unwrap();
    let layout_b = Layout::from_size_align(size_b, 1).unwrap();

    // Carve a few blocks of both classes. A segment has ONE shared bump
    // cursor, so back-to-back carves land blocks of different classes in the
    // SAME segment — confirm this against ground truth rather than assuming it.
    let mut ptrs_a: Vec<*mut u8> = Vec::new();
    for _ in 0..4 {
        let p = core.alloc(layout_a);
        assert!(!p.is_null());
        ptrs_a.push(p);
    }
    let mut ptrs_b: Vec<*mut u8> = Vec::new();
    for _ in 0..4 {
        let p = core.alloc(layout_b);
        assert!(!p.is_null());
        ptrs_b.push(p);
    }

    let base_of = |p: *mut u8| SegmentLayout::segment_base_of(p as usize);
    let target_base = ptrs_a
        .iter()
        .map(|&p| base_of(p))
        .find(|&ba| ptrs_b.iter().any(|&p| base_of(p) == ba))
        .expect("expected at least one segment to host both a 16 B and a 64 B block");
    // Any ptr in the target segment identifies it for the per-segment drain
    // hooks below (`dbg_drain_freelist_batch` derives the base from the ptr).
    let seg_ptr_a = *ptrs_a.iter().find(|&&p| base_of(p) == target_base).unwrap();
    let seg_ptr_b = *ptrs_b.iter().find(|&&p| base_of(p) == target_base).unwrap();
    assert_eq!(
        base_of(seg_ptr_a),
        base_of(seg_ptr_b),
        "the two ptrs must identify the same segment for this scenario to be meaningful",
    );

    // Fully drain BOTH classes' free lists in the target segment. The carve
    // refill (`carve_block_with_refill`, REFILL_BATCH = 31) `dealloc_small`s a
    // batch of extra blocks on every carve, so these free lists are non-empty
    // and their directory bits are SET. Draining them exhausts the chain →
    // `drain_freelist_batch` calls `publish_empty` → bits CLEARED. This is the
    // setup that makes the post-drain empty→non-empty transition OBSERVABLE:
    // the ring-drain reclaims one block of each class back onto an EMPTY free
    // list, so the correct sync must set the bit (clear→set) and the buggy
    // sync (last-class-only) leaves it stale (clear) — a mismatch the oracle
    // catches.
    //
    // The drained blocks are handed out (marked allocated); we re-push ONE per
    // class to the ring below.
    let mut drained_a: Vec<*mut u8> = vec![std::ptr::null_mut(); 256];
    let mut drained_b: Vec<*mut u8> = vec![std::ptr::null_mut(); 256];
    // SAFETY: `seg_ptr_a`/`seg_ptr_b` are valid live allocation pointers into a
    // segment owned by `core` (returned by `core.alloc`). The callee derives
    // the segment base and mutates only that segment's free list.
    let n_a = unsafe { core.dbg_drain_freelist_batch(seg_ptr_a, class_a, &mut drained_a) };
    let n_b = unsafe { core.dbg_drain_freelist_batch(seg_ptr_b, class_b, &mut drained_b) };
    assert!(
        n_a >= 1,
        "target segment must have ≥ 1 free class_a block to drain (carve refill)"
    );
    assert!(
        n_b >= 1,
        "target segment must have ≥ 1 free class_b block to drain (carve refill)"
    );

    // Simulate a cross-thread free of ONE drained block of EACH class,
    // targeting the SAME segment, WITHOUT touching the BinTable/directory yet
    // — exactly the ring-producer side of a real cross-thread free.
    // SAFETY (per `dbg_push_to_ring`'s contract): each pushed ptr is treated as
    // logically freed by this push and is not touched again until the drain
    // below consumes the notes (no intervening dealloc/alloc/re-issue of these
    // specific addresses).
    let pushed_a = unsafe { core.dbg_push_to_ring(drained_a[0], class_a) };
    assert!(pushed_a, "dbg_push_to_ring for class_a must succeed");
    let pushed_b = unsafe { core.dbg_push_to_ring(drained_b[0], class_b) };
    assert!(pushed_b, "dbg_push_to_ring for class_b must succeed");

    // Drain — exercises `dbg_drain_all_rings_impl`'s
    // `sync_directory_for_segment_classes` call (call site #4 of the R8-1
    // wiring). The two pushed notes (one per class) are consumed in a SINGLE
    // drain pass for the target segment.
    core.dbg_drain_all_rings();

    // The oracle: the incrementally-maintained directory must EXACTLY equal a
    // fresh rebuild. Both classes' bits for the target segment must be set
    // (each reclaim created an empty→non-empty transition). This is the
    // assertion that FAILS if `changed_classes` overwrote instead of OR-ing.
    assert_directory_equals_rebuild(&mut core);
}
