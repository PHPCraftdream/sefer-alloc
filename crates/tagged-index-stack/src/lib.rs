//! `tagged-index-stack` — a lock-free LIFO free-list of small **indices** (a
//! *slot recycler*) whose head is a single atomic word packing an
//! `(index | tag)` pair, where a monotonic **tag** in the high bits defeats the
//! ABA problem. Allocation-free, `no_std`, `#![forbid(unsafe_code)]`.
//!
//! This is the canonical "recycle a small integer id" primitive that slab
//! allocators, object pools, entity-component stores, and connection tables all
//! reinvent — and routinely reinvent *wrong*. The two subtleties people get
//! wrong (documented below) are the **H-2 empty-transition tag preservation**
//! and the **lazy link discipline (RAD-1)**; both are structurally enforced
//! here.
//!
//! # The packed word — [`TaggedIndex`]
//!
//! The stack head is one `AtomicU64` holding a [`TaggedIndex`]`<INDEX_BITS>`:
//! the low `INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS` bits
//! carry a monotonic **tag** bumped on every successful PUSH. The index half's
//! all-ones value (`(1 << INDEX_BITS) - 1`, the [`empty_index`](TaggedIndex::empty_index))
//! is reserved as the "stack empty" sentinel, so the usable index range is
//! `0 .. (1 << INDEX_BITS) - 1`.
//!
//! The classic ABA scenario — thread A reads `head = X`, thread B pops X then
//! re-pushes X — is defeated because B's re-push bumps the tag, so A's CAS on
//! `(X, old_tag)` observes a changed tag and fails, forcing a retry. A pop
//! preserves the tag; only a push advances it.
//!
//! # Links — slot-resident OR owned
//!
//! The stack stores only the HEAD. Each pushed index's "next" link lives in
//! caller storage, reached through the [`Links`] trait ([`load_next`](Links::load_next) /
//! [`store_next`](Links::store_next)). This is what lets a production allocator
//! keep its links **slot-resident** (an `AtomicU32` field inside each slot it
//! already owns) instead of paying for a second array. For standalone use, the
//! crate provides [`ArrayLinks`]`<N>` — an owned `[AtomicU32; N]` backing.
//!
//! [`Links::store_next`] is the ONLY write the stack ever makes to a link, and
//! it happens during [`push`](TaggedIndexStack::push), immediately before the
//! CAS that publishes the index as the new head. The stack NEVER eagerly
//! initialises links — see "The lazy link discipline (RAD-1)" below.
//!
//! # The two hard-won subtleties
//!
//! ## H-2: the empty-transition tag MUST be preserved (not reset to 0)
//!
//! When a [`pop`](TaggedIndexStack::pop) drains the LAST element, the head
//! transitions to "empty". A naive implementation packs the empty sentinel with
//! **tag 0** (`TaggedIndex::empty()`). **That is a bug.** Resetting the tag to 0
//! reopens the ABA window: a popper parked mid-`pop`, holding a stale
//! `(idx, tag)` snapshot from BEFORE the drain, can have its stale tag
//! spuriously RECUR once the stack drains (→ tag 0) and is immediately refilled
//! by a push of the SAME index (→ tag `0 + 1 = 1`); if the parked snapshot's tag
//! was `1`, the head word recurs EXACTLY and the stale CAS succeeds — a genuine
//! ABA collision that corrupts the free-list. The fix ([`pop`](TaggedIndexStack::pop)
//! here) packs the empty sentinel's index half with the RUNNING tag the draining pop just
//! observed, so the tag keeps climbing across the empty transition exactly as it
//! would across any other pop. [`is_empty`](TaggedIndex::is_empty) inspects only
//! the index half, so a non-zero tag on the empty word is still unambiguously
//! "empty". The [`push`](TaggedIndexStack::push) side already reads the tag out
//! of the current head (empty or not) and bumps it, so it composes with no other
//! change. The shipped loom counterfactual
//! `counterfactual_empty_transition_tag_reset_lets_aba_recur` proves this is
//! load-bearing: with tag-reset restored, loom finds the collision.
//!
//! ## The lazy link discipline (RAD-1): links are NEVER eagerly written
//!
//! The stack writes a slot's link ONLY inside [`push`](TaggedIndexStack::push)
//! (the [`store_next`](Links::store_next) immediately before publishing that
//! index as head). It performs NO bulk/eager initialisation of the link storage
//! at construction. A caller whose link backing is OS-zeroed memory (a fresh
//! mmap, a zeroed slot array) therefore never first-touches those pages merely
//! to set up the free-list — the pages are committed lazily, on first push of
//! each index. In the extracting allocator this saved a ~16 MiB
//! bootstrap-commit first-touch. [`ArrayLinks::new`] likewise starts every link
//! at `0` (the zero value), matching OS-zeroed backing, rather than eagerly
//! chaining a full free-list.
//!
//! Because links are lazy, a freshly-constructed stack is EMPTY — the caller
//! pushes indices in as they become free. This crate does NOT offer a "start
//! with `0..N` all pushed" constructor precisely because that would require an
//! eager link-chaining pass, defeating RAD-1. (A caller that genuinely wants
//! every index free from the start pushes `0..N` itself, or — as the extracting
//! allocator does — mints fresh indices via a separate monotonic counter and
//! only ever pushes RECYCLED ones onto this stack.)
//!
//! # Tag-width budget — why 48 bits is a structural non-hazard
//!
//! With `INDEX_BITS = 16` (holds 65535 indices, ample for a 4096-slot pool with
//! the empty sentinel `0xFFFF` reserved above the cap), the tag gets the other
//! **48 bits**, wrapping at `2^48 ≈ 2.8 × 10^14`. The only way a tag wrap
//! reopens ABA is if a victim thread is parked across an ENTIRE wrap's worth of
//! pushes on a SINGLE slot. At a sustained (already unrealistic) 100k pushes/sec
//! on one slot with the victim frozen the whole time, a wrap-around ABA would
//! take `2^48 / 100_000 / (3600 · 24 · 365) ≈ 89 years` — effectively
//! unreachable in any process lifetime. (A 32-bit tag, by contrast, gives only
//! `2^32 / 100_000 / 3600 ≈ 12 hours` of frozen-victim churn at the SAME
//! 100k pushes/sec rate — a probabilistic hazard, not a structural
//! non-hazard.) Widening the index half shrinks this budget; a caller
//! choosing `INDEX_BITS` trades index range against tag headroom.
//!
//! # loom — the tests run against THIS type
//!
//! Under `--cfg loom` the stack's atomics alias to `loom::sync::atomic`, so the
//! shipped loom suite (`tests/loom_aba.rs`) model-checks the REAL
//! [`TaggedIndexStack`] / [`TaggedIndex`] code, not a transcription — with
//! `#[should_panic]` counterfactuals (untagged corruption, the H-2
//! empty-transition tag-reset ABA, and a Relaxed-CAS-failure-ordering
//! regression) proving the harness is non-vacuous.
//!
//! # Portability limit — requires 64-bit atomics
//!
//! The stack head is a single `AtomicU64` (the packed `(index | tag)` word — see
//! above); packing both halves into ONE atomic word is the entire mechanism that
//! makes the CAS in [`push`](TaggedIndexStack::push)/[`pop`](TaggedIndexStack::pop)
//! atomic across index-and-tag together, so this is not an incidental
//! implementation choice. That means this crate needs `target_has_atomic = "64"`
//! and will **not compile** on a target without native 64-bit atomic support —
//! notably `thumbv6m-none-eabi`, `thumbv7em-none-eabi`, `riscv32imc-unknown-none-elf`,
//! and `armv5te-unknown-linux-gnueabi`. This crate is `no_std`-compatible, but
//! `no_std` alone does not imply 64-bit-atomic support: many Cortex-M and
//! RISC-V-without-A-extension targets are `no_std` yet lack `AtomicU64` entirely.
//! A build on an unsupported target fails fast with an explicit
//! [`compile_error!`] naming the requirement, rather than the more cryptic
//! "cannot find function/no `AtomicU64` in `core::sync::atomic`" error a bare
//! unresolved import would otherwise produce.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]
#![deny(missing_docs)]

