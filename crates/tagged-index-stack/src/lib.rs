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
//! `#![deny(unsafe_code)]` with exactly EIGHT audited, item-scoped
//! `#[allow(unsafe_code)]` lint-exception regions, all in `src/imp.rs` — see
//! ["Where unsafe lives"](#where-unsafe-lives) below for the full
//! region-by-region inventory, the unsafe-operation count those regions
//! contain, and the separate test-fixture inventory.
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
//! seal_time = 2^TAG_BITS / aggregate_successful_push_rate
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
//! gets the other **48 bits**, sealing after
//! `2^48 - 1 ≈ 2.8 × 10^14` successful pushes, which takes
//! `2^48 / (2 × 10^8) ≈ 16` days at the working ceiling; even at the
//! optimistic top of the hardware range it is still `2^48 / 10^9 ≈ 3.3`
//! days before a head this width seals — at which point pushes are refused
//! (not corrupted), never silently. This bound is why `INDEX_BITS > 16` is
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
//! The production library source (`src/`) contains exactly EIGHT audited
//! `#[allow(unsafe_code)]` LINT-EXCEPTION REGIONS — item-scoped spans where
//! the `unsafe_code` lint is permitted to fire — all in `src/imp.rs` (tier 2
//! of this workspace's two-tier unsafe-inventory convention). A region is a
//! BOUNDARY: on edition 2021 an `unsafe fn`'s body has AMBIENT permission to
//! call another `unsafe fn` with no local `unsafe {}` block, so one region
//! covering an `unsafe fn` declaration can legitimately contain several
//! distinct unsafe operations, not just the one declaration — the region
//! count alone does not say how many. Since 2026-09-02 this crate also sets
//! `#![deny(unsafe_op_in_unsafe_fn)]` (below the header), which forces every
//! such call to carry its own local `unsafe {}` + `// SAFETY:` — the
//! region count is unaffected (no new `#[allow(unsafe_code)]` was added),
//! but the actual unsafe-block count inside those regions rose from three to
//! six (see the operation count below). Grouped by role:
//!
//! 1. the `unsafe trait StackStorage` declaration — its allow also covers its
//!    three `unsafe fn` hook declarations (`head`, `load_next`, `store_next`;
//!    lint levels are inherited by nested items);
//! 2. the crate-private `SealedStorage` trait declaration — a safe `pub(crate)`
//!    trait whose one `store_next` member is an `unsafe fn` (the allow covers
//!    that declaration);
//! 3. the caller-facing boundary `StackOps::push_index`'s trait-method
//!    declaration;
//! 4. the blanket [`StackOps`] impl's `push_index` body — an `unsafe fn`
//!    whose own local `unsafe {}` block forwards to `push_index_impl`;
//! 5. the shared internal `push_index_impl` — an `unsafe fn` whose own local
//!    `unsafe {}` block calls `store_next`;
//! 6. the owned type's [`ArrayIndexStack::push`] — an `unsafe fn` whose own
//!    local `unsafe {}` block forwards to `push_index_impl`;
//! 7. the crate-private `SealedStorage` blanket-impl bridge — the SOLE call
//!    site of the three `StackStorage` hooks, holding their three `unsafe {}`
//!    blocks with per-call `// SAFETY:` proofs;
//! 8. the owned type's `SealedStorage` impl block (its `store_next` body).
//!
//! Exactly ONE `unsafe trait`, exactly TEN `unsafe fn` declarations, ZERO
//! `unsafe impl`, and exactly SIX `unsafe {}` blocks exist in `src/` (ten fn =
//! `StackStorage`'s three hook declarations (`head`, `load_next`,
//! `store_next`), `SealedStorage`'s `store_next` declaration,
//! `StackOps::push_index`'s declaration, the blanket impl's `push_index`,
//! `push_index_impl`, `ArrayIndexStack::push`, the bridge impl's
//! `store_next`, and the owned type's `SealedStorage` impl's `store_next`;
//! six blocks = the bridge's three per-hook `unsafe {}` calls (region 7)
//! PLUS three more required by `#![deny(unsafe_op_in_unsafe_fn)]` (regions
//! 4, 5, 6): the blanket impl's `push_index` calling `push_index_impl`,
//! `push_index_impl` itself calling `store_next`, and
//! `ArrayIndexStack::push` calling `push_index_impl`) — no other unsafe
//! syntax in the library target, pinned by `#![deny(unsafe_code)]`: unlike
//! `forbid`, `deny` can be locally relaxed — but only at those eight audited
//! REGIONS — so every OTHER `unsafe` token in the library target remains a
//! hard compile error. These declaration/block/trait counts are re-derived
//! by grepping `unsafe fn|unsafe impl|unsafe trait|unsafe \{` in
//! `src/imp.rs`, not by counting `#[allow(unsafe_code)]` regions — see the
//! boundary-vs-contents distinction below.
//!
//! A separate inventory, deliberately NOT folded into the production claim
//! above: this repository's integration tests are separate crate targets
//! that do NOT inherit the library root's `#![deny(unsafe_code)]`, and
//! `tests/` intentionally contains additional `unsafe impl StackStorage`
//! blocks as test infrastructure — correct implementor fixtures
//! (`tests/custom_storage_impl.rs`) and deliberately-broken compile-fail
//! fixtures (`tests/compile_fail/`). Those are expected, audited test
//! fixtures outside the `src/` count, not a violation of it.
//!
//! The `src/` inventory's REGION BOUNDARY is self-verifying — but a region
//! count is a boundary check, not a contents check, and must not be read as
//! one: the command below counts `#[allow(unsafe_code)]` attributes, and
//! given that the library target compiles under `#![deny(unsafe_code)]`,
//! that count proves no unsafe token exists OUTSIDE those eight regions —
//! every un-allowed unsafe token would otherwise be a hard compile error.
//! It does NOT, by itself, prove how many unsafe declarations/blocks/traits
//! exist INSIDE those regions: a single item-scoped allow can cover one
//! `unsafe fn`/`unsafe trait` declaration plus every `unsafe {}` block its
//! own body contains — six of this crate's eight regions (1, 4, 5, 6, 7, 8
//! above) are exactly that shape. The two counts answer different
//! questions and neither substitutes for the other:
//!
//! ```text
//! grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' crates/tagged-index-stack/src/
//! ```
//!
//! — run from the workspace root — returns exactly eight hits, ALL in
//! `src/imp.rs`: eight lint-exception regions in the production library
//! source and none anywhere else in it. The command is deliberately scoped
//! to `src/` rather than the whole crate: an unscoped whole-crate grep
//! additionally returns four statement-scoped allows in the tracked perf
//! A/B tooling (`scripts/tis_p3_ab/harness_bin.rs` and
//! `scripts/tis_p3_ab/codegen_wrapper.rs.tmpl`) — scratch-crate generator
//! files, each with its own `#![deny(unsafe_code)]` root and per-site `//
//! SAFETY:` proofs, not part of the published library target, excluded
//! from this inventory the same way the `tests/` fixtures above are (those
//! fixtures need no per-site allow at all, because their crate targets
//! never carry the library's deny). For the CONTENTS — the actual unsafe
//! declarations/blocks/operations those regions hold — grep the tokens
//! directly instead:
//!
//! ```text
//! grep -nE 'unsafe fn|unsafe impl|unsafe trait|unsafe \{' crates/tagged-index-stack/src/imp.rs
//! ```
//!
//! which returns the one `unsafe trait`, ten `unsafe fn`, zero `unsafe
//! impl`, and six `unsafe {}` block matches enumerated above (plus
//! comment-only lines that merely mention these tokens in prose, which a
//! human reader filters by inspection — this second command is a content
//! CHECKLIST to audit against, not a self-checking assertion the way the
//! region-boundary command is).
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
//! implementor hooks AND the caller-facing push surface are `unsafe fn` — a
//! bare call from safe code is E0133, and an `unsafe`-block call takes on the
//! callee's own caller-side `# Safety` contract (`push_index`'s is the
//! three-clause link-domain + liveness + exclusive-ownership contract); `pop_index` deliberately
//! stays safe, because an unauthorized pop can only LEAK an index, never
//! double-issue one. See the [`StackStorage`] trait doc's unsafe-fn hooks,
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
// `deny`, not `forbid`: the library target (`src/`) holds eight audited,
// item-scoped `#[allow(unsafe_code)]` regions (tier 2 of this workspace's
// two-tier unsafe-inventory convention) that a `forbid` lint could not
// locally relax; `deny` keeps every OTHER `unsafe` token a hard compile
// error. Integration tests are separate crate targets that do not inherit
// this attribute and intentionally carry additional `unsafe impl` test
// fixtures. See the crate docs' "Where unsafe lives" section (above) for
// the full region inventory, the self-verifying grep command, and the
// unsafe-operation count those regions hold.
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
