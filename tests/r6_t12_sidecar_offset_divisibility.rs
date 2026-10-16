//! Conformance test: the owner-drain's reciprocal divisibility agrees with
//! modulo over sidecar-relevant offsets. This also passes with the previous
//! modulo implementation; a rounded-down reciprocal mutant fails at true
//! multiples where rounding is insufficient.
//!
//! The sweep covers every MIN_BLOCK-aligned offset in the segment, all
//! offsets in the first three block periods (bounded by SEGMENT), and a
//! stride-8191 sample over the remaining segment range.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn reciprocal_divisibility_matches_modulo_for_every_class_and_offset() {
    let classes = SegmentLayout::SIZE_CLASS_TABLE.len();
    assert!(classes > 0);
    let segment = SegmentLayout::SEGMENT;
    let min_block = SegmentLayout::MIN_BLOCK;
    for class in 0..classes {
        let block = SegmentLayout::SIZE_CLASS_TABLE[class];
        // Leg 1: every class, every MIN_BLOCK-aligned offset in the segment.
        let mut off = 0usize;
        while off < segment {
            assert_eq!(
                AllocCore::dbg_sidecar_offset_divisible_for_test(class, off),
                off % block == 0,
                "class {class} (block {block}): offset {off}"
            );
            off += min_block;
        }
        // Leg 2: every offset (aligned or not) over three full periods.
        let limit = (3 * block + 97).min(segment);
        for off in 0..limit {
            assert_eq!(
                AllocCore::dbg_sidecar_offset_divisible_for_test(class, off),
                off % block == 0,
                "class {class} (block {block}): offset {off}"
            );
        }
        // Leg 3: sampled sweep over the rest of the segment.
        let mut off = limit;
        while off < segment {
            assert_eq!(
                AllocCore::dbg_sidecar_offset_divisible_for_test(class, off),
                off % block == 0,
                "class {class} (block {block}): offset {off}"
            );
            off += 8191;
        }
    }
}
