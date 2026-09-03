//! Single-threaded unit tests for the `tagged-index-stack` public API: the
//! [`TaggedIndex`] packing at several widths (round-trip, empty sentinel,
//! and the checked pack's acceptance/rejection boundaries — production
//! `push` never wraps the 48-bit tag: it SEALS at `TAG_MAX` and returns
//! `Err(TagExhausted)` before the tag could ever reach `2^48`; this file
//! pins that ceiling by rejection through the checked `pack`) and the
//! sentinel-boundary sweep folded in from the retired
//! `tests/regression_counter_wrap.rs`, plus the
//! [`ArrayIndexStack`] fused head+links LIFO push/pop (including the H-2
//! empty transition observed single-threaded: drain to empty then refill,
//! and confirm the tag keeps climbing).
//!
//! These do NOT run under `--cfg loom` (the loom real-type concurrency proof is
//! `tests/loom_aba.rs`); they are the ordinary `cargo test` conformance smoke.
//!
//! Three white-box probes below (`empty_transition_preserves_running_tag`,
//! `links_are_lazy`, `default_stack_head_behaves_like_new`) read through the
//! `test-internals`/loom-gated raw accessors (`raw_head` /
//! `load_next_for_test`) and carry the same
//! `#[cfg(any(feature = "test-internals", loom))]` gate, so plain
//! default-feature `cargo test` runs compile them out; CI runs this file
//! under `--features test-internals` to execute them (the same per-file row
//! shape `tests/threaded_conservation.rs`'s activation-oracle assertions
//! already use).

#![cfg(not(loom))]

use tagged_index_stack::{
    ArrayIndexStack, ArrayLinks, StackHead, StackOps, StackStorage, TaggedIndex, TAIL,
};

// Compile-time pin: all three public types must stay auto-`Send +
// Sync`. Every field of all three is an atomic today, so they derive the
// traits for free — but their entire purpose is lock-free CROSS-THREAD
// sharing, and a future non-auto field (a `Cell`, a raw pointer, ...) would
// silently drop one or both traits with no compile error anywhere obvious.
// This const makes that a hard compile error the moment it happens. Widths
// 16 and 4 are this file's conventional choices (see the existing push/pop
// tests below). Both fns are `const fn` and `_check()` is actually CALLED in
// the const initializer: that both forces the trait bounds to be checked and
// keeps the dead-code lint from firing on a helper that is never otherwise
// used.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    const fn _check() {
        assert_send_sync::<StackHead<16>>();
        assert_send_sync::<ArrayIndexStack<16, 4>>();
        assert_send_sync::<ArrayLinks<4>>();
    }
    _check();
};

// ---------------------------------------------------------------------------
// TaggedIndex packing.
// ---------------------------------------------------------------------------

#[test]
fn pack_unpack_round_trip_16() {
    type T = TaggedIndex<16>;
    assert_eq!(T::INDEX_MASK, 0xFFFF);
    assert_eq!(T::TAG_BITS, 48);
    for &idx in &[0u32, 1, 2748, 0xFFFE] {
        for &tag in &[0u64, 1, 12345, (1u64 << 48) - 1] {
            let w = T::pack(idx, tag).expect("in range: idx < INDEX_MASK, tag < 2^TAG_BITS");
            let (v, t) = T::unpack(w);
            assert_eq!(v, idx, "index round-trip (tag {tag})");
            assert_eq!(t, tag, "tag round-trip (idx {idx})");
            assert!(!T::is_empty(w), "a live index must not read empty");
        }
    }
}

