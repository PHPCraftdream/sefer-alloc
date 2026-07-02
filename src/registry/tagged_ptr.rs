//! [`TaggedPtr`] — the packed `(value | tag)` word used by the registry's
//! `free_slots` Treiber stack to defeat the ABA problem.
//!
//! The `free_slots` stack head is a single `AtomicU64`. The low
//! [`INDEX_BITS`] = 32 bits carry the slot index; the high 32 bits carry a
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
//! ## Why 32-bit tags are enough
//!
//! The tag wraps at `u32::MAX + 1 = 2^32 ≈ 4.3 × 10^9`. A wrap-around ABA
//! requires the SAME slot to be popped-and-repushed `2^32` times with the
//! racing thread parked across every one of them — at, say, 10 ns per
//! push/pop that is ~43 seconds of sustained churn on a single slot with the
//! victim thread frozen. That is far beyond any realistic allocator churn
//! (heaps are claimed/recycled on thread spawn/exit, not per-allocation).
//! This matches the judgement recorded in §2.1 / risk-register of
//! `ALLOC_PLAN_PHASE12-13.md`: "document the tag-width vs realistic churn".
//!
//! **0.3.0 (task #138) — honest status:** the push-pop-repush ABA loom model
//! for THIS `TaggedPtr`/`free_slots` protocol referenced above was never
//! actually written. `tests/loom_registry.rs` (Phase 12.4) models a
//! DIFFERENT protocol — the segment `owner_state` adoption CAS, which does
//! not use `TaggedPtr` at all (and is itself unreachable from any production
//! path today — see that file's own honesty note). No loom file exercises
//! `free_slots`'/`TaggedPtr`'s push-pop-repush ABA sequence. This is tracked
//! as follow-up debt (not written in task #138: the `free_slots` stack
//! lives in `src/registry/bootstrap.rs`/`heap_registry.rs`, both real
//! `core::sync::atomic` — modelling it would need a new loom harness,
//! judged out of scope for this hardening pass; see the task #138 report).
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
pub(crate) const INDEX_BITS: u32 = 32;

/// Bit-mask for the low [`INDEX_BITS`] (the value half).
const INDEX_MASK: u64 = (1u64 << INDEX_BITS) - 1;

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

    /// The sentinel "empty stack" word: value = all-ones-index (no real slot
    /// is `u32::MAX` — the registry caps at `MAX_HEAPS`), tag = 0. Both stacks
    /// are initialised to this.
    #[must_use]
    pub(crate) const fn empty() -> u64 {
        // value = INDEX_MASK (all-ones in the low bits) is an impossible slot
        // index / segment base, so it unambiguously denotes "empty".
        Self::pack(INDEX_MASK, 0)
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
