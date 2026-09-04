//! The tagged-index-stack implementation, gated as one unit by the crate
//! root's valid-configuration `#[cfg]`.
//!
//! `compile_error!` does not stop name-resolution of sibling items, so invalid
//! configurations must fail with only the named error: `lib.rs` cfgs this
//! module OUT under every invalid configuration and re-exports it (`pub use
//! imp::*`) under every valid one, so public paths are unchanged. The whole
//! body in one module is this single-responsibility crate's established file
//! structure.

// The atomics are aliased so loom can shadow the REAL stack type: under
// `--cfg loom` they are built on `loom::sync::atomic`, so the shipped loom
// tests exercise the actual code; otherwise `core::sync::atomic`, keeping the
// crate zero-non-std-dep.
#[cfg(not(loom))]
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
#[cfg(loom)]
use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};

/// The "no next" sentinel stored in a slot's link to denote the BOTTOM of the
/// stack (the first index pushed onto an empty stack chains to this).
/// `u32::MAX`.
///
/// Note this is distinct from the "stack empty" head sentinel
/// ([`TaggedIndex::empty_index`]): `TAIL` marks a per-slot link's end-of-chain,
/// while the empty sentinel marks the HEAD word as carrying no index at all
/// (their low bits do agree — `TAIL & INDEX_MASK == INDEX_MASK` is a
/// mathematical identity at every legal width, all-ones AND
/// all-ones-low-bits — but their ROLES are distinct). The two mappings are
/// kept spelled out separately in
/// [`push_index`](StackOps::push_index) /
/// [`pop_index`](StackOps::pop_index) purely for readability.
pub const TAIL: u32 = u32::MAX;

/// Exponential-backoff cap for `push_index`/`pop_index`'s CAS-retry arms: the
/// Kth lost CAS within one call, counting from 0, spins `1 << K` times via
/// [`core::hint::spin_loop`] before retrying (K = 0 spins once, up to
/// `1 << BACKOFF_SPIN_CAP` at the cap). The cap is enforced by
/// [`Backoff`]`::spin`'s saturation — `K` never exceeds it — and is a
/// per-call local, reset on every fresh `push_index`/`pop_index`; backoff
/// happens within one call's retry loop, never across calls. `pop_index`
/// skips the backoff when the lost CAS reveals the stack just went empty
/// (documented at [`pop_index`](StackOps::pop_index)).
///
/// The cap is 6 — a deliberate fairness-vs-throughput compromise, not a
/// low-contention optimum: caps 8/10 give more aggregate throughput but
/// measurably worse per-thread fairness under oversubscription, while caps
/// 0/4 are fairer but slower. Measurements and the full fairness/throughput
/// tables are in `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (a repository
/// file, not part of the published package). Lock-free is not
/// starvation-free — see the crate-root doc's "Lock-freedom and starvation"
/// section for the measured trade.
const BACKOFF_SPIN_CAP: u32 = 6;

// `1u32 << K` masks/panics if `BACKOFF_SPIN_CAP` ever reaches 32 — the same technique [`TaggedIndex::_CHECK_BITS`] uses to
// turn a would-be shift-overflow into a compile error instead of a debug
// panic / silently masked shift in release.
const _: () = assert!(BACKOFF_SPIN_CAP < 32);

/// Per-call exponential-backoff state for the CAS-retry arms: wraps the retry
/// counter (`K`, starting at 0) that drives the spin-loop depth below, plus
/// (under `test-internals`/`loom` only) the PRE-increment at-cap verdict of
/// the most recent [`Backoff::spin`], reported by [`Backoff::spun_at_cap`].
/// Starts fresh every call, never persisted.
struct Backoff(
    u32,
    // Oracle flag: written by `spin`, read by `spun_at_cap`. Exists only
    // where the oracle counters exist — in a production build the field,
    // its write, and `spun_at_cap` are all compiled out together.
    #[cfg(any(feature = "test-internals", loom))] bool,
);

impl Backoff {
    fn new() -> Self {
        #[cfg(not(any(feature = "test-internals", loom)))]
        return Backoff(0);

        #[cfg(any(feature = "test-internals", loom))]
        return Backoff(0, false);
    }

    /// `#[inline]`: called from generic fns monomorphized in downstream
    /// crates — a non-`#[inline]` non-generic private fn would not be
    /// cross-crate-inlinable, a codegen regression in a hot path.
    #[inline]
    fn at_cap(&self) -> bool {
        self.0 >= BACKOFF_SPIN_CAP
    }

    /// Exponential backoff before retrying (BACKOFF_SPIN_CAP): spins
    /// `1 << K` times, letting the winning thread's Release CAS drain off
    /// the head cache line instead of every loser re-hammering it
    /// immediately. `K` grows only within one call.
    ///
    /// Under `test-internals`/`loom`, records in `.1` whether THIS retry
    /// spun at FULL depth (the PRE-increment `K` was already at the cap) —
    /// the oracle verdict [`Self::spun_at_cap`] reports. The check
    /// deliberately happens before the increment, so the oracle does not
    /// fire one retry early; the verdict cannot be recomputed after the
    /// fact, because a post-increment `K == BACKOFF_SPIN_CAP` is ambiguous
    /// between "was already at the cap" and "incremented into it".
    ///
    /// `#[inline]`: see [`Self::at_cap`] — same monomorphization/codegen
    /// reasoning.
    ///
    /// Capped, not unconditional: unbounded `K` would eventually be an
    /// `attempt to add with overflow` panic under overflow-checks after
    /// ~2^32 consecutive lost CASes in one call — remote, but free to
    /// close. The saturation also guarantees `self.0 <= BACKOFF_SPIN_CAP`
    /// at the shift, so no `.min` guard is needed on the shift expression.
    #[inline]
    fn spin(&mut self) {
        let at_cap = self.at_cap();
        for _ in 0..(1u32 << self.0) {
            core::hint::spin_loop();
        }
        if !at_cap {
            self.0 += 1;
        }
        #[cfg(any(feature = "test-internals", loom))]
        {
            self.1 = at_cap;
        }
    }

    /// Query-only oracle trigger for `PUSH_BACKOFF_CAP_REACH_COUNT` /
    /// `POP_BACKOFF_CAP_REACH_COUNT`: whether the most recent
    /// [`Self::spin`] spun at FULL depth (its PRE-increment `K` was already
    /// at the cap). Must be called AFTER `spin` — before it, the flag still
    /// holds the PREVIOUS retry's verdict. Mirrors [`Self::at_cap`]'s
    /// shape; `#[cfg]`-gated with the oracle counters it feeds.
    #[cfg(any(feature = "test-internals", loom))]
    #[inline]
    fn spun_at_cap(&self) -> bool {
        self.1
    }
}

/// A packed `(index | tag)` word with a compile-time-chosen index width.
///
/// The low `INDEX_BITS` bits carry a slot index; the high `64 - INDEX_BITS`
/// bits carry a strictly monotonic generation ABA tag that SEALS at
/// [`TAG_MAX`](Self::TAG_MAX) rather than wrapping. The all-ones index value
/// ([`empty_index`](Self::empty_index)) is reserved as the empty-stack sentinel,
/// so valid indices are `0 .. (1 << INDEX_BITS) - 1`.
///
/// This is a namespace of `const fn` bit operations, not a value type — no
/// state, no memory, no `unsafe`, strict-provenance-clean by construction (it
/// packs a plain integer index, never a pointer/address). Declared as an
/// UNINHABITED `enum` (zero variants) rather than a unit `struct`: a unit
/// struct is freely constructible, and closing that off later with a private
/// field would be a breaking change once published, whereas an uninhabited
/// `enum` has no constructor at all from the start.
pub enum TaggedIndex<const INDEX_BITS: u32> {}

impl<const INDEX_BITS: u32> TaggedIndex<INDEX_BITS> {
    /// Compile-time guard: `INDEX_BITS` must be in `1..=16` so both halves
    /// are non-empty, every valid index fits the `u32` that the whole
    /// index-carrying surface takes ([`push_index`](StackOps::push_index),
    /// [`pack`](Self::pack)'s parameter, [`unpack`](Self::unpack)'s index
    /// half, [`empty_index`](Self::empty_index) — all `u32`), with no
    /// casts, and the tag half keeps a minimum of 48 bits — the seal-time
    /// floor below which a head's pushes-until-sealed lifetime comes within
    /// reach of an ordinary long-running process (see the crate docs'
    /// "Tag-width budget" section). At
    /// every legal width `INDEX_MASK <= 0xFFFF`, so the historical
    /// `INDEX_MASK == TAIL` coincidence at the former width-32 cap is
    /// structurally impossible, and `index == TAIL` can never silently
    /// pass the runtime guard.
    ///
    /// This `const` is forced to evaluate from every associated item of
    /// `TaggedIndex<INDEX_BITS>`: [`pack`](Self::pack) forces it directly
    /// with a `let () = Self::_CHECK_BITS;` statement, `INDEX_MASK` and
    /// [`TAG_BITS`](Self::TAG_BITS) evaluate it in their own initializers,
    /// and [`unpack`](Self::unpack), [`empty_index`](Self::empty_index),
    /// [`is_empty`](Self::is_empty), [`empty`](Self::empty), and the
    /// crate-private `pack_truncating` all route through `INDEX_MASK` — so
    /// an out-of-range `INDEX_BITS` cannot reach any associated item
    /// without tripping this guard.
    const _CHECK_BITS: () = assert!(
        INDEX_BITS >= 1 && INDEX_BITS <= 16,
        "INDEX_BITS must be in 1..=16: the tag half must keep at least 48 bits \
         (the cache-line-throughput-derived floor against premature tag \
         exhaustion/seal — see the crate docs' \"Tag-width budget\" \
         section), both halves must be \
         non-empty, and every valid index must fit in the shared u32 index \
         half (pack/unpack/push_index/empty_index)"
    );

    /// Bit-mask for the low `INDEX_BITS` (the index half), e.g. `0xFFFF`
    /// for `INDEX_BITS = 16`. Its `u32`-typed form is the
    /// [`empty_index`](Self::empty_index) value.
    ///
    /// Forces `_CHECK_BITS` to evaluate here too — see `_CHECK_BITS`'s doc.
    pub const INDEX_MASK: u64 = {
        let () = Self::_CHECK_BITS;
        (1u64 << INDEX_BITS) - 1
    };

    /// The `u32` form of [`Self::INDEX_MASK`] — identical value. The index
    /// half is `u32`-typed end to end ([`pack`](Self::pack)'s parameter,
    /// [`unpack`](Self::unpack)'s first element, [`empty_index`](Self::empty_index));
    /// this mirror exists so those surfaces need no cast: `INDEX_BITS <= 16`
    /// (`_CHECK_BITS`), so `(1u32 << INDEX_BITS) - 1` derives it directly.
    const INDEX_MASK_U32: u32 = {
        let () = Self::_CHECK_BITS;
        (1u32 << INDEX_BITS) - 1
    };

    /// Number of bits carrying the tag (`64 - INDEX_BITS`). The tag is
    /// strictly monotonic and SEALS at [`TAG_MAX`](Self::TAG_MAX) — it does
    /// not wrap.
    pub const TAG_BITS: u32 = {
        let () = Self::_CHECK_BITS;
        64 - INDEX_BITS
    };

    /// Largest tag a head word can carry: `2^TAG_BITS - 1`. A push that
    /// observes this tag on the current head is refused
    /// (`Err(`[`TagExhausted`]`)`) instead of bumping it to `2^TAG_BITS`,
    /// which would wrap back to 0 and re-issue a `(index, tag)` head word
    /// that a popper parked since the previous cycle may still hold as its
    /// stale CAS expectation — see
    /// [`push_index`](StackOps::push_index)'s `# Errors` section and the
    /// crate-root docs' "The tag is strictly monotonic" section.
    /// [`pack`](Self::pack)`(_, TAG_MAX)` is `Some`; `pack(_, TAG_MAX + 1)`
    /// is `None`.
    pub const TAG_MAX: u64 = {
        let () = Self::_CHECK_BITS;
        (1u64 << Self::TAG_BITS) - 1
    };

    /// Pack `(index, tag)` into one `u64`, CHECKED: `Some(word)` for an
    /// in-range pair, `None` when either half is out of range — `index >=
    /// 2^INDEX_BITS` over the `u32` index parameter (which unchecked masking
    /// would silently turn into a
    /// DIFFERENT, valid-looking index, or into the
    /// [empty sentinel](Self::empty_index) if the low bits happen to be all
    /// ones) or `tag >= 2^TAG_BITS` (whose high bits a `tag << INDEX_BITS`
    /// shift would silently drop). The index parameter is `u32`; the tag half
    /// is `u64`. For an accepted pair the word is exactly
    /// `(index | tag << INDEX_BITS)`: both halves are already within their
    /// bit budgets, so no masking takes place and `unpack` recovers both
    /// halves exactly.
    ///
    /// Note the two bounds in this crate are deliberately different ranges:
    /// `< 2^INDEX_BITS` is this function's acceptance boundary, while
    /// [`push_index`](StackOps::push_index)'s `< INDEX_MASK`
    /// (`INDEX_MASK == 2^INDEX_BITS - 1`) is stricter because it also
    /// excludes the reserved empty sentinel. Packing the empty index with a
    /// tag IS accepted here — that is the legitimate H-2 shape
    /// ([`empty_index`](Self::empty_index)).
    ///
    /// `push_index`/`pop_index` do NOT call this function on the hot path:
    /// their inputs are already guaranteed within range by the crate's own
    /// guards — in particular, `push_index_impl`'s seal check refuses
    /// (`Err(`[`TagExhausted`]`)`) BEFORE ever bumping the tag when the
    /// observed tag is already [`TAG_MAX`](Self::TAG_MAX), so the tag handed
    /// to the truncating helper below is always `<= TAG_MAX` and this
    /// checked function's rejection path is never needed on that path. They
    /// pack through the crate-private truncating fast path `pack_truncating`
    /// instead purely to skip this function's redundant range re-check, not
    /// because production input can be out of range — see
    /// `pack_truncating`'s own doc for why reintroducing an out-of-range tag
    /// there would be a soundness regression, not a style choice.
    #[must_use]
    pub const fn pack(index: u32, tag: u64) -> Option<u64> {
        // Forced here too: a const eval taking the short-circuit branch
        // would otherwise skip both const paths.
        let () = Self::_CHECK_BITS;
        if index >= (1u32 << INDEX_BITS) || tag >= (1u64 << Self::TAG_BITS) {
            None
        } else {
            Some((tag << INDEX_BITS) | (index as u64))
        }
    }

