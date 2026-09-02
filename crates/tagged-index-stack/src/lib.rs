//! `tagged-index-stack` — a lock-free LIFO free-list of small **indices** (a
//! *slot recycler*) whose head is a single atomic word packing an
//! `(index | tag)` pair, where a wrapping generation **tag** in the high bits
//! mitigates the ABA problem: the tag defeats the ordinary short-window ABA
//! pattern, but it is finite and demonstrably wraps, so ABA is reduced to a
//! quantified recurrence risk, not eliminated. A collision requires a full tag
//! wrap — `2^TAG_BITS` successful pushes anywhere on the stack — occurring
//! while one specific victim thread stays parked holding its stale snapshot
//! for that entire span; until both conditions hold, the stale CAS fails and
//! retries. The enforced `1..=16` cap on `INDEX_BITS` guarantees every legal
//! configuration a tag of at least 48 bits; the "Tag-width budget" section
//! below derives the hardware-bounded floor on that recurrence window. The
//! floor is a risk-reduction argument, not a proof of impossibility:
//! suspending a thread is outside the crate's control, and accepting that
//! residual risk is part of the caller's contract. Lock-freedom here describes
//! the stack's own CAS loops; end-to-end it additionally requires a
//! non-blocking [`StackStorage`] implementation.
//! Allocation-free, `no_std`; `#![deny(unsafe_code)]` with exactly TWO
//! audited `unsafe` sites — the `unsafe trait StackStorage` declaration
//! (whose three hooks are `unsafe fn`) and the crate-private bridge impl that
//! is their sole call site (see ["Where unsafe lives"](#where-unsafe-lives)
//! below).
//!
//! Slab allocators, object pools, entity-component stores, and connection
//! tables all need to recycle small integer ids, and commonly get two details
//! wrong (documented below): the **H-2 empty-transition tag preservation** and
//! the **lazy link discipline** (internally: RAD-1); both are structurally
//! enforced here.
//!
//! # The packed word — [`TaggedIndex`]
//!
//! The stack head is one `AtomicU64` holding a [`TaggedIndex`]`<INDEX_BITS>`:
//! the low `INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS`
//! bits carry a wrapping generation **tag** bumped on every successful push
//! and preserved on every pop. The all-ones value
//! ([`empty_index`](TaggedIndex::empty_index)) is reserved as the "stack
//! empty" sentinel, so the usable index range is `0 .. (1 << INDEX_BITS) - 1`.
//! The classic ABA scenario — a stale CAS on `(X, old_tag)` after X is popped
//! and re-pushed — fails because the re-push bumps the tag.
//! [`TaggedIndex::pack`]/[`unpack`](TaggedIndex::unpack) convert between an
//! `(index, tag)` pair and the packed word; `pack` is checked, returning
//! `None` for an out-of-range half instead of silently truncating it.
//!
//! # Storage — one implementor owns the head AND the links
//!
//! Each pushed index's "next" link lives in the implementor's storage, reached
//! through the [`StackStorage`] trait ([`load_next`](StackStorage::load_next) /
//! [`store_next`](StackStorage::store_next)), alongside the head it exposes via
//! [`head`](StackStorage::head). This is what lets a production allocator
//! keep its links **slot-resident** (an `AtomicU32` field inside each slot it
//! already owns) instead of paying for a second array; the crate provides
//! [`ArrayIndexStack`]`<INDEX_BITS, N>` for standalone use. The trait is
//! `unsafe` to implement — see its `# Safety` section. Slot-resident does
//! not mean payload-aliased — see the [`StackStorage`] trait doc's "Storage
//! requirement" section (violating it defeats
//! [`pop_index`](StackOps::pop_index)'s corruption-detection guard; see its
//! `# Panics`).
//!
//! The head↔links binding is expressed in ONE place — the implementor's own
//! single [`StackStorage`] impl, a trait
//! deliberately OPEN to external implementation (that is the extension
//! point, not a crate-owned surface) — instead of being re-asserted per
//! call via a per-call `&L: Links` parameter, as in the previous design.
//! What IS crate-owned is the operation side: [`StackOps`] is
//! blanket-implemented for every implementor and coherence makes a
//! downstream override impossible. The old repro's per-call shape — two
//! independent calls, each supplying a different backing against one head —
//! no longer compiles (pinned by a compile-fail regression test). The
//! obligation moved rather than vanished, and the part that stayed live is
//! implementor/caller discipline: one implementor value per head, for the
//! head's WHOLE life (trait clause 1), and disjoint index populations per
//! binding over any shared link-cell population — not "one link-cell
//! population per stack": cell sharing per se is harmless, only a
//! REACHABLE index across two bindings is the hazard (trait clause 3) — obligations about head↔links BINDINGS,
//! invisible to any per-impl audit.
//! The [`StackStorage`] trait doc's "The shared-storage hazard class"
//! section is the single source of truth for that hazard inventory and for
//! what the runtime does and does not detect; this doc does not re-derive
//! it.
//!
//! [`store_next`](StackStorage::store_next) is the only write the stack ever
//! makes to a link, and it happens during
//! [`push_index`](StackOps::push_index), immediately before the CAS that
//! publishes the index as the new head — see "The lazy link discipline
//! (RAD-1)" below. [`StackHead::is_empty`] is an advisory, `Relaxed`
//! emptiness check for diagnostics/monitoring; a concurrent push or pop can
//! make it stale the instant it returns, so
//! [`pop_index`](StackOps::pop_index)'s `None` remains the only authoritative
//! empty check.
//!
//! # Two correctness-critical subtleties (H-2 and RAD-1)
//!
//! ## H-2: the empty-transition tag MUST be preserved (not reset to 0)
//!
//! When a [`pop_index`](StackOps::pop_index) drains the last element, the head
//! transitions to "empty". A naive implementation packs the empty sentinel
//! with tag 0 ([`TaggedIndex::empty()`](TaggedIndex::empty)). That is a bug:
//! resetting the tag to 0 reopens the ABA window — a popper parked mid-`pop`
//! holding a stale `(idx, tag)` snapshot from before the drain sees its stale
//! tag recur once the stack drains (→ tag 0) and is refilled by a push of the
//! same index (→ tag 1); if the parked snapshot's tag was 1, the head word
//! recurs exactly and the stale CAS succeeds, corrupting the free-list. The
//! fix (in [`pop_index`](StackOps::pop_index)) packs the empty sentinel's
//! index half with the RUNNING tag the draining pop just observed, so the tag
//! keeps climbing across the empty transition. [`is_empty`](TaggedIndex::is_empty)
//! inspects only the index half, so a non-zero tag on the empty word is still
//! unambiguously "empty"; [`push_index`](StackOps::push_index) already reads
//! the tag out of the current head and bumps it, so it composes unchanged.
//! The shipped loom counterfactual
//! `counterfactual_empty_transition_tag_reset_lets_aba_recur` proves this is
//! load-bearing: with tag-reset restored, loom finds the collision.
//!
//! ## The lazy link discipline (RAD-1): links are never eagerly written
//!
//! The stack writes a slot's link only inside
//! [`push_index`](StackOps::push_index) (the
//! [`store_next`](StackStorage::store_next) immediately before publishing that
//! index as head) and performs no bulk/eager initialisation of the link
//! storage at construction. A caller whose link backing is OS-zeroed memory
//! (a fresh mmap, a zeroed slot array) therefore never first-touches those
//! pages merely to set up the free-list; [`ArrayLinks::new`] likewise starts
//! every link at `0`, matching OS-zeroed backing, rather than eagerly chaining
//! a full free-list. Consequently a freshly-constructed stack is empty — the
//! caller pushes indices in as they become free. This crate offers no "start
//! with `0..N` all pushed" constructor precisely because that would require an
//! eager link-chaining pass, defeating RAD-1. (A caller that wants every index
//! free from the start pushes `0..N` itself, or mints fresh indices via a
//! separate monotonic counter and pushes only recycled ones here.)
//!
//! # Tag-width budget — the wrap-time bound behind the ABA mitigation
//!
//! A tag defends against ABA only while it does not recur: a stale CAS can
//! succeed again only if the head word returns to the exact `(index, tag)`
//! pair the victim is holding, which takes a full tag wrap — `2^TAG_BITS`
//! successful pushes anywhere in the stack — the last of them re-pushing the
//! victim's own index, since the head must read exactly `(index, tag)` again,
//! not merely reach the tag again. The time a wrap takes is
//!
//! ```text
//! wrap_time = 2^TAG_BITS / aggregate_successful_push_rate
//! ```
//!
//! and the rate term is bounded by hardware, not by the workload. The tag is
//! global to the whole stack: every successful push is a compare-exchange (a
//! locked RMW) on the one `AtomicU64` head word, so in the contended regime
//! every push serializes on a single cache line whose exclusive ownership must
//! transfer between cores, capping the aggregate rate at roughly `10^8` to
//! `10^9` RMWs/sec no matter how many threads contend. The opposite regime —
//! the uncontended single-threaded case, where the head line stays resident
//! in one core's L1 — is governed instead by the latency of the bare RMW
//! instruction itself (`lock cmpxchg` on x86-64): materially faster, but
//! still bounded.
//!
//! Taking a generous `2 × 10^8` successful pushes/sec as the working ceiling:
//! at `INDEX_BITS = 16` — the widest permitted index half, 65535 usable
//! indices with the `0xFFFF` empty sentinel reserved above them — the tag
//! gets the other **48 bits**, wrapping at `2^48 ≈ 2.8 × 10^14`, and a wrap
//! takes `2^48 / (2 × 10^8) ≈ 16` days; even at the optimistic top of the
//! hardware range it is still `2^48 / 10^9 ≈ 3.3` days. And a wrap is only
//! the precondition for a collision: cashing one in further requires that
//! the head line stay saturated at the coherence ceiling continuously for
//! the entire span and that one specific victim thread sit parked holding
//! its stale snapshot the whole time. This bound is why `INDEX_BITS > 16` is
//! rejected at compile time (`TaggedIndex::_CHECK_BITS`) rather than merely
//! discouraged: at `INDEX_BITS = 24` the tag would be 40 bits,
//! `2^40 / (2 × 10^8) ≈ 92` minutes at the same ceiling — a long debugger
//! pause or OS scheduling delay defeats that — and the pre-cap `INDEX_BITS =
//! 32` maximum gave only `2^32 / (2 × 10^8) ≈ 21` seconds, within reach of
//! ordinary scheduling jitter. Within the permitted range a caller still
//! trades index range against tag headroom, but never below the 48-bit floor.
//!
//! The rate assumption's order of magnitude is confirmed by this repository's
//! own bench receipts
//! ([`docs/perf/_raw_tis_backoff_cap_sweep_run1.log`](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/_raw_tis_backoff_cap_sweep_run1.log)
//! — a repository file, not part of the published package; re-run `cargo
//! bench -p tagged-index-stack --bench tagged_index_stack_bench` for a fresh
//! sample) — the bound needs only the order of magnitude, not the exact
//! figure. The same receipts also bound the UNCONTENDED regime: the
//! single-threaded `churn` rows measure ~`2 × 10^7` successful pushes/sec (a
//! pop+push pair per iteration, so the push-only rate is somewhat higher) —
//! an order of magnitude under the working ceiling above.
//!
//! Read this section as what it is: a bound on the recurrence window — the
//! minimum time a victim thread must stay parked, at saturated push rates,
//! before its exact `(index, tag)` snapshot can recur. It does not prove
//! recurrence impossible; a caller whose threads can be parked indefinitely
//! (debuggers, stop-the-world pauses, extreme starvation) needs its own
//! hazard/epoch-style protection on top.
//!
//! # Lock-freedom and starvation
//!
//! [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
//! never block on a lock — a losing CAS retries — but lock-freedom is not
//! starvation-freedom: a call can lose arbitrarily many CASes in a row, and
//! the exponential backoff deliberately makes an unlucky call wait longer
//! between retries. The measured trade is not single-axis: the backoff-free
//! build (cap 0) wins the absolute worst single `pop` at every thread count
//! tested — on a 64-element `ArrayLinks` at 200,000 pop-then-repush
//! iterations, 41-60 ms across three runs under the shipped backoff cap vs
//! 0.6-24 ms disabled at 8 threads, and 130-173 ms vs 40-46 ms at 16 — AND,
//! at 8 threads specifically, the whole slow-pop tail-count band (pops
//! slower than 1 ms: 60-86 per run under the cap vs 0-8 disabled; slower
//! than 10 ms: 26-34 vs 0-2). In exchange the shipped cap wins every
//! percentile through p99.9 (≈ 1 µs vs 54-182 µs at 8-16 threads),
//! the >1 ms tail-count band at 16 threads specifically (249-285 pops vs
//! 553-661 — the tail-count axis is genuinely thread-count-dependent, not
//! uniform), and roughly 4-5x aggregate wall-clock throughput (median speedup
//! 4.85x at 8 threads, 4.05x at 16; the backoff-free build produced ~2.4x
//! more pops slower than 1 ms median-to-median, 1.9-2.6x across rep
//! pairings). A consumer
//! recycling a slot on a latency-sensitive request path should size its
//! tolerance for the extreme outliers AND the thread-count-dependent
//! tail-count band at its own thread count, not assume either single
//! thread count's story. Full measurements and the derivation are in
//! [`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md)
//! — a repository file (not
//! part of the published package), measured with
//! `examples/backoff_per_call_latency.rs`.
//!
//! # loom — the tests run against THIS type
//!
//! Under `--cfg loom` the stack's atomics alias to `loom::sync::atomic`, so
//! the shipped loom suite (`tests/loom_aba.rs`) model-checks the real
//! [`ArrayIndexStack`] / [`StackHead`] / [`TaggedIndex`] code exhaustively —
//! no `preemption_bound`, so loom explores every interleaving these small
//! models admit. Several models run end-to-end through the shipped
//! [`push`](StackOps::push_index)/[`pop`](StackOps::pop_index); most of the
//! rest drive the real head atomic and real packing through
//! `cas_head_for_test` — the one exception is the untagged-ABA counterfactual,
//! which drives a locally-defined buggy stand-in stack. `#[should_panic]`
//! counterfactuals prove the harness is non-vacuous. See
//! `tests/loom_aba.rs`'s own module doc for the per-model breakdown.
//!
//! # Where unsafe lives
//!
//! This crate contains exactly TWO audited `unsafe` sites, both item-scoped
//! `#[allow(unsafe_code)]`, both in `src/imp.rs` (tier 2 of this workspace's
//! two-tier unsafe-inventory convention): (1) the `unsafe trait StackStorage`
//! declaration — whose allow also covers its three `unsafe fn` hook
//! declarations (lint levels are inherited by nested items) — and (2) the
//! crate-private `SealedStorage` blanket-impl bridge, the SOLE call site of
//! the three hooks, holding their three `unsafe {}` blocks with per-call
//! `// SAFETY:` proofs. Exactly three `unsafe fn` declarations and exactly
//! three `unsafe` blocks exist in the crate — no other unsafe syntax
//! anywhere, pinned by `#![deny(unsafe_code)]`: unlike `forbid`, `deny` can
//! be locally relaxed — but only at those two audited sites — so every OTHER
//! `unsafe` token anywhere in the crate remains a hard compile error. The
//! inventory is self-verifying:
//!
//! ```text
//! grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' crates/tagged-index-stack/
//! ```
//!
//! which — run from the workspace root — returns exactly two hits in this
//! crate, both in `src/imp.rs` (the trait declaration; the bridge impl).
//!
//! WHY: because allocator consumers rely on [`StackStorage`]'s exclusive-issuance
//! contract for their own memory safety — sefer-alloc's registry free-list
//! today; any third-party unsafe allocator built on this crate after
//! publication. The moment unsafe code depends on a trait's contract, that
//! trait is in the same category as
//! [`core::alloc::GlobalAlloc`](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html)
//! and `std::alloc::Allocator` (unstable) — both `unsafe trait` for the
//! identical reason. Marking the trait `unsafe` does not make the compiler
//! verify the value-level binding invariant (unobservable to the type
//! system); it moves the unchecked promise into Rust's unsafe-contract
//! system, where responsibility for a violation is formally assigned to
//! whichever `unsafe impl` asserted a contract it did not uphold. The three
//! implementor hooks are `unsafe fn` — a call from safe code is E0133, and
//! an `unsafe`-block call takes on the hook's own caller-side `# Safety`
//! contract; see the [`StackStorage`] trait doc's unsafe-fn hooks, `# Safety`,
//! and `# Stability` sections.
//!
//! # Portability limit — requires 64-bit atomics
//!
//! The stack head is a single `AtomicU64` (the packed `(index | tag)` word —
//! see above); packing both halves into one atomic word is the entire
//! mechanism that makes the CAS in
//! [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
//! atomic across index-and-tag together, so this is not an incidental
//! implementation choice. That means this crate needs `target_has_atomic =
//! "64"` and will **not compile** on a target without native 64-bit atomic
//! support — notably `thumbv6m-none-eabi`, `thumbv7em-none-eabi`,
//! `riscv32imc-unknown-none-elf`, and `armv5te-unknown-linux-gnueabi`. This
//! crate is `no_std`-compatible, but `no_std` alone does not imply 64-bit
//! atomic support: many Cortex-M and RISC-V-without-A-extension targets are
//! `no_std` yet lack `AtomicU64` entirely. A build on an unsupported target
//! fails fast with an explicit [`compile_error!`] naming the requirement,
//! rather than the more cryptic "cannot find function/no `AtomicU64` in
//! `core::sync::atomic`" error a bare unresolved import would otherwise
//! produce.

