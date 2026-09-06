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
/// `TAIL` (`u32::MAX`) terminates a link chain; the head's empty sentinel is
/// `INDEX_MASK` (at most `0xFFFF`). Push and pop translate between them.
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
/// The shipped cap is a deliberate fairness-vs-throughput compromise, not a
/// low-contention optimum: caps 8/10 give more aggregate throughput but
/// measurably worse per-thread fairness under oversubscription, while caps
/// 0/4 are fairer but slower. Measurements and the full fairness/throughput
/// tables are in `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (a repository
/// file). `spin_loop` is a processor hint, so one unit is not a portable time
/// unit; the measured trade is host- and microarchitecture-specific. Lock-free
/// is not starvation-free — see the crate-root doc's "Lock-freedom and
/// starvation" section for the measured trade.
const BACKOFF_SPIN_CAP: u32 = 6;

// Keep the shift bound compile-time checked so overflow cannot become a release-build bug.
const _: () = assert!(BACKOFF_SPIN_CAP < 32);

/// Per-call exponential-backoff state for the CAS-retry arms: wraps the retry
/// counter (`K`, starting at 0) that drives the spin-loop depth of
/// [`Backoff::spin`]. Starts fresh every call, never persisted.
struct Backoff(u32);

impl Backoff {
    /// Inline into generic retry-loop instantiations; otherwise this private
    /// non-generic helper could remain an out-of-line call.
    #[inline]
    fn new() -> Self {
        Backoff(0)
    }

    /// Exponential backoff before retrying (BACKOFF_SPIN_CAP): spins
    /// `1 << K` times, letting the winning thread's Release CAS drain off
    /// the head cache line instead of every loser re-hammering it
    /// immediately. `K` grows only within one call.
    ///
    ///
    /// Capped, not unconditional: saturation keeps `K <= BACKOFF_SPIN_CAP`,
    /// so `1u32 << K` can never overflow (`K` = 32 would, after only 32
    /// consecutive lost CASes in one call — an ordinary event under the
    /// contention this crate is built for, not a remote one). The
    /// saturation also guarantees `self.0 <= BACKOFF_SPIN_CAP` at the
    /// shift, so no `.min` guard is needed on the shift expression.
    #[inline]
    fn spin(&mut self) {
        #[cfg(loom)]
        BACKOFF_SPIN_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        for _ in 0..(1u32 << self.0) {
            core::hint::spin_loop();
        }
        if self.0 < BACKOFF_SPIN_CAP {
            self.0 += 1;
        }
    }

    #[cfg(any(tagged_index_stack_test, loom))]
    #[inline]
    fn depth(&self) -> u32 {
        self.0
    }
}

// Test/loom instrumentation is kept beside the state it observes. The note
// functions are unconditional so retry sites have one cfg boundary only.
#[inline]
fn note_pop_retry() {
    #[cfg(any(tagged_index_stack_test, loom))]
    POP_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

#[inline]
fn note_push_retry() {
    #[cfg(any(tagged_index_stack_test, loom))]
    PUSH_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
}

#[cfg(any(tagged_index_stack_test, loom))]
static POP_RETRY_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);
#[cfg(loom)]
static BACKOFF_SPIN_COUNT: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
#[cfg(any(tagged_index_stack_test, loom))]
static PUSH_RETRY_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

#[cfg(loom)]
#[doc(hidden)]
#[must_use]
pub fn backoff_spin_count_for_test() -> usize {
    BACKOFF_SPIN_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

#[doc(hidden)]
#[must_use]
#[cfg(any(tagged_index_stack_test, loom))]
pub fn retry_counts_for_test() -> (usize, usize) {
    (
        POP_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed),
        PUSH_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed),
    )
}

