//! [`TaggedPtr`] — the packed `(value | tag)` word used by the registry's
//! `free_slots` Treiber stack to defeat the ABA problem.
//!
//! The `free_slots` stack head is a single `AtomicU64`. The low
//! [`INDEX_BITS`] = 16 bits carry the slot index; the high 48 bits carry a
//! monotonic **tag** that is bumped on every successful push. The classic ABA
//! scenario — thread A reads head=X, thread B pops X then re-pushes X — is
//! defeated because the re-push bumps the tag, so A's CAS on `(X, old_tag)`
//! fails.
//!
//! (Phase 12.4: the `abandoned_segs` stack previously also used `TaggedPtr`,
//! which stored the segment base in the low 32 bits and truncated addresses
//! above 4 GiB — FINDINGS №1. It now uses a dedicated intrusive
//! head+next layout in [`super::bootstrap`] that packs the full 64-bit base
//! with the ABA tag in the SEGMENT-alignment low bits. `TaggedPtr` is
//! henceforth `free_slots`-only.)
//!
//! ## Why 48-bit tags (task W7a)
//!
//! [`MAX_HEAPS`](super::bootstrap::MAX_HEAPS) = 4096 needs only 13 bits, so a
//! 32-bit index half was 19 bits of pure waste. Repacking to
//! [`INDEX_BITS`] = 16 (holds 65535 ≥ 4096, with the empty sentinel `0xFFFF`
//! reserved above the cap) hands the freed 16 bits to the tag: **48 tag bits**,
//! wrapping at `2^48 ≈ 2.8 × 10^14`. At a sustained (and already unrealistic)
//! 100k pushes/sec on a SINGLE slot with the victim thread parked across every
//! one, a wrap-around ABA would take ∼89 years — effectively unreachable in any
//! process lifetime, upgrading the OLD 32-bit tag's "probabilistic, ∼43 s of
//! frozen-victim churn" bound (§2.1 risk-register of
//! `ALLOC_PLAN_PHASE12-13.md`) to a structural non-hazard. The repack is
//! Ir-neutral (`pack`/`unpack` are the same two shifts/masks on different
//! constants; this is a cold registry-protocol word, off every hot alloc path).
//! A `const` assert below pins `MAX_HEAPS < 2^INDEX_BITS` so a future
//! `MAX_HEAPS` bump that no longer fits 16 bits fails to compile rather than
//! silently colliding the index with the tag.
//!
//! **0.3.0 (task #141) — resolved:** the push-pop-repush ABA loom model for
//! THIS `TaggedPtr`/`free_slots` protocol is now `tests/loom_free_slots_aba.rs`
//! (`RUSTFLAGS="--cfg loom" cargo test --release --features alloc-global
//! --test loom_free_slots_aba`, wired into the CI `loom` matrix). It
//! transcribes `pop_free_slot`/`push_free_slot`
//! (`src/registry/heap_registry.rs`) and `TaggedPtr::pack`/`unpack` verbatim
//! and models the classic ABA race (thread A reads a stale `(index, tag)`
//! head while thread B pops-then-repushes the SAME index, bumping the tag),
//! asserting the tag guard forces A's stale CAS to fail/retry and that the
//! free-list stays loss/duplication-free. A counterfactual with the tag
//! removed (bare `AtomicU32` head) proves the harness is non-vacuous: loom
//! finds the same interleaving corrupting the untagged free-list.
//! `tests/loom_registry.rs` (Phase 12.4) remains a model of a DIFFERENT,
//! unreachable protocol (the segment `owner_state` adoption CAS) — see that
//! file's own honesty note; it is untouched by this resolution.
//!
//! ## This file is pure safe arithmetic
//!
//! No `unsafe`, no memory operations — only bit packing / unpacking on `u64`.
//!
//! ## Provenance model (task #140)
//!
//! `TaggedPtr` itself never casts a `*mut T` to/from its packed `u64` word —
//! `free_slots` packs a plain `u32` SLOT INDEX (not a pointer), so there is
//! no provenance to reason about here at all. It is strict-provenance-clean
//! by construction, trivially. This is a DELIBERATE structural difference
//! from `abandoned_segs` (see `super::bootstrap`'s "Provenance model"
//! section): the doc comment above (§"For `abandoned_segs`...") describing
//! bases packed via `TaggedPtr` is HISTORICAL — Phase 12.4 moved
//! `abandoned_segs` off `TaggedPtr` onto the dedicated intrusive head+next
//! layout in `bootstrap.rs` specifically because a raw pointer address does
//! not fit this module's "pure integer, no provenance" contract cleanly.
//! `TaggedPtr` remains `free_slots`-only, as the note above already states.