    /// Truncating fast path: `(tag << INDEX_BITS) | index`. TRUSTS ITS
    /// PRECONDITION — the name is the contract: this silently produces a
    /// VALID-LOOKING word from invalid input, and no masking takes place:
    /// an over-wide index ORs its high bits across the index/tag boundary
    /// into the tag half, corrupting BOTH halves at once (a different
    /// index AND a different tag — nothing rounds invalid input to a
    /// benign value); an over-wide tag loses its high bits. If you cannot
    /// prove your halves are in range, use [`pack`](Self::pack), which
    /// rejects instead (see its doc for the checked semantics). The range
    /// proof is additionally tripped by a `debug_assert!` in the body —
    /// a debug-build check, never a release-build guarantee.
    ///
    /// Crate-private so the sharp edges stay in-crate; the only callers are
    /// [`push_index`](StackOps::push_index), [`pop_index`](StackOps::pop_index),
    /// and [`empty`](Self::empty). All three prove `tag <= TAG_MAX` before
    /// calling: `push_index_impl`'s seal check refuses with
    /// `Err(`[`TagExhausted`]`)` BEFORE ever bumping the tag when the
    /// observed tag is already [`TAG_MAX`](Self::TAG_MAX), so the
    /// `tag + 1` bump at the call site only ever runs on a tag
    /// `< TAG_MAX`, producing a value `<= TAG_MAX` that always fits within
    /// `TAG_BITS` — truncation never actually discards a bit on this path.
    ///
    /// The bump is plain `+` deliberately: the tag arriving here is always
    /// `< TAG_MAX` (above), so the addition can never overflow `u64` at
    /// any legal width — `TAG_MAX <= 2^63 - 1` when `INDEX_BITS` is in
    /// `1..=16` — in a debug build or in release. Plain `+` states that
    /// proven precondition directly; there is nothing for a wrapping or
    /// saturating operator to guard, and one would only invite the
    /// misreading that the operator is a safety mechanism.
    ///
    /// Note what the actual protection against a hypothetical future
    /// weakening or removal of the seal check is — the seal check itself,
    /// run before any side effect, NOT the choice of addition operator:
    /// with the check removed, the failure mode at these same legal widths
    /// would still not be `u64` arithmetic overflow. It would be a bump
    /// past [`TAG_MAX`](Self::TAG_MAX) whose high bits the
    /// `(tag << INDEX_BITS)` shift below silently discards past bit 63 —
    /// a semantically-wrapped pack (the tag field wraps exactly to 0)
    /// that no addition operator detects or prevents.
    ///
    /// This helper is NOT a wrap-on-truncate mechanism: it does not wrap
    /// the tag back to 0 in production use, and must never be made to —
    /// reintroducing wrap-on-truncation here would reopen the exact
    /// stale-CAS double-issue the run-8 P1-1 fix
    /// ([`TAG_MAX`](Self::TAG_MAX) + [`TagExhausted`]) exists to close
    /// (see the crate-root docs' "The tag is strictly monotonic" section).
    #[must_use]
    pub(crate) const fn pack_truncating(index: u32, tag: u64) -> u64 {
        let () = Self::_CHECK_BITS;
        debug_assert!(
            index as u64 <= Self::INDEX_MASK,
            "pack_truncating: index out of range — must be <= INDEX_MASK. \
             The `<=` is not a general invitation to pass INDEX_MASK: it \
             is admitted only for the empty-sentinel callers (empty() and \
             the H-2 drain path in pop_index_impl pack INDEX_MASK itself). \
             A push caller can never reach this call with INDEX_MASK — \
             push_index_impl's own >= INDEX_MASK guard panics upstream — \
             so push callers are strictly < INDEX_MASK"
        );
        debug_assert!(
            tag <= Self::TAG_MAX,
            "pack_truncating: tag out of range — must be <= TAG_MAX. \
             All callers prove `tag <= TAG_MAX` before calling: empty() \
             passes tag 0, push_index_impl's seal check refuses \
             (Err(TagExhausted)) BEFORE ever bumping when the observed \
             tag is already TAG_MAX — so the bumped tag is <= TAG_MAX — \
             and pop repacks an observed, already-valid tag. An \
             over-wide tag's high bits would be silently discarded by \
             the `(tag << INDEX_BITS)` shift below, yielding a \
             valid-looking word instead of a loud failure"
        );
        (tag << INDEX_BITS) | (index as u64)
    }

    /// Split a packed word back into `(u32 index, u64 tag)`.
    #[must_use]
    pub const fn unpack(word: u64) -> (u32, u64) {
        // INDEX_MASK <= 0xFFFF at every legal width per `_CHECK_BITS`, so the
        // AND result is <= 0xFFFF and the cast is lossless by construction —
        // the single, centralized, provably-dead truncation that replaces the
        // seven unchecked caller-side casts the old `(u64, u64)` signature
        // forced.
        ((word & Self::INDEX_MASK) as u32, word >> INDEX_BITS)
    }

    /// Bootstrap empty-stack word: index =
    /// [`empty_index`](Self::empty_index), tag = 0. A freshly-constructed
    /// [`StackHead`] is this.
    ///
    /// **Only bootstrap-time emptiness uses tag 0 unconditionally.** A RUNTIME
    /// empty transition (a pop that drains the last element) MUST preserve the
    /// running tag — see [`empty_index`](Self::empty_index); resetting to 0
    /// there reopens the ABA window (the crate docs' H-2 note).
    ///
    /// `#[doc(hidden)]`: part of this crate's test-only-forwarder
    /// convention — hidden from rustdoc's rendered navigation while staying
    /// callable; see the crate README's "Notes" section for the per-item
    /// breakdown. This is the only `#[doc(hidden)]` item that remains in a
    /// default build: unlike the test probes it is NOT feature-gated,
    /// because the crate's own bootstrap constructors
    /// ([`StackHead::new`] / [`ArrayIndexStack::new`]) call it. It also has
    /// one real in-workspace consumer outside this crate — `sefer-alloc`'s
    /// `#[cfg(loom)]` `bootstrap::loom_shim` (its mirrored const-capable
    /// `StackHead::new`; a loom-test-only shim that exists to keep a const
    /// static compiling under loom, never a production code path) — so it is
    /// not freely removable in a future 0.2 without checking that caller
    /// first.
    #[doc(hidden)]
    #[must_use]
    pub const fn empty() -> u64 {
        Self::pack_truncating(Self::INDEX_MASK_U32, 0)
    }

    /// The empty sentinel's index half: the `u32` form of `INDEX_MASK`, for
    /// packing it with a
    /// NON-zero, caller-supplied RUNNING tag (`pack(empty_index(), running_tag)`)
    /// instead of `empty()` (which always zeroes the tag).
    ///
    /// **H-2 fix:** the empty transition in [`pop_index`](StackOps::pop_index)
    /// uses this, packing the tag it just observed on the popped head, so the
    /// ABA tag keeps counting forward across the empty→non-empty churn cycle.
    /// [`is_empty`](Self::is_empty) inspects only the index half, so a non-zero
    /// tag here is still unambiguously "empty".
    #[must_use]
    pub const fn empty_index() -> u32 {
        Self::INDEX_MASK_U32
    }

    /// Whether a packed word denotes the empty stack (index half == the empty
    /// sentinel), REGARDLESS of the tag half.
    #[must_use]
    pub const fn is_empty(word: u64) -> bool {
        (word & Self::INDEX_MASK) == Self::INDEX_MASK
    }
}

/// A push was refused because the head's running tag is already
/// [`TaggedIndex::TAG_MAX`]: bumping it would wrap to 0 and re-issue a
/// `(index, tag)` head word that a popper parked since the previous cycle
/// may still hold as its CAS expectation — the exact stale-CAS double-issue
/// the tag exists to prevent (see the crate-root docs' "The tag is strictly
/// monotonic" section). The stack is now SEALED: every further
/// [`push_index`](StackOps::push_index) is refused the same way,
/// permanently; [`pop_index`](StackOps::pop_index) is unaffected and drains
/// the remaining chain normally. The refused index was never published and
/// remains owned by the caller — nothing leaked by this error alone.
///
/// No `core::error::Error` impl: this crate's declared MSRV
/// (`Cargo.toml`'s `rust-version`) is 1.79, and `core::error::Error`
/// stabilized in 1.81. Deferred, not silently skipped — add the impl in a
/// future change once the MSRV floor moves past 1.81.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TagExhausted;

impl core::fmt::Display for TagExhausted {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "tagged-index-stack: push refused, the head's tag has reached \
             TaggedIndex::TAG_MAX; the stack is sealed (pops still work, \
             pushes are refused permanently)"
        )
    }
}

/// The head word of a tagged Treiber free-list: a single `AtomicU64` packing an
/// `(index | tag)` pair (see [`TaggedIndex`]). Owned by exactly one
/// [`StackStorage`] implementor value at a time, and bound to one link
/// backing for its WHOLE life — the binding between this head and its links
/// is established by that impl, not re-asserted per call; sharing one head
/// between implementor values (clause 1) or rebinding a live head across time
/// (inventory shape 4) are hazards — see the
/// [`StackStorage`] trait doc's "The shared-storage hazard class" section
/// for the full inventory. The stack operations themselves live
/// on [`StackOps`] (blanket-implemented by the crate), not here; this type
/// is the bare atomic embedders inherit a cache line through.
///
/// # Layout note — no cache-line isolation
///
/// This type is a bare `AtomicU64` with no padding or alignment of its own —
/// `#[repr(transparent)]` makes that a compiler-enforced guarantee (layout,
/// size, and ABI identical to the single `head` field), not an incidental
/// property of the current definition — so it inherits the cache line of
/// whatever struct embeds it. If it lands adjacent
/// to another frequently-modified atomic, the two fields false-share — each
/// write invalidates the other core's copy of the line, and contending cores
/// ping-pong the line even though the atomics are logically independent. That
/// costs throughput, never correctness, and only matters when the line is
/// genuinely hot. Fix it at the embedding site when a profile shows it — wrap
/// this stack in a `#[repr(align(64))]` newtype or interpose padding — rather
/// than paying for blanket alignment inside the crate, which would waste most
/// of a cache line for every embedder that does not need the isolation.
///
/// # Sealing is permanent — no reset
///
/// Once [`pushes_remaining`](Self::pushes_remaining) reaches 0 (the tag is
/// [`TaggedIndex::TAG_MAX`]), this head is sealed and stays sealed: there is
/// no reset/rotation API, and none will be added. An in-place reset would be
/// a plain `store` on `head` — breaking the release-sequence invariant
/// documented on the private field below — AND would restore tag 0,
/// reintroducing the exact full-wrap collision this seal exists to close
/// (see the crate-root docs' H-2 note on why a naive tag reset is unsound in
/// general). A sealed head cannot be reset. A replacement must be a
/// distinct [`StackHead`] object; if it reuses the same link cells and index
/// population, the sealed head must first be fully drained
/// ([`pop_index`](StackOps::pop_index) → `None` repeatedly until empty) and
/// must outlive every popper that may still reference it — for a `'static`
/// head, forever.
#[repr(transparent)]
#[derive(Debug)]
pub struct StackHead<const INDEX_BITS: u32> {
    /// INVARIANT (release sequence): every modification of `head` MUST be a
    /// compare_exchange (an RMW). Today both writers are —
    /// [`push_index`](StackOps::push_index)'s `Release` CAS and
    /// [`pop_index`](StackOps::pop_index)'s `Acquire` CAS (plus the loom-only
    /// `cas_head_for_test`, also a CAS; constructing the atomic in `new` is
    /// initialization, not a modification, and `raw_head` only loads). Per
    /// the release-sequence rule, a release sequence continues through every
    /// subsequent RMW to the same location regardless of those RMWs' own
    /// orderings, so with every write here an RMW the release sequence headed
    /// by any push's `Release` CAS stays UNBROKEN across all later
    /// modifications. That is what lets `pop_index`'s successful CAS be plain
    /// `Acquire` instead of `AcqRel`: any later `Acquire` read of a value
    /// this pop wrote still lands inside that push's release sequence, so the
    /// happens-before edge back to the link-writing push survives
    /// transitively.
    ///
    /// Do NOT add a plain `store` to this field (e.g. a hypothetical
    /// `clear()`/`reset()`, or a `Drop` impl zeroing it). A non-RMW write
    /// severs every release sequence it follows; after that, `pop_index`'s
    /// `Acquire`-only success ordering can silently un-publish links on
    /// weakly-ordered targets — no compile error, and likely no test failure
    /// on x86. If such an API is ever genuinely needed, promote `pop_index`'s
    /// success ordering to `AcqRel` in the same change. (Plain loads are
    /// harmless: they modify nothing, so they break no sequence.)
    head: AtomicU64,
}

