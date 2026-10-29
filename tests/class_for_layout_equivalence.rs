//! WB-1/A-1: the hot-path classifier `SizeClasses::class_for_layout` (one
//! ceiling-indexed load into `SIZE2CLASS_CEIL`, reached through the
//! doc-hidden `SegmentLayout::class_for_layout` forwarder) must equal the
//! clamped classifier it replaced on the alloc and own-thread free paths,
//! `class_for(layout.size().max(MIN_BLOCK), layout.align())`, for every
//! `Layout`.
//!
//! Counterfactuals this test turns red: a ceiling table not shifted by one
//! (`SIZE2CLASS_CEIL[i] = SIZE2CLASS[i]`) differs at every bucket boundary
//! (`size = 16k + 1`); `SIZE2CLASS_CEIL[0] != SIZE2CLASS[0]` differs at
//! `size = 0`; a bound `idx > S2C_LEN` instead of `>=` indexes past the
//! table at `idx == S2C_LEN` and panics (sizes past `SMALL_MAX` are covered).

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;
use sefer_alloc::SegmentLayout;

const MIN_BLOCK: usize = SegmentLayout::MIN_BLOCK;

fn check(size: usize, align: usize) {
    let Ok(layout) = Layout::from_size_align(size, align) else {
        return;
    };
    let want = SegmentLayout::class_for(size.max(MIN_BLOCK), align);
    let got = SegmentLayout::class_for_layout(layout);
    assert_eq!(
        got, want,
        "class_for_layout drift at size={size} align={align}"
    );
}

#[test]
fn matches_clamped_class_for_all_small_sizes_and_aligns() {
    let buckets = SegmentLayout::SIZE2CLASS.len();
    let max_size = (buckets + 64) * MIN_BLOCK;
    let mut align = 1usize;
    while align <= 65_536 {
        for size in 0..=max_size {
            check(size, align);
        }
        align <<= 1;
    }
}

#[test]
fn matches_clamped_class_for_near_isize_max() {
    let top = isize::MAX as usize;
    let mut align = 1usize;
    while align <= 4096 {
        for k in 0..256 {
            check(top - k, align);
        }
        align <<= 1;
    }
}

#[test]
fn small_sizes_take_the_ceiling_bucket() {
    // Every bucket boundary for the first 64 buckets, both sides.
    for b in 0..64usize {
        for size in [b * MIN_BLOCK, b * MIN_BLOCK + 1] {
            check(size, 8);
            check(size, 16);
        }
    }
}
