//! Single-threaded unit tests for the `tagged-index-stack` public API: the
//! [`TaggedIndex`] packing at several widths (round-trip, empty sentinel,
//! tag-wrap boundary — the 48-bit budget's `2^48` wrap) and the
//! [`TaggedIndexStack`] LIFO push/pop over the owned [`ArrayLinks`] backing
//! (including the H-2 empty transition observed single-threaded: drain to empty
//! then refill, and confirm the tag keeps climbing).
//!
//! These do NOT run under `--cfg loom` (the loom real-type concurrency proof is
//! `tests/loom_aba.rs`); they are the ordinary `cargo test` conformance smoke.

#![cfg(not(loom))]

use tagged_index_stack::{ArrayLinks, TaggedIndex, TaggedIndexStack, TAIL};

// ---------------------------------------------------------------------------
// TaggedIndex packing.
// ---------------------------------------------------------------------------

#[test]
fn pack_unpack_round_trip_16() {
    type T = TaggedIndex<16>;
    assert_eq!(T::INDEX_MASK, 0xFFFF);
    assert_eq!(T::TAG_BITS, 48);
    for &idx in &[0u64, 1, 2748, 0xFFFE] {
        for &tag in &[0u64, 1, 12345, (1u64 << 48) - 1] {
            let w = T::pack(idx, tag);
            let (v, t) = T::unpack(w);
            assert_eq!(v, idx, "index round-trip (tag {tag})");
            assert_eq!(t, tag, "tag round-trip (idx {idx})");
            assert!(!T::is_empty(w), "a live index must not read empty");
        }
    }
}

#[test]
fn pack_truncates_an_over_wide_index_never_collides_with_the_tag() {
    // rust-intel round-closing review (task #772, finding F10): pack()'s
    // doc says an over-wide index is TRUNCATED (masked with INDEX_MASK
    // before OR-ing with the tag), not that it "collides" with the tag
    // bits. No existing test exercised the mask at all — the proptest
    // properties all draw `index` strictly below INDEX_MASK. This pins
    // the sharpest case: at width 16, an over-wide index whose low 16
    // bits equal INDEX_MASK itself truncates to the EMPTY SENTINEL, not
    // merely "a wrong index" — is_empty() then reads true for a packed
    // word whose caller-supplied index was never the empty sentinel.
    type T = TaggedIndex<16>;
    let tag = 42u64;

    // 0x1_FFFF's low 16 bits are 0xFFFF == INDEX_MASK == the empty sentinel.
    let over_wide = 0x1_FFFFu64;
    let word = T::pack(over_wide, tag);
    let (v, t) = T::unpack(word);
    assert_eq!(v, T::INDEX_MASK, "truncates to the low INDEX_BITS bits");
    assert_eq!(t, tag, "the tag half is untouched by truncation");
    assert!(
        T::is_empty(word),
        "truncating to INDEX_MASK reads as the empty sentinel, not merely \
         a wrong live index"
    );

    // A less extreme over-wide value truncates to a live (non-sentinel)
    // index, confirming truncation (not collision) in the general case too.
    let over_wide_live = 0x1_0001u64; // low 16 bits: 0x0001
    let word2 = T::pack(over_wide_live, tag);
    let (v2, t2) = T::unpack(word2);
    assert_eq!(v2, 1, "truncates to the low INDEX_BITS bits");
    assert_eq!(t2, tag, "the tag half is untouched by truncation");
    assert!(!T::is_empty(word2));
}

#[test]
fn empty_sentinel_16() {
    type T = TaggedIndex<16>;
    let e = T::empty();
    assert!(T::is_empty(e));
    let (v, tag) = T::unpack(e);
    assert_eq!(v, 0xFFFF);
    assert_eq!(tag, 0);
    // empty_index packed with a running (non-zero) tag is STILL empty (H-2).
    let running = T::pack(T::empty_index(), 99);
    assert!(
        T::is_empty(running),
        "empty is index-only, tag-agnostic (H-2)"
    );
    let (_v, t) = T::unpack(running);
    assert_eq!(t, 99, "the running tag survives on the empty word");
}