/// Number of low bits reserved for the index/value. The high bits of the
/// `u64` word carry the tag.
///
/// **16 (task W7a):** [`MAX_HEAPS`](super::bootstrap::MAX_HEAPS) = 4096 fits in
/// 13 bits, and the empty sentinel `INDEX_MASK = 0xFFFF` (65535) sits above the
/// cap, so 16 bits hold every valid index plus the sentinel with room to spare
/// — leaving 48 bits for the ABA tag (see the module doc's "Why 48-bit tags").
pub(crate) const INDEX_BITS: u32 = 16;

/// Bit-mask for the low [`INDEX_BITS`] (the value half). With
/// [`INDEX_BITS`] = 16 this is `0xFFFF`.
const INDEX_MASK: u64 = (1u64 << INDEX_BITS) - 1;

/// Compile-time guard: every valid slot index (`0..MAX_HEAPS`) AND the empty
/// sentinel (`INDEX_MASK`) must be representable in [`INDEX_BITS`], and the
/// sentinel must NOT collide with any valid index. `MAX_HEAPS <= INDEX_MASK`
/// guarantees both: the largest valid index is `MAX_HEAPS - 1 < INDEX_MASK`, so
/// `INDEX_MASK` is a non-index, and all indices fit the low bits. A future
/// `MAX_HEAPS` bump past `2^16 - 1` fails to compile here rather than silently
/// truncating an index into the tag or colliding with the sentinel.
const _: () = assert!(
    (super::bootstrap::MAX_HEAPS as u64) <= INDEX_MASK,
    "MAX_HEAPS must be < 2^INDEX_BITS so slot indices fit the value half and \
     never collide with the empty sentinel (INDEX_MASK)"
);

/// A packed `(value | tag)` word. Construct via [`TaggedPtr::pack`];
/// decompose via [`TaggedPtr::unpack`]. Stored inside an `AtomicU64` by the
/// registry stacks.
///
/// `value` is interpreted by the caller:
/// - for `free_slots` it is a slot index (`u32`);
/// - for `abandoned_segs` it is a segment base address (a `*mut u8` cast to
///   `u64`).
///
/// For `abandoned_segs` this restricts segment bases to the low 32 bits of
/// the address space. That holds for the miri aperture (host `std::alloc`
/// returns low addresses on the supported miri hosts) and is the realistic
/// case on x86-64 Windows/Linux/macOS user space where the registry carves
/// segments via `mmap`/`VirtualAlloc` (which the kernel places well below
/// 4 GiB for anonymous mappings in practice on the default ASLR layout). The
/// bootstrap `debug_assert`s the base fits; if a future target violates this,
/// switch `abandoned_segs` to an intrusive head+next layout (as
/// `ThreadFreeStack` already does for its `AtomicPtr`).
pub(crate) struct TaggedPtr;

impl TaggedPtr {
    /// Pack `(value, tag)` into one `u64`. `value` MUST be `< 2^INDEX_BITS`
    /// (the caller — the registry — guarantees this for slot indices by
    /// construction and for segment bases by the bootstrap `debug_assert`).
    #[must_use]
    pub(crate) const fn pack(value: u64, tag: u64) -> u64 {
        // The tag lives in the high bits. We trust the caller's invariant
        // that `value < 2^INDEX_BITS`; a wider value would silently collide
        // with the tag bits, so the registry `debug_assert`s this on pack.
        (tag << INDEX_BITS) | (value & INDEX_MASK)
    }

    /// Split a packed word back into `(value, tag)`.
    #[must_use]
    pub(crate) const fn unpack(word: u64) -> (u64, u64) {
        (word & INDEX_MASK, word >> INDEX_BITS)
    }

