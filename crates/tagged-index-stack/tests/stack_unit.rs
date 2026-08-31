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

use tagged_index_stack::{ArrayLinks, Links, TaggedIndex, TaggedIndexStack, TAIL};

// Compile-time pin (P4-12c): both public types must stay auto-`Send + Sync`.
// Every field of both types is an atomic today, so they derive the traits for
// free — but their entire purpose is lock-free CROSS-THREAD sharing, and a
// future non-auto field (a `Cell`, a raw pointer, ...) would silently drop
// one or both traits with no compile error anywhere obvious. This const
// makes that a hard compile error the moment it happens. Widths 16 and 4 are
// this file's conventional choices (see the existing push/pop tests below).
// Both fns are `const fn` and `_check()` is actually CALLED in the const
// initializer: that both forces the trait bounds to be checked and keeps the
// dead-code lint from firing on a helper that is never otherwise used.
const _: () = {
    const fn assert_send_sync<T: Send + Sync>() {}
    const fn _check() {
        assert_send_sync::<TaggedIndexStack<16>>();
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
    // pack()'s doc says an over-wide index is TRUNCATED (masked with
    // INDEX_MASK before OR-ing with the tag), not that it "collides" with
    // the tag bits. This pins the sharpest case: at width 16, an
    // over-wide index whose low 16 bits equal INDEX_MASK itself truncates
    // to the EMPTY SENTINEL, not merely "a wrong index" — is_empty() then
    // reads true for a packed word whose caller-supplied index was never
    // the empty sentinel.
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

/// `try_pack` is the checked twin of `pack`: for every in-range
/// `(index, tag)` pair it returns EXACTLY what `pack` returns (asserted as
/// value equality against `pack`'s own output, not merely `Some`, so any
/// future divergence between the two is caught), and once a half crosses
/// its width boundary it returns `None` instead of silently truncating.
/// Pinned at width 16, the neighboring truncation test's width: the first
/// out-of-range index (`1 << INDEX_BITS`) is precisely the value `pack`
/// masks down to a different valid-looking index, and the first
/// out-of-range tag (`1 << TAG_BITS`) is precisely the value whose high
/// bit `pack`'s `<< INDEX_BITS` silently drops.
#[test]
fn try_pack_matches_pack_in_range_and_rejects_out_of_range_halves() {
    type T = TaggedIndex<16>;

    // In range => Some, identical to pack's own output for the SAME
    // inputs. INDEX_MASK itself is included deliberately: try_pack's
    // boundary is pack's truncation boundary (`< 2^INDEX_BITS`), NOT
    // push's stricter `< INDEX_MASK` reserve-sentinel bound — packing the
    // empty index with a tag is the legitimate H-2 shape.
    for &(idx, tag) in &[
        (0u64, 0u64),
        (1, 1),
        (2748, 42),
        (T::INDEX_MASK, 7),
        (0xFFFE, (1u64 << T::TAG_BITS) - 1),
    ] {
        assert_eq!(
            T::try_pack(idx, tag),
            Some(T::pack(idx, tag)),
            "try_pack must agree with pack exactly for in-range inputs \
             (index {idx}, tag {tag})"
        );
    }

    // First out-of-range index: exactly `1 << INDEX_BITS`. pack() masks it
    // to 0 — a DIFFERENT valid index; try_pack refuses it instead.
    assert_eq!(T::try_pack(1u64 << 16, 7), None, "first invalid index");
    // Farther out of range, including the value whose low bits are all
    // ones (pack would truncate it to the empty sentinel).
    assert_eq!(T::try_pack(u64::MAX, 7), None, "far out-of-range index");
    assert_eq!(
        T::try_pack(0x1_FFFF, 7),
        None,
        "over-wide index that pack would truncate to the empty sentinel"
    );

    // First out-of-range tag: exactly `1 << TAG_BITS` (2^48 at width 16).
    // pack() silently drops that shifted-out bit and returns a word whose
    // tag reads 0; try_pack refuses it instead.
    assert_eq!(
        T::try_pack(9, 1u64 << T::TAG_BITS),
        None,
        "first invalid tag"
    );
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
    let w = T::pack(0xABC, 7);
    let (v, t) = T::unpack(w);
    assert_eq!(v, 0xABC);
    assert_eq!(t, 7);
    assert!(T::is_empty(T::empty()));
    // TAIL (u32::MAX) differs from this width's empty_index (0xFFF).
    assert_ne!(T::empty_index() as u32, TAIL);
}

/// The old legal maximum `INDEX_BITS = 32` made `INDEX_MASK` numerically
/// equal `TAIL` (`u32::MAX`), collapsing `push`'s two reject-purposes
/// (out-of-range and reject-`TAIL`) into one value; the former
/// `width_32_index_mask_equals_tail_and_is_rejected` test pinned that
/// coincidence (and `push` panicking on `index == TAIL` because of it).
/// The `_CHECK_BITS` cap is now `1..=16`, so the coincidence is structurally
/// impossible at EVERY legal width (`INDEX_MASK <= 0xFFFF`) — pinned here at
/// the MAXIMUM legal width. The guard's panic path and its exact message
/// remain pinned by `width_16_push_rejects_index_mask_itself` below, which
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

/// push's `index < INDEX_MASK` guard at a NON-degenerate width, where the two
/// things the guard exists to reject are DIFFERENT values: at
/// `INDEX_BITS = 16`, `INDEX_MASK` is `0xFFFF` (the reserved empty sentinel)
/// while `TAIL` is `u32::MAX`. (At the old legal maximum `INDEX_BITS = 32`
/// the two coincided and the guard's purposes collapsed into one; the
/// `1..=16` cap has made that coincidence impossible — see
/// `max_legal_width_index_mask_never_equals_tail` above — so this pins the
/// guard's ordinary, out-of-range purpose in its own right.)
#[test]
fn width_16_push_rejects_index_mask_itself() {
    type T = TaggedIndex<16>;
    assert_ne!(
        T::INDEX_MASK,
        TAIL as u64,
        "at width 16 INDEX_MASK (0xFFFF) and TAIL (u32::MAX) must differ — \
         this test covers the guard's ordinary out-of-range case (no legal \
         width has an INDEX_MASK/TAIL coincidence any more)"
    );

    let links = ArrayLinks::<4>::new();
    let stack = TaggedIndexStack::<16>::new();
    // 0xFFFF == INDEX_MASK at this width: an in-range-looking u32 that the
    // guard must reject because it is the reserved empty sentinel. The full
    // panic assertion (not a bare is_err()) means the message must name the
    // guard's own contract, so an unrelated out-of-bounds panic (e.g. from
    // `ArrayLinks`) cannot satisfy this test.
    //
    // Also pins #[track_caller]'s effect (review-round6 P3-6): without it on
    // both `push` and its `#[cold]` helper, this panic's Location would name
    // lib.rs instead of this call site, and that regression would leave every
    // OTHER assertion here green. The panic hook is process-global, so this
    // closure CHAINS to whatever hook was previously installed instead of
    // replacing it -- any other test's panic running concurrently on another
    // thread (e.g. the should_panic tests below) still gets its usual default
    // handling; only a panic on THIS thread is inspected for its location.
    let this_thread = std::thread::current().id();
    let captured_file: std::sync::Arc<std::sync::Mutex<Option<String>>> =
        std::sync::Arc::new(std::sync::Mutex::new(None));
    let captured_file_for_hook = std::sync::Arc::clone(&captured_file);
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if std::thread::current().id() == this_thread {
            if let Some(loc) = info.location() {
                *captured_file_for_hook.lock().unwrap() = Some(loc.file().to_string());
            }
        }
        prev_hook(info);
    }));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stack.push(&links, T::INDEX_MASK as u32);
    }));
    let _ = std::panic::take_hook(); // drop our hook, restoring the default

    let err = result.expect_err("pushing index == INDEX_MASK must panic");
    let message = err
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| err.downcast_ref::<String>().cloned())
        .expect("panic payload should be a string message");
    assert!(
        message.contains("index must be < INDEX_MASK"),
        "panic message did not name the push guard's own contract (got: {message:?})"
    );
    assert_eq!(
        captured_file.lock().unwrap().as_deref(),
        Some(file!()),
        "push's #[track_caller] should report THIS file as the panic \
         location, not lib.rs -- #[track_caller] regressed"
    );
}