impl<const INDEX_BITS: u32> StackHead<INDEX_BITS> {
    /// A fresh, EMPTY stack head (the bootstrap empty sentinel, tag 0). Under
    /// `--cfg loom` this cannot be `const` (loom's atomics have no `const` ctor).
    #[cfg(not(loom))]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            head: AtomicU64::new(TaggedIndex::<INDEX_BITS>::empty()),
        }
    }

    /// A fresh, EMPTY stack head (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            head: AtomicU64::new(TaggedIndex::<INDEX_BITS>::empty()),
        }
    }

    /// Thin wrapper over the head atomic's load — for the [`StackOps`] blanket
    /// impl.
    pub(crate) fn load(&self, ordering: Ordering) -> u64 {
        self.head.load(ordering)
    }

    /// Thin wrapper over the head atomic's compare_exchange — for the
    /// [`StackOps`] blanket impl.
    pub(crate) fn compare_exchange(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.head.compare_exchange(current, new, success, failure)
    }

    /// Whether the stack is currently empty. Advisory only — a concurrent
    /// push or pop can make the answer stale the instant this returns, in
    /// either direction — so use it for diagnostics/monitoring, not for
    /// correctness decisions ([`pop_index`](StackOps::pop_index)'s `None` is
    /// the authoritative empty check).
    ///
    /// A `Relaxed` load is sufficient here because the result is explicitly
    /// racy: no ordering is being promised, and a plain load touches nothing,
    /// so the release-sequence invariant documented on the private `head`
    /// field is untouched.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        TaggedIndex::<INDEX_BITS>::is_empty(self.head.load(Ordering::Relaxed))
    }

    /// Successful pushes this head can still accept before
    /// [`push_index`](StackOps::push_index) starts refusing with
    /// `Err(`[`TagExhausted`]`)`: `TaggedIndex::TAG_MAX - tag`. `0` means
    /// sealed — every future push is refused (see "Sealing is permanent"
    /// above).
    ///
    /// Advisory `Relaxed` load, same posture as [`is_empty`](Self::is_empty):
    /// a concurrent push can make this stale the instant it returns. A
    /// plain load touches nothing, so the release-sequence invariant on the
    /// private `head` field (see its doc) is untouched.
    #[must_use]
    pub fn pushes_remaining(&self) -> u64 {
        let (_, tag) = TaggedIndex::<INDEX_BITS>::unpack(self.head.load(Ordering::Relaxed));
        TaggedIndex::<INDEX_BITS>::TAG_MAX - tag
    }

    /// **test-only** constructor seeding a specific tag, for a tiny-tag
    /// regression oracle at the REAL tag width — never via a
    /// `TAG_BITS`-reducing cfg (this crate's Option-4 tiny-tag oracle
    /// convention). Builds fresh atomic storage directly
    /// (`AtomicU64::new(..)`): this is INITIALISATION, not a plain `store`
    /// on a live head, so the release-sequence invariant documented on the
    /// private `head` field is untouched.
    ///
    /// `#[doc(hidden)]` + gated: same test-only-forwarder convention as
    /// [`raw_head`](Self::raw_head) — see its rationale.
    ///
    /// Uses the CHECKED [`TaggedIndex::pack`], not the crate-private
    /// truncating fast path: a test passing `tag > TAG_MAX` must get a loud
    /// failure here, not a silently truncated (and therefore wrong) starting
    /// tag that would make a test oracle pass or fail for the wrong reason.
    ///
    /// # Panics
    /// Panics if `tag > `[`TaggedIndex::TAG_MAX`].
    #[doc(hidden)]
    #[cfg(any(feature = "test-internals", loom))]
    #[must_use]
    pub fn with_tag_for_test(tag: u64) -> Self {
        Self {
            head: AtomicU64::new(
                TaggedIndex::<INDEX_BITS>::pack(TaggedIndex::<INDEX_BITS>::empty_index(), tag)
                    .expect("with_tag_for_test: tag out of range (tag > TaggedIndex::TAG_MAX)"),
            ),
        }
    }

    /// The raw packed head word (`Acquire`) — for this crate's own diagnostics
    /// and tests only. The index half is a live top-of-stack index or
    /// [`empty_index`](TaggedIndex::empty_index); the high bits are the running
    /// tag. `Acquire` so a loom test that splits a pop's read from its CAS (to
    /// open the ABA window) still forms the same happens-before edge the real
    /// `pop_index`'s `Acquire` head load does.
    ///
    /// `#[doc(hidden)]` + gated (this project's established test-only surface
    /// convention — every other `#[doc(hidden)]` item in this crate points
    /// here for the generic rationale): this is a `pub` item solely so
    /// `tests/` — an external crate from this crate's own perspective — can
    /// reach it. Gated: compiled ONLY under the `test-internals` feature or a
    /// loom build — a default build (a downstream consumer, the docs.rs
    /// render) does not contain this item at all, so unlike `#[doc(hidden)]`
    /// alone the gate makes it genuinely unnameable from safe downstream
    /// code, not merely hidden from rustdoc navigation. It is not exercised
    /// by any production caller.
    #[doc(hidden)]
    #[cfg(any(feature = "test-internals", loom))]
    #[must_use]
    pub fn raw_head(&self) -> u64 {
        self.head.load(Ordering::Acquire)
    }

    /// **loom-test-only** raw CAS on the head word, exposed so the shipped loom
    /// proof (`tests/loom_aba.rs`) can split a pop's head-load from its CAS —
    /// opening the ABA window the real `pop_index` closes internally — and
    /// drive the buggy-drain counterfactual, all against the REAL head atomic.
    /// NOT part of the public API: it is compiled only under `--cfg loom`.
    ///
    /// `#[doc(hidden)]`: see [`raw_head`](StackHead::raw_head)'s
    /// rationale. This item carries the strictly narrower `#[cfg(loom)]`
    /// gate (vs `raw_head`'s `test-internals`-or-loom), so it does not exist
    /// at all outside a `--cfg loom` build.
    ///
    /// # Errors
    ///
    /// Forwards `AtomicU64::compare_exchange`'s `Err(actual)` on CAS failure.
    #[cfg(loom)]
    #[doc(hidden)]
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

impl<const INDEX_BITS: u32> Default for StackHead<INDEX_BITS> {
    fn default() -> Self {
        Self::new()
    }
}

