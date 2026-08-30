//! `tagged-index-stack` — a lock-free LIFO free-list of small **indices** (a
//! *slot recycler*) whose head is a single atomic word packing an
//! `(index | tag)` pair, where a wrapping generation **tag** in the high bits
//! structurally defeats the ABA problem for every permitted `INDEX_BITS`.
//! That is a derived claim, not a slogan: the enforced `1..=16` cap on
//! `INDEX_BITS` guarantees every legal configuration a tag of at least 48
//! bits, and the "Tag-width budget" section below derives, from
//! cache-coherence throughput on the single head cache line, that such a tag
//! cannot repeat within any physically plausible observation window. (The
//! tag is not strictly monotonic — a strictly monotonic counter never
//! repeats a value, and this one wraps — it just never repeats on a
//! timescale the coherence protocol can deliver.) Allocation-free, `no_std`,
//! `#![forbid(unsafe_code)]`.
//!
//! This is the canonical "recycle a small integer id" primitive that slab
//! allocators, object pools, entity-component stores, and connection tables all
//! reinvent — and routinely reinvent *wrong*. The two subtleties people get
//! wrong (documented below) are the **H-2 empty-transition tag preservation**
//! and the **lazy link discipline** (internally: RAD-1); both are structurally enforced
//! here.
//!
//! # The packed word — [`TaggedIndex`]
//!
//! The stack head is one `AtomicU64` holding a [`TaggedIndex`]`<INDEX_BITS>`:
//! the low `INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS` bits
//! carry a wrapping generation **tag** bumped on every successful PUSH. The
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
//! each index, so the avoided cost is a first-touch commit proportional to the
//! pool size. [`ArrayLinks::new`] likewise starts every link
//! at `0` (the zero value), matching OS-zeroed backing, rather than eagerly
//! chaining a full free-list.
//!
//! Because links are lazy, a freshly-constructed stack is EMPTY — the caller
//! pushes indices in as they become free. This crate does NOT offer a "start
//! with `0..N` all pushed" constructor precisely because that would require an
//! eager link-chaining pass, defeating RAD-1. (A caller that genuinely wants
//! every index free from the start pushes `0..N` itself, or mints fresh
//! indices via a separate monotonic counter and only ever pushes RECYCLED
//! ones onto this stack.)
//!
//! # Tag-width budget — the wrap-time bound behind the ABA guarantee
//!
//! A tag defends against ABA only while it does not recur: a stale CAS can
//! succeed again only if the head word returns to the exact `(index, tag)`
//! pair the victim is holding, which takes a FULL tag wrap — `2^TAG_BITS`
//! successful pushes anywhere in the stack, the last of them re-pushing the
//! victim's own index. The time a wrap takes is
//!
//! ```text
//! wrap_time = 2^TAG_BITS / aggregate_successful_push_rate
//! ```
//!
//! and the rate term is bounded by HARDWARE, not by the workload. The tag is
//! GLOBAL to the whole stack, not per-slot: every successful push — of any
//! index, from any thread — is a compare-exchange (a locked RMW) on the ONE
//! `AtomicU64` head word, so the rate in the formula is the stack's AGGREGATE
//! successful-push rate across ALL slots, and every one of those pushes
//! serializes on a single cache line whose exclusive ownership must transfer
//! between cores. That transfer cost caps the aggregate rate at roughly
//! `10^8` to `10^9` RMWs/sec no matter how many threads contend — more
//! contention only makes the line's ownership transfers slower, never
//! faster. (This crate's own benchmarks peak around `10^6` to `10^7` ops/sec,
//! far under that ceiling.)
//!
//! Taking a generous `2 × 10^8` successful pushes/sec as the working ceiling:
//! at `INDEX_BITS = 16` — the widest permitted index half, 65535 usable
//! indices with the `0xFFFF` empty sentinel reserved above them — the tag
//! gets the other **48 bits**, wrapping at `2^48 ≈ 2.8 × 10^14`, and a wrap
//! takes `2^48 / (2 × 10^8) ≈ 16` days; even at the optimistic top of the
//! hardware range it is still `2^48 / 10^9 ≈ 3.3` days. And a wrap is only
//! the PRECONDITION for a collision: cashing one in further requires that
//! the head line stay saturated at the coherence ceiling continuously for
//! the entire span AND that one specific victim thread sit parked,
//! motionless, holding its stale snapshot the whole time. This bound is why
//! `INDEX_BITS > 16` is REJECTED at compile time
//! (`TaggedIndex::_CHECK_BITS`) rather than merely discouraged: at
//! `INDEX_BITS = 24` the tag would be 40 bits, `2^40 / (2 × 10^8) ≈ 92`
//! minutes at the same ceiling — a long debugger pause or OS scheduling
//! delay defeats that — and the pre-cap `INDEX_BITS = 32` maximum gave only
//! `2^32 / (2 × 10^8) ≈ 21` seconds, within reach of ordinary scheduling
//! jitter. Within the permitted range a caller still trades index range
//! against tag headroom, but never below the 48-bit floor.
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
/// The low `INDEX_BITS` bits carry a slot index; the high `64 - INDEX_BITS`
/// bits carry a wrapping generation ABA tag. The all-ones index value
/// ([`empty_index`](Self::empty_index)) is reserved as the empty-stack sentinel,
/// so valid indices are `0 .. (1 << INDEX_BITS) - 1`.
///
/// This is a zero-sized namespace of `const fn` bit operations — no state, no
/// memory, no `unsafe`, strict-provenance-clean by construction (it packs a
/// plain integer index, never a pointer/address).
#[derive(Debug, Clone, Copy)]
pub struct TaggedIndex<const INDEX_BITS: u32>;

