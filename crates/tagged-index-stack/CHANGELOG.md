# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **`StackHead<INDEX_BITS>` + `StackStorage` / `StackOps` +
  `ArrayIndexStack<INDEX_BITS, N>`** — an allocation-free, `no_std`,
  `#![forbid(unsafe_code)]` lock-free LIFO free-list of small **indices** (a
  slot recycler): the "recycle a small integer id" primitive that
  slab allocators, object pools, entity-component stores, and connection
  tables reinvent. `StackHead` is the tagged head word; custom storage
  implementors supply a `StackStorage` impl, and `push_index`/`pop_index` are
  crate-owned `StackOps` blanket-impl Treiber CAS loops over that
  implementor's single `AtomicU64` head; `ArrayIndexStack` (owned standalone)
  exposes plain `push`/`pop` forwarders. Caller contract for
  `push_index`: an index still reachable from ANY stack that reads and writes
  the same link cells must never be pushed again — the push overwrites that
  index's link with the current head,
  closing a link-cycle that makes `pop_index` hand the same index to two
  callers — and `push_index` cannot check liveness cheaply (it would cost an
  O(n) chain walk per push), so it enforces only the `index < INDEX_MASK`
  range bound; liveness is the caller's obligation. (Consequence description
  since corrected: when the re-pushed index IS the current head, the cycle is
  a self-referential link and `pop_index`'s self-loop detector panics on the
  first pop rather than looping; see the `### Fixed` bullet below.)
- **`StackHead::is_empty()`** (also exposed via `ArrayIndexStack::is_empty()`)
  — an advisory emptiness check: a
  `Relaxed` peek at the head word's index half. Concurrent pushes/pops can
  make the answer stale the instant it returns, in either direction, so it
  is for diagnostics/monitoring, not correctness decisions.
- **`TaggedIndex<INDEX_BITS>`** — the packed head word: low `INDEX_BITS` bits
  carry a slot index, the high `64 - INDEX_BITS` bits a wrapping generation
  **tag** bumped on every successful push, which is what mitigates the ABA
  problem for every permitted width (a pop-then-re-push of the same index
  bumps the tag, so a parked CAS on the stale `(index, tag)` pair fails and
  retries; only a full tag wrap under a thread parked across that entire
  span can recur the stale pair — see the "Tag-width budget analysis" bullet
  below). `INDEX_BITS` is a const generic capped at `1..=16` at compile
  time (`TaggedIndex::_CHECK_BITS`) rather than merely discouraged — the cap
  keeps both halves non-empty, every valid index inside the `u32` that `push`
  actually takes, every legal configuration guaranteed a tag of at least
  48 bits, and `INDEX_MASK` below the `TAIL` link sentinel (`u32::MAX`) at
  every legal width (the historical `INDEX_MASK == TAIL` coincidence at the
  former width-32 cap is now structurally impossible); helpers
  `pack`/`unpack`/`empty`/`empty_index`/`is_empty`, all `const fn` (`empty`
  alone is additionally `#[doc(hidden)]` — callable but carrying no semver
  stability guarantee, like `raw_head()` below; the other four are ordinary
  documented API). The index half's all-ones value is the reserved "stack
  empty" sentinel.
