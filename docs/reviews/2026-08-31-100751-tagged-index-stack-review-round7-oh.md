# `tagged-index-stack` — independent publish-readiness review, round 7

- **Reviewer:** Claude (Opus 5), adversarial pass. Source, tests, bench and docs were read first and
  every finding was formed independently. The round-1..6 `-oh` reports and the two Sol-codex reports
  were read only AFTER my own findings were written down, and then only to (a) check overlap,
  (b) check whether a prior round's *remediation* left something half-done, and (c) re-verify the
  latest external report's claims against the current tree rather than inheriting them.
- **Date:** 2026-08-31 10:07:51 +0200 (CEST)
- **Revision reviewed:** `e92643f5cd250cf6d7b980d8b4dbb0cde8a3c9c0` (`main`;
  `git status --porcelain -- crates/tagged-index-stack` is empty — the crate tree is clean).
  Crate source identity: `sha256(crates/tagged-index-stack/src/lib.rs) =
  79391cd6abd5b02a394933577f3bc578a83de62a57c902aa76e5f45a4448f236`.
- **Scope:** `crates/tagged-index-stack/**` — `src/lib.rs`, all six `tests/*.rs`,
  `benches/tagged_index_stack_bench.rs`, `README.md`, `CHANGELOG.md`, `Cargo.toml` — plus the
  `.github/workflows/ci.yml` rows that cover the crate, the in-workspace consumers
  (`src/registry/heap_registry.rs`, `src/registry/bootstrap.rs`), and
  `docs/reviews/2026-08-31-090022-tagged-index-stack-review-Sol-codex.md`.
- **Machine / toolchain for every measured number:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  `cargo 1.97.0 (c980f4866 2026-06-30)`, host `x86_64-pc-windows-msvc`, 11th Gen Intel Core
  i7-11800H (8 cores / 16 threads), Windows 10 Pro 19045.
- **No file in the repository was modified.** Read-only review. Every A/B ran in throwaway trees
  OUTSIDE the repository (`D:/dev/rust/.scratch-tis-r7`, `D:/dev/rust/.scratch-tis-r7-lint`),
  deleted afterwards. The repo bench was deliberately NOT run in-tree, because
  `bench-scale-tool` writes `bench-iters.txt` into `CARGO_MANIFEST_DIR` and that file is neither
  committed nor `.gitignore`d (**P4-4**) — running it would have dirtied the tree.

## Verification actually performed

Every number below came from running something, not from reading.

| Check | Result |
| --- | --- |
| `cargo test -p tagged-index-stack --no-fail-fast` | **28 green** (18 `stack_unit`, 5 `proptest_pack_unpack`, 2 `regression_counter_wrap`, 2 `custom_links_impl`, 1 `readme_example`; `loom_aba` correctly 0; 0 doctests) |
| `cargo test -p tagged-index-stack --release --no-fail-fast` | **27 green** (the `#[cfg(debug_assertions)]`-gated test correctly disappears) |
| `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom --test loom_aba` | **10 green in 0.14 s**, all three `#[should_panic]` counterfactuals included |
| each of the 3 activation-oracle tests run ALONE (`-- --exact <name>`, 9 filtered out) | all 3 green — the direct positive control that no oracle passes on cross-test noise |
| `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `RUSTFLAGS="--cfg loom" cargo clippy -p tagged-index-stack --features loom --all-targets -- -D warnings` | clean — **but this row exists nowhere in CI** (**P3-2**) |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` | clean |
| `cargo package -p tagged-index-stack --no-verify --allow-dirty` | **16 files, 215.8 KiB (65.9 KiB compressed)**, no strays |
| out-of-tree A/B: `BACKOFF_SPIN_CAP` sweep (0/2/4/6/8/10) × {2,4,8,16} threads, 3 runs each | **P2-1** — cap 6 is 22–61 % below the measured optimum; tables in P2-1 |
| out-of-tree A/B: reload-head-after-backoff | **refuted** — 4.3× *worse* than shipped; see "Refuted hypotheses" |
| out-of-tree A/B: `#[track_caller]` removed from `push` | **refuted** — no measurable cost; see "Refuted hypotheses" |
| out-of-tree A/B: release-active `pop` rule-4 guard, interleaved 4×2 runs | **≈ 0 ns** (50.58 vs 51.60 ns/op median, guard arm *faster*) — **P3-1** |
| out-of-tree probe: does clippy's deny-by-default `let_underscore_lock` fire on a *tuple-nested* `_`? | **yes, it errors** — which is why **P3-2** matters |
| CHANGELOG `### Performance` numbers reproduced | **CONFIRMED** — see "CHANGELOG reproduction" |

---

## Overall verdict: **CONDITIONAL-GO**

**The shipping algorithm is correct.** I re-derived, rather than inherited, the load-bearing claims:

- **Tag monotonicity along the head's modification order holds.** Every successful `push` does
  `tag.wrapping_add(1)` (`src/lib.rs:865`); every successful `pop` re-packs the tag it observed,
  including on the drain-to-empty branch (`src/lib.rs:1001-1006`). A `(X, t)` head word can
  therefore only recur after a full `2^TAG_BITS` wrap whose last push re-installs `X`.
- **`pop`'s `Acquire`-success-without-`Release` ordering is still sound**, because its premise
  still holds: I re-grepped `head` for a plain `store` and found none — `new` is initialization,
  `raw_head`/`is_empty` only load, and all three writers (push's `Release` CAS, pop's `Acquire`
  CAS, the loom-only `cas_head_for_test`) are RMWs, so the release sequence headed by any push's
  `Release` CAS is unbroken. The `INVARIANT` block on the private field (`src/lib.rs:666-689`)
  states exactly this premise and is load-bearing.
