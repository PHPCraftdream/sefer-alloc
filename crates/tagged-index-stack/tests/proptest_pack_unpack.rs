//! Property-based round-trip tests for `TaggedIndex::pack`/`unpack`, covering
//! several widths (including the degenerate `INDEX_BITS = 1`) with randomly
//! generated `(index, tag)` pairs — complementing `stack_unit.rs`'s
//! hand-picked-literal tests: `pack_unpack_round_trip_16` (width 16 only),
//! `width_12_partitions` (width 12), and
//! `max_legal_width_index_mask_never_equals_tail` (width 16).
//!
//! Every width stays inside the legal `1..=16` range enforced by
//! `TaggedIndex::_CHECK_BITS`; width 15 probes just under the ceiling the way
//! 31 once sat against the old 32 ceiling, and width 16 is the ceiling itself
//! (the minimum 48-bit-tag configuration).
//!
//! Per this repo's fast-proptest convention (CLAUDE.md: "modest number of
//! cases by default (around 64) — this is a smoke-check for conformance, not
//! exhaustive fuzzing"), each property runs 64 cases.

#![cfg(not(loom))]

use proptest::prelude::*;
use tagged_index_stack::TaggedIndex;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn round_trip_width_1(
        // At width 1, INDEX_MASK == 1, so `index` (0..1) only ever takes the
        // value 0 — this property exercises the TAG axis under the degenerate
        // 1-bit index half, not a randomized index.
        index in 0u64..TaggedIndex::<1>::INDEX_MASK,
        tag in 0u64..(1u64 << TaggedIndex::<1>::TAG_BITS),
    ) {
        type T = TaggedIndex<1>;
        let word = T::pack(index, tag);
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word), "a valid index (< INDEX_MASK) must not read empty");
    }

    #[test]
    fn round_trip_width_16(
        index in 0u64..TaggedIndex::<16>::INDEX_MASK,
        tag in 0u64..(1u64 << TaggedIndex::<16>::TAG_BITS),
    ) {
        type T = TaggedIndex<16>;
        let word = T::pack(index, tag);
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }

    #[test]
    fn round_trip_width_15(
        index in 0u64..TaggedIndex::<15>::INDEX_MASK,
        tag in 0u64..(1u64 << TaggedIndex::<15>::TAG_BITS),
    ) {
        type T = TaggedIndex<15>;
        let word = T::pack(index, tag);
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }

    #[test]
    fn round_trip_width_12(
        index in 0u64..TaggedIndex::<12>::INDEX_MASK,
        tag in 0u64..(1u64 << TaggedIndex::<12>::TAG_BITS),
    ) {
        type T = TaggedIndex<12>;
        let word = T::pack(index, tag);
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }
}