/// The checked `pack` REJECTS an over-wide index with `None` — the
/// fail-closed checked `pack` (it replaced an earlier truncating `pack`
/// whose silent masking turned `0x1_FFFF` into the EMPTY SENTINEL (its low 16
/// bits equal `INDEX_MASK`) and `0x1_0001` into the unrelated live index
/// `1`. The truncating behaviour survives only as the crate-private
/// `pack_truncating` fast path used by `push_index`/`pop_index`, whose
/// inputs are guard-proven in range.
#[test]
fn pack_rejects_an_over_wide_index_instead_of_truncating_it() {
    type T = TaggedIndex<16>;
    let tag = 42u64;

    // 0x1_FFFF's low 16 bits are 0xFFFF == INDEX_MASK == the empty
    // sentinel: the old truncating pack turned this into a word that
    // is_empty() read as EMPTY. The checked pack refuses it instead.
    assert_eq!(
        T::pack(0x1_FFFF, tag),
        None,
        "an over-wide index whose low bits are the empty sentinel must be \
         rejected, not silently masked into it"
    );
    // A less extreme over-wide value would have truncated to the live
    // index 1 — also rejected now.
    assert_eq!(
        T::pack(0x1_0001, tag),
        None,
        "an over-wide index must be rejected, not truncated to its low bits"
    );
    assert_eq!(T::pack(u32::MAX, tag), None, "far out-of-range index");
}

/// The CHECKED `pack`'s boundary behaviour, pinned with literal expected
/// words (an independent hand-computed oracle, not a comparison against
/// `pack` itself): an in-range pair packs to the exact
/// `(tag << INDEX_BITS) | index` word — `INDEX_MASK` itself included,
/// because pack's acceptance boundary is `< 2^INDEX_BITS`, NOT push's
/// stricter `< INDEX_MASK` reserve-sentinel bound (packing the empty
/// index with a tag is the legitimate H-2 shape) — and the first
/// out-of-range value on either half is rejected with `None` exactly
/// where the old truncating `pack` silently produced a different
/// valid-looking word.
#[test]
fn pack_rejects_out_of_range_halves_and_accepts_the_full_index_range() {
    type T = TaggedIndex<16>;

    for &(idx, tag, word) in &[
        (0u32, 0u64, 0u64),
        (1, 1, (1u64 << 16) | 1),
        (2748, 42, (42u64 << 16) | 2748),
        (T::INDEX_MASK as u32, 7, (7u64 << 16) | T::INDEX_MASK),
        (
            0xFFFE,
            (1u64 << T::TAG_BITS) - 1,
            (((1u64 << T::TAG_BITS) - 1) << 16) | 0xFFFE,
        ),
    ] {
        assert_eq!(
            T::pack(idx, tag),
            Some(word),
            "in-range (index {idx}, tag {tag}) must pack to the exact word"
        );
    }

    // First out-of-range index: exactly `1 << INDEX_BITS` — the value the
    // old truncating pack masked to 0, a DIFFERENT valid index.
    assert_eq!(T::pack(1u32 << 16, 7), None, "first invalid index");
    // Farther out of range, including the value whose low bits are all
    // ones (the old pack truncated it to the empty sentinel).
    assert_eq!(T::pack(u32::MAX, 7), None, "far out-of-range index");
    assert_eq!(
        T::pack(0x1_FFFF, 7),
        None,
        "over-wide index that the old pack truncated to the empty sentinel"
    );

    // First out-of-range tag: exactly `1 << TAG_BITS` (2^48 at width 16) —
    // the value whose shifted-out high bit the old pack silently dropped,
    // restarting the tag at 0. The checked pack refuses it; `push` itself
    // never reaches this value in production — its seal check refuses
    // (`Err(TagExhausted)`) once the observed tag hits `TAG_MAX`, before
    // ever bumping past it.
    assert_eq!(T::pack(9, 1u64 << T::TAG_BITS), None, "first invalid tag");
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
    let running = T::pack(T::empty_index(), 99).expect("empty_index and 99 are both in range");
    assert!(
        T::is_empty(running),
        "empty is index-only, tag-agnostic (H-2)"
    );
    let (_v, t) = T::unpack(running);
    assert_eq!(t, 99, "the running tag survives on the empty word");
}