- **The backoff added in round 6 does not touch any of that.** `core::hint::spin_loop()` has no
  memory-model effect, it sits strictly inside the `Err(actual)` arm after `head = actual`, and
  `spins` is a per-call `let mut` declared before each loop (`src/lib.rs:842`, `:971`) — verified
  by reading, not by trusting the comment. **The backoff is correct.**
- **The wrap-time bound is conservative by an order of magnitude on this machine.** The docs adopt
  a "generous `2 × 10^8` successful pushes/sec" ceiling; the highest aggregate rate I could
  produce on the committed harness across *any* arm and any thread count is 37.5 M ops/sec
  ≈ 1.9 × 10^7 successful pushes/sec — 10× under the doc's own working ceiling. The 48-bit-floor
  argument survives a fortiori.
- **`_CHECK_BITS` is unbypassable**, `try_pack`'s `1u64 << TAG_BITS` never reaches the `<< 64`
  boundary (`TAG_BITS ∈ [48, 63]`), and all three `#[should_panic]` loom counterfactuals are real.

**What holds this back from an unconditional GO is two P2s**, both about the round-6 remediation
itself rather than about the algorithm: the tuning constant that remediation chose ships with a
published rationale that measurement contradicts and sits 22–61 % below the optimum on the very
harness it was tuned on (**P2-1**), and the real-thread correctness evidence for that remediation
is, by its own commit message, uncommitted and unreproducible — the crate still has **zero**
committed multi-threaded tests (**P2-2**).

**Findings: 0 × P0, 0 × P1, 2 × P2, 7 × P3, 7 × P4.**

---

## Special focus 1 — is `MODEL_LOCK` acquisition really structural now?

Audited function by function, because this defect class was introduced twice in a row by the same
file's own remediation (round 4 scoped the lock to `pop`'s oracle only; round 5's remediation then
added `push_push_conservation`'s oracle outside that scope).

**Result: yes, for every test in the file today, and yes by construction — with one residual
loophole the helper cannot close and one that a lint would close if the lint ever ran.**

`grep -n 'Builder::new'` over `tests/loom_aba.rs` returns exactly two hits, both inside the helpers
(`:147` in `model`, `:173` in `model_with_oracle`); `loom::model(` appears nowhere. Every one of
the ten `#[test]` functions reaches its model through one of the two helpers:

| # | Test | Route | Lock window |
| --- | --- | --- | --- |
| 1 | `aba_repush_keeps_free_list_conservation` (`:200`) | `model` (`:201`) | whole `check()` |
| 2 | `counterfactual_untagged_head_lets_aba_corrupt_free_list` (`:312`) | `model` (`:319`) | whole `check()` — deliberately included though it drives no crate code, with an explicit comment saying so |
| 3 | `tagged_stack_survives_the_same_resurrection_pattern` (`:407`) | `model` (`:408`) | whole `check()` |
| 4 | `pop_empty_transition_preserves_tag` (`:614`) | `run_h2(true)` → `model` (`:502`) | whole `check()` |
| 5 | `counterfactual_empty_transition_tag_reset_lets_aba_recur` (`:622`) | `run_h2(false)` → `model` | whole `check()` |
| 6 | `pop_retry_after_failed_cas_…_real_type` (`:654`) | `model_with_oracle` (`:666`) | **snapshot → check → assert**, guard bound to `_g` |
| 7 | `cas_retry_path_must_acquire_with_concurrent_push` (`:813`) | `run_cas_retry(Acquire)` → `model` (`:719`) | whole `check()` |
| 8 | `counterfactual_relaxed_cas_failure_corrupts_free_list` (`:822`) | `run_cas_retry(Relaxed)` → `model` | whole `check()` |
| 9 | `push_push_conservation` (`:851`) | `model_with_oracle` (`:852`) | snapshot → check → assert |
| 10 | `pop_pop_conservation` (`:932`) | `model_with_oracle` (`:933`) | snapshot → check → assert |

Three further checks I ran instead of assuming:

1. **Lock-before-snapshot ordering** is structural now, not per-test: `model_with_oracle` acquires
   the guard on its own first line (`:171`) and only then calls `snapshot()` (`:172`). A test
   cannot get this order wrong any more, because it no longer writes the order.
2. **Poisoning** is handled at both helpers (`unwrap_or_else(|e| e.into_inner())`), which matters
   because three lock-holders are `#[should_panic]` and unwind *through* the guard. Verified: the
   suite is green, no poisoned-mutex cascade.
3. **The oracles are not passing on cross-test noise.** I ran all three counter-reading tests
   ALONE. All three pass. Each model genuinely drives its own retry branch.

**The residual loopholes — what the helper does NOT close:**

- **(a) The caller must still bind the returned guard.** `model_with_oracle` returns
  `(T, MutexGuard<'static, ()>)` and the exclusivity of the post-`check()` delta depends on the
  caller writing `let (before, _g) = …` and not `let (before, _) = …` — the latter drops the guard
  at the end of that `let` statement, silently reopening exactly the round-4/round-5 hole. Nothing
  in the signature enforces it (`MutexGuard` is not `#[must_use]`, and a tuple return kills any
  `#[must_use]` on the function). **I probed whether a lint catches it and it does**: clippy's
  `let_underscore_lock` is deny-by-default and *does* fire on a tuple-nested `_`
  (`error: non-binding let on a synchronization lock`, verified in
  `D:/dev/rust/.scratch-tis-r7-lint`). So the hole is lint-catchable — **but the lint never runs on
  this file** (P3-2). The airtight version is to move the delta assertion inside the helper (take a
  `verify: impl FnOnce(T, T)` or just assert `after > before` in-helper) so nothing crosses the
  helper boundary while the window is open.
- **(b) The helper is a convention, not an enforcement.** An eleventh test can still call
  `loom::model(…)` or `Builder::new().check(…)` directly. This repo already has the tool for
  exactly this (`tests/ci_clippy_matrix_consistency.rs` pins a generated artifact against its
  source): a ~10-line non-loom test doing
  `assert_eq!(include_str!("loom_aba.rs").matches("Builder::new()").count(), 2)` would make the
  routing self-checking in the ordinary `cargo test` run.
