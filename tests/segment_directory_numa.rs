//! R11-6: NUMA node-indexed segment directory — correctness tests.
//!
//! Verifies the four mandatory correctness items from R10-6 §7.3:
//! 1. **Per-node oracle**: incremental directory == from-scratch rebuild for
//!    EVERY node bucket (not just the OR-across-buckets view the pre-R11-6
//!    oracle checked).
//! 2. **Local-first / foreign-fallback**: a FOREIGN-node segment appearing
//!    EARLIER in the segment table than a LOCAL-node segment must NOT be
//!    returned by the directory when the local segment has a free block.
//! 3. **R8-2 authoritative-miss under NUMA**: a directory miss (all node
//!    buckets empty for a class) still increments the per-class streak.
//! 4. **R9-8 rescue-scan interaction**: confirmation that the rescue
//!    wrappers remain gated `not(numa-aware)` — the OOM rescue path is
//!    unaffected (compiled out under NUMA, same as the pre-R11-6 status quo).
//!
//! Build/run:
//!   cargo test --features "numa-aware-mock alloc-segment-directory" \
//!       --test segment_directory_numa

#![cfg(all(feature = "numa-aware-mock", feature = "alloc-segment-directory"))]

use std::alloc::Layout;

use numa_shim::mock;
use sefer_alloc::{AllocCore, SegmentLayout};

/// SMALL_MAX class — the largest small block, fewest blocks per segment, so
/// the fewest allocations are needed to create many segments (and cross the
/// 32-segment materialisation threshold).
const CLASS_SIZE: usize = SegmentLayout::SMALL_MAX;

fn small_max_layout() -> Layout {
    Layout::from_size_align(CLASS_SIZE, 1).unwrap()
}

/// Script the mock to return `node`, then clear the call log.
fn script_node(node: u32) {
    mock::set_current_node(node);
    let _ = mock::drain();
}

// ── §7.3 item 1: per-node oracle ──────────────────────────────────────────

/// Assert that the incremental directory bitmap equals a fresh rebuild for
/// EVERY (bucket, class, slot) triple — not just the OR-across-buckets view.
/// This is the R10-6 §7.3 item 1 oracle extended to the per-node dimension.
fn assert_directory_equals_rebuild_per_node(core: &mut AllocCore) {
    let class_count = AllocCore::dbg_small_class_count();
    let node_bitmaps = AllocCore::dbg_directory_node_bitmaps();

    // Save the incremental (current) state for every bucket/class/slot.
    let mut incremental = vec![vec![vec![false; 1024]; class_count]; node_bitmaps];
    for (nb, plane) in incremental.iter_mut().enumerate() {
        for (c, row) in plane.iter_mut().enumerate() {
            for (s, cell) in row.iter_mut().enumerate() {
                *cell = core.dbg_directory_get_bit_bucket(nb, c, s).unwrap_or(false);
            }
        }
    }

    // Rebuild from scratch.
    assert!(
        core.dbg_rebuild_directory(),
        "directory should be materialised for this assertion"
    );

    // Compare per-bucket.
    for (nb, plane) in incremental.iter().enumerate() {
        for (c, row) in plane.iter().enumerate() {
            for (s, &inc_val) in row.iter().enumerate() {
                let fresh = core.dbg_directory_get_bit_bucket(nb, c, s).unwrap_or(false);
                assert_eq!(
                    inc_val, fresh,
                    "per-node directory mismatch at bucket={nb} class={c} slot={s}: \
                     incremental={inc_val}, rebuild={fresh}",
                );
            }
        }
    }
}

/// §7.3 item 1: the per-node oracle must hold after a mixed-node workload.
/// Creates segments on two mocked nodes, frees some blocks, then verifies
/// the incremental directory matches a from-scratch rebuild for every bucket.
#[test]
fn per_node_oracle_holds_after_mixed_node_workload() {
    let foreign_node = 3u32;
    let local_node = 1u32;
    let layout = small_max_layout();

    script_node(foreign_node);
    let mut core = AllocCore::new().expect("bootstrap");

    // Create foreign segments.
    let mut ptrs: Vec<*mut u8> = Vec::new();
    for _ in 0..300 {
        ptrs.push(core.alloc(layout));
    }
    // Create local segments (invalidate cache so the mock is re-queried).
    script_node(local_node);
    core.dbg_invalidate_numa_node_cache();
    for _ in 0..300 {
        ptrs.push(core.alloc(layout));
    }
    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised (>32 segments)"
    );

    // Verify segments are on different nodes.
    let any_foreign = ptrs
        .iter()
        .any(|&p| core.dbg_node_id_for(p) == Some(foreign_node));
    let any_local = ptrs
        .iter()
        .any(|&p| core.dbg_node_id_for(p) == Some(local_node));
    assert!(any_foreign, "must have foreign-node segments");
    assert!(any_local, "must have local-node segments");

    // Free some blocks to create non-empty BinTable entries.
    for (i, &p) in ptrs.iter().enumerate() {
        if i % 3 == 0 {
            unsafe { core.dealloc(p, layout) };
        }
    }

    // The per-node oracle must hold.
    assert_directory_equals_rebuild_per_node(&mut core);

    // Cleanup.
    for (i, &p) in ptrs.iter().enumerate() {
        if i % 3 != 0 {
            unsafe { core.dealloc(p, layout) };
        }
    }
}