#[doc(hidden)]
#[must_use]
#[cfg(any(tagged_index_stack_test, loom))]
pub fn backoff_spin_depths_for_test() -> [u32; 9] {
    let mut backoff = Backoff::new();
    let mut depths = [0; 9];
    for depth in &mut depths {
        *depth = 1u32 << backoff.depth();
        backoff.spin();
    }
    depths
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
    /// every legal width `INDEX_MASK <= 0xFFFF`, so `INDEX_MASK != TAIL`
    /// and `index == TAIL` can never silently pass the runtime guard.
    ///
    /// This `const` is forced to evaluate from every associated item of
    /// `TaggedIndex<INDEX_BITS>`: [`pack`](Self::pack) forces it directly
    /// with a `let () = Self::_CHECK_BITS;` statement, `INDEX_MASK` and
    /// [`TAG_BITS`](Self::TAG_BITS) evaluate it in their own initializers,
    /// and [`unpack`](Self::unpack), [`empty_index`](Self::empty_index),
    /// [`is_empty`](Self::is_empty), and the
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
    /// tag IS accepted here — the legitimate tag-preserving empty transition
    /// ([`empty_index`](Self::empty_index)).
    ///
    /// `push_index`/`pop_index` do NOT call this function on the hot path:
    /// their inputs are already proven within range by the crate's own
    /// guards, so they pack through the crate-private truncating fast path
    /// `pack_truncating` purely to skip this function's redundant range
    /// re-check (see its doc).
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
    /// and the bootstrap constructor. All three prove `tag <= TAG_MAX` before
    /// calling — push's seal check refuses an already-[`TAG_MAX`](Self::TAG_MAX)
    /// tag before its `tag + 1` bump — so truncation never actually discards
    /// a bit on this path, and the push caller's plain `+` bump can never
    /// overflow at any legal width (`TAG_MAX <= 2^63 - 1`): there is nothing
    /// for a wrapping or saturating operator to guard.
    ///
    /// This helper is NOT a wrap-on-truncate mechanism: it does not wrap
    /// the tag back to 0, and must never be made to — wrap-on-truncation
    /// would reopen the exact stale-CAS double-issue the seal
    /// ([`TAG_MAX`](Self::TAG_MAX) + [`TagExhausted`]) exists to close
    /// (see the crate-root docs' "The tag is strictly monotonic" section).
    #[must_use]
    pub(crate) const fn pack_truncating(index: u32, tag: u64) -> u64 {
        let () = Self::_CHECK_BITS;
        debug_assert!(
            index as u64 <= Self::INDEX_MASK,
            "pack_truncating: index out of range — must be <= INDEX_MASK"
        );
        debug_assert!(
            tag <= Self::TAG_MAX,
            "pack_truncating: tag out of range — must be <= TAG_MAX (all \
             callers prove this before calling — see this fn's doc)"
        );
        (tag << INDEX_BITS) | (index as u64)
    }

    /// Split a packed word back into `(u32 index, u64 tag)`.
    #[must_use]
    pub const fn unpack(word: u64) -> (u32, u64) {
        // INDEX_MASK <= 0xFFFF at every legal width per `_CHECK_BITS`, so the
        // AND result is <= 0xFFFF and the cast is lossless by construction.
        ((word & Self::INDEX_MASK) as u32, word >> INDEX_BITS)
    }

    /// Bootstrap empty-stack word: index =
    /// [`empty_index`](Self::empty_index), tag = 0. A freshly-constructed
    /// [`StackHead`] is this.
    ///
    /// **Only bootstrap-time emptiness uses tag 0 unconditionally.** A RUNTIME
    /// empty transition (a pop that drains the last element) MUST preserve the
    /// running tag — see [`empty_index`](Self::empty_index); resetting to 0
    /// there reopens the ABA window (see the crate docs' tag-preservation note).
    ///
    /// Crate-private bootstrap helper. Runtime empty transitions must use the
    /// observed tag instead; this word is only valid for construction.
    #[must_use]
    const fn bootstrap_empty() -> u64 {
        Self::pack_truncating(Self::INDEX_MASK_U32, 0)
    }

    /// The empty sentinel's index half: the `u32` form of `INDEX_MASK`, for
    /// packing it with a
    /// NON-zero, caller-supplied RUNNING tag (`pack(empty_index(), running_tag)`)
    /// instead of the crate-private bootstrap helper, which always zeroes the
    /// tag.
    ///
    /// **Empty-transition tag preservation:** the transition in [`pop_index`](StackOps::pop_index)
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
/// monotonic" section). The stack is then SEALED: every further
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
/// (see the crate-root docs' tag-preservation note on why a naive tag reset is unsound in
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
            head: AtomicU64::new(TaggedIndex::<INDEX_BITS>::bootstrap_empty()),
        }
    }

    /// A fresh, EMPTY stack head (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        Self {
            head: AtomicU64::new(TaggedIndex::<INDEX_BITS>::bootstrap_empty()),
        }
    }

    /// Thin wrapper over the head atomic's load — for the [`StackOps`] blanket
    /// impl.
    pub(crate) fn load(&self, ordering: Ordering) -> u64 {
        self.head.load(ordering)
    }

    /// Thin wrapper over the head atomic's strong compare_exchange — for the
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
    #[cfg(any(tagged_index_stack_test, loom))]
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
    /// here for the generic rationale): this is a `pub` item only so
    /// `tests/` — an external crate from this crate's own perspective — can
    /// reach it. Gated: compiled only under the repository test cfg or a
    /// loom build — a default build (a downstream consumer, the docs.rs
    /// render) does not contain this item at all, so unlike `#[doc(hidden)]`
    /// alone the gate makes it genuinely unnameable from safe downstream
    /// code, not merely hidden from rustdoc navigation. It is not exercised
    /// by any production caller. It is an unstable repository-test surface;
    /// enabling the test cfg is not a semver promise for these probes.
    #[doc(hidden)]
    #[cfg(any(tagged_index_stack_test, loom))]
    #[must_use]
    pub fn raw_head(&self) -> u64 {
        self.head.load(Ordering::Acquire)
    }

    /// **loom-test-only** raw CAS on the head word, exposed so the shipped loom
    /// proof (`tests/loom_aba.rs`) can split a pop's head-load from its CAS —
    /// opening the ABA window the real `pop_index` closes internally — and
    /// drive the buggy-drain counterfactual, all against the REAL head atomic.
    /// Not part of the stable API: it is compiled only under `--cfg loom`.
    ///
    /// `#[doc(hidden)]`: see [`raw_head`](StackHead::raw_head)'s
    /// rationale. This item carries the strictly narrower `#[cfg(loom)]`
    /// gate (vs `raw_head`'s test-cfg-or-loom gate), so it does not exist
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