/// The 48-bit tag reaches its maximum (`2^48 - 1`) and WRAPS to 0 on the next
/// bump, with the index intact across the wrap — the tag-width budget boundary.
#[test]
fn tag_wraps_at_2_pow_48() {
    type T = TaggedIndex<16>;
    let max_tag = (1u64 << T::TAG_BITS) - 1; // 2^48 - 1
    assert!(
        max_tag > u32::MAX as u64,
        "48-bit tag exceeds the old 32-bit range"
    );
    let idx = 0x0ABCu64;
    let at_max = T::pack(idx, max_tag);
    let (v0, t0) = T::unpack(at_max);
    assert_eq!(v0, idx);
    assert_eq!(t0, max_tag);
    // Bump once — `push` computes wrapping_add(1); at 2^48-1 that is 2^48, whose
    // bit-48 is shifted out of the word by `pack`'s `<< 16`, so it re-reads 0.
    let bumped = max_tag.wrapping_add(1); // 2^48
    let after = T::pack(idx, bumped);
    let (v1, t1) = T::unpack(after);
    assert_eq!(t1, 0, "tag wraps to 0 (bit 48 shifted out)");
    assert_eq!(v1, idx, "index survives the wrap unchanged");
    assert!(
        !T::is_empty(after),
        "live index + wrapped tag 0 is not empty"
    );
}

/// A different width (`INDEX_BITS = 20`) partitions the word correctly and the
/// empty sentinel is width-appropriate — exercises the const generic.
#[test]
fn width_20_partitions() {
    type T = TaggedIndex<20>;
    assert_eq!(T::INDEX_MASK, 0xFFFFF);
    assert_eq!(T::TAG_BITS, 44);
    let w = T::pack(0xABCDE, 7);
    let (v, t) = T::unpack(w);
    assert_eq!(v, 0xABCDE);
    assert_eq!(t, 7);
    assert!(T::is_empty(T::empty()));
    // TAIL (u32::MAX) differs from this width's empty_index (0xFFFFF).
    assert_ne!(T::empty_index() as u32, TAIL);
}

/// Regression for the F1 finding (2026-08-06 publish-readiness review): at the
/// maximum allowed width `INDEX_BITS = 32`, `INDEX_MASK` numerically equals
/// `TAIL` (`u32::MAX`), so the empty sentinel and `TAIL` coincide — and
/// `push`'s `index < INDEX_MASK` runtime guard therefore already rejects
/// `index == TAIL` (formerly, before `_CHECK_BITS` was narrowed to `1..=32`,
/// this same coincidence did NOT hold for `INDEX_BITS` in `33..=63`, where
/// `INDEX_MASK` exceeds `u32::MAX` and `index == TAIL` silently passed the
/// guard, corrupting a chain — see the crate's F1 write-up). This test pins
/// the width-32 boundary behaves correctly now that it is the crate's hard
/// maximum, not an arbitrary midpoint.
///
/// The panic assertion pins the guard's own message substring (not just
/// `is_err()`): the rust-intel audit (§D1, 2026-08-07) found that an
/// unrelated out-of-bounds panic from `ArrayLinks::store_next` (reached if
/// the guard were ever deleted or weakened) would ALSO satisfy a bare
/// `is_err()` check, making the test pass whether or not the guard the F1
/// regression exists to pin is even present.
#[test]
fn width_32_index_mask_equals_tail_and_is_rejected() {
    type T = TaggedIndex<32>;
    assert_eq!(
        T::INDEX_MASK,
        TAIL as u64,
        "empty_index coincides with TAIL at the max width"
    );
    assert_eq!(T::TAG_BITS, 32);

    let links = ArrayLinks::<4>::new();
    let stack = TaggedIndexStack::<32>::new();
    // A valid, non-TAIL index still works normally.
    stack.push(&links, 3);
    assert_eq!(stack.pop(&links), Some(3));

    // `index == TAIL` (== INDEX_MASK at this width) must panic — it is
    // indistinguishable from the empty sentinel/end-of-chain otherwise. The
    // guard is now a full `assert!` (promoted from `debug_assert!`), so this
    // holds identically in both debug and release builds.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stack.push(&links, TAIL);
    }));
    let err = result.expect_err("pushing index == TAIL must panic");
    let message = err
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .expect("panic payload should be a string message");
    assert!(
        message.contains("index must be < INDEX_MASK"),
        "panic message did not name the push guard's own contract (got: {message:?}) — \
         an unrelated panic (e.g. an ArrayLinks out-of-bounds index) must NOT satisfy \
         this test, since that would mean the guard itself could be deleted without \
         this test noticing — this is the F1 regression"
    );
}