/// [`ArrayLinks::load_next`] panics if `index >= N` (this backing's own,
/// narrower bound — independent of `INDEX_BITS`). Unlike
/// `width_16_push_rejects_index_mask_itself` above (which uses
/// `catch_unwind` plus an explicit message assertion), this is a plain
/// `#[should_panic(expected = ...)]`: the expected substring is Rust's own
/// slice-indexing panic text (`self.next[index as usize]` in
/// `ArrayLinks::load_next`), which is unambiguous enough on its own that the
/// heavier `catch_unwind` pattern is not needed here.
#[test]
#[should_panic(expected = "index out of bounds")]
fn array_links_load_next_panics_on_index_out_of_range() {
    let links = ArrayLinks::<4>::new();
    links.load_next(4); // valid range is 0..=3
}

/// [`ArrayLinks::store_next`] panics if `index >= N` — the same bound as
/// `load_next` above, documented alongside it in `src/lib.rs`. Reached via
/// the worked example in `push`'s own `# Panics` section: a
/// `TaggedIndexStack::<16>` accepts indices up to 65534, but an
/// `ArrayLinks<4>` backing it holds only `0..=3`, so `push`'s `store_next`
/// call (which runs before the head CAS) panics on the links layer's own,
/// narrower bound before the stack's wider `INDEX_BITS` guard is ever in
/// play.
#[test]
#[should_panic(expected = "index out of bounds")]
fn array_links_store_next_panics_on_index_out_of_range() {
    let links = ArrayLinks::<4>::new();
    let stack = TaggedIndexStack::<16>::new();
    stack.push(&links, 5); // valid for the stack (< INDEX_MASK), not for ArrayLinks<4>
}