/// One implementor supplies a [`StackHead`] and atomic per-index links. The
/// head↔links binding is fixed by the implementor; links may be slot-resident
/// or fused in [`ArrayIndexStack`]. The implementation must be non-blocking if
/// the resulting [`StackOps`] operations are to remain lock-free.
///
/// # Safety
///
/// Implementing this trait is a soundness commitment. The implementor must:
///
/// 1. Bind each head to exactly one live implementor and one link backing for
///    its whole life; never share or rebind it while an index is reachable.
/// 2. Use one stable index↔cell mapping. After an `Acquire` observation of a
///    head published by a `Release` push, `load_next` must observe that push's
///    `store_next` or a later write in the cell's modification order, never an
///    earlier write. A later pop+repush may legitimately be the observed write.
/// 3. Keep reachable index populations disjoint across bindings sharing link
///    cells. Sharing cells with disjoint populations is allowed.
/// 4. Return only [`TAIL`] or a valid index from a dedicated, non-payload-
///    aliased link cell.
/// 5. Return the same logical head from [`head`](Self::head) on every call.
/// 6. Document a fixed link domain, a subset of `0 .. INDEX_MASK`, with a
///    dedicated cell for every member. Hooks must be memory-safe in that
///    domain; unchecked access outside it is permitted only because the
///    caller contract guarantees the algorithm never supplies such an index.
/// 7. Make every link-cell access atomic; a stale popper may read while a
///    concurrent push writes and then lose its head CAS.
///
/// # Ordering contract
///
/// `load_next` must use `Acquire` (or stronger), and `store_next` must use
/// `Release` (or stronger). Head publication already carries the link's
/// visibility, but these link orderings are deliberate defence-in-depth and
/// keep an implementation independent of the stack's internal head orderings;
/// see `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` for the measured status.
///
/// The three hooks are `unsafe fn`: the compiler requires an unsafe call site,
/// while the semantic obligations above remain the caller's responsibility.
/// Their caller-side contracts are stated on the methods below. The complete
/// design rationale is in
/// `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`.
///
/// # Stability
///
/// This trait is intentionally open for external slot-resident implementations;
/// future methods will have default bodies or require a major release.
#[allow(unsafe_code)]
// Unsafe: implementors bind one head to stable atomic link storage.
pub unsafe trait StackStorage<const INDEX_BITS: u32> {
    /// The stack's head word.
    ///
    /// # Safety
    ///
    /// The caller may use the returned reference only as this binding's head
    /// and must not create a competing head↔links binding around it.
    unsafe fn head(&self) -> &StackHead<INDEX_BITS>;

