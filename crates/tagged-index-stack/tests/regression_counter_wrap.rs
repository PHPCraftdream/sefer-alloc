//! 48-bit tag WRAP-boundary regression tests for [`TaggedIndex`], pinning the
//! `INDEX_BITS = 16` / `TAG_BITS = 48` split across the tag wrap at `2^48`.
//!
//! These are the crate-side successors to the extracting allocator's in-tree
//! `tests/regression_counter_wrap.rs` (1): they drive the tag to its maximum
//! (`2^48 - 1`), bump it once so it WRAPS to 0, and assert the packed index
//! round-trips intact across the wrap and that the empty sentinel is never
//! confused with a live index. Non-vacuous: on a narrower tag (e.g. a 32-bit
//! revert) the `2^48 - 1` maximum is unrepresentable, so these values cannot
//! even be expressed pre-widening.

#![cfg(not(loom))]

use tagged_index_stack::TaggedIndex;

type T = TaggedIndex<16>;

#[test]
fn split_is_16_48() {
    assert_eq!(T::INDEX_MASK, 0xFFFF, "index half must be 16 all-ones bits");
    assert_eq!(T::TAG_BITS, 48, "tag half must be 48 bits");
}

#[test]
fn tag_wraps_at_2_pow_48_and_index_survives() {
    let max_tag: u64 = (1u64 << T::TAG_BITS) - 1; // 2^48 - 1
    assert!(
        max_tag > u32::MAX as u64,
        "the 48-bit max tag must exceed the old 32-bit range (the point of the \
         widening; makes this test unrepresentable on a 32-bit tag)"
    );

    let idx: u64 = 0x0ABC; // 2748 — a representative valid index
    let at_max = T::pack(idx, max_tag);
    let (v0, t0) = T::unpack(at_max);
    assert_eq!(v0, idx, "index survives packing at the tag maximum");
    assert_eq!(t0, max_tag, "tag round-trips at its 48-bit maximum");

    // `push` computes `tag.wrapping_add(1)` on the unpacked tag (always < 2^48)
    // and re-packs. At the maximum this is 2^48, whose bit-48 is shifted OUT by
    // `pack`'s `tag << 16`, so the stored-and-re-read tag wraps to 0.
    let bumped = max_tag.wrapping_add(1); // 2^48
    let after = T::pack(idx, bumped);
    let (v1, t1) = T::unpack(after);
    assert_eq!(
        t1, 0,
        "tag at 2^48 - 1 wraps to 0 once bumped and re-packed"
    );
    assert_eq!(
        v1, idx,
        "index is IDENTICAL across the wrap (no tag->index bleed)"
    );
    assert_eq!(v0, v1, "index at the maximum and after the wrap match");
    assert!(
        !T::is_empty(after),
        "a live index with a wrapped (0) tag must NOT read as the empty sentinel"
    );
}

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
    for &tag in &[0u64, 1, 42, (1u64 << T::TAG_BITS) - 1, 0] {
        let w = T::pack(T::empty_index(), tag);
        assert!(
            T::is_empty(w),
            "empty_index packed with running tag {tag} must read empty (H-2)"
        );
    }
}
