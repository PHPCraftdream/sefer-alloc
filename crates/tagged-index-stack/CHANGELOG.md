# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped before it.

### Added

- **`StackHead<INDEX_BITS>` + `StackStorage` / `StackOps` + `ArrayIndexStack<INDEX_BITS, N>`** —
  an allocation-free, `no_std`, `#![deny(unsafe_code)]` (eight audited, item-scoped `#[allow(unsafe_code)]`
  lint-exception regions in `src/imp.rs`; see
  `### Changed`) lock-free LIFO free-list of small **indices** (a slot recycler): the "recycle a
  small integer id" primitive that slab allocators, object pools, entity-component stores, and
  connection tables reinvent. `StackHead` is the tagged head word; custom storage implementors
  supply a `StackStorage` impl, and `push_index`/`pop_index` are crate-owned `StackOps`
  blanket-impl Treiber CAS loops over that implementor's single `AtomicU64` head; `ArrayIndexStack`
  (owned standalone) exposes plain `push`/`pop` forwarders. Caller contract for `push_index`: an
  index still reachable from ANY stack that reads and writes the same link cells must never be
  pushed again — the push overwrites that index's link with the current head, closing a link-cycle
  that makes `pop_index` hand the same index to two callers — and `push_index` cannot check liveness
  cheaply (it would cost an O(n) chain walk per push), so it enforces only the `index < INDEX_MASK`
  range bound; liveness is the caller's obligation. When the re-pushed index IS the current head,
  the cycle is a self-referential link and `pop_index`'s self-loop detector panics on the first pop
  rather than looping (see `### Fixed`).
- **`StackHead::is_empty()`** (also via `ArrayIndexStack::is_empty()`) — an advisory emptiness
  check: a `Relaxed` peek at the head word's index half. Concurrent pushes/pops can make the answer
  stale the instant it returns, in either direction, so it is for diagnostics/monitoring, not
  correctness decisions.