    /// Load `index`'s next link with `Acquire` ordering.
    ///
    /// # Safety
    ///
    /// `index` must have been pushed through this exact binding at least once,
    /// so its link cell was initialized by `store_next`. It need not remain
    /// reachable: a concurrent popper may win before this caller's CAS.
    unsafe fn load_next(&self, index: u32) -> u32;

    /// Store `index`'s next link with `Release` ordering. This is the only
    /// stack write to link storage, and it is lazy: links are written only
    /// during a push immediately before that push's publishing CAS.
    ///
    /// # Safety
    ///
    /// The caller must be in the CAS-valid push phase: `index` satisfies
    /// [`StackOps::push_index`]'s three caller obligations, `next` is
    /// [`TAIL`] or the index observed as this binding's head, and this call
    /// precedes the CAS that publishes `index`.
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
    /// The current head index (or [`TAIL`] when empty) is stored in `index`'s
    /// link with `Release`, then the head is CASed to `(index, tag + 1)`.
    /// The numeric guard below is unconditional because an invalid index would
    /// corrupt the free-list and could double-issue a slot.
    ///
    /// # Safety
    ///
    /// The caller must uphold all three clauses:
    ///
    /// 1. `index` is in this implementor's documented link domain: it has a
    ///    dedicated cell and the hooks are memory-safe for it. The runtime
    ///    `index < INDEX_MASK` guard checks only the packed-word range, not
    ///    this possibly narrower domain.
    /// 2. `index` is not reachable through any binding whose hooks touch the
    ///    same link cells. It was never pushed, or its latest push was followed
    ///    by a successful [`pop_index`](Self::pop_index) returning it. A stale
    ///    popper that observed the index but lost its CAS did not pop it and
    ///    does not block this push; the tag makes that stale CAS fail, and this
    ///    push overwrites the old link before publishing the index.
    /// 3. This call owns a unique, unconsumed publish/recycle authority: either
    ///    a fresh index or one returned by a specific successful pop. The
    ///    authority is consumed at this call's successful head CAS, not at
    ///    physical return, so a later popper may republish the index with its
    ///    own authority before this call returns. Two pushes may not consume the
    ///    same authority without an intervening successful pop. These liveness
    ///    and authority obligations are not runtime-checked; violating them
    ///    can create a cycle or double-issue an index.
    ///
    /// # Errors
    ///
    /// Returns `Err(`[`TagExhausted`]`)` without publishing when the current tag
    /// is [`TaggedIndex::TAG_MAX`]. The head is then permanently sealed; the
    /// refused index remains the caller's. A retry may have left stale link
    /// contents, which the next successful push overwrites before publishing.
    ///
    /// # Panics
    ///
    /// Panics if `index >= INDEX_MASK` (the empty sentinel is reserved), in both
    /// debug and release. The implementor's link hooks may enforce a narrower
    /// bound; callers must satisfy that bound too.
    #[track_caller]
    #[allow(unsafe_code)]
    // Single documented reason to hold `unsafe`: this method carries the
    // caller-side three-clause unsafe contract (link domain + liveness +
    // exclusive ownership), relied on for memory safety by allocator
    // consumers; see the `# Safety` section above.
    unsafe fn push_index(&self, index: u32) -> Result<(), TagExhausted>;