- **(c) Non-reentrancy is now a live footgun, not a theoretical one.** `model_with_oracle` hands
  back a *still-held* non-reentrant guard whose scope runs to the end of the test function; a
  future test that calls `model(…)` after it self-deadlocks. The `model` doc mentions
  non-reentrancy (`:138-141`) but frames it as a reason the two helpers are separate, not as a
  hazard the returned guard creates.

None of (a)–(c) is a defect *today*. (a) and (b) are folded into **P3-2**; (c) is noted here only.

## Special focus 2 — independent assessment of the CAS backoff

### Correctness: clean

`spins` is a per-call local (`src/lib.rs:842` in `push`, `:971` in `pop`), declared before the loop
and never persisted anywhere — I checked for any `static`/field/thread-local carrier and there is
none, so the doc's "starts fresh every call, never persisted" is exactly true.
`core::hint::spin_loop()` is a scheduling hint with no memory-model semantics, placed strictly
after `head = actual` inside the `Err` arm, so it cannot affect atomicity, ordering, or the
release-sequence argument `pop`'s `Acquire`-only success ordering rests on. Lock-freedom is
preserved (system-wide progress still holds: some thread's CAS always succeeds); wait-freedom was
never claimed. The loom suite is green at the same wall-clock, which is expected — `spin_loop`
touches no loom-tracked atomic and adds no interleaving.

The one real correctness nit is the *unbounded* `spins += 1` (**P3-3**).

### CHANGELOG reproduction: CONFIRMED

Committed-bench shape, 8 threads, out-of-tree copy of the crate under this repo's
`[profile.release]`/`[profile.bench]` (`lto = "thin"`, `codegen-units = 1`), median of 3 runs:

| row | CHANGELOG baseline | my baseline | CHANGELOG w/ backoff | my cap 6 | published ratio | my ratio |
| --- | --- | --- | --- | --- | --- | --- |
| `contention/push_pop` | 5.80 / 5.28 M | **5.88 M** | 30.05 / 29.85 / 29.99 M | **28.63 M** | ~5.3× | **4.9×** |
| `contention/churn` | 2.90 / 2.96 M | **2.63 M** | 28.45 / 28.10 / 28.63 M | **26.93 M** | ~9.7× | **10.2×** |
| `churn` single-thread | 49.71 / 51.20 ns | **52.01 ns** | 50.47 / 50.18 / 50.18 ns | **52.62 ns** | noise | noise |

The published multipliers and the zero single-thread cost both reproduce. The absolute figures sit
~5 % low across the board, consistent with a scratch build and a warmer machine.

### The choice of `BACKOFF_SPIN_CAP = 6`: not supported by any measurement — see **P2-1**

---

## Findings

### P2-1 — `BACKOFF_SPIN_CAP = 6` ships a published rationale that measurement contradicts, and sits 22–61 % below the measured optimum in the crate's own benchmark

`src/lib.rs:254-264` documents the constant as:

> Capped at 6 (max 64 spins/retry): high enough to materially reduce head-cache-line ping-pong
> under contention, low enough that a spurious retry under LOW contention doesn't stall the one
> thread that lost the CAS for longer than the win is worth.

Both halves are presented as if measured. Neither is. The committed evidence
(`CHANGELOG.md:150-169`, commit `069d187`) compares exactly two arms — no backoff vs cap 6 — at
exactly one thread count (8). No other cap was ever run; no low-contention arm was ever run; the
`docs/perf/` tree contains no gate report, raw log or summary CSV for this decision (**P3-6**).

**Measured (out-of-tree copy of the shipped code, only the cap constant and the retry-arm body
switched by a Cargo feature; committed bench shape; median of 3 runs at 8 threads):**

| arm | `contention/push_pop` | `contention/churn` | single-thread `churn` | per-thread skew (max/min) pp / churn |
| --- | --- | --- | --- | --- |
| no backoff | 5.88 M | 2.63 M | 52.01 ns | 1.20 / 1.16 |
| cap 2 | 4.96 M | 3.41 M | 57.80 ns | 1.10 / 1.28 |
| cap 4 | 15.04 M | 11.57 M | 53.74 ns | 1.44 / 1.46 |
| **cap 6 (shipped)** | **28.63 M** | **26.93 M** | **52.62 ns** | **3.62 / 1.65** |
| cap 8 | **34.81 M** (+22 %) | **34.31 M** (+27 %) | 51.71 ns | 3.79 / 2.77 |
| cap 10 | **35.92 M** (+25 %) | **35.91 M** (+33 %) | 52.18 ns | 4.15 / **28.5** |

**Thread-count sweep** (median of 2 runs; M ops/sec, `push_pop` / `churn`):

| threads | no backoff | cap 6 (shipped) | cap 8 | cap 10 |
| --- | --- | --- | --- | --- |
| 2 | 19.42 / 17.38 | 36.92 / 36.64 | 37.38 / 37.52 | 37.31 / 37.36 |
| 4 | 7.93 / 3.76 | 34.28 / 33.49 | 36.44 / 36.25 | 37.51 / 37.41 |
| 8 | 5.88 / 2.63 | 28.63 / 26.93 | 34.81 / 34.31 | 35.92 / 35.91 |
| 16 (oversubscribed) | 5.56 / 2.36 | 22.65 / 19.70 | **32.36 / 31.67** (+43 % / +61 %) | 35.19 / 35.07 |

Three things follow, and each falsifies a piece of the shipped rationale:

1. **The "low enough for LOW contention" half is unsupported and, as far as anything measurable
   here goes, false.** At 2 threads — the lowest contention this harness can produce — cap 8 and
   cap 10 are equal-to-better than cap 6 on throughput (37.4/37.5 vs 36.9/36.6) and cap 8's
   fairness is still 1.3/2.0. There is no measured low-contention regime in which cap 6 beats a
   larger cap. The genuine "no cost" result is the *single-threaded* one (the backoff branch is
   never taken uncontended, so every cap measures 51–53 ns) — and that is cap-independent, so it
   cannot justify choosing 6.
2. **Cap 6 is not on a plateau.** The curve is still climbing steeply at 6: +22 %/+27 % at 8
   threads and +43 %/+61 % at 16 threads for a one-character change. Under oversubscription — the
   regime the docs implicitly worry about — the shipped cap is where the crate degrades *most*
   (28.63 → 22.65 M from 8 to 16 threads), while cap 8 barely moves.
3. **The real limiting axis is fairness/starvation, and it is documented nowhere.** The backoff
   trades per-thread fairness for aggregate throughput and the effect is large and measurable:
   without backoff, per-thread op counts differ by ≤ 1.2×; at the shipped cap 6 the
   `contention/push_pop` spread is **3.6×** (the slowest thread does 44 % of the mean); at cap 10 a
   thread is effectively starved (worst observed max/min = **9247×** at 8 threads, **52 607×** at
   4 threads — a thread that did essentially nothing for a full second). That is the honest reason
   not to set the cap to 10, and it is exactly the reason the docs do not give.

**Failure scenario.** A downstream consumer reads `BACKOFF_SPIN_CAP`'s rustdoc (it is a
crate-private const, but the identical claim is in `CHANGELOG.md:150-158`, which ships to
crates.io) and takes "6 is the tuned optimum, the constraint is low-contention latency" as the
crate's measured position. Both premises are wrong: 6 was never compared against anything but 0,
and the binding constraint is thread starvation, which the crate never mentions. A consumer whose
workload needs bounded per-operation latency (an allocator slow path on a latency-SLA thread) gets
no warning that this primitive now deliberately starves losers, and a consumer chasing throughput
leaves 22–61 % on the table believing the constant was optimised.