// The stack head is one AtomicU64 (see the crate-doc "Portability limit"
// section above) — that requires native 64-bit atomic support from the target.
// Fail fast with an explicit, named reason instead of the cryptic "no
// `AtomicU64` in `core::sync::atomic`" unresolved-import error a naive use
// would otherwise produce on e.g. thumbv6m/thumbv7em/riscv32imc/armv5te.
#[cfg(not(target_has_atomic = "64"))]
compile_error!(
    "tagged-index-stack requires a target with native 64-bit atomics \
     (target_has_atomic = \"64\") because its head is a single AtomicU64 \
     packing the (index | tag) word atomically. This target does not have \
     them (e.g. thumbv6m-none-eabi, thumbv7em-none-eabi, \
     riscv32imc-unknown-none-elf, and armv5te-unknown-linux-gnueabi are all \
     known-unsupported) — see the crate-root doc comment's \"Portability \
     limit\" section."
);

// The atomics are aliased so loom can shadow the REAL stack type: under
// `--cfg loom` they are built on `loom::sync::atomic`, so the shipped loom tests
// exercise the actual `TaggedIndexStack`/`TaggedIndex` code rather than a
// transcription. Under normal builds it is `core::sync::atomic`, keeping the
// crate zero-non-std-dep.
#[cfg(not(loom))]
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(loom)]
use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// The "no next" sentinel stored in a slot's link to denote the BOTTOM of the
/// stack (the last-pushed index chains to this). `u32::MAX`.
///
/// Note this is distinct from the "stack empty" head sentinel
/// ([`TaggedIndex::empty_index`]): `TAIL` marks a per-slot link's end-of-chain,
/// while the empty sentinel marks the HEAD word as carrying no index at all. The
/// two mappings are kept spelled out separately in [`push`](TaggedIndexStack::push) /
/// [`pop`](TaggedIndexStack::pop) so the invariant never rests on a numeric
/// coincidence between them.
pub const TAIL: u32 = u32::MAX;