    /// The sentinel "empty stack" word: value = all-ones-index
    /// (`INDEX_MASK` = `0xFFFF` = 65535, which is above `MAX_HEAPS` = 4096, so
    /// it is never a real slot index), tag = 0. The `free_slots` stack is
    /// initialised to this.
    ///
    /// Only the BOOTSTRAP-time empty state uses tag 0 unconditionally; a
    /// RUNTIME empty transition (a pop that drains the last slot) must
    /// instead preserve the running tag — see [`empty_index`](Self::empty_index)
    /// and the H-2 fix note on `pop_free_slot` in `heap_registry.rs`.
    #[must_use]
    pub(crate) const fn empty() -> u64 {
        // value = INDEX_MASK (all-ones in the low bits) is an impossible slot
        // index / segment base, so it unambiguously denotes "empty".
        Self::pack(INDEX_MASK, 0)
    }

    /// The "empty stack" sentinel's index half (`INDEX_MASK`), for callers
    /// that need to pack it together with a NON-zero, caller-supplied tag
    /// (`pack(TaggedPtr::empty_index(), running_tag)`) rather than the
    /// bootstrap `empty()` word, which always zeroes the tag.
    ///
    /// **H-2 fix:** `pop_free_slot`'s empty transition (the pop that drains
    /// the last live slot) uses this — packing the tag it just observed on
    /// the popped head — instead of `empty()`, which would reset the running
    /// ABA tag to 0 and reopen the classic Treiber ABA window across an
    /// empty→non-empty→... churn cycle. [`is_empty`](Self::is_empty) only
    /// inspects the index half, so a non-zero tag here is still
    /// unambiguously "empty".
    #[must_use]
    pub(crate) const fn empty_index() -> u64 {
        INDEX_MASK
    }

    /// Whether a packed word denotes the empty stack (the [`empty`] sentinel).
    ///
    /// [`empty`]: Self::empty
    #[must_use]
    pub(crate) const fn is_empty(word: u64) -> bool {
        let (value, _tag) = Self::unpack(word);
        value == INDEX_MASK
    }
}

// ---------------------------------------------------------------------------
// Test-only forwarders (task W7a wrap counterfactual).
//
// `TaggedPtr` and its constants are `pub(crate)`, so an integration test in
// `tests/` cannot reach them directly. These `#[doc(hidden)]` `pub` forwarders
// expose the pure pack/unpack arithmetic (and the width constants) to
// `tests/regression_counter_wrap.rs`, which drives the 48-bit tag WRAP
// boundary (pack the max tag `2^48 - 1`, bump it once to wrap → 0, assert the
// index round-trips across the wrap and the empty sentinel is never mistaken
// for a live index). They add NO code to any allocation path — they are thin
// `const fn` shims over the same bit ops, compiled the same as a direct call,
// and are not referenced by any production caller.
#[doc(hidden)]
#[must_use]
pub const fn dbg_pack(value: u64, tag: u64) -> u64 {
    TaggedPtr::pack(value, tag)
}

#[doc(hidden)]
#[must_use]
pub const fn dbg_unpack(word: u64) -> (u64, u64) {
    TaggedPtr::unpack(word)
}

#[doc(hidden)]
#[must_use]
pub const fn dbg_empty() -> u64 {
    TaggedPtr::empty()
}

#[doc(hidden)]
#[must_use]
pub const fn dbg_is_empty(word: u64) -> bool {
    TaggedPtr::is_empty(word)
}

/// Number of index bits (16 since W7a). Exposed for the wrap counterfactual.
#[doc(hidden)]
pub const DBG_INDEX_BITS: u32 = INDEX_BITS;

/// The tag half's width in bits (`64 - INDEX_BITS` = 48 since W7a). The tag
/// wraps at `2^DBG_TAG_BITS`.
#[doc(hidden)]
pub const DBG_TAG_BITS: u32 = 64 - INDEX_BITS;

/// The index mask (`0xFFFF` since W7a) — also the empty-sentinel index value.
#[doc(hidden)]
pub const DBG_INDEX_MASK: u64 = INDEX_MASK;