**What it needs before it can land.** Not necessarily a different constant — cap 6 is a defensible
*fairness-conscious* choice, and I would not object to keeping it. What is not defensible is
shipping it with an unmeasured rationale. The minimum resolution is: (a) run the cap sweep on the
committed harness (my numbers are from a scratch tree), (b) state the actual decision axis —
throughput vs per-thread starvation — with the fairness numbers next to the throughput numbers,
and (c) if 6 is kept, say "6 was chosen over 8/10 because the skew at 8+ is N×", which is a real
sentence backed by a real measurement. Per this repo's own R22-14 rule that also means a
`docs/perf/` report with the raw logs and a summary CSV (**P3-6**).

---

### P2-2 — the crate has zero committed multi-threaded tests, and the backoff's only real-thread correctness evidence is explicitly uncommitted

`grep` for `std::thread`/`thread::scope` across `crates/tagged-index-stack/tests/` returns exactly
two hits, both in `stack_unit.rs:266,272` — and both are `std::thread::current().id()` inside the
`#[track_caller]` panic-hook test, i.e. single-threaded. Every other concurrency test is a loom
model. Loom's models are exhaustive but tiny: at most 2 spawned threads, at most 2 seeded indices,
`ArrayLinks<1>`/`<2>`, and — relevant here — schedules in which the retry arm executes a handful of
times, never enough for `spins` to climb past 2 or 3.

The commit that landed the backoff (`069d187`) says so itself:

> Ad hoc conservation check (throwaway `examples/` run, **not committed**): 8 threads × 200,000
> contention-shaped pop/push iterations under the backoff, then drained and confirmed the exact
> multiset `0..64` came back with no duplicate or missing index.

`CHANGELOG.md:161-166` repeats it and adds a justification — "per this crate's own convention that
a probe reproducing an already-published number does not itself need a permanent harness". That
justification does not apply: this probe was not reproducing a published *number*, it was the sole
correctness evidence for a change to the concurrency-critical retry path, and this repo's actual
convention (CLAUDE.md, R22-14 / the R21-2 precedent) is the opposite — the R21-2 throwaway probe was
*promoted* to a committed `examples/` file precisely so a documented reproducibility gap would not
be left standing.

**Failure scenario.** A future change to the retry loop — raising the cap per P2-1, adding jitter
per P4-5, switching to `compare_exchange_weak`, or a `Links` implementor's own refactor — is
validated by `cargo test` (single-threaded), `cargo test --release` (single-threaded) and the loom
suite (2 threads, 2 indices). None of those exercises a chain longer than 2, a tag that advances
more than a handful of times, or a retry loop that reaches the backoff cap. A conservation defect
that only manifests with 64 indices and 8 real threads ships green. The evidence that this is not
hypothetical is that the maintainer felt the need to write that probe at all — and then threw it
away.

**Fix.** Promote the described probe to a committed test: `tests/threaded_conservation.rs`,
`#![cfg(not(loom))]`, N threads × M iterations of the bench's own pop-then-repush-exactly-what-you-
popped discipline, then drain and `assert_eq!` the exact multiset. Sized for CLAUDE.md's
"short scenario by default" (~0.1 s, e.g. 4 threads × 20 000), it costs nothing and it is also the
one test that would give a future `cargo miri test` / TSan row something to find — today the crate
has neither, and with no threaded test there would be nothing for them to look at.

---

### P3-1 — `pop`'s rule-4 guard is debug-only while `push`'s guard for the same corruption class is release-active, and I measured the cost of closing that asymmetry at ≈ 0 ns

`push` rejects an out-of-range index with an unconditional, release-active check
(`src/lib.rs:836-838` → `#[cold] push_index_out_of_range`), and its own doc explains why it is not a
`debug_assert!` (`src/lib.rs:819-825`):