/// A packed `(index | tag)` word with a compile-time-chosen index width.
///
/// The low `INDEX_BITS` bits carry a slot index; the high `64 - INDEX_BITS` bits
/// carry a monotonic ABA tag. The all-ones index value
/// ([`empty_index`](Self::empty_index)) is reserved as the empty-stack sentinel,
/// so valid indices are `0 .. (1 << INDEX_BITS) - 1`.
///
/// This is a zero-sized namespace of `const fn` bit operations — no state, no
/// memory, no `unsafe`, strict-provenance-clean by construction (it packs a
/// plain integer index, never a pointer/address).
#[derive(Debug, Clone, Copy)]
pub struct TaggedIndex<const INDEX_BITS: u32>;

impl<const INDEX_BITS: u32> TaggedIndex<INDEX_BITS> {
    /// Compile-time guard: `INDEX_BITS` must be in `1..=32` so both halves are
    /// non-empty, the shifts are well-defined, AND every representable index fits
    /// in the `u32` that [`push`](TaggedIndexStack::push) actually takes.
    ///
    /// Widths above 32 are rejected rather than merely discouraged: `push` takes
    /// a `u32` index, so `INDEX_BITS > 32` can never buy reachable index range —
    /// it only shrinks the tag budget — and worse, it makes `INDEX_MASK` exceed
    /// `u32::MAX`, which let `index == u32::MAX` (the internal [`TAIL`] sentinel)
    /// silently pass the old `< INDEX_MASK` runtime guard and corrupt a chain.
    /// Capping at compile time closes that class of bug structurally instead of
    /// requiring every caller to separately exclude `TAIL` at runtime.
    ///
    /// This `const` is forced to evaluate from EVERY public associated item of
    /// `TaggedIndex<INDEX_BITS>`: [`pack`](Self::pack) references it via a
    /// `let () = Self::_CHECK_BITS;` statement, `INDEX_MASK` and
    /// [`TAG_BITS`](Self::TAG_BITS) evaluate it in their own initializers, and
    /// [`unpack`](Self::unpack), [`empty_index`](Self::empty_index),
    /// [`is_empty`](Self::is_empty), and [`empty`](Self::empty) all route through
    /// `INDEX_MASK` or `pack` — so an out-of-range `INDEX_BITS` cannot reach
    /// any associated item without tripping this guard.
    const _CHECK_BITS: () = assert!(
        INDEX_BITS >= 1 && INDEX_BITS <= 32,
        "INDEX_BITS must be in 1..=32 (both the index half and the tag half must \
         be non-empty, and every valid index must fit in push's u32 parameter — \
         widths above 32 buy no reachable index range and are rejected)"
    );

