# `tagged-index-stack` — independent publish-readiness review, round 8

- **Reviewer:** Claude (Opus 5), adversarial pass. `src/lib.rs`, all seven `tests/*.rs`,
  `benches/tagged_index_stack_bench.rs`, `README.md`, `CHANGELOG.md`, `Cargo.toml`,
  `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` + its committed CSV and both raw logs, and the
  `ci.yml` rows covering the crate were read first and every finding was formed independently.
  Prior rounds' reports were NOT read for this round (per the task brief); the round-7 report's
  *header* was consulted only at the end to match this report's established format, and the
  round-7 remediation commits (`git log -- crates/tagged-index-stack/`) were read to know which
  claims are new.
- **Date:** 2026-08-31 12:54:20 +0200 (CEST)
- **Revision reviewed:** `10811fbdae45a62cade7556bfa2241f0a0a6fbf6` (`main`).
  `git status --porcelain -- crates/tagged-index-stack docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md`
  is empty — the reviewed tree is clean.
  Crate source identity: `sha256(crates/tagged-index-stack/src/lib.rs) =`
  `0f0395c2996bc0d25702b819e52fb8bc61005bc7d00b3a71e5fea636cba923c6`.
- **Scope:** `crates/tagged-index-stack/**`, `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (+
  `_summary.csv`, `_raw_tis_backoff_cap_sweep_run1.log`,
  `_raw_tis_backoff_cap_sweep_run2_repeat16.log`), `.github/workflows/ci.yml`'s
  `test workspace members` / `loom-alloc-global` / `msrv` rows, and the in-workspace consumers
  (`src/registry/heap_registry.rs`, `src/registry/bootstrap.rs`, `src/kani_proofs.rs`).
- **Machine / toolchain for every measured number:** `rustc 1.97.0 (2d8144b78 2026-07-07)`,
  `cargo 1.97.0 (c980f4866 2026-06-30)`, host `x86_64-pc-windows-msvc`, 11th Gen Intel Core
  i7-11800H (8 cores / 16 logical), Windows 10 Pro 19045. Shared dev host, no core pinning.
- **No file in the repository was modified.** Read-only review. The two instrumented probes
  behind **P2-2** and **P3-1** ran in a throwaway copy of the crate OUTSIDE the repository
  (`D:/dev/rust/.scratch-tis-r8/tis`, deleted afterwards); the in-repo bench was deliberately
  NOT run, so the tracked root `bench-iters.txt` was never touched.

## Verification actually performed

Every number below came from running something, not from reading.

| Check | Result |
| --- | --- |
| `cargo test -p tagged-index-stack --no-fail-fast` | **29 green** (18 `stack_unit`, 5 `proptest_pack_unpack`, 2 `regression_counter_wrap`, 2 `custom_links_impl`, 1 `readme_example`, 1 `threaded_conservation`; `loom_aba` correctly 0; 0 doctests) |
| `cargo test -p tagged-index-stack --release --no-fail-fast` | **29 green** — including `pop_rule_4_guard_fires_on_invalid_next_from_backing`, which is what actually pins the round-7 release-active promotion |
| `[profile.release]` inspected for `debug-assertions` | absent → default `false`, so the release row above is a *real* oracle for the promotion, not a tautology |
| `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom --test loom_aba` | **10 green in 0.16 s**, all three `#[should_panic]` counterfactuals included |
| `cargo clippy -p tagged-index-stack --all-targets -- -D warnings` | clean |
| `cargo fmt -p tagged-index-stack --check` | clean |
| `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` | clean |
| `cargo package -p tagged-index-stack --no-verify --allow-dirty --list` | **17 files**, no strays (`benches/`, all 7 `tests/`, both licences, both docs) |
| wall time of `threaded_conservation` | **0.01 s** (both debug and release) vs. its own doc's "running in a couple of seconds" — **P3-1** |
| out-of-tree probe A: per-retry `spins`-level histogram inside `push`/`pop` | **P3-1** — at the shipped test's exact shape only **1–5 push and 3–8 pop calls out of 80,000** ever retry |
| out-of-tree probe B: per-call `pop` wall latency, cap 6 vs cap 0, 3 shapes | **P2-2** — single-call max **168 ms** at 8 threads under the shipped cap 6, vs **0.85 ms** at cap 0 |
| `TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv` re-aggregated independently | §3.3's averages **reproduce exactly** (6.12 / 13.12 / 20.64 and 0.375 / 0.190 / 0.201); §3.1's deltas reproduce exactly; **§3.2 + §5's fairness conclusion does NOT survive the full CSV** — **P2-1** |
| `git cat-file -t 47c81e90…` (the report's cited base SHA) | resolves — `docs(tagged-index-stack): add review round 2` |
| every writer of `HeapSlot::next_free` re-grepped across `src/` | exactly one: the crate's own `push` via `RegistryLinks::store_next` — **rule-4 guard is structurally untrippable there** |

---

## Overall verdict: **CONDITIONAL-GO**

**The shipping algorithm is still correct, and I re-derived that rather than inheriting it.**

- **Tag monotonicity along the head's modification order holds.** Every successful `push` does
  `tag.wrapping_add(1)` (`src/lib.rs:888`); every successful `pop` re-packs the tag it observed,
  including on the drain-to-empty branch (`src/lib.rs:1052-1057`). A `(X, t)` head word can only
  recur after a full `2^TAG_BITS` wrap whose last push re-installs `X`.
- **`pop`'s `Acquire`-success-without-`Release` is still sound**, because its premise still holds:
  I re-grepped `head` for a plain `store` and found none — `new` is initialisation, `raw_head` /
  `is_empty` only load, and all three writers (push's `Release` CAS, pop's `Acquire` CAS, the
  loom-only `cas_head_for_test`) are RMWs, so the release sequence headed by any push's `Release`
  CAS is unbroken. The `INVARIANT` block on the private field (`src/lib.rs:689-712`) states that
  premise and is load-bearing.
- **The round-7 changes did not break anything.** `pop`'s backoff-skip-when-empty is correct
  (re-derived below, §Q4); the rule-4 guard fires strictly *before* the CAS so a panic cannot
  leave the head half-mutated; the `spins` bound is right; `_CHECK_BITS` still routes from every
  public associated item.
- **The single in-workspace `Links` implementor cannot trip the new release-active guard**
  (re-derived below, §Q1), and the CI release row genuinely pins the promotion.

**What holds this back from an unconditional GO is two P2s, both about the round-7 remediation
of `BACKOFF_SPIN_CAP` rather than about the algorithm.** Round 7 was raised because the cap's
published rationale was unmeasured. The replacement rationale is measured — but it is **selective**
(**P2-1**: the fairness conclusion is contradicted by the report's own committed CSV, which
measured two fairer caps and then dropped them from the fairness table), and it measured the
**wrong unit of fairness** (**P2-2**: aggregate per-thread ops skew over a 1-second window, never
per-CALL latency — which, measured, is 200× worse under the shipped default than with no backoff
at all, and is documented nowhere). Both are the *same defect class* round 7 set out to close,
in a new form.

Neither P2 requires changing `BACKOFF_SPIN_CAP`. Both require the published rationale to say what
the data actually says.

---

## Findings

### P2-1 — the cap-6 fairness conclusion is contradicted by the sweep's own committed CSV: caps 0 and 4 were measured, are fairer than cap 6 almost everywhere, and were silently dropped from the fairness table

**Files:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:166-188` (§3.2) and `:251-282` (§5);
`crates/tagged-index-stack/CHANGELOG.md:188-189`;
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv` (the contradicting evidence).

§3.1's throughput table spans **all five** measured caps `{0, 4, 6, 8, 10}`. §3.2's fairness table
spans **three** — `{6, 8, 10}` — with no stated reason for dropping 0 and 4, even though the CSV
carries their `max_over_min` / `min_over_mean` columns for every arm. §5 then concludes, and
`CHANGELOG.md:188-189` repeats verbatim:

> `BACKOFF_SPIN_CAP` stays `6` — **the most fairness-conscious of the caps measured**

That is false against the report's own CSV. `min/mean` is the report's own primary starvation
metric ("unluckiest thread's share of a fair split"). Re-aggregating all `run=1` rows:

| threads / bench | cap 0 | cap 4 | **cap 6** | cap 8 | cap 10 |
|---|---:|---:|---:|---:|---:|
| 2 / push_pop | 0.950 | 0.852 | **0.950** | 0.877 | 0.757 |
| 2 / churn    | 0.975 | 0.955 | **0.931** | 0.890 | 0.953 |
| 4 / push_pop | 0.929 | 0.825 | **0.549** | 0.742 | 0.629 |
| 4 / churn    | 0.951 | 0.792 | **0.803** | 0.693 | 0.548 |
| 8 / push_pop | 0.951 | 0.855 | **0.655** | 0.541 | 0.502 |
| 8 / churn    | 0.878 | 0.784 | **0.634** | 0.355 | 0.352 |
| 16 / push_pop| 0.493 | 0.615 | **0.331** | 0.242 | 0.295 |
| 16 / churn   | 0.638 | 0.441 | **0.365** | 0.157 | 0.267 |

**Cap 0 is better-or-equal to cap 6 in 8 of 8 arms** (strictly better in 7, tied at 2/push_pop).
**Cap 4 is better than cap 6 in 6 of 8.** On `max/min`, cap 0 beats cap 6 in 7 of 8 and cap 4 in
5 of 8. So the fairness ordering across the caps actually measured is
`0 > 4 > 6 > 10 ≈ 8`, and cap 6 is the **middle** of a five-point Pareto curve, not its
fairness end.

`src/lib.rs:272-277` gets this right and is carefully hedged ("the most fair **of the three**").
`CHANGELOG.md` and the report itself are not. §3.2's own heading is additionally
self-contradicting on its own three-cap subset: it claims "cap 6 has the BEST (lowest) skew at
every thread count" and then the body two paragraphs later says "in every row except one
(`4/push_pop`, where cap 8 edges it)".

**Failure scenario:** a downstream reader (or the next round) takes "cap 6 = the fairness-optimal
choice among everything we measured" at face value from `CHANGELOG.md` — the artifact crates.io
renders — and concludes no fairness headroom is left. In fact halving the cap to 4 recovers most
of the fairness gap at a ~10-25 % throughput cost per §3.1, and cap 0 recovers all of it at a
2.5-8× throughput cost. The decision to ship 6 may well be right; the published statement of
*why* is not what the data says, which is exactly the defect round 7 was opened to fix.

**Fix (no code change):** restore caps 0 and 4 to §3.2's fairness table, and reword §5 +
`CHANGELOG.md` to `src/lib.rs`'s already-correct hedge — cap 6 is a *compromise*: fairer than
8/10, less fair than 0/4, and 2.5-8× faster in aggregate than 0/4. Repair §3.2's heading to match
its own body.

---

### P2-2 — per-CALL latency was never measured on any axis; under the shipped cap 6 a single `pop` can block for 168 ms, ~200× the no-backoff baseline, and nothing in the crate documents that `push`/`pop` are lock-free but not starvation-free

**Files:** `crates/tagged-index-stack/src/lib.rs:254-285` (`BACKOFF_SPIN_CAP`'s doc),
`:927-943` (push's backoff), `:1082-1100` (pop's backoff);
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` (whole report — it measures only
`total_ops_per_sec` and per-thread ops over a 1-second window).

The doc comment says the cap is "a fairness-vs-throughput choice", and §5 reasons about "an
occasional badly-starved thread". Neither ever states how badly, in what unit, or for how long —
and the sweep's fairness metric (per-thread ops ratio over 1 s) cannot answer it: a thread that
loses 90 % of a second inside ONE call and a thread that is uniformly 10× slow produce the same
`min/mean`.

I measured the missing axis directly. Out-of-tree copy of the crate, `ArrayLinks<64>` prefilled
`0..64`, N threads × M `pop`-then-repush-exactly-what-you-popped iterations (the shipped test's
and the bench's own discipline), `Instant` around each `pop`, `--release`. Identical
instrumentation in both arms, so the comparison is apples-to-apples:

| shape | **max single `pop`, cap 6 (shipped)** | max single `pop`, cap 0 | wall, cap 6 | wall, cap 0 |
|---|---:|---:|---:|---:|
| 4 × 20,000 | **8.15 / 6.96 / 7.08 ms** (3 runs) | 1.10 / 0.20 / 0.98 ms | 14.2 ms | 32-55 ms |
| 8 × 200,000 | **168.1 ms** | 0.85 ms | 352 ms | 1.75 s |
| 16 × 200,000 | **173.2 ms** | 29.6 ms | 967 ms | 4.63 s |

A second, independent, non-timing probe confirms the mechanism rather than blaming the scheduler.
Instrumenting the `Err(actual)` arm with a `spins`-level histogram and a per-call longest-retry-run
tracker (both touched only on a retry, so the hot path is unperturbed) gives, at cap 6:

| shape | calls that retried at all (push / pop) | longest single call (push / pop), in retries | share of retries at the 64-spin cap |
|---|---|---:|---:|
| 4 × 20,000 | 1 / 3 of 80,000 | 403 / 577 | ~98 % |
| 8 × 200,000 | 10 / 26 of 1,600,000 | 8,931 / 11,779 | ~99.8 % |
| 16 × 200,000 | 55 / 67 of 3,200,000 | 16,623 / 20,638 | ~99.8 % |

versus cap 0 at 4 × 20,000: **79,386-101,153 retries spread over ~79 k calls** (99-126 % of calls
retry at least once), longest single call 3,090-8,534.

The shape of the result is the finding: **the backoff does not reduce contention, it
redistributes it.** It converts "almost every call retries once or twice" into "one call in tens
of thousands is starved for thousands of consecutive lost CASes, 99.8 % of them at the maximum
64-spin backoff." A single starved `pop` at 8 threads executes ≈ 11,779 × 64 ≈ 754,000 `pause`
instructions, which is ≈ 23 ms of pure spinning at Intel's documented ~140-cycle PAUSE for
Skylake+ at 4.6 GHz before any scheduling effect — and the measured wall figure is 168 ms.

**Failure scenario:** the README pitches this crate for "slab allocators, object pools,
entity-component stores, id allocators, and connection tables". A consumer recycling a slot on a
request path, under 8 threads of ordinary contention, sees a p99.99 `pop` of ~170 ms on a
64-element free-list — three orders of magnitude past anything the docs prepare them for. Nothing
in the crate says `push`/`pop` are lock-free but **not** wait-free or starvation-free, and nothing
says the backoff is the reason. A reader who knows "lock-free" only as "no locks, so no blocking"
gets no warning at all.

**Fix (no code change required):** (a) add one paragraph to `BACKOFF_SPIN_CAP`'s doc and to the
README stating explicitly that the operations are lock-free but not starvation-free, that the
backoff trades per-call tail latency for aggregate throughput, and roughly how much; (b) add a
per-call-latency column to the sweep report (the workload is already committed — the bench's
contention rows just need a max/percentile alongside the ops count) so the tradeoff is stated in
the unit that actually matters to a consumer. Changing the cap is a separate decision this finding
does not force.

---

### P3-1 — `tests/threaded_conservation.rs`: its central purpose claim has no oracle, is satisfied by 1-8 calls out of 80,000, and its scale justification is wrong by ~300×

**File:** `crates/tagged-index-stack/tests/threaded_conservation.rs:1-32` (module doc),
`:46-56` (the constants and their justification).

The file's stated reason to exist, over and above `loom_aba.rs`, is:

> a fixed, modest number of REAL OS threads hammering a SHARED stack for many iterations, so
> genuine contention forces many CAS retries per call, and `spins` genuinely climbs into its
> higher range in practice

and `ITERS_PER_THREAD`'s doc justifies 4 × 20,000 as "enough real contention to push the CAS-retry
backoff's `spins` counter well past 2-3 … while still running in a couple of seconds."

Three problems, all measured:

1. **No activation oracle.** The crate *has* retry counters built for exactly this
   (`POP_RETRY_COUNT` / `PUSH_RETRY_COUNT`, `src/lib.rs:1210` / `:1250`), and `loom_aba.rs`
   correctly refuses to trust a conservation assertion without asserting the retry branch was
   reached. Those counters are `#[cfg(loom)]`-only, so this test — the one place the claim is
   about *real* threads — asserts nothing about activation.
2. **The activation it does get is a handful of calls, not "many retries per call".** Probe A
   above: at exactly this shape, **1-5 push calls and 3-8 pop calls out of 80,000** ever lose a
   CAS. The claim is literally true (`spins` does reach 6) but by a 1-in-20,000 event that varies
   1..8 run to run; a run where the count is 0 is entirely plausible on a busier host or a
   machine with fewer cores, and nothing would notice.
3. **"running in a couple of seconds" is wrong by ~300×.** Measured: `finished in 0.01s` under
   both `cargo test` and `cargo test --release`; 4.5-5.1 ms of actual threaded phase in the
   out-of-tree probe. The stated runtime is the entire justification for the modest scale — and
   the scale it justifies is **20× smaller** than the throwaway probe the file's own doc says it
   is the committed replacement for (8 × 200,000 = 1.6 M pairs vs. 4 × 20,000 = 80 k). At the
   probe's original scale the test would run in ~0.1 s.

**Failure scenario:** a future change that makes the retry path unreachable (e.g. a botched
`spins` reset, or a refactor that turns the `Err` arm into a `return`) leaves this test green,
because it never checks that a retry happened; and the conservation assertion it *does* make is
satisfied by 80,000 essentially-uncontended pop/push pairs. The test is not vacuous — it would
catch gross loss/duplication — but it does not deliver the coverage its own doc sells.

**Fix:** promote the two retry counters out of `#[cfg(loom)]` (a `#[doc(hidden)]` non-loom twin,
or a `bench-internals`-style feature — the crate already has the doc-hidden test-only-forwarder
convention for this), assert a non-zero delta in this test, and raise `ITERS_PER_THREAD` /
`NUM_THREADS` to at least the 8 × 200,000 the file claims to replace; correct the runtime
sentence to the measured figure.

---

### P3-2 — the report's throughput headline and the rustdoc/CHANGELOG "+17 % to +58 %" range are contradicted by the report's own table

**Files:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:142-164` (§3.1 heading + table);
`crates/tagged-index-stack/src/lib.rs:265-271`; `crates/tagged-index-stack/CHANGELOG.md:179-182`.

§3.1's heading: "cap 8 and cap 10 beat cap 6 at EVERY thread count, including 2". Its own table's
`4 / churn` row: cap 10 is **−0.4 %** — it *loses*. `src/lib.rs:268-271` and
`CHANGELOG.md:180-182` then both compress the table to "+17 % to +58 % depending on regime". The
table's actual span of cap-8/cap-10 deltas is **−0.4 % to +58.4 %**, and the whole 4-thread block
(+4.7 %, +6.7 %, +2.7 %, −0.4 %) sits outside the quoted range in both directions of honesty:
the quoted floor overstates the throughput cap 6 gives up.

Compounding this, every §3.1 cell is a **single sample**, quoted to one decimal place, while the
report's own §3.3 shows cap 8's 16-thread throughput swinging **14.66 M → 32.68 M ops/sec (2.2×)**
between two reps of the identical arm. A "+4.7 %" delta read off one sample from a distribution
that wide is not a result.

**Failure scenario:** the next round reads "cap 8 and cap 10 both beat cap 6 at every thread
count (+17 % to +58 %)" from `CHANGELOG.md`, treats the throughput case as settled and uniform,
and re-opens the cap decision on a premise the underlying table does not support at 4 threads.

**Fix:** state the real range and its `n=1`-per-cell caveat, name the `4/churn` exception in the
heading, and (per CLAUDE.md's derived-tables rule, point 4) keep every percentage's numerator and
denominator inline.

---

### P3-3 — the sweep's derivation pipeline is not committed, so the report's tables cannot be regenerated — and the one rule that would have caught P2-1 and P3-2 is the one that is unmet

**File:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:126-138` (§2's "Raw logs" / "Summary CSV"
paragraphs), `:56-92` (§1's reproduction recipe).

The report correctly commits raw logs + a summary CSV, and correctly says the CSV was "parsed by
a small `awk` script (not hand-transcribed)". But it also says "the driver script itself was
scratch (not committed)", and the `awk` pass is not committed either. So of CLAUDE.md's
"tables must be DERIVED, by one checked script, from raw per-sample data" rule:

- point 1 (raw per-sample data written first) — **met**;
- point 2 (CSV and Markdown tables derived by one checked script, not retyped) — **claimed but
  unverifiable**: neither the driver nor the parser exists in the commit, so nobody can re-run
  the derivation and diff it against the published tables;
- point 6 (the generating script must **assert** the arithmetic it prints, so a wrong ratio is a
  failing check rather than a published claim) — **not met**.

That is not a bookkeeping nit here: point 6 is *precisely* the check that would have failed on
P2-1 ("`min/mean` of cap 6 is the maximum over all measured caps" — false in 7 of 8 arms) and on
P3-2 ("every cap-8/cap-10 delta ≥ +17 %" — false in 4 of 16 cells). The two live defects in this
report are exactly the two an asserting derivation script exists to prevent.

**Failure scenario:** a future re-run or a re-derivation from the committed CSV produces different
tables than the report shows, with no way to tell whether the CSV, the prose, or the (absent)
script was wrong. Reproducing the *measurement* is documented; reproducing the *report* is not.

**Fix:** commit the sweep driver and the aggregation script (a ~40-line `scripts/` or
`examples/`-adjacent file) with in-script assertions for every ratio and superlative the prose
states, in the same shape `examples/r21_2_opt_h_stage1_probe.rs` set for a throwaway probe that
had already published its numbers.

---

### P3-4 — `pop`'s new backoff-skip-when-empty branch is reachable but covered by no test and no loom model

**File:** `crates/tagged-index-stack/src/lib.rs:1092-1100` (added by `df8f8b8`, round-7 P4-3).

```rust
if !TaggedIndex::<INDEX_BITS>::is_empty(actual) {
    for _ in 0..(1u32 << spins.min(BACKOFF_SPIN_CAP)) { core::hint::spin_loop(); }
    if spins < BACKOFF_SPIN_CAP { spins += 1; }
}
```

The branch is correct (see §Q4), but the `is_empty(actual) == true` arm is not reached by
anything shipped. I traced every model in `loom_aba.rs`:

- `pop_pop_conservation` — 2 elements, 2 real poppers: the loser's CAS fails with `actual` = the
  *remaining* element, never empty;
- `pop_retry_after_failed_cas_sees_concurrent_pushs_link_real_type` — 1 element + a concurrent
  `push`: the failure's `actual` is the pushed, non-empty head;
- `aba_repush_…` / `tagged_stack_survives_…` / `run_cas_retry` / `run_h2` — thread A uses
  `cas_head_for_test`, so the shipped `pop`'s `Err` arm is not on the path at all;
- `threaded_conservation.rs` — 64 prefilled indices, 4 threads: the stack provably never empties
  (that is its own `.expect` message).

The uncovered arm needs exactly one element and two concurrent real poppers: both read
`head = (0, t)`, both compute `new_head = empty`, the loser's CAS fails with an empty `actual`
and must return `None` without spinning. That is a ~20-line loom model in the file's existing
`model()` shape.

**Failure scenario:** a future edit inverts the condition, or moves the `head = actual`
assignment below it, and the whole test suite — including the loom suite whose entire pitch is
exhaustive coverage of the real type — stays green. The crate's own README advertises "an
exhaustive loom model-check run against the real type"; a shipped branch no model reaches
undercuts that claim more than the branch's own risk warrants.

---

### P3-5 — `src/registry/bootstrap.rs`'s loom shim claims to be a faithful replica of the crate's `push`/`pop` and is now wrong on three counts

**File:** `src/registry/bootstrap.rs:461-544` (the `loom_shim` module), specifically the claim at
`:465-469`:

> This keeps the shim's push/pop a FAITHFUL byte-for-byte replica of the crate's algorithm (same
> H-2 running-tag empty transition, same Acquire/Release/Relaxed orderings, same RAD-1 lazy links
> …), **differing from the shipped type ONLY in which `AtomicU64` backs the head**.

Since that comment was written the crate's `push`/`pop` have gained three things the shim does
not have:

1. the round-6 exponential CAS-retry backoff (`BACKOFF_SPIN_CAP`, the `spins` counter and its
   round-7 bound) — shim `push`/`pop` retry immediately, forever;
2. `push`'s release-active `index < INDEX_MASK` guard (`src/lib.rs:858-861`) — the shim has no
   bounds check at all;
3. `pop`'s release-active rule-4 guard (`src/lib.rs:1048-1051`, round-7 P3-1) — likewise absent.

None of the three is a correctness bug *for the shim's job* (the backoff is a latency device; both
guards catch caller-contract violations that the registry's own construction excludes). But the
comment's "ONLY in which `AtomicU64` backs the head" is now materially false, and the shim IS the
code that actually runs `free_slots` in the `loom-alloc-global` and `loom-xthread` CI jobs
(`.github/workflows/ci.yml:2644`), so a reader checking "does sefer's loom build exercise the
shipped algorithm?" is told yes when the answer is "the head protocol, not the guards".

**Failure scenario:** the next change to the crate's `pop` (say, a different tag rule on the drain
branch) is not mirrored into the shim, because the shim's comment says it only differs in atomic
type and nobody thinks to look; sefer's loom jobs then model-check a protocol that has quietly
diverged from the one that ships, and report green.

**Fix:** correct the comment to enumerate the three deliberate divergences and say why each is
irrelevant to what the shim is for. (Copying the backoff and the guards into the shim is the
alternative and is *not* recommended — it doubles the drift surface.)

---

### P4 findings

**P4-1 — three wrong internal cross-references in the gate report's §1.**
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md:34` cites "§2's protocol" for the clean-working-tree
protocol (that is §7); `:40` says the per-cap edits are "documented in full in §2 below" (they are
in §1 itself, ~25 lines further down); `:52-53` cites "§5/P4-7" for the profile discussion (that
is §6). A reader following any of the three lands on unrelated text.

**P4-2 — an unfinished sentence in the gate report's §2.**
`:98-102`: "2 is the lowest contention this harness's `contention/*` section can reach (it always
spawns `num_threads >= 2`**...** in practice the harness's own `available_parallelism()` floor is
whatever the OS reports, clamped to the arm under test here)". The ellipsis joins two
half-thoughts and the parenthetical never resolves what the floor actually is.

**P4-3 — the crate doc's and README's "51.56 ns/pair" has no committed receipt.**
`src/lib.rs:143-146` and `README.md:116-120` cite "51.56 ns/pair on an 11th Gen Intel Core
i7-11800H, rustc 1.97.0 (2026-08-31)" as the measured uncontended push rate underpinning the
tag-wrap bound. `grep -rn "51\.56" docs/perf/ crates/tagged-index-stack/` finds nothing. The
committed sweep logs — same machine, same rustc, same day — carry 20 single-thread `churn`
samples spanning **51.41-55.65 ns/op** and none equal to 51.56. The doc mitigates by saying "the
bound below only needs the order of magnitude", so the *argument* is unaffected; the *citation*
is to a run that was never saved. Cite one of the committed samples instead.

**P4-4 — three unwrapped 300-400-character lines in `README.md`.**
Lines 183, 185 and 187 are 310, 400 and 391 characters on one physical line while the rest of the
file wraps at ~80. They are the entire `## Notes` section — the part a reader most needs to
skim — and they diff as one blob on any future edit.

**P4-5 — `pop_link_out_of_range` is not `#[track_caller]`; its documented twin is.**
`src/lib.rs:1113-1126` carries `#[cold] #[inline(never)]` and its doc says it is "the same
`#[cold]` + `#[inline(never)]` shape as `push_index_out_of_range`" — but
`push_index_out_of_range` (`:957-965`) additionally has `#[track_caller]`, forwarded from `push`
itself, specifically so a consumer with many call sites learns *which* one violated the contract.
Now that both guards are release-active and both diagnose a caller-contract violation, the
asymmetry is undocumented: a rule-4 panic reports `lib.rs`, not the `pop` call site. Either add
it (and note the cost) or state in one line why `pop` deliberately does not pay for it.

**P4-6 — `CHANGELOG.md`'s `### Behavior` section contradicts its own release header.**
`:7-10` says "0.1.0 - Unreleased … First release. Everything below is new in this version;
nothing has shipped before it." `:205-228` then documents a *change*: "**is now release-active,
not debug-only** … Previously this was a `debug_assert!`". Nothing was previously anything, from a
consumer's perspective. The content (why the guard is unconditional, and that `RegistryLinks`
cannot trip it) is worth keeping — it belongs under `### Added`'s `Links` entry, which already
describes the guard, not under a `Behavior`-change heading in a first release.

**P4-7 — no `-p tagged-index-stack` row in the `msrv` job.**
`.github/workflows/ci.yml:2035-2064`: the job runs root-scoped `cargo check --all-features` and
`cargo test --no-run --all-features`, which compiles the crate's **lib** (via `alloc-global`'s
`dep:tagged-index-stack`) on 1.88 but never its own `tests/` or `benches/` targets. Three sibling
crates carry an explicit `-p <crate>` MSRV row for precisely this gap class (`sefer-region` at
`:2069-2072`, `aligned-vmem` at `:2089-2090`, `numa-shim` below them); `tagged-index-stack` — the
one about to be published with `rust-version = "1.88"` — does not. The test targets pull in
`proptest`, `bench-scale-tool` and `std::thread::scope`, none of which is MSRV-verified for this
crate today.

**P4-8 — the tag-width derivation is triplicated verbatim, and it has already drifted.**
The ~40-line "Tag-width budget" derivation exists three times (`src/lib.rs:109-165`,
`README.md:84-139`, `CHANGELOG.md:63-83`), as does the backoff rationale. The drift is not
hypothetical: `src/lib.rs:272` says cap 6 is "the most fair **of the three**" (correct), while
`CHANGELOG.md:188` says "the most fairness-conscious of **the caps measured**" (false — P2-1).
`src/lib.rs` is 958 comment lines out of 1,275 (75 %). The volume is defensible for a subtle
primitive; the *copies* are what produce P2-1-shaped divergence. Consider making README and
CHANGELOG point at the rustdoc for the derivation rather than restating it.

**P4-9 — the release-active panic path allocates, and the crate's headline use case is a global allocator.**
`src/lib.rs:1121-1125` (and `:961-964`) `panic!` with formatted arguments. Under `std`, that
payload is boxed through the global allocator — so if a `#[global_allocator]` consumer ever
tripped either guard from inside its own allocation path, the panic machinery re-enters the
allocator. This is unreachable-by-construction for `RegistryLinks` (§Q1) and was already true of
`push`'s guard before round 7, so it is informational — but the crate-root doc explicitly sells
slot-recycling "in the parent allocator", and one sentence in `# Panics` noting that a consumer
in that position should treat both guards as abort-equivalent would close it.

---

## The four questions in the brief, answered directly

### Q1 — did `pop`'s release-active rule-4 guard break anything, and is it correctly documented?

**No, and yes — with one correction to the premise.** Re-derived from scratch:

- **The guard cannot fire for the in-workspace consumer, structurally.**
  `RegistryLinks::load_next` (`src/registry/heap_registry.rs:577-591`) returns
  `reg.slot(index).next_free.load(Acquire)`. I grepped every `next_free` write in `src/`: the
  only one is `RegistryLinks::store_next` (`:593-598`), which is called *only* from the crate's
  own `push`. `push` writes exactly two value shapes (`src/lib.rs:878-886`): `TAIL` when the head
  is empty, or `cur_idx as u32` where `cur_idx = head & INDEX_MASK`. Since `push`'s own
  release-active guard admits only `index < INDEX_MASK`, the head's index half is always either a
  previously-admitted index `≤ 0xFFFE` or the empty sentinel (which maps to `TAIL`). So
  `next_free ∈ {TAIL} ∪ [0, 0xFFFE]`, and the guard's condition
  `next != TAIL && next >= 0xFFFF` is unsatisfiable. The slot's const initial value is `0`
  (`src/registry/bootstrap.rs:950`, RAD-1 lazy) — also in range, and unreachable anyway since
  `pop` only reads links of indices that a `push` put on the stack. `MAX_HEAPS = 4096`, well
  under `INDEX_MASK = 65535`. **`CHANGELOG.md:223-228`'s claim is accurate.**
- **Correction to the brief's premise:** `RegistryLinks` is the only in-workspace `Links`
  *implementor*, but not the only in-workspace *consumer* of the crate. `src/kani_proofs.rs:182`
  binds `TaggedIndex<16>::pack`/`unpack` in bounded proofs (packing only, no guard involvement),
  and `src/registry/bootstrap.rs:478-544` carries a whole second copy of the stack algorithm for
  loom builds — which is where **P3-5** comes from.
- **Tests/benches confirm it indirectly, and one test confirms it directly.**
  `stack_unit.rs:349-368`'s `pop_rule_4_guard_fires_on_invalid_next_from_backing` drives an
  `AlwaysInvalidLinks` returning `INDEX_MASK` and asserts the panic message. It carries **no**
  `#[cfg(debug_assertions)]` gate, and `[profile.release]` in the root `Cargo.toml` does not set
  `debug-assertions`, so CI's `cargo test -p tagged-index-stack --release` row
  (`.github/workflows/ci.yml:1754`) is a genuine oracle for the promotion, not a tautology — I
  confirmed both halves. All 29 tests pass in both profiles, all 10 loom models pass, and
  `threaded_conservation` (80,000 real-thread pops through `ArrayLinks`) never trips it.
- **Placement is right:** the guard runs *after* `load_next` and *before* the CAS
  (`src/lib.rs:1033-1051`), so a panic cannot leave the head partially mutated, and the `#[cold]`
  `#[inline(never)]` helper keeps the message formatting out of the loop body.
- **Documentation:** consistent in all five places (`src/lib.rs:50-53`, `:556-563`, `:991-1003`;
  `README.md:43-48`; `CHANGELOG.md:89-95`, `:205-228`). Two residual nits: **P4-5**
  (`#[track_caller]` asymmetry, contradicting "the same shape as `push_index_out_of_range`") and
  **P4-6** (a `### Behavior`-change section in a first release).

### Q2 — is `tests/threaded_conservation.rs` non-trivial, is 4 × 20,000 enough, should it grow?

**It is non-vacuous for what it asserts, it is far weaker than it claims, and yes — it should
grow.** See **P3-1** for the numbers. In short: the conservation assertion is real (an exact
multiset check over 80,000 contended pop/push pairs would catch loss or duplication), and every
discipline in the file is right (re-push exactly what you popped; no locally-invented indices;
prefill on a provably empty stack). But its stated *raison d'être* — exercising the backoff depth
loom cannot reach — is asserted by nothing and is delivered by **1-5 push and 3-8 pop calls out
of 80,000** per run; the "running in a couple of seconds" that justifies the scale is **0.01 s**
measured; and 4 × 20,000 is **20× smaller** than the uncommitted probe it exists to replace.
Concretely: add a real activation oracle (non-loom retry counters, asserted non-zero) and go to
at least 8 × 200,000 — which runs in ~0.1 s, still well inside this repo's fast-test convention.

### Q3 — is `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md`'s cap-6 decision still justified, and is the report methodologically sound?

**The decision is defensible; the report's stated justification is not, on two counts, and its
methodology has one real gap.**

What the report gets right, verified: the base SHA `47c81e90…` resolves; the raw logs carry the
host/rustc/base-SHA header and the per-arm per-thread breakdowns the CSV is built from; every
number in §3.1's table and §3.3's six-sample averages reproduces **exactly** when I re-aggregate
the CSV myself (6.12 / 13.12 / 20.64 and 0.375 / 0.190 / 0.201); the noise caveat in §4 is honest;
and §6's `[profile.release]`-vs-`[profile.bench]` correction is right (both sections are
byte-identical in the root `Cargo.toml` today — I checked).

What it gets wrong:

- **P2-1** — §3.2 drops the two caps (0 and 4) that its own CSV shows are *fairer* than 6, and §5
  + `CHANGELOG.md` then call 6 "the most fairness-conscious of the caps measured". Cap 0 beats
  cap 6 on `min/mean` in 7 of 8 arms and ties the eighth.
- **P2-2** — the entire report measures fairness as per-thread ops over a 1-second window and
  never measures per-call latency, which is the unit a consumer of a slot recycler experiences.
  Measured, that axis is 200× worse under cap 6 than under cap 0.
- **P3-2** — §3.1's "at EVERY thread count" heading is falsified by its own −0.4 % cell, and the
  "+17 % to +58 %" range that propagated into rustdoc and CHANGELOG excludes the whole 4-thread
  block; every cell is `n = 1` while §3.3 shows 2.2× rep-to-rep variance in the same arm.
- **P3-3** — the derivation scripts are not committed, so CLAUDE.md's "derived by one checked
  script" / "assert the arithmetic you print" rule is unmet — and it is exactly that assertion
  that would have caught P2-1 and P3-2.
- **P4-1/P4-2** — three misdirected internal cross-references and one unfinished sentence.

**Immutable source identity:** the report cites a base commit plus verbatim one-line patches and
a hard-failing restore check, which is the R29-6 rule's option (1)/(3) in spirit and is
reproducible byte-for-byte from the report. That part is fine. The gap is reproducing the
**report**, not the measurement.

### Q4 — is `pop`'s backoff-skip-when-empty optimisation correct?

**Yes.** Re-derived line by line at `src/lib.rs:1081-1100`:

`head = actual` is assigned **before** the `is_empty(actual)` test, and the loop's very first
statement is `if is_empty(head) { return None; }` (`:1025-1027`). So when the skip fires, the next
iteration reads the same word, takes the early return, and performs zero further work — backing
off first would only add latency to a call that is about to return. The skipped `spins += 1` is
dead for the same reason (the call returns before `spins` is read again). No outcome changes:
`is_empty` is still evaluated at loop top exactly as before, so which value a call returns is
untouched — only how fast it gets there. `None` remains linearizable: `actual` is a value the head
genuinely held at the CAS attempt, so there is a real point in time at which the stack was empty,
exactly as before the change. The code comment at `:1082-1091` states all of this accurately.

The only defect attached to this optimisation is coverage, not correctness: no test or loom model
reaches the `is_empty(actual) == true` arm — **P3-4**.

---

## Checked and clean (no finding)

- `_CHECK_BITS` is unbypassable: `pack` and `try_pack` force it with an explicit `let ()`,
  `INDEX_MASK` and `TAG_BITS` evaluate it in their initialisers, and `unpack` / `empty_index` /
  `is_empty` / `empty` all route through one of those. `TaggedIndexStack::new` reaches it via
  `empty()`.
- `try_pack`'s `1u64 << Self::TAG_BITS` never reaches the `<< 64` UB boundary: `TAG_BITS ∈
  [48, 63]` at every legal width, and `proptest_pack_unpack.rs:92-110` pins the width-1 / shift-63
  boundary explicitly.
- The `head` field still has **no** plain `store` — I re-grepped. The release-sequence premise
  behind `pop`'s `Acquire`-only success ordering holds.
- Both `#[should_panic]` ordering counterfactuals and the H-2 counterfactual are real and green;
  `model_with_oracle` now runs `verify` **inside** the `MODEL_LOCK` critical section
  (`loom_aba.rs:179-189`), so no caller can drop the guard early — the round-7 fix is structurally
  sound, not merely lint-dependent.
- `contention/push_pop`'s new `num_threads <= LINKS_SIZE` assert is sufficient for seed
  distinctness (`(t+1)·L/n − t·L/n ≥ ⌊L/n⌋ ≥ 1` when `L ≥ n`).
- `cargo package --list` is clean: 17 files, both licences, no scratch. `bench-scale-tool 0.1.0`
  and `proptest 1` both resolve from crates.io (`Cargo.lock:64-68`), so the dev-dependency graph
  will not block `cargo publish`.
- The crate declares no `[package.metadata.docs.rs]`, so CI's
  `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps` (default features) IS
  docs.rs's exact configuration — CLAUDE.md's docs.rs-feature-set rule has no gap here.
- Round-7's `--cfg loom` clippy row exists and is correctly placed inside `loom-alloc-global`,
  which already carries the job-level `RUSTFLAGS` (`.github/workflows/ci.yml:2480`).
- Round-7's P4-4 (`bench-iters.txt`) was investigated correctly: the manifest is the tracked
  **root** `bench-iters.txt`, written by `bench-scale-tool` at the workspace root, not a
  crate-local stray. Nothing to ignore.
- `docs/CORRECTNESS_OPEN_ITEMS.md` item 27 (the `compile_error!` guard not suppressing the
  cascading `E0432`) is still open and still accurate — a tracked, deliberate tradeoff, not a
  new finding.

## Refuted

- **"The backoff broke the ordering argument."** It did not: `core::hint::spin_loop()` has no
  memory-model effect, it sits strictly inside the `Err(actual)` arm after `head = actual`, and
  `spins` is a per-call `let mut` declared before each loop (`:865`, `:1023`). Verified by reading
  the code, not the comment.
- **"The release-active rule-4 guard costs throughput."** No evidence for it here: the guard is
  two integer comparisons on a path that already executes a `lock cmpxchg`, and the panic path is
  `#[cold] #[inline(never)]`. My probe runs saw no cap-6 wall-time change attributable to it.
- **"`threaded_conservation` is vacuous."** It is not — the exact-multiset drain would catch loss
  or duplication. It is *under-powered and over-claimed* (**P3-1**), which is a different charge.

---

## Suggested task queue for round 8

| # | Finding | Task |
|---|---|---|
| 1 | **P2-1** | Restore caps 0 and 4 to the gate report's §3.2 fairness table; reword §5 + `CHANGELOG.md:188` to `src/lib.rs`'s already-correct hedge (cap 6 = compromise, not fairness optimum); fix §3.2's self-contradicting heading. |
| 2 | **P2-2** | Add a per-call tail-latency axis to the sweep report, and one paragraph to `BACKOFF_SPIN_CAP`'s rustdoc + README stating that `push`/`pop` are lock-free but **not** starvation-free and that the backoff trades tail latency for throughput. |
| 3 | **P3-1** | Give `threaded_conservation.rs` a real activation oracle (non-loom retry counters) and raise its scale to ≥ 8 × 200,000; correct the runtime sentence. |
| 4 | **P3-2 + P3-3** | Fix the "+17 % to +58 %" range and the "EVERY thread count" heading; commit the sweep driver + aggregation script with in-script assertions for every published ratio and superlative. |
| 5 | **P3-4** | Add the 1-element/2-popper loom model covering `pop`'s empty-`actual` retry arm. |
| 6 | **P3-5** | Correct `bootstrap.rs`'s loom-shim comment to enumerate its three deliberate divergences. |
| 7 | **P4 bundle** | P4-1/P4-2 (report cross-refs + garbled sentence), P4-3 (uncited 51.56 ns), P4-4 (README wrapping), P4-5 (`#[track_caller]` asymmetry), P4-6 (`### Behavior` in a first release), P4-7 (MSRV row), P4-8 (triplication), P4-9 (`# Panics` allocator note). |

**Findings by priority: P0 = 0, P1 = 0, P2 = 2, P3 = 5, P4 = 9.**
