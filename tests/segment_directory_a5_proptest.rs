//! R7-A5 proptest: randomised op-stream verifying the directory invariant.
//!
//! **Invariant:** after every batch of alloc/free operations, for every
//! (class, slot) pair, `SegmentDirectory::get_bit(class, slot)` == the actual
//! `BinTable::head(class) != FREE_LIST_NULL` state of the segment at that slot.
//!
//! This is the PROPTEST that A5 adds — the A2 tests use a fixed LCG, not a
//! proptest strategy. Per CLAUDE.md: modest case count (~64).
//!
//! Feature-gated behind `alloc-segment-directory`.

#![cfg(feature = "alloc-segment-directory")]

use std::alloc::Layout;

use proptest::prelude::*;
use sefer_alloc::{AllocCore, SegmentLayout};

/// Operation applied to the allocator.
#[derive(Clone, Debug)]
enum Op {
    /// Allocate a block of this size class index (modulo the number of sizes).
    Alloc(usize),
    /// Free the block at this index (modulo the number of live blocks).
    Free(usize),
}

/// Generate a sequence of alloc/free operations.
fn ops_strategy() -> impl Strategy<Value = Vec<Op>> {
    prop::collection::vec(
        prop_oneof![
            // 60% allocs, 40% frees — keeps the working set growing so we
            // cross the directory materialisation threshold.
            6 => any::<usize>().prop_map(Op::Alloc),
            4 => any::<usize>().prop_map(Op::Free),
        ],
        50..300,
    )
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn directory_bits_match_bintable_under_random_ops(ops in ops_strategy()) {
        // Small sizes that cover several different classes.
        let sizes: &[usize] = &[16, 32, 64, 128, 256, 512, 1024, 2048];
        let layouts: Vec<Layout> = sizes
            .iter()
            .map(|&s| Layout::from_size_align(s, 1).unwrap())
            .collect();

        let mut core = AllocCore::new().unwrap();

        // Phase 1: push past threshold using SMALL_MAX to materialise the
        // directory quickly.
        let threshold = AllocCore::dbg_directory_materialize_threshold();
        let big_layout =
            Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();

        let mut live: Vec<(*mut u8, Layout)> = Vec::new();
        let max_allocs = (threshold as usize + 5) * 20;
        for _ in 0..max_allocs {
            let p = core.alloc(big_layout);
            prop_assert!(!p.is_null(), "alloc returned null during threshold push");
            live.push((p, big_layout));
            if core.dbg_table_count() > threshold {
                break;
            }
        }
        prop_assert!(
            core.dbg_directory_is_materialised(),
            "directory not materialised after threshold push"
        );

        // Phase 2: execute the random op-stream.
        for op in &ops {
            match op {
                Op::Alloc(idx) => {
                    let layout = layouts[*idx % layouts.len()];
                    let p = core.alloc(layout);
                    if !p.is_null() {
                        live.push((p, layout));
                    }
                }
                Op::Free(idx) => {
                    if !live.is_empty() {
                        let i = *idx % live.len();
                        let (p, layout) = live.swap_remove(i);
                        unsafe { core.dealloc(p, layout) };
                    }
                }
            }
        }

        // Phase 3: verify the invariant.
        assert_directory_equals_rebuild(&mut core);
    }
}
