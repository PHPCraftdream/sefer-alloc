//! Wave B4: exhaustive equivalence test for the constant-folded
//! `align <= MIN_BLOCK` fast path in
//! `src/alloc_core/platform/size_classes.rs` (`SizeClasses::class_for_small_align`,
//! reached through the public `SegmentLayout::class_for` forwarder).
//!
//! Reference model: the crate's documented `class_for` algorithm
//! (seed LUT + index-space guard + divisibility-jump slow walk), re-run
//! INDEPENDENTLY here over the PUBLIC tables
//! (`SegmentLayout::SIZE2CLASS` / `SIZE_CLASS_TABLE` / `MIN_BLOCK_SHIFT`)
//! with generic (non-folded) arithmetic — including a runtime
//! `min_block_shift` read through a local variable so the compiler cannot
//! fold it the way the production fast path does.
//!
//! Equivalence claim under test: for every `size` and every power-of-two
//! `align`, `SegmentLayout::class_for(size, align)` equals the model —
//! in particular for `align <= MIN_BLOCK`, where production now takes the
//! folded fast path instead of `SC.class_for`.
//!
//! Counterfactual (verified red before landing): replacing the fast path's
//! `(size.max(1) - 1) >> MIN_BLOCK_SHIFT` with the naive
//! `(size - 1) >> MIN_BLOCK_SHIFT` (release-mode wrapping underflow at
//! `size == 0`) makes this test fail at `size = 0` for every
//! `align <= MIN_BLOCK` (`wrapping` fast gives `seed_idx = (usize::MAX)>>
//! 4 >= S2C_LEN - 1 -> None`, model gives `Some(0)`).

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use sefer_alloc::SegmentLayout;

const MIN_BLOCK: usize = SegmentLayout::MIN_BLOCK;
const MIN_BLOCK_SHIFT: u32 = SegmentLayout::MIN_BLOCK_SHIFT;

/// Independent, generic re-implementation of the crate's `class_for`
/// algorithm over the public tables. `shift` is a runtime value on purpose.
fn model_class_for(size: usize, align: usize) -> Option<usize> {
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let s2c = SegmentLayout::SIZE2CLASS;
    let shift: u32 = MIN_BLOCK_SHIFT;
    let need = if size > align { size } else { align };
    let seed_idx = (need - 1) >> shift;
    if seed_idx >= s2c.len() - 1 {
        return None;
    }
    let seed = s2c[seed_idx] as usize;
    if align <= (1usize << shift) {
        return Some(seed);
    }
    // Slow path: smallest class at/after `seed` whose block size is a
    // multiple of `align` (linear scan — provably the same answer as the
    // crate's divisibility JUMP, since every block size strictly between
    // two consecutive multiples of `align` is itself not a multiple).
    let mut i = seed;
    while i < table.len() {
        if table[i] & (align - 1) == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// The production entry under test.
fn live_class_for(size: usize, align: usize) -> Option<usize> {
    SegmentLayout::class_for(size, align)
}

fn check(size: usize, align: usize) {
    let m = model_class_for(size, align);
    let l = live_class_for(size, align);
    assert_eq!(
        l, m,
        "class_for drift at size={size} align={align}: live={l:?} model={m:?}"
    );
}

#[test]
fn fast_path_matches_generic_model_all_sizes_small_aligns() {
    // Exhaustive over sizes 0..=262_144 (SMALL_MAX ~ 253 KiB, plus headroom
    // past it) x every power-of-two align 1..=65_536.
    let mut align = 1usize;
    while align <= 65_536 {
        for size in 0..=262_144usize {
            check(size, align);
        }
        align <<= 1;
    }
}

#[test]
fn fast_path_matches_generic_model_around_class_boundaries() {
    // Around every class boundary, all power-of-two aligns (small and the
    // slow-path-relevant larger ones), including just-past-SMALL_MAX sizes.
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let mut align = 1usize;
    while align <= SegmentLayout::SMALL_MAX {
        for (i, &b) in table.iter().enumerate() {
            for d in 0..=1usize {
                for delta in [0usize, 1, MIN_BLOCK - 1, MIN_BLOCK] {
                    let base = b + d;
                    if base >= delta {
                        check(base - delta, align);
                    }
                    if i + 1 < table.len() {
                        check(b + delta, align);
                    }
                }
            }
        }
        align <<= 1;
    }
}

#[test]
fn fast_path_matches_generic_model_huge_sizes() {
    // Large/huge boundary and far-past-it sizes: both paths must agree on
    // `None` (or a small class at the boundary), for small and large aligns.
    let mut align = 1usize;
    while align <= 1 << 20 {
        for size in [
            SegmentLayout::SMALL_MAX - 1,
            SegmentLayout::SMALL_MAX,
            SegmentLayout::SMALL_MAX + 1,
            SegmentLayout::SEGMENT - 1,
            SegmentLayout::SEGMENT,
            SegmentLayout::SEGMENT + 1,
            usize::MAX / 2,
            usize::MAX - 1,
            usize::MAX,
        ] {
            check(size, align);
        }
        align <<= 1;
    }
}