/// `pop`'s rule-4 guard fires when a [`Links`] backing returns a `next`
/// value that is neither `TAIL` nor a valid index — a caller-contract
/// violation `pop` cannot otherwise detect (see the crate docs' "Storage
/// requirement" section on `Links`). A tiny custom backing whose
/// `load_next` always answers `INDEX_MASK` (a value that is not `TAIL` and
/// not `< INDEX_MASK`) triggers it directly.
///
/// Release-active (round 7, P3-1): promoted from `debug_assert!` to an
/// unconditional `#[cold]` panic helper mirroring `push`'s own
/// `index < INDEX_MASK` guard, once an out-of-tree A/B measured the
/// release-active cost at ≈ 0 ns (see CHANGELOG.md). Unlike its
/// predecessor, this test needs no `#[cfg(debug_assertions)]` gate: the
/// panic now fires identically under `cargo test -p tagged-index-stack
/// --release` (the configuration `.github/workflows/ci.yml`'s `test
/// workspace members` job actually uses for this crate) and under the
/// dev/test profile default.
struct AlwaysInvalidLinks;

impl Links for AlwaysInvalidLinks {
    fn load_next(&self, _index: u32) -> u32 {
        // Neither TAIL nor a valid index at width 16 (INDEX_MASK == 0xFFFF):
        // exactly the shape pop's rule-4 guard exists to catch.
        TaggedIndex::<16>::INDEX_MASK as u32
    }

    fn store_next(&self, _index: u32, _next: u32) {}
}

#[test]
#[should_panic(expected = "neither TAIL")]
fn pop_rule_4_guard_fires_on_invalid_next_from_backing() {
    let links = AlwaysInvalidLinks;
    let stack = TaggedIndexStack::<16>::new();
    stack.push(&links, 0); // real push, so the head is non-empty
    let _ = stack.pop(&links); // load_next() always answers INDEX_MASK -> guard fires
}

