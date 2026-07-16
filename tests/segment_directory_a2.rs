//! R7-A2 correctness tests: the incrementally-maintained directory bitmap must
//! EXACTLY equal a fresh rebuild (`dbg_rebuild_directory`) at every observation
//! point.
//!
//! Feature-gated behind `alloc-segment-directory`.

#![cfg(feature = "alloc-segment-directory")]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

// ── helpers ──────────────────────────────────────────────────────────────

const SMALL_CLASS_COUNT: usize = {
    // Mirror what the crate uses; available via the dbg accessor.
    // Verified at the start of each test that needs it.
    49
};

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

/// A single block freed into an empty segment sets the directory bit.
/// After the free, the incremental bitmap must equal a fresh rebuild.
#[test]
fn single_free_sets_bit() {
    let mut core = AllocCore::new().unwrap();
    assert_eq!(
        AllocCore::dbg_small_class_count(),
        SMALL_CLASS_COUNT,
        "SMALL_CLASS_COUNT assumption"
    );

    let (mut ptrs, _class_idx) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // All blocks are allocated (consumed from the free list by alloc). The
    // directory should show all zeros (no free blocks).
    // Free ONE block -- this pushes it onto its segment's BinTable for
    // `class_idx`. If old_head was NULL (empty list), the directory bit
    // for that (class, slot) should now be SET.
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let p = ptrs.pop().unwrap();
    unsafe { core.dealloc(p, layout) };

    // The incremental directory must match a fresh rebuild.
    assert_directory_equals_rebuild(&mut core);
}

/// Pop the LAST block from a segment's class free list clears the bit.
#[test]
fn last_pop_clears_bit() {
    let mut core = AllocCore::new().unwrap();

    let (mut ptrs, _class_idx) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Free one block to create a non-empty free list.
    let p = ptrs.pop().unwrap();
    unsafe { core.dealloc(p, layout) };
    assert_directory_equals_rebuild(&mut core);

    // Now allocate that block back (pop from the free list). If it was the
    // ONLY block on the list, the directory bit should now be cleared.
    let q = core.alloc(layout);
    assert!(!q.is_null());
    ptrs.push(q);

    assert_directory_equals_rebuild(&mut core);
}

/// Multiple classes in one segment: freeing blocks of different classes into
/// the same segment sets independent bits.
#[test]
fn multiple_classes_one_segment() {
    let mut core = AllocCore::new().unwrap();

    // Use two different small sizes that map to different classes.
    let size_a = 16;
    let size_b = 64;
    let class_a = SegmentLayout::class_for(size_a, 1).unwrap();
    let class_b = SegmentLayout::class_for(size_b, 1).unwrap();
    assert_ne!(class_a, class_b, "need two different classes");

    let layout_a = Layout::from_size_align(size_a, 1).unwrap();
    let layout_b = Layout::from_size_align(size_b, 1).unwrap();

    // Push past threshold using SMALL_MAX (big blocks that fill segments
    // quickly). Small blocks (16 B) would need millions of allocs per
    // segment and the loop would never cross the threshold.
    let (_base_ptrs, _) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Now allocate some class A and class B blocks.
    let mut ptrs_a: Vec<*mut u8> = Vec::new();
    for _ in 0..20 {
        let p = core.alloc(layout_a);
        assert!(!p.is_null());
        ptrs_a.push(p);
    }
    let mut ptrs_b: Vec<*mut u8> = Vec::new();
    for _ in 0..10 {
        let p = core.alloc(layout_b);
        assert!(!p.is_null());
        ptrs_b.push(p);
    }

    // Free one class A and one class B.
    let pa = ptrs_a.pop().unwrap();
    unsafe { core.dealloc(pa, layout_a) };

    let pb = ptrs_b.pop().unwrap();
    unsafe { core.dealloc(pb, layout_b) };

    assert_directory_equals_rebuild(&mut core);
}