- **`TaggedIndex::pack(index, tag)` is CHECKED** (review Sol-codex run-3
  P2-1): it now returns `Option<u64>` — `Some(word)` for an in-range
  `(index, tag)` pair, `None` instead of a silently truncated word when
  the index is `>= 2^INDEX_BITS` (which the old `pack` masked to a
  different, possibly empty-sentinel, index) or the tag is `>= 2^TAG_BITS`
  (whose high bits the old `pack`'s shift silently dropped). The stack's
  own `push_index`/`pop_index` are behaviourally unchanged: they pack
  through a crate-private truncating fast path (`pack_truncating`), their
  inputs already guaranteed in range by their own guards, so the hot path
  pays no new branch and the ABA tag wrap at `2^TAG_BITS` still happens
  exactly as before.
- **ABA-mitigating empty transition (the H-2 rule)** — when a `pop` drains the
  last element, the empty sentinel is packed with the **running tag** the
  draining pop observed, not reset to `0`: a tag reset would reopen the ABA
  window for a popper parked across a drain-and-refill. The shipped loom
  counterfactual `counterfactual_empty_transition_tag_reset_lets_aba_recur`
  proves this is load-bearing — with the tag reset restored, loom finds the
  collision.
- **Tag-width budget analysis** — the enforced `INDEX_BITS = 1..=16` cap
  guarantees every legal configuration a tag of at least 48 bits. The tag is
  GLOBAL to the whole stack, not per-slot, and every successful push serializes
  on the single head cache line (a locked RMW), so a wrap takes
  `wrap_time = 2^TAG_BITS / aggregate_successful_push_rate` with the rate term
  bounded by hardware, not workload. At a generous `2 × 10^8` pushes/sec
  ceiling, a 48-bit tag wraps every `~16` days (`~3.3` days even at
  `10^9`/sec) — and a wrap is only the PRECONDITION for a collision, which
  further requires the head line saturated continuously for the entire span
  AND one specific victim thread parked motionless the whole time. Widths
  above 16 are rejected at compile time (`TaggedIndex::_CHECK_BITS`) because
  the tag would shrink to 40 bits (at width 24) or 32 bits, collapsing the
  wrap window from days to minutes-to-seconds. The derivation bounds the
  recurrence window — the minimum time a victim thread must stay parked
  before its stale snapshot can recur — it does not prove recurrence
  impossible (suspending a thread is outside the crate's control).
  Documented so a consumer choosing `INDEX_BITS` knows the trade. Full
  derivation (rate bound, contended vs uncontended regimes, and why
  `INDEX_BITS` > 16 is rejected): the crate-root docs' "Tag-width budget"
  section.
- **One-implementor storage binding — `StackStorage` / `StackOps`** — the
  implementor supplies head AND links in ONE impl: `head()` alongside
  `load_next` / `store_next`, so the head↔links binding is expressed once
  per impl rather than re-asserted per call. A production allocator keeps
  links
  **slot-resident** (an `AtomicU32` inside a slot it already owns) instead of
  paying for a second array. The blanket `StackOps` impl's CAS-loop bodies
  are crate-owned and cannot be overridden or reimplemented downstream.
  The former per-call `&Links` parameter — which allowed the old repro's
  specific shape, two `ArrayLinks` backings supplied as per-call arguments
  against one head, double-issuing an index (an independent pre-release
  review's release-blocking P1-1 finding) — is gone, and THAT repro no
  longer compiles. The double-issue CLASS is not structurally closed
  (round-11 @oh review, finding P2-1): two `StackStorage` implementor
  values whose `head()` methods return the same borrowed `StackHead` while
  their links differ still compile and still double-issue; that shape is
  now a named implementor/caller obligation (the `StackStorage` trait
  doc's rule 1), pinned by an assert-based demonstration in
  `tests/custom_storage_impl.rs`. **`ArrayLinks<N>`** remains a public links
  building block (inherent Acquire `load_next` / Release `store_next`); it is
  what `ArrayIndexStack` composes internally. The link storage must be a
  DEDICATED cell, never
  payload-aliased on the popped slot's own bytes — `pop` carries an
  unconditional, release-active guard (a `#[cold]`, `#[inline(never)]`,
  `#[track_caller]` panic helper mirroring `push`'s own index-range guard)
  that panics the moment a backing returns anything but `TAIL` or a
  currently-valid index — in EVERY build profile, not only debug — which is
  exactly what a payload-aliased backing does on every ordinary benign race.
  The guard is release-active by measurement, not assumption: an out-of-tree
  A/B of this exact check on the single-threaded `churn` bench (the
  pop-heaviest row) measured the guarded arm *faster* at the median (50.58
  vs 51.60 ns/op debug-only; source:
  `docs/reviews/2026-08-31-100751-tagged-index-stack-review-round7-oh.md`,
  finding P3-1's interleaved A/B table), i.e. the cost sits below the
  harness's noise
  floor next to the two `lock cmpxchg`/iteration already on the hot path —
  and the failure mode (silent free-list corruption) is the same class
  `push_index`'s guard already treats as unconditional. The one in-workspace
  consumer, the root crate's `StackStorage<16>` impl on its registry
  (`src/registry/heap_registry.rs`), cannot trigger the guard: its `next_free`
  field is only ever
  written by this crate's own `push_index` with `TAIL` or a previously-admitted
  index `< MAX_HEAPS (4096) < INDEX_MASK (65535)`, so `load_next` can only
  ever return `TAIL` or an in-range value.
- **Lazy link discipline (internally: RAD-1)** — links are never eagerly initialised:
  only a `push` writes a link, immediately before publishing that index as
  head. A caller whose link backing is OS-zeroed memory (a fresh `mmap`, a
  zeroed slot array) never first-touches pages merely to set up the free-list;
  links commit lazily on first push of each index. A fresh stack is therefore
  **empty** — deliberately no "start with `0..N` pushed" constructor, which
  would require exactly the eager chaining pass this discipline forbids.
- **Correct CAS orderings** — push_index's success ordering and pop_index's
  retry ordering are both chosen so a popper can never read a link through a
  stale head: pop_index's CAS-failure load is `Acquire` — a `Relaxed` retry
  could read a
  stale link, and the shipped loom counterfactual
  `counterfactual_relaxed_cas_failure_corrupts_free_list` plants exactly that
  bug and watches the free-list corrupt. Push's index-validity and
  sentinel-reservation check is a single release-active bounds check (a
  `#[cold]` panic helper, not `debug_assert!`) — one guard covers both
  conditions, and it stays enforced in release builds too.
- **`ArrayIndexStack<INDEX_BITS, N>`** — the owned standalone stack fusing
  `StackHead` + `ArrayLinks<N>`, with plain `push`/`pop`/`is_empty` inherent
  forwarders, plus `Default` and `Debug`; the compile-fail-pinned shape for
  standalone callers.
- **64-bit-atomic portability gate** — the head is one `AtomicU64`, so the
  crate fails fast with a named `compile_error!` on targets without native
  64-bit atomics (`thumbv6m-none-eabi`, `thumbv7em-none-eabi`, `riscv32imc-…`,
  `armv5te-…`) rather
  than a cryptic unresolved-import error. `no_std`-compatible, but `no_std`
  alone does not imply `AtomicU64`.
- **Exhaustive loom model-check against the real type**: under `--cfg loom`
  the stack's atomics alias to `loom::sync::atomic`, so the shipped loom suite
  (`tests/loom_aba.rs`) model-checks the actual `ArrayIndexStack` /
  `StackHead` / `TaggedIndex` code with NO `preemption_bound` — loom explores every
  interleaving these small models admit — with `#[should_panic]`
  counterfactuals (untagged corruption, the H-2 empty-transition tag-reset
  ABA, and the Relaxed-CAS-failure-ordering regression) proving the harness
  is non-vacuous. Several models run end-to-end through the shipped
  `push_index`/`pop_index`; most of the rest drive the real head atomic and the real
  packing through `cas_head_for_test` so an interleaving can be pinned — the
  one exception is the untagged-ABA counterfactual, which drives a
  locally-defined buggy stand-in stack instead of the real type. See
  `tests/loom_aba.rs`'s own module doc for the per-model breakdown. `loom` is an
  OPTIONAL `cfg(loom)`-gated dependency (feature
  `loom`): a normal build (default features, no `--cfg loom`) has zero
  non-`std` entries in `Cargo.lock` — not merely zero compiled code, which is
  the weaker guarantee a non-optional `cfg(loom)` dependency gives (Cargo's
  resolver locks normal target-cfg dependencies regardless of their cfg).
  Running the loom suite requires BOTH `RUSTFLAGS="--cfg loom"` and
  `--features loom`.
- **`StackHead::raw_head()`** — a `#[doc(hidden)]` test-probe accessor for the
  packed head word (also reachable through `ArrayIndexStack`'s
  `#[doc(hidden)]` forwarder); the attribute only excludes it from rustdoc's
  rendered
  navigation (it remains publicly callable), it carries no semver stability
  guarantee, and it exists for this crate's own `tests/`.
- **`retry_counts_for_test()`** and **`backoff_cap_reached_for_test()`** —
  `#[doc(hidden)]` test-support accessors, each reading a `(pop, push)`
  tuple of counters (process-global, cumulative, never reset by this
  crate — snapshot and diff is the caller's job). `retry_counts_for_test`
  reads both CAS-retry counters (the non-loom twin of the loom suite's
  `#[cfg(loom)]` `pop_retry_count_for_test`/`push_retry_count_for_test`);
  `backoff_cap_reached_for_test` reads two further counters that advance
  only when a retry's spin loop ran at full backoff depth. Together they
  serve `tests/threaded_conservation.rs`'s two-level activation oracle:
  the first level proves the retry branch was reached under real OS
  threads, the second proves the backoff genuinely climbs into its higher
  range rather than shipping silently inert. Both accessors, both
  counters, and the retry-arm increments that write them compile only
  under the crate's off-by-default `test-internals` Cargo feature (or a
  `--cfg loom` build, where the loom suite's own accessors need them) — a
  default published build carries none of this instrumentation. The
  `#[doc(hidden)]` attribute only excludes them from rustdoc's rendered
  navigation (they remain publicly callable when the feature is on) and
  they carry no semver stability guarantee, like `raw_head()` above.
- **`pub const TAIL: u32`** — the per-slot link end-of-chain sentinel
  (`u32::MAX`), part of the `StackStorage` contract: an implementor's backing must
  be able to represent it.
- **`Default` for `StackHead`, `ArrayIndexStack`, and `ArrayLinks`** — all
  forward to `new()`; pinned by `default_stack_head_behaves_like_new` /
  `default_array_index_stack_behaves_like_new` /
  `default_array_links_behaves_like_new` (`tests/stack_unit.rs`).
- **`Debug` derived on `StackHead`, `ArrayIndexStack`, and `ArrayLinks`.**

### Performance

- **Exponential backoff on `push`/`pop`'s CAS-retry arm** (`BACKOFF_SPIN_CAP =
  6`, max 64 `core::hint::spin_loop()` spins per retry, per-call `spins`
  counter never persisted across calls). Measured on the committed harness
  (`benches/tagged_index_stack_bench.rs`, x86-64): roughly 5.3x-9.7x
  contended throughput at 8 threads (`contention/push_pop` ~5.3x,
  `contention/churn` ~9.7x over baseline); single-thread cost stayed within
  run-to-run noise. A contention-shaped conservation check drained the stack
  after 8 threads × 200,000 pop/push iterations under the backoff and
  confirmed the exact multiset `0..64` came back with no duplicate or
  missing index (the same shape ships permanently as the committed
  `tests/threaded_conservation.rs` conservation test). The loom suite
  (`tests/loom_aba.rs`) stayed green at the
  same wall-clock: `core::hint::spin_loop()` touches no loom-tracked atomic,
  so it adds no new interleaving for loom to explore. Full ops/sec and
  ns/op receipt tables: `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (with its
  raw logs and `TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`).
- **`BACKOFF_SPIN_CAP = 6` kept after a dedicated throughput-vs-fairness cap
  sweep** (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` — its §3.1-§3.2/§5 hold
  the tables) — an independent review flagged the cap's original doc comment
  as claiming an unmeasured "low enough for LOW contention" rationale.
  Sweeping caps `{0, 4, 6, 8, 10}` at 2/4/8/16 threads found that claim
  WRONG: caps 8 and 10 beat cap 6 on aggregate throughput in nearly all
  cells, but with a real fairness cost that GROWS with the cap under
  oversubscription (single threads starved to a small fraction of their
  fair share), while caps 0/4 are fairer but slower. `BACKOFF_SPIN_CAP`
  stays `6` — a deliberate compromise, not a fairness optimum: fairer than
  caps 8/10, LESS fair than caps 0/4 — trading a real but bounded
  throughput ceiling against a starvation risk judged not worth imposing on
  every caller by default. Both `src/imp.rs`'s doc comment and this bullet
  now state the real throughput-vs-fairness axis instead of the old
  unmeasured low-contention-latency claim.
- **`push`/`pop` are lock-free but NOT starvation-free** — the shipped cap
  trades worse pops on TWO axes where disabling the backoff wins: the
  absolute extreme-maximum pop time at every thread count tested (8
  threads: 41-60 ms under the cap vs 0.6-24 ms disabled; 16 threads:
  130-173 ms vs 40-46 ms), and — at 8 threads specifically — the entire
  slow-pop tail-count band (>1 ms: 60-86 pops per run under the cap vs
  0-8 disabled; >10 ms: 26-34 vs 0-2), in exchange for better latency
  at every percentile through p99.9, a LOWER over-1-ms tail count at 16
  threads specifically (249-285 pops vs 553-661 — the tail-count axis
  is thread-count-dependent, not uniform), and ~4-5x better aggregate
  throughput (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.4).
- **One speculative perf change evaluated and declined** (unrelated to the
  backoff above): both `push`'s and `pop`'s CAS loops are
  `compare_exchange_weak` candidates, but any difference is specific to
  non-LSE AArch64 and similar `ldxr`/`stxr`-style architectures. The
  measurement harness cited below now exists
  (`scripts/tis_p3_ab_runner.mjs`, plus a workflow_dispatch-only arm64 CI
  gate `tis-weak-memory-wallclock-gate`), and its static codegen leg
  (rustc 1.97.0/LLVM 22) measured the weak-vs-strong CAS candidate as
  codegen-IDENTICAL on aarch64 under both the outlined-atomics default and
  the LSE lowerings — the once-hypothesized weak-CAS win does not exist on
  this toolchain (an oracle re-arms the question automatically if a
  toolchain change reintroduces an inline-LL/SC lowering). The wall-clock
  A/B on real arm64 silicon is still pending; until it runs, no
  ordering/CAS change is made (details: `docs/perf/TIS_LINK_ORDERING_WEAK_CAS_GATE.md`,
  a repository file, not part of the published package). A further
  candidate in the same class,
  considered and NOT changed this round (unmeasured, same standard as the
  sibling candidates here): `pop_index`'s CAS SUCCESS ordering
  (`Ordering::Acquire` today) as a `Relaxed` candidate, by the same style
  of argument as the push-side weakening candidates already listed in this
  section — on success the CAS reads no new value: the matched `head` is
  already held locally, Acquire-loaded either by `pop`'s initial head load
  or by the previous failed iteration's Acquire failure ordering, so the
  synchronizes-with edge already exists without the success ordering
  itself being `Acquire`/`AcqRel`.
- **`push`'s initial head load uses `Ordering::Relaxed`** (push never follows
  a link through the observed word, so no ordering burden applies — the
  proof is in `push_index`'s source comment and `StackStorage`'s
  "Ordering contract" docs; expected benefit on weakly-ordered targets,
  unmeasured). `ArrayLinks`' link `Acquire`/`Release` and both CAS loops'
  strong `compare_exchange` remain in place: the static codegen gate
  (`scripts/tis_p3_ab_runner.mjs`, same toolchain as above) confirmed the
  link ordering is REAL at the ISA level on aarch64 (one acquire link load
  per pop, release link store(s) per push — `ldar`/`stlr`), while all
  variants are codegen-identical on x86-64, so keeping the ordering costs
  nothing there. These stay as deliberate defence-in-depth until the
  pending arm64 wall-clock run shows a measured win, and the contention
  harness times every
  worker against one shared `[timed_start, deadline)` window with an
  uncounted warm-up.

### Changed

- **BREAKING (unpublished 0.1.0): `StackStorage` is now an `unsafe trait`
  (methods stay safe `fn`) with a normative `# Safety` contract**; the
  crate moved from `#![forbid(unsafe_code)]` to `#![deny(unsafe_code)]`
  with exactly one audited unsafe token (see the crate docs' "Where unsafe
  lives"). `ArrayIndexStack` no longer implements `StackStorage` at all
  (crate-internal sealed accessor; competing bindings against the
  standalone type no longer compile, compile-fail pinned). External
  implementors add `unsafe impl` and uphold the contract;
  `push_index`/`pop_index` call sites are unchanged.

### Fixed

- **`pop_index`'s release-active rule-4 guard now also catches the
  self-loop shape (`next == index`) — the first RUNTIME code change to
  the crate since its documentation-hardening rounds began** (round-13
  @oh review, findings P2-1/P2-2). A contract-abiding chain can never
  link an index to itself — `push_index` stores the PREVIOUS head into
  `next[index]`, and that head is trivially already reachable — so a
  self-loop proves a caller-contract violation, of two possible causes
  (round-14 correction: the original text here concluded "proves a foreign
  writer answered", which is not the honest disjunction — the simplest
  cause needs no foreign writer at all). By far the simpler and more
  likely cause: a double-push of the index that is ALREADY the current
  head — the pushed index IS the head, so `push_index` itself writes
  `next[index] = index`; this needs no foreign writer and no shared
  storage, and the guard fires on the FIRST pop. The other: a writer
  other than a contract-abiding push answering for the popped index — in
  practice the zero-initialised backing of a second implementor value
  shared with (or stolen via `head()` from) another stack, whose `0`
  answers coincide with the popped index on the SECOND pop through it.
  That is exactly the double-issue repro shape the round-11 and round-12
  pinning tests in `tests/custom_storage_impl.rs` documented as silently
  returning a never-pushed `0` forever; both tests are now
  `#[should_panic]` — they pin the GUARD FIRING (silent corruption made
  loud), a strict improvement, and the same cold panic helper
  (`pop_link_out_of_range`) now dispatches THREE cases (self-loop;
  masked-to-empty-sentinel; masked-to-live-index). The added arm is a
  single `|| next == index` on the already-cold, release-active path; a
  real-contention audit over the actual `StackOps` code path (8 threads ×
  200,000 pop-then-repush iterations through an auditing `StackStorage`
  wrapper, mirroring `tests/threaded_conservation.rs`'s shape) observed
  1,670,492 `load_next` calls and ZERO self-loops — no false-positive
  risk under contract-abiding contention — and the loom suite stayed
  11/11 green. HONEST LIMITS, stated in the guard's own comment,
  `pop_index`'s `# Panics`, and pinned by two new tests: (a) a
  hand-crafted ACYCLIC link forgery (`links[1] = 0`, `links[0] = TAIL`)
  still double-issues silently
  (`hand_crafted_acyclic_forgery_still_double_issues`) — the guard is a
  DETECTOR for one shape, not a structural fix for the shared-storage
  hazard class; (b) SHARED LINK STORAGE between two independent stacks —
  two SEPARATE heads over one link array — double-issues with no
  detection at all (`two_stacks_sharing_link_storage_still_double_issue`:
  `b`'s push of an index chains it into both stacks; the shared chain
  stays acyclic). For (b) the fix is documentation: `push_index`'s
  `# Caller contract` now states the liveness obligation over LINK CELLS
  ("reachable from ANY stack that reads and writes the same link cells",
  not merely "the stack"), and the `StackStorage` trait's rule 3 gained
  a value-level clause — link cells must not be shared between
  implementor values whose stacks are meant to be independent — parallel
  to rule 1's instance-level one-value-per-head clause. Rule 1's
  abbreviated shape description and the trait doc's "what does NOT carry
  over" hazard-class inventory were updated to match the new behavior
  (three surviving hazard shapes, one of them now partially detected).

### MSRV

- Rust 1.81 (round-13 @oh review, finding P3-3 — re-derived from the
  workspace-inherited 1.88; the library alone needs only 1.80, but
  `tests/stack_unit.rs`'s use of `std::panic::PanicHookInfo`, stable since
  1.81, is the real floor across the full target set this crate's own
  `cargo clippy --all-targets` gate checks).
