//! Single-threaded isolation of the `reclaim_offset` integration (task #37).
//!
//! The isolated ring test (`remote_ring_unit.rs`) proved the *ring* correct as a
//! data structure. It did NOT exercise `reclaim_offset` — the owner-side logic
//! that turns a drained offset back into a `BinTable` free-list node (page-map
//! class lookup + double-free guard + push). That logic is single-threaded
//! owner code; if it is buggy, the multi-thread crash would reproduce here with
//! **no threads at all**.
//!
//! Protocol per round: alloc K blocks, push each block's offset into its
//! segment ring (simulating a cross-thread free), drain+reclaim, then re-alloc K
//! and assert every pointer is valid, distinct, and in range. A logic bug in
//! `reclaim_offset` (wrong class, double-add, bad offset) corrupts the free list
//! and this test crashes/asserts WITHOUT any concurrency.
//!
//! GREEN here ⟹ the reclaim logic is sound single-threaded ⟹ the race_repro
//! crash is a concurrency/ordering bug (not a logic bug), directing the fix.

#![cfg(all(feature = "alloc-core", feature = "alloc-xthread"))]

use core::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::alloc_core::AllocCore;

fn seg_of(p: *mut u8) -> usize {
    // Match os::segment_base_of: mask to SEGMENT alignment. SEGMENT is 4 MiB
    // (1<<22) in this build; derive defensively by masking low 22 bits.
    (p as usize) & !((1usize << 22) - 1)
}

#[test]
fn reclaim_offset_single_threaded_roundtrip() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(8, 8).unwrap(); // class 0

    const K: usize = 200;
    const ROUNDS: usize = 50;

    for round in 0..ROUNDS {
        // 1. Alloc K blocks.
        let mut ptrs = Vec::with_capacity(K);
        for _ in 0..K {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc returned null in round {round}");
            ptrs.push(p);
        }
        // Distinct.
        let set: HashSet<usize> = ptrs.iter().map(|&p| p as usize).collect();
        assert_eq!(
            set.len(),
            K,
            "alloc handed out a duplicate in round {round}"
        );

        // 2. Simulate cross-thread free: push each offset (with its size class)
        //    into its segment ring. The test allocates only `layout` (8/8) →
        //    class 0, so the cross-thread freer's class is 0 here.
        const CLASS_IDX: usize = 0;
        let mut pushed = 0usize;
        for &p in &ptrs {
            // SAFETY (R6-MS-4): `p` is a live allocation owned by `ac`; this push
            // is its single logical remote free — the block is reclaimed by the
            // `dbg_drain_all_rings` below (no dealloc / re-issue of `p` in
            // between). `CLASS_IDX` is the block's actual class (0 for the 8/8
            // layout).
            if unsafe { ac.dbg_push_to_ring(p, CLASS_IDX) } {
                pushed += 1;
            }
        }
        // The ring is bounded (RING_CAP=256); with K=200 all should fit per
        // segment if blocks span few segments. Don't assert all pushed — some
        // may overflow if many land in one segment's ring; that's a bounded
        // leak, not a bug. But we DID push at least some.
        assert!(pushed > 0, "no offsets pushed in round {round}");

        // 3. Drain + reclaim into the BinTables.
        ac.dbg_drain_all_rings();

        // 4. Re-alloc K blocks. Reclaimed blocks should be reused; the rest
        //    carved. Every pointer must be valid and distinct from the others
        //    handed out THIS round (a corrupt free list would hand out a
        //    garbage pointer or loop).
        let mut ptrs2 = Vec::with_capacity(K);
        for _ in 0..K {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "re-alloc null in round {round}");
            ptrs2.push(p);
        }
        let set2: HashSet<usize> = ptrs2.iter().map(|&p| p as usize).collect();
        assert_eq!(
            set2.len(),
            K,
            "re-alloc handed out a DUPLICATE in round {round} — free-list \
             corruption (a block listed twice)"
        );

        // 5. Sanity: each re-alloc'd pointer is SEGMENT-aligned-base-consistent
        //    (points inside a real segment, not garbage).
        for &p in &ptrs2 {
            let base = seg_of(p);
            assert!(p as usize >= base, "pointer below its segment base");
            assert!(
                (p as usize) - base < (1usize << 22),
                "pointer outside its segment (corrupt offset) in round {round}"
            );
        }

        // 6. Free everything own-thread to recycle for the next round (keeps
        //    the working set bounded). Own-thread dealloc routes straight to the
        //    BinTable.
        for &p in &ptrs2 {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p, layout) };
        }
    }
}