    /// Pop the top index off the stack, or `None` if empty.
    /// Loads the tagged head, reads its next link, and CASes the head to that
    /// link without changing the tag. A popper that loses its CAS retries with
    /// an `Acquire` observation; the monotonic, sealing tag prevents ABA.
    ///
    /// When the popped element is the last one (`next == TAIL`), the empty
    /// sentinel keeps the observed running tag rather than resetting to zero.
    /// On a lost CAS whose `actual` head is already empty, retry backoff is
    /// skipped because the next iteration returns `None` (see the configured
    /// backoff cap).
    ///
    /// `load_next` is reached through this implementor's binding, whose head is
    /// read once for the whole CAS loop; see [`StackStorage`].
    ///
    /// # Panics
    ///
    /// Panics if `load_next` returns neither [`TAIL`] nor an index below
    /// `INDEX_MASK`, or returns the popped index itself. The release-active
    /// guard prevents `pack_truncating` from turning an invalid value into a
    /// wrong live index or the empty sentinel. A self-loop indicates a caller
    /// contract violation; the guard is a detector, not a repair. Dedicated
    /// link storage is required because a stale popper may read a link after
    /// another thread has popped the index, and payload-aliasing could produce
    /// an arbitrary in-range value that passes this guard silently.
    ///
    /// The implementor's `load_next` may also panic on its own narrower domain
    /// bound; see [`StackOps::push_index`]'s `# Panics`.
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
/// All three hooks are `unsafe fn`: the bridge forwards them verbatim, and
/// the algorithm supplies the caller-side proofs at its call sites.
// Unsafe: internal hooks may rely on caller-proved link-domain invariants.
#[allow(unsafe_code)]
pub(crate) trait SealedStorage<const B: u32> {
    /// # Safety
    ///
    /// Same contract as [`StackStorage::head`].
    unsafe fn head(&self) -> &StackHead<B>;
    /// # Safety
    ///
    /// Same contract as [`StackStorage::load_next`].
    unsafe fn load_next(&self, index: u32) -> u32;
    /// # Safety
    ///
    /// Same contract as [`StackStorage::store_next`]'s `# Safety` — see
    /// there (one normative location; this crate cross-references it).
    unsafe fn store_next(&self, index: u32, next: u32);
}

/// Bridge every public [`StackStorage`] implementor to the internal
/// [`SealedStorage`] algorithm. Calls are qualified because both traits
/// declare methods with the same shapes.
// Unsafe: forwards the public storage hooks without changing their contracts.
#[allow(unsafe_code)]
impl<const B: u32, S: StackStorage<B> + ?Sized> SealedStorage<B> for S {
    unsafe fn head(&self) -> &StackHead<B> {
        // SAFETY: `S: StackStorage<B>` means an `unsafe impl` asserted the
        // implementor contract for this binding. The stack algorithm calls
        // `head()` exactly once per operation and uses the returned
        // reference only as THIS binding's head — never building a second,
        // competing binding around it — discharging
        // [`StackStorage::head`]'s caller-side contract.
        unsafe { StackStorage::head(self) }
    }
    unsafe fn load_next(&self, index: u32) -> u32 {
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
// Unsafe: the caller supplies exclusive publish authority for `index`.
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
    // SAFETY: this operation uses one stable binding's head exactly once;
    // the caller forwarded `StackStorage::head`'s binding contract.
    let head_ref: &StackHead<B> = unsafe { s.head() };
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
        // Seal check: refuse rather than wrap the tag to 0 (a wrapped tag
        // would re-issue a head word a parked popper may still hold as its
        // CAS expectation) — BEFORE any side effect, so a first-attempt
        // refusal touches nothing; see `StackOps::push_index`'s `# Errors`.
        if tag == TaggedIndex::<B>::TAG_MAX {
            return Err(TagExhausted);
        }
        // The link this index chains to: the current head's index, or TAIL
        // if the stack is empty. On an empty head the observed index half
        // IS the sentinel `INDEX_MASK` (which `_CHECK_BITS` keeps <= 0xFFFF
        // at every legal width), NOT `TAIL` — so the branch mapping it to
        // `TAIL` is semantically REQUIRED, not a readability choice:
        // without it an empty-head push would store `INDEX_MASK` as the
        // link, and the next pop would panic on it at the clause-4 guard
        // below (neither TAIL nor a valid index).
        let next_link = if cur_idx == TaggedIndex::<B>::empty_index() {
            TAIL
        } else {
            cur_idx
        };
        // Write the link under Release so a concurrent pop's Acquire read of
        // this slot's link (after observing it as head) sees it. This is the
        // ONLY link write — never an eager init — and it may run
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
        // SAFETY: `next_link` is `TAIL` or the observed head index, and the
        // publishing CAS follows this store. The caller forwarded the
        // link-domain, liveness, and exclusive-ownership proof; the retry
        // overwrite argument above covers stale writes.
        unsafe {
            s.store_next(index, next_link);
        }
        // Plain `+`: the seal check above ran before this bump, so
        // tag < TAG_MAX and the addition cannot overflow.
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
        // Strong, not weak: codegen-identical on every measured lowering —
        // see `TIS_LINK_ORDERING_WEAK_CAS_GATE.md`, "Codegen matrix and observations".
        match head_ref.compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return Ok(()),
            Err(actual) => {
                note_push_retry();
                head = actual;
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
// Unsafe: the head observation proves the link hook's read precondition.
#[allow(unsafe_code)]
#[track_caller]
pub(crate) fn pop_index_impl<const B: u32, S: SealedStorage<B> + ?Sized>(s: &S) -> Option<u32> {
    // `head()` is read exactly once per operation — see StackStorage's
    // "Mechanical requirement on `head()`".
    // SAFETY: this operation uses one stable binding's head exactly once;
    // the caller forwarded `StackStorage::head`'s binding contract.
    let head_ref: &StackHead<B> = unsafe { s.head() };
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
        // SAFETY: `index` came from this binding's Acquire head observation;
        // its link was initialized by the publishing push, even if this
        // operation later loses its CAS.
        let next = unsafe { s.load_next(index) };
        // Unconditional guard (release-active, mirroring push's
        // `index < INDEX_MASK` check) for clause 4 of the StackStorage
        // implementor contract: pack_truncating() below would silently
        // truncate a bad value to a wrong (possibly still-live) index or
        // to the empty sentinel. The guard is not separately measured.
        // See `# Panics` above.
        // The self-loop and truncation meanings are defined in `# Panics`
        // above; the shared-storage catch boundary is in `StackStorage`.
        let mask = TaggedIndex::<B>::INDEX_MASK;
        if next != TAIL && (u64::from(next) >= mask || next == index) {
            pop_link_out_of_range(index, next, mask);
        }
        let new_head = if next == TAIL {
            // Preserve the running tag across the empty transition.
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
        // `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md`'s
        // "Codegen matrix and observations" section).
        match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => return Some(index),
            Err(actual) => {
                note_pop_retry();
                head = actual;
                // Skipped when the lost CAS reveals the stack just went
                // empty: the top-of-loop `is_empty` check returns `None`
                // next iteration regardless, so spinning here is pure
                // wasted latency; which outcome a call eventually returns
                // is unchanged, only how fast it gets there.
                if !TaggedIndex::<B>::is_empty(actual) {
                    backoff.spin();
                }
            }
        }
    }
}

// Unsafe: exposes and forwards `push_index`'s caller-side contract.
#[allow(unsafe_code)]
impl<const B: u32, S: StackStorage<B> + ?Sized> StackOps<B> for S {
    #[track_caller]
    unsafe fn push_index(&self, index: u32) -> Result<(), TagExhausted> {
        // SAFETY: this fn's own caller-side contract (link domain +
        // liveness + exclusive ownership, `push_index`'s `# Safety` above)
        // is forwarded verbatim
        // to `push_index_impl`'s identical `# Safety` contract — not
        // discharged locally, just passed through.
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

/// Cold panic path for [`ArrayLinks`] index bounds, with a crate-owned
/// diagnostic shared by its load and store methods.
#[cold]
#[inline(never)]
#[track_caller]
fn array_links_out_of_range(index: u32, capacity: usize) -> ! {
    panic!("ArrayLinks index out of bounds: index {index} >= capacity {capacity}");
}

/// Cold panic path for [`StackOps::pop_index`]'s clause-4 guard, split out of
/// `pop_index` itself — same `#[cold]` + `#[inline(never)]` +
/// `#[track_caller]` shape and caller-location-chaining rationale as
/// [`push_index_out_of_range`] above. Reports which of the three caught
/// shapes the caller's [`load_next`](StackStorage::load_next) answer has
/// — a self-loop or one of the two truncation outcomes an over-wide value
/// would silently produce. See `pop_index`'s `# Panics` for the self-loop
/// causes.
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
/// `tests/compile_fail/array_index_stack_head/`). That fixture pins one
/// instantiation (`<16, 64>`); the
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
/// for standalone callers (`push` is an `unsafe fn`, carrying
/// [`StackOps::push_index`]'s `# Safety` contract); a fresh stack is EMPTY (lazy links) — the
/// caller pushes indices as they become free. Custom implementors with
/// slot-resident links do not use this type: they implement [`StackStorage`]
/// instead and call the [`StackOps`] methods. `N` is constrained at
/// construction to `N <= TaggedIndex::<INDEX_BITS>::INDEX_MASK`; link access
/// still checks `index < N`, so a caller must stay inside the owned domain.
#[derive(Debug)]
pub struct ArrayIndexStack<const INDEX_BITS: u32, const N: usize> {
    head: StackHead<INDEX_BITS>,
    links: ArrayLinks<N>,
}

impl<const B: u32, const N: usize> ArrayIndexStack<B, N> {
    const _CHECK_N: () = assert!(
        N as u64 <= TaggedIndex::<B>::INDEX_MASK,
        "ArrayIndexStack capacity N must be <= INDEX_MASK"
    );

    /// A fresh, EMPTY stack (head = the bootstrap empty sentinel, tag 0; every
    /// link at `0`). Under `--cfg loom` this cannot be `const` (loom's atomics
    /// have no `const` ctor).
    #[cfg(not(loom))]
    #[must_use]
    pub const fn new() -> Self {
        let () = Self::_CHECK_N;
        Self {
            head: StackHead::new(),
            links: ArrayLinks::new(),
        }
    }

    /// A fresh, EMPTY stack (loom build — non-`const`).
    #[cfg(loom)]
    #[must_use]
    pub fn new() -> Self {
        let () = Self::_CHECK_N;
        Self {
            head: StackHead::new(),
            links: ArrayLinks::new(),
        }
    }

    /// Push `index` onto the stack, driving the crate-internal CAS-retry
    /// algorithm (`push_index_impl`) directly. This type deliberately does
    /// NOT implement the public [`StackStorage`] trait (see the type doc), so
    /// it does not go through [`StackOps::push_index`]'s blanket impl — the
    /// identical algorithm body is crate-internal. See
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
        unsafe { push_index_impl::<B, _>(self, index) }
    }

    /// Pop the top index off the stack, or `None` if empty — driving the
    /// crate-internal CAS-retry algorithm (`pop_index_impl`) directly. This
    /// type deliberately does NOT implement the public [`StackStorage`] trait
    /// (see the type doc), so it does not go through
    /// [`StackOps::pop_index`]'s blanket impl — the identical algorithm body
    /// is crate-internal. See [`StackOps::pop_index`]'s doc for the
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
    /// Gated: same test-cfg/loom gate as [`StackHead::raw_head`] —
    /// it does not exist in a default build.
    #[doc(hidden)]
    #[cfg(any(tagged_index_stack_test, loom))]
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
    /// `links_are_lazy` reads a never-pushed index's link): this type does
    /// not implement the public [`StackStorage`] trait, so
    /// [`StackStorage::load_next`] cannot reach its links.
    /// `#[doc(hidden)]` per the crate's established test-only-forwarder
    /// rationale (see [`raw_head`] and [`cas_head_for_test`]): not part of the
    /// stable API. Gated: same test-cfg/loom gate as [`StackHead::raw_head`]
    /// — it does not exist in a default build. Read-only — it exposes no
    /// `&StackHead` and no link write, so it reopens none of the sealed
    /// hazard.
    #[doc(hidden)]
    #[cfg(any(tagged_index_stack_test, loom))]
    pub fn load_next_for_test(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }

    /// **test-only** write-side twin of [`load_next_for_test`] — stores
    /// `next` into `index`'s link cell directly
    /// ([`ArrayLinks::store_next`], `Release`), bypassing the stack
    /// algorithm entirely. Needed for a hand-inlined counterfactual that
    /// reproduces the tag-wrap behaviour the seal forbids, without going
    /// through the real [`push`](Self::push) — see
    /// `tests/loom_aba.rs`'s tiny-tag counterfactual.
    ///
    /// `#[doc(hidden)]` per this crate's established test-only-forwarder
    /// rationale (see [`raw_head`]). Gated: `loom` only — unlike
    /// [`load_next_for_test`], this is a raw link-cell WRITE that bypasses
    /// the stack algorithm entirely; under the repository test cfg it is a
    /// safe `pub fn` reachable by any consumer, letting safe code construct a
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
    #[cfg(any(tagged_index_stack_test, loom))]
    #[must_use]
    pub fn with_tag_for_test(tag: u64) -> Self {
        let () = Self::_CHECK_N;
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

// Unsafe: the owned head and links form one fixed in-domain binding.
#[allow(unsafe_code)]
impl<const B: u32, const N: usize> SealedStorage<B> for ArrayIndexStack<B, N> {
    unsafe fn head(&self) -> &StackHead<B> {
        &self.head
    }
    unsafe fn load_next(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }
    unsafe fn store_next(&self, index: u32, next: u32) {
        self.links.store_next(index, next)
    }
}

/// An owned `[AtomicU32; N]` link backing (used inside the fused
/// [`ArrayIndexStack`]; slot-resident implementors host their own links
/// instead). Every link starts at `0` — matching OS-zeroed backing — and is
/// only ever written by a push (no eager free-list chaining).
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
    /// only become meaningful once their index is pushed. Under
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
    /// Panics if `index >= N`; the owned stack also checks this backing bound
    /// after its head-word range check.
    #[must_use]
    #[track_caller]
    pub fn load_next(&self, index: u32) -> u32 {
        if index as usize >= N {
            array_links_out_of_range(index, N);
        }
        self.next[index as usize].load(Ordering::Acquire)
    }

    /// Store the "next" link for `index` with `Release` ordering. This is the
    /// ONLY write the stack makes to link storage, and only during a push — the
    /// lazy-link discipline: link storage is never eagerly initialised.
    /// Like [`load_next`](Self::load_next)'s `Acquire`, this `Release` is
    /// deliberate defence-in-depth, not a stack-proof requirement — see
    /// [`StackStorage`]'s "Ordering contract".
    ///
    /// # Panics
    ///
    /// Panics if `index >= N` — the same bound as
    /// [`load_next`](Self::load_next).
    #[track_caller]
    pub fn store_next(&self, index: u32, next: u32) {
        if index as usize >= N {
            array_links_out_of_range(index, N);
        }
        self.next[index as usize].store(next, Ordering::Release);
    }
}

impl<const N: usize> Default for ArrayLinks<N> {
    fn default() -> Self {
        Self::new()
    }
}