/// One implementor supplies both the stack head and the per-index link access —
/// the head↔links binding is established once per impl instead of being
/// re-asserted on every [`push_index`](StackOps::push_index)/
/// [`pop_index`](StackOps::pop_index) call. The stack stores the head word; each
/// pushed index's next pointer (another index, or [`TAIL`]) lives in the
/// implementor's storage — slot-resident in implementor-owned storage (the
/// production shape) or in an owned fused object ([`ArrayIndexStack`]).
///
/// The stack's own CAS loops never block, but end-to-end lock-freedom of
/// [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
/// additionally requires this trait's implementation to be non-blocking:
/// the shipped `AtomicU32`-cell implementations are; a hypothetical
/// mutex-backed `StackStorage` would make every stack operation blocking again.
///
/// # Safety
///
/// Implementing this trait is a SOUNDNESS commitment, not a convenience —
/// allocator consumers build memory safety on the exclusive index issuance
/// it promises (the [`core::alloc::GlobalAlloc`] / `std::alloc::Allocator`
/// category).
///
/// An implementor must uphold all of the following:
///
/// 1. **One live binding per head, for the head's whole life.** The
///    [`StackHead`] returned by [`head`](Self::head) must be bound to
///    exactly one live implementor value for as long as any index is
///    reachable through it: never shared with another live implementor
///    value, and never rebound to different link storage across time (even
///    with never more than one live value at any instant).
/// 2. **One backing, consistently.** [`load_next`](Self::load_next) and
///    [`store_next`](Self::store_next) must read and write the same link
///    storage through a stable one-to-one index↔cell mapping, and a
///    `load_next` must never answer with a write OLDER than the
///    publishing push's own [`store_next`](Self::store_next): after a
///    thread `Acquire`-observes a head published by one specific
///    `Release`-ordered push's head CAS, a subsequent `load_next` of that
///    push's link cell observes either that push's own `store_next` or
///    some LATER write in the cell's modification order — never any write
///    that precedes it there. This publication-relative lower bound is
///    deliberately weaker than a "most recent store" promise, and it is
///    the version every atomic-cell implementor can actually honour: a
///    legal intervening pop+repush of the same index may write the cell
///    between that observation and the load, and the load may observe
///    THAT write. Such a late observation is harmless: the observing
///    thread's head expectation still carries the pre-intervention tag,
///    so its head CAS is guaranteed to fail before the late link value
///    could ever be installed as head. (The `# Ordering contract` below
///    discharges the ordering half for a single, stable implementor — the
///    Release head publication paired with the popper's Acquire head
///    observation is what forbids an earlier write — and clause 7's
///    atomic cells supply the per-location modification order the bound
///    is stated over.)
/// 3. **Disjoint reachable-index populations across shared cells.** No
///    index reachable from two live head↔links bindings whose hooks touch
///    the same link cells (cell sharing per se is harmless with disjoint
///    populations; the hazard is a reachable index).
/// 4. **Valid answers, dedicated cells.** [`load_next`](Self::load_next)
///    must return only [`TAIL`] or a currently-valid index, from a link
///    cell DEDICATED to this purpose, never payload-aliased.
/// 5. **Same logical head every call** — see "Mechanical requirement on
///    `head()`" below.
/// 6. **Declared link domain.** The implementor defines and documents its
///    own domain — a subset of `0 .. INDEX_MASK`, fixed for the binding's
///    whole life — for which it owns a dedicated backing cell; a
///    lazily-materialised backing (like `sefer-alloc::Registry`'s chunked
///    slot array) still counts as in-domain once its allocation policy
///    guarantees the cell exists. [`load_next`](Self::load_next) and
///    [`store_next`](Self::store_next) must be memory-safe for every index
///    inside that domain and MAY use unchecked access outside a validity
///    check for any index OUTSIDE it — the stack's own algorithm never
///    calls them out-of-domain, by [`push_index`](StackOps::push_index)'s
///    caller-side `# Safety` contract (its clause 1). The caller-side
///    domain proof and the implementor-side unchecked-access permission
///    are two halves of one boundary.
/// 7. **Atomic cells.** Every link-cell access must be atomic: a
///    [`store_next`](Self::store_next)`(i, ..)` can race with a stale
///    popper's [`load_next`](Self::load_next)`(i)` that will go on to lose
///    its head CAS, so a non-atomic implementor is undefined behaviour even
///    with every OTHER contract clause honoured. (The `# Ordering contract`
///    section above presupposes atomic cells — its mandated
///    `Acquire`/`Release` orderings are the same requirement restated as
///    orderings; this clause states it plainly.)
///
/// The sections below — "The binding: structural vs. value-level
/// obligations" and "The shared-storage hazard class: detection boundary"
/// — remain the explanatory appendix to this contract, not a replacement
/// for it; the full design/audit detail beneath their compact summaries is
/// archived in the repository ADR
/// `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`
/// (a repository file, not part of the published package).
///
/// # Ordering contract
///
/// Implementations MUST use `Acquire` (or a stronger ordering) on
/// [`load_next`](Self::load_next) and `Release` (or a stronger ordering) on
/// [`store_next`](Self::store_next). The load-bearing
/// `Acquire` for the stack's own proof is the head observation itself — the
/// initial `Acquire` load of the head, or (on a retry) the PREVIOUS
/// iteration's `Acquire`-ordered CAS-failure read — which happens before the
/// [`load_next`](Self::load_next) call: each
/// [`store_next`](Self::store_next) is sequenced-before the pushing
/// thread's `Release` CAS on the head, and a release publishes all of its
/// thread's prior writes, whatever tags those writes carry themselves. So a
/// pop that observes a slot as the head sees the link a pusher wrote before
/// publishing that slot as head EVEN IF the link accesses themselves were
/// `Relaxed`; the CAS is attempted only AFTER
/// [`load_next`](Self::load_next) has run, so its success ordering plays no
/// part in making that link visible.
///
/// The full link-level `Acquire`/`Release` pairing is mandated as deliberate
/// change-resilience — defence-in-depth: it is kept even where the
/// head-publication proof above would permit `Relaxed`, so a
/// [`StackStorage`] implementation stays correct on its own terms rather
/// than coupled to the stack's internal head orderings (an implementation
/// detail that could change). On weakly-ordered targets, where
/// `Acquire`/`Release` cost real instructions, read this as considered
/// defence-in-depth; the measured cost status of the choice and the deferral
/// rationale are recorded in
/// `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` (a repository file, not
/// part of the published package).
///
/// This ordering contract speaks to one head-and-backing pair used
/// consistently. Together with clause 7's atomic cells it is what
/// discharges the ordering half of clause 2's coherence obligation — the
/// publication-relative lower bound stated there (a
/// [`load_next`](Self::load_next) never answers with a write preceding the
/// publishing push's own [`store_next`](Self::store_next) in the cell's
/// modification order) — *given* that every call reaches the same
/// implementor; it promises nothing stronger (in particular, no globally
/// most-recent store). Under this API that "given" is structural only at
/// the type level and a live obligation at the value level: see "The
/// binding: structural vs. value-level obligations" below.
///
/// # The three hooks are unsafe fn — a compiler-enforced unsafe boundary
///
/// [`head`](Self::head), [`load_next`](Self::load_next), and
/// [`store_next`](Self::store_next) are the STORAGE IMPLEMENTOR's hooks —
/// the three surfaces this crate's own `pub(crate)` internal bridge
/// (the [`StackOps`] blanket impl) drives — and each is an `unsafe fn`
/// carrying its own caller-side `# Safety` clause stating what the CALLER
/// must uphold to invoke it soundly (see each method's docs). A call to
/// any hook outside an `unsafe` block is a compile error (E0133, "call to
/// unsafe function is unsafe") — the compiler enforces only that an
/// `unsafe` context exists, not the clause's substance (link domain,
/// liveness, or any other semantic precondition), which the human writing
/// the call must verify by hand — and every hook invocation anywhere must
/// discharge the callee's caller-side contract inside an `unsafe {}` with a
/// `// SAFETY:` proof. The crate's own sole call site is that bridge, inside
/// the [`push_index`](StackOps::push_index)/
/// [`pop_index`](StackOps::pop_index) CAS algorithms; callers drive a
/// stack ONLY through
/// [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
/// (or [`ArrayIndexStack`]'s inherent
/// `push`/`pop` — `push` under the same `unsafe` boundary); the three
/// hooks belong inside the implementor's impl block. The caller-facing
/// [`push_index`](StackOps::push_index) joined this `unsafe` boundary in
/// the same spirit (see its `# Safety` section).
///
/// [`head`](Self::head) is callable from outside the crate only inside an
/// `unsafe` block, which puts the CALLER under [`head`](Self::head)'s own
/// `# Safety` contract — the clause forbidding a second, competing binding
/// built around the returned reference. Against the crate's shipped
/// standalone type ([`ArrayIndexStack`]) even this route is closed — the
/// type does not implement this trait and hands out no head reference (see
/// its own doc). This `unsafe fn` boundary gates the crate's OWN hooks,
/// NOT an implementor's own storage: an implementor that exposes its own
/// head through its own inherent (non-trait) API can still have a
/// competing binding rebuilt against it that way, so one-value-per-head
/// stays a convention the implementor upholds by construction — asserted
/// formally by every `unsafe impl`, not detected by it.
///
/// # The binding: structural vs. value-level obligations
///
/// The caller-side obligation the old per-call-`&L` API could only document
/// (same logical backing on every call) is now an OBLIGATION OF THE
/// IMPLEMENTOR — but it is only PARTLY discharged by the compiler. What the
/// compiler enforces is the CALLING convention: a caller cannot hand a
/// second, different storage ARGUMENT to
/// [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
/// on a later call — the two-backings-one-head call shape does not compile
/// (pinned by `tests/compile_fail.rs`). What NOTHING enforces — not the
/// type system, not the [`StackOps`] blanket impl, not clause 4's
/// release-active guard, and not even the `unsafe impl` acknowledgment
/// itself (which forces every implementor to ASSERT the contract but
/// detects no violation) — is the INSTANCE-level half of clause 1 (a given
/// [`StackHead`] value reachable through exactly one live implementor VALUE
/// at a time, for the head's whole life) and the BINDING-level half of
/// clause 3 (no index reachable from two live bindings sharing link
/// cells). Two separate, individually coherent implementor values, each
/// satisfying every clause on ITS OWN terms, can still violate the
/// contract together — no per-implementor audit sees the combination.
/// Discharge both by construction: one implementor value per head, for the
/// head's WHOLE life (one-live-value-AT-A-TIME is not enough — clause 1's
/// body already forbids rebinding a live head to different links across
/// time even with never more than one live value at any instant, see
/// hazard shape 4 below); disjoint reachable-index populations per binding
/// over any shared cell population. Clause 1's whole-life half also binds
/// the backing: the implementor and its cells must remain alive and keep
/// their identity for as long as the stack's head can reference them — in
/// practice, for the implementor's own lifetime. Clause 2's mapping half
/// (every valid index maps to the same link cell for the implementor's
/// whole lifetime) holds by the implementor's own stable structure; its
/// coherence half is the publication-relative modification-order lower
/// bound stated in clause 2 itself (a `load_next` of index `i` never
/// observes a write preceding the publishing push's own `store_next(i,
/// _)`), discharged by the Acquire/Release ordering contract above plus
/// clause 7's atomic cells for a single, stable implementor. When the
/// load DOES observe a later write — an intervening pop+repush of `i` —
/// clause 2 already covers the consequence: the observing thread's
/// outdated head tag makes its CAS fail, so the late link value is never
/// installed as head.
///
/// Clause 1's coverage of the shared-head hazard is EXHAUSTIVE: there are
/// exactly two routes to a `&StackHead<INDEX_BITS>` from safe code (own a
/// [`StackHead`] value directly, or call the `unsafe fn`
/// [`head`](Self::head) hook — itself closed to outside callers except
/// through an implementor's own storage/API, see "The three hooks are
/// unsafe fn" above), so clause 1's obligation stays over VALUES, not over
/// types. The crate's own fused type, [`ArrayIndexStack`], does not
/// implement this trait at all, so a competing binding against a
/// standalone [`ArrayIndexStack`] is UNEXPRESSIBLE (compile-fail pinned —
/// see [`ArrayIndexStack`]'s own doc). The dated census proving no third
/// route exists, its grep recipes, and its known falsification limits are
/// archived in the repository ADR
/// `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`
/// (a repository file, not part of the published package) — re-verify it
/// mechanically before relying on it.
///
/// Clause 4 (valid answers, dedicated cells) stays a live RUNTIME
/// obligation nothing here structurally prevents: an implementation
/// returning an arbitrary, stale, or foreign value corrupts the free-list
/// with no adversarial intent required. What the runtime catches of this,
/// and what it still misses, is "Detection boundary" in the hazard-class
/// section below; the release-active guard itself, the exact
/// zero-initialised-backing and out-of-range corruption mechanisms, and
/// the two-cause disjunction of what a self-loop proves, are canonically
/// in [`pop_index`](StackOps::pop_index)'s `# Panics` — not restated here.
/// See "Storage requirement" below for why payload-aliased link storage
/// always violates this clause. The full per-clause elaboration this
/// section summarizes is archived in the same repository ADR's P3-4
/// addendum, for readers who want the worked examples rather than the
/// operative conclusion.
///
/// # The shared-storage hazard class: detection boundary
///
/// The caller-facing calling convention makes the two-backings-one-head
/// swap trap — two independent calls, each supplying a different backing
/// for the same head — uncompilable (`tests/compile_fail.rs` pins exactly
/// that). What remains expressible is the REST of the shared-storage
/// hazard class: FOUR shapes, none of them the only gap the others leave,
/// each still expressible only behind an `unsafe impl StackStorage` — a
/// compiler-forced acknowledgment, at the impl site, of the very `#
/// Safety` contract the shape then violates. Shape 1 VIOLATES
/// per-implementor clauses (3 and 4): implementor-enforced, not
/// structurally impossible, and auditable inside one impl block. Shapes
/// 2-4 are BINDING-level: their subject is a head↔links BINDING (how many
/// live bindings exist over a given head or cell population, and across
/// how much time), not the state of any single implementor, so no
/// per-implementor clause can even name them — each is reachable with
/// every per-implementor clause individually satisfied. This section is
/// the source of truth for the inventory and for what the runtime
/// currently detects; the crate-root docs, README, type/method docs, and
/// pinning tests point here rather than re-deriving it (the same pattern
/// `tests/loom_aba.rs`'s module doc establishes for the loom per-model
/// breakdown).
///
/// | Shape | Hazard | Runtime detection |
/// |---|---|---|
/// | 1. Internally disagreeing storage | one implementor's [`load_next`](Self::load_next)/[`store_next`](Self::store_next) read and write different backings behind one head | zero-init sub-shape: 2nd-pop self-loop panic; otherwise silent |
/// | 2. Shared head, different links | two bindings' [`head`](Self::head) return the same [`StackHead`] value over different link cells | zero-init sub-shape: 2nd-pop self-loop panic; against the owned standalone [`ArrayIndexStack`] the shape is UNEXPRESSIBLE (compile-fail); otherwise silent |
/// | 3. Separate heads, shared link cells | one index REACHABLE from two bindings sharing link cells (disjoint reachable populations over shared cells are harmless — the hazard is reachability, not sharing) | no detector at all — the chain stays acyclic; always silent |
/// | 4. Temporal rebinding | a live head moved BY VALUE into fresh links, mid-life — never more than one live implementor value at any instant, but clause 1's body forbids rebinding across time regardless (see "The binding" above) | 1st pop: silent leak of every deeper index; 2nd pop: self-loop panic |
///
/// Detection coverage, stated once: [`pop_index`](StackOps::pop_index)'s
/// release-active clause-4 guard (see its `# Panics`, which also holds the
/// full two-cause disjunction of what a self-loop proves — a self-loop
/// ALSO fires for an unrelated caller-contract violation outside this
/// hazard class, a plain double-push of the current head) is a VALUE-shape
/// detector, not a structural fix — it panics on an out-of-range answer or
/// a self-loop (`next == index`), which catches only the zero-initialised
/// sub-shapes of shapes 1, 2, and 4, one pop too late. Shape 3 has no
/// detector at all, because every link value stays numerically valid and
/// the chain acyclic — documented, not detected. The full shape-by-shape
/// walkthroughs (worked examples, exact corruption mechanisms) and the
/// per-test status list (which test pins which shape, guard fires vs.
/// silent) are archived in the repository ADR
/// `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`'s
/// P3-4 addendum and in `tests/custom_storage_impl.rs`'s own module doc
/// respectively (both repository files/locations, per this crate's
/// existing single-sourcing convention).
///
/// (This inventory counts head↔links BINDINGS, not implementor values: a
/// shape qualifies when a head or a link-cell population reaches two live
/// bindings at once (shapes 2 and 3), or when one live binding is replaced
/// across time by another binding over the same head with different links
/// (shape 4). One expressible shape is excluded here by construction: one
/// implementor whose [`head`](Self::head) returns different heads across
/// calls is not a shared-or-rebound-binding hazard, and it is covered by
/// its own section above, "Mechanical requirement on `head()`".)
///
/// # Mechanical requirement on `head()`
///
/// Implementations must return the same logical head from [`head`](Self::head)
/// for every operation on a given implementor. The crate's [`StackOps`]
/// blanket impl reads `head()` exactly once per operation and holds the
/// resulting `&StackHead` for the entire CAS retry loop, but that one-read
/// discipline only makes sense if every read lands on the same logical head.
///
/// # Storage requirement: a DEDICATED cell, never payload-aliased
///
/// A link cell must remain dedicated storage — bytes this crate alone
/// writes — for as long as its index is out of the stack; it must NOT be
/// overlaid on the popped slot's payload (the classic "the link IS the free
/// block's first bytes" idiom this crate does not support, despite the
/// crate-root docs' "slot-resident" phrasing: slot-resident means the link
/// lives in memory the slot owns, not that it may share bytes with the
/// slot's live payload). Reason: a popper may legitimately call
/// [`load_next`](Self::load_next) on an index another thread has already
/// popped and handed to a consumer (the popper read a stale head, hasn't yet
/// CASed) — benign with dedicated storage because the stale value is still a
/// valid TAIL-or-index value and the CAS is guaranteed to fail. With
/// payload-aliased storage the same read can observe arbitrary
/// consumer-written user data — not link-shaped at all — defeating that
/// reasoning. What [`pop_index`](StackOps::pop_index)'s clause-4 guard
/// (release-active — see its `# Panics`) then does is NARROW the blast
/// radius, not close it: an out-of-range stale read panics, and so does
/// the self-loop coincidence where the stale read equals the popped
/// index — but a payload whose first four bytes decode as a small
/// IN-RANGE value other than the popped index (a length, a refcount, a
/// tag, a small enum discriminant — all common) passes the guard
/// SILENTLY and is packed as the new head: a phantom index handed to a
/// second owner, with NO panic, in every build profile. Use a DEDICATED
/// link field per slot (as
/// [`ArrayLinks`] does, and as this crate's own downstream production
/// consumers do), not payload overlay.
///
/// # Stability
///
/// This trait is intentionally OPEN to external implementation — slot-resident
/// links in implementor-owned storage (rather than an owned array like
/// [`ArrayLinks`]) is the whole design point. New methods will only ever be
/// added with default bodies (or via a major version bump); this trait is not
/// sealed.
///
/// This trait IS an `unsafe trait`: implementing it is a soundness
/// commitment (see `# Safety` above). The crate ships
/// `#![deny(unsafe_code)]` with item-scoped, audited
/// `#[allow(unsafe_code)]` regions — one on this declaration — inventoried
/// in the crate docs' "Where unsafe lives" section. The three hooks are
/// `unsafe fn` with per-method caller-side `# Safety` contracts,
/// discharged at their one call site (the crate-internal bridge);
/// [`push_index`](StackOps::push_index) — and the owned type's
/// [`push`](ArrayIndexStack::push) — carries the three-clause
/// link-domain+liveness+exclusive-ownership caller contract, while
/// [`pop_index`](StackOps::pop_index) stays safe (an unauthorized pop can
/// only leak an index, never double-issue one). The boundary's design
/// history, including the superseded designs that preceded it, is
/// recorded in the repository ADRs
/// `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md` and
/// `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`
/// (repository files, not part of the published package).
#[allow(unsafe_code)]
// Tier-2 item-scoped allow — one of the crate's audited lint-exception regions
// (see the crate docs' "Where unsafe lives" for the full inventory). This allow
// covers the trait declaration AND its three `unsafe fn` hook declarations (lint levels are
// inherited by nested items). Single documented reason to hold `unsafe`:
// the trait's implementor obligations (the `# Safety` section in the doc
// comment above) are relied on for memory safety by allocator consumers
// and cannot be expressed in the type system, so the trait is declared
// `unsafe` — the same category as `core::alloc::GlobalAlloc` — and its
// hooks expose that boundary to callers as `unsafe fn`: a compiler-enforced
// acknowledgement that a caller-side contract applies, not a compiler-checked
// contract. Crate-wide `#![deny(unsafe_code)]` keeps every OTHER `unsafe`
// token a hard error; this allow is confined to this one declaration.
pub unsafe trait StackStorage<const INDEX_BITS: u32> {
    /// The stack's head word. Must return the same logical head for every
    /// operation on this implementor — see the trait doc's "Mechanical
    /// requirement on `head()`".
    ///
    /// Implementor hook — callable only by upholding this caller-side
    /// contract inside `unsafe` (see the trait doc's "The three hooks are
    /// unsafe fn" section for the full picture).
    ///
    /// # Safety
    ///
    /// The caller must not build a second, competing head↔links binding
    /// around the returned reference: the reference may be used only as
    /// the head of the binding `self` implements, for that binding's
    /// whole life. This is the caller-side twin of trait `# Safety`
    /// clause 1 ("one live binding per head, for the head's whole
    /// life") — see that clause rather than this one for the
    /// implementor-side statement of the same obligation.
    unsafe fn head(&self) -> &StackHead<INDEX_BITS>;

    /// Load the "next" link for `index` with `Acquire` ordering.
    ///
    /// Implementor hook — callable only by upholding this caller-side
    /// contract inside `unsafe` (see the trait doc's "The three hooks are
    /// unsafe fn" section for the full picture).
    ///
    /// # Safety
    ///
    /// The caller must invoke this only for an `index` that has been
    /// PUSHED THROUGH THIS EXACT BINDING at least once — i.e. its link
    /// cell was initialised by a prior
    /// [`store_next`](Self::store_next) through this same storage
    /// binding. Note deliberately: this does NOT require `index` to be
    /// currently reachable or live. The crate's own pop algorithm calls
    /// this on an index it observed as head, but a concurrent popper may
    /// already have popped that same index before this caller's CAS
    /// lands (this caller's CAS then fails and retries) — so a
    /// "currently reachable" formulation would be a contract the crate's
    /// own algorithm violates under contention. Do not "strengthen" this
    /// clause back into that false claim.
    unsafe fn load_next(&self, index: u32) -> u32;

    /// Store the "next" link for `index` with `Release` ordering. This is the
    /// ONLY write the stack makes to link storage, and only during a push — the
    /// lazy-link (RAD-1) discipline: link storage is never eagerly initialised.
    ///
    /// Implementor hook — callable only by upholding this caller-side
    /// contract inside `unsafe` (see the trait doc's "The three hooks are
    /// unsafe fn" section for the full picture).
    ///
    /// # Safety
    ///
    /// The caller must invoke this only in the stack algorithm's
    /// CAS-valid push phase: `index` already satisfies
    /// [`push_index`](StackOps::push_index)'s caller-side `# Safety`
    /// contract (in `self`'s link domain; not currently live; held under
    /// exclusive publish/recycle authority), `next` is
    /// [`TAIL`] or the index most recently observed as THIS binding's
    /// head, and the call happens before the head CAS that publishes
    /// `index`. (The trait-level `# Safety` clauses above say nothing
    /// about CAPACITY — the domain, liveness, and exclusive-ownership legs
    /// this leans on are `push_index`'s three caller-side clauses, not those clauses.)
    unsafe fn store_next(&self, index: u32, next: u32);
}