> this is a caller-contract violation checked unconditionally, not a `debug_assert!`, because the
> failure mode is silent free-list corruption rather than a merely-suboptimal fallback.

`pop`'s rule-4 check (`src/lib.rs:989-1000`) guards the *identical* failure class — a `next` that is
neither `TAIL` nor `< INDEX_MASK` is silently truncated by `pack`, either to a live index
(double-issue) or to the empty sentinel (the whole remaining chain leaks at once, no panic, no
`None` anomaly) — and is a `debug_assert!`, justified as "release builds pay nothing".

I measured what a release-active version costs. Out-of-tree, `pop` gained
`if next != TAIL && (next as u64) >= INDEX_MASK { cold_panic() }` before the `pack`, with the panic
in a `#[cold] #[inline(never)]` helper exactly like `push`'s. Interleaved A/B, 4 pairs of runs to
cancel thermal drift, single-threaded `churn` (the pop-heaviest row):

| | run 1 | run 2 | run 3 | run 4 | median |
| --- | --- | --- | --- | --- | --- |
| shipped (debug-only) | 51.48 | 50.83 | 51.88 | 51.71 | **51.60 ns/op** |
| release-active guard | 49.89 | 50.78 | 51.48 | 50.37 | **50.58 ns/op** |

The guarded arm is *faster* at the median — i.e. the cost is below this harness's noise floor
(≈ ±1.5 ns), which is unsurprising next to two `lock cmpxchg` per iteration. "Release builds pay
nothing" is true of the current `debug_assert!`; it is equally true of the check it is avoiding.

**Failure scenario.** A `Links` implementor whose backing can return a stale/foreign value — the
payload-aliased layout the `Links` doc spends 30 lines warning against, or simply an off-by-one in
an index mapping — gets a loud panic in `cargo test` and total silence in the release binary their
users run, where the same value quietly truncates to the empty sentinel and leaks every remaining
slot. That is the exact asymmetry `push`'s own doc argues against.

I am rating this P3 rather than P2 (the external Sol-codex report rates it P2) because it is
non-breaking to add later — an added panic on input that is already a documented contract violation
does not need a major version — and the one in-workspace consumer (`RegistryLinks`,
`src/registry/heap_registry.rs:576-600`) structurally cannot produce an out-of-range `next`. But the
stated cost objection is now measured away, so if it is declined again it should be declined on some
other ground.

---

### P3-2 — `tests/loom_aba.rs` gets **zero** lint coverage, and the deny-by-default lint that would close `model_with_oracle`'s residual loophole is exactly the one that never runs

`tests/loom_aba.rs` is `#![cfg(loom)]` (`:94`). CI's only clippy row for this crate
(`.github/workflows/ci.yml:1985`) is `cargo clippy -p tagged-index-stack --all-targets -- -D
warnings` — **without** `--cfg loom` and without `--features loom` — so the whole file compiles to
nothing there. `grep -n clippy .github/workflows/ci.yml | grep -i loom` returns nothing: there is no
clippy row anywhere in the workflow that sets the cfg. 508 lines of test code (and the crate's own
`#[cfg(loom)]` lib code: `cas_head_for_test`, both retry counters, both counter increments inside
the hot retry arms) are never linted.

This is not abstract. Special focus 1(a) above identifies the one loophole the `model_with_oracle`
helper cannot close by construction: a future test writing `let (before, _) = model_with_oracle(…)`
drops the guard immediately and silently reopens the round-4/round-5 hole. I probed whether a lint
catches that shape and **it does** — clippy's `let_underscore_lock` is deny-by-default and fires on
a tuple-nested `_`, not just on a top-level `let _ =`:

```text
error: non-binding let on a synchronization lock
  --> src\main.rs:14:13     // this is `let (b, _) = helper();`
   = note: `#[deny(let_underscore_lock)]` (part of `#[deny(let_underscore)]`) on by default
```

So the repository already owns the mechanism that makes round-6's structural remediation airtight —
and does not point it at the file. I ran the missing row by hand
(`RUSTFLAGS="--cfg loom" cargo clippy -p tagged-index-stack --features loom --all-targets -- -D
warnings`): **clean today**, so this is a coverage gap with no current instance, not a latent bug.

**Failure scenario.** Round 8 adds an eleventh model with an activation oracle, binds the returned
guard to `_` (or calls `Builder::new().check()` directly), and the suite stays green because the
counter delta is satisfiable by another test's increments. Both mistakes are mechanically
detectable — the first by the lint above, the second by a 10-line `include_str!` count assertion of
the kind `tests/ci_clippy_matrix_consistency.rs` already establishes as this repo's pattern —
and neither is checked.

**Fix.** Add the `--cfg loom` clippy row to the `loom` job (one line, next to the existing loom test
invocation at `ci.yml:2468-2470`), and move the oracle's delta assertion inside
`model_with_oracle` so the guard never crosses the helper boundary at all.

---

### P3-3 — `spins += 1` is unbounded, so a long-starving `push`/`pop` panics on integer overflow in any debug build, and `push`'s `# Panics` doc no longer lists every way it can panic

`src/lib.rs:908-911` (push) and `:1033-1036` (pop):

```rust
for _ in 0..(1u32 << spins.min(BACKOFF_SPIN_CAP)) {
    core::hint::spin_loop();
}
spins += 1;
```

`spins` is only ever consumed through `.min(BACKOFF_SPIN_CAP)`, so every increment past 6 is
semantically dead — but it keeps happening, and at `u32::MAX` it is an `attempt to add with
overflow` panic in any profile with `overflow-checks` on (the dev/test default for every downstream
consumer). Reaching it needs ~4.3 × 10^9 consecutive lost CASes inside ONE call, which at the capped
64 `pause`es per retry is on the order of tens of minutes of uninterrupted starvation — remote, but
"remote" is the same word that applies to the ABA tag wrap this crate spends 60 lines bounding, and
unlike the tag wrap this one is closed by three characters: `if spins < BACKOFF_SPIN_CAP { spins += 1 }`
(or `saturating_add`).

