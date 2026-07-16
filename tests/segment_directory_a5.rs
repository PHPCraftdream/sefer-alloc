//! R7-A5 correctness gate — fills the gaps in the A1–A4 test matrix.
//!
//! ## Scenarios covered here (not covered by A1–A4)
//!
//! 1. `decommit_recommit_directory_consistency` — decommit/reset/recommit cycle
//!    preserves directory consistency (requires `alloc-decommit`).
//! 2. `medium_classes_directory_rebuild` — the directory works correctly with
//!    55 classes (requires `medium-classes`). Feature-additive: only compiled
//!    when `medium-classes` is on alongside `alloc-segment-directory`.
//! 3. `below_threshold_fallback_scan_functional` — sidecar-absent path:
//!    allocation and deallocation work without the directory.
//! 4. `recycle_reuse_different_class` — a recycled slot reused for a different
//!    class does not inherit stale bits from the old class.
//! 5. `periodic_proptest_workload` — a 500-op workload with periodic
//!    assertions every 50 ops (deterministic complement to the proptest).
//!
//! Feature-gated behind `alloc-segment-directory`.

#![cfg(feature = "alloc-segment-directory")]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

// ── helpers ──────────────────────────────────────────────────────────────

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
/// ALL (class, slot) pairs.
fn assert_directory_equals_rebuild(core: &mut AllocCore) {
    let class_count = AllocCore::dbg_small_class_count();
    let mut incremental = vec![vec![false; 1024]; class_count];
    for (c, row) in incremental.iter_mut().enumerate() {
        for (s, cell) in row.iter_mut().enumerate() {
            *cell = core.dbg_directory_get_bit(c, s).unwrap_or(false);
        }
    }
    let rebuilt = core.dbg_rebuild_directory();
    assert!(
        rebuilt,
        "directory should be materialised for this assertion"
    );
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

/// Decommit/reset/recommit cycle: after decommitting and recommitting
/// segments, the directory must remain consistent with BinTable state.
#[test]
#[cfg(feature = "alloc-decommit")]
fn decommit_recommit_directory_consistency() {
    use sefer_alloc::SmallSegmentPoolConfig;

    // Pool cap = 2: segments are pooled (decommitted) on empty, then reused.
    let pool_cfg = SmallSegmentPoolConfig::new().pool_segments(2);
    let cfg = sefer_alloc::LargeCacheConfig::new().pool(pool_cfg);
    let mut core = AllocCore::new_with_config(cfg).unwrap();

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Push past threshold.
    let (mut ptrs, _class) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Check initial consistency.
    assert_directory_equals_rebuild(&mut core);

    // Free ALL blocks — triggers decommit (pool) for some segments.
    while let Some(p) = ptrs.pop() {
        unsafe { core.dealloc(p, layout) };
    }
    // After freeing everything, the decommitted segments should have their
    // directory bits cleared.
    assert_directory_equals_rebuild(&mut core);

    // Allocate again — triggers recommit (unpool) of pooled segments.
    let mut ptrs2: Vec<*mut u8> = Vec::new();
    for _ in 0..300 {
        let p = core.alloc(layout);
        if p.is_null() {
            break;
        }
        ptrs2.push(p);
    }

    // Free some to create non-empty free lists.
    let n_free = ptrs2.len() / 3;
    for _ in 0..n_free {
        if let Some(p) = ptrs2.pop() {
            unsafe { core.dealloc(p, layout) };
        }
    }

    // Allocate more to trigger another round of pool/unpool.
    for _ in 0..100 {
        let p = core.alloc(layout);
        if p.is_null() {
            break;
        }
        ptrs2.push(p);
    }

    // Final consistency check after the full cycle.
    assert_directory_equals_rebuild(&mut core);
}

/// Medium-classes: the directory must work with 55 classes (SMALL_CLASS_COUNT
/// changes from 49 to 55). The bitmap is wider (55 * 16 * 8 = 7040 B).
#[test]
#[cfg(feature = "medium-classes")]
fn medium_classes_directory_rebuild() {
    let mut core = AllocCore::new().unwrap();

    // With medium-classes, SMALL_CLASS_COUNT should be 55.
    let class_count = AllocCore::dbg_small_class_count();
    assert_eq!(
        class_count, 55,
        "medium-classes should give 55 small classes, got {class_count}"
    );

    let (mut ptrs, _class) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Free some blocks across different classes.
    let sizes: &[usize] = &[16, 64, 256, 1024, 2048];
    let layouts: Vec<Layout> = sizes
        .iter()
        .map(|&s| Layout::from_size_align(s, 1).unwrap())
        .collect();

    // Allocate blocks of different classes.
    let mut extra: Vec<(*mut u8, Layout)> = Vec::new();
    for l in &layouts {
        for _ in 0..10 {
            let p = core.alloc(*l);
            if !p.is_null() {
                extra.push((p, *l));
            }
        }
    }

    // Free half.
    for _ in 0..extra.len() / 2 {
        let (p, l) = extra.pop().unwrap();
        unsafe { core.dealloc(p, l) };
    }

    // The directory must be correct for all 55 classes.
    assert_directory_equals_rebuild(&mut core);

    // Also try a medium-class-specific size (e.g. 393216 = 384 KiB).
    let medium_size = 393216;
    if let Some(medium_class) = SegmentLayout::class_for(medium_size, 1) {
        // This class index should be >= 49 (in the medium range).
        let medium_layout = Layout::from_size_align(medium_size, 1).unwrap();
        let mp = core.alloc(medium_layout);
        if !mp.is_null() {
            unsafe { core.dealloc(mp, medium_layout) };
            // After free, the medium class bit should be set.
            assert_directory_equals_rebuild(&mut core);
        }
        let _ = medium_class;
    }

    // Clean up remaining.
    for (p, l) in extra {
        unsafe { core.dealloc(p, l) };
    }
    while let Some(p) = ptrs.pop() {
        unsafe { core.dealloc(p, layout) };
    }
}

/// Recycle+reuse with a DIFFERENT class: a slot that previously held
/// segments with class A free-list entries, once recycled and reused for
/// class B allocations, must NOT carry stale bits from class A.
#[test]
#[cfg(feature = "alloc-decommit")]
fn recycle_reuse_different_class() {
    use sefer_alloc::SmallSegmentPoolConfig;

    // Pool cap = 0: segments are released immediately on empty.
    let pool_cfg = SmallSegmentPoolConfig::new().pool_segments(0);
    let cfg = sefer_alloc::LargeCacheConfig::new().pool(pool_cfg);
    let mut core = AllocCore::new_with_config(cfg).unwrap();

    let layout_big = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Phase 1: fill with big blocks to cross threshold.
    let (mut ptrs_big, class_big) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Free some big blocks (sets directory bits for class_big).
    let n_free = ptrs_big.len() / 2;
    for _ in 0..n_free {
        let p = ptrs_big.pop().unwrap();
        unsafe { core.dealloc(p, layout_big) };
    }
    assert_directory_equals_rebuild(&mut core);

    // Free ALL remaining big blocks — triggers recycle for segments.
    while let Some(p) = ptrs_big.pop() {
        unsafe { core.dealloc(p, layout_big) };
    }
    assert_directory_equals_rebuild(&mut core);

    // Phase 2: allocate with a DIFFERENT class (16 B).
    let layout_small = Layout::from_size_align(16, 1).unwrap();
    let class_small = SegmentLayout::class_for(16, 1).unwrap();
    assert_ne!(
        class_big, class_small,
        "need different classes for this test"
    );

    let mut ptrs_small: Vec<*mut u8> = Vec::new();
    for _ in 0..500 {
        let p = core.alloc(layout_small);
        if p.is_null() {
            break;
        }
        ptrs_small.push(p);
    }

    // Free some small blocks.
    for _ in 0..ptrs_small.len() / 3 {
        let p = ptrs_small.pop().unwrap();
        unsafe { core.dealloc(p, layout_small) };
    }

    // The directory must be correct: class_big bits for recycled slots must
    // be 0; class_small bits for segments with freed blocks must be set.
    assert_directory_equals_rebuild(&mut core);
}

/// Below threshold: the directory is absent but allocation/deallocation
/// still works correctly via the linear scan.
#[test]
fn below_threshold_fallback_scan_functional() {
    let mut core = AllocCore::new().unwrap();
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    assert!(
        core.dbg_table_count() < threshold,
        "fresh core should be below threshold"
    );
    assert!(
        !core.dbg_directory_is_materialised(),
        "directory should not be materialised"
    );

    // Do a small workload that stays below threshold.
    let layout = Layout::from_size_align(64, 1).unwrap();
    let mut live: Vec<*mut u8> = Vec::new();
    for _ in 0..200 {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        live.push(p);
    }

    // Free half.
    for _ in 0..100 {
        let p = live.pop().unwrap();
        unsafe { core.dealloc(p, layout) };
    }

    // Alloc again — should reuse from free list (no directory needed).
    for _ in 0..50 {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        live.push(p);
    }

    // Directory must still be absent.
    assert!(
        !core.dbg_directory_is_materialised(),
        "directory should not materialise for small workloads"
    );
}

/// 500-op deterministic workload with periodic assertions every 50 ops,
/// using multiple classes and interleaved alloc/free. Complement to the
/// proptest (deterministic, covers the free-then-alloc-same-class pattern).
#[test]
fn periodic_assertions_multiclass_workload() {
    let mut core = AllocCore::new().unwrap();

    let sizes: &[usize] = &[16, 32, 64, 128, 256, 512, 1024, 2048];
    let layouts: Vec<Layout> = sizes
        .iter()
        .map(|&s| Layout::from_size_align(s, 1).unwrap())
        .collect();

    // Push past threshold.
    let (base_ptrs, _) = push_past_threshold(&mut core);
    let base_layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let mut live: Vec<(*mut u8, Layout)> = base_ptrs.iter().map(|&p| (p, base_layout)).collect();
    assert!(core.dbg_directory_is_materialised());

    // LCG for deterministic pseudo-random selection.
    let mut rng: u64 = 0xA5A5_B7B7_C3C3_D1D1;
    let lcg = |state: &mut u64| -> u64 {
        *state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        *state >> 33
    };

    for op in 0..500 {
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

        if (op + 1) % 50 == 0 {
            assert_directory_equals_rebuild(&mut core);
        }
    }

    assert_directory_equals_rebuild(&mut core);
}
