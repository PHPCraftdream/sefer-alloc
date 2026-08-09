//! Property-based round-trip tests for `TaggedIndex::pack`/`unpack`, covering
//! several widths (including the degenerate `INDEX_BITS = 1`) with randomly
//! generated `(index, tag)` pairs — complementing `stack_unit.rs`'s
//! `pack_unpack_round_trip_16` test, which only exercises hand-picked literals
//! at widths 16/20/32.
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
    fn round_trip_width_31(
        index in 0u64..TaggedIndex::<31>::INDEX_MASK,
        tag in 0u64..(1u64 << TaggedIndex::<31>::TAG_BITS),
    ) {
        type T = TaggedIndex<31>;
        let word = T::pack(index, tag);
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }

    #[test]
    fn round_trip_width_32(
        index in 0u64..TaggedIndex::<32>::INDEX_MASK,
        tag in 0u64..(1u64 << TaggedIndex::<32>::TAG_BITS),
    ) {
        type T = TaggedIndex<32>;
        let word = T::pack(index, tag);
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }
}
