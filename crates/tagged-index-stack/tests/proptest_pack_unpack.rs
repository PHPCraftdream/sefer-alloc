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

// The strategies below shift `1u64 << TaggedIndex::<N>::TAG_BITS` at several
// widths — a compile-time shift-overflow panic/UB if `TAG_BITS == 64` (i.e.
// `INDEX_BITS == 0`). `_CHECK_BITS` caps `INDEX_BITS` at 1..=16 today, but
// these strategies are not inside a `const fn` and do not benefit from that
// compile-time guard, so the boundary is pinned here — one assert per width
// this file instantiates — making a future widening that legalizes
// `INDEX_BITS = 0` fail THIS file's build at exactly the shifted widths it
// would break.
const _: () = assert!(TaggedIndex::<1>::TAG_BITS < 64);
const _: () = assert!(TaggedIndex::<12>::TAG_BITS < 64);
const _: () = assert!(TaggedIndex::<15>::TAG_BITS < 64);
const _: () = assert!(TaggedIndex::<16>::TAG_BITS < 64);

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn round_trip_width_1(
        // At width 1, INDEX_MASK == 1, so `index` (0..1) only ever takes the
        // value 0 — this property exercises the TAG axis under the degenerate
        // 1-bit index half, not a randomized index.
        index in 0u32..((1u32 << 1) - 1),
        tag in 0u64..(1u64 << TaggedIndex::<1>::TAG_BITS),
    ) {
        type T = TaggedIndex<1>;
        let word = T::pack(index, tag).expect("strategy generates in-range halves");
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word), "a valid index (< INDEX_MASK) must not read empty");
    }

    #[test]
    fn round_trip_width_16(
        index in 0u32..((1u32 << 16) - 1),
        tag in 0u64..(1u64 << TaggedIndex::<16>::TAG_BITS),
    ) {
        type T = TaggedIndex<16>;
        let word = T::pack(index, tag).expect("strategy generates in-range halves");
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }

    #[test]
    fn round_trip_width_15(
        index in 0u32..((1u32 << 15) - 1),
        tag in 0u64..(1u64 << TaggedIndex::<15>::TAG_BITS),
    ) {
        type T = TaggedIndex<15>;
        let word = T::pack(index, tag).expect("strategy generates in-range halves");
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }

    #[test]
    fn round_trip_width_12(
        index in 0u32..((1u32 << 12) - 1),
        tag in 0u64..(1u64 << TaggedIndex::<12>::TAG_BITS),
    ) {
        type T = TaggedIndex<12>;
        let word = T::pack(index, tag).expect("strategy generates in-range halves");
        let (v, t) = T::unpack(word);
        prop_assert_eq!(v, index);
        prop_assert_eq!(t, tag);
        prop_assert!(!T::is_empty(word));
    }

    #[test]
    fn rejects_out_of_range_index_width_16(
        // Strictly OUTSIDE the index half: the first invalid index
        // (1 << INDEX_BITS) is the smallest possible reject, u32::MAX the
        // largest — the exact values the old truncating pack silently
        // masked into different valid-looking indices.
        // `INDEX_BITS` is the const generic parameter, not an associated
        // const, so the shift amount is spelled literally: width 16.
        index in (1u32 << 16)..=u32::MAX,
        tag in 0u64..(1u64 << TaggedIndex::<16>::TAG_BITS),
    ) {
        prop_assert_eq!(TaggedIndex::<16>::pack(index, tag), None);
    }

    #[test]
    fn rejects_out_of_range_tag_width_16(
        index in 0u32..((1u32 << 16) - 1),
        // Strictly OUTSIDE the tag half: the first invalid tag
        // (1 << TAG_BITS) is the value whose high bit the old truncating
        // pack's shift silently dropped, wrapping the tag to 0.
        tag in (1u64 << TaggedIndex::<16>::TAG_BITS)..=u64::MAX,
    ) {
        prop_assert_eq!(TaggedIndex::<16>::pack(index, tag), None);
    }
}

/// The checked pack's bounds check shifts `1u64 << Self::TAG_BITS`; at
/// width 1, `TAG_BITS` is 63 — the closest this crate gets to `u64` shift
/// overflow (`1u64 << 63` is the last representable shift amount, `1u64
/// << 64` would panic/UB). The properties above sample `pack` randomly at
/// width 1 but are not guaranteed to land exactly on this boundary, so it
/// gets its own focused assertion: the last valid tag (`2^63 - 1`) still
/// packs to the exact word, and the first invalid tag (`2^63`, the shift
/// boundary itself) is rejected.
#[test]
fn pack_width_1_tag_boundary_at_shift_63() {
    type T = TaggedIndex<1>;
    assert_eq!(T::TAG_BITS, 63);

    let max_valid_tag = (1u64 << 63) - 1;
    assert_eq!(
        T::pack(0, max_valid_tag),
        Some(max_valid_tag << 1),
        "the last valid tag at the width-1 shift boundary must still pack \
         to the exact word (tag in the high 63 bits, index 0 in the low bit)"
    );

    let first_invalid_tag = 1u64 << 63;
    assert_eq!(
        T::pack(0, first_invalid_tag),
        None,
        "the first invalid tag, exactly at the shift boundary, must be \
         rejected"
    );
}