- **`TaggedIndex<INDEX_BITS>`** — the packed head word: low `INDEX_BITS` bits carry a slot index,
  the high `64 - INDEX_BITS` bits a STRICTLY MONOTONIC generation **tag** bumped on every successful
  push, ELIMINATING ABA outright for every permitted width — it never wraps (a pop-then-re-push of
  the same index bumps the tag, so a parked CAS on the stale `(index, tag)` pair fails and retries;
  a push that would need to bump the tag past `TaggedIndex::TAG_MAX` is refused instead
  (`Err(TagExhausted)`), sealing the stack — see the "Tag-width budget analysis" bullet below and
  the `TagExhausted` bullet in `### Changed`). `INDEX_BITS` is a const generic capped at `1..=16` at
  compile time (`TaggedIndex::_CHECK_BITS`) rather than merely discouraged: the cap keeps both halves non-empty, every valid index inside the `u32` that `push` takes, every legal configuration a tag of at least 48 bits, and `INDEX_MASK` below the `TAIL` link sentinel (`u32::MAX`) at every legal width (the historical `INDEX_MASK == TAIL` coincidence at the former width-32 cap is now structurally impossible); helpers `pack`/`unpack`/`empty`/`empty_index`/`is_empty`, all `const fn` (`empty` alone is additionally `#[doc(hidden)]` — hidden from rustdoc navigation while remaining callable, since its consumers include this crate's own bootstrap constructors; the other four are ordinary documented API). The index half's all-ones value is the reserved "stack empty" sentinel.
- **`TaggedIndex::pack(index, tag)` is CHECKED**: it returns `Option<u64>`: `Some(word)` for an in-range `(index, tag)` pair, `None` instead of a silently truncated word when the index is `>= 2^INDEX_BITS` (masking would yield a different, possibly empty-sentinel, index) or the tag is `>= 2^TAG_BITS` (whose high bits a shift would silently drop). The stack's own `push_index`/`pop_index` pack through a crate-private truncating fast path (`pack_truncating`) instead, purely to skip this function's redundant range re-check: `push_index` proves `tag <= TaggedIndex::TAG_MAX` before every call, refusing with `Err(TagExhausted)` instead of ever bumping the tag past it (see `### Changed`), so the truncating path never actually discards a bit in production use — it is not a wrap mechanism.
- **ABA-mitigating empty transition (the H-2 rule)** — when a `pop` drains the last element, the
  empty sentinel is packed with the **running tag** the draining pop observed, not reset to `0`: a tag reset would reopen the ABA window for a popper parked across a drain-and-refill. The shipped loom counterfactual `counterfactual_empty_transition_tag_reset_lets_aba_recur` proves this is load-bearing — with the tag reset restored, loom finds the collision.
- **Tag-width budget analysis** — the enforced `INDEX_BITS = 1..=16` cap guarantees every legal
  configuration a tag of at least 48 bits. The tag is GLOBAL to the whole stack, not per-slot, and every successful push serializes on the single head cache line (a locked RMW), so a head's pushes-until-sealed LIFETIME is `seal_time = 2^TAG_BITS / aggregate_successful_push_rate`, with the rate term bounded by hardware, not workload. At a generous `2 × 10^8` pushes/sec ceiling, a 48-bit tag seals every `~16` days (`~3.3` days even at `10^9`/sec) — a LIFETIME bound, not a risk bound: because the tag never recurs, there is no collision to reason about at any point before or after that seal, only an availability question (pushes are refused, loudly, once the budget is spent; pops are unaffected and keep draining). Widths above 16 are rejected at compile time (`TaggedIndex::_CHECK_BITS`) because the tag would shrink to 40 bits (at width 24) or 32 bits, collapsing the pushes-until-sealed window from days to minutes-to-seconds — an availability floor, not a soundness one (sealing is safe at any width, just impractically frequent below it). Documented so a consumer choosing `INDEX_BITS` knows the trade; full derivation (rate bound, contended vs uncontended regimes, and why `INDEX_BITS` > 16 is rejected) is in the crate-root docs' "Tag-width budget" section.
- **One-implementor storage binding — `StackStorage` / `StackOps`** — the implementor supplies head
  AND links in ONE impl: `head()` alongside `load_next` / `store_next`, so the head↔links binding
  is expressed once per impl rather than re-asserted per call. A production allocator keeps links
  **slot-resident** (an `AtomicU32` inside a slot it already owns) instead of paying for a second
  array. The blanket `StackOps` impl's CAS-loop bodies are crate-owned and cannot be overridden or
  reimplemented downstream. The former per-call `&Links` parameter — which allowed two `ArrayLinks`
  backings to be supplied as per-call arguments against one head, double-issuing an index — is
  gone, and that repro no longer compiles. The double-issue CLASS is not closed by the type system:
  two `StackStorage` implementor values whose `head()` methods return the same borrowed `StackHead`
  while their links differ still compile and still double-issue; those shapes compile only behind
  an `unsafe impl` that asserts the `# Safety` contract they violate — the class is closed by
  contract, not by the type system, with the value-level obligation named as the `StackStorage`
  trait doc's clause 1 and pinned by an assert-based demonstration in `tests/custom_storage_impl.rs`.
  **`ArrayLinks<N>`** remains a public links building block (inherent Acquire `load_next` / Release
  `store_next`); it is what `ArrayIndexStack` composes internally. The link storage must be a
  DEDICATED cell, never payload-aliased on the popped slot's own bytes. `pop` carries an
  unconditional, release-active guard (a `#[cold]`, `#[inline(never)]`, `#[track_caller]` panic
  helper mirroring `push`'s own index-range guard) that panics when a backing returns a link that is
  neither `TAIL` nor `< INDEX_MASK`, or a direct self-loop (`next == index`) — in EVERY build
  profile, not only debug. That is its entire scope: it does NOT validate index membership in, or
  reachability from, the live chain, so a foreign but in-range, non-self link value passes silently
  (pinned by `hand_crafted_acyclic_forgery_still_double_issues`), and payload aliasing is caught only
  in its self-loop sub-case, not made safe in general. Release-active by measurement: an out-of-tree
  A/B of this guard on the single-threaded `churn` bench (the pop-heaviest row) measured the guarded
  arm *faster* at the median (50.58 vs 51.60 ns/op debug-only; interleaved A/B table, source:
  `docs/reviews/2026-08-31-100751-tagged-index-stack-review-round7-oh.md`). The one in-workspace
  consumer (the root crate's `StackStorage<16>` `Registry` impl, `src/registry/heap_registry.rs`)
  cannot trigger the guard: its `next_free` field is only ever written by this crate's own
  `push_index` with `TAIL` or a previously-admitted index `< MAX_HEAPS (4096) < INDEX_MASK (65535)`,
  so `load_next` can only ever return `TAIL` or an in-range value — and a self-loop would
  additionally require the double-push `push_index`'s liveness contract forbids.
- **Lazy link discipline (internally: RAD-1)** — links are never eagerly initialised: only a `push`
  writes a link, immediately before publishing that index as head. A caller whose link backing is
  OS-zeroed memory (a fresh `mmap`, a zeroed slot array) never first-touches pages merely to set up
  the free-list; links commit lazily on first push of each index. A fresh stack is therefore
  **empty** — deliberately no "start with `0..N` pushed" constructor, which would require exactly
  the eager chaining pass this discipline forbids.
- **Correct CAS orderings** — push_index's success ordering and pop_index's retry ordering are both
  chosen so a popper can never read a link through a stale head: pop_index's CAS-failure load is
  `Acquire` — a `Relaxed` retry could read a stale link, and the shipped loom counterfactual
  `counterfactual_relaxed_cas_failure_corrupts_free_list` plants exactly that bug and watches the
  free-list corrupt. Push's index-validity and sentinel-reservation check is a single
  release-active bounds check (a `#[cold]` panic helper, not `debug_assert!`) — one guard covers
  both conditions, and it stays enforced in release builds too.
- **`ArrayIndexStack<INDEX_BITS, N>`** — the owned standalone stack fusing `StackHead` +
  `ArrayLinks<N>`, with plain `push`/`pop`/`is_empty` inherent forwarders, plus `Default` and
  `Debug`; the compile-fail-pinned shape for standalone callers.
- **64-bit-atomic portability gate** — the head is one `AtomicU64`, so the crate fails fast with a
  named `compile_error!` on targets without native 64-bit atomics (`thumbv6m-none-eabi`,
  `thumbv7em-none-eabi`, `riscv32imc-…`, `armv5te-…`) rather than a cryptic unresolved-import
  error. `no_std`-compatible, but `no_std` alone does not imply `AtomicU64`.
- **Exhaustive loom model-check against the real type**: under `--cfg loom` the stack's atomics
  alias to `loom::sync::atomic`, so the shipped loom suite (`tests/loom_aba.rs`) model-checks the
  actual `ArrayIndexStack` / `StackHead` / `TaggedIndex` code with NO `preemption_bound` — loom
  explores every interleaving these small models admit — with `#[should_panic]` counterfactuals
  (untagged corruption, the H-2 empty-transition tag-reset ABA, and the
  Relaxed-CAS-failure-ordering regression) proving the harness is non-vacuous. Several models run
  end-to-end through the shipped `push_index`/`pop_index`; most of the rest drive the real head
  atomic and the real packing through `cas_head_for_test` so an interleaving can be pinned — the
  one exception is the untagged-ABA counterfactual, which drives a locally-defined buggy stand-in
  stack instead of the real type. See `tests/loom_aba.rs`'s own module doc for the per-model
  breakdown. `loom` is an OPTIONAL `cfg(loom)`-gated dependency (feature `loom`): a normal build
  (default features, no `--cfg loom`) has zero non-`std` entries in `Cargo.lock` — not merely zero
  compiled code, which is the weaker guarantee a non-optional `cfg(loom)` dependency gives (Cargo's
  resolver locks normal target-cfg dependencies regardless of their cfg). Running the loom suite
  requires BOTH `RUSTFLAGS="--cfg loom"` and `--features loom`.
- **`StackHead::raw_head()`** — a `#[doc(hidden)]`, `test-internals`/loom-gated test-probe accessor
  for the packed head word (also reachable through `ArrayIndexStack`'s gated forwarder); it
  compiles only under the crate's off-by-default `test-internals` Cargo feature or a `--cfg loom`
  build, so a default published build — and its docs.rs render — does not contain it at all (not
  merely hidden from rustdoc navigation), and it exists for this crate's own `tests/` with no
  semver stability guarantee. `ArrayIndexStack::load_next_for_test`, the read-only link probe its
  tests use, carries the same gate.
- **`retry_counts_for_test()`** and **`backoff_cap_reached_for_test()`** — `#[doc(hidden)]`
  test-support accessors, each reading a `(pop, push)` tuple of counters (process-global,
  cumulative, never reset by this crate — snapshot and diff is the caller's job).
  `retry_counts_for_test` reads both CAS-retry counters (the non-loom twin of the loom suite's
  `#[cfg(loom)]` `pop_retry_count_for_test`/`push_retry_count_for_test`);
  `backoff_cap_reached_for_test` reads two further counters that advance only when a retry's spin
  loop ran at full backoff depth. Together they serve `tests/threaded_conservation.rs`'s two-level
  activation oracle: the first level proves the retry branch was reached under real OS threads, the
  second proves the backoff genuinely climbs into its higher range rather than shipping silently
  inert. Both accessors, both counters, and the retry-arm increments that write them compile only
  under the crate's off-by-default `test-internals` Cargo feature (or a `--cfg loom` build, where
  the loom suite's own accessors need them) — a default published build carries none of this
  instrumentation. The `#[doc(hidden)]` attribute only excludes them from rustdoc's rendered
  navigation (they remain publicly callable when the feature is on) and they carry no semver
  stability guarantee, like `raw_head()` above.
- **`pub const TAIL: u32`** — the per-slot link end-of-chain sentinel (`u32::MAX`), part of the
  `StackStorage` contract: an implementor's backing must be able to represent it.
- **`Default` for `StackHead`, `ArrayIndexStack`, and `ArrayLinks`** — all forward to `new()`;
  pinned by `default_stack_head_behaves_like_new` /
  `default_array_index_stack_behaves_like_new` / `default_array_links_behaves_like_new`
  (`tests/stack_unit.rs`; `default_stack_head_behaves_like_new` is itself
  `test-internals`/loom-gated — it reads the raw head word — while the other two stay ungated).
- **`Debug` derived on `StackHead`, `ArrayIndexStack`, and `ArrayLinks`.**

### Performance

- **Exponential backoff on `push`/`pop`'s CAS-retry arm** (`BACKOFF_SPIN_CAP = 6`, max 64
  `core::hint::spin_loop()` spins per retry, per-call `spins` counter never persisted across
  calls). Measured on the committed harness (`benches/tagged_index_stack_bench.rs`, x86-64):
  roughly 5.3x-9.7x contended throughput at 8 threads (`contention/push_pop` ~5.3x,
  `contention/churn` ~9.7x over baseline); single-thread cost stayed within run-to-run noise. A
  contention-shaped conservation check drained the stack after 8 threads × 200,000 pop/push
  iterations under the backoff and confirmed the exact multiset `0..64` came back with no duplicate
  or missing index (the same shape ships permanently as the committed
  `tests/threaded_conservation.rs` conservation test). The loom suite (`tests/loom_aba.rs`) stayed
  green at the same wall-clock: `core::hint::spin_loop()` touches no loom-tracked atomic, so it
  adds no new interleaving for loom to explore. Full ops/sec and ns/op receipt tables:
  `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (with its raw logs and
  `TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`).
- **`BACKOFF_SPIN_CAP = 6` kept after a dedicated throughput-vs-fairness cap sweep**
  (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` — its §3.1-§3.2/§5 hold the tables). Sweeping caps
  `{0, 4, 6, 8, 10}` at 2/4/8/16 threads: caps 8 and 10 beat cap 6 on aggregate throughput in
  nearly all cells, but with a real fairness cost that GROWS with the cap under oversubscription
  (single threads starved to a small fraction of their fair share), while caps 0/4 are fairer but
  slower. `BACKOFF_SPIN_CAP` stays `6` — a deliberate compromise, not a fairness optimum: fairer
  than caps 8/10, LESS fair than caps 0/4 — trading a real but bounded throughput ceiling against
  a starvation risk judged not worth imposing on every caller by default. `src/imp.rs`'s doc
  comment and this bullet state this throughput-vs-fairness axis.
- **`push`/`pop` are lock-free but NOT starvation-free** — the shipped cap trades worse pops on TWO
  axes where disabling the backoff wins: the absolute extreme-maximum pop time at every thread
  count tested (8 threads: 41-60 ms under the cap vs 0.6-24 ms disabled; 16 threads: 130-173 ms vs
  40-46 ms), and — at 8 threads specifically — the entire slow-pop tail-count band (>1 ms: 60-86
  pops per run under the cap vs 0-8 disabled; >10 ms: 26-34 vs 0-2), in exchange for better latency
  at every percentile through p99.9, a LOWER over-1-ms tail count at 16 threads specifically
  (249-285 pops vs 553-661 — the tail-count axis is thread-count-dependent, not uniform), and
  ~4-5x better aggregate throughput (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4).
- **One speculative perf change evaluated and declined** (unrelated to the backoff above): both
  `push`'s and `pop`'s CAS loops are `compare_exchange_weak` candidates, but any difference is
  specific to non-LSE AArch64 and similar `ldxr`/`stxr`-style architectures. The measurement
  harness exists (`scripts/tis_p3_ab_runner.mjs`, plus a workflow_dispatch-only arm64 CI gate
  `tis-weak-memory-wallclock-gate`), and its static codegen leg (rustc 1.97.0/LLVM 22) measured the
  weak-vs-strong CAS candidate as codegen-IDENTICAL on aarch64 under both the outlined-atomics
  default and the LSE lowerings — the once-hypothesized weak-CAS win does not exist on this
  toolchain (an oracle re-arms the question automatically if a toolchain change reintroduces an
  inline-LL/SC lowering). The wall-clock A/B on real arm64 silicon is still pending; until it runs,
  no ordering/CAS change is made (details:
  `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md`, a repository file, not part of the published
  package). A further candidate in the same class was considered and NOT changed: `pop_index`'s CAS
  SUCCESS ordering (`Ordering::Acquire` today) as a `Relaxed` candidate — on success the CAS reads
  no new value: the matched `head` is already held locally, Acquire-loaded either by `pop`'s
  initial head load or by the previous failed iteration's Acquire failure ordering, so the
  synchronizes-with edge already exists without the success ordering itself being
  `Acquire`/`AcqRel`; it remains `Acquire` (unmeasured, same standard as the sibling candidates in
  this section).
- **`push`'s initial head load uses `Ordering::Relaxed`** (push never follows a link through the
  observed word, so no ordering burden applies — the proof is in `push_index`'s source comment and
  `StackStorage`'s "Ordering contract" docs; expected benefit on weakly-ordered targets,
  unmeasured). `ArrayLinks`' link `Acquire`/`Release` and both CAS loops' strong
  `compare_exchange` remain in place: the static codegen gate (`scripts/tis_p3_ab_runner.mjs`,
  same toolchain as above) confirmed the link ordering is REAL at the ISA level on aarch64 (one
  acquire link load per pop, release link store(s) per push — `ldar`/`stlr`), while all variants
  are codegen-identical on x86-64, so keeping the ordering costs nothing there. These stay as
  deliberate defence-in-depth until the pending arm64 wall-clock run shows a measured win, and the
  contention harness times every worker against one shared `[timed_start, deadline)` window with an
  uncounted warm-up.

### Changed

- **BREAKING (unpublished 0.1.0): the crate's unsafe boundary, as shipped.** `StackStorage` is a
  `pub unsafe trait` carrying the normative implementor-side `# Safety` contract, and its three
  hooks (`head`, `load_next`, `store_next`) are `unsafe fn`, each with its own caller-side `#
  Safety` clause. `StackOps::push_index`, `ArrayIndexStack::push`, and the crate-internal push path
  (`push_index_impl`) are `unsafe fn` too, carrying a two-clause caller contract: (1) LINK DOMAIN
  — `index` must be in the implementor's declared link domain, for which the release-active
  `index < INDEX_MASK` guard (necessary for the head-word encoding) is NEVER sufficient proof; (2)
  LIVENESS / no double push — `index` must not currently be reachable through any binding whose
  hooks touch the same link cells. `pop_index`/`ArrayIndexStack::pop` deliberately stay safe: an
  unauthorized pop can only leak an index, never double-issue one. The crate-private
  `SealedStorage` bridge remains the sole hook call site; its `store_next` surface (trait + both
  impls) is `unsafe fn`, so the bridge forwards verbatim and the actual safety proof lives at the
  call site inside `push_index_impl` (the `push_index` contract is the
  `core::alloc::GlobalAlloc::dealloc` analogue — violating either clause is a soundness violation
  attributable to the caller). `ArrayIndexStack` deliberately does not implement the public
  `StackStorage` trait (crate-internal sealed accessor; competing bindings against the standalone
  type do not compile, compile-fail pinned). The crate moved from `#![forbid(unsafe_code)]` to
  `#![deny(unsafe_code)]`: the audited unsafe surface is EIGHT item-scoped `#[allow(unsafe_code)]`
  regions in the production library source (`src/`), all in `src/imp.rs` (self-verifying inventory:
  `grep -rnE '^\s*#!?\[allow\(unsafe_code\)\]' crates/tagged-index-stack/src/`; see the crate docs'
  "Where unsafe lives"). External implementors write `unsafe impl StackStorage` and `unsafe fn` hook bodies,
  upholding the trait's `# Safety` contract. Decision history — this boundary passed through
  three earlier designs in this unreleased cycle (safe hooks with one audited token; an
  unconstructible `&Hook` witness; whole-trait-unsafe with safe hooks), each superseded by the next
  re-audit — is in `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`
  (repository file, not part of the published package).
- **BREAKING (unpublished 0.1.0): `TaggedIndex::pack`'s `index` parameter and `unpack`'s index half
  move from `u64` to `u32`.** `_CHECK_BITS` already guarantees every valid index fits in 16 bits, so
  the old `u64` signature forced callers into narrowing/widening casts purely to move a value that
  could never legitimately need more than 32 bits; the type now carries that invariant directly. No
  runtime/algorithmic behavior changed — same bit patterns, same packing arithmetic, only the
  parameter/return type narrows to match the value's real range.
- **BREAKING (unpublished 0.1.0): the tag is now strictly monotonic — it never wraps — and
  `push_index`/`ArrayIndexStack::push` return `Result<(), TagExhausted>` instead of `()`.** Closes
  P1-1 from run-8's review (`docs/reviews/2026-09-02-180547-tagged-index-stack-review-Sol-codex-run-8.md`):
  a fully contract-compliant sequence of pushes/pops could previously wrap the tag counter back to
  its starting value, letting a stale CAS from an earlier-observed head succeed and hand out an
  index a different, concurrent thread still legitimately owned — an exclusive-issuance violation
  reachable without breaking either of `push_index`'s two documented `# Safety` clauses. Every
  successful push now installs a tag exactly one greater than the one it observed; a push that
  observes the ceiling (new `TaggedIndex::TAG_MAX`) is refused (`Err(TagExhausted)`) instead of
  wrapping to 0, sealing that head permanently (pops are unaffected and keep draining). New public
  surface: `TaggedIndex::TAG_MAX`, `TagExhausted` (the refusal error — no `core::error::Error` impl
  yet, deferred past this crate's 1.79 MSRV floor to a future MSRV bump past 1.81), and
  `StackHead::pushes_remaining()` (an advisory `Relaxed` readback of the remaining push budget). No
  reset/rotation API exists or is planned — a sealed head cannot be reset (see `StackHead`'s
  "Sealing is permanent" doc section); a replacement must be a distinct `StackHead`, fully drained
  first. Every in-workspace call site (root `sefer-alloc`'s `Registry::push_free_slot` and its loom
  shim, plus every test/bench/example in this crate) is updated in the same change. Chosen
  architecture and the counterexample this closes are recorded in an addendum to
  `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md`.

### Fixed

- **`pop_index`'s release-active clause-4 guard catches the self-loop shape (`next == index`) in
  addition to out-of-range links.** A contract-abiding chain can never link an index to itself —
  `push_index` stores the PREVIOUS head into `next[index]`, and that head is trivially already
  reachable — so a self-loop proves a caller-contract violation, of two possible causes. By far the
  simpler and more likely cause: a double-push of the index that is ALREADY the current head — the
  pushed index IS the head, so `push_index` itself writes `next[index] = index`; this needs no
  foreign writer and no shared storage, and the guard fires on the FIRST pop. The other: a writer
  other than a contract-abiding push answering for the popped index — in practice the
  zero-initialised backing of a second implementor value shared with (or stolen via `head()` from)
  another stack, whose `0` answers coincide with the popped index on the SECOND pop through it.
  The same cold panic helper (`pop_link_out_of_range`) dispatches THREE cases (self-loop;
  masked-to-empty-sentinel; masked-to-live-index). The added arm is a single `|| next == index` on
  the already-cold, release-active path; a real-contention audit over the actual `StackOps` code
  path observed 1,670,492 `load_next` calls and ZERO self-loops — no false-positive risk under
  contract-abiding contention — and the loom suite stayed 11/11 green. The canonical two-cause
  disjunction and the full catch/miss boundary live in `pop_index`'s `# Panics` and the
  `StackStorage` trait doc's hazard-class section. HONEST LIMITS, each pinned by a test: (a) a
  hand-crafted ACYCLIC link forgery still double-issues silently
  (`hand_crafted_acyclic_forgery_still_double_issues`); (b) SHARED LINK STORAGE between two
  independent stacks double-issues with no detection at all
  (`two_stacks_sharing_link_storage_still_double_issue`).
- **The P3-1/P3-2 wall-clock harness template (`scripts/tis_p3_ab/harness_bin.rs`) failed to
  compile (E0133) for a full round** after `push`/`push_index` became `unsafe fn` (see the
  unsafe-boundary bullet in `### Changed` above): its only exerciser is the
  `workflow_dispatch`-only `tis-weak-memory-wallclock-gate` CI job, and nothing in the regular
  per-PR/push CI path ever built it, so the break went undetected (Sol-codex review run 8, P2-2).
  Fixed: the three `stack.push(...)` call sites now carry local `unsafe` blocks, each with a
  `// SAFETY:` comment arguing `push_index`'s two-clause contract (link domain + liveness) for
  that specific call; the stale "100% safe code" module doc claim is removed
  (`#![deny(unsafe_code)]` stays crate-wide, with a statement-scoped `#[allow(unsafe_code)]` at
  each of the three sites); and a new `--mode build-check` mode in
  `scripts/tis_p3_ab_runner.mjs`, wired into a step in the `tagged-index-stack-gates` CI job
  (regular, non-manual), compiles the template on every push/PR — so a future `push`/`pop` API
  break fails immediately instead of staying invisible until the next manually-dispatched arm64
  run.

### Documentation

- **Rustdoc carries ONE canonical statement per narrative, with short pointers elsewhere**: the
  shared-storage hazard inventory + `# Safety` clauses + ordering contract → the `StackStorage`
  trait doc; the no-double-push rule → `push_index`'s `# Safety` section (clause 2, the caller-side
  unsafe contract); the self-loop two-cause disjunction → `pop_index`'s `# Panics`; the loom
  per-model breakdown → `tests/loom_aba.rs`'s module doc; the per-test status list →
  `tests/custom_storage_impl.rs`'s module doc.
- **Decision history**: the found-and-fixed development history lives in
  `docs/adr/2026-09-01-tagged-index-stack-doc-consolidation-and-review-history.md` and
  `docs/adr/2026-09-01-tagged-index-stack-storage-binding-closure.md` (repository files, not part
  of the published package).

### MSRV

- Rust **1.79** — the DECLARED, MEASURED floor of the PUBLISHED LIBRARY surface; the MSRV policy
  covers the library only. `cargo +1.79 check` compiles the library clean, both default and
  `--features test-internals`; the newest library API is the inline `const` in `ArrayLinks::new`'s
  array repeat expression, stable since 1.79. Dev/test code needs newer toolchains (the
  dev-dependency graph; `tests/stack_unit.rs`'s `std::panic::PanicHookInfo`, stable 1.81) and is
  code a library consumer never builds, so it does not raise the floor.