/// A segment recycled then reused with a different base must NOT inherit
/// stale bits from the old segment lifetime.
#[test]
#[cfg(feature = "alloc-decommit")]
fn recycled_segment_no_stale_bits() {
    use sefer_alloc::SmallSegmentPoolConfig;

    // Disable the pool so segments are released (recycled) immediately
    // when they empty.
    let pool_cfg = SmallSegmentPoolConfig::new().pool_segments(0);
    let cfg = sefer_alloc::LargeCacheConfig::new().pool(pool_cfg);
    let mut core = AllocCore::new_with_config(cfg).unwrap();

    let (mut ptrs, _class_idx) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Free ALL blocks to empty every segment (they will be recycled since
    // pool_cap == 0). Track which segment bases we are freeing from.
    while let Some(p) = ptrs.pop() {
        unsafe { core.dealloc(p, layout) };
    }

    // After freeing everything, the directory should show all zeros
    // (all slots recycled or the primordial is empty/carve-only).
    assert_directory_equals_rebuild(&mut core);

    // Now allocate again -- this may reuse recycled slot indices.
    let mut new_ptrs: Vec<*mut u8> = Vec::new();
    for _ in 0..200 {
        let p = core.alloc(layout);
        if p.is_null() {
            break;
        }
        new_ptrs.push(p);
    }

    // Free some of the new blocks.
    let n_free = new_ptrs.len() / 3;
    for _ in 0..n_free {
        let p = new_ptrs.pop().unwrap();
        unsafe { core.dealloc(p, layout) };
    }

    // The directory must still be consistent.
    assert_directory_equals_rebuild(&mut core);
}

/// Randomized alloc/free workload: after each batch of operations, the live
/// directory bitmap must equal a fresh rebuild. This is the STRONGEST test
/// of A2 wiring correctness.
#[test]
fn randomized_workload_incremental_equals_rebuild() {
    let mut core = AllocCore::new().unwrap();

    let sizes: &[usize] = &[16, 32, 64, 128, 256, 512, 1024, 2048];
    let layouts: Vec<Layout> = sizes
        .iter()
        .map(|&s| Layout::from_size_align(s, 1).unwrap())
        .collect();

    let mut live: Vec<(*mut u8, Layout)> = Vec::new();

    // Phase 1: allocate enough to cross the threshold.
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let big_layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let max_allocs = (threshold as usize + 5) * 20;
    for _ in 0..max_allocs {
        let p = core.alloc(big_layout);
        assert!(!p.is_null());
        live.push((p, big_layout));
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(core.dbg_directory_is_materialised());

    // Phase 2: mixed alloc/free workload using a simple deterministic
    // pseudo-random sequence (LCG).
    let mut rng: u64 = 0xDEAD_BEEF_CAFE_1234;
    let lcg = |state: &mut u64| -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *state >> 33
    };

    const OPS: usize = 500;
    for _ in 0..OPS {
        let r = lcg(&mut rng);
        if live.is_empty() || r % 3 != 0 {
            // Allocate.
            let idx = (lcg(&mut rng) as usize) % layouts.len();
            let layout = layouts[idx];
            let p = core.alloc(layout);
            if !p.is_null() {
                live.push((p, layout));
            }
        } else {
            // Free a random live block.
            let idx = (lcg(&mut rng) as usize) % live.len();
            let (p, layout) = live.swap_remove(idx);
            unsafe { core.dealloc(p, layout) };
        }
    }

    // After the workload, the incremental bitmap must match a fresh rebuild.
    assert_directory_equals_rebuild(&mut core);
}

/// Batch flush (magazine flush_class) path: the incremental bitmap must
/// stay correct after a batch of frees via flush_class.
#[test]
fn flush_class_maintains_directory() {
    let mut core = AllocCore::new().unwrap();

    let size = 64;
    let _class_idx = SegmentLayout::class_for(size, 1).unwrap();
    let layout = Layout::from_size_align(size, 1).unwrap();

    // Push past threshold.
    let (_ptrs, _) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Allocate a batch of blocks to later flush.
    let mut batch: Vec<*mut u8> = Vec::new();
    for _ in 0..32 {
        let p = core.alloc(layout);
        if p.is_null() {
            break;
        }
        batch.push(p);
    }
    assert!(!batch.is_empty());

    // Flush the batch via flush_class (the magazine batch-free path).
    // We need the class index for the flush; re-derive it from size.
    let flush_class = SegmentLayout::class_for(size, 1).unwrap();
    // SAFETY: all pointers were returned by alloc with the same layout.
    unsafe { core.flush_class(flush_class, &batch) };

    // The directory must match a fresh rebuild.
    assert_directory_equals_rebuild(&mut core);
}

/// Drain-freelist-batch path: the incremental bitmap must stay correct
/// after a batch drain empties a class's free list.
#[test]
fn drain_freelist_batch_maintains_directory() {
    let mut core = AllocCore::new().unwrap();

    let size = 64;
    let _class_idx = SegmentLayout::class_for(size, 1).unwrap();
    let layout = Layout::from_size_align(size, 1).unwrap();

    // Push past threshold.
    let (_ptrs, _) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Allocate and free blocks to create a non-empty free list.
    let mut to_free: Vec<*mut u8> = Vec::new();
    for _ in 0..10 {
        let p = core.alloc(layout);
        if p.is_null() {
            break;
        }
        to_free.push(p);
    }
    for &p in &to_free {
        unsafe { core.dealloc(p, layout) };
    }

    // The free list for class_idx should be non-empty now.
    assert_directory_equals_rebuild(&mut core);

    // Drain via alloc (which calls pop_free/drain_freelist_batch).
    let mut drained: Vec<*mut u8> = Vec::new();
    for _ in 0..to_free.len() + 5 {
        let p = core.alloc(layout);
        if p.is_null() {
            break;
        }
        drained.push(p);
    }

    // The directory must still match after the drain.
    assert_directory_equals_rebuild(&mut core);
}

/// Randomized workload with PERIODIC assertions: after every N ops (not
/// just at the end), the incremental bitmap must match a rebuild. This
/// catches mid-workload drift that a final-only check would miss.
#[test]
fn randomized_workload_periodic_assertions() {
    let mut core = AllocCore::new().unwrap();

    let sizes: &[usize] = &[16, 64, 256, 1024, 2048];
    let layouts: Vec<Layout> = sizes
        .iter()
        .map(|&s| Layout::from_size_align(s, 1).unwrap())
        .collect();

    let mut live: Vec<(*mut u8, Layout)> = Vec::new();

    // Phase 1: cross the threshold.
    let (base_ptrs, _) = push_past_threshold(&mut core);
    for p in &base_ptrs {
        let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
        live.push((*p, layout));
    }
    assert!(core.dbg_directory_is_materialised());

    // Phase 2: 300 ops, asserting every 30.
    let mut rng: u64 = 0x1234_5678_ABCD_EF01;
    let lcg = |state: &mut u64| -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *state >> 33
    };

    for op in 0..300 {
        let r = lcg(&mut rng);
        if live.is_empty() || r % 3 != 0 {
            let idx = (lcg(&mut rng) as usize) % layouts.len();
            let layout = layouts[idx];
            let p = core.alloc(layout);
            if !p.is_null() {
                live.push((p, layout));
            }
        } else {
            let idx = (lcg(&mut rng) as usize) % live.len();
            let (p, layout) = live.swap_remove(idx);
            unsafe { core.dealloc(p, layout) };
        }

        // Assert periodically.
        if (op + 1) % 30 == 0 {
            assert_directory_equals_rebuild(&mut core);
        }
    }

    // Final assertion.
    assert_directory_equals_rebuild(&mut core);
}

