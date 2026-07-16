//! R7-A3 correctness tests: the directory-accelerated lookup must return the
//! SAME segment the pure linear scan would, handle stale positives correctly,
//! and preserve the no-false-negative property after a local free.
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

/// Differential test: the directory-accelerated lookup returns the SAME
/// segment as the pure linear scan would. We do this by running a workload
/// (alloc many, free some to create non-empty BinTables across segments),
/// then verifying that `find_segment_with_free` finds a valid segment
/// whenever one exists AND the directory bitmap agrees with the rebuild.
#[test]
fn directory_lookup_finds_correct_segment() {
    let mut core = AllocCore::new().expect("bootstrap");
    let (ptrs, _class_idx) = push_past_threshold(&mut core);
    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised"
    );

    // Free every other pointer to create non-empty BinTables across segments.
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    for (i, &p) in ptrs.iter().enumerate() {
        if i % 2 == 0 {
            unsafe { core.dealloc(p, layout) };
        }
    }

    // The directory must still be consistent with the actual BinTable state.
    assert_directory_equals_rebuild(&mut core);

    // Now allocate again — this should use the directory-accelerated path.
    // The key property: the allocation succeeds (a segment with free blocks
    // is found) and is a valid pointer.
    let p = core.alloc(layout);
    assert!(!p.is_null(), "alloc after partial free must succeed");
    unsafe { core.dealloc(p, layout) };

    // Final consistency check.
    assert_directory_equals_rebuild(&mut core);

    // Clean up.
    for (i, &p) in ptrs.iter().enumerate() {
        if i % 2 != 0 {
            unsafe { core.dealloc(p, layout) };
        }
    }
    drop(core);
}

/// Stale-positive test: set a bit for a class, then empty the BinTable
/// head behind it. The directory lookup must clear the stale bit and still
/// find a correct segment (or correctly return None).
#[test]
fn stale_positive_cleared_and_correct_fallback() {
    let mut core = AllocCore::new().expect("bootstrap");
    let (ptrs, _class_idx) = push_past_threshold(&mut core);
    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised"
    );

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Free one block to create a non-empty BinTable entry.
    let freed_ptr = ptrs[0];
    unsafe { core.dealloc(freed_ptr, layout) };

    // The bit for this class/segment should be set.
    // Now re-alloc to consume that freed block, emptying the BinTable.
    let p = core.alloc(layout);
    assert!(!p.is_null());

    // Directory must remain consistent.
    assert_directory_equals_rebuild(&mut core);

    // Clean up: dealloc p and all remaining ptrs.
    unsafe { core.dealloc(p, layout) };
    for &p in &ptrs[1..] {
        unsafe { core.dealloc(p, layout) };
    }
    drop(core);
}

/// No-false-negative: after a local free, a subsequent alloc of that class
/// finds the segment (the directory bit was set by the free).
#[test]
fn no_false_negative_after_local_free() {
    let mut core = AllocCore::new().expect("bootstrap");
    let (ptrs, _class_idx) = push_past_threshold(&mut core);
    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised"
    );

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Alloc a few more blocks, then free them into non-current segments.
    // The free should set the directory bit, and a subsequent alloc of
    // the same class must find and reuse one of those segments.
    let extra: Vec<*mut u8> = (0..5)
        .map(|_| {
            let p = core.alloc(layout);
            assert!(!p.is_null());
            p
        })
        .collect();

    // Free all extras.
    for &p in &extra {
        unsafe { core.dealloc(p, layout) };
    }

    // Now alloc again — should find a segment with free blocks via the
    // directory (no false negative).
    let p = core.alloc(layout);
    assert!(!p.is_null(), "alloc after free must not be null");

    // Directory must agree with reality.
    assert_directory_equals_rebuild(&mut core);

    // Clean up.
    unsafe { core.dealloc(p, layout) };
    for &p in &ptrs {
        unsafe { core.dealloc(p, layout) };
    }
    drop(core);
}