/// The stack operations — [`push_index`](Self::push_index) /
/// [`pop_index`](Self::pop_index) — blanket-implemented by the crate for every
/// [`StackStorage`] implementor. Downstream impls are impossible (trait
/// coherence: a second impl would conflict with this blanket), so the
/// CAS-retry-loop bodies cannot be overridden or drifted from; an implementor
/// controls only `head`/`load_next`/`store_next`.
pub trait StackOps<const INDEX_BITS: u32>: StackStorage<INDEX_BITS> {
    /// Push `index` onto the stack (classic Treiber push with a tag bump).
    ///
    /// Writes `index`'s next link (the current head's index, or [`TAIL`] if the
    /// stack is empty) under `Release`, bumps the tag (the ABA defence), then
    /// CASes the head to `(index, tag + 1)`. `index` MUST be a valid index
    /// (`< TaggedIndex::INDEX_MASK`) — a violation panics (see `# Panics`)
    /// rather than being trusted, because a corrupted head word downstream lets
    /// a later `pop_index` return an index nobody actually pushed, which in the
    /// parent allocator means handing out a slot that is still live elsewhere
    /// — memory unsafety downstream of this caller-contract violation,
    /// reachable from this crate's public API. Since `INDEX_BITS` is compile-time capped at 16
    /// (see [`TaggedIndex`]'s `_CHECK_BITS`), `index < INDEX_MASK` already
    /// implies `index != TAIL` at every legal width — one guard covers both.
    ///
    /// # Safety
    ///
    /// This is the caller-side unsafe contract, in three clauses; violating
    /// any one of them is a soundness violation attributable to the caller —
    /// the
    /// same posture as [`core::alloc::GlobalAlloc::dealloc`], whose
    /// exclusive-issuance contract unsafe allocator code relies on.
    ///
    /// 1. **Link domain.** `index` must be in `self`'s LINK DOMAIN — the
    ///    set of indices for which this implementor owns a dedicated
    ///    backing cell, as the implementor documents it
    ///    ([`ArrayIndexStack<B, N>`](ArrayIndexStack)'s/[`ArrayLinks`]'s
    ///    domain is `0..N`; `sefer-alloc::Registry`'s is `0..MAX_HEAPS`).
    ///    The method's own `index < INDEX_MASK` guard (see `# Panics`) is
    ///    necessary for the head-word ENCODING and stays release-active
    ///    (same rationale as [`pop_index`](Self::pop_index)'s existing
    ///    clause-4 guard), but it is NEVER sufficient proof of domain
    ///    membership — a storage's domain may be (and routinely is)
    ///    narrower than the numeric range `INDEX_MASK` admits; the guard
    ///    observes only the numeric width. Do not conflate the numeric
    ///    guard with the domain obligation.
    /// 2. **Liveness (no double push).** `index` must NOT currently be
    ///    reachable through the head of any binding whose hooks touch the
    ///    same link cells as `self`'s: either `index` was never pushed
    ///    through such a binding, or its most recent push was followed by
    ///    a [`pop_index`](Self::pop_index) that actually RETURNED it, and
    ///    it has not been pushed again since. Precision sub-clause: a
    ///    concurrent popper that OBSERVED `index` as head but LOST its
    ///    CAS did NOT pop it and did not take ownership of it — such a
    ///    stale observer imposes no obligation on this push, and stale
    ///    content sitting in `index`'s link cell from an earlier push
    ///    cycle is irrelevant (the lazy-link/RAD-1 discipline: this
    ///    push's own [`store_next`](StackStorage::store_next) overwrites
    ///    it before the head CAS publishes `index`). Do not read this
    ///    clause as forbidding a lost-CAS observer.
    ///
    ///    This sub-clause is sound precisely because the tag never wraps
    ///    (see the crate-root docs' "The tag is strictly monotonic"
    ///    section): the lost-CAS observer's `(index, tag)` expectation can
    ///    never be reinstalled, so its CAS is guaranteed to fail regardless
    ///    of what this push does.
    /// 3. **Exclusive ownership epoch (no duplicate authority over the same
    ///    index).** The caller's call to this method must be backed by a
    ///    unique, not-yet-consumed PUBLISH/RECYCLE AUTHORITY over `index`:
    ///    either freshly minted (`index` has never been pushed through any
    ///    binding whose hooks touch the same link cells), or obtained from
    ///    one specific successful [`pop_index`](Self::pop_index) call that
    ///    returned `index` to this caller. This call CONSUMES that
    ///    authority, and its linearization point is THIS call's own
    ///    successful head CAS — not physical return: at the CAS's instant,
    ///    authority over `index` transfers from the caller to the stack.
    ///    Consequently, another thread MAY legitimately
    ///    [`pop_index`](Self::pop_index) the just-published `index` and
    ///    legitimately push it again — backed by ITS OWN freshly obtained
    ///    authority from THAT pop, a distinct later epoch — even before
    ///    this original call has physically returned `Ok(())`. This is NOT
    ///    a clause-3 violation: this call's own authority over `index`
    ///    already ended at its own CAS, and nothing this call does
    ///    afterward (returning) touches shared memory. What clause 3
    ///    forbids is two push calls ([`push_index`](Self::push_index) or
    ///    [`push`](ArrayIndexStack::push), through this binding or any
    ///    binding whose hooks touch the same link cells) consuming the
    ///    SAME unconsumed authority epoch — i.e. two pushes of one `index`
    ///    with no intervening successful [`pop_index`](Self::pop_index)
    ///    between them. Counterexample (fresh empty stack, `index` in
    ///    domain): threads A and B each independently believe they hold
    ///    exclusive authority over the SAME freshly-minted `index` (a
    ///    caller bug: the authority was duplicated instead of obtained
    ///    singly) and concurrently call this; at each call's entry `index`
    ///    is unreachable, so both calls satisfy clauses 1 and 2. A wins
    ///    its CAS and publishes `index`. B's CAS then loses; B's retry
    ///    loop observes the NEW head — `index` itself, just published by
    ///    A — and stores `next[index] = index` (this method always chains
    ///    the observed head's index into `index`'s link cell), and B's own
    ///    CAS can succeed too: the stack now holds
    ///    `next[index] == index`, a self-loop that
    ///    [`pop_index`](Self::pop_index)'s self-loop detector PANICS on at
    ///    the first pop through it — the same corruption shape the
    ///    sequential double-push clause 2 forbids. Pinned from both sides
    ///    in `tests/loom_aba.rs`:
    ///    `counterfactual_same_index_concurrent_push_self_loops` is the
    ///    loom counterfactual deliberately violating THIS clause (two
    ///    pushes on one duplicated freshly-minted epoch; both calls
    ///    satisfy clauses 1 and 2 at entry) that panics inside the
    ///    shipped [`pop`](ArrayIndexStack::pop), and
    ///    `pop_repush_after_publish_conserves` is the positive regression
    ///    proving the PERMITTED republish — pop-then-repush of a
    ///    just-published index, backed by the popper's own distinct later
    ///    epoch — conserves the free-list on every schedule loom explores;
    ///    the original push's physical-return timing is not distinguished
    ///    by that test and is irrelevant to this clause (this call's
    ///    authority already ended at its own CAS). Like clause 2, this
    ///    clause is not runtime-CHECKED: detecting a duplicated authority
    ///    epoch
    ///    would require ownership tracking the stack does not keep.
    ///    Authority transfer: on `Ok(())` it already happened at the head
    ///    CAS (the return does not cause it) — the caller must not push
    ///    `index` again until a future [`pop_index`](Self::pop_index)
    ///    RETURNS it to the caller (restoring exactly the authority
    ///    clause 2 already requires before the next push; this clause
    ///    composes with clause 2, it does not replace it). On
    ///    `Err(`[`TagExhausted`]`)` no publishing CAS occurred, so
    ///    authority never left the caller and the refused `index` remains
    ///    the caller's — see `# Errors` below.
    ///
    /// The obligation is stated over LINK CELLS, not over "the stack":
    /// link cells shared between two
    /// stacks with completely separate heads are the [`StackStorage`] trait
    /// contract's own clause-3 binding-level
    /// hazard — see the [`StackStorage`] trait doc's "The shared-storage
    /// hazard class" section for the full inventory (this shape has no
    /// runtime detector); pinned by
    /// `two_stacks_sharing_link_storage_still_double_issue` in
    /// `tests/custom_storage_impl.rs`. Re-pushing a live index is a
    /// caller-contract violation
    /// this method cannot catch — and cannot even check cheaply, because
    /// liveness is a property of the whole link chain and verifying it would
    /// cost an O(n) walk on every push. (Unlike the crate-root docs' H-2 and
    /// RAD-1 subtleties, this one is part of this method's caller-side
    /// `unsafe fn` contract, behind a compiler-enforced unsafe boundary — a
    /// bare call from safe code is E0133 — though the clause's substance is
    /// still not runtime-CHECKED: the method cannot detect a
    /// violation.) What `push_index` DOES check unconditionally is the
    /// `index < INDEX_MASK` range bound (see `# Panics`) — which observes only
    /// the index's numeric width, never whether it is already live.
    ///
    /// Violating the liveness rule corrupts the free-list: the push
    /// overwrites `index`'s link with the current head, so if `index` was
    /// still chained in, the chain closes a cycle. If the re-pushed index
    /// was DEEPER in the chain than the head, that cycle loops silently —
    /// `pop_index` never returns `None` again and the same index is handed
    /// to two different callers, two owners of one slot in the parent
    /// allocator. If the re-pushed index IS the current head, the cycle is
    /// a self-referential link, and [`pop_index`](StackOps::pop_index)'s
    /// self-loop detector PANICS on the first pop through it instead of
    /// looping.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`TagExhausted`]`)` — publishing nothing — when the
    /// head's running tag is already [`TaggedIndex::TAG_MAX`]: bumping it
    /// would wrap to 0, and a wrapped tag re-issues a `(index, tag)` head
    /// word that a popper parked since the previous cycle may still hold as
    /// its CAS expectation — the exact stale-CAS double-issue the tag
    /// exists to prevent. The stack is then sealed: every subsequent
    /// [`push_index`](Self::push_index) returns the same error,
    /// permanently; [`pop_index`](Self::pop_index) is unaffected and drains
    /// the remaining chain. The refused `index` remains the caller's. A
    /// refusal on a CAS retry may have left stale content in `index`'s link
    /// cell from an earlier iteration of the retry loop — the RAD-1
    /// discipline already makes that irrelevant (the next successful push
    /// of `index` overwrites it before publishing); a refusal on the FIRST
    /// attempt has no side effect at all, since the check runs before the
    /// link write, but this distinction does not matter to a caller — the
    /// refused index is unaffected either way. This is legitimate resource
    /// exhaustion, not a caller-contract violation — unlike the panics
    /// below, hence `Err`, not a panic.
    ///
    /// # Panics
    ///
    /// Panics if `index >= INDEX_MASK` (the empty sentinel is reserved), in
    /// both debug and release builds — this IS a caller-contract violation
    /// (unlike tag exhaustion above), checked unconditionally, not a
    /// `debug_assert!`, because the failure mode is silent free-list
    /// corruption rather than a merely-suboptimal fallback. The formatted
    /// panic payload is allocated through the global allocator, so a
    /// consumer running this stack inside its own `#[global_allocator]`
    /// allocation path should treat this guard firing as abort-equivalent,
    /// not catchable-and-recoverable.
    ///
    /// That guard is the only bound this method itself checks, and it
    /// depends on `INDEX_BITS` alone. The [`StackStorage`] implementation's
    /// [`load_next`](StackStorage::load_next)/[`store_next`](StackStorage::store_next)
    /// (e.g. [`ArrayLinks`]'s `index >= N`) may impose their own, separate
    /// bound — see [`ArrayLinks::load_next`]/[`ArrayLinks::store_next`]'s own
    /// `# Panics` docs for the `N`-vs-`INDEX_BITS` independence — so
    /// out-of-range access in the links layer is a second panic source this
    /// guard does not cover.
    #[track_caller]
    #[allow(unsafe_code)]
    // Single documented reason to hold `unsafe`: this method carries the
    // caller-side three-clause unsafe contract (link domain + liveness +
    // exclusive ownership) —
    // the `core::alloc::GlobalAlloc::dealloc` analogue — relied on for
    // memory safety by allocator consumers; see the `# Safety` section
    // above.
    unsafe fn push_index(&self, index: u32) -> Result<(), TagExhausted>;

    /// Pop the top index off the stack (classic Treiber pop), or `None` if
    /// empty.
    ///
    /// Loads the tagged head, reads its next link, then CASes the head to that
    /// link with the same tag (a pop never bumps the tag). The tag in the high
    /// bits is the ABA defence: if a concurrent thread pops-then-repushes the
    /// same index between our load and our CAS, the tag advances and our CAS
    /// fails. The tag is strictly monotonic (see the crate-root docs' "The
    /// tag is strictly monotonic" section) —
    /// [`push_index`](Self::push_index) refuses instead of wrapping once the
    /// tag reaches [`TaggedIndex::TAG_MAX`] (`Err(`[`TagExhausted`]`)`), so
    /// this defence cannot be defeated by a full tag cycle no matter how
    /// long a thread stays parked: ABA is eliminated, not merely mitigated.
    ///
    /// **H-2 empty transition:** when the popped element is the last one
    /// (`next == TAIL`), the new head packs the empty sentinel's index with the
    /// RUNNING tag we just observed — NOT tag 0 — so the ABA tag keeps counting
    /// across the empty→non-empty churn (see the crate docs' H-2 section).
    ///
    /// On a lost CAS, the retry backoff (see
    /// [`push_index`](StackOps::push_index)'s identical backoff comment and
    /// `BACKOFF_SPIN_CAP`) is skipped when the CAS's `actual` value shows the
    /// stack just went empty — the loop's next iteration returns `None`
    /// immediately regardless, so backing off first would only add latency to a
    /// call about to do zero further work.
    ///
    /// `pop_index` reads links through the implementor's own
    /// [`load_next`](StackStorage::load_next), so the head↔links binding cannot
    /// be swapped between calls — see [`StackStorage`].
    ///
    /// # Panics
    ///
    /// Panics if the [`load_next`](StackStorage::load_next) result for the
    /// popped index is neither [`TAIL`] nor `< INDEX_MASK`, or is exactly
    /// the popped index itself (a self-loop — see below) — a value that
    /// `pop_index`'s crate-private truncating fast path (`pack_truncating`)
    /// would otherwise silently truncate into a wrong (possibly still-live)
    /// index or into the empty sentinel (clause 4 of the [`StackStorage`]
    /// implementor contract). Two corruption modes this guard prevents: an
    /// out-of-range answer packs — via `pack_truncating`, not the public
    /// checked [`pack`](TaggedIndex::pack) — to its low `INDEX_BITS` bits,
    /// landing on either a LIVE index elsewhere in the free-list (e.g.
    /// `0x1_0000` at `INDEX_BITS = 16` packs as index `0`: a double-issue)
    /// or the EMPTY sentinel (low bits all ones), silently reporting the
    /// stack drained and leaking every remaining chained index.
    ///
    /// The self-loop arm (`next == index`) is a DETECTOR for one shape, not
    /// a structural fix for the shared-storage hazard class: a
    /// contract-abiding chain can never link an index to itself —
    /// [`push_index`](StackOps::push_index) stores the
    /// previous head into `next[index]`, and that head is trivially already
    /// reachable — so a self-loop proves a caller-contract violation, of which
    /// there are two causes. By far the simpler and more likely: a double-push
    /// of the index that is ALREADY the current head — the pushed index IS the
    /// head, so [`push_index`](StackOps::push_index) itself writes
    /// `next[index] = index` directly, no foreign writer and no shared storage
    /// involved (this crate's separate, older no-double-push rule — see
    /// [`push_index`](StackOps::push_index)'s `# Safety` section; the guard
    /// fires on the FIRST pop through it). The other: a writer other than a
    /// contract-abiding push answering for this index — in practice a
    /// zero-initialised foreign backing (one shared with another implementor
    /// value, or a live head moved into fresh links — shape 4 of the
    /// [`StackStorage`] trait doc's hazard inventory; both fire on the
    /// second pop through them), or a direct, out-of-contract
    /// [`store_next`](StackStorage::store_next) hook call writing the
    /// index's own link. What this guard
    /// still does NOT catch — hand-crafted acyclic link tables, link cells
    /// shared between two independent stacks — is inventoried, with the
    /// exact catch/miss boundary, in the [`StackStorage`] trait doc's "The
    /// shared-storage hazard class" section (this guard is clause 4's runtime
    /// enforcement; the pinning tests live in `tests/custom_storage_impl.rs`).
    ///
    /// Unconditional (release-active), in both debug and release builds,
    /// mirroring [`push_index`](StackOps::push_index)'s `index < INDEX_MASK`
    /// guard: the release-active check measures ≈ free next to the head CAS
    /// (see CHANGELOG.md), so there is no throughput reason to leave a
    /// caller-contract violation whose failure mode is silent free-list
    /// corruption checked only in debug builds. The formatted panic payload is
    /// allocated through the global allocator, so a consumer running this stack
    /// inside its own `#[global_allocator]` allocation path should treat this
    /// guard firing as abort-equivalent, not catchable-and-recoverable.
    ///
    /// `pop_index` also reaches link storage through the implementor's
    /// [`load_next`](StackStorage::load_next), which may panic on an
    /// out-of-range index under its OWN, narrower bound — see
    /// [`push_index`](StackOps::push_index)'s `# Panics`.
    ///
    /// # Lock-freedom and starvation
    ///
    /// Lock-free is not starvation-free: `pop_index` never blocks on a lock,
    /// but a call can lose arbitrarily many CASes in a row, and the retry
    /// backoff deliberately makes an unlucky call wait longer between retries.
    /// The measured trade — a small number of very large outlier calls for
    /// better latency through p99.9 and better aggregate throughput — is in
    /// the crate-root doc's "Lock-freedom and starvation" section.
    #[must_use = "a popped index is removed from the free-list; discarding it leaks the slot"]
    #[track_caller]
    fn pop_index(&self) -> Option<u32>;
}