    /// Bit-mask for the low [`INDEX_BITS`](Self) (the index half), e.g. `0xFFFF`
    /// for `INDEX_BITS = 16`. Also the [`empty_index`](Self::empty_index) value.
    ///
    /// Forces `_CHECK_BITS` to evaluate here too (not just
    /// inside [`pack`](Self::pack)), since [`unpack`](Self::unpack),
    /// [`empty_index`](Self::empty_index), and [`is_empty`](Self::is_empty) all
    /// reference `INDEX_MASK` directly — this closes the residual gap where an
    /// out-of-range `INDEX_BITS` could reach those associated items without
    /// ever calling `pack` first.
    pub const INDEX_MASK: u64 = {
        let () = Self::_CHECK_BITS;
        (1u64 << INDEX_BITS) - 1
    };

    /// Number of bits carrying the tag (`64 - INDEX_BITS`). The tag wraps at
    /// `2^TAG_BITS`.
    pub const TAG_BITS: u32 = {
        // Force the compile-time bounds check to be evaluated.
        let () = Self::_CHECK_BITS;
        64 - INDEX_BITS
    };

    /// Pack `(index, tag)` into one `u64`. `index` MUST be `< 2^INDEX_BITS`; a
    /// wider value is silently TRUNCATED to its low `INDEX_BITS` bits (the
    /// `& Self::INDEX_MASK` below masks it before OR-ing with the tag, so the
    /// two never actually collide bitwise — the failure mode is a wrong index
    /// round-tripping out of [`unpack`](Self::unpack), not tag corruption).
    /// The sharpest case of this: if the truncated low bits happen to equal
    /// `INDEX_MASK` itself, the result reads as the EMPTY sentinel via
    /// [`is_empty`](Self::is_empty), not merely as some other live index.
    /// The caller — the stack — guarantees the `< 2^INDEX_BITS` precondition by
    /// construction, since indices come from
    /// [`push`](TaggedIndexStack::push)'s `< INDEX_MASK` contract, which this
    /// truncation can never be reached through.
    #[must_use]
    pub const fn pack(index: u64, tag: u64) -> u64 {
        // Force the compile-time bounds check to be evaluated.
        let () = Self::_CHECK_BITS;
        (tag << INDEX_BITS) | (index & Self::INDEX_MASK)
    }

    /// Split a packed word back into `(index, tag)`.
    #[must_use]
    pub const fn unpack(word: u64) -> (u64, u64) {
        (word & Self::INDEX_MASK, word >> INDEX_BITS)
    }

