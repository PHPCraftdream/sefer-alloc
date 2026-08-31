//! 48-bit tag WRAP-boundary regression tests for [`TaggedIndex`], pinning the
//! `INDEX_BITS = 16` / `TAG_BITS = 48` split across the tag wrap at `2^48`.
//!
//! `tests/stack_unit.rs`'s `pack_unpack_round_trip_16` and
//! `tag_wraps_at_2_pow_48` already pin the width/wrap facts this file used to
//! duplicate (`split_is_16_48` and `tag_wraps_at_2_pow_48_and_index_survives`
//! were removed as exact duplicates — round-4 review P4-8). What remains
//! here is the coverage those two tests do NOT provide: a parametrized sweep
//! over multiple (index, tag) pairs confirming the empty sentinel is never
//! confused with a live one, including the pool-cap-relevance argument, and
//! a check that the empty sentinel stays unambiguous at multiple tags
//! spanning the wrap boundary specifically. Non-vacuous: on a narrower tag
//! (e.g. a 32-bit revert) the `2^48 - 1` maximum is unrepresentable, so
//! these values cannot even be expressed pre-widening.

#![cfg(not(loom))]

use tagged_index_stack::TaggedIndex;

type T = TaggedIndex<16>;

#[test]
fn empty_sentinel_never_collides_with_a_live_index() {
    let empty = T::empty();
    assert!(T::is_empty(empty), "the empty sentinel reads as empty");
    let (sentinel_idx, sentinel_tag) = T::unpack(empty);
    assert_eq!(
        sentinel_idx,
        T::INDEX_MASK,
        "empty sentinel index is INDEX_MASK"
    );
    assert_eq!(sentinel_tag, 0, "bootstrap empty sentinel tag is 0");

    // A representative pool cap: 4096. The sentinel (0xFFFF = 65535) is far
    // above it, so it can never be a real slot index.
    const CAP: u64 = 4096;
    const _: () = assert!(
        T::INDEX_MASK >= CAP,
        "the empty sentinel index must be >= the pool cap so it is a non-index"
    );

    for &idx in &[0u64, 1, CAP - 1] {
        for &tag in &[0u64, 1, (1u64 << T::TAG_BITS) - 1] {
            let word = T::pack(idx, tag);
            assert!(
                !T::is_empty(word),
                "valid index {idx} (tag {tag}) is not empty"
            );
            let (v, t) = T::unpack(word);
            assert_eq!(v, idx, "index {idx} round-trips (tag {tag})");
            assert_eq!(t, tag, "tag {tag} round-trips (index {idx})");
        }
    }
}

/// The empty word carrying a NON-zero running tag (the H-2 shape) is still
/// unambiguously empty, across the wrap boundary.
#[test]
fn empty_word_with_running_tag_reads_empty_across_wrap() {
    for &tag in &[0u64, 1, 42, (1u64 << T::TAG_BITS) - 1] {
        let w = T::pack(T::empty_index(), tag);
        assert!(
            T::is_empty(w),
            "empty_index packed with running tag {tag} must read empty (H-2)"
        );
    }

    // The wrap itself, through the operations the stack actually uses:
    // `push` bumps the observed tag via `wrapping_add(1)` and hands the
    // result to `pack`, whose shift drops every tag bit at or above
    // 2^TAG_BITS — so the all-ones tag restarts at 0 there, not inside
    // `wrapping_add` itself. A literal repeated `0` in the sweep above
    // cannot show that (round-9 review P4-10) — derive the post-wrap tag
    // through the real bump-then-pack sequence and confirm the wrapped
    // word is still unambiguously empty.
    let max_tag = (1u64 << T::TAG_BITS) - 1;
    let bumped_tag = max_tag.wrapping_add(1);
    assert_eq!(
        bumped_tag,
        1u64 << T::TAG_BITS,
        "wrapping_add(1) past the all-ones tag yields 2^TAG_BITS — the \
         value `push` hands to pack after bumping the observed tag"
    );
    let w = T::pack(T::empty_index(), bumped_tag);
    let (_, packed_tag) = T::unpack(w);
    assert_eq!(
        packed_tag, 0,
        "pack's shift drops the 2^TAG_BITS high bit, restarting the tag \
         at 0 — the actual wrap boundary `push` crosses"
    );
    assert!(
        T::is_empty(w),
        "empty_index packed with the post-wrap tag (0, derived from the \
         all-ones tag via wrapping_add plus pack's truncating shift) must \
         read empty (H-2)"
    );
}
