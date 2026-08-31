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
  slot recycler): the canonical "recycle a small integer id" primitive that
  slab allocators, object pools, entity-component stores, and connection
  tables reinvent. `StackHead` is the tagged head word; custom storage
  implementors supply a `StackStorage` impl, and `push_index`/`pop_index` are
  crate-owned `StackOps` blanket-impl Treiber CAS loops over that
  implementor's single `AtomicU64` head; `ArrayIndexStack` (owned standalone)
  exposes plain `push`/`pop` forwarders. Caller contract for
  `push_index`: an index still reachable from the stack must never be pushed
  again — the push overwrites that index's link with the current head,
  closing a link-cycle that makes `pop_index` hand the same index to two
  callers — and `push_index` cannot check liveness cheaply (it would cost an
  O(n) chain walk per push), so it enforces only the `index < INDEX_MASK`
  range bound; liveness is the caller's obligation.
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
- **`TaggedIndex::try_pack(index, tag)`** — now a deprecated forwarding
  twin of the (itself checked) `pack`, kept only so existing references
  to the name keep resolving; call `pack` directly. Scheduled for removal
  in 0.2.
- **ABA-mitigating empty transition (the H-2 rule)** — when a `pop` drains the
  last element, the empty sentinel is packed with the **running tag** the
  draining pop observed, not reset to `0`: a tag reset would reopen the ABA
  window for a popper parked across a drain-and-refill. The shipped loom
  counterfactual `counterfactual_empty_transition_tag_reset_lets_aba_recur`
  proves this is load-bearing — with the tag reset restored, loom finds the
  collision.