Two smaller companions, same three lines:

- `push`'s `# Panics` section (`src/lib.rs:819-832`) enumerates its panic sources — the
  `INDEX_MASK` guard, plus the `Links` layer's own bound — and is now incomplete; `pop` has no
  `# Panics` section at all yet can panic the same way (and through `load_next`).
- Nothing pins `BACKOFF_SPIN_CAP < 32`. The shift `1u32 << spins.min(BACKOFF_SPIN_CAP)` is a debug
  panic / masked shift in release the moment someone edits the constant to 32 while chasing P2-1.
  A `const _: () = assert!(BACKOFF_SPIN_CAP < 32);` next to the constant costs one line and makes
  that a compile error — the same technique `_CHECK_BITS` already uses two hundred lines above.

---

### P3-4 — the README's hidden-API inventory contradicts itself four lines later: it says "two more" / "all four", then lists three

`README.md:182`:

> This crate has two `#[doc(hidden)]` `pub` items under default features, **and two more that only
> exist under `--cfg loom`**. In **all four cases** the attribute only hides the item…

`README.md:186` then lists **three** loom-only items — `cas_head_for_test`,
`pop_retry_count_for_test`, `push_retry_count_for_test` — which is correct against the source
(`src/lib.rs:1093-1095`, `:1149-1152`, `:1193-1196`, plus `raw_head` `:1070` and
`TaggedIndex::empty` `:439` under default features). The real inventory is 2 + 3 = **5**.

`push_retry_count_for_test` was added in round 5 and `cas_head_for_test` gained its
`#[doc(hidden)]` in round 6 (`ee522c0`, P4-1); the sentence counting them was updated in neither.

**Failure scenario.** This paragraph is on the crates.io landing page and is the crate's only
statement about which symbols carry no semver guarantee. A consumer auditing "what am I allowed to
depend on" counts four, finds five `pub` symbols missing from the rendered docs, and cannot tell
which one the README forgot — or concludes the README is describing a different version.

This is the same *stale-enumeration* class round 5 fixed three instances of (its P3-2/P3-4/P3-5)
and round 6 fixed two more (P3-3/P3-4). The fix those rounds converged on applies verbatim: delete
the count, keep the per-item description that follows it. Note the external Sol-codex report also
found this one; I verified it independently against the source before reading that report.

---

### P3-5 — the crate's headline metric has no regression track, no committed baseline, and no fairness statistic — and the fairness evidence was printed on every run since the backoff landed

Since `069d187` the crate's advertised value is a contended-throughput multiple ("~5.3×–9.7×",
`CHANGELOG.md:150-160`). The rows that produce it live entirely outside `bench-scale-tool`'s
`Harness` (`benches/tagged_index_stack_bench.rs:101` onward), so:

- they are not in `bench-iters.txt` and nothing regression-tracks them;
- they emit no summary statistic beyond aggregate ops/sec and a raw per-thread `Vec` dump;
- there is no assertion of any kind on the throughput or its distribution.

Round 6 raised the tracking half as P4-4 and the remediation (`e70b2d9`) fixed only the
`--calibrate` sub-item. The consequence is concrete: the harness has been printing
`Per-thread breakdown: [...]` under every measurement in this review series, and that array is
where the backoff's 3.6× fairness skew (P2-1) was visible the whole time. It was never read,
because the harness never reduces it to a number and no report ever cited it.

**Fix (cheap):** print `max/min` and `min/mean` next to the ops/sec line — two lines of arithmetic
on data the bench already has — so the fairness axis is in the output a reviewer actually reads, and
add the two contention rows' figures to whatever artifact P3-6 produces.

---

### P3-6 — the `perf(runtime)` backoff decision produced no `docs/perf/` artifact at all, contrary to this repo's own R22-14 rule, and P2-1 is the direct consequence

CLAUDE.md's boundary rule is explicit: *any* report whose verdict rests on measured numbers owes
raw logs plus a summary CSV, "regardless of whether the measurement came from criterion/iai, a
`paired-ab-runner.mjs` process-level judge, or an ad-hoc probe built for a single one-off question".
Commit `069d187` is a `perf(runtime)` commit whose verdict rests entirely on measured numbers.
`ls docs/perf/ | grep -i tagged` returns nothing; there is no gate report, no `_raw_*.log`, no
`*_summary.csv`, and no immutable source identity (the R29-6 rule) for the tree those numbers were
measured on. The entire evidentiary record is five lines in a commit message and twenty in a
CHANGELOG.

This is not a bookkeeping complaint. The rule exists to force the shape of evidence that catches
exactly what P2-1 found: a summary CSV has one row per arm, which makes "we ran two arms and picked
a constant" visibly incomplete, and a path/config-evidence discipline would have made the
missing low-contention and cap-sweep arms obvious before the constant shipped. The crate is also
one CI job away from having no reproducible artifact at all, since `bench-iters.txt` is not
committed (**P4-4**), so even the iteration counts behind the published ns/op figures are not
recoverable.

---

### P3-7 — `src/lib.rs` is still 74.8 % comment lines; round 6's trim moved the ratio by 2 %, and the single largest remaining block is a rationale paragraph duplicated four times

Measured on the current file: **1198 lines, 38 blank, 896 comment, 264 code — 74.8 % of all lines
(77.2 % of non-blank), 3.39 comment lines per line of code.** Round 6 measured 3.46 and raised it as
P3-5; the remediation (`c34bbe5`, "trim three over-long doc passages") moved it to 3.39 while the
file grew 36 lines net. For calibration, the same reviewer series set an explicit "target ≤ 55 %"
for the sibling `size-classes` crate.