impl<const INDEX_BITS: u32> TaggedIndex<INDEX_BITS> {
    /// Compile-time guard: `INDEX_BITS` must be in `1..=16` so both halves are
    /// non-empty, the shifts are well-defined, every representable index fits
    /// in the `u32` that [`push`](TaggedIndexStack::push) actually takes, AND
    /// the tag half keeps a minimum of 48 bits.
    ///
    /// Widths above 16 are rejected rather than merely discouraged: the 16 cap
    /// guarantees every legal configuration at least a 48-bit ABA tag, the
    /// cache-line-throughput-derived floor below which a tag wrap stops being
    /// physically implausible (see the crate docs' "Tag-width budget"
    /// section for the full derivation). The `u32` bound is respected a
    /// fortiori: `push` takes a `u32` index, so `INDEX_BITS > 32` could never
    /// buy reachable index range anyway — it only shrinks the tag budget — and
    /// worse, it would make `INDEX_MASK` exceed `u32::MAX`, letting
    /// `index == u32::MAX` (the internal [`TAIL`] sentinel) silently pass the
    /// `< INDEX_MASK` runtime guard and corrupt a chain. (At every legal width
    /// `INDEX_MASK <= 0xFFFF`, so the historical `INDEX_MASK == TAIL`
    /// coincidence at width 32 is now structurally impossible.) Capping at
    /// compile time closes that class of bug structurally instead of requiring
    /// every caller to separately exclude `TAIL` at runtime.
    ///
    /// This `const` is forced to evaluate from EVERY public associated item of
    /// `TaggedIndex<INDEX_BITS>`: [`pack`](Self::pack) references it via a
    /// `let () = Self::_CHECK_BITS;` statement, `INDEX_MASK` and
    /// [`TAG_BITS`](Self::TAG_BITS) evaluate it in their own initializers, and
    /// [`unpack`](Self::unpack), [`empty_index`](Self::empty_index),
    /// [`is_empty`](Self::is_empty), and [`empty`](Self::empty) all route through
    /// `INDEX_MASK` or `pack`, while [`try_pack`](Self::try_pack) forces it
    /// directly with the same `let ()` statement as `pack` — so an
    /// out-of-range `INDEX_BITS` cannot reach any associated item without
    /// tripping this guard.
    const _CHECK_BITS: () = assert!(
        INDEX_BITS >= 1 && INDEX_BITS <= 16,
        "INDEX_BITS must be in 1..=16: the tag half must keep at least 48 bits \
         (the cache-line-throughput-derived floor against ABA tag wrap — see \
         the crate docs' \"Tag-width budget\" section), both halves must be \
         non-empty, and every valid index must fit in push's u32 parameter"
    );

    /// Bit-mask for the low `INDEX_BITS` (the index half), e.g. `0xFFFF`
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
    /// Through the stack's own API this truncation is unreachable by
    /// construction: indices only ever enter via
    /// [`push`](TaggedIndexStack::push), whose stricter `< INDEX_MASK` bound
    /// lies below this truncation boundary, so no index the stack packs is
    /// ever truncated. External callers of `pack` get no such protection and
    /// must uphold the `< 2^INDEX_BITS` precondition themselves; callers
    /// that cannot should use [`try_pack`](Self::try_pack), the checked
    /// twin that returns `None` rather than a silently truncated word.
    /// Note the two bounds are deliberately different ranges, not a typo: `< 2^INDEX_BITS`
    /// is pack's truncation boundary (where the silent masking kicks in),
    /// while `push`'s `< INDEX_MASK` (`INDEX_MASK == 2^INDEX_BITS - 1`) is
    /// stricter because it also excludes the reserved empty sentinel.
    #[must_use]
    pub const fn pack(index: u64, tag: u64) -> u64 {
        // Force the compile-time bounds check to be evaluated.
        let () = Self::_CHECK_BITS;
        (tag << INDEX_BITS) | (index & Self::INDEX_MASK)
    }

