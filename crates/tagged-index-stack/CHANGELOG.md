# Changelog

All notable changes to this project will be documented in this file.

This project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 0.1.0 - Unreleased

First release. Everything below is new in this version; nothing has shipped
before it.

### Added

- **`TaggedIndexStack<INDEX_BITS>`** — an allocation-free, `no_std`,
  `#![forbid(unsafe_code)]` lock-free LIFO free-list of small **indices** (a
  slot recycler): the canonical "recycle a small integer id" primitive that
  slab allocators, object pools, entity-component stores, and connection
  tables reinvent. `push(links, index)` / `pop(links)` are Treiber-style
  CAS loops over a single `AtomicU64` head. Caller contract for `push`: an
  index still reachable from the stack must never be pushed again — the push
  overwrites that index's link with the current head, closing a link-cycle
  that makes `pop` hand the same index to two callers — and `push` cannot
  check liveness cheaply (it would cost an O(n) chain walk per push), so it
  enforces only the `index < INDEX_MASK` range bound; liveness is the
  caller's obligation.
- **`TaggedIndexStack::is_empty()`** — an advisory emptiness check: a
  `Relaxed` peek at the head word's index half. Concurrent pushes/pops can
  make the answer stale the instant it returns, in either direction, so it
  is for diagnostics/monitoring, not correctness decisions.
- **`TaggedIndex<INDEX_BITS>`** — the packed head word: low `INDEX_BITS` bits
  carry a slot index, the high `64 - INDEX_BITS` bits a wrapping generation
  **tag** bumped on every successful push, which is what structurally defeats
  the ABA problem for every permitted width (a pop-then-re-push of the same
  index bumps the tag, so a parked CAS on the stale `(index, tag)` pair fails
  and retries). `INDEX_BITS` is a const generic capped at `1..=16` at compile
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
- **`TaggedIndex::try_pack(index, tag)`** — the checked twin of `pack`:
  `Some(word)` — with `word` exactly what `pack` returns — for an in-range
  `(index, tag)` pair, `None` instead of a silently truncated word when
  the index is `>= 2^INDEX_BITS` (where `pack` would mask it to a
  different, possibly empty-sentinel, index) or the tag is
  `>= 2^TAG_BITS` (where `pack`'s shift would silently drop the high
  bits). Exists for external callers, whom `pack` trusts to uphold the
  precondition itself; the stack's own `push`/`pop` keep calling `pack`
  directly, their inputs already guaranteed in range.
- **ABA-defeating empty transition (the H-2 rule)** — when a `pop` drains the
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
  within reach of ordinary scheduling jitter. Documented so a consumer
  choosing `INDEX_BITS` knows the trade.
- **Caller-owned links — `Links` trait** — the stack stores only the HEAD;
  each index's `next` link lives in caller storage (`load_next` /
  `store_next`), so a production allocator keeps links **slot-resident** (an
  `AtomicU32` inside a slot it already owns) instead of paying for a second
  array. **`ArrayLinks<N>`** provides an owned `[AtomicU32; N]` backing for
  standalone use. The link storage must be a DEDICATED cell, never
  payload-aliased on the popped slot's own bytes — `pop` carries a
  `debug_assert!` that fires (debug builds only) the moment a backing
  returns anything but `TAIL` or a currently-valid index, which is exactly
  what a payload-aliased backing does on every ordinary benign race.
- **Lazy link discipline (internally: RAD-1)** — links are never eagerly initialised:
  only a `push` writes a link, immediately before publishing that index as
  head. A caller whose link backing is OS-zeroed memory (a fresh `mmap`, a
  zeroed slot array) never first-touches pages merely to set up the free-list;
  links commit lazily on first push of each index. A fresh stack is therefore
  **empty** — deliberately no "start with `0..N` pushed" constructor, which
  would require exactly the eager chaining pass this discipline forbids.
- **Correct CAS orderings** — push's success ordering and pop's retry
  ordering are both chosen so a popper can never read a link through a stale
  head: pop's CAS-failure load is `Acquire` — a `Relaxed` retry could read a
  stale link, and the shipped loom counterfactual
  `counterfactual_relaxed_cas_failure_corrupts_free_list` plants exactly that
  bug and watches the free-list corrupt. Push's index-validity and
  sentinel-reservation check is a single release-active bounds check (a
  `#[cold]` panic helper, not `debug_assert!`) — one guard covers both
  conditions, and it stays enforced in release builds too.
- **64-bit-atomic portability gate** — the head is one `AtomicU64`, so the
  crate fails fast with a named `compile_error!` on targets without native
  64-bit atomics (`thumbv6m-none-eabi`, `thumbv7em-none-eabi`, `riscv32imc-…`,
  `armv5te-…`) rather
  than a cryptic unresolved-import error. `no_std`-compatible, but `no_std`
  alone does not imply `AtomicU64`.
- **Exhaustive loom model-check against the real type**: under `--cfg loom`
  the stack's atomics alias to `loom::sync::atomic`, so the shipped loom suite
  (`tests/loom_aba.rs`) model-checks the actual `TaggedIndexStack` /
  `TaggedIndex` code with NO `preemption_bound` — loom explores every
  interleaving these small models admit — with `#[should_panic]`
  counterfactuals (untagged corruption, the H-2 empty-transition tag-reset
  ABA, and the Relaxed-CAS-failure-ordering regression) proving the harness
  is non-vacuous. Several models run end-to-end through the shipped
  `push`/`pop`; most of the rest drive the real head atomic and the real
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
- **`raw_head()`** — a `#[doc(hidden)]` test-probe accessor for the packed
  head word; the attribute only excludes it from rustdoc's rendered
  navigation (it remains publicly callable), it carries no semver stability
  guarantee, and it exists for this crate's own `tests/`.
- **`pub const TAIL: u32`** — the per-slot link end-of-chain sentinel
  (`u32::MAX`), part of the `Links` contract: an implementor's backing must
  be able to represent it.
- **`Default` for `TaggedIndexStack` and `ArrayLinks`** — both forward to
  `new()`; pinned by `default_stack_behaves_like_new` /
  `default_array_links_behaves_like_new` (`tests/stack_unit.rs`).
- **`Debug` derived on `TaggedIndexStack` and `ArrayLinks`.**

### Performance

- **Exponential backoff on `push`/`pop`'s CAS-retry arm** (`BACKOFF_SPIN_CAP =
  6`, max 64 `core::hint::spin_loop()` spins per retry, per-call `spins`
  counter never persisted across calls). Measured on the committed harness
  (`benches/tagged_index_stack_bench.rs`, x86-64, this repo's
  `[profile.release]`, 8 threads = `available_parallelism().min(8)` on this
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
  loom suite (`tests/loom_aba.rs`, all 10 models) stayed green at the same
  wall-clock (~0.16s test time): `core::hint::spin_loop()` touches no
  loom-tracked atomic, so it adds no new interleaving for loom to explore.
- **Two other speculative perf changes evaluated and declined** (unrelated
  to the backoff above): `push`'s initial `head.load(Ordering::Acquire)`
  could plausibly be `Relaxed`, and both `push`'s and `pop`'s CAS loops are
  `compare_exchange_weak` candidates. Neither change would show up on
  x86-64 (`lock cmpxchg`) or LSE AArch64 (`casal`) — any difference is
  specific to non-LSE AArch64 and similar `ldxr`/`stxr`-style
  architectures, and this repository has no AArch64 wall-clock/perf-gate
  harness to measure it. Revisit when one exists.

### MSRV

- Rust 1.88.
