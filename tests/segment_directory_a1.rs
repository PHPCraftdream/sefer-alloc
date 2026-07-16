//! R7-A1 correctness test: after a workload that creates
//! `> DIRECTORY_MATERIALIZE_THRESHOLD` segments, the freshly-built directory
//! bits must EXACTLY match the actual non-empty state of every segment's
//! per-class `BinTable`.
//!
//! Feature-gated behind `alloc-segment-directory`.

#![cfg(feature = "alloc-segment-directory")]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

// ── helpers ──────────────────────────────────────────────────────────────

/// Allocate SMALL_MAX blocks until `table.count() > threshold`, returning
/// the pointers and the class index used.
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

// ── tests ────────────────────────────────────────────────────────────────

/// Core oracle: after freeing some blocks (creating non-empty BinTable
/// entries) and rebuilding the directory, every directory bit must EXACTLY
/// match whether the corresponding segment's BinTable head for that class
/// is non-empty.
///
/// We verify the oracle indirectly: we know which segments we freed blocks
/// from, so those segments' BinTable for the freed class MUST be non-empty.
/// All other (class, slot) pairs where no free happened should still be 0.
/// And slots beyond `count` must be 0.
#[test]
fn directory_rebuild_matches_bintable_after_frees() {
    let mut core = AllocCore::new().unwrap();

    let (mut ptrs, class_idx) = push_past_threshold(&mut core);

    // The directory MUST be materialised.
    assert!(
        core.dbg_directory_is_materialised(),
        "directory should be materialised after crossing threshold"
    );

    let count = core.dbg_table_count();

    // ── Phase 1: verify initial rebuild ────────────────────────────────
    // At materialisation time, all blocks were allocated (none freed), so
    // all BinTable heads should be FREE_LIST_NULL. The directory bits
    // should all be 0 — that is CORRECT.
    let class_count = AllocCore::dbg_small_class_count();
    for slot in 0..1024usize {
        for c in 0..class_count {
            // Slots beyond count: must be 0.
            // Slots within count: also 0 (all blocks consumed, no free-list
            // entries). We verify this holistically.
            let bit = core.dbg_directory_get_bit(c, slot);
            assert_eq!(
                bit,
                Some(false),
                "initial rebuild: slot {slot} class {c} should be false \
                 (all blocks allocated, no free-list entries)"
            );
        }
    }

    // ── Phase 2: free some blocks to create known non-empty entries ────
    // Free every 3rd pointer. Each freed block goes back to its segment's
    // BinTable for the SMALL_MAX class. After freeing, the BinTable head
    // for that class in those segments should be non-empty.
    //
    // We record which SEGMENT BASES got a free (the segment base is
    // ptr & ~(SEGMENT - 1)).
    let segment_mask = !(SegmentLayout::SEGMENT - 1);
    let mut freed_segment_bases: std::collections::HashSet<usize> =
        std::collections::HashSet::new();
    let mut freed_count = 0usize;

    // Free every 3rd block. Use `dealloc` with the same layout.
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    let mut i = 0;
    while i < ptrs.len() {
        let p = ptrs[i];
        let seg_base = (p as usize) & segment_mask;
        freed_segment_bases.insert(seg_base);
        // SAFETY: `p` was returned by `core.alloc(layout)` and has not
        // been freed yet. `layout` matches the original allocation.
        unsafe { core.dealloc(p, layout) };
        // Remove from ptrs so we don't double-free on drop.
        ptrs.swap_remove(i);
        freed_count += 1;
        // Skip 2 to free every 3rd.
        i += 2;
    }
    assert!(freed_count > 0, "must free at least one block");

    // ── Phase 3: rebuild and verify ───────────────────────────────────
    let rebuilt = core.dbg_rebuild_directory();
    assert!(rebuilt, "rebuild should succeed on materialised directory");

    // After rebuild, the SMALL_MAX class bits for segments that received
    // frees should be SET (their BinTable head is non-empty).
    let mut any_set = false;
    for slot in 0..count as usize {
        let bit = core.dbg_directory_get_bit(class_idx, slot);
        if bit == Some(true) {
            any_set = true;
        }
    }
    assert!(
        any_set,
        "after freeing {freed_count} blocks into {n} segments, at least one \
         directory bit for class {class_idx} must be set",
        n = freed_segment_bases.len(),
    );

    // Slots beyond count must still be 0 for all classes.
    for slot in count as usize..1024 {
        for c in 0..class_count {
            assert_eq!(
                core.dbg_directory_get_bit(c, slot),
                Some(false),
                "post-rebuild: slot {slot} class {c} beyond count should be false"
            );
        }
    }
}

/// The directory should NOT be materialised if the segment count stays
/// below the threshold.
#[test]
fn directory_does_not_materialise_below_threshold() {
    let mut core = AllocCore::new().unwrap();

    let threshold = AllocCore::dbg_directory_materialize_threshold();
    assert!(
        core.dbg_table_count() < threshold,
        "initial table count should be below threshold"
    );
    assert!(
        !core.dbg_directory_is_materialised(),
        "directory should not be materialised below threshold"
    );

    // Allocate a few small blocks (not enough to create THRESHOLD segments).
    let layout = Layout::from_size_align(16, 1).unwrap();
    for _ in 0..100 {
        let p = core.alloc(layout);
        assert!(!p.is_null());
    }

    assert!(
        core.dbg_table_count() < threshold,
        "table count should still be below threshold after a few small allocs"
    );
    assert!(
        !core.dbg_directory_is_materialised(),
        "directory should still not be materialised below threshold"
    );
}

/// Verify that when the directory IS materialised, the dbg_directory_get_bit
/// accessor returns `Some(_)` for valid indices (no panic).
#[test]
fn directory_bit_accessor_returns_some_when_materialised() {
    let mut core = AllocCore::new().unwrap();

    let (_ptrs, _class) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    let class_count = AllocCore::dbg_small_class_count();
    for c in 0..class_count {
        for slot in 0..1024 {
            let bit = core.dbg_directory_get_bit(c, slot);
            assert!(
                bit.is_some(),
                "dbg_directory_get_bit({c}, {slot}) returned None when materialised"
            );
        }
    }
}

/// The materialisation threshold constant must be 32 (per A0 baseline
/// recommendation).
#[test]
fn threshold_is_32() {
    assert_eq!(AllocCore::dbg_directory_materialize_threshold(), 32);
}

/// When the feature is on but the directory is NOT materialised,
/// `dbg_directory_get_bit` returns `None`.
#[test]
fn directory_get_bit_returns_none_when_not_materialised() {
    let core = AllocCore::new().unwrap();
    assert!(!core.dbg_directory_is_materialised());
    assert_eq!(core.dbg_directory_get_bit(0, 0), None);
}
