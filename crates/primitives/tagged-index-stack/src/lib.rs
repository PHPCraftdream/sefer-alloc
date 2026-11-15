//! `tagged-index-stack` — a lock-free LIFO free-list of small **indices** (a
//! *slot recycler*) whose head is a single atomic word packing an
//! `(index | tag)` pair, where a STRICTLY MONOTONIC generation **tag** in the
//! high bits eliminates the ABA problem outright — it never wraps; a push
//! that would need to wrap is refused instead (`Err(`[`TagExhausted`]`)`) —
//! see "The tag is strictly monotonic" below for the full mechanism and
//! "Tag-width budget" for the pushes-until-sealed lifetime (at least
//! `2^48 - 1` at every legal `INDEX_BITS`). Lock-freedom here describes the
//! stack's own CAS loops; end-to-end it additionally requires a
//! non-blocking [`StackStorage`] implementation.
//!
//! # The tag is strictly monotonic — it never wraps
//!
//! Every successful push installs a tag exactly one greater than the one it
//! observed, and a push that observes [`TaggedIndex::TAG_MAX`] is refused
//! (`Err(`[`TagExhausted`]`)`) instead of wrapping to 0. Consequently every
//! `(index, tag)` head word occurs in at most one contiguous interval of the
//! head's history — from the push that installed it until the pop that
//! removes `index` — so a popper's CAS expecting `(index, tag)` can succeed
//! only while `index` is still the head it observed, and the link it read is
//! the link that push wrote. ABA is eliminated, not mitigated. The price is
//! a finite lifetime of `2^TAG_BITS - 1` successful pushes per head — at
//! least `2^48 - 1` at every legal width — after which the stack is sealed
//! (pops continue; pushes are refused). See [`StackHead`]'s "Sealing is
//! permanent" section: there is no reset API, by design.
//!
//! Allocation-free, `no_std`; the production library source (`src/`) is
//! `#![deny(unsafe_code)]`, with its `unsafe` surface confined to an audited
//! set of item-scoped `#[allow(unsafe_code)]` lint-exception regions, all in
//! `src/imp.rs` — see ["Where unsafe lives"](#where-unsafe-lives) below for
//! the audited region count, the full region-by-region inventory, and the
//! unsafe-operation count those regions contain.
//!
//! Slab allocators, object pools, entity-component stores, and connection
//! tables all need to recycle small integer ids, and commonly get two details
//! wrong (documented below): **empty-transition tag preservation** and the
//! **lazy link discipline**; both are structurally enforced here.
//!
//! # The packed word — [`TaggedIndex`]
//!
//! The stack head is one `AtomicU64` holding a [`TaggedIndex`]`<INDEX_BITS>`:
//! the low `INDEX_BITS` bits carry a slot index, the high `64 - INDEX_BITS`
//! bits carry a strictly monotonic generation **tag** bumped on every
//! successful push and preserved on every pop. The all-ones value
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
//! not mean payload-aliased — see the [`StackStorage`] trait's `# Safety`
//! contract (violating it defeats
//! [`pop_index`](StackOps::pop_index)'s corruption-detection guard; see its
//! `# Panics`).
//!
//! The head↔links binding is established once by the implementor's single
//! [`StackStorage`] impl. [`StackOps`] owns the operation side through its
//! blanket implementation, so a caller cannot supply different backing for
//! the same head on a later call. The value-level obligations are one live
//! binding per head for its whole life and disjoint reachable-index
//! populations when link cells are shared; cell sharing itself is harmless.
//! The [`StackStorage`] trait's `# Safety` contract is the source of truth
//! for those binding obligations.
//!
//! [`store_next`](StackStorage::store_next) is the only write the stack ever
//! makes to a link, and it happens during
//! [`push_index`](StackOps::push_index), immediately before the CAS that
//! publishes the index as the new head — see "The lazy link discipline"
//! below. [`StackHead::is_empty`] is an advisory, `Relaxed`
//! emptiness check for diagnostics/monitoring; a concurrent push or pop can
//! make it stale the instant it returns, so
//! [`pop_index`](StackOps::pop_index)'s `None` remains the only authoritative
//! empty check.
//! The unsafe implementation must ensure that no other storage, payload, or
//! binding without authority writes a link cell: only the stack algorithm may
//! mutate it during a push through the binding currently receiving valid,
//! unique publish/recycle authority. A successful pop through one binding may
//! transfer that authority to another binding sharing the cells; reachable
//! populations must remain disjoint before and after the transfer. Direct or
//! forged writes remain forbidden.
//!
//! # Two correctness-critical subtleties
//!
//! ## Empty-transition tag preservation
//!
//! When a [`pop_index`](StackOps::pop_index) drains the last element, the head
//! transitions to "empty". A naive implementation packs the empty sentinel
//! with tag 0 (the bootstrap word). That is a bug:
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
//! ## The lazy link discipline
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
//! eager link-chaining pass. (A caller that wants every index
//! free from the start pushes `0..N` itself, or mints fresh indices via a
//! separate monotonic counter and pushes only recycled ones here.)
//!
//! # Tag-width budget — the pushes-until-sealed lifetime
//!
//! Because the tag is strictly monotonic, it does not wrap — it SEALS: a
//! head accepts successful pushes until its tag reaches
//! [`TaggedIndex::TAG_MAX`] (`2^TAG_BITS - 1`), and the next push is refused
//! (`Err(`[`TagExhausted`]`)`) rather than wrapping the tag back to 0. This
//! is a LIFETIME bound, not a risk bound: once a head seals, pushes stop —
//! loudly, via `Err`, never silently — because the tag never recurs, so
//! there is no collision to reason about. This section derives how many
//! successful pushes, and how much wall time at a hardware-bounded rate
//! ceiling, a head's tag budget affords before that seal is reached:
//!
//! ```text
//! seal_time = (2^TAG_BITS - 1) / aggregate_successful_push_rate
//! ```
//!
//! The concrete `2^48 / rate`, `2^40 / rate`, and `2^32 / rate` forms below
//! are approximation-only shorthand; the exact numerator is one less in each
//! case.
//!
//! The rate term is bounded above by the fastest regime, not by the workload.
//! An uncontended head line resident in one core's L1 makes the successful
//! push rate roughly a `10^8`/sec hardware ceiling; contention on that one
//! cache line only lowers the aggregate. The cited sweep's 8-16-thread rows
//! measure roughly `1.1–1.4 × 10^7` pop+push pairs/sec, versus about
//! `1.8 × 10^7` pairs/sec single-threaded. The deliberately generous
//! `2 × 10^8` working ceiling below is therefore an upper bound for both
//! regimes, not a contended-rate estimate.
//!
//! Taking a generous `2 × 10^8` successful pushes/sec as the working ceiling:
//! at `INDEX_BITS = 16` — the widest permitted index half, 65535 usable
//! indices with the `0xFFFF` empty sentinel reserved above them — the tag
//! gets the other **48 bits**, sealing after
//! `2^48 - 1 ≈ 2.8 × 10^14` successful pushes, which takes
//! `2^48 / (2 × 10^8) ≈ 16` days at the deliberately generous ceiling —
//! at which point pushes are refused (not corrupted), never silently. This
//! bound is why `INDEX_BITS > 16` is
//! rejected at compile time (`TaggedIndex::_CHECK_BITS`) rather than merely
//! discouraged: at `INDEX_BITS = 24` the tag would be 40 bits,
//! `2^40 / (2 × 10^8) ≈ 92` minutes at the same ceiling — sealing a hot
//! free-list within a single long-running process's ordinary lifetime is a
//! real availability concern, not merely a debugger-pause hazard — and the
//! pre-cap `INDEX_BITS = 32` maximum gave only `2^32 / (2 × 10^8) ≈ 21`
//! seconds, well within reach of a single benchmark run. Within the
//! permitted range a caller still trades index range against tag headroom,
//! but never below the 48-bit floor.
//!
//! The rate assumption's order of magnitude is confirmed by this repository's
//! own bench receipt
//! ([`docs/perf/_raw_tis_backoff_cap_sweep_run1.log`](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/_raw_tis_backoff_cap_sweep_run1.log)).
//! For a fresh sample, run `cargo bench -p tagged-index-stack
//! --bench tagged_index_stack_bench`; the bound needs only the order of
//! magnitude, not the exact figure.
//!
//! Read this section as what it is: a bound on how long — in pushes, and in
//! wall time at a hardware-bounded rate ceiling — a head's tag budget lasts
//! before [`push_index`](StackOps::push_index) starts refusing with
//! `Err(`[`TagExhausted`]`)`. It is NOT a bound on a residual ABA risk: the
//! seal makes tag recurrence impossible regardless of how long any thread
//! stays parked (see "The tag is strictly monotonic" above) — a caller does
//! not need its own hazard/epoch-style protection on top for correctness.
//! What it DOES need, for AVAILABILITY, is either enough tag headroom for
//! its expected process lifetime at this rate ceiling, or a plan for what
//! happens once a head seals: drain and replace it with a distinct
//! [`StackHead`] object (see [`StackHead`]'s "Sealing is permanent" section
//! — there is no reset). A caller needing a longer lifetime trades index
//! range for tag headroom via a narrower `INDEX_BITS` (see
//! [`TaggedIndex::TAG_BITS`]).
//!
//! # Lock-freedom and starvation
//!
//! [`push_index`](StackOps::push_index)/[`pop_index`](StackOps::pop_index)
//! never block on a lock — a losing CAS retries — but lock-freedom is not
//! starvation-freedom: a call can lose arbitrarily many CASes, and capped
//! exponential backoff can make an unlucky call wait longer between retries.
//! The shipped cap trades a small number of extreme outliers for better
//! latency through p99.9. A historical repository contention sweep reported a
//! roughly 4-5x aggregate-throughput difference on its measured host; that is
//! historical evidence, not a current or portable performance guarantee. A
//! latency-sensitive consumer should size its tolerance at its own thread count; the trade is host- and
//! microarchitecture-dependent because the cap counts `spin_loop` hints, not
//! portable time units. Full measurements and the derivation are in
//! [`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4](https://github.com/PHPCraftdream/sefer-alloc/blob/main/docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md).
//!
//! # loom — the tests run against THIS type
//!
//! Under `--cfg loom` the stack's atomics alias to `loom::sync::atomic`, so
//! the loom model suite model-checks the real [`ArrayIndexStack`] /
//! [`StackHead`] / [`TaggedIndex`] code exhaustively —
//! no `preemption_bound`, so loom explores every interleaving these small
//! models admit. Several models run end-to-end through the shipped
//! [`push`](StackOps::push_index)/[`pop`](StackOps::pop_index); most of the
//! rest drive the real head atomic and real packing through
//! `cas_head_for_test` — the one exception is the untagged-ABA counterfactual,
//! which drives a locally-defined buggy stand-in stack. `#[should_panic]`
//! counterfactuals prove the harness is non-vacuous.
//!
//! # Where unsafe lives
//!
//! The production library source (`src/`) contains exactly ten audited
//! `#[allow(unsafe_code)]` regions, all in `src/imp.rs`:
//!
//! 1. `StackStorage`'s unsafe-trait declaration;
//! 2. `SealedStorage`'s three unsafe-hook declarations;
//! 3. the `StackOps::push_index` unsafe-method declaration;
//! 4. the `StackOps` blanket implementation;
//! 5. the shared `push_index_impl` body;
//! 6. the shared `pop_index_impl` body;
//! 7. the `SealedStorage` blanket bridge;
//! 8. `ArrayIndexStack::push`;
//! 9. `ArrayIndexStack`'s `SealedStorage` implementation.
//! 10. the loom-only `ArrayIndexStack::store_next_for_test` probe.
//!
//! The production contents are exactly one unsafe trait, seventeen unsafe
//! function declarations, zero unsafe impls, and nine local `unsafe {}`
//! blocks. The inventory covers only the published library source.
//!
//! ```text
//! rg -n '^\s*#\[allow\(unsafe_code\)\]' src/imp.rs
//! rg -n '^\s*(?:pub(?:\([^)]*\))?\s+)?unsafe (?:trait|fn|impl)|^\s*unsafe \{|=\s*unsafe \{' src/imp.rs
//! ```
//!
//! The first command checks region boundaries; the second checks the unsafe
//! contents inside them, so neither count substitutes for the other.
//!
//! WHY: because allocator consumers rely on [`StackStorage`]'s exclusive-issuance
//! contract for their own memory safety — an allocator's registry free-list
//! today, and any third-party unsafe allocator built on this crate after
//! publication. The moment unsafe code depends on a trait's contract, that
//! trait is in the same category as
//! [`core::alloc::GlobalAlloc`](https://doc.rust-lang.org/core/alloc/trait.GlobalAlloc.html)
//! and `std::alloc::Allocator` (unstable) — both `unsafe trait` for the
//! identical reason. Marking the trait `unsafe` does not make the compiler
//! verify the value-level binding invariant (unobservable to the type
//! system); it moves the unchecked promise into Rust's unsafe-contract
//! system, where responsibility for a violation is formally assigned to
//! whichever `unsafe impl` asserted a contract it did not uphold. The three
//! implementor hooks AND the caller-facing push surface are `unsafe fn` — a
//! bare call from safe code is E0133, and an `unsafe`-block call takes on the
//! callee's own caller-side `# Safety` contract (`push_index`'s is the
//! three-clause link-domain + liveness + exclusive-ownership contract); `pop_index` deliberately
//! stays safe, because an unauthorized pop can only LEAK an index, never
//! double-issue one. See the [`StackStorage`] trait's unsafe-fn hooks,
//! `# Safety`, and `# Stability` sections.
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
// `deny`, not `forbid`: the library target (`src/`) holds audited,
// item-scoped `#[allow(unsafe_code)]` regions (tier 2 of this workspace's
// two-tier unsafe-inventory convention) that a `forbid` lint could not
// locally relax; `deny` keeps every OTHER `unsafe` token a hard compile
// error. Integration tests are separate crate targets that do not inherit
// this attribute and intentionally carry additional `unsafe impl` test
// fixtures. See the crate docs' "Where unsafe lives" section (above) for
// the audited region count, the full region inventory, the self-verifying
// grep command, and the unsafe-operation count those regions hold.
#![deny(unsafe_code)]
// Edition 2021 gives an `unsafe fn` body ambient permission to call another
// `unsafe fn` with no local `unsafe {}` — a real gap the tier-2 allow-region
// grep (see "Where unsafe lives" above) cannot see, because it counts
// `#[allow(unsafe_code)]` REGIONS, not unsafe OPERATIONS inside them. This
// closes that gap: every unsafe call inside an `unsafe fn` body now needs
// its own local `unsafe {}` + `// SAFETY:`, same as safe-code call sites.
#![deny(unsafe_op_in_unsafe_fn)]
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
