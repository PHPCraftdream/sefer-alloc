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
//! [`TaggedIndex::pack`]/[`unpack`](TaggedIndex::unpack) convert between an
//! `(index, tag)` pair and the packed word; [`try_pack`](TaggedIndex::try_pack)
//! is `pack`'s checked twin, returning `None` instead of silently truncating
//! an out-of-range index or tag.
//!
//! # Links — slot-resident OR owned
//!
//! The stack stores only the HEAD. Each pushed index's "next" link lives in
//! caller storage, reached through the [`Links`] trait ([`load_next`](Links::load_next) /
//! [`store_next`](Links::store_next)). This is what lets a production allocator
//! keep its links **slot-resident** (an `AtomicU32` field inside each slot it
//! already owns) instead of paying for a second array. For standalone use, the
//! crate provides [`ArrayLinks`]`<N>` — an owned `[AtomicU32; N]` backing.
//! Slot-resident does NOT mean payload-aliased — see the [`Links`] trait
//! doc's "Storage requirement" section for the full dedicated-storage rule
//! and why violating it defeats [`pop`](TaggedIndexStack::pop)'s own
//! corruption-detection guard (release-active, not debug-only — see
//! [`pop`](TaggedIndexStack::pop)'s `# Panics`).
//!
//! [`Links::store_next`] is the ONLY write the stack ever makes to a link, and
//! it happens during [`push`](TaggedIndexStack::push), immediately before the
//! CAS that publishes the index as the new head. The stack NEVER eagerly
//! initialises links — see "The lazy link discipline (RAD-1)" below.
//!
//! [`TaggedIndexStack::is_empty`] is an advisory, `Relaxed` emptiness check —
//! useful for diagnostics/monitoring, but a concurrent push or pop can make
//! it stale the instant it returns, so [`pop`](TaggedIndexStack::pop)'s
//! `None` remains the only authoritative empty check.
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
//! faster. That argument covers the CONTENDED regime; the other bound to
//! check is its opposite — the UNCONTENDED single-threaded case, where the
//! head line stays resident and exclusive in one core's L1 and no
//! cross-core ownership transfer ever happens — a regime governed not by
//! coherence transfer but by the latency of the bare RMW instruction itself
//! (`lock cmpxchg` on x86-64, or the target's equivalent CAS instruction):
//! materially faster, but still bounded. The wrap-time conclusion survives
//! both regimes: this crate's own single-threaded `churn` bench row
//! measures ~`2 × 10^7` successful pushes/sec in that uncontended regime (a
//! pop+push pair per iteration, so one successful push per pair — a
//! push-only rate would run faster still, but the pair rate is already an
//! order of magnitude under the working ceiling the next paragraph adopts,
//! so the argument does not need the tighter push-only number). Measured at
//! 51.56 ns/pair on an 11th Gen Intel Core i7-11800H, rustc 1.97.0
//! (2026-08-31); re-run `cargo bench -p tagged-index-stack --bench
//! tagged_index_stack_bench` for a fresh sample — the bound below only
//! needs the order of magnitude, not the exact figure.
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
//! shipped loom suite (`tests/loom_aba.rs`) model-checks the real
//! [`TaggedIndexStack`] / [`TaggedIndex`] code exhaustively — no
//! `preemption_bound`, so loom explores every interleaving these small models
//! admit. Several models run end-to-end through the shipped
//! [`push`](TaggedIndexStack::push)/[`pop`](TaggedIndexStack::pop); most of
//! the rest drive the real head atomic and the real packing through
//! `cas_head_for_test` so an interleaving can be pinned — the one exception is
//! the untagged-ABA counterfactual, which drives a locally-defined buggy
//! stand-in stack instead of the real type. `#[should_panic]` counterfactuals
//! (untagged corruption, the H-2 empty-transition tag-reset ABA, and a
//! Relaxed-CAS-failure-ordering regression) prove the harness is non-vacuous.
//! See `tests/loom_aba.rs`'s own module doc for the per-model breakdown.
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

#![no_std]
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

