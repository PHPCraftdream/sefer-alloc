//! 48-bit tag WRAP-boundary regression tests for [`TaggedIndex`], pinning the
//! `INDEX_BITS = 16` / `TAG_BITS = 48` split across the tag wrap at `2^48`.
//!
//! `tests/stack_unit.rs`'s `pack_unpack_round_trip_16` and
//! `checked_pack_still_accepts_max_tag_but_rejects_the_post_bump_2_pow_48`
//! already pin the width facts and the checked pack's boundary behaviour
//! this file used to duplicate (`split_is_16_48` and `tag_wraps_at_2_pow_48_and_index_survives`
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
            let word = T::pack(idx, tag).expect("in range: idx < INDEX_MASK, tag < 2^TAG_BITS");
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
        let w =
            T::pack(T::empty_index(), tag).expect("empty_index and every swept tag is in range");
        assert!(
            T::is_empty(w),
            "empty_index packed with running tag {tag} must read empty (H-2)"
        );
    }

    // The wrap boundary itself, through the value the stack actually
    // computes there: `push` bumps the observed tag via `wrapping_add(1)`,
    // which at the all-ones tag is exactly `2^TAG_BITS`. The CHECKED pack
    // (review P2-1) now REJECTS that value instead of silently dropping
    // its high bit; the wrap happens inside push, which packs through the
    // crate-private truncating fast path (machine behaviour unchanged).
    // Deriving the post-bump tag through the real bump sequence and
    // confirming the checked pack refuses it is what remains testable at
    // this boundary — a literal repeated `0` in the sweep above cannot
    // show it (round-9 review P4-10).
    let max_tag = (1u64 << T::TAG_BITS) - 1;
    let bumped_tag = max_tag.wrapping_add(1);
    assert_eq!(
        bumped_tag,
        1u64 << T::TAG_BITS,
        "wrapping_add(1) past the all-ones tag yields 2^TAG_BITS — the \
         value `push` hands to its truncating pack after bumping the \
         observed tag"
    );
    assert_eq!(
        T::pack(T::empty_index(), bumped_tag),
        None,
        "the post-bump 2^TAG_BITS tag is out of range: the checked pack \
         refuses it instead of silently wrapping (push's private \
         truncating path performs the actual wrap)"
    );
}