    /// The bootstrap empty-stack word: index = [`empty_index`](Self::empty_index),
    /// tag = 0. A freshly-constructed [`TaggedIndexStack`] head is this.
    ///
    /// **Only the bootstrap-time empty state uses tag 0 unconditionally.** A
    /// RUNTIME empty transition (a pop that drains the last element) MUST instead
    /// preserve the running tag — see [`empty_index`](Self::empty_index) and the
    /// H-2 note in the crate docs. Resetting the tag to 0 on a runtime drain
    /// reopens the ABA window.
    ///
    /// `#[doc(hidden)]`: this is a `pub const fn` (so `tests/` — an external
    /// crate from this crate's own perspective — can reach it) but NOT a
    /// stable, documented part of the public API. Its only correct in-crate
    /// caller is [`TaggedIndexStack::new`]'s bootstrap path; anywhere else the
    /// unconditional tag-0 reset reopens the H-2 ABA window documented above —
    /// a runtime drain must instead use [`empty_index`](Self::empty_index) with
    /// the tag it just observed. See this project's established
    /// `#[doc(hidden)]` rationale convention (cf. `raw_head`).
    #[doc(hidden)]
    #[must_use]
    pub const fn empty() -> u64 {
        Self::pack(Self::INDEX_MASK, 0)
    }

    /// The empty sentinel's index half (`INDEX_MASK`), for packing it with a
    /// NON-zero, caller-supplied RUNNING tag (`pack(empty_index(), running_tag)`)
    /// instead of [`empty`](Self::empty) (which always zeroes the tag).
    ///
    /// **H-2 fix:** the empty transition in [`pop`](TaggedIndexStack::pop) uses
    /// this, packing the tag it just observed on the popped head, so the ABA tag
    /// keeps counting forward across the empty→non-empty churn cycle.
    /// [`is_empty`](Self::is_empty) inspects only the index half, so a non-zero
    /// tag here is still unambiguously "empty".
    #[must_use]
    pub const fn empty_index() -> u64 {
        Self::INDEX_MASK
    }

    /// Whether a packed word denotes the empty stack (index half == the empty
    /// sentinel), REGARDLESS of the tag half.
    #[must_use]
    pub const fn is_empty(word: u64) -> bool {
        (word & Self::INDEX_MASK) == Self::INDEX_MASK
    }
}

/// The "next link" storage for a [`TaggedIndexStack`]. Each pushed index's next
/// pointer (another index, or [`TAIL`]) lives here — slot-resident in caller
/// storage (the production shape) or in an owned array ([`ArrayLinks`]).
///
/// # Ordering contract
///
/// Implementations MUST use `Acquire` on [`load_next`](Self::load_next) and
/// `Release` on [`store_next`](Self::store_next): the stack relies on this
/// pairing so a pop that observes a slot as the head (via its `Acquire` CAS
/// success) also sees the link a pusher wrote (via its `Release` store) before
/// publishing that slot as head.
///
/// This requirement is deliberately STRONGER than the stack's own internal
/// minimum. Each [`store_next`](Self::store_next) is sequenced-before the
/// pushing thread's `Release` CAS on the head — and a release operation
/// publishes ALL of its thread's prior writes, whatever tags those writes
/// carry themselves — and each [`load_next`](Self::load_next) is
/// sequenced-after the popping thread's `Acquire` observation of that head.
/// Given the stack's own head orderings, even `Relaxed` links would therefore
/// be ordered correctly for this stack's own usage. The full pairing is
/// mandated anyway so that a [`Links`] implementation stays correct on its
/// own terms, rather than being coupled to the stack's internal head
/// orderings — an implementation detail that could change. On weakly-ordered
/// targets, where `Acquire`/`Release` cost real instructions, read this as
/// considered defence-in-depth for an openly-implementable trait, not
/// naivety.
///
/// # Stability
///
/// This trait is intentionally OPEN to external implementation — slot-resident
/// links in caller-owned storage (rather than an owned array like
/// [`ArrayLinks`]) is the whole design point. New methods will only ever be
/// added with default bodies (or via a major version bump); this trait is not
/// sealed.
pub trait Links {
    /// Load the "next" link for `index` with `Acquire` ordering.
    fn load_next(&self, index: u32) -> u32;

    /// Store the "next" link for `index` with `Release` ordering. This is the
    /// ONLY write the stack makes to link storage, and only during a push — the
    /// lazy-link (RAD-1) discipline: link storage is never eagerly initialised.
    fn store_next(&self, index: u32, next: u32);
}

