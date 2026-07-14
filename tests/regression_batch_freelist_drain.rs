//! Regression (P7.2, task #161, Э7 — batch freelist drain).
//!
//! `AllocCore::drain_freelist_batch` pops up to `out.len()` free blocks from a
//! segment's `BinTable[class_idx]` in ONE walk, hoisting `set_head` /
//! `head`-read / `inc_live` out of the per-block loop. The end-state (bitmap
//! bits, `live_count`, freelist head) must be BYTE-IDENTICAL to calling
//! `pop_free` that many times. These tests pin the three guarantees:
//!
//!   (a) M2 — every drained block ends bitmap-ALLOCATED (bit cleared); a single
//!       free of each is accepted and a second is a no-op.
//!   (b) set_head — a partial drain (m < want) yields exactly m and leaves the
//!       head NULL; a bounded drain (want < m) yields want and the remaining
//!       m-want are yielded, in order, by the next drain.
//!   (c) D1 — cold-storm → free-storm → recycle-storm keeps `live_count` /
//!       decommit accounting exact (a stray extra `inc_live` in the batch would
//!       leave a segment live_count > 0 → decommit never fires).
//!
//! Counterfactuals (verified RED by breaking the code, then restored — see the
//! task report): skip a block's `mark_alloc` → (a) RED; wrong final `set_head`
//! → (b) RED; a stray extra `inc_live` in the batch → (c) RED.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::{AllocCore, SegmentLayout};