    /// The checked twin of [`pack`](Self::pack): `Some(word)` with `word`
    /// EXACTLY what [`pack`](Self::pack) returns for the same inputs, or
    /// `None` when either half is out of range — `index >= 2^INDEX_BITS`
    /// (which `pack` would silently mask to a DIFFERENT, valid-looking
    /// index, or to the [empty sentinel](Self::empty_index) if the low bits
    /// happen to be all ones) or `tag >= 2^TAG_BITS` (whose high bits
    /// `pack`'s `tag << INDEX_BITS` would silently drop). Use this wherever
    /// the `< 2^INDEX_BITS` / `< 2^TAG_BITS` precondition is not already
    /// guaranteed by the calling logic; [`pack`](Self::pack) itself stays
    /// the trusted fast primitive for
    /// [`push`](TaggedIndexStack::push)/[`pop`](TaggedIndexStack::pop) and
    /// the other in-crate callers that uphold it.
    #[must_use]
    pub const fn try_pack(index: u64, tag: u64) -> Option<u64> {
        // Force the compile-time bounds check to be evaluated HERE, not
        // only via `TAG_BITS` in one branch or `pack` on the other: a
        // const evaluation taking the short-circuited branch would
        // otherwise skip both, weakening the documented
        // _CHECK_BITS-from-every-public-item invariant into a
        // branch-dependent one.
        let () = Self::_CHECK_BITS;
        if index >= (1u64 << INDEX_BITS) || tag >= (1u64 << Self::TAG_BITS) {
            None
        } else {
            Some(Self::pack(index, tag))
        }
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
/// pairing so a pop that observes a slot as the head also sees the link a
/// pusher wrote (via its `Release` store) before publishing that slot as
/// head. The load-bearing `Acquire` is the head observation itself — the
/// initial `Acquire` load of the head, or (on a retry) the PREVIOUS
/// iteration's `Acquire`-ordered CAS-failure read — which happens BEFORE the
/// [`load_next`](Self::load_next) call. The CAS is attempted only AFTER
/// [`load_next`](Self::load_next) has run, so its success ordering plays no
/// part in making that link visible.
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
/// This ordering contract speaks to ONE backing used consistently — it is
/// what makes a [`load_next`](Self::load_next) observe the
/// [`store_next`](Self::store_next) the stack performed, *given* that every
/// call reaches the same backing. It cannot say anything about a
/// [`TaggedIndexStack`] being handed a DIFFERENT backing instance (even of
/// the identical type) between calls: nothing binds the stack's head to the
/// backing a push wrote, so a fresh/different backing was never the target
/// of any [`store_next`](Self::store_next) the stack performed at all, and
/// coherence across the swap is broken trivially. That swap is a
/// caller-contract violation this crate cannot detect — see
/// [`push`](TaggedIndexStack::push)'s `# Caller contract` section ("ONE
/// `Links` backing for the whole life of a non-empty stack") for the full
/// rule and its concrete failure mode.
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
    /// # Panics
    ///
    /// Panics if `index >= N`. `N` (this backing's capacity) and the
    /// stack's `INDEX_BITS` are independent const parameters with nothing
    /// relating them, so a [`TaggedIndexStack`] can accept an index this
    /// backing cannot hold — see [`TaggedIndexStack::push`]'s note on the
    /// two bounds.
    fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    /// # Panics
    ///
    /// Panics if `index >= N` — the same bound as
    /// [`load_next`](Links::load_next); likewise independent of the stack's
    /// `INDEX_BITS` (see [`TaggedIndexStack::push`]'s note on the two
    /// bounds).
    fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

/// A lock-free LIFO free-list of indices with a wrapping generation tag packed
/// into the head word, structurally defeating ABA at every permitted
/// `INDEX_BITS` (the crate-root docs' "Tag-width budget" section carries the
/// wrap-time derivation). Const-generic over the index width `INDEX_BITS`.
///
/// The stack owns ONLY the head (`AtomicU64`); the per-index next links live in
/// caller-supplied [`Links`] storage passed to [`push`](Self::push) /
/// [`pop`](Self::pop). A fresh stack is EMPTY (lazy links, RAD-1) — the caller
/// pushes indices as they become free.
///
/// # Layout note — no cache-line isolation
///
/// This type is a bare `AtomicU64` with no cache-line padding or alignment
/// attribute of its own: it inherits the cache line of whatever struct embeds
/// it. If it lands adjacent to another frequently-modified atomic — say, a
/// slot counter bumped on every allocation — the two fields false-share:
/// each write invalidates the other core's copy of the line, and contending
/// cores ping-pong the line even though the two atomics are logically
/// independent. That costs throughput, never correctness, and only matters
/// when the line is genuinely hot. Fix it at the embedding site when a
/// profile shows it — wrap this stack in a `#[repr(align(64))]` newtype or
/// interpose padding — rather than paying for blanket alignment inside the
/// crate, which would waste most of a cache line for every embedder that
/// does not need the isolation.
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
    /// `INDEX_BITS` is compile-time capped at `16` (see [`TaggedIndex`]'s
    /// `_CHECK_BITS`), `INDEX_MASK` can never exceed `u32::MAX` (`TAIL`), so
    /// `index < INDEX_MASK` already implies `index != TAIL` for every
    /// representable width — one guard covers both conditions, no separate
    /// `TAIL` assertion is needed.
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
    /// ## Second rule, same contract: ONE `Links` backing for the whole life
    /// of a non-empty stack
    ///
    /// [`push`](Self::push) and [`pop`](Self::pop) are independently generic
    /// over `&L` on every call — nothing in the signatures binds the head to
    /// the specific backing a push wrote. The caller must therefore uphold
    /// what the type system cannot express: the SAME logical [`Links`]
    /// backing must serve every push and pop across a [`TaggedIndexStack`]
    /// instance's entire non-empty lifetime.
    ///
    /// 1. **One backing for the whole life of a non-empty stack.** Swapping
    ///    to a DIFFERENT backing instance (even of the identical type and
    ///    width) while the stack holds live indices is undefined WITHIN
    ///    this crate's safe-Rust guarantees: memory-safe (no unsafe code
    ///    runs), but logically corrupting.
    /// 2. **Stable one-to-one index↔cell mapping.** Every valid index must
    ///    map to the SAME link cell for the backing's whole lifetime.
    /// 3. **Coherence of `store_next`/`load_next`.** A
    ///    [`load_next`](Links::load_next) of index `i` must observe the
    ///    most recent [`store_next`](Links::store_next) of `(i, _)` this
    ///    crate itself performed. The [`Links`] trait's Acquire/Release
    ///    ordering contract (documented on the trait) already guarantees
    ///    THIS for a single, stable backing — but says nothing about using
    ///    two DIFFERENT backings, which breaks it trivially, since a
    ///    fresh/different backing was never the target of any `store_next`
    ///    this crate performed at all.
    /// 4. **`load_next` must return only [`TAIL`] or a currently-valid
    ///    index** for the backing in use. A backing that returns an
    ///    arbitrary, stale, or foreign value can corrupt the free-list
    ///    with no adversarial intent at all: a fresh zero-initialized
    ///    [`ArrayLinks`] "coincidentally" returns `0` for every index, and
    ///    if that happens to equal the live head's own index,
    ///    [`pop`](Self::pop)'s compare-exchange `current -> current`
    ///    succeeds trivially.
    /// 5. **Backing lifetime.** The backing and its cells must remain alive
    ///    and keep their identity for as long as the stack's head can
    ///    reference them — in practice, for the stack's own lifetime.
    ///
    /// Violating this contract is memory-safe (no unsafe code runs) but
    /// logically catastrophic: `pop` against an unrelated backing can read a
    /// value that happens to equal the current head's own index, so its CAS
    /// succeeds as a no-op (`current -> current`) and returns the SAME index
    /// again and again — an infinite double-issue of one index — which in
    /// the parent allocator means handing the same slot to two different
    /// owners. The crate cannot detect a backing swap. This rule is caller
    /// discipline, unenforceable at compile time AND at runtime — unlike the
    /// `index < INDEX_MASK` bound this very method's `assert!` DOES enforce
    /// on every call (see `# Panics`).
    ///
    /// # Panics
    ///
    /// Panics if `index >= INDEX_MASK` (the empty sentinel is reserved), in
    /// both debug and release builds — this is a caller-contract violation
    /// checked unconditionally, not a `debug_assert!`, because the failure
    /// mode is silent free-list corruption rather than a merely-suboptimal
    /// fallback.
    ///
    /// That guard is the only bound this method itself checks, and it
    /// depends on `INDEX_BITS` alone. The supplied [`Links`] implementation
    /// may impose its own, separate bound: `N` in [`ArrayLinks`]`<N>` and
    /// `INDEX_BITS` are independent const parameters with nothing relating
    /// them — a `TaggedIndexStack<16>` accepts indices up to 65534 even over
    /// an `ArrayLinks<256>` that holds only `0..=255` — so out-of-range
    /// access in the links layer ([`ArrayLinks::store_next`] panics on
    /// `index >= N`) is a second panic source the guard above does not
    /// cover.
    pub fn push<L: Links + ?Sized>(&self, links: &L, index: u32) {
        assert!(
            (index as u64) < TaggedIndex::<INDEX_BITS>::INDEX_MASK,
            "index must be < INDEX_MASK (the empty sentinel is reserved)"
        );
        let mut head = self.head.load(Ordering::Acquire);
        loop {
            // Unpack the current head ONCE: the index half chains this push to
            // the top of the stack (below), the tag half feeds the ABA bump.
            let (cur_idx, tag) = TaggedIndex::<INDEX_BITS>::unpack(head);
            // The link this index chains to: the current head's index, or TAIL
            // if the stack is empty. The empty sentinel packs INDEX_MASK, which
            // can no longer equal TAIL (`u32::MAX`) at ANY legal width: the
            // `1..=16` cap on INDEX_BITS (`TaggedIndex::_CHECK_BITS`) keeps
            // `INDEX_MASK <= 0xFFFF`, so the historical INDEX_BITS == 32
            // coincidence is structurally impossible. We still spell the
            // empty→TAIL mapping out explicitly (and keep `is_empty` a
            // dedicated check on the raw word rather than deriving emptiness
            // from the unpacked index) so the invariant never rests on any
            // numeric coincidence.
            let next_link = if TaggedIndex::<INDEX_BITS>::is_empty(head) {
                TAIL
            } else {
                cur_idx as u32
            };
            // Write the link under Release so a concurrent pop's Acquire read of
            // this slot's link (after observing it as head) sees it. This is the
            // ONLY link write — never an eager init (RAD-1).
            links.store_next(index, next_link);
            // Advance the tag (the ABA fix) and CAS the head to this index.
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
    /// bits is the ABA defence: if a concurrent thread pops-then-repushes the
    /// SAME index between our load and our CAS, the tag advances and our CAS
    /// fails, forcing a retry. (The only residual hazard is a full tag wrap
    /// inside that window, which the wrap-time bound in the crate docs'
    /// "Tag-width budget" section places outside any physically plausible
    /// observation window at every permitted width.)
    ///
    /// **H-2 empty transition:** when the popped element is the last one
    /// (`next == TAIL`), the new head packs the empty sentinel's index with the
    /// RUNNING tag we just observed — NOT tag 0 — so the ABA tag keeps counting
    /// across the empty→non-empty churn. Resetting to 0 here reopens ABA (see
    /// the crate docs' H-2 section).
    ///
    /// Pop reaches link storage only through the supplied [`Links`]
    /// implementation ([`load_next`](Links::load_next)), which may panic on
    /// an out-of-range index under its OWN, narrower bound (e.g.
    /// [`ArrayLinks::load_next`]'s `index >= N`) — the links-layer panic
    /// source [`push`](Self::push)'s `# Panics` section describes; nothing
    /// here re-validates the index beyond what `push` guaranteed when the
    /// index was admitted.
    ///
    /// The `&L` passed here is subject to the same caller contract as
    /// [`push`](Self::push)'s — and `pop` is equally exposed to a backing
    /// swap (the corrupting call in that failure mode is typically a `pop`
    /// itself): see [`push`](Self::push)'s `# Caller contract` section,
    /// "ONE `Links` backing for the whole life of a non-empty stack".
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

    /// Whether the stack is currently empty. Advisory only — a concurrent
    /// push or pop can make the answer stale the instant this returns, in
    /// either direction — so use it for diagnostics/monitoring, not for
    /// correctness decisions ([`pop`](Self::pop)'s `None` is the
    /// authoritative empty check).
    ///
    /// A `Relaxed` load is sufficient here because the result is explicitly
    /// racy: no ordering is being promised, and a plain load touches nothing,
    /// so the release-sequence invariant documented on the private `head`
    /// field is untouched.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        TaggedIndex::<INDEX_BITS>::is_empty(self.head.load(Ordering::Relaxed))
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