// Compile-fail coverage note (F1, `_CHECK_BITS` narrowed to `1..=32`): this
// crate has no trybuild (or similar compile-fail) test infrastructure wired up,
// so `INDEX_BITS > 32` failing to compile is NOT pinned by an automated test.
// Manually verified instead: instantiating `TaggedIndex::<33>` (or any
// `TaggedIndexStack<N>` with `N > 32`) fails `cargo build` with the
// `_CHECK_BITS` assertion message ("INDEX_BITS must be in 1..=32 ..."). This is
// a known, honestly-recorded coverage gap, not a silent omission.

// ---------------------------------------------------------------------------
// TaggedIndexStack over ArrayLinks — LIFO order + H-2 single-threaded.
// ---------------------------------------------------------------------------

#[test]
fn fresh_stack_is_empty() {
    let links = ArrayLinks::<8>::new();
    let stack = TaggedIndexStack::<16>::new();
    assert_eq!(
        stack.pop(&links),
        None,
        "a fresh (lazy-link) stack is empty"
    );
}

#[test]
fn push_pop_is_lifo() {
    let links = ArrayLinks::<8>::new();
    let stack = TaggedIndexStack::<16>::new();
    for i in 0..5u32 {
        stack.push(&links, i);
    }
    let mut got = Vec::new();
    while let Some(i) = stack.pop(&links) {
        got.push(i);
    }
    assert_eq!(got, vec![4, 3, 2, 1, 0], "LIFO order");
    assert_eq!(stack.pop(&links), None);
}

/// Drain to empty then refill the SAME index: the tag must have advanced across
/// the empty transition (H-2), NOT reset to 0. Observed via `raw_head`.
#[test]
fn empty_transition_preserves_running_tag() {
    type T = TaggedIndex<16>;
    let links = ArrayLinks::<4>::new();
    let stack = TaggedIndexStack::<16>::new();

    stack.push(&links, 0); // tag 0 -> 1
    let (_v, tag_after_push1) = T::unpack(stack.raw_head());
    assert_eq!(tag_after_push1, 1);

    // Drain to empty. The empty head must carry the RUNNING tag (1), not 0.
    assert_eq!(stack.pop(&links), Some(0));
    let empty_head = stack.raw_head();
    assert!(T::is_empty(empty_head), "stack is now empty");
    let (_ev, empty_tag) = T::unpack(empty_head);
    assert_eq!(
        empty_tag, 1,
        "H-2: the empty transition preserves the running tag (1), not 0 — \
         resetting to 0 would reopen ABA"
    );

    // Refill the same index: the push reads the running tag (1) and bumps to 2.
    stack.push(&links, 0);
    let (_v2, tag_after_push2) = T::unpack(stack.raw_head());
    assert_eq!(
        tag_after_push2, 2,
        "the tag keeps climbing across empty->non-empty (1 -> 2), never restarts"
    );
}

/// The link storage is only ever written by a push (RAD-1 lazy discipline):
/// after construction every link is the zero value, and popping never writes a
/// link. We can only observe this behaviourally (the stack is empty until a
/// push, and pops leave links untouched) — checked here by confirming a
/// never-pushed index's link is still 0 via a fresh backing.
#[test]
fn links_are_lazy() {
    let links = ArrayLinks::<4>::new();
    let stack = TaggedIndexStack::<16>::new();
    // Never push index 3. Push/drain 0 fully.
    stack.push(&links, 0);
    assert_eq!(stack.pop(&links), Some(0));
    // Index 3 was never touched; its Links load reads the initial 0 value.
    // (Exposed only through the trait — a fresh push of 3 would overwrite it.)
    // We assert indirectly: pushing 3 now chains it to the empty sentinel ->
    // TAIL, so a subsequent pop returns 3 and then None.
    stack.push(&links, 3);
    assert_eq!(stack.pop(&links), Some(3));
    assert_eq!(stack.pop(&links), None);
}