The mechanically removable part is now easy to name. `#[doc(hidden)]` appears 15 times; the phrases
"semver stability guarantee" and "test-only-forwarder convention" appear 3 and 4 times, in four
near-verbatim ~8-line paragraphs (`src/lib.rs:426-438`, `:1064-1069`, `:1082-1088`, `:1134-1140`,
`:1178-1184`) that say the same thing — the attribute hides it from rustdoc only, it stays
callable, no semver guarantee, cf. `raw_head`. Three of the four already end with "cf. `raw_head`",
which is the proof that one canonical statement plus a one-line cross-reference is sufficient. That
is ~30 lines of pure duplication in the crate's only source file.

**Failure scenario** is the one every round has paid for: the same fact written in four to six
places with no mechanical link between copies is why P3-4 (and round 5's three, and round 6's two)
keep recurring. This is the root cause of the stale-enumeration class, not a style preference.

---

### P4-1 — `CHANGELOG.md` hardcodes "all 10 models", against the loom file's own explicit no-count policy

`CHANGELOG.md:167`: "The loom suite (`tests/loom_aba.rs`, all 10 models) stayed green…". The loom
module doc (`tests/loom_aba.rs:14-16`) declares itself "the source of truth for this per-model
breakdown" and says the other published copies "point back here rather than repeating a specific
count" — which the crate rustdoc and README both honour ("Several models…"). The `### Performance`
section added in round 6 reintroduced exactly the count the policy exists to prevent. (10 is correct
today.)

### P4-2 — `contention/push_pop` has no seed-collision assert, while `contention/churn` has the analogous one

`benches/…:195` computes each worker's seed as `thread_id * LINKS_SIZE / num_threads`. Distinct for
every `num_threads ≤ LINKS_SIZE`, and today `num_threads ≤ 8` — but nothing says so. Phase 2 guards
its analogous invariant explicitly (`assert!(num_threads <= prefill_count, …)`, `:291-294`). If the
`min(8)` cap is ever raised past 256 (or the constant lowered), two workers seed the SAME index, and
that is a double-push of a live index — the free-list closes into a cycle and the row silently
measures a corrupted structure at inflated throughput. One `assert!(num_threads <= LINKS_SIZE)` next
to the existing one.

### P4-3 — `pop` backs off before discovering the stack is empty

`src/lib.rs:1030-1037`: on a lost CAS, `pop` sets `head = actual`, spins up to 64 `pause`es, then
loops back to the top where `is_empty(head)` may immediately `return None`. The spin is pure latency
on a path that is about to do no work at all. Moving the emptiness test above the backoff (or
skipping the backoff when `is_empty(actual)`) is two lines. `contention/push_pop` is exactly the
workload where `None` returns are common (8 indices, 8 threads).

### P4-4 — running the bench in a clean checkout dirties the repo, and `cargo package` then needs `--allow-dirty`