#![no_std]
// `deny`, not `forbid`: the crate now contains exactly TWO audited `unsafe`
// sites, both item-scoped `#[allow(unsafe_code)]`, both in src/imp.rs (tier 2
// of this workspace's two-tier unsafe-inventory convention): the `unsafe
// trait StackStorage` declaration (whose allow also covers its three `unsafe
// fn` hooks) and the crate-private `SealedStorage` bridge impl — their sole
// call site, holding their three `unsafe {}` blocks. A `forbid` lint cannot
// be locally relaxed by any inner `#[allow]`, so it would reject both audited
// declarations; `deny` keeps every OTHER `unsafe` token anywhere in the crate
// a hard error.
// The self-verifying inventory command (see the crate docs' "Where unsafe
// lives" section) — run from the workspace root:
// `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' crates/tagged-index-stack/`
// — returns exactly two hits, both in src/imp.rs.
#![deny(unsafe_code)]
#![deny(missing_docs)]

// The stack head is one AtomicU64 (see the crate-doc "Portability limit"
// section above), which requires native 64-bit atomic support from the target.
// Fail fast with an explicit, named reason instead of the cryptic "no
// `AtomicU64` in `core::sync::atomic`" unresolved-import error.
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

// Loom is an optional `cfg(loom)`-gated dependency (feature `loom`), but Cargo
// only resolves and links it when the implicit `loom` feature is also enabled.
// Fail fast with a named reason instead of the cryptic "unresolved import
// `loom`" error a cfg-without-feature build would otherwise produce.
#[cfg(all(loom, not(feature = "loom")))]
compile_error!(
    "building with --cfg loom requires --features loom (loom is now an \
     optional dependency)"
);

// The entire implementation lives in one module gated on the exact complement
// of the two `compile_error!` conditions above: `compile_error!` does not stop
// rustc from parsing and name-resolving sibling items, so under an invalid
// configuration the module below is cfg'd out entirely and the build fails
// with only the named error — no secondary name-resolution error from the
// loom-aliasing `use` (nor from `AtomicU64` on a target without native 64-bit
// atomics). Under a valid configuration the module compiles and its public
// items are re-exported here.
#[cfg(all(target_has_atomic = "64", any(not(loom), feature = "loom")))]
mod imp;

#[cfg(all(target_has_atomic = "64", any(not(loom), feature = "loom")))]
pub use imp::*;