/// The 48-bit tag reaches its maximum (`2^48 - 1`) and still packs; the
/// value one PAST it (`2^48`, what `wrapping_add(1)` would produce if
/// applied to `TAG_MAX`) is REJECTED by the checked `pack` — the
/// fail-closed checked pack contract. In production `push` never computes
/// this value: its seal check observes `tag == TAG_MAX` and returns
/// `Err(TagExhausted)` BEFORE ever bumping, so the tag never wraps —
/// `2^48` is pinned here only as the checked pack's rejection boundary,
/// not as a value `push` ever produces.
#[test]
fn checked_pack_still_accepts_max_tag_but_rejects_the_post_bump_2_pow_48() {
    type T = TaggedIndex<16>;
    let max_tag = (1u64 << T::TAG_BITS) - 1; // 2^48 - 1
    assert!(
        max_tag > u32::MAX as u64,
        "48-bit tag exceeds the old 32-bit range"
    );
    let idx = 0x0ABCu32;
    let at_max = T::pack(idx, max_tag).expect("2^48 - 1 is the last valid tag");
    let (v0, t0) = T::unpack(at_max);
    assert_eq!(v0, idx);
    assert_eq!(t0, max_tag);
    // Compute the value one bump PAST TAG_MAX (2^48). `push` itself never
    // reaches this: its seal check returns `Err(TagExhausted)` the moment
    // it observes `tag == TAG_MAX`, before ever calling `wrapping_add`.
    // This pins the checked pack's own rejection boundary in isolation,
    // independent of push.
    let bumped = max_tag.wrapping_add(1); // 2^48
    assert_eq!(
        T::pack(idx, bumped),
        None,
        "the post-bump 2^48 tag is out of range and must be rejected by \
         the checked pack (push never reaches it: it seals at TAG_MAX with \
         Err(TagExhausted) first)"
    );
}

/// A different width (`INDEX_BITS = 12`) partitions the word correctly and the
/// empty sentinel is width-appropriate — exercises the const generic. (Width
/// 20 was retired when `_CHECK_BITS` narrowed the legal range to `1..=16`;
/// 12 keeps the same shape at a mid-range legal width, distinct from this
/// file's other widths 1 and 16.)
#[test]
fn width_12_partitions() {
    type T = TaggedIndex<12>;
    assert_eq!(T::INDEX_MASK, 0xFFF);
    assert_eq!(T::TAG_BITS, 52);
    let w = T::pack(0xABC, 7).expect("0xABC < 0xFFF, 7 < 2^52");
    let (v, t) = T::unpack(w);
    assert_eq!(v, 0xABC);
    assert_eq!(t, 7);
    assert!(T::is_empty(T::empty()));
    // TAIL (u32::MAX) differs from this width's empty_index (0xFFF).
    assert_ne!(T::empty_index(), TAIL);
}

/// The old legal maximum `INDEX_BITS = 32` made `INDEX_MASK` numerically
/// equal `TAIL` (`u32::MAX`), collapsing `push`'s two reject-purposes
/// (out-of-range and reject-`TAIL`) into one value; the former
/// `width_32_index_mask_equals_tail_and_is_rejected` test pinned that
/// coincidence (and `push` panicking on `index == TAIL` because of it).
/// The `_CHECK_BITS` cap is now `1..=16`, so the coincidence is structurally
/// impossible at EVERY legal width (`INDEX_MASK <= 0xFFFF`) — pinned here at
/// the MAXIMUM legal width. The guard's panic path and its exact message
/// remain pinned by `width_16_push_rejects_index_mask_itself` in
/// `tests/push_guard_track_caller.rs`, which
/// rejects the equally out-of-range `INDEX_MASK` itself.
#[test]
fn max_legal_width_index_mask_never_equals_tail() {
    type T = TaggedIndex<16>;
    assert_eq!(T::INDEX_MASK, 0xFFFF, "width 16 is the maximum legal width");
    assert_ne!(
        T::INDEX_MASK,
        TAIL as u64,
        "INDEX_MASK must never coincide with TAIL at any legal width — the \
         1..=16 cap makes the old width-32 coincidence impossible"
    );
}