// ── §7.3 item 2: local-first / foreign-fallback ──────────────────────────

/// The headline test: construct segments on TWO different mocked nodes, with
/// a FOREIGN segment appearing EARLIER in the segment table than a LOCAL
/// segment, both with free blocks for the same class. Assert the directory
/// scan returns the LOCAL segment, not the foreign one.
///
/// **Construction note (R11-6 zero-trust review):** This test calls
/// `dbg_find_segment_with_free` DIRECTLY, bypassing `alloc_small`'s step-1
/// `pop_free(small_cur)` fast path. The earlier version called `core.alloc()`
/// and ASSUMED `small_cur`'s free list would be empty, forcing the directory
/// scan to run. That assumption was not guaranteed — if `small_cur` happened
/// to point at a segment with a free block, the alloc resolved at step 1
/// before the directory was ever consulted, making the test vacuous with
/// respect to the bucket scan order. The direct hook removes that dependency
/// entirely: the directory scan is ALWAYS the deciding factor.
///
/// Catches a naive "first set bit across any node" or "wrong bucket order"
/// bug: if the directory scanned foreign buckets before local, or didn't
/// separate node buckets at all, the foreign segment (earlier in the table)
/// would be returned first.
#[test]
fn directory_returns_local_not_foreign() {
    let foreign_node = 3u32;
    let local_node = 1u32;
    let layout = small_max_layout();

    // ── Phase 1: create foreign-node segments (node 3) — LOWER slot indices ──
    script_node(foreign_node);
    let mut core = AllocCore::new().expect("bootstrap");
    let mut foreign_ptrs: Vec<*mut u8> = Vec::new();
    for _ in 0..300 {
        foreign_ptrs.push(core.alloc(layout));
    }

    // ── Phase 2: create local-node segments (node 1) — HIGHER slot indices ──
    script_node(local_node);
    core.dbg_invalidate_numa_node_cache();
    let mut local_ptrs: Vec<*mut u8> = Vec::new();
    for _ in 0..300 {
        local_ptrs.push(core.alloc(layout));
    }

    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised"
    );

    // Verify the segments are actually on different nodes.
    assert_eq!(
        core.dbg_node_id_for(foreign_ptrs[0]),
        Some(foreign_node),
        "foreign segments must be stamped with node {foreign_node}"
    );
    assert_eq!(
        core.dbg_node_id_for(local_ptrs[0]),
        Some(local_node),
        "local segments must be stamped with node {local_node}"
    );

    // Foreign segments must have LOWER slot indices than local segments
    // (they were created first → registered first → lower table slots).
    let foreign_slot = core.dbg_segment_id_of(foreign_ptrs[0]);
    let local_slot = core.dbg_segment_id_of(local_ptrs[0]);
    assert!(
        foreign_slot < local_slot,
        "foreign segment (slot {foreign_slot}) must appear EARLIER in the table \
         than local (slot {local_slot}) for this test to be meaningful"
    );

    let class_idx = core
        .dbg_layout_class_for(layout)
        .expect("SMALL_MAX resolves to a class");

    // ── Phase 3: free ALL blocks — every segment now has free blocks ──
    // After this, BOTH the foreign bucket AND the local bucket have directory
    // bits set. The directory scan MUST prefer local (scan local bucket first)
    // even though foreign segments have lower slot indices AND also have free
    // blocks. A naive "first set bit" or "wrong bucket order" directory would
    // return a foreign segment.
    let all_ptrs: Vec<*mut u8> = foreign_ptrs
        .iter()
        .chain(local_ptrs.iter())
        .copied()
        .collect();
    for &p in &all_ptrs {
        unsafe { core.dealloc(p, layout) };
    }

    // ── Phase 4: call find_segment_with_free DIRECTLY ──
    // This bypasses alloc_small's step-1 pop_free(small_cur) entirely, so the
    // directory-driven lookup is ALWAYS the deciding factor — no dependency on
    // incidental small_cur state.
    let seg = core
        .dbg_find_segment_with_free(class_idx)
        .expect("must find a segment with free blocks (all were just freed)");

    let seg_node = core.dbg_node_id_for(seg);
    assert_eq!(
        seg_node,
        Some(local_node),
        "directory must return a LOCAL segment (node {local_node}), not the \
         foreign one (node {foreign_node}) — a FOREIGN return here means the \
         directory is not honouring node-preference order (the binding R7 P2 \
         constraint from R10-6 §3.1). foreign_node={foreign_node} has LOWER \
         slot indices AND free blocks, so only correct bucket-order prevents \
         its return."
    );
}

