//! Finding #2 (T10 perf#9) — `class_for` align>16 slow-path equivalence.
//!
//! `SizeClasses::class_for`'s slow path (align > `SMALL_ALIGN_MAX`) was
//! replaced (T10) with a jump-based walk: instead of stepping one class at a
//! time (`i += 1`), it jumps from a non-divisible class to the class covering
//! the next multiple of `align` via the `SIZE2CLASS` table. This file proves
//! the new slow path is **bit-identical** to the old step-by-1 walk for EVERY
//! valid `(size, align)` pair — not just the common async-runtime alignments.
//!
//! The reference here (`walk_class_for`) is a faithful re-implementation of the
//! PRE-T10 slow path: seed at `SIZE2CLASS`, then `i += 1` until a class whose
//! `block_size` is a multiple of `align`. The new path must agree with it for
//! every power-of-two `align` (the `Layout` contract) from `2*MIN_BLOCK` up to
//! `SMALL_MAX`, and every `size` from 1 to `SMALL_MAX` (plus one past it, to
//! exercise the `None` leg).
//!
//! Non-vacuous: the counterfactual (reverting `class_for` to the old `i += 1`
//! walk) keeps this suite green trivially, but a BUGGY jump (e.g. rounding
//! `next_mult` wrong, or an off-by-one in the `SIZE2CLASS` index) makes at
//! least one assertion below fail — confirmed during T10 by deliberately
//! perturbing the round-up (`(block | align) + 1`, missing the `- 1`) which
//! produced drift at `align=128, size=128`.

#![cfg(feature = "alloc-core")]

use sefer_alloc::SegmentLayout;

/// The PRE-T10 slow-path reference: seed at `SIZE2CLASS`, then step forward one
/// class at a time until the first whose `block_size` is a multiple of `align`.
/// This is exactly the algorithm `class_for` used before T10's jump optimisation
/// — re-implemented here over the public table so the test does not trust the
/// crate's own `class_for`.
fn walk_class_for(size: usize, align: usize) -> Option<usize> {
    let table = SegmentLayout::SIZE_CLASS_TABLE;
    let s2c = SegmentLayout::SIZE2CLASS;
    let shift = SegmentLayout::MIN_BLOCK_SHIFT;
    let need = if size > align { size } else { align };
    if need > SegmentLayout::SMALL_MAX {
        return None;
    }
    let seed = s2c[(need - 1) >> shift] as usize;
    if align <= SegmentLayout::SMALL_ALIGN_MAX {
        return Some(seed);
    }
    let mut i = seed;
    while i < table.len() {
        if table[i].is_multiple_of(align) {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Every power-of-two alignment the slow path can see: `MIN_BLOCK` is the fast-
/// path ceiling, so the slow path begins at `2 * MIN_BLOCK`; the largest align
/// that does not already force `None` (`need > SMALL_MAX`) is the greatest
/// power of two `<= SMALL_MAX`.
fn slow_path_aligns() -> Vec<usize> {
    let mut aligns = Vec::new();
    let mut a = SegmentLayout::MIN_BLOCK * 2;
    while a <= SegmentLayout::SMALL_MAX {
        aligns.push(a);
        a <<= 1;
    }
    aligns
}

// ---------------------------------------------------------------------------
// 1. Exhaustive equivalence: new `class_for` == old walk, for EVERY (size, align).
// ---------------------------------------------------------------------------

#[test]
fn class_for_slow_path_matches_walk() {
    let aligns = slow_path_aligns();
    // size from 1 (the raw layout-contract minimum) through SMALL_MAX inclusive,
    // plus one past SMALL_MAX to exercise the `need > SMALL_MAX → None` leg.
    let smax = SegmentLayout::SMALL_MAX;
    for &align in &aligns {
        for size in 1..=(smax + 1) {
            let got = SegmentLayout::class_for(size, align);
            let want = walk_class_for(size, align);
            assert_eq!(
                got, want,
                "drift at size={size} align={align}: jump={got:?} walk={want:?}"
            );
            // When the slow path resolves Some, the resolved class must honour
            // BOTH fidelity predicates (M4): block >= max(size, align) AND
            // block % align == 0.
            if let Some(idx) = got {
                let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
                let need = if size > align { size } else { align };
                assert!(
                    block >= need,
                    "size={size} align={align}: class {idx} block={block} < need={need}"
                );
                assert_eq!(
                    block % align,
                    0,
                    "size={size} align={align}: class {idx} block={block} not divisible",
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// 2. The slow-path aligns themselves actually reach the slow path (no align in
//    the set is <= SMALL_ALIGN_MAX, and none exceeds SMALL_MAX). Guards against
//    a future change to SMALL_ALIGN_MAX / SMALL_MAX silently hollowing out the
//    sweep above.
// ---------------------------------------------------------------------------

#[test]
fn slow_path_align_set_covers_the_slow_path_range() {
    let small_align_max = SegmentLayout::SMALL_ALIGN_MAX;
    let smax = SegmentLayout::SMALL_MAX;
    let aligns = slow_path_aligns();
    assert!(!aligns.is_empty(), "slow-path align set must be non-empty");
    for &a in &aligns {
        assert!(
            a > small_align_max,
            "align {a} <= SMALL_ALIGN_MAX {small_align_max}: would take the fast path, not the slow path"
        );
        assert!(
            a <= smax,
            "align {a} > SMALL_MAX {smax}: need > SMALL_MAX → None before the slow path"
        );
    }
    // The smallest slow-path align is exactly 2 * MIN_BLOCK (the first power of
    // two above the fast-path ceiling).
    assert_eq!(aligns[0], small_align_max * 2);
}

// ---------------------------------------------------------------------------
// 3. The jump actually skips classes in the known-bad case (align=128 from the
//    ~144 B seed). This is the case the round-up arithmetic is most likely to
//    get wrong (the counterfactual in the module doc). Asserts the resolved
//    class is the 256 B class (the first 128-divisible class), NOT the seed.
// ---------------------------------------------------------------------------

#[test]
fn jump_skips_non_divisible_run_for_align_128() {
    // (size=128, align=128): need=128. The seed class covers 128; the table's
    // smallest class >= 128 is NOT 128 itself (the geometric progression skips
    // 128), so the seed is ~144 — not divisible by 128. The first 128-divisible
    // class is 256 (EXACT_EXTRA). The jump must land there in one hop.
    let got = SegmentLayout::class_for(128, 128).expect("(128,128) resolves");
    let block = SegmentLayout::SIZE_CLASS_TABLE[got];
    assert_eq!(block % 128, 0);
    assert!(block >= 128);
    // And it matches the old walk.
    assert_eq!(Some(got), walk_class_for(128, 128));
    // Sanity: the seed class for need=128 is NOT 128-divisible (this is what
    // makes the jump do real work — if 128 were a table entry the seed would
    // already satisfy divisibility and the jump would be a no-op there).
    let seed = SegmentLayout::SIZE2CLASS[(128 - 1) >> SegmentLayout::MIN_BLOCK_SHIFT] as usize;
    assert_ne!(
        SegmentLayout::SIZE_CLASS_TABLE[seed] % 128,
        0,
        "seed must be non-divisible for this case to exercise the jump"
    );
}