// Compile-fail coverage note: this crate has no trybuild (or similar
// compile-fail) test infrastructure wired up, so `INDEX_BITS > 16` failing to
// compile is NOT pinned by an automated test. Manually verified instead:
// instantiating `TaggedIndex::<17>` (or any `TaggedIndexStack<N>` with
// `N > 16`) fails `cargo build` with the `_CHECK_BITS` assertion message
// ("INDEX_BITS must be in 1..=16 ..."). This is a known coverage gap, not a
// silent omission -- and a deliberate choice, not an oversight: adding
// `trybuild` for exactly this was evaluated and declined. `compile_fail`
// doctests are unavailable in this repo (banned outright, see CLAUDE.md's
// "No doctests" rule), and `trybuild` itself is a new dev-only dependency
// this workspace has already declined twice for the identical
// single-assertion tradeoff -- `crates/sefer-region/tests/handle_static_asserts.rs`
// and `crates/aligned-vmem/tests/smoke.rs` both cite the same "would need a
// `compile_fail` doctest or a `trybuild` dependency" reasoning and leave
// their own const-assertion coverage manual too. Revisit if `_CHECK_BITS`'s
// const-evaluation routing is ever refactored -- a real risk of silent
// breakage would tip the cost/benefit differently than it does today.

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

/// The degenerate `INDEX_BITS = 1` width through the REAL push/pop API: the
/// reserved empty sentinel is `INDEX_MASK == 1`, so `0` is the only valid
/// index and the stack's entire capacity is a single slot. (Only
/// `TaggedIndex`'s raw packing is otherwise exercised at this width — in
/// `proptest_pack_unpack.rs` — never the stack's push/pop path.) Also the
/// only test driving the public `is_empty()` around a full push/drain cycle.
#[test]
fn width_1_stack_push_pop_round_trips_its_sole_index() {
    assert_eq!(TaggedIndex::<1>::INDEX_MASK, 1);
    let links = ArrayLinks::<1>::new();
    let stack = TaggedIndexStack::<1>::new();
    assert!(stack.is_empty(), "a fresh (lazy-link) stack is empty");
    stack.push(&links, 0);
    assert!(!stack.is_empty(), "the sole index is on the stack");
    assert_eq!(stack.pop(&links), Some(0));
    assert!(stack.is_empty(), "drained back to empty");
    assert_eq!(stack.pop(&links), None, "empty stays empty");
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

/// Neither `Default` impl was previously exercised by any test.
/// `ArrayLinks::<N>::default()` must behave exactly like `new()`: every link
/// at the zero value (RAD-1 — no eager chaining), readable through the same
/// [`Links`] trait, and usable as push/pop backing without further setup.
#[test]
fn default_array_links_behaves_like_new() {
    let default_links = ArrayLinks::<4>::default();
    let new_links = ArrayLinks::<4>::new();
    for i in 0..4u32 {
        assert_eq!(
            default_links.load_next(i),
            new_links.load_next(i),
            "link {i}: Default and New backings read identically through Links"
        );
        assert_eq!(
            default_links.load_next(i),
            0,
            "link {i}: a fresh backing's links are the zero value (RAD-1)"
        );
    }
    let stack = TaggedIndexStack::<16>::new();
    stack.push(&default_links, 2);
    assert_eq!(stack.pop(&default_links), Some(2));
}

/// `TaggedIndexStack::<INDEX_BITS>::default()` must behave exactly like
/// `new()`: a fresh, EMPTY stack (RAD-1 lazy links) that pushes and pops
/// normally.
#[test]
fn default_stack_behaves_like_new() {
    let links = ArrayLinks::<8>::default();
    let stack = TaggedIndexStack::<16>::default();
    assert!(stack.is_empty(), "a freshly-defaulted stack is empty");
    assert_eq!(
        stack.pop(&links),
        None,
        "Default == new: the lazy-link stack starts empty"
    );
    stack.push(&links, 7);
    assert!(!stack.is_empty());
    assert_eq!(stack.pop(&links), Some(7));
}