`bench-scale-tool` writes its iteration manifest to `CARGO_MANIFEST_DIR`
(`benches/…:27`), i.e. `crates/tagged-index-stack/bench-iters.txt`. That file is not committed
(`git log --all -- crates/tagged-index-stack/bench-iters.txt` is empty) and matches nothing in
`.gitignore`, so the invocation the crate's own docs tell readers to run
(`README.md:117`, `src/lib.rs:143-145`: "re-run `cargo bench -p tagged-index-stack --bench
tagged_index_stack_bench` for a fresh sample") leaves an untracked file behind. I confirmed
out-of-tree that the harness JIT-calibrates and then writes the manifest when it is absent, so
the ns/op figures the docs cite are not even reproducible against the same iteration counts. Either
commit the manifest (the root crate's own `bench-iters.txt` *is* committed) or `.gitignore` it —
the current state is the only one that is wrong both ways.

### P4-5 — the backoff has no jitter, so all losers retry in lockstep

Every loser spins exactly `1 << n`, so threads that collide on iteration *n* wake up at the same
offset and collide again; textbook exponential backoff randomises the delay for precisely this
reason. This is the mechanism behind P2-1's fairness numbers (a thread that keeps losing keeps
losing) and is not mentioned in either the constant's doc or the CHANGELOG. Cheap deterministic
jitter is available in `core` without a dependency (e.g. mixing the low bits of the failed CAS's
`actual` word, which every retry already holds, into the spin count). Worth a measurement, not
necessarily a change.

### P4-6 — the README's "Notes" section is three unwrapped 300+-character lines

`README.md:182,184,186` are single long lines in a file otherwise wrapped at ~78 columns. Cosmetic,
but it is the section a consumer reads to learn what is and is not stable API (see P3-4).

### P4-7 — `BACKOFF_SPIN_CAP`'s doc cites the wrong profile

`src/lib.rs:257-258` attributes its numbers to "this repo's `[profile.release]`"; `cargo bench`
builds with `[profile.bench]`. The two are byte-identical in this repo (`lto = "thin"`,
`codegen-units = 1`), so no number changes — but a reader reproducing the measurement is pointed at
the wrong knob.

---

## Refuted hypotheses (measured, and the answer was "no")

Recording these because a negative result is worth as much as a finding, and each of these is an
"obvious" optimisation a later round would otherwise re-propose.

1. **Re-read the head after backing off, instead of re-CASing the pre-spin `actual`.** My first
   read of `src/lib.rs:903-911` flagged this as a defect: the code captures `actual`, spins for up
   to ~64 `pause`es, then CASes with a value that is by then almost certainly stale, so the retry
   is a doomed exclusive-ownership acquisition — and textbook backoff re-reads before retrying.
   **Measured: it is 4.3× WORSE** (8 threads, cap 6: 6.63 M vs 28.63 M `push_pop`, 6.46 M vs
   26.93 M `churn`) — barely above the no-backoff baseline. The mechanism is clear in hindsight and
   worth writing down: the backoff's entire benefit is that a losing thread stops *touching the
   line*, leaving it undisturbed in the winner's cache; adding a load back puts the coherence
   traffic back and cancels the win. **The shipped code is right and my hypothesis was wrong.**
2. **`#[track_caller]` on `push` costs something on the hot path.** Removing it out-of-tree:
   51.64/51.62/51.99 ns/op with, 52.86/52.83/54.27 ns/op without — no measurable cost, wrong
   direction if anything. The round-6 P3-6 attribute is free; leave it.
3. **A release-active `pop` rule-4 guard would cost throughput.** Measured at ≈ 0 (P3-1).

## Assessment of the external Sol-codex report (`2026-08-31-090022`, revision `e70b2d9`)

Re-verified against `e92643f5` rather than inherited. Its two **P1**s I do not carry at that
severity:

- **"Safe API still allows a backing swap."** The scenario is real and I reproduced it by reading:
  `pop(&b)` against a fresh `ArrayLinks` whose `b[0] == 0` makes the CAS a `current -> current`
  no-op that re-issues index 0 forever. But it is a documented caller-contract violation
  (`src/lib.rs:754-817`, rule 1, with that exact failure mode spelled out), it is memory-safe within
  this crate, and the proposed fix — have the stack own the backing — is incompatible with the
  design's whole point: the one real consumer builds `RegistryLinks { reg }`
  (`src/registry/heap_registry.rs:566,608,624`) as a *borrowed view* constructed per call, which cannot be
  owned by a `'static` stack embedded in that same registry. **P3 at most, and I do not re-raise it**
  — no new evidence since it was last considered.
- **"The tag is finite, so 'structurally defeats ABA' overclaims."** Literally true, and the crate
  already says so in the same breath ("That is a derived claim, not a slogan… The tag is not
  strictly monotonic — a strictly monotonic counter never repeats a value, and this one wraps",
  `src/lib.rs:4-12`) and spends a 57-line section deriving the bound. My own measurement makes the
  bound *more* conservative, not less (see the verdict section: peak aggregate ≈ 1.9 × 10^7
  pushes/sec, 10× under the doc's adopted ceiling). **P4 wording at most.**

Its **P2**s: the README count drift is **CONFIRMED and still present** (my P3-4, found
independently). The debug-only `load_next` check is **CONFIRMED** and I add the missing cost
measurement (my P3-1). The `compile_error!`/import-isolation point I checked and **do not carry**:
`#[cfg(not(loom))] use core::sync::atomic::{AtomicU32, AtomicU64, …}` on a target without 64-bit
atomics does add a second error after the intended one, but the intended `compile_error!` is emitted
first and names the reason, which is the entire stated goal; gating the import on
`target_has_atomic` would leave every downstream `use` unresolved and produce *more* noise, not
less. The "lock-free claim depends on the `Links` impl" point is technically correct and, in my
judgement, P4 (the trait doc's ordering contract already scopes it, and every `Links` impl in
existence is two atomic accesses). The hidden-hooks-are-semver-surface point is already
acknowledged verbatim in the crate's own docs; the disagreement is about whether to remove them
before 0.1.0, which is a product decision, not a defect.

Its **P3**s on backoff portability and the benchmark's timing skew are directionally right; P2-1
above supersedes the first with numbers, and I did not find the second material (the barrier
already excludes spawn cost and the worker/main window disagreement is well under 1 ms of a 1 s
window — the per-thread *distribution*, not the window alignment, is where the real signal was
hiding).

## What is genuinely good (verified, not assumed)

- **The `head` field's `INVARIANT` block (`src/lib.rs:666-689`)** remains the best comment in the
  crate: it names the exact premise `pop`'s `Acquire`-only success ordering rests on, lists every
  current writer, and prescribes the compensating change (`AcqRel`) if a plain store is ever added.
  I re-derived the release-sequence argument and re-grepped the premise; both hold, and the backoff
  did not disturb either.
- **The round-6 `model`/`model_with_oracle` remediation is genuinely structural**, not cosmetic —
  it removed ten independent chances to forget a line and folded round-6's P4-6 (`Builder::new()`
  duplication) into the same change. The residual loopholes in special focus 1 are second-order.
- **The three activation oracles are non-vacuous**, verified by running each alone.
- **The bench's contention discipline has now survived six rounds of edits** — re-push exactly what
  you popped, drain-and-assert-empty before the phase-2 prefill — and my eight-arm A/B reused it
  unchanged across 24 timed windows with no anomaly.
- **`cargo package` is clean**: 16 files, no strays, no `bench-iters.txt`, licences and CHANGELOG
  included.
- **The `--calibrate` skip added in round 6 (`e70b2d9`) works**: the contention section is skipped
  and the invocation returns in ~1 s per row.

## Suggested order of work

1. **P2-1** — re-run the cap sweep on the committed harness, decide the constant on the
   throughput-vs-starvation axis, and rewrite the constant's doc and the CHANGELOG bullet to state
   the axis that actually decided it. This is the only item gating an unconditional GO on the perf
   side.
2. **P2-2** — commit the threaded conservation test the backoff commit describes but threw away.
   ~40 lines, ~0.1 s, and it is the prerequisite for any future miri/TSan row.
3. **P3-6** with P2-1 — the sweep produces the raw log + summary CSV the rule already required.
4. **P3-2** — the `--cfg loom` clippy row (one line) plus moving the oracle assertion inside
   `model_with_oracle`. Do it before round 8 adds an eleventh model.
5. **P3-3** — three characters plus one `const _: () = assert!(…)`, and update `push`'s `# Panics`.
6. **P3-4**, **P4-1** as one bundle — delete both counts rather than updating them, per the fix
   rounds 5 and 6 already converged on.
7. **P3-1** — decide the release-active `pop` guard on its merits now that the cost objection is
   measured away.
8. **P3-5**, **P4-2**…**P4-7** as one bundle.
9. **P3-7** — the comment-density trim; largest edit, lowest urgency, but it is why items 6 keeps
   recurring.