/// The crate-INTERNAL accessor shape — the head plus the link hooks — that
/// the shared CAS-retry algorithm (`push_index_impl`/`pop_index_impl` below)
/// is written against: `head()` + `load_next`/`store_next`, the same three
/// signatures as [`StackStorage`]'s.
///
/// Sealed BY CONSTRUCTION: `pub(crate)` means this trait can be neither named
/// nor implemented outside this crate, so no downstream impl can ever exist.
/// This is NOT an extension point — the public extension point remains
/// [`StackStorage`]. Its in-crate implementors are [`ArrayIndexStack`]
/// (directly, below) and every [`StackStorage`] implementor (via the blanket
/// bridge impl). The whole point: [`ArrayIndexStack`] stops implementing the
/// PUBLIC trait — so its head becomes unreachable from outside — while the
/// algorithm body stays written exactly once.
///
/// [`store_next`](Self::store_next) is the one member that is an `unsafe
/// fn`: the crate-private bridge forwards it verbatim, so the actual
/// safety proof lives at the algorithm's call site in [`push_index_impl`],
/// not at the bridge — a "my only caller is `push_index_impl`" privacy
/// argument is not a proof and would silently break the next time an
/// in-crate caller appears.
// Tier-2 item-scoped allow — one of the crate's audited lint-exception regions
// (see the crate docs' "Where unsafe lives"). Single documented reason to
// hold `unsafe`: this trait's `store_next` member is an `unsafe fn`
// declaration, so the crate-private bridge is a verbatim forwarder and the
// actual safety proof lives at the algorithm's call site in
// `push_index_impl`.
#[allow(unsafe_code)]
pub(crate) trait SealedStorage<const B: u32> {
    fn head(&self) -> &StackHead<B>;
    fn load_next(&self, index: u32) -> u32;
    /// # Safety
    ///
    /// Same contract as [`StackStorage::store_next`]'s `# Safety` — see
    /// there (one normative location; this crate cross-references it).
    unsafe fn store_next(&self, index: u32, next: u32);
}

/// Bridge: every public [`StackStorage`] implementor is also a
/// [`SealedStorage`], so the crate-internal algorithm serves the public
/// [`StackOps`] blanket impl. The calls below are fully qualified to name
/// the trait each body delegates to — and the qualifier is now
/// SEMANTICALLY LOAD-BEARING, not a style choice: [`StackStorage::head`]/`load_next`/`store_next`
/// and this impl's own [`SealedStorage`] methods have IDENTICAL `(&self)`
/// arity and parameter shapes, so a bare `self.head()` inside this impl
/// is genuinely ambiguous between two applicable trait methods (E0034,
/// "multiple applicable items in scope"). The `StackStorage::` qualifier
/// resolves that ambiguity and pins the callee.
// Tier-2 item-scoped allow — one of the crate's audited lint-exception regions
// (see the crate docs' "Where unsafe lives" for the full inventory). Single
// documented reason to hold `unsafe`: this bridge is the SOLE call site of
// [`StackStorage`]'s three `unsafe fn` hooks — the one place the
// implementor-side `unsafe impl` contract and the hooks' caller-side
// `# Safety` contracts meet — and each call below carries its own
// `// SAFETY:` proof.
#[allow(unsafe_code)]
impl<const B: u32, S: StackStorage<B> + ?Sized> SealedStorage<B> for S {
    fn head(&self) -> &StackHead<B> {
        // SAFETY: `S: StackStorage<B>` means an `unsafe impl` asserted the
        // implementor contract for this binding. The stack algorithm calls
        // `head()` exactly once per operation and uses the returned
        // reference only as THIS binding's head — never building a second,
        // competing binding around it — discharging
        // [`StackStorage::head`]'s caller-side contract.
        unsafe { StackStorage::head(self) }
    }
    fn load_next(&self, index: u32) -> u32 {
        // SAFETY: the pop algorithm calls this only on an index unpacked
        // from a head word observed through THIS binding's `head()`; such
        // an index was pushed through this binding at least once (the push
        // that published it initialised its link cell via `store_next`),
        // which is exactly [`StackStorage::load_next`]'s caller-side
        // contract — it does NOT require the index to still be reachable,
        // and it may not be (a concurrent popper can win the CAS first;
        // this caller's CAS then fails and retries).
        unsafe { StackStorage::load_next(self, index) }
    }
    /// # Safety
    ///
    /// Verbatim forwarder to [`StackStorage::store_next`] — same contract
    /// (see there). The proof lives at the sole caller, [`push_index_impl`].
    unsafe fn store_next(&self, index: u32, next: u32) {
        // SAFETY: the proof lives at the sole caller, `push_index_impl` —
        // this bridge cannot locally verify the push phase/liveness
        // obligations.
        unsafe { StackStorage::store_next(self, index, next) }
    }
}

/// The push CAS-retry algorithm, written once against [`SealedStorage`] —
/// the body of [`StackOps::push_index`], which remains the documented public
/// surface (see its doc for the algorithm, its `# Safety` section (the caller-side
/// contract) and `# Panics`). [`ArrayIndexStack`]'s inherent `push` calls this directly,
/// off the public trait plumbing.
///
/// # Safety
///
/// Same caller-side contract as [`StackOps::push_index`]'s `# Safety` —
/// the normative location, which this crate cross-references here. This
/// function is the shared body behind both [`StackOps::push_index`] and
/// [`ArrayIndexStack::push`]; its caller must discharge the link-domain,
/// liveness, and exclusive-ownership clauses.
#[track_caller]
#[allow(unsafe_code)]
// Single documented reason to hold `unsafe`: this is the shared body of
// `StackOps::push_index`/`ArrayIndexStack::push`, forwarding their
// caller-side unsafe contract (link domain + liveness + exclusive
// ownership) to the algorithm's internal `store_next` call.
pub(crate) unsafe fn push_index_impl<const B: u32, S: SealedStorage<B> + ?Sized>(
    s: &S,
    index: u32,
) -> Result<(), TagExhausted> {
    let mask = TaggedIndex::<B>::INDEX_MASK;
    if u64::from(index) >= mask {
        push_index_out_of_range(index, mask);
    }
    // `head()` is read exactly once per operation — see StackStorage's
    // "Mechanical requirement on `head()`".
    let head_ref: &StackHead<B> = s.head();
    // `Relaxed`, not `Acquire`: push uses the observed word ONLY as
    // `(index, tag)` values — it never follows a link through it. Whatever
    // a concurrent popper must observe of this push is published by the
    // Release SUCCESS CAS and recovered by the popper's OWN Acquire head
    // observation, never by anything this load orders. `pop_index`'s
    // initial load below MUST stay `Acquire` — pop DOES follow a link from
    // the observed word. The loom suite passes with exactly this ordering.
    let mut head = head_ref.load(Ordering::Relaxed);
    let mut backoff = Backoff::new();
    loop {
        // Unpack the current head once: the index half chains this push to
        // the top of the stack (below), the tag half feeds the ABA bump.
        let (cur_idx, tag) = TaggedIndex::<B>::unpack(head);
        // Seal check (P1-1 fix): bumping this tag would wrap it to 0,
        // re-issuing a (index, tag) head word a parked popper may still
        // hold as its stale CAS expectation. Refuse instead of wrapping —
        // BEFORE any side effect (store_next below): a first-attempt
        // refusal touches nothing; a refusal on a CAS retry may leave
        // stale content in `index`'s link cell from an earlier iteration
        // of this same retry loop, which the RAD-1 discipline already
        // makes irrelevant (the next successful push overwrites it before
        // publishing) — see `StackOps::push_index`'s `# Errors`.
        if tag == TaggedIndex::<B>::TAG_MAX {
            return Err(TagExhausted);
        }
        // The link this index chains to: the current head's index, or TAIL
        // if the stack is empty. The empty sentinel packs INDEX_MASK, which
        // is <= 0xFFFF at every legal width per `_CHECK_BITS`, so it can no
        // longer equal TAIL; and `TAIL & INDEX_MASK == INDEX_MASK` is a
        // mathematical identity (all-ones AND all-ones-low-bits), not a
        // coincidence. The branch is kept explicit purely for readability.
        let next_link = if TaggedIndex::<B>::is_empty(head) {
            TAIL
        } else {
            cur_idx
        };
        // Write the link under Release so a concurrent pop's Acquire read of
        // this slot's link (after observing it as head) sees it. This is the
        // ONLY link write — never an eager init (RAD-1) — and it may run
        // more than once: on a CAS failure the NEXT iteration recomputes
        // `next_link` from the fresh head and OVERWRITES this same link
        // cell before its own CAS. The stale write from the failed
        // iteration is never observable in the stack's read-set, for two
        // disjoint reasons — the normative retry-overwrite proof (the
        // SAFETY comment below cross-references it instead of repeating):
        //
        // (a) An ordinary pop cannot select `index`'s link cell at all
        //     while this push is mid-retry. Under the liveness and
        //     exclusive-ownership clauses of the caller-side push
        //     contract, the `index` this push is publishing is
        //     UNREACHABLE — part of no live chain — until this push's CAS
        //     successfully publishes it, so no pop's traversal reaches
        //     `index` before publication, stale-read or not. On the
        //     losing-CAS path described here, this push's CAS has
        //     displaced nothing.
        //
        // (b) A pop that DOES read `index`'s link cell mid-retry must be a
        //     stale popper from a PRIOR push/pop lifecycle of this same
        //     `index` (not this push). That stale popper's own CAS
        //     expectation is the old `(index, tag)` head word from that
        //     prior cycle — already displaced by whichever pop won the
        //     head CAS and transferred ownership of `index` to ITS
        //     caller. The tag is strictly monotonic (it never wraps — see
        //     the crate-root docs' "The tag is strictly monotonic"
        //     section), so that stale expected value can never be
        //     reinstalled: the stale popper's own CAS is guaranteed to
        //     fail regardless of what THIS push stores to the link cell
        //     or does with the head word.
        //
        // SAFETY: (a) [`SealedStorage::store_next`] forwards to
        // [`StackStorage::store_next`], whose caller-side contract this
        // discharges: we are in the CAS-valid push phase — `next_link` is
        // [`TAIL`] or the just-unpacked head index `cur_idx`, and the head
        // CAS that publishes `index` happens after, at the
        // `compare_exchange` below — per iteration, and on retry: a failed
        // CAS sends the loop back here, the next iteration recomputes
        // `next_link` from the fresh head and overwrites `index`'s link
        // cell before its own CAS; the failed iteration's stale write is
        // never observable in the stack's read-set (normative two-case
        // proof — unreachable-before-publication, and a prior-cycle stale
        // popper whose CAS expectation is already permanently displaced —
        // on the retry-store comment directly above, not repeated here);
        // (b) the link-domain, liveness, and exclusive-ownership legs come
        // from [`StackOps::push_index`]'s caller-side `# Safety`
        // contract, which this function's own `# Safety` forwards — the
        // caller of `push_index_impl` proved them.
        //
        // `#![deny(unsafe_op_in_unsafe_fn)]` requires this local block even
        // though `push_index_impl` is itself an `unsafe fn` — edition 2021's
        // ambient unsafe permission inside an `unsafe fn` body is exactly
        // the implicit-unsafe-operation hazard that lint closes.
        unsafe {
            s.store_next(index, next_link);
        }
        // Plain `+`: the seal check above guarantees tag < TAG_MAX, so the
        // bump cannot overflow (range proof in `pack_truncating`'s doc —
        // that check, not the operator, is the real guard).
        let new_tag = tag + 1;
        let new_head = TaggedIndex::<B>::pack_truncating(index, new_tag);
        // Release on success so a pop's Acquire sees the link we wrote.
        // Relaxed on failure is sound HERE, and the asymmetry with pop is
        // deliberate: a failed CAS sends push around the loop with the
        // value it read used ONLY as a value — push never follows a link
        // through that read, so the read carries no ordering burden. pop
        // is NOT symmetric: its retry's re-read names the index whose link
        // load_next will consult next, so pop's failure ordering MUST
        // stay Acquire (the loom counterfactual
        // `counterfactual_relaxed_cas_failure_corrupts_free_list` proves
        // Relaxed corrupts; the end-to-end guard is
        // `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`).
        // The happens-before edge a popper needs from this push is carried
        // by the Release success CAS's own release sequence — extended by
        // every later head RMW (see the `head` field's INVARIANT) — never
        // by anything push's failed-CAS reads observe.
        // Strong `compare_exchange`, deliberately NOT
        // `compare_exchange_weak`: measured equivalent on x86-64, and
        // `weak` codegen-IDENTICAL to strong on aarch64 under both the
        // outlined-atomics default and the `+lse` lowerings (multi-target
        // A/B harness, `scripts/tis_p3_ab_runner.mjs`) — the hypothesized
        // inline-LL/SC spurious-failure win does not exist on this
        // toolchain. See `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md`
        // §0 — measured NULL; the driver asserts the identity, so a toolchain
        // change fails loudly and reopens the question). This concerns the
        // CAS KIND only; the separate LINK-ordering relaxation's native
        // AArch64 wall-clock cost remains unmeasured — its static
        // multi-target A/B codegen comparison IS done (a real `ldar`/`stlr`
        // delta exists) — see `StackStorage`'s "Ordering contract".
        match head_ref.compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return Ok(()),
            Err(actual) => {
                // Retry-counter oracle (see `PUSH_RETRY_COUNT` below): a
                // REAL core atomic, so counts survive loom re-runs;
                // `Relaxed` counts only. Gated so a default build compiles
                // neither the counters nor this increment.
                #[cfg(any(feature = "test-internals", loom))]
                PUSH_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                head = actual;
                #[cfg(any(feature = "test-internals", loom))]
                {
                    backoff.spin();
                    if backoff.spun_at_cap() {
                        PUSH_BACKOFF_CAP_REACH_COUNT
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                }
                #[cfg(not(any(feature = "test-internals", loom)))]
                backoff.spin();
            }
        }
    }
}