// ── §7.3 item 3: R8-2 authoritative-miss under NUMA ───────────────────────

/// A directory miss (all node buckets empty for a class) under NUMA must
/// still trigger the R8-2 per-class miss-streak (the miss is trusted). This
/// test verifies the streak counter increments on a genuine miss, proving
/// the R8-2 authoritative-miss machinery fires correctly under the
/// node-indexed directory.
#[test]
fn r82_authoritative_miss_under_numa() {
    let layout = small_max_layout();

    script_node(0u32);
    let mut core = AllocCore::new().expect("bootstrap");

    // Create enough segments to materialise the directory.
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let mut ptrs: Vec<*mut u8> = Vec::new();
    loop {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(core.dbg_directory_is_materialised());

    // Pick a class that has NO free blocks anywhere (all blocks allocated,
    // none freed). Use a DIFFERENT class than the one we allocated — one we
    // never touched, so its BinTable is empty in every segment, meaning all
    // node buckets are empty → genuine directory miss.
    let empty_layout = Layout::from_size_align(16, 1).unwrap();
    let class_idx = core
        .dbg_layout_class_for(empty_layout)
        .expect("16 B resolves to a class");

    // Reset the streak for all classes.
    core.dbg_directory_reset_miss_streak();

    // Allocate the empty class. The directory has no bits set for this class
    // in ANY node bucket → directory MISS → R8-2 trust → carve a new segment.
    // The streak should increment to 1.
    let p = core.alloc(empty_layout);
    assert!(!p.is_null(), "alloc of a fresh class must succeed (carve)");

    let streak = core.dbg_directory_miss_streak_for_class(class_idx);
    assert_eq!(
        streak, 1,
        "a genuine NUMA directory miss must increment the per-class streak to 1 \
         (got {streak}) — if this is 0, the R8-2 miss handling is not firing \
         under numa-aware"
    );

    // Cleanup.
    unsafe { core.dealloc(p, empty_layout) };
    for &p in &ptrs {
        unsafe { core.dealloc(p, layout) };
    }
}

// ── §7.3 item 4: R9-8 rescue-scan interaction ────────────────────────────

/// The R9-8 rescue-scan wrappers (`find_segment_with_free_forced` /
/// `find_segment_with_free_checked_forced`) are gated
/// `#[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]`.
/// Under NUMA they do not exist — the OOM path surfaces OOM without a rescue
/// scan (same as the pre-R11-6 status quo). This test confirms the
/// `dbg_directory_rescue_scan` hook returns `None` under NUMA (the rescue
/// machinery is compiled out), verifying R11-6 did not accidentally enable
/// the rescue path for NUMA.
#[test]
fn rescue_scan_unaffected_under_numa() {
    let layout = small_max_layout();
    script_node(0u32);
    let mut core = AllocCore::new().expect("bootstrap");

    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let mut ptrs: Vec<*mut u8> = Vec::new();
    loop {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(core.dbg_directory_is_materialised());

    // The rescue scan hook's inner block is gated
    // not(feature = "numa-aware") — under numa-aware it returns None.
    let rescue_result = core.dbg_directory_rescue_scan(0);
    assert!(
        rescue_result.is_none(),
        "rescue scan must NOT run under numa-aware (it is gated \
         not(feature = \"numa-aware\")) — a Some() here means the rescue path \
         was accidentally enabled for NUMA"
    );

    for &p in &ptrs {
        unsafe { core.dealloc(p, layout) };
    }
}