- **Tag-width budget analysis** — the enforced `INDEX_BITS = 1..=16` cap
  guarantees every legal configuration a tag of at least 48 bits. The tag is
  GLOBAL to the whole stack, not per-slot: every successful push — of any
  index, from any thread — is a compare-exchange (a locked RMW) on the ONE
  `AtomicU64` head word, serializing on a single cache line whose exclusive
  ownership must transfer between cores, so a wrap takes
  `wrap_time = 2^TAG_BITS / aggregate_successful_push_rate` with the rate
  term bounded by hardware, not workload: cache-coherence transfer cost caps
  the aggregate rate at roughly `10^8` to `10^9` RMWs/sec no matter how many
  threads contend. At a generous `2 × 10^8` successful pushes/sec ceiling, a
  48-bit tag wraps at `2^48 ≈ 2.8 × 10^14` — a wrap every `~16` days
  (`~3.3` days even at `10^9`/sec) — and a wrap is only the PRECONDITION for
  a collision, which further requires the head line saturated at that
  ceiling continuously for the entire span AND one specific victim thread
  parked motionless, holding its stale snapshot, the whole time. Widths
  above 16 are rejected at compile time (`TaggedIndex::_CHECK_BITS`) rather
  than merely discouraged: at `INDEX_BITS = 24` the tag would be 40 bits
  (`2^40 / (2 × 10^8) ≈ 92` minutes at the same ceiling) and the pre-cap
  `INDEX_BITS = 32` maximum gave only `2^32 / (2 × 10^8) ≈ 21` seconds,
  within reach of ordinary scheduling jitter. The derivation bounds the
  recurrence window — the minimum time a victim thread must stay parked
  before its stale snapshot can recur — it does not prove recurrence
  impossible (suspending a thread is outside the crate's control), so the
  tag is documented as an ABA mitigation with a quantified bound, not an ABA
  prevention guarantee. Documented so a consumer choosing `INDEX_BITS` knows
  the trade.
- **One-implementor storage binding — `StackStorage` / `StackOps`** — the
  implementor supplies head AND links in ONE impl: `head()` alongside
  `load_next` / `store_next`, so the head↔links binding is structural,
  established once per impl. A production allocator keeps links
  **slot-resident** (an `AtomicU32` inside a slot it already owns) instead of
  paying for a second array. The blanket `StackOps` impl's CAS-loop bodies
  are crate-owned and cannot be overridden or reimplemented downstream.
  The former per-call `&Links` parameter — which allowed two `ArrayLinks`
  backings against one head, double-issuing an index (an independent
  pre-release review's release-blocking P1-1 finding) — is gone, and that
  repro no longer compiles. **`ArrayLinks<N>`** remains a public links
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
  vs 51.60 ns/op debug-only), i.e. the cost sits below the harness's noise
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
  forward to `new()`; pinned by `default_stack_behaves_like_new` /
  `default_array_index_stack_behaves_like_new` /
  `default_array_links_behaves_like_new` (`tests/stack_unit.rs`).
- **`Debug` derived on `StackHead`, `ArrayIndexStack`, and `ArrayLinks`.**

### Performance

- **Exponential backoff on `push`/`pop`'s CAS-retry arm** (`BACKOFF_SPIN_CAP =
  6`, max 64 `core::hint::spin_loop()` spins per retry, per-call `spins`
  counter never persisted across calls). Measured on the committed harness
  (`benches/tagged_index_stack_bench.rs`, x86-64, this repo's
  `[profile.bench]` — `cargo bench`'s actual profile, byte-identical to
  `[profile.release]` in this repo's `Cargo.toml` today — 8 threads =
  `available_parallelism().min(8)` on this
  machine): `contention/push_pop` 5,804,630 / 5,284,307 ops/sec (baseline, 2
  runs) → 30,049,461 / 29,850,246 / 29,993,450 ops/sec (with backoff, 3
  runs) — roughly 5.3x; `contention/churn` 2,899,902 / 2,961,754 ops/sec →
  28,450,827 / 28,101,745 / 28,633,845 ops/sec — roughly 9.7x. Single-thread
  cost stayed within run-to-run noise: `push_pop/single_thread` 50.63/51.14
  ns/op (baseline) vs 50.38/53.53/49.87 ns/op (with backoff); `churn`
  49.71/51.20 ns/op vs 50.47/50.18/50.18 ns/op. A separate ad hoc check (not
  committed — a throwaway `examples/` run, per this crate's own convention
  that a probe reproducing an already-published number does not itself need
  a permanent harness) drained the stack after 8 threads × 200,000
  contention-shaped pop/push iterations under the backoff and confirmed the
  exact multiset `0..64` came back with no duplicate or missing index. The
  loom suite (`tests/loom_aba.rs` — see its own module doc for the
  per-model breakdown) stayed green at the same wall-clock (~0.16s test
  time): `core::hint::spin_loop()` touches no loom-tracked atomic, so it
  adds no new interleaving for loom to explore.
- **`BACKOFF_SPIN_CAP = 6` kept after a dedicated throughput-vs-fairness cap
  sweep** (`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md`) — an independent
  review flagged the cap's original doc comment as claiming an unmeasured
  "low enough for LOW contention" rationale. Sweeping caps `{0, 4, 6, 8,
  10}` at 2/4/8/16 threads on the committed bench found that claim WRONG:
  cap 8 and cap 10 beat cap 6 on aggregate throughput in 15 of the 16
  thread-count x bench cells measured (one sample per cell; the real span
  is -0.4% to +58.4% — the sole exception is 4-thread churn, where cap 10
  landed 0.4% BELOW cap 6; the lowest-contention 2-thread arm specifically
  spans +17.4% to +37.3%). What the sweep found INSTEAD is a real
  fairness cost that GROWS with the cap under oversubscription: at 16
  threads on a 16-logical-CPU host, cap 6's per-thread throughput skew
  (`max/min`) averaged ~6.1x across 6 independent samples vs. ~13.1x for
  cap 8 and ~20.6x for cap 10, with cap 8/cap 10 each producing a
  single-run outlier past 19x/46x (one thread starved to a small fraction
  of its fair share). `BACKOFF_SPIN_CAP` stays `6` — a deliberate
  compromise, not a fairness optimum: fairer than caps 8/10, LESS fair
  than caps 0/4 (also measured: cap 0's min/mean beats cap 6's in 7 of 8
  arms and ties the eighth; cap 4's beats it in 6 of 8) — trading a real
  but bounded throughput ceiling against a starvation risk judged not
  worth imposing on every caller by default. Both `src/lib.rs`'s doc
  comment and this bullet now state the real throughput-vs-fairness axis
  instead of the old unmeasured low-contention-latency claim.
- **Round-8 review corrections to the two backoff bullets above**
  (`docs/reviews/2026-08-31-125420-tagged-index-stack-review-round8-oh.md`,
  findings P2-1/P2-2/P3-2/P3-3; task tis-r8-Group1 #1758). (1) The
  cap-sweep bullet's "most fairness-conscious of the caps measured" claim
  was FALSE against the sweep's own committed CSV — its fairness table had
  silently dropped caps 0 and 4, the two caps fairer than 6; corrected
  above and in `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` §3.2. (2) The
  "+17% to +58% at every thread count" range was contradicted by the same
  table's own -0.4% cell; corrected above and in the report's §3.1. (3)
  Per-CALL latency had never been measured on any axis. New
  observation-only example `examples/backoff_per_call_latency.rs` (public
  API only) measures it on a 64-element `ArrayLinks` under the crate's own
  contention discipline: at 8 threads x 200k pop-then-repush iterations
  the single worst `pop` blocked 41-60 ms across 3 runs under the shipped
  cap 6 vs 0.6-24 ms at cap 0 (median 54.5 ms vs 2.0 ms), with 60-86 pops
  over 1 ms per rep vs 0-8; at 16 threads x 200k, worst pop 130-173 ms vs
  40-46 ms, and 4/3/3 pops over 100 ms vs cap 0's 0/0/0. The full tail
  picture cuts BOTH ways — quoting only the max side is selective (all
  counts 3 reps, from the cited raw log): at 16 threads the `> 1 ms`
  cell REVERSES, cap 6 logging 285/266/249 pops over 1 ms per rep vs cap
  0's 553/661/650 (~2.4x MORE at cap 0: 650 vs 266 at the rep medians);
  the 16-thread `> 10 ms` band is roughly tied (178/131/169 vs
  110/161/157); and cap 6 is 1-2 orders of magnitude better at
  p50/p90/p99/p99.9 in EVERY shape (cap 0's p99.9 spans
  0.022-0.037 / 0.054-0.057 / 0.172-0.182 ms across the three shapes vs
  cap 6's 0.000-0.001 ms everywhere), with the same workloads 4.05-4.85x
  faster under cap 6 on median wall-clock (4.18x / 4.85x / 4.05x for the
  4x20k / 8x200k / 16x200k shapes). **`push`/`pop` are lock-free but NOT
  starvation-free**: the shipped cap trades a small number of very large
  outlier pops — worse ONLY at the extreme maximum — for better latency
  at every percentile through p99.9 AND ~4-5x better aggregate
  throughput, not "worse tail latency across the board";
  `BACKOFF_SPIN_CAP`'s doc comment now says exactly that. Raw log
  `docs/perf/_raw_tis_backoff_per_call_latency.log` plus run-3 rows in
  `TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`; derived, with in-script
  assertions, by `scripts/tis_backoff_cap_sweep_derive_report_data.mjs` —
  the same script now re-derives the sweep tables and hard-fails on the
  two corrected claim shapes (closing P3-3's uncommitted-pipeline gap).
- **One speculative perf change evaluated and declined** (unrelated to the
  backoff above): both `push`'s and `pop`'s CAS loops are
  `compare_exchange_weak` candidates, but any difference is specific to
  non-LSE AArch64 and similar `ldxr`/`stxr`-style architectures, and this
  repository has no AArch64 wall-clock/perf-gate harness to measure it.
  Revisit when one exists. (A second item originally listed here —
  relaxing `push`'s initial head load to `Relaxed` — has since LANDED; see
  the Sol-codex run-3 bullet below. Correction to this bullet's original
  wording: "neither change would show up on x86-64 or LSE AArch64" was
  accurate for the CAS-strength half but not the load-ordering half — an
  acquire load is a real ordering constraint (`ldar`/`LDAPR`) on AArch64
  with or without LSE; it is x86-64 where the two compile identically.)
- **`push`'s initial head load relaxed to `Relaxed`; link-ordering and
  CAS-strength relaxations deferred; contention-harness timing window
  fixed** (`docs/reviews/2026-08-31-162115-tagged-index-stack-review-Sol-codex-run-3.md`,
  findings P3-1/P3-2/P3-3/P3-4; task tis-sc3-Group5 #1771 + #1772).
  (1) LANDED — `push_index`'s initial head load is now `Ordering::Relaxed`
  (was `Acquire`): push uses the observed word only as `(index, tag)`
  values and never follows a link through it, so the load carries no
  ordering burden by the same proof already applied to push's `Relaxed`
  failed-CAS read. The reasoning is target-independent (happens-before
  structure, not a machine-model assumption); it was exhaustively
  model-checked by the full 11-model loom suite (`tests/loom_aba.rs`)
  staying green with exactly this ordering, and an x86-64 A/B on the
  committed contention benches confirmed expected neutrality (baseline
  28,447,805 / 27,711,456 ops/sec → 30,146,657 / 27,815,346 — machine
  noise: on x86-64 an Acquire load and a Relaxed load compile to the same
  plain load, so no timing difference exists by construction). The
  expected benefit is on weakly-ordered targets, where the change drops a
  real acquire-load ordering constraint (e.g. AArch64 `ldar`/`LDAPR` →
  plain `ldr`) — unmeasured here. `pop_index`'s orderings are deliberately
  untouched (pop DOES follow a link from the observed head word). (2)
  DEFERRED — `ArrayLinks`' link `Acquire`/`Release` (P3-1) and both CAS
  loops' strong `compare_exchange` (P3-3) stay as-is: both would-be
  relaxations target LL/SC weakly-ordered hardware this repository has no
  timing-valid harness for (CI's aarch64 row is cross/QEMU on GitHub
  runners — tests only, wall-clock-invalid), so they are documented
  in-code as deliberate defence-in-depth / pending real multi-target A/B
  measurement rather than switched blind. (3) FIXED — the contention
  harness (`benches/tagged_index_stack_bench.rs`) now times every worker
  against ONE shared `[timed_start, deadline)` window computed before the
  barrier is released, with an uncounted 300 ms warm-up phase, instead of
  each worker's own post-barrier-resume clock against the coordinator's
  separate post-barrier start (the old shape let scheduler skew
  decorrelate the summed-ops numerator's exposure window from the elapsed
  denominator). Impact on the already-published
  `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` numbers (which used the old
  harness across rounds 6-9): expected footnote-level — per-thread
  fairness ratios were computed from each worker's own full 1-second
  window count (window-duration-normalized by construction), and
  barrier-resume skew on an idle host is µs-scale against a 1 s window
  (post-fix runs still measure 1.001 s elapsed); the old totals were, if
  anything, very slightly UNDERstated, since the coordinator-to-last-join
  denominator absorbed the skew tail. No re-run or re-publication of that
  report is part of this change.

### MSRV

- Rust 1.88.