/// The pop CAS-retry algorithm, written once against [`SealedStorage`] —
/// the body of [`StackOps::pop_index`], which remains the documented public
/// surface (see its doc for the algorithm and `# Panics`).
/// [`ArrayIndexStack`]'s inherent `pop` calls this directly, off the public
/// trait plumbing.
#[track_caller]
pub(crate) fn pop_index_impl<const B: u32, S: SealedStorage<B> + ?Sized>(s: &S) -> Option<u32> {
    // `head()` is read exactly once per operation — see StackStorage's
    // "Mechanical requirement on `head()`".
    let head_ref: &StackHead<B> = s.head();
    let mut head = head_ref.load(Ordering::Acquire);
    let mut backoff = Backoff::new();
    loop {
        if TaggedIndex::<B>::is_empty(head) {
            return None;
        }
        let (index, tag) = TaggedIndex::<B>::unpack(head);
        // Read the next link Before the CAS (the push stored it under
        // Release; our Acquire observation of head — whether from the
        // initial load OR from a retry CAS failure — synchronizes with it).
        let next = s.load_next(index);
        // Unconditional guard (release-active, mirroring push's
        // `index < INDEX_MASK` check) for clause 4 of the StackStorage
        // implementor contract: pack_truncating() below would silently
        // truncate a bad value to a wrong (possibly still-live) index or
        // to the empty sentinel. Measured ≈ free next to the head CAS —
        // see `# Panics` above and CHANGELOG.md.
        // The `next == index` arm is a self-loop DETECTOR (a
        // contract-abiding chain can never link an index to itself,
        // so the loop proves a caller-contract violation): see
        // `# Panics` above for the full two-cause disjunction it
        // proves, and StackStorage's "The shared-storage hazard
        // class" section for the exact catch/miss boundary — not
        // restated here.
        let mask = TaggedIndex::<B>::INDEX_MASK;
        if next != TAIL && (u64::from(next) >= mask || next == index) {
            pop_link_out_of_range(index, next, mask);
        }
        let new_head = if next == TAIL {
            // H-2: preserve the RUNNING tag across the empty transition.
            TaggedIndex::<B>::pack_truncating(TaggedIndex::<B>::empty_index(), tag)
        } else {
            TaggedIndex::<B>::pack_truncating(next, tag)
        };
        // Acquire on success with NO Release half is sound ONLY because
        // every write to `head` is an RMW: this CAS stays inside the
        // release sequence headed by the push that `Release`d the link
        // being handed out, so our own write need not head one. See the
        // INVARIANT on the `head` field — a plain `store` there would
        // sever that sequence and make this ordering unsound.
        // The success/failure asymmetry with push's `Release`/`Relaxed`
        // CAS is deliberate and explained from push's side in
        // `push_index_impl`'s CAS comment (why push's failure ordering is
        // `Relaxed` while pop's must stay `Acquire` — pop follows a link
        // on retry, push does not).
        // Strong CAS over `compare_exchange_weak` — measured
        // codegen-identical on aarch64 (see push's CAS note:
        // `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` §0).
        match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => return Some(index),
            Err(actual) => {
                // Retry-counter oracle (`POP_RETRY_COUNT`; same mechanism
                // as push's arm).
                #[cfg(any(feature = "test-internals", loom))]
                POP_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                head = actual;
                // Skipped when the lost CAS reveals the stack just went
                // empty: the top-of-loop `is_empty` check returns `None`
                // next iteration regardless, so spinning here is pure
                // wasted latency; which outcome a call eventually returns
                // is unchanged, only how fast it gets there.
                if !TaggedIndex::<B>::is_empty(actual) {
                    #[cfg(any(feature = "test-internals", loom))]
                    {
                        backoff.spin();
                        if backoff.spun_at_cap() {
                            POP_BACKOFF_CAP_REACH_COUNT
                                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    #[cfg(not(any(feature = "test-internals", loom)))]
                    backoff.spin();
                }
            }
        }
    }
}

// Tier-2 item-scoped allow — one of the crate's audited lint-exception regions
// (see the crate docs' "Where unsafe lives"). Single documented reason to
// hold `unsafe`: `push_index` carries the caller-side unsafe contract
// (link domain + liveness + exclusive ownership), forwarded verbatim to
// `push_index_impl`.
#[allow(unsafe_code)]
impl<const B: u32, S: StackStorage<B> + ?Sized> StackOps<B> for S {
    #[track_caller]
    unsafe fn push_index(&self, index: u32) -> Result<(), TagExhausted> {
        // SAFETY: this fn's own caller-side contract (link domain +
        // liveness + exclusive ownership, `push_index`'s `# Safety` above)
        // is forwarded verbatim
        // to `push_index_impl`'s identical `# Safety` contract — not
        // discharged locally, just passed through. `#![deny(unsafe_op_in_unsafe_fn)]`
        // requires this local block even inside this `unsafe fn`'s own body.
        unsafe { push_index_impl::<B, S>(self, index) }
    }

    #[track_caller]
    fn pop_index(&self) -> Option<u32> {
        pop_index_impl::<B, S>(self)
    }
}

/// Cold panic path for [`StackOps::push_index`]'s `index < INDEX_MASK`
/// caller-contract guard, split out of `push_index` itself so the panic and
/// its message formatting can never land in the hot loop's body (`#[cold]` +
/// `#[inline(never)]`). `#[track_caller]` here — combined with
/// `#[track_caller]` on `push_index` — forwards `push_index`'s received
/// caller location down, so a consumer pushing from many call sites learns
/// WHICH one violated the contract.
#[cold]
#[inline(never)]
#[track_caller]
fn push_index_out_of_range(index: u32, mask: u64) -> ! {
    panic!(
        "index must be < INDEX_MASK (the empty sentinel is reserved), \
         got {index} (INDEX_MASK = {mask:#x})"
    );
}

/// Cold panic path for [`StackOps::pop_index`]'s clause-4 guard, split out of
/// `pop_index` itself — same `#[cold]` + `#[inline(never)]` +
/// `#[track_caller]` shape and caller-location-chaining rationale as
/// [`push_index_out_of_range`] above. Reports which of the three caught
/// shapes the caller's [`load_next`](StackStorage::load_next) answer has
/// — a self-loop (`next == index`; see `# Panics` on
/// [`StackOps::pop_index`] for the full two-cause disjunction it proves,
/// not restated here) or one of the two truncation outcomes an over-wide
/// value would silently produce.
#[cold]
#[inline(never)]
#[track_caller]
fn pop_link_out_of_range(index: u32, next: u32, mask: u64) -> ! {
    if next == index {
        panic!(
            "load_next({index}) returned {next:#x}, the index's own link points \
             back to itself — a self-loop, corrupting the free-list into a cycle: \
             pop_index's truncating pack would silently re-issue this same index \
             to a second owner"
        );
    }
    let outcome = if (u64::from(next) & mask) == mask {
        "the EMPTY SENTINEL, leaking the whole remaining chain"
    } else {
        "a wrong index, possibly a live one — double-issuing it"
    };
    panic!(
        "load_next({index}) returned {next:#x}, neither TAIL nor \
         a valid index (< {mask:#x}): pop_index's truncating pack would silently \
         truncate it to {outcome}"
    );
}

/// An owned standalone stack: head and links fused into one object. A
/// lock-free LIFO free-list of indices with a STRICTLY MONOTONIC generation
/// tag packed into the head word that ELIMINATES ABA outright at every
/// permitted `INDEX_BITS` — it never wraps; a push that would need to bump
/// the tag past [`TaggedIndex::TAG_MAX`] is refused instead
/// (`Err(`[`TagExhausted`]`)`), sealing the stack (pops are unaffected and
/// keep draining). The pushes-until-sealed lifetime is derived in the
/// crate-root docs' "Tag-width budget" section. Const-generic over the
/// index width `INDEX_BITS` and the link capacity `N`.
///
/// Fusion is ALSO the structural closure of the shared-head hazard: this
/// type deliberately does NOT implement the public [`StackStorage`] trait
/// (its head↔links binding is served by a crate-internal sealed accessor
/// instead), its `head` field is private, and no trait impl hands out a
/// `&StackHead` for it — so building a competing binding around a
/// standalone `ArrayIndexStack` does not COMPILE (E0277/E0599, pinned by
/// `tests/compile_fail/array_index_stack_head/`, the compile-fail successor
/// of the former `array_index_stack_head_still_double_issue` runtime
/// demonstration). That fixture pins one instantiation (`<16, 64>`); the
/// seal itself is instantiation-independent and held by COHERENCE, not by
/// the fixture: any in-crate `impl StackStorage<B> for ArrayIndexStack<B, N>`
/// fails with **E0119** (it would overlap the `pub(crate)` blanket bridge
/// the stack's own algorithm is written against), and any out-of-crate
/// attempt fails with **E0117** (orphan rule) — do not mistake the
/// one-instantiation fixture for the only thing holding the seal. The
/// remaining hazard class is over CUSTOM
/// [`StackStorage`] implementors — see the trait doc's "The shared-storage
/// hazard class" section.
///
/// The simple [`push`](Self::push)/[`pop`](Self::pop) inherent methods exist
/// for standalone callers (`push` is now an `unsafe fn`, carrying
/// [`StackOps::push_index`]'s `# Safety` contract); a fresh stack is EMPTY (lazy links, RAD-1) — the
/// caller pushes indices as they become free. Custom implementors with
/// slot-resident links do not use this type: they implement [`StackStorage`]
/// instead and call the [`StackOps`] methods.
#[derive(Debug)]
pub struct ArrayIndexStack<const INDEX_BITS: u32, const N: usize> {
    head: StackHead<INDEX_BITS>,
    links: ArrayLinks<N>,
}

impl<const B: u32, const N: usize> ArrayIndexStack<B, N> {
    /// A fresh, EMPTY stack (head = the bootstrap empty sentinel, tag 0; every
    /// link at `0`). Under `--cfg loom` this cannot be `const` (loom's atomics
    /// have no `const` ctor).
    #[cfg(not(loom))]
    #[must_use]
    pub const fn new() -> Self {
        Self {
            head: StackHead::new(),
            links: ArrayLinks::new(),
        }
    }

    /// A fresh, EMPTY stack (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            head: StackHead::new(),
            links: ArrayLinks::new(),
        }
    }

    /// Push `index` onto the stack, driving the crate-internal CAS-retry
    /// algorithm (`push_index_impl`) directly. This type deliberately does
    /// NOT implement the public [`StackStorage`] trait (see the type doc), so
    /// it does not go through [`StackOps::push_index`]'s blanket impl — the
    /// identical algorithm body is crate-internal now. See
    /// [`StackOps::push_index`]'s doc for the algorithm, its `# Safety`
    /// section (the caller contract) and `# Panics`.
    ///
    /// # Safety
    ///
    /// Same contract as [`StackOps::push_index`]'s `# Safety` — see there
    /// (one normative location; this crate cross-references it).
    ///
    /// # Errors
    ///
    /// Same as [`StackOps::push_index`]'s `# Errors` — see there.
    // `#[track_caller]` chains the caller location through the forwarder down
    // to `push_index_impl` and its `#[cold]` panic helper, so diagnostics through
    // the owned type name the user's call site exactly as the trait method does.
    #[track_caller]
    #[allow(unsafe_code)]
    // Single documented reason to hold `unsafe`: forwards
    // `StackOps::push_index`'s caller-side unsafe contract (link domain +
    // liveness + exclusive ownership) to the shared body `push_index_impl`.
    pub unsafe fn push(&self, index: u32) -> Result<(), TagExhausted> {
        // SAFETY: forwards this fn's own caller-side contract (link domain +
        // liveness + exclusive ownership, same as `StackOps::push_index`'s
        // `# Safety`) verbatim to
        // `push_index_impl` — not discharged locally, just passed through.
        // `#![deny(unsafe_op_in_unsafe_fn)]` requires this local block even
        // inside this `unsafe fn`'s own body.
        unsafe { push_index_impl::<B, _>(self, index) }
    }

    /// Pop the top index off the stack, or `None` if empty — driving the
    /// crate-internal CAS-retry algorithm (`pop_index_impl`) directly. This
    /// type deliberately does NOT implement the public [`StackStorage`] trait
    /// (see the type doc), so it does not go through
    /// [`StackOps::pop_index`]'s blanket impl — the identical algorithm body
    /// is crate-internal now. See [`StackOps::pop_index`]'s doc for the
    /// algorithm and `# Panics`.
    #[must_use = "a popped index is removed from the free-list; discarding it leaks the slot"]
    // `#[track_caller]` chains the caller location through the forwarder down
    // to `pop_index_impl` and its `#[cold]` panic helper, so diagnostics through
    // the owned type name the user's call site exactly as the trait method does.
    #[track_caller]
    pub fn pop(&self) -> Option<u32> {
        pop_index_impl::<B, _>(self)
    }

    /// Whether the stack is currently empty. Advisory `Relaxed` check — see
    /// [`StackHead::is_empty`].
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.head.is_empty()
    }

    /// Successful pushes this head can still accept before `push` starts
    /// refusing with `Err(`[`TagExhausted`]`)` — forwarder to
    /// [`StackHead::pushes_remaining`].
    #[must_use]
    pub fn pushes_remaining(&self) -> u64 {
        self.head.pushes_remaining()
    }

    /// The raw packed head word (`Acquire`) — forwarder to
    /// [`StackHead::raw_head`] (tests/loom suite need it).
    ///
    /// Gated: same `test-internals`/loom gate as [`StackHead::raw_head`] —
    /// it does not exist in a default build.
    #[doc(hidden)]
    #[cfg(any(feature = "test-internals", loom))]
    #[must_use]
    pub fn raw_head(&self) -> u64 {
        self.head.raw_head()
    }

    /// **loom-test-only** raw CAS on the head word — forwarder to
    /// [`StackHead::cas_head_for_test`].
    ///
    /// # Errors
    ///
    /// Forwards `AtomicU64::compare_exchange`'s `Err(actual)` on CAS failure.
    #[cfg(loom)]
    #[doc(hidden)]
    pub fn cas_head_for_test(
        &self,
        current: u64,
        new: u64,
        success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.head.cas_head_for_test(current, new, success, failure)
    }

    /// **test-only** read-only link forwarder — loads index `index`'s
    /// link cell (`Acquire`), forwarding to [`ArrayLinks::load_next`].
    /// The shipped test suites read a link directly off the REAL
    /// [`ArrayIndexStack`] (the loom suite `tests/loom_aba.rs` splits a
    /// pop's link read from its CAS; `tests/stack_unit.rs`'s
    /// `links_are_lazy` reads a never-pushed index's link), which they can
    /// no longer do through [`StackStorage::load_next`] now that this type
    /// deliberately does not implement the public [`StackStorage`] trait.
    /// `#[doc(hidden)]` per the crate's established test-only-forwarder
    /// rationale (see [`raw_head`] and [`cas_head_for_test`]): not public
    /// API. Gated: same `test-internals`/loom gate as [`StackHead::raw_head`]
    /// — it does not exist in a default build. Read-only — it exposes no
    /// `&StackHead` and no link write, so it reopens none of the sealed
    /// hazard.
    #[doc(hidden)]
    #[cfg(any(feature = "test-internals", loom))]
    pub fn load_next_for_test(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }

    /// **test-only** write-side twin of [`load_next_for_test`] — stores
    /// `next` into `index`'s link cell directly
    /// ([`ArrayLinks::store_next`], `Release`), bypassing the stack
    /// algorithm entirely. Needed for a hand-inlined counterfactual that
    /// reproduces the OLD (pre-seal) wrapping behaviour without going
    /// through the real, now-sealing, [`push`](Self::push) — see
    /// `tests/loom_aba.rs`'s tiny-tag counterfactual.
    ///
    /// `#[doc(hidden)]` per this crate's established test-only-forwarder
    /// rationale (see [`raw_head`]). Gated: `loom` only — unlike
    /// [`load_next_for_test`], this is a raw link-cell WRITE that bypasses
    /// the stack algorithm entirely; under plain `test-internals` (a
    /// published, downstream-enabled Cargo feature) it would be a safe
    /// `pub fn` reachable by any consumer, letting safe code construct a
    /// cycle in the linked chain (e.g. double-issuing an index from
    /// `pop()`). Its only real caller is `tests/loom_aba.rs`, which is
    /// itself `#![cfg(loom)]`-gated, so `loom` alone is the correct and
    /// sufficient gate.
    #[doc(hidden)]
    #[cfg(loom)]
    pub fn store_next_for_test(&self, index: u32, next: u32) {
        self.links.store_next(index, next);
    }

    /// **test-only** constructor seeding a specific tag — forwarder to
    /// [`StackHead::with_tag_for_test`] (see its doc, including its `# Panics`
    /// contract for an out-of-range tag).
    ///
    /// `#[doc(hidden)]` + gated: same test-only-forwarder convention as
    /// [`raw_head`].
    ///
    /// # Panics
    /// Panics if `tag > `[`TaggedIndex::TAG_MAX`].
    #[doc(hidden)]
    #[cfg(any(feature = "test-internals", loom))]
    #[must_use]
    pub fn with_tag_for_test(tag: u64) -> Self {
        Self {
            head: StackHead::with_tag_for_test(tag),
            links: ArrayLinks::new(),
        }
    }
}