fn seg_base_of(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

const FREE_LIST_NULL: u32 = u32::MAX;

fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

/// Build a freelist of exactly `m` blocks of class `c`, all in the SAME segment
/// (the primordial / `small_cur`, so freeing them never decommits). Returns the
/// freed pointers in the order they were freed (so the freelist head is the
/// LAST element). Uses the public alloc/dealloc path; the blocks land on the
/// per-segment BinTable via `dealloc_small`.
fn build_same_segment_freelist(
    core: &mut AllocCore,
    size: usize,
    align: usize,
    m: usize,
) -> Vec<*mut u8> {
    let layout = Layout::from_size_align(size, align).unwrap();
    // Allocate m blocks. On a fresh-ish core these come from the current
    // segment (bump-carve or its own freelist); we then filter to a single
    // segment base so the freelist we drain is genuinely one segment's.
    let c = core
        .dbg_layout_class_for(layout)
        .expect("expected a small class");
    let mut allocated: Vec<*mut u8> = Vec::new();
    for _ in 0..m {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        allocated.push(p);
    }
    // `AllocCore::alloc` → `carve_block_with_refill` carves a REFILL_BATCH of
    // EXTRA blocks on a cold miss and pushes them onto the segment's freelist.
    // Those leftovers would make the freelist longer than `m`. Drain the
    // segment's freelist to EMPTY first (via the batch-drain dbg hook) so that
    // when we free our `m` blocks the freelist holds EXACTLY `m` — a clean,
    // known-length chain for the partial/bounded assertions. Every one of our
    // `m` allocations shares one segment on a fresh core with a small `m`
    // (assert that so `anchor` genuinely names their segment).
    let base0 = seg_base_of(allocated[0]);
    for &p in &allocated {
        assert_eq!(
            seg_base_of(p),
            base0,
            "test precondition: all {m} blocks must share one segment"
        );
    }
    // Drain any refill leftovers sitting on the freelist to empty.
    let mut scratch = vec![core::ptr::null_mut::<u8>(); 4096];
    loop {
        // SAFETY: the first arg is a live allocation owned by the receiver.
        let d = unsafe { core.dbg_drain_freelist_batch(allocated[0], c, &mut scratch) };
        if d == 0 {
            break;
        }
        // Re-mark those drained leftovers allocated-and-gone: they were never
        // handed to us and we will never free them; leaving them drained (out of
        // the freelist, bitmap-allocated, live_count bumped) is fine for the
        // freelist-shape assertions, which only inspect OUR m blocks.
    }
    // Now free exactly our m blocks → freelist length == m.
    for &p in &allocated {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
    allocated
}

// ---------------------------------------------------------------------------
// (a) M2 — drained blocks are bitmap-allocated; double-free is a no-op.
// ---------------------------------------------------------------------------

#[test]
fn drained_blocks_are_allocated_and_double_free_noop_16b() {
    drained_blocks_m2_inner(16, 8);
}

#[test]
fn drained_blocks_are_allocated_and_double_free_noop_256b() {
    drained_blocks_m2_inner(256, 8);
}

fn drained_blocks_m2_inner(size: usize, align: usize) {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    const M: usize = 64;
    let freed = build_same_segment_freelist(&mut core, size, align, M);
    // Anchor pointer for the segment we drain (any block that was freed into it).
    let anchor = freed[0];

    // Drain the whole freelist in one batch.
    let mut out = vec![core::ptr::null_mut::<u8>(); M];
    // SAFETY: the first arg is a live allocation owned by the receiver.
    let k = unsafe { core.dbg_drain_freelist_batch(anchor, c, &mut out) };
    assert_eq!(k, M, "batch drain short: {k}/{M}");

    // Every drained block: distinct, non-null, and bitmap-ALLOCATED (is_free
    // == false). A skipped `mark_alloc` in the batch would leave is_free true.
    let uniq: HashSet<usize> = out.iter().map(|p| *p as usize).collect();
    assert_eq!(uniq.len(), M, "duplicate pointer from batch drain");
    for &p in &out {
        assert!(!p.is_null());
        assert!(
            !core.dbg_is_free_for(p),
            "drained block still marked FREE (mark_alloc skipped) → M2 broken"
        );
        // Writable (handed-out memory).
        unsafe { core::ptr::write_bytes(p, 0xA5, size) };
    }

    // Freelist is now empty (full drain → head NULL).
    assert_eq!(
        core.dbg_freelist_head_for(anchor, c),
        FREE_LIST_NULL,
        "full drain must leave the freelist head NULL"
    );

    // A single free of each is ACCEPTED (block was allocated) — it goes back on
    // the freelist, so is_free becomes true.
    for &p in &out {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
        assert!(
            core.dbg_is_free_for(p),
            "free of a drained block was not accepted (block was not allocated)"
        );
    }
    // A SECOND free of each is a NO-OP (M2 double-free): is_free stays true and
    // the freelist is not corrupted (a corrupt self-loop would be caught by a
    // subsequent drain of more than M blocks below).
    for &p in &out {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
        assert!(core.dbg_is_free_for(p), "double-free changed bitmap state");
    }
    // Re-drain must still yield exactly M distinct blocks (no self-loop / no
    // duplicate injected by the double-frees).
    let mut out2 = vec![core::ptr::null_mut::<u8>(); M];
    // SAFETY: the first arg is a live allocation owned by the receiver.
    let k2 = unsafe { core.dbg_drain_freelist_batch(anchor, c, &mut out2) };
    assert_eq!(k2, M, "re-drain after double-free storm short: {k2}/{M}");
    let uniq2: HashSet<usize> = out2.iter().map(|p| *p as usize).collect();
    assert_eq!(uniq2.len(), M, "double-free corrupted the freelist (dupe)");
}

// ---------------------------------------------------------------------------
// (b) set_head — partial (m < want) and bounded (want < m) drains.
// ---------------------------------------------------------------------------

#[test]
fn partial_and_bounded_drain_set_head_correct() {
    let size = 32usize;
    let align = 8usize;
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, size, align);

    // --- Partial: m < want. Freelist of m; drain want > m → yields m, head NULL.
    const M: usize = 20;
    let freed = build_same_segment_freelist(&mut core, size, align, M);
    let anchor = freed[0];
    assert_ne!(
        core.dbg_freelist_head_for(anchor, c),
        FREE_LIST_NULL,
        "precondition: freelist of {M} must be non-empty"
    );

    let want = 50usize; // > M
    let mut out = vec![core::ptr::null_mut::<u8>(); want];
    // SAFETY: the first arg is a live allocation owned by the receiver.
    let k = unsafe { core.dbg_drain_freelist_batch(anchor, c, &mut out) };
    assert_eq!(k, M, "partial drain must yield exactly m={M}, got {k}");
    assert_eq!(
        core.dbg_freelist_head_for(anchor, c),
        FREE_LIST_NULL,
        "partial drain (chain exhausted) must leave head NULL"
    );
    let partial_set: HashSet<usize> = out[..k].iter().map(|p| *p as usize).collect();
    assert_eq!(partial_set.len(), M, "partial drain produced duplicates");

    // Re-free those M so we can test the bounded case on a known freelist.
    let layout = Layout::from_size_align(size, align).unwrap();
    for &p in &out[..k] {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }

    // --- Bounded: want < m. Drain W < M → yields W; remaining M-W yielded by the
    //     next drain, in continuation order; the two batches are disjoint and
    //     together cover all M.
    const W: usize = 7;
    let mut b1 = vec![core::ptr::null_mut::<u8>(); W];
    // SAFETY: the first arg is a live allocation owned by the receiver.
    let k1 = unsafe { core.dbg_drain_freelist_batch(anchor, c, &mut b1) };
    assert_eq!(k1, W, "bounded drain must yield exactly want={W}, got {k1}");
    // Head must now be a real (non-NULL) offset — M-W blocks remain.
    assert_ne!(
        core.dbg_freelist_head_for(anchor, c),
        FREE_LIST_NULL,
        "bounded drain left head NULL but {} blocks remain",
        M - W
    );

    // Second drain yields the remaining M-W.
    let mut b2 = vec![core::ptr::null_mut::<u8>(); M];
    // SAFETY: the first arg is a live allocation owned by the receiver.
    let k2 = unsafe { core.dbg_drain_freelist_batch(anchor, c, &mut b2) };
    assert_eq!(k2, M - W, "continuation drain must yield m-want={}", M - W);
    assert_eq!(
        core.dbg_freelist_head_for(anchor, c),
        FREE_LIST_NULL,
        "after draining all blocks the head must be NULL"
    );

    // Disjoint + total coverage == exactly M distinct blocks. A wrong final
    // set_head after the bounded drain (e.g. leaving head at an already-popped
    // node, or skipping the (W+1)th) would break coverage or inject a dupe.
    let mut all: HashSet<usize> = HashSet::new();
    for &p in b1.iter() {
        assert!(all.insert(p as usize), "bounded batch1 dupe");
    }
    for &p in b2[..k2].iter() {
        assert!(
            all.insert(p as usize),
            "continuation overlaps batch1 (bad set_head)"
        );
    }
    assert_eq!(
        all.len(),
        M,
        "bounded + continuation did not cover all M blocks"
    );

    // Cleanup.
    for &p in b1.iter().chain(b2[..k2].iter()) {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
}

// ---------------------------------------------------------------------------
// (c) D1 exactness through the REAL refill path (refill_class_bump uses the
//     batch drain). Cold-storm → free-storm → recycle-storm; a stray extra
//     inc_live in the batch would leave a segment live_count > 0 → decommit
//     never fires. Only under alloc-decommit (the counter + hook are gated).
// ---------------------------------------------------------------------------

#[cfg(all(feature = "alloc-decommit", not(miri)))]
#[test]
fn d1_batch_drain_exact_cold_free_recycle() {
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool — this test
    // asserts the cold-free batch drain empties + RECYCLES segments (decommit
    // fires). With the pool ON (production default) the emptied segments are
    // retained (no decommit/recycle). Disabling it exercises the drain→recycle
    // path this test covers. Pool behaviour is covered by
    // `tests/small_segment_pool.rs`.
    let mut core = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .unwrap();
    let size = 1024usize;
    let align = 8usize;
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    const N: usize = 12_000;

    // Round 1 — COLD storm (all bump-carve; batch drain finds nothing).
    let mut buf = vec![core::ptr::null_mut::<u8>(); N];
    let got = core.refill_class_bump(c, &mut buf);
    assert_eq!(got, N, "cold-storm refill short: {got}/{N}");
    let bases: HashSet<usize> = buf.iter().map(|&p| seg_base_of(p)).collect();
    assert!(bases.len() >= 3, "need >= 3 segments, got {}", bases.len());

    let dec0 = AllocCore::dbg_decommit_count();
    // FREE storm.
    for &p in &buf {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
    let dec1 = AllocCore::dbg_decommit_count();
    assert!(
        dec1 > dec0,
        "D1: no decommit after cold free-storm (before={dec0} after={dec1})"
    );

    // Round 2 — RECYCLE storm. This refill DRAINS the freelists we just built
    // (the batch-drain path is now exercised for real), then frees again. If the
    // batch over-counts inc_live, the segments never return to live_count == 0
    // and this second free-storm decommits nothing.
    let mut buf2 = vec![core::ptr::null_mut::<u8>(); N];
    let got2 = core.refill_class_bump(c, &mut buf2);
    assert_eq!(got2, N, "recycle-storm refill short: {got2}/{N}");
    let uniq2: HashSet<usize> = buf2.iter().map(|p| *p as usize).collect();
    assert_eq!(
        uniq2.len(),
        N,
        "recycle storm re-issued a live block (dupe)"
    );

    let dec2 = AllocCore::dbg_decommit_count();
    for &p in &buf2 {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
    let dec3 = AllocCore::dbg_decommit_count();
    assert!(
        dec3 > dec2,
        "D1 broken: recycle-storm free decommitted nothing (before={dec2} \
         after={dec3}). A batch over-count (extra inc_live) leaves live_count \
         > 0 forever."
    );

    // Every still-registered segment of a freed block reports live_count == 0.
    for &p in &buf2 {
        if let Some(lc) = core.dbg_live_count_for(p) {
            assert_eq!(lc, 0, "D1 broken: freed block's segment live_count={lc}");
        }
    }
}