/// 48-bit tag SEAL-boundary coverage for [`TaggedIndex`] (folded in from the
/// retired `tests/regression_counter_wrap.rs`): pins the
/// `INDEX_BITS = 16` / `TAG_BITS = 48` split across the tag's `TAG_MAX`
/// ceiling (`2^48 - 1`; push seals here rather than wrapping to `2^48`).
/// [`pack_unpack_round_trip_16`] and
/// [`checked_pack_still_accepts_max_tag_but_rejects_the_post_bump_2_pow_48`]
/// above already pin the width facts and the checked pack's boundary
/// behaviour (the older `split_is_16_48` and
/// `tag_wraps_at_2_pow_48_and_index_survives` were removed as exact
/// duplicates). What the two tests below provide is the coverage those do
/// NOT: a parametrized sweep over multiple (index, tag) pairs confirming the
/// empty sentinel is never confused with a live one, including the
/// pool-cap-relevance argument, and a check that the empty sentinel stays
/// unambiguous at multiple tags spanning the `TAG_MAX` ceiling specifically.
/// Non-vacuous: on a narrower tag (e.g. a 32-bit revert) the `2^48 - 1`
/// maximum is unrepresentable, so these values cannot even be expressed
/// pre-widening.
#[test]
fn empty_sentinel_never_collides_with_a_live_index() {
    type T = TaggedIndex<16>;
    let empty = T::empty();
    assert!(T::is_empty(empty), "the empty sentinel reads as empty");
    let (sentinel_idx, sentinel_tag) = T::unpack(empty);
    assert_eq!(
        sentinel_idx,
        T::INDEX_MASK as u32,
        "empty sentinel index is INDEX_MASK"
    );
    assert_eq!(sentinel_tag, 0, "bootstrap empty sentinel tag is 0");

    // A representative pool cap: 4096. The sentinel (0xFFFF = 65535) is far
    // above it, so it can never be a real slot index.
    const CAP: u32 = 4096;
    const _: () = assert!(
        T::INDEX_MASK >= CAP as u64,
        "the empty sentinel index must be >= the pool cap so it is a non-index"
    );

    for &idx in &[0u32, 1, CAP - 1] {
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

/// The empty word carrying a NON-zero running tag (the H-2 shape) stays
/// unambiguously empty at tags spanning up to the `TAG_MAX` ceiling (this
/// test's name predates the P1-1 seal fix; the boundary is now a seal, not
/// a wrap — see the body for the current framing).
#[test]
fn empty_word_with_running_tag_reads_empty_across_wrap() {
    type T = TaggedIndex<16>;
    for &tag in &[0u64, 1, 42, (1u64 << T::TAG_BITS) - 1] {
        let w =
            T::pack(T::empty_index(), tag).expect("empty_index and every swept tag is in range");
        assert!(
            T::is_empty(w),
            "empty_index packed with running tag {tag} must read empty (H-2)"
        );
    }

    // The TAG_MAX ceiling itself, through the value a wrap WOULD compute
    // there: `wrapping_add(1)` on the all-ones tag is exactly
    // `2^TAG_BITS`. The CHECKED pack REJECTS that value. In production
    // `push` never computes it: its seal check returns
    // `Err(TagExhausted)` the moment the observed tag equals `TAG_MAX`,
    // before ever calling `wrapping_add` — the tag never wraps. Deriving
    // the post-bump value through the real bump arithmetic and confirming
    // the checked pack refuses it is what remains testable at this
    // boundary — a literal repeated `0` in the sweep above cannot show
    // it, because a repeated `0` never reaches the boundary.
    let max_tag = (1u64 << T::TAG_BITS) - 1;
    let bumped_tag = max_tag.wrapping_add(1);
    assert_eq!(
        bumped_tag,
        1u64 << T::TAG_BITS,
        "wrapping_add(1) past the all-ones tag yields 2^TAG_BITS — the \
         value push's seal check exists precisely to prevent ever reaching \
         (it refuses at TAG_MAX, before this bump would run)"
    );
    assert_eq!(
        T::pack(T::empty_index(), bumped_tag),
        None,
        "the post-bump 2^TAG_BITS tag is out of range: the checked pack \
         refuses it, pinning the boundary push's seal check keeps \
         production from ever reaching"
    );
}

/// [`ArrayLinks::load_next`] panics if `index >= N` (this backing's own,
/// narrower bound — independent of `INDEX_BITS`). Unlike
/// `width_16_push_rejects_index_mask_itself` in
/// `tests/push_guard_track_caller.rs` (which uses
/// `catch_unwind` plus an explicit message assertion), this is a plain
/// `#[should_panic(expected = ...)]`: the expected substring is Rust's own
/// slice-indexing panic text (`self.next[index as usize]` in
/// `ArrayLinks::load_next`), which is unambiguous enough on its own that the
/// heavier `catch_unwind` pattern is not needed here.
#[test]
#[should_panic(expected = "index out of bounds")]
fn array_links_load_next_panics_on_index_out_of_range() {
    let links = ArrayLinks::<4>::new();
    let _ = links.load_next(4); // valid range is 0..=3
}

/// [`ArrayLinks::store_next`] panics if `index >= N` — the same bound as
/// `load_next` above, documented alongside it in `src/imp.rs`. Reached via
/// the worked example in `push_index`'s own `# Panics` section: an
/// `ArrayIndexStack::<16, 4>` accepts indices up to 65534 by `INDEX_BITS`,
/// but its `ArrayLinks<4>` links hold only `0..=3`, so
/// [`StackOps::push_index`]'s `store_next` call (which runs before the head
/// CAS) panics on the links layer's own, narrower bound before the stack's
/// wider `INDEX_BITS` guard is ever in play.
#[test]
#[should_panic(expected = "index out of bounds")]
fn array_links_store_next_panics_on_index_out_of_range() {
    let stack = ArrayIndexStack::<16, 4>::new();
    // SAFETY: DELIBERATE contract violation under test — index 5 is outside the ArrayLinks<4> domain
    // (0..4); the links-layer panic it triggers is this test's subject.
    // Result discarded: the ArrayLinks bound panics before push_index_impl
    // would ever return a value here.
    let _ = unsafe { stack.push(5) }; // valid for the stack (< INDEX_MASK), not for ArrayLinks<4>
}

/// [`pop_index`]'s clause-4 guard fires when a [`StackStorage`] implementor
/// returns a `next` value that is neither `TAIL` nor a valid index — a
/// caller-contract violation `pop_index` cannot otherwise detect (see the
/// crate docs' "Storage requirement" section on `StackStorage`). A tiny
/// custom implementor whose `load_next` always answers `INDEX_MASK` (a value
/// that is not `TAIL` and not `< INDEX_MASK`) triggers it directly.
///
/// Release-active: promoted from `debug_assert!` to an
/// unconditional `#[cold]` panic helper mirroring
/// [`push_index`]'s own `index < INDEX_MASK` guard, once an out-of-tree A/B
/// measured the release-active cost at ≈ 0 ns (see CHANGELOG.md). Unlike its
/// predecessor, this test needs no `#[cfg(debug_assertions)]` gate: the
/// panic now fires identically under `cargo test -p tagged-index-stack
/// --release` (the configuration `.github/workflows/ci.yml`'s `test
/// workspace members` job actually uses for this crate) and under the
/// dev/test profile default.
struct AlwaysInvalidStorage {
    head: StackHead<16>,
}

// SAFETY: DELIBERATE contract violation — clause 4 (load_next must return
// only TAIL or a currently-valid index): this implementor deliberately
// answers INDEX_MASK, an out-of-range value, to fire pop_index's clause-4
// guard.
unsafe impl StackStorage<16> for AlwaysInvalidStorage {
    unsafe fn head(&self) -> &StackHead<16> {
        &self.head
    }

    unsafe fn load_next(&self, _index: u32) -> u32 {
        // Neither TAIL nor a valid index at width 16 (INDEX_MASK == 0xFFFF):
        // exactly the shape pop_index's clause-4 guard exists to catch.
        TaggedIndex::<16>::INDEX_MASK as u32
    }

    unsafe fn store_next(&self, _index: u32, _next: u32) {}
}

#[test]
#[should_panic(expected = "neither TAIL")]
fn pop_rule_4_guard_fires_on_invalid_next_from_backing() {
    let storage = AlwaysInvalidStorage {
        head: StackHead::new(),
    };
    // SAFETY: DELIBERATE double contract violation, both intentional to this
    // fixture: (1) clause 4 (valid answers) — load_next always answers
    // INDEX_MASK, neither TAIL nor a valid index, which is the guard this
    // test targets; (2) clause 6 (declared link domain) — this storage
    // declares NO domain at all (no backing cells; store_next is a no-op),
    // and an UNDECLARED domain is not proof push_index's clause-1 (link
    // domain) is discharged — clause 6 requires the implementor to define
    // and document a domain it owns a dedicated cell for, so the absence of
    // one is a second, separate violation this fixture also commits, not a
    // vacuous non-issue. index 0 has never been pushed through this
    // binding, so push_index's clause-2 (liveness) alone is honestly held.
    unsafe { storage.push_index(0) }.expect("fresh head has tag budget"); // real push, so the head is non-empty
    let _ = storage.pop_index(); // load_next() always answers INDEX_MASK -> guard fires
}

/// The self-loop guard's SIMPLEST real-world trigger, pinned without any
/// custom implementor, shared storage, or foreign backing: a plain
/// [`ArrayIndexStack`] whose CURRENT head is double-pushed. [`push_index`]
/// writes the current head's index into `next[index]` before its CAS, so
/// re-pushing the live head writes the index's own link back to itself, and
/// [`pop_index`]'s clause-4 guard panics on the FIRST pop — unlike the
/// zero-initialised-foreign-backing shapes in
/// `tests/custom_storage_impl.rs`, which fire on the second. This pins the
/// guard against a caller-contract violation OUTSIDE the shared-storage
/// hazard class entirely, so a future narrowing of the detector to that
/// class's specific sub-shape (e.g. only a zero-initialised backing) fails
/// here too. See `StackStorage`'s "Detection coverage" section.
#[test]
#[should_panic(expected = "self-loop, corrupting the free-list into a cycle")]
fn double_push_of_current_head_panics_on_first_pop() {
    let stack = ArrayIndexStack::<16, 64>::new();
    // SAFETY: fresh stack (domain 0..64); index 1 is in-domain and this is its first push.
    unsafe { stack.push(1) }.expect("fresh head has tag budget");
    // SAFETY: DELIBERATE contract violation under test — index 1 is the CURRENT head (live); the
    // self-loop the re-push writes is what the following pop's guard fires on.
    unsafe { stack.push(1) }.expect("fresh head has tag budget"); // re-push the CURRENT head: writes next[1] = 1
    let _ = stack.pop(); // first pop: self-loop -> panic
}

// Compile-fail coverage: out-of-range `INDEX_BITS` (the
// `tests/compile_fail/index_bits_zero/` and `index_bits_seventeen/`
// fixtures) and the cfg-without-feature fast-fail are pinned
// out-of-process by `tests/compile_fail.rs`, which asserts each failure
// is `_CHECK_BITS`'s E0080 / the named `compile_error!` with no secondary
// name-resolution error. This hand-rolled setup is the workspace's
// established alternative to `trybuild` (`compile_fail` doctests are
// banned; find the notes with
// `grep -rn trybuild --include=*.rs .` from the workspace root).
// Revisit only if `_CHECK_BITS`'s const-evaluation routing is ever
// refactored.

// ---------------------------------------------------------------------------
// ArrayIndexStack — fused head+links LIFO order + H-2 single-threaded.
// ---------------------------------------------------------------------------

#[test]
fn fresh_stack_is_empty() {
    let stack = ArrayIndexStack::<16, 8>::new();
    assert_eq!(stack.pop(), None, "a fresh (lazy-link) stack is empty");
}

#[test]
fn push_pop_is_lifo() {
    let stack = ArrayIndexStack::<16, 8>::new();
    for i in 0..5u32 {
        // SAFETY: fresh stack (domain 0..8); each index 0..5 is in-domain and pushed exactly once.
        unsafe { stack.push(i) }.expect("fresh head has tag budget");
    }
    let mut got = Vec::new();
    while let Some(i) = stack.pop() {
        got.push(i);
    }
    assert_eq!(got, vec![4, 3, 2, 1, 0], "LIFO order");
    assert_eq!(stack.pop(), None);
}

/// The degenerate `INDEX_BITS = 1` width through the REAL push/pop API: the
/// reserved empty sentinel is `INDEX_MASK == 1`, so `0` is the only valid
/// index and the stack's entire capacity is a single slot. (Only
/// `TaggedIndex`'s raw packing is otherwise exercised at this width — in
/// `proptest_pack_unpack.rs` — never the stack's push/pop path.) Also the
/// only test driving the public `is_empty()` around a full push/drain cycle.
#[test]
fn width_1_stack_push_pop_round_trips_its_sole_index() {
    assert_eq!(TaggedIndex::<1>::INDEX_MASK, 1);
    let stack = ArrayIndexStack::<1, 1>::new();
    assert!(stack.is_empty(), "a fresh (lazy-link) stack is empty");
    // SAFETY: fresh stack (sole in-domain index 0); not yet pushed.
    unsafe { stack.push(0) }.expect("fresh head has tag budget");
    assert!(!stack.is_empty(), "the sole index is on the stack");
    assert_eq!(stack.pop(), Some(0));
    assert!(stack.is_empty(), "drained back to empty");
    assert_eq!(stack.pop(), None, "empty stays empty");
}

/// Drain to empty then refill the SAME index: the tag must have advanced across
/// the empty transition (H-2), NOT reset to 0. Observed via `raw_head` — a
/// `test-internals`/loom-gated accessor, so this probe carries the same gate
/// (see the module doc).
#[cfg(any(feature = "test-internals", loom))]
#[test]
fn empty_transition_preserves_running_tag() {
    type T = TaggedIndex<16>;
    let stack = ArrayIndexStack::<16, 4>::new();

    // SAFETY: fresh stack (domain 0..4); index 0 is in-domain and this is its first push.
    unsafe { stack.push(0) }.expect("fresh head has tag budget"); // tag 0 -> 1
    let (_v, tag_after_push1) = T::unpack(stack.raw_head());
    assert_eq!(tag_after_push1, 1);

    // Drain to empty. The empty head must carry the RUNNING tag (1), not 0.
    assert_eq!(stack.pop(), Some(0));
    let empty_head = stack.raw_head();
    assert!(T::is_empty(empty_head), "stack is now empty");
    let (_ev, empty_tag) = T::unpack(empty_head);
    assert_eq!(
        empty_tag, 1,
        "H-2: the empty transition preserves the running tag (1), not 0 — \
         resetting to 0 would reopen ABA"
    );

    // Refill the same index: the push reads the running tag (1) and bumps to 2.
    // SAFETY: index 0 was just popped (drained to empty), so it is not live; in-domain by construction.
    unsafe { stack.push(0) }.expect("fresh head has tag budget");
    let (_v2, tag_after_push2) = T::unpack(stack.raw_head());
    assert_eq!(
        tag_after_push2, 2,
        "the tag keeps climbing across empty->non-empty (1 -> 2), never restarts"
    );
}

/// The link storage is only ever written by a push (RAD-1 lazy discipline):
/// after construction every link is the zero value, and popping never writes
/// a link. Observed DIRECTLY through the storage trait (`load_next`), not
/// inferred from push/pop behaviour: a never-pushed index's link reads 0
/// before AND after other indices are pushed and popped, so an eager
/// link-chaining pass (at construction or on the first push) fails here.
/// (`fresh_stack_is_empty` alone cannot distinguish lazy links from an
/// eagerly-chained-but-empty-headed stack.)
/// Gated like the accessor it reads through (`load_next_for_test`) — see the
/// module doc.
#[cfg(any(feature = "test-internals", loom))]
#[test]
fn links_are_lazy() {
    let stack = ArrayIndexStack::<16, 4>::new();
    // Never push index 3 in this test.
    assert_eq!(
        stack.load_next_for_test(3),
        0,
        "a never-pushed index's link is the zero value straight after construction"
    );
    // Push/drain 0 fully, then re-check: operating on OTHER indices must not
    // have touched index 3's link (a pop never writes a link, a push writes
    // only the pushed index's own link).
    // SAFETY: fresh stack (domain 0..4); index 0 is in-domain and this is its first push.
    unsafe { stack.push(0) }.expect("fresh head has tag budget");
    assert_eq!(stack.pop(), Some(0));
    assert_eq!(
        stack.load_next_for_test(3),
        0,
        "push/pop of other indices never writes a never-pushed index's link"
    );
}

/// Neither `Default` impl was previously exercised by any test.
/// `ArrayLinks::<N>::default()` must behave exactly like `new()`: every link
/// at the zero value (RAD-1 — no eager chaining), readable through the
/// inherent `load_next`, verified here link-for-link across all `N` indices.
/// (A bare `ArrayLinks` is not itself a `StackStorage`; push/pop behavior is
/// the stack-level tests' subject, e.g. `default_array_index_stack_behaves_like_new`.)
#[test]
fn default_array_links_behaves_like_new() {
    let default_links = ArrayLinks::<4>::default();
    let new_links = ArrayLinks::<4>::new();
    for i in 0..4u32 {
        assert_eq!(
            default_links.load_next(i),
            new_links.load_next(i),
            "link {i}: Default and New backings read identically"
        );
        assert_eq!(
            default_links.load_next(i),
            0,
            "link {i}: a fresh backing's links are the zero value (RAD-1)"
        );
    }
}

/// `ArrayIndexStack::<INDEX_BITS, N>::default()` must behave exactly like
/// `new()`: a fresh, EMPTY stack (RAD-1 lazy links) that pushes and pops
/// normally.
#[test]
fn default_array_index_stack_behaves_like_new() {
    let stack = ArrayIndexStack::<16, 8>::default();
    assert!(stack.is_empty(), "a freshly-defaulted stack is empty");
    assert_eq!(
        stack.pop(),
        None,
        "Default == new: the lazy-link stack starts empty"
    );
    // SAFETY: fresh default stack (domain 0..8); index 7 is in-domain and this is its first push.
    unsafe { stack.push(7) }.expect("fresh head has tag budget");
    assert!(!stack.is_empty());
    assert_eq!(stack.pop(), Some(7));
}

/// `StackHead::<INDEX_BITS>::default()` must behave exactly like `new()`:
/// the same bootstrap head word — `TaggedIndex::<16>::empty()`'s documented
/// empty-index sentinel with tag 0 — reading empty through the advisory
/// `is_empty`. This is the one `Default` impl a custom-storage implementor
/// reaches directly (`StackHead` is the head half of the `StackStorage`
/// extension point); it was previously pinned by nothing —
/// the CHANGELOG cited a test name that did not exist.
/// Gated like the accessor it reads through (`raw_head`) — see the module
/// doc. (The sibling `default_array_links_behaves_like_new` /
/// `default_array_index_stack_behaves_like_new` stay ungated: they read only
/// through public API.)
#[cfg(any(feature = "test-internals", loom))]
#[test]
fn default_stack_head_behaves_like_new() {
    let default_head = StackHead::<16>::default();
    let new_head = StackHead::<16>::new();
    assert_eq!(
        default_head.raw_head(),
        new_head.raw_head(),
        "Default and New heads hold the identical packed word"
    );
    assert_eq!(
        default_head.raw_head(),
        TaggedIndex::<16>::empty(),
        "a freshly-defaulted head IS the documented bootstrap empty sentinel"
    );
    assert_eq!(
        new_head.raw_head(),
        TaggedIndex::<16>::empty(),
        "a freshly-newed head IS the documented bootstrap empty sentinel"
    );
    assert!(
        default_head.is_empty(),
        "a freshly-defaulted head reads empty"
    );
    assert!(new_head.is_empty(), "a freshly-newed head reads empty");
}