/// An owned `[AtomicU32; N]` link backing for standalone use of
/// [`TaggedIndexStack`] (when there is no pre-existing slot storage to host the
/// links). Every link starts at `0` — matching OS-zeroed backing — and is only
/// ever written by a push (RAD-1: no eager free-list chaining).
#[derive(Debug)]
pub struct ArrayLinks<const N: usize> {
    next: [AtomicU32; N],
}

impl<const N: usize> ArrayLinks<N> {
    /// Construct `N` links, every one at `0`. NOT a bulk free-list init — links
    /// only become meaningful once their index is pushed (RAD-1). Under
    /// `--cfg loom` this cannot be `const` (loom's atomics have no `const` ctor).
    #[cfg(not(loom))]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next: [const { AtomicU32::new(0) }; N],
        }
    }

    /// Construct `N` links, every one at `0` (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            next: core::array::from_fn(|_| AtomicU32::new(0)),
        }
    }
}

impl<const N: usize> Default for ArrayLinks<N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const N: usize> Links for ArrayLinks<N> {
    fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

/// A lock-free LIFO free-list of indices with an ABA-defeating tag packed into
/// the head word. Const-generic over the index width `INDEX_BITS`.
///
/// The stack owns ONLY the head (`AtomicU64`); the per-index next links live in
/// caller-supplied [`Links`] storage passed to [`push`](Self::push) /
/// [`pop`](Self::pop). A fresh stack is EMPTY (lazy links, RAD-1) — the caller
/// pushes indices as they become free.
#[derive(Debug)]
pub struct TaggedIndexStack<const INDEX_BITS: u32> {
    /// INVARIANT (release sequence): every modification of `head` MUST be a
    /// compare_exchange (an RMW). Today both writers are —
    /// [`push`](TaggedIndexStack::push)'s `Release` CAS and
    /// [`pop`](TaggedIndexStack::pop)'s `Acquire` CAS (plus the loom-only
    /// `cas_head_for_test`, also a CAS; constructing the atomic in `new` is
    /// initialization, not a modification, and `raw_head` only loads). Per
    /// the release-sequence rule, a release sequence continues through every
    /// subsequent RMW to the same location regardless of those RMWs' own
    /// orderings, so with every write here an RMW the release sequence headed
    /// by any push's `Release` CAS stays UNBROKEN across all later
    /// modifications. That is what lets `pop`'s successful CAS be plain
    /// `Acquire` instead of `AcqRel`: any later `Acquire` read of a value
    /// this pop wrote still lands inside that push's release sequence, so the
    /// happens-before edge back to the link-writing push survives
    /// transitively.
    ///
    /// Do NOT add a plain `store` to this field (e.g. a hypothetical
    /// `clear()`/`reset()`, or a `Drop` impl zeroing it). A non-RMW write
    /// severs every release sequence it follows; after that, `pop`'s
    /// `Acquire`-only success ordering can silently un-publish links on
    /// weakly-ordered targets — no compile error, and likely no test failure
    /// on x86. If such an API is ever genuinely needed, promote `pop`'s
    /// success ordering to `AcqRel` in the same change. (Plain loads are
    /// harmless: they modify nothing, so they break no sequence.)
    head: AtomicU64,
}

impl<const INDEX_BITS: u32> TaggedIndexStack<INDEX_BITS> {
    /// A fresh, EMPTY stack (head = the bootstrap empty sentinel, tag 0). Under
    /// `--cfg loom` this cannot be `const` (loom's atomics have no `const` ctor).
    #[cfg(not(loom))]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            head: AtomicU64::new(TaggedIndex::<INDEX_BITS>::empty()),
        }
    }

    /// A fresh, EMPTY stack (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            head: AtomicU64::new(TaggedIndex::<INDEX_BITS>::empty()),
        }
    }

    /// Push `index` onto the stack (classic Treiber push with a tag bump).
    ///
    /// Writes `index`'s next link (the current head's index, or [`TAIL`] if the
    /// stack is empty) under `Release`, bumps the tag (the ABA defence), then
    /// CASes the head to `(index, tag + 1)`. `index` MUST be a valid index
    /// (`< TaggedIndex::INDEX_MASK`) — a violation panics (see `# Panics`)
    /// rather than being trusted, because a corrupted head word downstream lets
    /// a later `pop` return an index nobody actually pushed, which in the
    /// parent allocator means handing out a slot that is still live elsewhere
    /// — memory unsafety reachable from this `#![forbid(unsafe_code)]` crate's
    /// 100% safe public API. The real bound is also `index != TAIL`: since
    /// `INDEX_BITS` is compile-time capped at `32` (see [`TaggedIndex`]'s
    /// `_CHECK_BITS`), `INDEX_MASK` can never exceed `u32::MAX` (`TAIL`), so
    /// `index < INDEX_MASK` already implies `index != TAIL` for every
    /// representable width — the two conditions coincide instead of needing to
    /// be asserted separately.
    ///
    /// # Caller contract
    ///
    /// `index` must NOT already be reachable from the stack: every index on
    /// the stack must have been placed there by exactly one `push` and not
    /// yet popped. Re-pushing an index that is still live is a
    /// caller-contract violation this method cannot catch — and cannot even
    /// check cheaply, because liveness is a property of the whole link chain
    /// and verifying it would cost an O(n) walk of that chain on every push.
    /// (Unlike the two subtleties the crate root documents — H-2 and RAD-1 —
    /// this one is enforced by caller discipline, not structurally.)
    ///
    /// What `push` DOES check, unconditionally on every call, is the separate
    /// `index < INDEX_MASK` range bound (see `# Panics` below); that observes
    /// only the index's numeric width, never whether it is already live.
    ///
    /// Violating the liveness rule corrupts the free-list silently: `push`
    /// overwrites `index`'s link with the current head, so if `index` was
    /// still chained into the stack, the chain closes a cycle — following it
    /// from the head reaches `index` again, and `index` now links back to
    /// the very head it was reached from. `pop` then stops returning `None`
    /// (the chain never reaches [`TAIL`] again) and hands the same index to
    /// two different callers, which in the parent allocator means two owners
    /// of one slot.
    ///
    /// # Panics
    ///
    /// Panics if `index >= INDEX_MASK` (the empty sentinel is reserved), in
    /// both debug and release builds — this is a caller-contract violation
    /// checked unconditionally, not a `debug_assert!`, because the failure
    /// mode is silent free-list corruption rather than a merely-suboptimal
    /// fallback.
    pub fn push<L: Links + ?Sized>(&self, links: &L, index: u32) {
        assert!(
            (index as u64) < TaggedIndex::<INDEX_BITS>::INDEX_MASK,
            "index must be < INDEX_MASK (the empty sentinel is reserved)"
        );
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            // The link this index chains to: the current head's index, or TAIL
            // if the stack is empty. The empty sentinel packs INDEX_MASK, which
            // numerically equals TAIL (`u32::MAX`) only when INDEX_BITS == 32;
            // for other widths they differ. We spell the empty→TAIL mapping out
            // explicitly so the invariant never rests on that coincidence.
            let next_link = if TaggedIndex::<INDEX_BITS>::is_empty(head) {
                TAIL
            } else {
                let (cur_idx, _tag) = TaggedIndex::<INDEX_BITS>::unpack(head);
                cur_idx as u32
            };
            // Write the link under Release so a concurrent pop's Acquire read of
            // this slot's link (after observing it as head) sees it. This is the
            // ONLY link write — never an eager init (RAD-1).
            links.store_next(index, next_link);
            // Advance the tag (the ABA fix) and CAS the head to this index.
            let (_cur_idx, tag) = TaggedIndex::<INDEX_BITS>::unpack(head);
            let new_tag = tag.wrapping_add(1);
            let new_head = TaggedIndex::<INDEX_BITS>::pack(index as u64, new_tag);
            // Release on success so a pop's Acquire sees the link we wrote;
            // Relaxed on failure (retry).
            match self
                .head
                .compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(actual) => head = actual,
            }
        }
    }

    /// Pop the top index off the stack (classic Treiber pop), or `None` if
    /// empty.
    ///
    /// Loads the tagged head, reads its next link, then CASes the head to that
    /// link with the SAME tag (a pop never bumps the tag). The tag in the high
    /// bits defeats ABA: if a concurrent thread pops-then-repushes the SAME
    /// index between our load and our CAS, the tag advances and our CAS fails,
    /// forcing a retry.
    ///
    /// **H-2 empty transition:** when the popped element is the last one
    /// (`next == TAIL`), the new head packs the empty sentinel's index with the
    /// RUNNING tag we just observed — NOT tag 0 — so the ABA tag keeps counting
    /// across the empty→non-empty churn. Resetting to 0 here reopens ABA (see
    /// the crate docs' H-2 section).
    #[must_use = "a popped index is removed from the free-list; discarding it leaks the slot"]
    pub fn pop<L: Links + ?Sized>(&self, links: &L) -> Option<u32> {
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            if TaggedIndex::<INDEX_BITS>::is_empty(head) {
                return None;
            }
            let (idx_v, tag) = TaggedIndex::<INDEX_BITS>::unpack(head);
            let index = idx_v as u32;
            // Read the next link BEFORE the CAS (the push stored it under
            // Release; our Acquire observation of head — whether from the
            // initial load OR from a retry CAS failure — synchronizes with it).
            let next = links.load_next(index);
            let new_head = if next == TAIL {
                // H-2: preserve the RUNNING tag across the empty transition.
                TaggedIndex::<INDEX_BITS>::pack(TaggedIndex::<INDEX_BITS>::empty_index(), tag)
            } else {
                TaggedIndex::<INDEX_BITS>::pack(next as u64, tag)
            };
            // Acquire on success with NO Release half is sound ONLY because
            // every write to `head` is an RMW: this CAS stays inside the
            // release sequence headed by the push that `Release`d the link
            // being handed out, so our own write need not head one. See the
            // INVARIANT on the `head` field — a plain `store` there would
            // sever that sequence and make this ordering unsound.
            match self
                .head
                .compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire)
            {
                Ok(_) => return Some(index),
                Err(actual) => head = actual,
            }
        }
    }

    /// The raw packed head word (`Acquire`) — for this crate's own diagnostics
    /// and tests only. The index half is a live top-of-stack index or
    /// [`empty_index`](TaggedIndex::empty_index); the high bits are the running
    /// tag. `Acquire` so a loom test that splits a pop's read from its CAS (to
    /// open the ABA window) still forms the same happens-before edge the real
    /// `pop`'s `Acquire` head load does.
    ///
    /// `#[doc(hidden)]`: this is a `pub fn` (so `tests/` — an external crate
    /// from this crate's own perspective — can reach it) but NOT a stable,
    /// documented part of the public API; it is not exercised by any
    /// production caller and carries no semver stability guarantee. See this
    /// project's established `#[doc(hidden)]` test-only-forwarder convention.
    #[doc(hidden)]
    #[must_use]
    pub fn raw_head(&self) -> u64 {
        self.head.load(Ordering::Acquire)
    }

    /// **loom-test-only** raw CAS on the head word, exposed so the shipped loom
    /// proof (`tests/loom_aba.rs`) can split a pop's head-load from its CAS —
    /// opening the ABA window the real `pop` closes internally — and drive the
    /// buggy-drain counterfactual, all against the REAL head atomic. NOT part of
    /// the public API: it is compiled only under `--cfg loom`.
    ///
    /// # Errors
    ///
    /// Forwards `AtomicU64::compare_exchange`'s `Err(actual)` on CAS failure.
    #[cfg(loom)]
    pub fn cas_head_for_test(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.head.compare_exchange(current, new, success, failure)
    }
}

impl<const INDEX_BITS: u32> Default for TaggedIndexStack<INDEX_BITS> {
    fn default() -> Self {
        Self::new()
    }
}