/// Pool/unpool cycle: a pooled segment retains correct directory bits, and
/// re-use via find_segment_with_free does not corrupt the bitmap.
#[test]
#[cfg(feature = "alloc-decommit")]
fn pool_unpool_maintains_directory() {
    use sefer_alloc::SmallSegmentPoolConfig;

    // Pool cap = 4 (default-ish): segments are pooled before being released.
    let pool_cfg = SmallSegmentPoolConfig::new().pool_segments(4);
    let cfg = sefer_alloc::LargeCacheConfig::new().pool(pool_cfg);
    let mut core = AllocCore::new_with_config(cfg).unwrap();

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Push past threshold.
    let (mut ptrs, _) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Free ALL blocks -- segments will be pooled (not released immediately).
    while let Some(p) = ptrs.pop() {
        unsafe { core.dealloc(p, layout) };
    }
    assert_directory_equals_rebuild(&mut core);

    // Allocate again -- reuses pooled segments via find_segment_with_free.
    let mut ptrs2: Vec<*mut u8> = Vec::new();
    for _ in 0..100 {
        let p = core.alloc(layout);
        if p.is_null() {
            break;
        }
        ptrs2.push(p);
    }
    assert_directory_equals_rebuild(&mut core);

    // Free half.
    let n_free = ptrs2.len() / 2;
    for _ in 0..n_free {
        let p = ptrs2.pop().unwrap();
        unsafe { core.dealloc(p, layout) };
    }
    assert_directory_equals_rebuild(&mut core);
}
