//! Tiny-tag regression oracle for the P1-1 fix (the full-wrap exclusive-
//! issuance hole from run-8's review, closed by making the tag strictly
//! monotonic and sealing at [`TaggedIndex::TAG_MAX`]): single-threaded,
//! non-loom conformance tests seeding the tag NEAR the ceiling via
//! `with_tag_for_test` at the REAL tag width — never a `TAG_BITS`-reducing
//! cfg (this crate's Option-4 tiny-tag oracle convention).
//!
//! Concurrent evidence that the seal is what actually closes the P1-1
//! counterexample (not just that the ceiling can be reached) lives in
//! `tests/loom_aba.rs`'s "(h) Tiny-tag seal" section; this file covers the
//! single-threaded API contract the seal establishes: the exact
//! `Ok, Ok, Err` sequence at the ceiling, `pushes_remaining()`'s readback,
//! that a refused push has no observable side effect, that pops keep
//! working after a seal, and that the seal is permanent.
//!
//! These do NOT run under `--cfg loom` (matching `tests/stack_unit.rs`, whose
//! ordinary conformance tests this complements) — the concurrent seal-engagement
//! proof is `tests/loom_aba.rs`'s job, not this file's.

#![cfg(not(loom))]

use tagged_index_stack::{ArrayIndexStack, TagExhausted, TaggedIndex};

type T = TaggedIndex<16>;

/// Pins the ceiling's off-by-one arithmetic directly against `pack`: exactly
/// `TAG_MAX` is still a packable tag, `TAG_MAX + 1` is not — the boundary
/// [`TaggedIndex::TAG_MAX`]'s own doc states.
#[test]
fn tag_max_is_the_exact_pack_ceiling() {
    assert!(
        T::pack(0, T::TAG_MAX).is_some(),
        "TAG_MAX itself must still be a packable tag"
    );
    assert!(
        T::pack(0, T::TAG_MAX + 1).is_none(),
        "TAG_MAX + 1 must be rejected by the checked pack — one past the ceiling"
    );
}

/// `TAG_MAX == 2^TAG_BITS - 1` at width 16, pinned as a `const` assertion so
/// a future arithmetic regression in the constant's definition is a compile
/// error, not a runtime surprise.
const _: () = assert!(T::TAG_MAX == (1u64 << T::TAG_BITS) - 1);

/// The core seal sequence: seed `TAG_MAX - 2` (2 pushes of headroom), three
/// pushes onto a fresh 1-slot chain-building sequence produce exactly
/// `Ok, Ok, Err(TagExhausted)`; `pushes_remaining()` reads `2, 1, 0, 0`
/// (readback BEFORE each push, so the count observed before the first push
/// is 2); `raw_head()` is byte-identical across the refusal (a first-attempt
/// refusal has no side effect at all — the check runs before `store_next`);
/// after the seal, pops drain every successfully pushed index and then
/// return `None`; and a further push after the full drain still returns
/// `Err` (the seal is permanent, not lifted by draining).
#[cfg(any(feature = "test-internals", loom))]
#[test]
fn seal_sequence_ok_ok_err_with_permanent_seal_after_drain() {
    let stack = ArrayIndexStack::<16, 4>::with_tag_for_test(T::TAG_MAX - 2);

    assert_eq!(
        stack.pushes_remaining(),
        2,
        "seeded 2 pushes short of the ceiling"
    );

    // SAFETY: freshly-seeded empty head (domain 0..4); index 0 is in-domain and this is its first push.
    assert!(
        unsafe { stack.push(0) }.is_ok(),
        "1st push: 2 remaining -> Ok"
    );
    assert_eq!(stack.pushes_remaining(), 1);

    // SAFETY: index 1 is in-domain and has never been pushed.
    assert!(
        unsafe { stack.push(1) }.is_ok(),
        "2nd push: 1 remaining -> Ok"
    );
    assert_eq!(
        stack.pushes_remaining(),
        0,
        "the tag is now exactly TAG_MAX"
    );

    let head_before_refusal = stack.raw_head();
    // SAFETY: index 2 is in-domain and has never been pushed -- the seal
    // refuses it before any side effect (the check runs before store_next).
    let third = unsafe { stack.push(2) };
    assert_eq!(
        third,
        Err(TagExhausted),
        "3rd push: 0 remaining (tag == TAG_MAX) -> Err(TagExhausted)"
    );
    assert_eq!(
        stack.pushes_remaining(),
        0,
        "a refusal must not change the remaining budget"
    );
    assert_eq!(
        stack.raw_head(),
        head_before_refusal,
        "a FIRST-ATTEMPT refusal has no observable side effect at all: the \
         TAG_MAX check runs before store_next and before the head CAS, so \
         the head word must be byte-identical across the refusal"
    );

    // Pops are unaffected by the seal: the two successfully pushed indices
    // drain in LIFO order, then the stack reports empty.
    assert_eq!(stack.pop(), Some(1), "LIFO: index 1 was pushed last");
    assert_eq!(stack.pop(), Some(0), "then index 0");
    assert_eq!(
        stack.pop(),
        None,
        "drained -- and the seal never blocks pops"
    );

    // The seal is PERMANENT: a push after the full drain still refuses,
    // never resetting just because the stack is now empty (no reset API --
    // see StackHead's "Sealing is permanent" doc section).
    // SAFETY: index 2 is in-domain; this call's refusal is the test's subject.
    let after_drain = unsafe { stack.push(2) };
    assert_eq!(
        after_drain,
        Err(TagExhausted),
        "the seal survives a full drain -- pushes stay refused permanently"
    );
    assert_eq!(
        stack.pushes_remaining(),
        0,
        "pushes_remaining stays 0 forever once sealed"
    );
}

/// `pushes_remaining()`'s readback at every step of a slightly larger
/// budget, phrased as the exact sequence the module doc's summary
/// (`2, 1, 0, 0`) describes, isolated from the `raw_head`/drain assertions
/// above so a failure here pinpoints the counter specifically.
#[cfg(any(feature = "test-internals", loom))]
#[test]
fn pushes_remaining_counts_down_to_zero_and_stays_there() {
    let stack = ArrayIndexStack::<16, 4>::with_tag_for_test(T::TAG_MAX - 2);
    let mut remaining_before_each_push = Vec::new();

    for i in 0..3u32 {
        remaining_before_each_push.push(stack.pushes_remaining());
        // SAFETY: freshly-seeded empty head (domain 0..4); each index 0..3
        // is DISTINCT and pushed exactly once (never popped), so none is
        // ever re-pushed while live -- the 3rd push (i == 2) is refused by
        // the tag-exhaustion seal, not a liveness violation.
        let _ = unsafe { stack.push(i) };
    }
    // A 4th observation, after the loop's 3rd (refused) push attempt.
    remaining_before_each_push.push(stack.pushes_remaining());

    assert_eq!(
        remaining_before_each_push,
        vec![2, 1, 0, 0],
        "pushes_remaining must read 2 (before push 1), 1 (before push 2), \
         0 (before push 3, already sealed), 0 (after the refused push 3)"
    );
}
