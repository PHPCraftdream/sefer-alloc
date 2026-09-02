//! The tagged-index-stack implementation, gated as ONE unit by the crate
//! root's valid-configuration `#[cfg]`.
//!
//! `compile_error!` does not stop name-resolution of sibling items, so invalid
//! configurations must fail with ONLY the named error: `lib.rs` cfgs this
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
/// [`core::hint::spin_loop`] before retrying — the 1st lost CAS (K = 0) spins
/// once, the 2nd (K = 1) spins twice, and so on up to the cap (the 7th and
/// every later lost CAS spin 64 times). The cap is ENFORCED by
/// [`Backoff`]`::spin`, which refuses to increment `K` past it, saturating
/// `K` — it can never exceed the cap. `K` is a per-call local owned by a
/// [`Backoff`], reset on every fresh
/// `push_index`/`pop_index` — this backs off within one call's retry loop,
/// never across calls. Not unconditional: `pop_index` skips the backoff when
/// the lost CAS reveals the stack just went empty (documented at
/// [`pop_index`](StackOps::pop_index)).
///
/// **The cap is 6 — a deliberate fairness-vs-throughput compromise, not a
/// low-contention optimum.** Caps 8/10 give more aggregate throughput under
/// contention but measurably worse per-thread fairness under oversubscription,
/// while caps 0/4 are fairer but slower. A starved thread here means a starved
/// allocator-slot recycler, so the shipped default does not impose that trade;
/// a caller wanting peak aggregate throughput can measure a higher local cap
/// using the report's reproduction recipe. Measurements, derivation, and the
/// full fairness/throughput tables are in
/// `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (a repository file, not part of
/// the published package).
///
/// Lock-free is not starvation-free, and the backoff trades a small number of
/// very large outliers for better latency through p99.9 and better
/// throughput — see the crate-root doc's "Lock-freedom and starvation"
/// section for the measured trade (this `const` is private, so rustdoc never
/// renders this copy; the crate-root section is the rendered surface).
const BACKOFF_SPIN_CAP: u32 = 6;

// `1u32 << K` masks/panics if `BACKOFF_SPIN_CAP` ever reaches 32 — the same technique [`TaggedIndex::_CHECK_BITS`] uses to
// turn a would-be shift-overflow into a compile error instead of a debug
// panic / silently masked shift in release.
const _: () = assert!(BACKOFF_SPIN_CAP < 32);

/// Per-call exponential-backoff state for the CAS-retry arms: wraps the retry
/// counter (`K`, starting at 0) that drives the spin-loop depth below.
/// Starts fresh every call, never persisted.
struct Backoff(u32);

