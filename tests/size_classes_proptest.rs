//! Property test for `SizeClasses::class_for` — task B1 (2026-07), the
//! page-aligned follow-up to #114.
//!
//! Complements the exhaustive small-sweep in `tests/size_classes_lookup.rs`
//! (every `size` up to `SMALL_MAX` at small aligns) with a broader
//! `size × align` product, including the full range of power-of-two aligns a
//! real `Layout` can carry (1..=4096), over a `size` range that spans past
//! `SMALL_MAX` (so the Large-path branch is exercised too). ~64 cases per the
//! repo's "modest number of cases by default" policy (`CLAUDE.md`) — this is
//! a conformance smoke-check, not exhaustive fuzzing (the exhaustive sweep
//! already lives in `size_classes_lookup.rs`).
//!
//! Two properties are checked for every generated `(size, align)`:
//!
//! 1. **Fidelity, whenever `class_for` resolves `Some(idx)`:**
//!    `SIZE_CLASS_TABLE[idx] >= size.max(align)` (M4: the block is big
//!    enough) AND `SIZE_CLASS_TABLE[idx] % align == 0` (the block's natural
//!    offset within a SEGMENT-aligned segment lands on an `align`-aligned
//!    address).
//! 2. **The B1 regression canary:** for `align` in `{512, 1024, 2048, 4096}`
//!    and `size <= 16384`, `class_for` must ALWAYS return `Some` — never
//!    fall through to the Large path. Before task B1, no class in the plain
//!    geometric `SIZE_CLASS_TABLE` was ever a multiple of 512 or above, so
//!    every one of these cases resolved to `None` (Large), which — under a
//!    workload repeating such an allocation more than `MAX_SEGMENTS` (1024)
//!    times — is the exact `SegmentTable`-exhaustion abort #114 fixed for a
//!    different alignment range. This assertion is the property that would
//!    fail if `PAGE_ALIGNED_EXTRA` were reverted to empty (see
//!    `src/alloc_core/size_classes.rs` and
//!    `tests/regression_page_aligned_no_segment_exhaustion.rs` for the
//!    execution-level counterfactual).

#![cfg(feature = "alloc-core")]

use proptest::prelude::*;
use sefer_alloc::SegmentLayout;

/// Every alignment a real `Layout` can carry that we care about here: powers
/// of two from 1 up to 4096 (the `Layout` contract requires align to be a
/// power of two; sefer-alloc's small classes only ever need to cover up to a
/// few page multiples — larger aligns are legitimately Large-path territory).
fn pow2_align() -> impl Strategy<Value = usize> {
    prop_oneof![
        Just(1usize),
        Just(2usize),
        Just(4usize),
        Just(8usize),
        Just(16usize),
        Just(32usize),
        Just(64usize),
        Just(128usize),
        Just(256usize),
        Just(512usize),
        Just(1024usize),
        Just(2048usize),
        Just(4096usize),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn class_for_resolution_is_always_fidelity_correct(
        size in 1usize..=(2 * SegmentLayout::SMALL_MAX),
        align in pow2_align(),
    ) {
        if let Some(idx) = SegmentLayout::class_for(size, align) {
            let block = SegmentLayout::SIZE_CLASS_TABLE[idx];
            let need = size.max(align);
            prop_assert!(
                block >= need,
                "size={size} align={align}: class {idx} block={block} < need={need}"
            );
            prop_assert_eq!(
                block % align, 0,
                "size={} align={}: class {} block={} not divisible by align",
                size, align, idx, block
            );
        }
    }

    /// The B1 regression canary: page-aligned small requests must resolve to
    /// a small class, not fall through to Large.
    #[test]
    fn page_aligned_small_requests_never_fall_through_to_large(
        size in 1usize..=16384usize,
        align in prop_oneof![Just(512usize), Just(1024usize), Just(2048usize), Just(4096usize)],
    ) {
        let got = SegmentLayout::class_for(size, align);
        prop_assert!(
            got.is_some(),
            "size={size} align={align}: class_for fell through to Large — \
             pre-B1 regression (no small class was ever a multiple of {align})"
        );
    }
}