impl<const B: u32, const N: usize> Default for ArrayIndexStack<B, N> {
    fn default() -> Self {
        Self::new()
    }
}

// Tier-2 item-scoped allow — one of the crate's audited lint-exception regions
// (see the crate docs' "Where unsafe lives"). Single documented reason to
// hold `unsafe`: this impl's `store_next` is an `unsafe fn` declaration —
// a verbatim forwarder to the safe [`ArrayLinks::store_next`], whose proof
// lives at [`push_index_impl`].
#[allow(unsafe_code)]
impl<const B: u32, const N: usize> SealedStorage<B> for ArrayIndexStack<B, N> {
    fn head(&self) -> &StackHead<B> {
        &self.head
    }
    fn load_next(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }
    /// # Safety
    ///
    /// Verbatim forwarder — same contract as [`StackStorage::store_next`].
    unsafe fn store_next(&self, index: u32, next: u32) {
        self.links.store_next(index, next)
    }
}

/// An owned `[AtomicU32; N]` link backing (now used inside the fused
/// [`ArrayIndexStack`]; slot-resident implementors host their own links
/// instead). Every link starts at `0` — matching OS-zeroed backing — and is
/// only ever written by a push (RAD-1: no eager free-list chaining).
///
/// # Layout note — link-array false sharing
///
/// Each link is a 4-byte `AtomicU32`, so 16 consecutive indices share one
/// 64-byte cache line. If indices from the same 16-index group are handed to
/// different threads under contention, this array becomes a SECOND contended
/// surface alongside the stack's own head — contended by accident of index
/// numbering, not by design. Fix it at the CALLER when a profile shows it:
/// wrap the index-to-link mapping so contended indices land in different
/// groups, use a `#[repr(align(64))]` newtype per link, or — this crate's own
/// README's recommendation for production — host links slot-resident inside a
/// larger per-slot struct. Do NOT pad `ArrayLinks` itself to one link per
/// cache line: that would multiply its footprint 16x for every
/// single-threaded (or contention-indifferent) caller.
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

    /// Load the "next" link for `index` with `Acquire` ordering.
    ///
    /// This `Acquire` — and [`store_next`](Self::store_next)'s `Release` — is
    /// deliberately retained rather than weakened to `Relaxed`, which the
    /// stack's own head-publication proof would permit: defence-in-depth, see
    /// [`StackStorage`]'s "Ordering contract".
    ///
    /// # Panics
    ///
    /// Panics if `index >= N`. `N` (this backing's capacity) and the
    /// stack's `INDEX_BITS` are independent const parameters with nothing
    /// relating them, so a stack can accept an index this
    /// backing cannot hold — see [`StackOps::push_index`]'s note on the
    /// two bounds.
    #[must_use]
    pub fn load_next(&self, index: u32) -> u32 {
        self.next[index as usize].load(Ordering::Acquire)
    }

    /// Store the "next" link for `index` with `Release` ordering. This is the
    /// ONLY write the stack makes to link storage, and only during a push — the
    /// lazy-link (RAD-1) discipline: link storage is never eagerly initialised.
    /// Like [`load_next`](Self::load_next)'s `Acquire`, this `Release` is
    /// deliberate defence-in-depth, not a stack-proof requirement — see
    /// [`StackStorage`]'s "Ordering contract".
    ///
    /// # Panics
    ///
    /// Panics if `index >= N` — the same bound as
    /// [`load_next`](Self::load_next); likewise independent of the stack's
    /// `INDEX_BITS` (see [`StackOps::push_index`]'s note on the two
    /// bounds).
    pub fn store_next(&self, index: u32, next: u32) {
        self.next[index as usize].store(next, Ordering::Release);
    }
}

impl<const N: usize> Default for ArrayLinks<N> {
    fn default() -> Self {
        Self::new()
    }
}

/// Test-only activation counter for
/// [`pop_index`](StackOps::pop_index)'s CAS-retry branch (the
/// `Err(actual) => head = actual` arm, incremented there). Deliberately a REAL
/// `core::sync::atomic::AtomicUsize`, NOT `loom::sync::atomic`: loom re-runs
/// the closure passed to `Builder::check` across many schedules within one
/// process, and a real static survives those re-runs, so the accumulated count
/// is an exact "how often was the retry branch actually reached" oracle over an
/// entire exploration. `Relaxed` access: the counter promises no ordering, it
/// only counts.
///
/// Gated: compiled ONLY under the `test-internals` feature or a loom build —
/// a default build of the crate carries neither the counters nor the
/// retry-arm increments that write them (shipping two process-global atomics
/// and a hot-path write per lost CAS to consumers who can neither use nor
/// remove them would be unjustified). Under the gate it serves the
/// `#[cfg(loom)]` loom suite via `pop_retry_count_for_test` and the non-loom
/// threaded test via [`retry_counts_for_test`]. Cost when enabled: one
/// Relaxed `fetch_add` per lost CAS, on the retry arm only — the
/// uncontended fast path never touches it. Never reset by this crate
/// (snapshot and diff is the caller's job); process-global and cumulative.
#[cfg(any(feature = "test-internals", loom))]
static POP_RETRY_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// **loom-test-only** activation oracle: reads `POP_RETRY_COUNT` — the
/// number of times `pop_index`'s CAS-retry branch has executed in this
/// process. The loom suite asserts this counter ADVANCES across an exploration
/// so a model whose schedules never actually reach `pop_index`'s retry path
/// fails loudly instead of passing vacuously (see the assertion in
/// `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`).
///
/// `#[doc(hidden)]`: see [`raw_head`](StackHead::raw_head)'s rationale.
/// Never reset: process-global and cumulative — see `POP_RETRY_COUNT`'s doc
/// (the shipped loom suite's `MODEL_LOCK` serializes tests that read it).
#[cfg(loom)]
#[doc(hidden)]
#[must_use]
pub fn pop_retry_count_for_test() -> usize {
    POP_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

/// Push-side twin of `POP_RETRY_COUNT` — identical rationale, gate, ordering
/// and never-reset semantics; counts [`push_index`](StackOps::push_index)'s
/// CAS-retry branch (the `Err(actual) => head = actual` arm). See
/// `POP_RETRY_COUNT`'s doc. Serves `push_retry_count_for_test` and
/// [`retry_counts_for_test`].
#[cfg(any(feature = "test-internals", loom))]
static PUSH_RETRY_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// **loom-test-only** activation oracle: reads `PUSH_RETRY_COUNT` — the
/// number of times `push_index`'s CAS-retry branch has executed in this
/// process. The loom suite asserts this counter ADVANCES across an exploration
/// so a model whose schedules never actually reach `push_index`'s retry path
/// fails loudly instead of passing vacuously (see the assertion in
/// `push_push_conservation`).
///
/// `#[doc(hidden)]`: see [`raw_head`](StackHead::raw_head)'s rationale.
/// Never reset: process-global and cumulative — see `POP_RETRY_COUNT`'s doc
/// (the shipped loom suite's `MODEL_LOCK` serializes tests that read it).
#[cfg(loom)]
#[doc(hidden)]
#[must_use]
pub fn push_retry_count_for_test() -> usize {
    PUSH_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

/// **test-only** activation oracle: reads BOTH CAS-retry counters in one
/// call, as `(pop, push)` — `POP_RETRY_COUNT` first, then
/// `PUSH_RETRY_COUNT`. The non-loom twin of `pop_retry_count_for_test` /
/// `push_retry_count_for_test` (both `#[cfg(loom)]`, invisible to a plain
/// build): `tests/threaded_conservation.rs` snapshots this tuple before its
/// threaded phase and asserts BOTH counters advanced after it — the FIRST
/// half of its two-level activation oracle, pinning that the retry branches
/// are reached under real threads ([`backoff_cap_reached_for_test`] supplies
/// the second half — that `spins` climbs into its higher range; this counter
/// alone cannot even distinguish 1 retry from thousands).
///
/// `#[doc(hidden)]`: see [`raw_head`](StackHead::raw_head)'s rationale.
/// Gated: under the same `test-internals`/loom gate as the counters
/// themselves — it does not exist in a default build.
///
/// Never reset: process-global and cumulative — see `POP_RETRY_COUNT`'s doc
/// — so a test wanting a delta exclusive to its own window must be the only
/// active driver of the real `push_index`/`pop_index` during it (the loom
/// suite serializes with `MODEL_LOCK`; `threaded_conservation.rs` is a
/// one-test binary, so its window is exclusive by construction).
#[doc(hidden)]
#[must_use]
#[cfg(any(feature = "test-internals", loom))]
pub fn retry_counts_for_test() -> (usize, usize) {
    (
        POP_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed),
        PUSH_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed),
    )
}

/// Test-only backoff-activation counter for
/// [`pop_index`](StackOps::pop_index): incremented in `pop_index`'s retry arm
/// for every retry whose spin loop ran at FULL backoff depth (`spins` already
/// saturated at `BACKOFF_SPIN_CAP`, so `1 << BACKOFF_SPIN_CAP` = 64
/// `spin_loop` iterations actually executed). Non-zero proves the backoff
/// climbs into its higher range under real contention; a regression that caps
/// `spins` at 0, resets it per iteration, or moves its increment off the
/// reachable path zeroes this counter while `POP_RETRY_COUNT` keeps advancing
/// — exactly the silently-inert-backoff failure
/// `tests/threaded_conservation.rs`'s second oracle level catches. Same gate,
/// ordering and never-reset semantics as `POP_RETRY_COUNT`; see its doc for
/// the gating rationale.
#[cfg(any(feature = "test-internals", loom))]
static POP_BACKOFF_CAP_REACH_COUNT: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// Test-only backoff-activation counter for
/// [`push_index`](StackOps::push_index): the push-side twin of
/// `POP_BACKOFF_CAP_REACH_COUNT` — same condition, same gate, same never-reset
/// semantics.
#[cfg(any(feature = "test-internals", loom))]
static PUSH_BACKOFF_CAP_REACH_COUNT: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// **test-only** backoff-activation oracle: reads BOTH backoff-cap-reach
/// counters in one call, as `(pop, push)` — `POP_BACKOFF_CAP_REACH_COUNT`
/// first, then `PUSH_BACKOFF_CAP_REACH_COUNT`. The second half of
/// `tests/threaded_conservation.rs`'s two-level activation oracle: where
/// [`retry_counts_for_test`] proves only that a retry branch
/// was reached at all, a non-zero delta here proves `spins` genuinely
/// climbs into its higher range — at least one call per branch executed
/// its spin loop at full `1 << BACKOFF_SPIN_CAP` depth — so a future
/// change that silently disarms the backoff fails loudly instead of
/// shipping with the documented behavior inert.
///
/// `#[doc(hidden)]`: see [`raw_head`](StackHead::raw_head)'s rationale.
/// Gated: under the same `test-internals`/loom gate as the counters
/// themselves — it does not exist in a default build.
/// Never reset: process-global and cumulative, like
/// [`retry_counts_for_test`].
#[doc(hidden)]
#[must_use]
#[cfg(any(feature = "test-internals", loom))]
pub fn backoff_cap_reached_for_test() -> (usize, usize) {
    (
        POP_BACKOFF_CAP_REACH_COUNT.load(core::sync::atomic::Ordering::Relaxed),
        PUSH_BACKOFF_CAP_REACH_COUNT.load(core::sync::atomic::Ordering::Relaxed),
    )
}