// Loom is an OPTIONAL `cfg(loom)`-gated dependency (feature `loom`): setting
// `--cfg loom` compiles the `#[cfg(loom)]` atomic aliasing below, which
// references the loom crate — but Cargo only resolves and links that
// dependency when the implicit `loom` feature is enabled as well. Fail fast
// with a named reason instead of the cryptic "unresolved import `loom`"
// error a cfg-without-feature build would otherwise produce.
#[cfg(all(loom, not(feature = "loom")))]
compile_error!(
    "building with --cfg loom requires --features loom (loom is now an \
     optional dependency)"
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
/// stack (the first index pushed onto an empty stack chains to this).
/// `u32::MAX`.
///
/// Note this is distinct from the "stack empty" head sentinel
/// ([`TaggedIndex::empty_index`]): `TAIL` marks a per-slot link's end-of-chain,
/// while the empty sentinel marks the HEAD word as carrying no index at all. The
/// two mappings are kept spelled out separately in [`push`](TaggedIndexStack::push) /
/// [`pop`](TaggedIndexStack::pop) so the invariant never rests on a numeric
/// coincidence between them.
pub const TAIL: u32 = u32::MAX;

/// Exponential-backoff cap for `push`/`pop`'s CAS-retry arms: on the Nth lost
/// CAS within one call, spin `1 << N.min(BACKOFF_SPIN_CAP)` times via
/// [`core::hint::spin_loop`] before retrying. `N` is a per-call local, reset
/// on every fresh `push`/`pop` — this backs off within one call's retry loop,
/// never across calls. Measured on the committed bench (x86-64, this repo's
/// `[profile.bench]` — `cargo bench`'s actual profile; byte-identical to
/// `[profile.release]` in this repo's `Cargo.toml` today, so no cited number
/// is affected by which name is used): ~5.3x-9.7x contended throughput at 8
/// threads, 0% single-thread cost (see CHANGELOG.md for the exact numbers).
///
/// **The cap is 6, and this is a fairness-vs-throughput choice, NOT a
/// low-contention-latency one.** A dedicated cap sweep
/// (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md`) measured caps `{0, 4, 6, 8,
/// 10}` at 2/4/8/16 threads on the committed bench and found cap 8 and cap
/// 10 BOTH beat cap 6 on throughput at every thread count tested, including
/// the lowest-contention 2-thread arm (+17% to +58% depending on regime) —
/// so cap 6 is not "the low-contention-optimal cap", contrary to an earlier
/// version of this doc comment that claimed exactly that without having
/// measured it. What cap 6 IS is the most fair of the three under
/// oversubscription: at 16 threads on a 16-logical-CPU host, cap 6's
/// per-thread throughput skew (`max/min` across threads) averaged ~6.1x
/// across 6 independent samples, vs. ~13.1x for cap 8 and ~20.6x for cap 10
/// — both cap 8 and cap 10 showed single-run outliers past 19x and 46x
/// respectively, i.e. one thread starved to a small fraction of its fair
/// share. Because a starved thread here means a starved allocator-slot
/// recycler for whatever consumer thread lost the race, that fairness cost
/// was judged not worth the throughput gain for the SHIPPED default — see
/// the linked report's §5 for the full reasoning. A caller who specifically
/// wants peak aggregate throughput under contention they know is benign can
/// measure a higher local cap using that report's §1 reproduction recipe;
/// the crate's default does not impose that tradeoff on every caller.
const BACKOFF_SPIN_CAP: u32 = 6;

/// `1u32 << spins.min(BACKOFF_SPIN_CAP)` masks/panics if `BACKOFF_SPIN_CAP`
/// ever reaches 32 — the same technique [`TaggedIndex::_CHECK_BITS`] uses to
/// turn a would-be shift-overflow into a compile error instead of a debug
/// panic / silently masked shift in release.
const _: () = assert!(BACKOFF_SPIN_CAP < 32);

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
    ///
    /// The tag half truncates the same silent way, and that truncation is
    /// deliberate, relied-upon behaviour rather than an oversight: `tag <<
    /// INDEX_BITS` on the fixed-width `u64` drops every bit at or above
    /// `2^TAG_BITS`, so an out-of-range tag loses its high bits instead of
    /// corrupting the (separately masked) index half. The stack depends on
    /// exactly this at the ABA wrap boundary —
    /// [`push`](TaggedIndexStack::push)'s `tag.wrapping_add(1)` may produce
    /// `2^TAG_BITS`, whose shifted-out high bit `pack` drops, restarting the
    /// tag at 0 (pinned by `tag_wraps_at_2_pow_48` in `tests/stack_unit.rs`).
    /// As with the index half, a caller that has not already validated its
    /// arguments should use [`try_pack`](Self::try_pack), the checked twin
    /// that returns `None` rather than a silently truncated word.
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
    /// crate from this crate's own perspective — can reach it). The attribute
    /// hides it from rustdoc's rendered navigation ONLY — it is still a fully
    /// callable `pub` item from any downstream crate; nothing in the language
    /// or this crate enforces non-callability. It carries no semver stability
    /// guarantee. Beyond the `tests/` reason above, it also has a real
    /// in-workspace consumer outside this crate, so it is NOT freely
    /// removable in a future 0.2 release without checking that caller first.
    /// Anywhere else the unconditional tag-0 reset reopens the H-2 ABA window
    /// documented above — a runtime drain must instead use
    /// [`empty_index`](Self::empty_index) with the tag it just observed. See
    /// this project's established `#[doc(hidden)]` rationale convention (cf.
    /// `raw_head`).
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
/// # Storage requirement: a DEDICATED cell, never payload-aliased
///
/// A link cell must remain dedicated storage — bytes this crate alone
/// writes — for as long as its index is out of the stack. It must NOT be
/// overlaid on the popped slot's payload (the classic "the link IS the free
/// block's first N bytes" idiom other allocators use to avoid a second
/// array). This crate does not support that layout, even though the
/// crate-root docs' "slot-resident" phrasing ("an `AtomicU32` field inside a
/// slot it already owns") can read as inviting it: slot-resident means the
/// link lives in memory the slot owns, not that it may share bytes with the
/// slot's live payload. The reason is [`pop`](TaggedIndexStack::pop)'s own
/// concurrency shape: a popper may legitimately call
/// [`load_next`](Self::load_next) on an index that a DIFFERENT thread has
/// already popped and handed to a consumer in the meantime (the popper read
/// a stale head, hasn't yet CASed) — this is benign only because the
/// popper's subsequent CAS is guaranteed to fail (the head moved) and the
/// read value is discarded on retry, never acted on. With dedicated storage
/// that stale read is still a valid TAIL-or-index value, merely a stale one.
/// With payload-aliased storage, the same read can observe arbitrary
/// consumer-written user data instead — not a link at all — which defeats
/// two things at once: the reasoning above (the read was never "safe
/// because meaningful", but a payload-aliased read is not even
/// link-shaped), and [`pop`](TaggedIndexStack::pop)'s rule-4 guard (which
/// exists to catch a backing violating rule 4 of
/// [`push`](TaggedIndexStack::push)'s `# Caller contract`, and is
/// release-active — see [`pop`](TaggedIndexStack::pop)'s `# Panics`) — that
/// guard would then PANIC on every ordinary benign race instead of only on
/// a real contract violation, in every build profile, not just a corruption
/// report confined to debug/test builds. A caller that wants this idiom
/// needs a DEDICATED link field per slot (as [`ArrayLinks`] does, and as
/// this crate's own downstream production consumers do), not payload
/// overlay.
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
///
/// # Layout note — link-array false sharing
///
/// Each link is a 4-byte `AtomicU32`, so 16 consecutive indices share one
/// 64-byte cache line (`4 bytes × 16 = 64 bytes`). [`push`](Links::store_next)
/// writes a link under `Release` and [`pop`](Links::load_next) reads one under
/// `Acquire`, so if indices from the same 16-index group are handed to
/// different threads under contention, this array becomes a SECOND contended
/// surface alongside the stack's own head — contended by accident of index
/// numbering, not by design. Fix it at the CALLER when a profile shows it:
/// wrap the index-to-link mapping so contended indices land in different
/// groups, use a `#[repr(align(64))]` newtype per link, or — the shape this
/// crate's own README recommends for production use — host links
/// slot-resident inside a larger per-slot struct instead of this array. Do
/// NOT pad `ArrayLinks` itself to one link per cache line: that would
/// multiply its footprint 16x for every single-threaded (or
/// contention-indifferent) caller, and this crate's whole pitch is not
/// paying for a second array's worth of memory traffic that most callers
/// never need.
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
    ///    succeeds trivially. An out-of-range return is a second, distinct
    ///    hazard — and a silent one, because [`pop`](Self::pop) packs the
    ///    value it read with [`TaggedIndex::pack`], which never rejects an
    ///    over-wide value: it masks it to its low `INDEX_BITS` bits. That
    ///    truncation corrupts in one of two ways. Masked to a live index:
    ///    `next = 0x1_0000` at `INDEX_BITS = 16` packs as index `0`, which
    ///    may still be owned elsewhere in the free-list — the same
    ///    double-issue as the zero-init collision above, reached by a
    ///    different route. Masked to the empty sentinel: a `next` whose low
    ///    `INDEX_BITS` bits are all ones (e.g. `0xFFFF` at width 16) packs
    ///    into a word [`is_empty`](TaggedIndex::is_empty) reads as EMPTY,
    ///    so the stack silently reports itself drained and every remaining
    ///    index in the chain is leaked at once — no panic, no `None`
    ///    anomaly, just a free-list that quietly shrinks to zero. See the
    ///    [`Links`] trait doc's "Storage requirement" section for why
    ///    payload-aliased link storage in particular always violates this
    ///    rule, even though no adversarial intent is involved.
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
    /// `index < INDEX_MASK` bound this very method's release-active bounds
    /// check DOES enforce on every call (see `# Panics`).
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
    /// may impose its own, separate bound — see [`ArrayLinks::load_next`]/
    /// [`ArrayLinks::store_next`]'s own `# Panics` docs for the `N`-vs-
    /// `INDEX_BITS` independence — so out-of-range access in the links layer
    /// is a second panic source this guard does not cover.
    #[track_caller]
    pub fn push<L: Links + ?Sized>(&self, links: &L, index: u32) {
        let mask = TaggedIndex::<INDEX_BITS>::INDEX_MASK;
        if (index as u64) >= mask {
            Self::push_index_out_of_range(index, mask);
        }
        let mut head = self.head.load(Ordering::Acquire);
        // Per-call retry counter driving the backoff below (see
        // BACKOFF_SPIN_CAP) — starts fresh every call, never persisted.
        let mut spins: u32 = 0;
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
            // empty→TAIL mapping out explicitly so the invariant never rests
            // on any numeric coincidence.
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
            // Release on success so a pop's Acquire sees the link we wrote.
            // Relaxed on failure is sound HERE, and the asymmetry with pop is
            // deliberate: a failed CAS sends push around the loop with the
            // value it read used ONLY as a value — the (cur_idx, tag)
            // recomputed for the next attempt's own store_next and CAS — so
            // push never follows (dereferences) a link through that read, and
            // the read carries no ordering burden. pop is NOT symmetric: its
            // retry's re-read names the index whose link load_next will
            // consult next, so pop's failure ordering MUST stay Acquire (the
            // loom counterfactual
            // `counterfactual_relaxed_cas_failure_corrupts_free_list` proves
            // Relaxed corrupts the free-list in a faithful hand-expansion of
            // this loop; the actual guard on `pop` itself is the end-to-end
            // test `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`).
            // The happens-before edge a popper needs from THIS push is
            // carried entirely by the Release
            // success CAS's own release sequence — which every later head RMW
            // extends (see the `head` field's INVARIANT) — never by anything
            // push's failed-CAS reads observe.
            match self
                .head
                .compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed)
            {
                Ok(_) => return,
                Err(actual) => {
                    #[cfg(loom)]
                    {
                        // Activation oracle for the loom suite (see
                        // `PUSH_RETRY_COUNT` below): deliberately a REAL
                        // core atomic, NOT loom's, so the count survives
                        // loom's many re-runs of the test closure and
                        // accumulates across the explored schedules.
                        // `Relaxed`: the counter promises no ordering, it
                        // only counts.
                        PUSH_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                    head = actual;
                    // Exponential backoff before retrying (BACKOFF_SPIN_CAP):
                    // lets the winning thread's Release CAS drain off the
                    // head cache line instead of every loser re-hammering it
                    // immediately. `spins` grows only within this call.
                    for _ in 0..(1u32 << spins.min(BACKOFF_SPIN_CAP)) {
                        core::hint::spin_loop();
                    }
                    // Capped, not unconditional: every increment past
                    // BACKOFF_SPIN_CAP is already dead (only ever consumed
                    // through `.min(BACKOFF_SPIN_CAP)` above), so let it keep
                    // climbing forever would just be an eventual
                    // `attempt to add with overflow` panic under
                    // overflow-checks after ~2^32 consecutive lost CASes in
                    // one call — remote, but free to close.
                    if spins < BACKOFF_SPIN_CAP {
                        spins += 1;
                    }
                }
            }
        }
    }

    /// Cold panic path for [`push`](Self::push)'s `index < INDEX_MASK`
    /// caller-contract guard, split out of `push` itself so the panic and its
    /// message formatting can never land in the hot loop's body (`#[cold]` +
    /// `#[inline(never)]`). `#[track_caller]` here — combined with
    /// `#[track_caller]` on `push`, which is what makes the location name the
    /// caller's call site rather than this crate's source line — forwards
    /// `push`'s received caller location down: a consumer pushing from many
    /// call sites learns WHICH one violated the contract.
    #[cold]
    #[inline(never)]
    #[track_caller]
    fn push_index_out_of_range(index: u32, mask: u64) -> ! {
        panic!(
            "index must be < INDEX_MASK (the empty sentinel is reserved), \
             got {index} (INDEX_MASK = {mask:#x})"
        );
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
    /// # Panics
    ///
    /// Panics if the [`Links::load_next`] result for the popped index is
    /// neither [`TAIL`] nor `< INDEX_MASK` — i.e. a value
    /// [`TaggedIndex::pack`] would otherwise silently truncate into a wrong
    /// (possibly still-live) index or into the empty sentinel (see rule 4 of
    /// [`push`](Self::push)'s `# Caller contract`). This check is
    /// unconditional (release-active), in both debug and release builds,
    /// mirroring [`push`](Self::push)'s `index < INDEX_MASK` guard: an
    /// out-of-tree A/B measured a release-active version of this check at
    /// ≈ 0 ns cost (see CHANGELOG.md), so there is no throughput reason left
    /// to leave a caller-contract violation whose failure mode is silent
    /// free-list corruption checked only in debug builds.
    ///
    /// `pop` also reaches link storage through the supplied [`Links`]
    /// implementation ([`load_next`](Links::load_next)), which may panic on
    /// an out-of-range index under its OWN, narrower bound (e.g.
    /// [`ArrayLinks::load_next`]'s `index >= N`) — the same links-layer
    /// panic source [`push`](Self::push)'s `# Panics` section describes;
    /// nothing here re-validates the index beyond what `push` guaranteed
    /// when the index was admitted.
    ///
    /// The `&L` passed here is subject to the same caller contract as
    /// [`push`](Self::push)'s — and `pop` is equally exposed to a backing
    /// swap (the corrupting call in that failure mode is typically a `pop`
    /// itself): see [`push`](Self::push)'s `# Caller contract` section,
    /// "ONE `Links` backing for the whole life of a non-empty stack".
    #[must_use = "a popped index is removed from the free-list; discarding it leaks the slot"]
    pub fn pop<L: Links + ?Sized>(&self, links: &L) -> Option<u32> {
        let mut head = self.head.load(Ordering::Acquire);
        // Per-call retry counter driving the backoff below (see
        // BACKOFF_SPIN_CAP) — starts fresh every call, never persisted.
        let mut spins: u32 = 0;
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
            // Unconditional guard (release-active, mirroring push's
            // `index < INDEX_MASK` check) for rule 4 of push's `# Caller
            // contract`: a backing returning anything but TAIL or a
            // currently-valid index is SILENTLY TRUNCATED by the pack()
            // below — to a wrong (possibly still-live) index, double-issuing
            // it, or to the empty sentinel, leaking the whole remaining
            // chain at once. Promoted from `debug_assert!` in round 7
            // (P3-1): an out-of-tree A/B measured the release-active check
            // at ≈ 0 ns cost (within noise of two `lock cmpxchg`/iter — see
            // `# Panics` below and CHANGELOG.md), so "release builds pay
            // nothing" no longer distinguishes debug-only from
            // release-active here, and the failure mode (silent free-list
            // corruption) is the same one `push`'s guard already treats as
            // unconditional.
            let mask = TaggedIndex::<INDEX_BITS>::INDEX_MASK;
            if next != TAIL && (next as u64) >= mask {
                Self::pop_link_out_of_range(index, next, mask);
            }
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
                Err(actual) => {
                    #[cfg(loom)]
                    {
                        // Activation oracle for the loom suite (see
                        // `POP_RETRY_COUNT` below): deliberately a REAL
                        // core atomic, NOT loom's, so the count survives
                        // loom's many re-runs of the test closure and
                        // accumulates across the explored schedules.
                        // `Relaxed`: the counter promises no ordering, it
                        // only counts.
                        POP_RETRY_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                    }
                    head = actual;
                    // Exponential backoff before retrying — see push's
                    // identical comment and BACKOFF_SPIN_CAP.
                    for _ in 0..(1u32 << spins.min(BACKOFF_SPIN_CAP)) {
                        core::hint::spin_loop();
                    }
                    // Capped — see push's identical comment.
                    if spins < BACKOFF_SPIN_CAP {
                        spins += 1;
                    }
                }
            }
        }
    }

    /// Cold panic path for [`pop`](Self::pop)'s rule-4 guard, split out of
    /// `pop` itself so the panic and its message formatting can never land
    /// in the hot loop's body — the same `#[cold]` + `#[inline(never)]`
    /// shape as [`push_index_out_of_range`](Self::push_index_out_of_range).
    /// Reports which of the two truncation outcomes the caller's
    /// `Links::load_next` would otherwise have silently produced (see
    /// `# Panics` on [`pop`](Self::pop)).
    #[cold]
    #[inline(never)]
    fn pop_link_out_of_range(index: u32, next: u32, mask: u64) -> ! {
        let outcome = if (next as u64 & mask) == mask {
            "the EMPTY SENTINEL, leaking the whole remaining chain"
        } else {
            "a wrong index, possibly a live one — double-issuing it"
        };
        panic!(
            "Links::load_next({index}) returned {next:#x}, neither TAIL nor \
             a valid index (< {mask:#x}): pop's pack() would silently \
             truncate it to {outcome}"
        );
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
    /// from this crate's own perspective — can reach it). The attribute hides
    /// it from rustdoc's rendered navigation ONLY; nothing prevents any
    /// downstream crate from calling it. It is not exercised by any
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
    /// `#[doc(hidden)]`: this is a `pub fn` (so `tests/` — an external crate
    /// from this crate's own perspective — can reach it). The attribute hides
    /// it from rustdoc's rendered navigation ONLY; nothing prevents any
    /// downstream crate from calling it under `--cfg loom`. It is not
    /// exercised by any production caller and carries no semver stability
    /// guarantee. See this project's established `#[doc(hidden)]`
    /// test-only-forwarder convention (cf. `raw_head`).
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

impl<const INDEX_BITS: u32> Default for TaggedIndexStack<INDEX_BITS> {
    fn default() -> Self {
        Self::new()
    }
}

/// **loom-test-only** activation counter for [`pop`](TaggedIndexStack::pop)'s
/// CAS-retry branch (the `Err(actual) => head = actual` arm, incremented
/// there). Deliberately a REAL `core::sync::atomic::AtomicUsize`, NOT
/// `loom::sync::atomic`: loom re-runs the closure passed to
/// `Builder::check` across many schedules within one process, and a real
/// static survives those re-runs, so the accumulated count is an exact
/// "how often was the retry branch actually reached" oracle over an entire
/// exploration. `Relaxed` access: the counter promises no ordering, it only
/// counts.
///
/// Compiled only under `--cfg loom`; never reset by this crate (snapshot and
/// diff is the caller's job — see [`pop_retry_count_for_test`]).
#[cfg(loom)]
static POP_RETRY_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// **loom-test-only** activation oracle: reads `POP_RETRY_COUNT` — the
/// number of times `pop`'s CAS-retry branch has executed in this process. The
/// loom suite asserts this counter ADVANCES across an exploration so a model
/// whose schedules never actually reach `pop`'s retry path fails loudly
/// instead of passing vacuously (see the assertion in
/// `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type`).
///
/// `#[doc(hidden)]`: this is a `pub fn` (so `tests/` — an external crate
/// from this crate's own perspective — can reach it). The attribute hides
/// it from rustdoc's rendered navigation ONLY; nothing prevents any
/// downstream crate from calling it. It is not exercised by any production
/// caller and carries no semver stability guarantee. See this project's
/// established `#[doc(hidden)]` test-only-forwarder convention (cf.
/// `raw_head`).
///
/// Never reset by this crate: snapshot before and diff after. The count is
/// process-global and cumulative — across loom's internal re-runs of a
/// test closure, across a test file's models, and (under the default
/// multi-threaded test harness) across concurrently running test functions.
/// The shipped loom suite's `MODEL_LOCK` mutex (`tests/loom_aba.rs`)
/// serializes every test that reads this counter specifically to prevent
/// this cross-test contamination.
#[cfg(loom)]
#[doc(hidden)]
#[must_use]
pub fn pop_retry_count_for_test() -> usize {
    POP_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}

/// **loom-test-only** activation counter for [`push`](TaggedIndexStack::push)'s
/// CAS-retry branch (the `Err(actual) => head = actual` arm, incremented
/// there). Deliberately a REAL `core::sync::atomic::AtomicUsize`, NOT
/// `loom::sync::atomic`: loom re-runs the closure passed to
/// `Builder::check` across many schedules within one process, and a real
/// static survives those re-runs, so the accumulated count is an exact
/// "how often was the retry branch actually reached" oracle over an entire
/// exploration. `Relaxed` access: the counter promises no ordering, it only
/// counts.
///
/// Compiled only under `--cfg loom`; never reset by this crate (snapshot and
/// diff is the caller's job — see [`push_retry_count_for_test`]).
#[cfg(loom)]
static PUSH_RETRY_COUNT: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// **loom-test-only** activation oracle: reads `PUSH_RETRY_COUNT` — the
/// number of times `push`'s CAS-retry branch has executed in this process. The
/// loom suite asserts this counter ADVANCES across an exploration so a model
/// whose schedules never actually reach `push`'s retry path fails loudly
/// instead of passing vacuously (see the assertion in
/// `push_push_conservation`).
///
/// `#[doc(hidden)]`: this is a `pub fn` (so `tests/` — an external crate
/// from this crate's own perspective — can reach it). The attribute hides
/// it from rustdoc's rendered navigation ONLY; nothing prevents any
/// downstream crate from calling it. It is not exercised by any production
/// caller and carries no semver stability guarantee. See this project's
/// established `#[doc(hidden)]` test-only-forwarder convention (cf.
/// `raw_head`).
///
/// Never reset by this crate: snapshot before and diff after. The count is
/// process-global and cumulative — across loom's internal re-runs of a
/// test closure, across a test file's models, and (under the default
/// multi-threaded test harness) across concurrently running test functions.
/// The shipped loom suite's `MODEL_LOCK` mutex (`tests/loom_aba.rs`)
/// serializes every test that reads this counter specifically to prevent
/// this cross-test contamination.
#[cfg(loom)]
#[doc(hidden)]
#[must_use]
pub fn push_retry_count_for_test() -> usize {
    PUSH_RETRY_COUNT.load(core::sync::atomic::Ordering::Relaxed)
}