impl Backoff {
    fn new() -> Self {
        Backoff(0)
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
    /// Returns whether THIS retry spun at FULL depth (the PRE-increment
    /// `K` was already at the cap) — the oracle trigger for
    /// `PUSH_BACKOFF_CAP_REACH_COUNT` / `POP_BACKOFF_CAP_REACH_COUNT`.
    /// The check deliberately happens BEFORE the increment, so the oracle
    /// does not fire one retry early.
    ///
    /// `#[inline]`: see [`Self::at_cap`] — same monomorphization/codegen
    /// reasoning.
    ///
    /// Capped, not unconditional: every increment past BACKOFF_SPIN_CAP is
    /// already dead (`K` is only ever consumed through the shift here), so
    /// letting `K` keep climbing forever would just be an eventual
    /// `attempt to add with overflow` panic under overflow-checks after
    /// ~2^32 consecutive lost CASes in one call — remote, but free to
    /// close. That saturation also guarantees `self.0 <= BACKOFF_SPIN_CAP`
    /// at the shift, so no `.min` guard is needed on the shift expression.
    #[inline]
    fn spin(&mut self) -> bool {
        let at_cap = self.at_cap();
        for _ in 0..(1u32 << self.0) {
            core::hint::spin_loop();
        }
        if !at_cap {
            self.0 += 1;
        }
        at_cap
    }
}

/// A packed `(index | tag)` word with a compile-time-chosen index width.
///
/// The low `INDEX_BITS` bits carry a slot index; the high `64 - INDEX_BITS`
/// bits carry a wrapping generation ABA tag. The all-ones index value
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
    /// Compile-time guard: `INDEX_BITS` must be in `1..=16` so both halves are
    /// non-empty, the shifts are well-defined, every valid index fits the
    /// `u32` that the whole index-carrying surface takes ([`push_index`](
    /// StackOps::push_index), [`pack`](Self::pack)'s parameter,
    /// [`unpack`](Self::unpack)'s index half, [`empty_index`](Self::empty_index)
    /// — all `u32`), so this width cap is what makes every valid index fit —
    /// by construction, with no casts,
    /// AND the tag half keeps a minimum of 48 bits.
    ///
    /// Widths above 16 are rejected rather than merely discouraged: the 16 cap
    /// guarantees every legal configuration at least a 48-bit ABA tag — the
    /// wrap-time floor below which a tag wrap comes within reach of an
    /// ordinary long suspension (see the crate docs' "Tag-width budget"
    /// section). The `u32` bound is respected a fortiori: `INDEX_BITS > 32`
    /// could never buy reachable index range and would only shrink the tag
    /// budget; worse, it would make `INDEX_MASK` exceed `u32::MAX`, letting
    /// `index == u32::MAX` (the internal [`TAIL`] sentinel) silently pass the
    /// `< INDEX_MASK` runtime guard and corrupt a chain. At every legal width
    /// `INDEX_MASK <= 0xFFFF`, so the historical `INDEX_MASK == TAIL`
    /// coincidence at width 32 is structurally impossible, and `index ==
    /// TAIL` can never silently pass the runtime guard. Capping at compile
    /// time closes that bug class structurally instead of requiring every
    /// caller to separately exclude `TAIL` at runtime.
    ///
    /// This `const` is forced to evaluate from EVERY associated item of
    /// `TaggedIndex<INDEX_BITS>`: [`pack`](Self::pack) forces it directly
    /// with a `let () = Self::_CHECK_BITS;` statement, `INDEX_MASK` and
    /// [`TAG_BITS`](Self::TAG_BITS) evaluate it in their own initializers, and
    /// [`unpack`](Self::unpack), [`empty_index`](Self::empty_index),
    /// [`is_empty`](Self::is_empty), [`empty`](Self::empty), and the
    /// crate-private `pack_truncating` all route through `INDEX_MASK` — so an
    /// out-of-range `INDEX_BITS` cannot reach any associated item without
    /// tripping this guard.
    const _CHECK_BITS: () = assert!(
        INDEX_BITS >= 1 && INDEX_BITS <= 16,
        "INDEX_BITS must be in 1..=16: the tag half must keep at least 48 bits \
         (the cache-line-throughput-derived floor against ABA tag wrap — see \
         the crate docs' \"Tag-width budget\" section), both halves must be \
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

    /// Number of bits carrying the tag (`64 - INDEX_BITS`). The tag wraps at
    /// `2^TAG_BITS`.
    pub const TAG_BITS: u32 = {
        // Force the compile-time bounds check to be evaluated.
        let () = Self::_CHECK_BITS;
        64 - INDEX_BITS
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
    /// `< 2^INDEX_BITS` is THIS function's acceptance boundary, while
    /// [`push_index`](StackOps::push_index)'s `< INDEX_MASK`
    /// (`INDEX_MASK == 2^INDEX_BITS - 1`) is stricter because it also
    /// excludes the reserved empty sentinel. Packing the empty index with a
    /// tag IS accepted here — that is the legitimate H-2 shape
    /// ([`empty_index`](Self::empty_index)).
    ///
    /// `push_index`/`pop_index` do NOT call this function on the hot path:
    /// their inputs are already guaranteed within their halves by the
    /// crate's own guards, AND `push_index`'s tag bump legitimately produces
    /// `tag == 2^TAG_BITS` at the ABA wrap boundary (the value whose
    /// shifted-out high bit restarts the tag at 0), which this checked
    /// function must reject. They pack through the crate-private truncating
    /// fast path `pack_truncating` instead, which is where the silent
    /// truncation semantics — and their sharp edges — now live.
    #[must_use]
    pub const fn pack(index: u32, tag: u64) -> Option<u64> {
        // Force the compile-time bounds check to be evaluated HERE, not
        // only via `TAG_BITS` in one branch: a const evaluation taking the
        // short-circuited branch would otherwise skip both, weakening the
        // documented _CHECK_BITS-from-every-public-item invariant into a
        // branch-dependent one.
        let () = Self::_CHECK_BITS;
        if index >= (1u32 << INDEX_BITS) || tag >= (1u64 << Self::TAG_BITS) {
            None
        } else {
            Some((tag << INDEX_BITS) | (index as u64))
        }
    }

    /// Truncating fast path: `(tag << INDEX_BITS) | (index &
    /// INDEX_MASK)`, dropping every index bit at or above `2^INDEX_BITS`
    /// and every tag bit at or above `2^TAG_BITS`. TRUSTS ITS PRECONDITION —
    /// the name is the contract: this silently produces a VALID-LOOKING
    /// word from invalid input. An over-wide index (in
    /// `2^INDEX_BITS..=u32::MAX`) masks to a DIFFERENT
    /// (possibly still-live) index, or to the
    /// [empty sentinel](Self::empty_index) if the low bits are all ones; an
    /// over-wide tag loses its high bits. If you cannot prove your halves
    /// are in range, use [`pack`](Self::pack), which rejects instead.
    ///
    /// Crate-private precisely so the sharp edges stay in-crate: the only
    /// callers are [`push_index`](StackOps::push_index),
    /// [`pop_index`](StackOps::pop_index), and [`empty`](Self::empty)
    /// (whose arguments are the compile-time constants `INDEX_MASK, 0`,
    /// in range by construction); the two hot-path callers' inputs are
    /// guaranteed in range by their own guards (`push_index`'s `index <
    /// INDEX_MASK` panic guard, `pop_index`'s rule-4 guard on the link it
    /// read) — with
    /// ONE deliberate exception: `push_index` hands this helper
    /// `tag.wrapping_add(1)`, which at the ABA wrap boundary is exactly
    /// `2^TAG_BITS`, whose shifted-out high bit RESTARTS THE TAG AT 0
    /// here. That truncation is the wrap mechanism, relied-upon behaviour
    /// rather than an oversight (the checked [`pack`](Self::pack) rejects
    /// that value; the hot path must wrap it instead).
    #[must_use]
    pub(crate) const fn pack_truncating(index: u32, tag: u64) -> u64 {
        // Force the compile-time bounds check to be evaluated.
        let () = Self::_CHECK_BITS;
        (tag << INDEX_BITS) | ((index as u64) & Self::INDEX_MASK)
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
    /// `#[doc(hidden)]`: see [`raw_head`](StackHead::raw_head)'s
    /// rationale. This item also has one real in-workspace consumer outside
    /// this crate — `sefer-alloc`'s `#[cfg(loom)]` `bootstrap::loom_shim`
    /// (its mirrored const-capable `StackHead::new`; a loom-test-only shim
    /// that exists to keep a const static compiling under loom, never a
    /// production code path) — so it is not freely removable in a future
    /// 0.2 without checking that caller first.
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

/// The head word of a tagged Treiber free-list: a single `AtomicU64` packing an
/// `(index | tag)` pair (see [`TaggedIndex`]). Owned by exactly ONE
/// [`StackStorage`] implementor VALUE at a time, and bound to ONE link
/// backing for its WHOLE life — the binding between this head and its links
/// is established by that impl, not re-asserted per call; sharing one head
/// between implementor values (rule 1) or rebinding a live head across time
/// (inventory shape 4) are hazards — see the
/// [`StackStorage`] trait doc's "The shared-storage hazard class" section
/// for the full inventory. The stack operations themselves live
/// on [`StackOps`] (blanket-implemented by the crate), not here; this type
/// is the bare atomic embedders inherit a cache line through.
///
/// # Layout note — no cache-line isolation
///
/// This type is a bare `AtomicU64` with no padding or alignment of its own: it
/// inherits the cache line of whatever struct embeds it. If it lands adjacent
/// to another frequently-modified atomic, the two fields false-share — each
/// write invalidates the other core's copy of the line, and contending cores
/// ping-pong the line even though the atomics are logically independent. That
/// costs throughput, never correctness, and only matters when the line is
/// genuinely hot. Fix it at the embedding site when a profile shows it — wrap
/// this stack in a `#[repr(align(64))]` newtype or interpose padding — rather
/// than paying for blanket alignment inside the crate, which would waste most
/// of a cache line for every embedder that does not need the isolation.
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

    /// The raw packed head word (`Acquire`) — for this crate's own diagnostics
    /// and tests only. The index half is a live top-of-stack index or
    /// [`empty_index`](TaggedIndex::empty_index); the high bits are the running
    /// tag. `Acquire` so a loom test that splits a pop's read from its CAS (to
    /// open the ABA window) still forms the same happens-before edge the real
    /// `pop_index`'s `Acquire` head load does.
    ///
    /// `#[doc(hidden)]` (this project's established test-only-forwarder
    /// convention — every other `#[doc(hidden)]` item in this crate points
    /// here for the generic rationale): this is a `pub` item solely so
    /// `tests/` — an external crate from this crate's own perspective — can
    /// reach it. The attribute hides it from rustdoc's rendered navigation
    /// ONLY; it stays a fully callable `pub` item from any downstream crate,
    /// nothing in the language or this crate enforces non-callability. It is
    /// not exercised by any production caller and carries no semver
    /// stability guarantee.
    #[doc(hidden)]
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
    /// rationale. This item is additionally `#[cfg(loom)]`-gated, so unlike
    /// `raw_head` it does not exist at all outside a `--cfg loom` build.
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

/// The crate-private-constructible witness required by the three
/// [`StackStorage`] implementor hooks ([`StackStorage::head`],
/// [`StackStorage::load_next`], [`StackStorage::store_next`]).
///
/// The field is private, so no code outside this crate can construct a
/// `Hook` value by ANY spelling — tuple-struct construction (`Hook(())`)
/// and struct-literal construction (`Hook { 0: () }`) are both compile
/// errors — and the hooks take `&Hook` (a reference, not an owned value),
/// so an implementor cannot stash a token and re-expose it through its own
/// safe methods. The hooks are therefore unreachable from outside this
/// crate regardless of what is in scope; only this crate's own stack
/// algorithm, driving the hooks through its `pub(crate)` internal bridge,
/// can call them.
///
/// Pinned by the compile-fail fixture
/// `tests/compile_fail/hook_token_unconstructible/` (repository test
/// infrastructure, not part of the published package).
pub struct Hook(());

/// The single [`Hook`] witness value this crate drives all
/// [`StackStorage`] hooks with (crate-private; each use of `&HOOK` is
/// a promoted `'static` reference).
const HOOK: Hook = Hook(());

/// ONE implementor supplies BOTH the stack head and the per-index link access —
/// the head↔links binding is established ONCE per impl instead of being
/// re-asserted on every [`push_index`](StackOps::push_index)/
/// [`pop_index`](StackOps::pop_index) call. The stack stores the head word; each
/// pushed index's next pointer (another index, or [`TAIL`]) lives in the
/// implementor's storage — slot-resident in implementor-owned storage (the
/// production shape) or in an owned fused object ([`ArrayIndexStack`]).
///
/// The stack's own CAS loops never block, but end-to-end lock-freedom of
/// [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
/// additionally requires THIS trait's implementation to be non-blocking:
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
/// An implementor MUST uphold ALL of the following:
///
/// 1. **One live binding per head, for the head's whole life.** The
///    [`StackHead`] returned by [`head`](Self::head) must be bound to
///    exactly ONE live implementor value for as long as any index is
///    reachable through it: never shared with another live implementor
///    value, and never rebound to different link storage across time (even
///    with never more than one live value at any instant).
/// 2. **One backing, consistently.** [`load_next`](Self::load_next) and
///    [`store_next`](Self::store_next) must read and write the SAME link
///    storage through a stable one-to-one index↔cell mapping, and a
///    `load_next` must observe the most recent `store_next` the stack
///    itself performed (the `# Ordering contract` below discharges the
///    ordering half for a single, stable implementor).
/// 3. **Disjoint reachable-index populations across shared cells.** No
///    index reachable from two live head↔links bindings whose hooks touch
///    the same link cells (cell sharing per se is harmless with disjoint
///    populations; the hazard is a reachable index).
/// 4. **Valid answers, dedicated cells.** [`load_next`](Self::load_next)
///    must return only [`TAIL`] or a currently-valid index, from a link
///    cell DEDICATED to this purpose, never payload-aliased.
/// 5. **Same logical head every call** — see "Mechanical requirement on
///    `head()`" below.
///
/// The numbered rules and "The shared-storage hazard class" section below
/// remain the explanatory appendix to THIS contract, not a replacement
/// for it.
///
/// # Ordering contract
///
/// Implementations MUST use `Acquire` on [`load_next`](Self::load_next) and
/// `Release` on [`store_next`](Self::store_next). The load-bearing
/// `Acquire` for the stack's own proof is the head observation itself — the
/// initial `Acquire` load of the head, or (on a retry) the PREVIOUS
/// iteration's `Acquire`-ordered CAS-failure read — which happens BEFORE the
/// [`load_next`](Self::load_next) call: each
/// [`store_next`](Self::store_next) is sequenced-before the pushing
/// thread's `Release` CAS on the head, and a release publishes ALL of its
/// thread's prior writes, whatever tags those writes carry themselves. So a
/// pop that observes a slot as the head sees the link a pusher wrote before
/// publishing that slot as head EVEN IF the link accesses themselves were
/// `Relaxed`; the CAS is attempted only AFTER
/// [`load_next`](Self::load_next) has run, so its success ordering plays no
/// part in making that link visible.
///
/// The full link-level `Acquire`/`Release` pairing is therefore mandated as
/// deliberate change-resilience, retained at a real but unmeasured cost on
/// weakly-ordered targets; relaxing to `Relaxed` was considered and deferred
/// pending a multi-target A/B measurement. This keeps a [`StackStorage`]
/// implementation correct on its own terms rather than coupled to the
/// stack's internal head orderings — an implementation detail that could
/// change. On weakly-ordered targets, where `Acquire`/`Release` cost real
/// instructions, read this as considered defence-in-depth, not naivety.
///
/// This ordering contract speaks to ONE head-and-backing pair used
/// consistently — it is what makes a [`load_next`](Self::load_next) observe
/// the [`store_next`](Self::store_next) the stack performed, *given* that
/// every call reaches the same implementor. Under this API that "given" is
/// structural only at the type level and a live obligation at the value
/// level: see "The binding: what is structural, what is not" below.
///
/// # The three hooks are witness-gated — unreachable from outside this crate
///
/// [`head`](Self::head), [`load_next`](Self::load_next), and
/// [`store_next`](Self::store_next) are the STORAGE IMPLEMENTOR's hooks —
/// the three surfaces this crate's own `pub(crate)` internal bridge
/// (a [`StackOps`] blanket impl) drives — and each takes a first
/// `&Hook` witness parameter. [`Hook`]'s field is private, so NO code
/// outside this crate can construct a witness by ANY spelling:
/// tuple-struct construction is E0423, the struct-literal spelling
/// (`Hook { 0: () }`) is E0451, and omitting the argument is E0061 — no
/// external caller can invoke the hooks regardless of what is in scope.
/// The witness is a REFERENCE (`&Hook`), not an owned value, and that is
/// load-bearing: an owned non-`Copy` token could be stashed by a
/// cooperating implementor into a `Cell<Option<Hook>>` and re-exposed
/// through that implementor's own safe method, silently reopening the
/// route; the reference form makes such stashing a lifetime error. This
/// closes the caller-side forgery route the pre-witness trait doc had to
/// admit in prose (the audit that found it is a repository file:
/// `docs/reviews/2026-09-01-tagged-index-stack-full-audit-run5-fxx.md`,
/// finding P2-1). The closure is pinned by the compile-fail fixture
/// `tests/compile_fail/hook_token_unconstructible/` (both the
/// omitted-witness and forge-the-witness spellings fail; asserted by
/// `tests/compile_fail.rs`). Callers drive a
/// stack ONLY through
/// [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
/// (or [`ArrayIndexStack`]'s inherent `push`/`pop`); the three hooks
/// belong inside the implementor's `impl` block, and within it only this
/// crate's stack algorithm ever supplies the witness.
///
/// Post-closure the trait's [`head`](Self::head) is witness-gated like the
/// other two hooks and is NOT a route to a `&StackHead` from outside this
/// crate. What remains is the IMPLEMENTOR-side half: an implementor's head
/// field is its own storage, and if the implementor exposes it through its
/// OWN inherent API, rule 1's obligation (one live implementor value per
/// head) is what covers that — unchanged by this closure. Against the
/// crate's shipped standalone type ([`ArrayIndexStack`]) the route is
/// CLOSED — the type does not implement this trait, its `head` field is
/// private, and no trait impl hands out its head (compile-fail pinned by
/// `tests/compile_fail/array_index_stack_head/`) — see "The shared-storage
/// hazard class" below.
///
/// # The binding: what is structural, what is not
///
/// The caller-side obligation the old per-call-`&L` API could only document
/// (same logical backing on every call) is now an OBLIGATION OF THE
/// IMPLEMENTOR — but it is only PARTLY discharged by the compiler:
///
/// 1. **One backing and ONE live implementor value for the whole life of a
///    non-empty stack.** A [`StackStorage`] implementor IS the backing: the
///    [`head`](Self::head) it returns and the cells its
///    `load_next`/`store_next` touch are bound through ONE `impl` block on
///    ONE object. What the compiler enforces is the death of the old
///    per-call shape: a caller cannot hand a second, different backing
///    ARGUMENT to
///    [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
///    on a later call (the per-call-`&L` repro does not compile — pinned by
///    `tests/compile_fail.rs`). What NOTHING enforces — not
///    the type system, not the [`StackOps`] blanket impl, not rule 4's
///    release-active guard, and not even the `unsafe impl` acknowledgment
///    (which forces every implementor to ASSERT the contract but detects no
///    violation) — is the instance-level half: a given
///    [`StackHead`] value must be reachable through exactly ONE live
///    implementor VALUE at a time. Two SEPARATE, individually coherent
///    implementor values can each return a reference to the SAME
///    [`StackHead`] while reading and writing DIFFERENT link storage —
///    that still compiles (behind an `unsafe impl` asserting the contract
///    it violates), and it reproduces this crate's original
///    release-blocking double-issue hazard. No rule's wording catches it:
///    every rule here is stated per implementor, and each of the two
///    values satisfies the rules on its own terms; rule 4's runtime guard
///    catches only one shape of the class — "The shared-storage hazard
///    class" section below holds the exact catch/miss boundary. The
///    shape, abbreviated
///    (a runnable, assert-pinned version lives at
///    `two_implementor_values_sharing_one_head_still_double_issue` in
///    `tests/custom_storage_impl.rs`):
///
///    ```text
///    struct View<'a> { head: &'a StackHead<16>, links: &'a ArrayLinks<64> }
///    // ONE StackStorage<16> impl for View: head() -> self.head,
///    // load_next(i) -> self.links.load_next(i), store_next(i, n) analogous.
///    let head = StackHead::<16>::new();
///    let (a, b) = (ArrayLinks::<64>::new(), ArrayLinks::<64>::new());
///    let va = View { head: &head, links: &a };
///    let vb = View { head: &head, links: &b }; // SAME head, DIFFERENT links
///    va.push_index(1);
///    // pop via vb: Some(1), then — via pop_index's self-loop detector —
///    // a PANIC on the second pop (vb's zero-initialised links answer
///    // load_next(0) with 0 while the popped index IS 0: a self-loop).
///    // Before that detector the pop returned Some(0) forever; a
///    // HAND-CRAFTED acyclic backing (links[1] = 0, links[0] = TAIL)
///    // still returns Some(0) silently.
///    ```
///
///    The binding is therefore structural only at the type level (one impl
///    establishes the head↔links pairing for a type) and a LIVE
///    implementor/caller obligation at the value level. One value per head
///    is not enforceable by reading any single impl block — the hazard is
///    two implementor VALUES of possibly the SAME impl sharing one head,
///    not multiple impl blocks existing, and the values are individually
///    correct, so no per-impl audit can see the combination. Discharge it
///    by construction — one implementor value per head, for the head's
///    WHOLE life (one-live-value-at-a-time is not enough: shape 4 below
///    rebinds a live head across time while every moment still has
///    exactly one live value). The crate's own fused type,
///    [`ArrayIndexStack`], no longer implements this trait AT ALL, so a
///    competing binding against a standalone [`ArrayIndexStack`] is
///    UNEXPRESSIBLE (compile-fail pinned by
///    `tests/compile_fail/array_index_stack_head/`, the compile-fail
///    successor of the former
///    `array_index_stack_head_still_double_issue` runtime demonstration);
///    for every trait IMPLEMENTOR the discharge remains by construction —
///    the witness gates the crate's hooks, NOT the implementor's own
///    storage: an implementor that exposes its own head through its own
///    inherent API can still have the shape rebuilt against it, so
///    one-value-per-head stays a convention the implementor upholds
///    (asserted formally by every `unsafe impl` — see the `# Safety`
///    section above). An implementor whose
///    `load_next`/`store_next` internally touch different storage still
///    compiles too, and violates rules 3 and 4 below — live implementor
///    obligations, not structural impossibilities.
///
///    **Completeness of rule 1's coverage.** Rule 1's obligation — "any
///    implementor value that can produce a [`StackHead`] reference
///    matching another implementor's" —
///    covers the shared-head hazard EXHAUSTIVELY. There are exactly TWO
///    routes to a `&StackHead<INDEX_BITS>` from safe code as of this
///    writing: (1) own a [`StackHead`] value directly —
///    [`new`](StackHead::new) / `Default::default()` are the only public
///    constructors — and hand `&` it to multiple implementor values;
///    (2) call the trait's [`head`](Self::head) hook — now witness-gated
///    and unreachable from OUTSIDE this crate (the witness cannot be
///    constructed or obtained there; pinned by
///    `tests/compile_fail/hook_token_unconstructible/`), surviving only
///    (a) inside the crate, where the stack algorithm itself is the sole
///    caller, and (b) through an implementor's OWN storage/API, which is
///    route 1's implementor-side obligation, not a trait surface.
///    Consequently, from outside this crate exactly ONE route to a
///    `&StackHead` remains: owning a [`StackHead`] value directly — which
///    is why rule 1's obligation stays over VALUES, not over types. No
///    third route exists as of this writing: none of the three public
///    structs ([`StackHead`], [`ArrayIndexStack`], [`ArrayLinks`])
///    implements `Clone`, `Copy`, `Deref`, `DerefMut`, `AsRef`, `Borrow`,
///    `Index`, or `From` (the only derive on any of them is `Debug`),
///    every field of all three is private, and the only signatures
///    returning `&StackHead` in the crate — [`head`](Self::head) itself
///    and `ArrayIndexStack`'s own `pub(crate)` accessor — are both
///    unreachable from outside the crate (`head` via the witness, the
///    accessor via `pub(crate)`). This enumeration is falsifiable,
///    not an assertion — re-verify it mechanically before relying on it:
///    list every `impl` block and every signature in this file returning
///    `&StackHead` (grep recipes and the dated 2026-09-01 census are
///    recorded in the repository ADR
///    `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`
///    — a repository file, not part of the published package; note a
///    signature rustfmt wraps so the return type lands on the next line
///    evades a naive signature grep, so read the impl list too). Earlier
///    adversarial reviews each found "a new way" to
///    reach a `&StackHead`; a full enumeration of this trait module's
///    entire public surface found nothing beyond the two routes above. A
///    reviewer who finds a third route has falsified THIS paragraph and
///    rule 1's coverage claim — update both in the same change. (This
///    paragraph WAS so updated in the same change as the witness closure,
///    2026-09-01.)
///
/// 2. **Stable one-to-one index↔cell mapping.** Every valid index must map to
///    the SAME link cell for the implementor's whole lifetime.
/// 3. **Coherence of `store_next`/`load_next`.** A
///    [`load_next`](Self::load_next) of index `i` must observe the most recent
///    [`store_next`](Self::store_next) of `(i, _)` the crate itself performed
///    on this implementor. The Acquire/Release ordering contract above
///    guarantees THIS for a single, stable implementor.
///    The rule also carries a BINDING-level clause, parallel to rule 1's
///    instance-level obligation — and it is a REACHABILITY invariant,
///    not a cell-ownership one: an index must not already be REACHABLE
///    from any head↔links binding that reads and writes the same link
///    cells this implementor's
///    [`load_next`](Self::load_next)/[`store_next`](Self::store_next)
///    touch (the same invariant
///    [`push_index`](StackOps::push_index)'s `# Caller contract` states
///    from the caller's side). Cell sharing PER SE is harmless: two
///    stacks over the same cells with DISJOINT index populations (e.g.
///    `{0, 2, 4, 6}` and `{1, 3, 5, 7}`) coexist correctly, because each
///    `store_next(i, _)`/`load_next(i)` touches only cell `i` — the
///    earlier "the link CELLS themselves must not be shared" framing was
///    FALSE as stated. The hazard fires when
///    one index is reachable from two bindings over the same cells: the
///    second binding's push overwrites a link the first still chains
///    through, one index ends up chained into BOTH stacks, and each
///    stack then hands it out — double-issue with every per-implementor
///    rule individually satisfied and rule 4's guard silent (the shared
///    chain stays acyclic — pinned by
///    `two_stacks_sharing_link_storage_still_double_issue` in
///    `tests/custom_storage_impl.rs`). Discharge it by construction —
///    disjoint index populations per binding over any shared cell
///    population, the same one-binding-per-head discipline rule 1
///    demands for heads.
/// 4. **`load_next` must return only [`TAIL`] or a currently-valid index** for
///    the implementor in use. This rule STAYS a live runtime obligation:
///    an implementation returning an arbitrary, stale, or foreign value
///    corrupts the free-list with no adversarial intent (a zero-initialized
///    [`ArrayLinks`] "coincidentally" returns `0` for every index, and if
///    that equals the live head's own index,
///    [`pop_index`](StackOps::pop_index)'s compare-exchange
///    `current -> current` succeeds trivially). That self-loop sub-shape
///    is one of the two value shapes this rule's release-active guard
///    catches — detection of shapes, not structural prevention. (The
///    same self-loop arm also fires for caller-contract violations
///    outside this implementor contract entirely — cross-referenced
///    with "Detection coverage" below; the full two-cause disjunction
///    of what a self-loop proves is kept in ONE place,
///    [`pop_index`](StackOps::pop_index)'s `# Panics`.) Every other
///    rule-4-violating value that is in range and not the popped index
///    itself still passes silently (see
///    [`pop_index`](StackOps::pop_index)'s `# Panics` for the guard
///    itself, and "The shared-storage hazard class" below for the
///    class-wide catch/miss boundary). An out-of-range return is a
///    second, silent hazard: [`pop_index`](StackOps::pop_index) packs the
///    value it read with its crate-private truncating fast path
///    (`pack_truncating` — the public [`pack`](TaggedIndex::pack) is checked
///    and rejects), which masks an over-wide value to its low `INDEX_BITS`
///    bits. Two corruption modes result: masked to a LIVE index (e.g.
///    `0x1_0000` at `INDEX_BITS = 16` packs as index `0`, which may still be
///    owned elsewhere in the free-list — a double-issue), or masked to the
///    EMPTY sentinel (low bits all ones) so the stack silently reports
///    itself drained and leaks every remaining index in the chain. See
///    [`pop_index`](StackOps::pop_index)'s `# Panics` for the release-active
///    guard enforcing this rule, and "Storage requirement" below for why
///    payload-aliased link storage always violates it.
/// 5. **Backing lifetime.** The implementor and its cells must remain alive
///    and keep their identity for as long as the stack's head can reference
///    them — in practice, for the implementor's own lifetime.
///
/// # The shared-storage hazard class: inventory and detection coverage
///
/// The old API's two-backings-one-head swap trap — two independent calls,
/// each supplying a different backing for the same head — does not compile
/// against this API. That is the API REMOVAL, not a safety invariant: the
/// per-call calling convention itself is gone
/// (`tests/compile_fail.rs` pins exactly that), and the trap's
/// hazard content — one head, two backings — survives as shape 2 below.
/// What does NOT carry over is the REST of the shared-storage hazard class:
/// FOUR surviving shapes, none of them the only gap the others leave. Since
/// the 2026-09-01 `unsafe trait` conversion, none of them is expressible in
/// plain safe code: each requires an `unsafe impl StackStorage` — a
/// compiler-forced acknowledgment, at the impl site, of the very `# Safety`
/// contract the shape then violates. Still expressible, in other words —
/// closed only by the unsafe-impl contract, NOT by the type system.
/// Exactly ONE of them — shape 1 — VIOLATES
/// per-implementor rules (rules 3 and 4 oblige the implementor to prevent
/// it): implementor-enforced, not structurally impossible, and auditable
/// inside one impl block, unlike the binding-level shapes 2, 3, and 4,
/// which ARE reachable with every per-implementor rule individually
/// satisfied — their subject is a head↔links BINDING (how many live
/// bindings exist over a given head or cell population, and across how
/// much time), not the state of any single implementor, so no
/// per-implementor rule can even name them. THIS section is the source of
/// truth for that inventory and
/// for what the runtime currently detects; the crate-root docs, the README,
/// the type and method docs, and the pinning tests point here rather than
/// re-deriving it (the same pattern `tests/loom_aba.rs`'s module doc
/// establishes for the loom per-model breakdown):
///
/// 1. **One implementor, internally disagreeing storage** — a SINGLE
///    implementor whose [`load_next`](Self::load_next)/
///    [`store_next`](Self::store_next) read and write different backings
///    behind one head. Rules 3 and 4 oblige the implementor to prevent it:
///    implementor-enforced, not structurally impossible — and auditable
///    inside one impl block, unlike the three binding-level shapes below.
/// 2. **Shared head, different links** — TWO head↔links bindings whose
///    [`head`](Self::head) methods return the SAME [`StackHead`] VALUE
///    while their links differ (rule 1's instance-level obligation above;
///    the completeness note there shows the reference-sharing route
///    cannot be sealed). In practice the two bindings are two implementor
///    values — coherence allows only ONE `impl StackStorage<B>` per type,
///    so a single value cannot carry two live same-width bindings — but
///    the inventory's unit is the BINDING, not the value, because shapes
///    3 and 4 below are reachable within a single value (shape 3) or
///    across time with never more than one live value (shape 4).
/// 3. **Separate heads, shared link cells** — link STORAGE (the cells
///    `load_next`/`store_next` touch) shared between TWO head↔links
///    bindings with completely SEPARATE heads (rule 3's binding-level
///    clause). NOT "the cells are clobbered on every push": with DISJOINT
///    index populations two stacks over the same cells coexist correctly,
///    because each `store_next(i, _)`/`load_next(i)` touches only cell
///    `i` (`{0, 2, 4, 6}` and `{1, 3, 5, 7}` interleaved over one
///    `ArrayLinks` each drain exactly their own multiset). The hazard
///    fires when one index is REACHABLE from more than one binding over
///    the same cells: the second binding's push overwrites a link the
///    first still chains through, one index ends up chained into BOTH
///    stacks, and the shared chain stays perfectly acyclic. Reachable
///    with the two bindings in two implementor values AND inside ONE value
///    carrying two different-width heads
///    with a `StackStorage` impl at each width over the same backing
///    (push 1, 2 at width 16; push 3, re-push 1 at width 12; the width-16
///    drain yields `2, 1, 3` and the width-12 drain `1, 3` — indices 1
///    and 3 each issued twice, ONE implementor value the whole time;
///    pinned by
///    `one_value_two_bindings_shared_backing_still_double_issue` in
///    `tests/custom_storage_impl.rs`).
///
/// 4. **Temporal rebinding — a live head moved into fresh links** — ONE
///    head↔links binding replaced, mid-life, by a second binding over
///    the SAME [`StackHead`] VALUE with DIFFERENT links: `let grown = Pool { head: old.head, links:
///    ArrayLinks::new() }` where `old` is a rule-abiding implementor
///    with a non-empty stack. The move consumes `old`, so NO reference
///    is ever shared and there is never more than ONE live implementor
///    value — rule 1's instance-level obligation ("exactly ONE live
///    implementor VALUE at a time") holds as stated, its completeness
///    note sees nothing (no `&StackHead` route is involved — the head
///    moves by VALUE, not by reference), and the scoping parenthetical
///    below counts no second live value: only rule 1's HEADLINE — one
///    backing for the whole life of a non-empty stack — covers this
///    shape, in spirit. Effect: the fresh (zero-initialised) backing
///    answers every `load_next` with `0`; the FIRST pop hands back the
///    real head index from a backing that no longer describes the chain
///    (every deeper index silently LEAKED — unreachable, never issued),
///    and the SECOND pop panics through the self-loop detector (pinned
///    by `head_moved_into_fresh_links_leaks_and_then_panics` in
///    `tests/custom_storage_impl.rs`).
///
///    (This inventory counts head↔links BINDINGS, not implementor
///    values: a shape qualifies when a head or a link-cell population
///    reaches two live bindings at once (shapes 2 and 3), or when one
///    live binding is replaced across time by another binding over the
///    same head with different links (shape 4). One expressible shape
///    is still excluded here by construction: ONE implementor whose
///    [`head`](Self::head) returns different heads across calls is not
///    a shared-or-rebound-binding hazard, and it is covered by its own
///    section above, "Mechanical requirement on `head()`".)
///
/// Detection coverage, stated once: [`pop_index`](StackOps::pop_index)'s
/// release-active rule-4 guard (see its `# Panics`, which also holds the
/// full two-cause disjunction of what a self-loop proves — this section
/// covers only the shared-storage/rebinding half of the class) is a
/// VALUE-shape detector — it panics on an out-of-range answer (before its
/// two truncation corruptions) and on a self-loop
/// (`next == index`, unreachable for a contract-abiding chain because a
/// push stores the PREVIOUS head into `next[index]`, which is trivially
/// already reachable). That catches the zero-initialised sub-shapes of
/// shapes 1, 2, and 4 — on the SECOND pop, where the foreign `0` answers
/// coincide with the popped index (pinned by the three `#[should_panic]`
/// tests in `tests/custom_storage_impl.rs`:
/// `internally_disagreeing_storage_still_double_issue` for shape 1, plus
/// `two_implementor_values_sharing_one_head_still_double_issue` for shape 2
/// in its custom-implementor form; shape 2 against the owned standalone
/// type no longer compiles at all, pinned by
/// `tests/compile_fail/array_index_stack_head/`). The same
/// self-loop arm ALSO fires for a caller-contract violation OUTSIDE this
/// hazard class entirely — a plain double-push of the index that is
/// already the current head, this crate's separate, older no-double-push
/// rule ([`push_index`](StackOps::push_index)'s `# Caller contract`;
/// pinned by `double_push_of_current_head_panics_on_first_pop` in
/// `tests/stack_unit.rs`; see `pop_index`'s `# Panics` for the full
/// two-cause disjunction — not restated here). Everything else still
/// corrupts SILENTLY, at least at first: a hand-crafted acyclic link
/// forgery (pinned by `hand_crafted_acyclic_forgery_still_double_issues`),
/// ALL of shape 3 (pinned by
/// `two_stacks_sharing_link_storage_still_double_issue`), and shape 4's
/// FIRST pop — the stale head index is returned from a backing that no
/// longer describes the chain and every deeper index leaks BEFORE the
/// self-loop detector makes the rebinding loud on the second pop, one
/// index too late (pinned by
/// `head_moved_into_fresh_links_leaks_and_then_panics`). Shape 3 has no
/// detector at all, because every link value stays numerically valid and
/// the chain acyclic — documented, not detected. None of the four shapes
/// is a compiler-enforced impossibility; all remain implementor/caller-
/// enforced obligations — since the 2026-09-01 `unsafe trait` conversion
/// each is reachable only behind an `unsafe impl` that asserts the
/// `# Safety` contract the shape violates: the acknowledgment is
/// compiler-forced, the contract still is not checked.
///
/// # Mechanical requirement on `head()`
///
/// Implementations must return the SAME logical head from [`head`](Self::head)
/// for every operation on a given implementor. The crate's [`StackOps`]
/// blanket impl reads `head()` exactly ONCE per operation and holds the
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
/// reasoning. What [`pop_index`](StackOps::pop_index)'s rule-4 guard
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
/// This trait IS an `unsafe trait` (owner-approved decision, 2026-09-01);
/// the earlier decline was a consequence of the then-`#![forbid(unsafe_code)]`
/// policy, which the owner chose to spend (the crate now ships
/// `#![deny(unsafe_code)]` with exactly one audited allow on this
/// declaration — see the crate docs' "Where unsafe lives" section). Why:
/// allocator consumers rely on the exclusive-issuance contract for memory
/// safety (the [`core::alloc::GlobalAlloc`] / `std::alloc::Allocator`
/// category), and marking the trait `unsafe` assigns responsibility for a
/// violation to whichever `unsafe impl` asserted a contract it did not
/// uphold. The methods stay safe `fn` because the crate-private [`Hook`]
/// witness makes them unreachable from outside this crate, so every
/// remaining hazard is IMPLEMENTOR-side — the audit's caller-side forgery
/// (repository file
/// `docs/reviews/2026-09-01-tagged-index-stack-full-audit-run5-fxx.md`,
/// finding P2-1) is what forced the witness into the design, and the
/// storage-binding ADR's 2026-09-01 addendum records the decision; the
/// full design rationale for rejecting the `unsafe fn head()` alternative
/// is recorded in the repository ADRs
/// `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md` and
/// `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md`
/// (repository files, not part of the published package).
///
/// The trait's `# Safety` contract now carries the implementor obligations
/// normatively; what remains caller-side discipline is
/// [`push_index`](StackOps::push_index)'s pre-existing, already-documented
/// no-double-push liveness rule, which this crate has always accepted as
/// caller discipline rather than a structural guarantee.
#[allow(unsafe_code)]
// Tier-2 item-scoped allow — the ONE `unsafe` token in this crate (see the
// crate docs' "Where unsafe lives"). Single documented reason to hold
// `unsafe`: the trait's implementor obligations (the `# Safety` section in
// the doc comment above) are relied on for memory safety by allocator
// consumers and cannot be expressed in the type system, so the trait is
// declared `unsafe` — the same category as `core::alloc::GlobalAlloc`.
// Crate-wide `#![deny(unsafe_code)]` keeps every OTHER `unsafe` token a hard
// error; this allow is confined to this one declaration.
pub unsafe trait StackStorage<const INDEX_BITS: u32> {
    /// The stack's head word. Must return the SAME logical head for every
    /// operation on this implementor — see the trait doc's "Mechanical
    /// requirement on `head()`".
    ///
    /// Implementor hook — callable ONLY by this crate: requires the
    /// crate-private [`Hook`] witness, which no code outside this crate can
    /// construct (see the trait doc's witness-gating section).
    fn head(&self, _: &Hook) -> &StackHead<INDEX_BITS>;

    /// Load the "next" link for `index` with `Acquire` ordering.
    ///
    /// Implementor hook — callable ONLY by this crate: requires the
    /// crate-private [`Hook`] witness, which no code outside this crate can
    /// construct (see the trait doc's witness-gating section).
    fn load_next(&self, _: &Hook, index: u32) -> u32;

    /// Store the "next" link for `index` with `Release` ordering. This is the
    /// ONLY write the stack makes to link storage, and only during a push — the
    /// lazy-link (RAD-1) discipline: link storage is never eagerly initialised.
    ///
    /// Implementor hook — callable ONLY by this crate: requires the
    /// crate-private [`Hook`] witness, which no code outside this crate can
    /// construct (see the trait doc's witness-gating section).
    fn store_next(&self, _: &Hook, index: u32, next: u32);
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
    /// # Caller contract
    ///
    /// `index` must NOT already be reachable from ANY stack that reads and
    /// writes the same link cells this implementor's
    /// [`load_next`](StackStorage::load_next)/[`store_next`](StackStorage::store_next)
    /// touch — not merely from this one implementor value's stack: every
    /// index must have been placed on ITS stack by exactly one
    /// `push_index` and not yet popped.
    ///
    /// The obligation is stated over LINK CELLS, not over "the stack":
    /// link cells shared between two
    /// stacks with completely separate heads are rule 3's binding-level
    /// hazard — see the [`StackStorage`] trait doc's "The shared-storage
    /// hazard class" section for the full inventory (this shape has no
    /// runtime detector); pinned by
    /// `two_stacks_sharing_link_storage_still_double_issue` in
    /// `tests/custom_storage_impl.rs`. Re-pushing a live index is a
    /// caller-contract violation
    /// this method cannot catch — and cannot even check cheaply, because
    /// liveness is a property of the whole link chain and verifying it would
    /// cost an O(n) walk on every push. (Unlike the crate-root docs' H-2 and
    /// RAD-1 subtleties, this one is enforced by caller discipline, not
    /// structurally.) What `push_index` DOES check unconditionally is the
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
    /// # Panics
    ///
    /// Panics if `index >= INDEX_MASK` (the empty sentinel is reserved), in
    /// both debug and release builds — this is a caller-contract violation
    /// checked unconditionally, not a `debug_assert!`, because the failure
    /// mode is silent free-list corruption rather than a merely-suboptimal
    /// fallback. The formatted panic payload is allocated through the global
    /// allocator, so a consumer running this stack inside its own
    /// `#[global_allocator]` allocation path should treat this guard firing
    /// as abort-equivalent, not catchable-and-recoverable.
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
    fn push_index(&self, index: u32);

    /// Pop the top index off the stack (classic Treiber pop), or `None` if
    /// empty.
    ///
    /// Loads the tagged head, reads its next link, then CASes the head to that
    /// link with the SAME tag (a pop never bumps the tag). The tag in the high
    /// bits is the ABA defence: if a concurrent thread pops-then-repushes the
    /// SAME index between our load and our CAS, the tag advances and our CAS
    /// fails. (The residual hazard is a full tag wrap while this thread stays
    /// parked the whole time — bounded, see the crate docs' "Tag-width budget"
    /// section.)
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
    /// index or into the empty sentinel (rule 4 of the [`StackStorage`]
    /// implementor contract — see that section for the two corruption modes).
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
    /// [`push_index`](StackOps::push_index)'s `# Caller contract`; the guard
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
    /// shared-storage hazard class" section (this guard is rule 4's runtime
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
/// algorithm body stays written exactly ONCE.
pub(crate) trait SealedStorage<const B: u32> {
    fn head(&self) -> &StackHead<B>;
    fn load_next(&self, index: u32) -> u32;
    fn store_next(&self, index: u32, next: u32);
}

/// Bridge: every public [`StackStorage`] implementor is also a
/// [`SealedStorage`], so the crate-internal algorithm serves the public
/// [`StackOps`] blanket impl. The calls below are fully qualified to name
/// the trait each body delegates to — not for recursion avoidance and not
/// against E0034: a bare `self.head(&HOOK)` compiles fine today, because
/// the `&Hook` witness argument exists only on [`StackStorage`]'s `head`,
/// so arity alone resolves it to [`StackStorage::head`], never back into
/// this impl. The qualifier pins the callee instead of relying on that
/// signature accident.
impl<const B: u32, S: StackStorage<B> + ?Sized> SealedStorage<B> for S {
    fn head(&self) -> &StackHead<B> {
        StackStorage::head(self, &HOOK)
    }
    fn load_next(&self, index: u32) -> u32 {
        StackStorage::load_next(self, &HOOK, index)
    }
    fn store_next(&self, index: u32, next: u32) {
        StackStorage::store_next(self, &HOOK, index, next)
    }
}

/// The push CAS-retry algorithm, written ONCE against [`SealedStorage`] —
/// the body of [`StackOps::push_index`], which remains the documented public
/// surface (see its doc for the algorithm, the caller contract and
/// `# Panics`). [`ArrayIndexStack`]'s inherent `push` calls this directly,
/// off the public trait plumbing.
#[track_caller]
pub(crate) fn push_index_impl<const B: u32, S: SealedStorage<B> + ?Sized>(s: &S, index: u32) {
    let mask = TaggedIndex::<B>::INDEX_MASK;
    if u64::from(index) >= mask {
        push_index_out_of_range(index, mask);
    }
    // `head()` is read exactly ONCE per operation and the resulting
    // `&StackHead` held for the whole retry loop — see StackStorage's
    // "Mechanical requirement on head()".
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
        // Unpack the current head ONCE: the index half chains this push to
        // the top of the stack (below), the tag half feeds the ABA bump.
        let (cur_idx, tag) = TaggedIndex::<B>::unpack(head);
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
        // ONLY link write — never an eager init (RAD-1).
        s.store_next(index, next_link);
        // Advance the tag (the ABA fix) and CAS the head to this index.
        let new_tag = tag.wrapping_add(1);
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
        // The happens-before edge a popper needs from THIS push is carried
        // by the Release success CAS's own release sequence — extended by
        // every later head RMW (see the `head` field's INVARIANT) — never
        // by anything push's failed-CAS reads observe.
        // Strong `compare_exchange`, deliberately NOT
        // `compare_exchange_weak`. Measured, not unmeasured: the two are
        // equivalent on x86-64, and this crate's multi-target A/B harness
        // (`scripts/tis_p3_ab_runner.mjs`) found `weak` codegen-IDENTICAL
        // to strong on aarch64 under both the outlined-atomics default and
        // the `+lse` lowerings — the hypothesized inline-LL/SC
        // spurious-failure win does not exist on this toolchain, so the
        // strong form is kept for NO LL/SC benefit, not because the
        // question is unmeasured. See
        // `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` §0 (P3-2: NULL;
        // the driver asserts the identity, so a toolchain change fails
        // loudly and reopens the question). This concerns the CAS KIND
        // only; the separate LINK-ordering relaxation remains unmeasured —
        // see `StackStorage`'s "Ordering contract".
        match head_ref.compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => {
                // Retry-counter oracle (see `PUSH_RETRY_COUNT` below): a
                // REAL core atomic, so counts survive loom re-runs;
                // `Relaxed` counts only. Gated so a default build compiles
                // neither the counters nor this increment.
                #[cfg(any(feature = "test-internals", loom))]
                PUSH_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                head = actual;
                #[cfg(any(feature = "test-internals", loom))]
                if backoff.spin() {
                    PUSH_BACKOFF_CAP_REACH_COUNT
                        .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                }
                #[cfg(not(any(feature = "test-internals", loom)))]
                backoff.spin();
            }
        }
    }
}

/// The pop CAS-retry algorithm, written ONCE against [`SealedStorage`] —
/// the body of [`StackOps::pop_index`], which remains the documented public
/// surface (see its doc for the algorithm and `# Panics`).
/// [`ArrayIndexStack`]'s inherent `pop` calls this directly, off the public
/// trait plumbing.
#[track_caller]
pub(crate) fn pop_index_impl<const B: u32, S: SealedStorage<B> + ?Sized>(s: &S) -> Option<u32> {
    // `head()` is read exactly ONCE per operation and the resulting
    // `&StackHead` held for the whole retry loop — see StackStorage's
    // "Mechanical requirement on head()".
    let head_ref: &StackHead<B> = s.head();
    let mut head = head_ref.load(Ordering::Acquire);
    let mut backoff = Backoff::new();
    loop {
        if TaggedIndex::<B>::is_empty(head) {
            return None;
        }
        let (index, tag) = TaggedIndex::<B>::unpack(head);
        // Read the next link BEFORE the CAS (the push stored it under
        // Release; our Acquire observation of head — whether from the
        // initial load OR from a retry CAS failure — synchronizes with it).
        let next = s.load_next(index);
        // Unconditional guard (release-active, mirroring push's
        // `index < INDEX_MASK` check) for rule 4 of the StackStorage
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
        // Strong CAS, deliberately kept over `compare_exchange_weak` —
        // see push's identical note (measured NULL on aarch64:
        // `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md` §0).
        match head_ref.compare_exchange(head, new_head, Ordering::Acquire, Ordering::Acquire) {
            Ok(_) => return Some(index),
            Err(actual) => {
                // Retry-counter oracle — see push's identical comment
                // (`POP_RETRY_COUNT`).
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
                    if backoff.spin() {
                        POP_BACKOFF_CAP_REACH_COUNT
                            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                    #[cfg(not(any(feature = "test-internals", loom)))]
                    backoff.spin();
                }
            }
        }
    }
}

impl<const B: u32, S: StackStorage<B> + ?Sized> StackOps<B> for S {
    #[track_caller]
    fn push_index(&self, index: u32) {
        push_index_impl::<B, S>(self, index)
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

/// Cold panic path for [`StackOps::pop_index`]'s rule-4 guard, split out of
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

/// An owned standalone stack: head and links fused into ONE object. A
/// lock-free LIFO free-list of indices
/// with a wrapping generation
/// tag packed into the head word mitigating ABA at every permitted
/// `INDEX_BITS` (the tag defeats the ordinary short-window pattern; the
/// residual wrap bound is derived in the crate-root docs' "Tag-width budget"
/// section). Const-generic over the index width `INDEX_BITS` and the link
/// capacity `N`.
///
/// Fusion is ALSO the structural closure of the shared-head hazard: this
/// type deliberately does NOT implement the public [`StackStorage`] trait
/// (its head↔links binding is served by a crate-internal sealed accessor
/// instead), its `head` field is private, and no trait impl hands out a
/// `&StackHead` for it — so building a competing binding around a
/// standalone `ArrayIndexStack` does not COMPILE (E0277/E0599, pinned by
/// `tests/compile_fail/array_index_stack_head/`, the compile-fail successor
/// of the former `array_index_stack_head_still_double_issue` runtime
/// demonstration). That fixture pins ONE instantiation (`<16, 64>`); the
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
/// for standalone callers; a fresh stack is EMPTY (lazy links, RAD-1) — the
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
    /// [`StackOps::push_index`]'s doc for the algorithm, the caller contract
    /// and `# Panics`.
    // `#[track_caller]` chains the caller location through the forwarder down
    // to `push_index_impl` and its `#[cold]` panic helper, so diagnostics through
    // the owned type name the user's call site exactly as the trait method does.
    #[track_caller]
    pub fn push(&self, index: u32) {
        push_index_impl::<B, _>(self, index)
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

    /// The raw packed head word (`Acquire`) — forwarder to
    /// [`StackHead::raw_head`] (tests/loom suite need it).
    #[doc(hidden)]
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
    /// API, no semver guarantee. Read-only — it exposes no `&StackHead` and
    /// no link write, so it reopens none of the sealed hazard.
    #[doc(hidden)]
    pub fn load_next_for_test(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }
}

impl<const B: u32, const N: usize> Default for ArrayIndexStack<B, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const B: u32, const N: usize> SealedStorage<B> for ArrayIndexStack<B, N> {
    fn head(&self) -> &StackHead<B> {
        &self.head
    }
    fn load_next(&self, index: u32) -> u32 {
        self.links.load_next(index)
    }
    fn store_next(&self, index: u32, next: u32) {
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