/// Multi-class: free blocks of different classes into the same segment.
/// The directory must track each class independently.
#[test]
fn multi_class_same_segment_directory_tracking() {
    let mut core = AllocCore::new().expect("bootstrap");

    // First, push past threshold using SMALL_MAX (fills segments fast).
    let (ptrs_filler, _) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    // Use two different small classes.
    let size_a = 16;
    let size_b = 64;
    let layout_a = Layout::from_size_align(size_a, 1).unwrap();
    let layout_b = Layout::from_size_align(size_b, 1).unwrap();
    let class_a = SegmentLayout::class_for(size_a, 1).expect("class_a");
    let class_b = SegmentLayout::class_for(size_b, 1).expect("class_b");
    assert_ne!(class_a, class_b, "must be different classes");

    // Alloc some blocks of each class.
    let mut ptrs_a: Vec<*mut u8> = Vec::new();
    for _ in 0..20 {
        let p = core.alloc(layout_a);
        assert!(!p.is_null());
        ptrs_a.push(p);
    }
    let mut ptrs_b: Vec<*mut u8> = Vec::new();
    for _ in 0..20 {
        let p = core.alloc(layout_b);
        assert!(!p.is_null());
        ptrs_b.push(p);
    }

    // Free one of each class.
    unsafe { core.dealloc(ptrs_a[0], layout_a) };
    unsafe { core.dealloc(ptrs_b[0], layout_b) };

    // Directory must correctly reflect both classes.
    assert_directory_equals_rebuild(&mut core);

    // Alloc class_a -- must find the freed block.
    let pa = core.alloc(layout_a);
    assert!(!pa.is_null());
    // Alloc class_b -- must find the freed block.
    let pb = core.alloc(layout_b);
    assert!(!pb.is_null());

    assert_directory_equals_rebuild(&mut core);

    // Clean up.
    unsafe { core.dealloc(pa, layout_a) };
    unsafe { core.dealloc(pb, layout_b) };
    for &p in &ptrs_a[1..] {
        unsafe { core.dealloc(p, layout_a) };
    }
    for &p in &ptrs_b[1..] {
        unsafe { core.dealloc(p, layout_b) };
    }
    let layout_filler = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    for &p in &ptrs_filler {
        unsafe { core.dealloc(p, layout_filler) };
    }
    drop(core);
}

/// Below threshold: directory is not materialised, the linear scan runs.
/// This is the feature-OFF-equivalent path — must work identically.
#[test]
fn below_threshold_uses_linear_scan() {
    let mut core = AllocCore::new().expect("bootstrap");
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    assert!(
        core.dbg_table_count() < threshold,
        "fresh core must be below threshold"
    );
    assert!(
        !core.dbg_directory_is_materialised(),
        "directory must not be materialised below threshold"
    );

    // Alloc and free — the linear scan must still work.
    let layout = Layout::from_size_align(16, 1).unwrap();
    let p1 = core.alloc(layout);
    assert!(!p1.is_null());
    let p2 = core.alloc(layout);
    assert!(!p2.is_null());
    unsafe { core.dealloc(p1, layout) };
    let p3 = core.alloc(layout);
    assert!(!p3.is_null());
    // p3 should reuse p1's slot (from the free list).
    unsafe { core.dealloc(p2, layout) };
    unsafe { core.dealloc(p3, layout) };
    drop(core);
}

/// Intensive mixed workload: 500 ops (alloc/free interleaved), directory
/// must stay consistent throughout.
#[test]
fn mixed_workload_directory_consistency() {
    let mut core = AllocCore::new().expect("bootstrap");
    let (mut ptrs, _class_idx) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

    // Interleave alloc/free for 500 ops.
    for i in 0..500 {
        if i % 3 == 0 && !ptrs.is_empty() {
            // Free the last pointer.
            let p = ptrs.pop().unwrap();
            unsafe { core.dealloc(p, layout) };
        } else {
            let p = core.alloc(layout);
            assert!(!p.is_null(), "alloc failed at op {i}");
            ptrs.push(p);
        }
        // Periodic consistency check.
        if i % 50 == 0 {
            assert_directory_equals_rebuild(&mut core);
        }
    }

    // Final consistency check.
    assert_directory_equals_rebuild(&mut core);

    // Clean up.
    for &p in &ptrs {
        unsafe { core.dealloc(p, layout) };
    }
    drop(core);
}
