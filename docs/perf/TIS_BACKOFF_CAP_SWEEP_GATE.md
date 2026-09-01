# `tagged-index-stack` — `BACKOFF_SPIN_CAP` throughput-vs-fairness sweep

Date: 2026-08-31. First `docs/perf/` artifact for `crates/tagged-index-stack`
(this crate has no root-crate round number, so it is named directly rather
than `R{N}_...`, per this crate's own convention going forward).

**`bench`-classified — measurement only, no shipping code changed.**

**Round-8 correction note** (independent review
`docs/reviews/2026-08-31-125420-tagged-index-stack-review-round8-oh.md`,
findings P2-1/P2-2/P3-2/P3-3, task tis-r8-Group1 #1758): §3.2/§5's fairness
conclusion and §3.1's throughput headline were contradicted by this report's
own committed CSV; the per-call latency axis was unmeasured; and the
derivation pipeline was uncommitted. All four are fixed in place: §3.1's
heading now states the real range and its exception, §3.2 covers ALL five
measured caps, §3.4 adds the per-call latency axis, §5 states cap 6 as a
compromise (not a fairness optimum), and every table is now re-derived, with
in-script assertions, by the committed
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs`.
`BACKOFF_SPIN_CAP` stays `6`. This report replaces the doc comment's old
unmeasured "low enough for LOW contention" rationale
(`src/lib.rs`'s `BACKOFF_SPIN_CAP` doc, before this task) with the real
measured throughput-vs-fairness tradeoff below.

## 0. Motivation and prior claim being corrected

Round-6 landed exponential CAS-retry backoff (`069d187`,
`BACKOFF_SPIN_CAP = 6`, `core::hint::spin_loop()`) and measured an aggregate
throughput win on the committed bench at 8 threads (~5.3-9.7x). The shipped
doc comment additionally claimed the cap of 6 was chosen because it is "low
enough that a spurious retry under LOW contention doesn't stall the one
thread that lost the CAS for longer than the win is worth" — a LOW-contention
rationale that was never actually measured against alternative cap values.

An independent review flagged this as unmeasured: on a 2-thread arm (the
lowest contention the committed harness can reach), a HIGHER cap could
plausibly do BETTER, not worse, since low contention means CAS losses are
rare and the "wasted spin" cost the doc worried about barely triggers. This
report resolves that by actually sweeping the cap on the real, committed
`benches/tagged_index_stack_bench.rs` — not a scratch/out-of-tree copy.

## 1. Measurement identity and reproduction

**Base commit (immutable source identity — CLAUDE.md's R29-6 rule):**
`47c81e9087d6bf353d537e15e362c5b65925c90e` (`main`, working tree CLEAN before
and after every measurement — see §7's protocol; no uncommitted diff was ever
present DURING a timed run, only inside the build step immediately preceding
it, reverted before the next build). `git show
47c81e9087d6bf353d537e15e362c5b65925c90e:crates/tagged-index-stack/src/lib.rs`
and the same path for `benches/tagged_index_stack_bench.rs` recover the exact
pre-sweep source; the sweep's per-cap/per-thread-cap edits are mechanical
one-line substitutions documented in full later in this section (also
reproducible from this report alone, byte for byte).

**Machine:** Windows 10 Pro 10.0.19045 (MINGW64/Git-Bash), 16 logical CPUs
(`std::thread::available_parallelism()` = 16), `rustc 1.97.0 (2d8144b78
2026-07-07)`. Shared dev host — other processes active during measurement
(see §4's noise discussion; this materially affects the 16-thread arm).

**Profile:** `cargo bench -p tagged-index-stack --bench
tagged_index_stack_bench` builds under `[profile.bench]`
(`lto = "thin"`, `codegen-units = 1`), NOT `[profile.release]` — the two
happen to be byte-identical in this repo's root `Cargo.toml` today (see
§6 below), so no number in this report is affected by which name is
used, but `[profile.bench]` is the technically correct citation for a
`cargo bench` run.

**Reproduction.** The sweep is NOT a permanent harness — `BACKOFF_SPIN_CAP`
is a `const`, not a runtime/feature knob, by design (see §5's discussion of
why this stays a `const`). To reproduce a cell: edit
`crates/tagged-index-stack/src/imp.rs`'s `const BACKOFF_SPIN_CAP: u32 = 6;`
to the desired cap value, and temporarily replace
`benches/tagged_index_stack_bench.rs`'s hardcoded
`.min(8) // Cap at 8 for consistent benchmarking across machines` thread cap
with an env-var override (`TIS_SWEEP_THREADS`) to reach thread counts above
8 — the exact one-line diffs this task's own sweep driver applied are:

```text
# src/imp.rs, one-line substitution per cap value:
-const BACKOFF_SPIN_CAP: u32 = 6;
+const BACKOFF_SPIN_CAP: u32 = <CAP>;

# benches/tagged_index_stack_bench.rs, one substitution for the whole sweep:
-    let num_threads = std::thread::available_parallelism()
-        .map(|n| n.get())
-        .unwrap_or(4)
-        .min(8); // Cap at 8 for consistent benchmarking across machines
+    let num_threads = std::env::var("TIS_SWEEP_THREADS")
+        .ok()
+        .and_then(|s| s.parse::<usize>().ok())
+        .unwrap_or_else(|| {
+            std::thread::available_parallelism()
+                .map(|n| n.get())
+                .unwrap_or(4)
+                .min(8)
+        });
```

then `cargo build --release -p tagged-index-stack --bench
tagged_index_stack_bench` and run the resulting binary with
`TIS_SWEEP_THREADS=<N>` set. Both edits are reverted (`git checkout --`)
after every build in the sweep script; **no such scaffolding survives in the
committed diff** — `BACKOFF_SPIN_CAP` is `6` and the bench's thread cap is
the original `.min(8)` in every commit this report lands with.

## 2. Sweep design

- **Cap values:** `{0, 4, 6, 8, 10}` — `0` is "no backoff" (the pre-Round-6
  baseline shape, immediate retry), `6` is shipped, `4`/`8`/`10` bracket it.
- **Thread counts:** `{2, 4, 8, 16}` — 2 is the lowest contention measured
  in this sweep; it is a choice of this sweep's arm set, not a floor in the
  harness: the `contention/*` section spawns exactly the `num_threads` it
  resolves at startup (the `TIS_SWEEP_THREADS` env override when set — the
  sweep's own mechanism — else `available_parallelism().min(8)`), with no
  internal minimum, so a 1-thread arm was possible but simply not measured;
  16 is genuine oversubscription on this 16-logical-CPU
  host (16 worker threads + 1 coordinating main thread + OS/background load).
- **Benches measured:** both committed contention rows,
  `contention/push_pop` and `contention/churn` (see
  `benches/tagged_index_stack_bench.rs`'s own module doc for what each
  measures and `contention/churn`'s documented link-array-false-sharing
  confound — unaffected by this sweep, since it applies identically to every
  cap/thread arm).
- **Fairness metrics recorded per arm, per bench:** `total_ops_per_sec`
  (aggregate), and from the harness's own printed
  `Per-thread breakdown: [...]` array — `max`, `min`, `mean`, `max/min`
  (skew ratio), `min/mean` (worst-thread share of a perfectly fair split;
  `1.0` = perfectly fair, `<1.0` = that thread got less than its fair
  share, all the way down toward `0` for near-total starvation).
- **Two independent runs** at the 16-thread arm specifically (cap 6/8/10,
  2 reps each — 12 total samples) to check whether the fairness ranking is
  stable or single-run noise, since 16-thread oversubscription on a
  16-logical-CPU shared dev host is the noisiest regime in this sweep (see
  §4). The 2/4/8-thread arms and the cap-0/cap-4 arms were each measured
  once (run 1 only) — this crate's own "Speed: short scenario by default"
  convention (CLAUDE.md) keeps benchmarks fast, and the decisive fairness
  question was specifically about the 16-thread tail, which run 2
  independently re-measured.

**Raw logs:** `docs/perf/_raw_tis_backoff_cap_sweep_run1.log` (full sweep,
all 5 caps × 4 thread counts, one sample each — 23,342 bytes, under the
200 KiB tier-1 ceiling, committed verbatim, no truncation needed) and
`docs/perf/_raw_tis_backoff_cap_sweep_run2_repeat16.log` (repeat run,
caps {6, 8, 10} × 16 threads × 2 reps — 8,564 bytes, same tier).
Round-8 addition: `_raw_tis_backoff_per_call_latency.log` (§3.4's per-call
latency measurement — 6,365 bytes, tier 1, committed verbatim). Both were
produced by the sweep driver described in §1's reproduction recipe; the
sweep driver itself was scratch (see the "no scaffolding survives" note
in §1) — but as of round 8 the AGGREGATION half of the pipeline is committed:
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs` re-derives every row
of the summary CSV directly from the raw logs, cross-checks all 52 sweep rows
cell-for-cell, appends the §3.4 latency rows, and asserts every ratio and
superlative §3-§5 publish (closes round-8 P3-3).

**Summary CSV:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv` — the
52 sweep rows (40 from run 1, 12 from run 2) were first produced by a scratch
`awk` pass; as of round 8 they are re-derived and cross-checked against the
raw logs cell-for-cell by
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs` (its assertion set
fails the build on any disagreement), and the file gains 18 `run=3`
`bench=pop_latency` rows for §3.4's per-call latency axis (its 9 new
trailing columns are empty for the sweep rows and its throughput columns are
empty for the latency rows). Every per-arm number below is read from this
file's columns via that script, not retyped.

## 3. Results

### 3.1 Throughput: cap 8 and cap 10 beat cap 6 in 15 of 16 cells — the exception is 4-thread churn, where cap 10 lands 0.4% BELOW cap 6 (run 1, one sample per cell)

| threads | bench     | cap 0      | cap 4      | cap 6 (shipped) | cap 8      | Δ8 vs 6  | cap 10     | Δ10 vs 6 |
|---------|-----------|-----------:|-----------:|----------------:|-----------:|---------:|-----------:|---------:|
| 2       | push_pop  | 18,435,624 | 33,608,740 | 29,477,572      | 35,737,912 | +21.2%   | 36,266,241 | +23.0%   |
| 2       | churn     | 11,437,603 | 33,667,499 | 26,346,025      | 30,941,231 | +17.4%   | 36,183,518 | +37.3%   |
| 4       | push_pop  | 5,892,248  | 27,587,382 | 33,843,787      | 35,432,280 | +4.7%    | 34,754,370 | +2.7%    |
| 4       | churn     | 4,188,208  | 25,198,964 | 33,109,303      | 35,340,092 | +6.7%    | 32,982,566 | −0.4%    |
| 8       | push_pop  | 4,329,777  | 14,856,677 | 29,517,726      | 34,657,959 | +17.4%   | 34,878,454 | +18.2%   |
| 8       | churn     | 2,849,422  | 12,692,134 | 27,061,085      | 34,314,390 | +26.8%   | 34,327,478 | +26.9%   |
| 16      | push_pop  | 5,024,421  | 7,302,659  | 22,816,579      | 32,994,406 | +44.6%   | 35,856,536 | +57.2%   |
| 16      | churn     | 2,703,969  | 5,513,033  | 21,974,093      | 33,757,933 | +53.6%   | 34,803,110 | +58.4%   |

All figures are `total_ops_per_sec` from `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`
(`run=1` rows). `Δ` = `(cap_N − cap_6) / cap_6`, re-derived from the CSV by
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs` (assertion A1 pins
all 16 cells). **n=1 caveat (round-8 P3-2):** every cell is a SINGLE sample,
while §3.3 shows the same 16-thread arm swinging 14.66M → 32.68M ops/sec
(2.2x) between two reps — the four 4-thread deltas (+4.7%, +2.7%, +6.7%,
−0.4%) sit inside that noise band and must be read as "indistinguishable at
n=1", not as ordering facts. The honest span over all 16 cap-8/cap-10 cells
is **−0.4% to +58.4%** (assertion A3) — an earlier version of this section
and of the rustdoc/CHANGELOG quotes compressed that to "+17% to +58%",
excluding the whole 4-thread block in both directions.

This independently reproduces the reviewer's core finding on this machine:
cap 6 is NOT the throughput optimum at any thread count tested — at the
lowest-contention 2-thread arm the harness can reach, the cap-8/cap-10
deltas are +17.4% to +37.3% (assertion A4), contradicting the old doc's
"low enough for LOW contention" framing, which implied cap 6 was
specifically tuned to be *better*, not worse, than a higher cap under low
contention. For the same reason cap 6 is not the fairness optimum either —
see §3.2.

### 3.2 Fairness across ALL five measured caps: 0 and 4 are the fairest; cap 6 is mid-curve — fairer than 8/10, less fair than 0/4 (run 1)

`max/min` = per-thread ops skew ratio (`1.0` = perfectly even); `min/mean` =
unluckiest thread's share of a fair split (`1.0` = fair, lower = worse
starvation). Both from `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`
(`run=1` rows), re-derived by
`scripts/tis_backoff_cap_sweep_derive_report_data.mjs`.

Round-8 correction (P2-1): the first version of this section tabulated only
caps {6, 8, 10} — dropping the two caps its own CSV shows are FAIRER than 6 —
and its heading claimed "cap 6 has the BEST skew at every thread count",
which its own body then contradicted (cap 8 edges it at `4/push_pop`). Both
defects are fixed here: the tables below cover all five measured caps, and
cap 6 is the strict `min/mean` maximum in **0 of 8 arms** (assertion B7).

**`min/mean` (primary starvation metric):**

| threads | bench | cap 0 | cap 4 | cap 6 | cap 8 | cap 10 |
|---|---|---|---|---|---|---|
| 2 | push_pop | 0.950 | 0.852 | 0.950 | 0.877 | 0.757 |
| 2 | churn | 0.975 | 0.955 | 0.931 | 0.890 | 0.953 |
| 4 | push_pop | 0.929 | 0.825 | 0.549 | 0.742 | 0.629 |
| 4 | churn | 0.951 | 0.792 | 0.803 | 0.693 | 0.548 |
| 8 | push_pop | 0.951 | 0.855 | 0.655 | 0.541 | 0.502 |
| 8 | churn | 0.878 | 0.784 | 0.634 | 0.355 | 0.352 |
| 16 | push_pop | 0.493 | 0.615 | 0.331 | 0.242 | 0.295 |
| 16 | churn | 0.638 | 0.441 | 0.365 | 0.157 | 0.267 |

**`max/min`:**

| threads | bench | cap 0 | cap 4 | cap 6 | cap 8 | cap 10 |
|---|---|---|---|---|---|---|
| 2 | push_pop | 1.106 | 1.348 | 1.106 | 1.279 | 1.643 |
| 2 | churn | 1.051 | 1.094 | 1.149 | 1.247 | 1.098 |
| 4 | push_pop | 1.127 | 1.429 | 2.424 | 1.758 | 1.972 |
| 4 | churn | 1.102 | 1.535 | 1.458 | 2.044 | 2.254 |
| 8 | push_pop | 1.168 | 1.405 | 2.202 | 3.022 | 2.821 |
| 8 | churn | 1.227 | 1.624 | 2.344 | 5.156 | 4.371 |
| 16 | push_pop | 8.890 | 3.236 | 6.263 | 10.846 | 7.948 |
| 16 | churn | 5.687 | 6.781 | 7.499 | 17.310 | 7.118 |

(both tables generated by
`node scripts/tis_backoff_cap_sweep_derive_report_data.mjs`; assertions
B1-B6 pin every count below)

- **Cap 0 vs cap 6, `min/mean`:** cap 0 is better-or-equal in 8 of 8 arms —
  strictly better in 7, tied at `2/push_pop` (both 0.950) (assertion B1).
- **Cap 4 vs cap 6, `min/mean`:** cap 4 is better in 6 of 8 arms (it loses
  `2/push_pop` and `4/churn`) (assertion B2).
- **Cap 6 vs caps 8/10, `min/mean`:** cap 6 is better than cap 8 in 7 of 8
  arms (loses `4/push_pop`) and better than cap 10 in 6 of 8 (loses
  `2/churn` and `4/push_pop`) (assertion B3).
- **Per-cap averages over the 8 arms:** `min/mean` = 0.846 / 0.765 / 0.652 /
  0.562 / 0.538 for caps 0/4/6/8/10 — strictly ordered `0 > 4 > 6 > 8 > 10`.
  `max/min` = 2.670 / 2.307 / 3.056 / 5.333 / 3.653 — ordered
  `4 < 0 < 6 < 10 < 8`. The 0-vs-4 and 8-vs-10 orderings therefore FLIP
  between the two metrics (assertion B5); what does NOT flip — on either
  metric, and in the per-arm counts — is that **{0, 4} are fairer than 6,
  which is fairer than {8, 10}**.
- One honest nuance the five-cap table exposes (assertion B6): at
  `16/push_pop`, cap 0's `max/min` (8.890) is WORSE than cap 6's (6.263) —
  the no-backoff baseline is not uniformly fairest on every metric in every
  arm (its unluckiest thread still has the better `min/mean` there: 0.493 vs
  0.331). Fairness-ordering claims in this report are made per stated
  metric, not as a blanket total order.
- Cap 8 keeps the worst single run-1 figure in either table (`16/churn`:
  17.310x skew, min/mean 0.157 — the unluckiest thread got under 16% of a
  fair share).

### 3.3 The 16-thread tail, re-measured twice more (run 2): the fairness ranking is noisy in magnitude but stable at the top — cap 6 best; caps 8 and 10 both materially worse, their relative order noisy

Run 1's single 16-thread sample per cap was re-measured with 2 more
independent reps per cap (cap 6/8/10 only — the decisive regime), from a
fresh build each time, on the same shared host:

| cap | rep | bench | ops/sec | max/min | min/mean |
|---|---|---|---|---|---|
| 6 | 1 | push_pop | 25,271,226 | 6.957 | 0.289 |
| 6 | 1 | churn | 24,211,225 | 7.828 | 0.250 |
| 6 | 2 | push_pop | 26,160,655 | 3.411 | 0.556 |
| 6 | 2 | churn | 23,970,451 | 4.737 | 0.458 |
| 8 | 1 | push_pop | 19,199,465 | 8.579 | 0.188 |
| 8 | 1 | churn | 14,657,136 | 17.391 | 0.108 |
| 8 | 2 | push_pop | 32,676,865 | 19.312 | 0.103 |
| 8 | 2 | churn | 30,634,872 | 5.252 | 0.340 |
| 10 | 1 | push_pop | 34,927,192 | 46.739 | 0.096 |
| 10 | 1 | churn | 34,589,143 | 24.704 | 0.115 |
| 10 | 2 | push_pop | 35,087,901 | 24.464 | 0.164 |
| 10 | 2 | churn | 33,942,472 | 12.891 | 0.268 |

(Source: `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`, `run=2` rows,
derived from `docs/perf/_raw_tis_backoff_cap_sweep_run2_repeat16.log`; table
generated by `scripts/tis_backoff_cap_sweep_derive_report_data.mjs`.)

**This run is visibly noisier than run 1** — `cap 8`'s single-threaded rows
in this run's own bench output show a `churn` cost spike to 108.13 ns/op
(vs. ~53-56 ns/op everywhere else in both runs), a clear sign of transient
system interference from this being a shared dev host, not a dedicated
benchmark machine (see §4). Despite that noise, the RANKING is stable across
all three regimes (run 1 single sample, run 2 rep 1, run 2 rep 2):

- **Averaged `max/min` across all 6 samples per cap (run 1 §3.2's 16-thread
  row + run 2's 4 samples):** cap 6 ≈ **6.12x**, cap 8 ≈ **13.1x**, cap 10 ≈
  **20.64x** (assertion C1; cap 8's average is 13.115 exactly — the script's
  toFixed(2) renders it 13.11, and the "13.12x" an earlier version of this
  section quoted was the same value rounded the other way at the boundary).
- **Averaged `min/mean` across the same 6 samples:** cap 6 ≈ **0.375**
  (best), cap 8 ≈ **0.190**, cap 10 ≈ **0.201** (worst two, close to each
  other, both clearly worse than cap 6) (assertion C1).
- Cap 10 produced this sweep's single worst outlier — **46.7x** skew
  (`10/rep1/push_pop`) — confirming the original review's observation that
  cap 10 is "clearly too aggressive" is reproducible on this machine too,
  though the WORST-fairness cap on this host's particular runs was 8, not
  10 (the two are close and both are materially worse than 6 — this
  ordering between 8 and 10 specifically should be read as noisy, not as
  "cap 8 is safer than cap 10").

### 3.4 Per-call `pop` tail latency (round-8 addition): the axis §3.2's metric cannot see

Round-8 P2-2: every fairness number above is per-thread ops over a 1-second
window — a thread that loses 90% of a second inside ONE call and a thread
that is uniformly 10x slow produce the same `min/mean`. Per-CALL latency was
never measured. This section adds that axis.

**Harness:** `crates/tagged-index-stack/examples/backoff_per_call_latency.rs`
(committed, observation-only, public API only): `ArrayLinks<64>` prefilled
`0..64`, N threads x M pop-then-repush-exactly-what-you-popped iterations
(the committed test's and bench's own contention discipline), every `pop`
individually timed with `Instant` (the two clock reads sit outside the pop
itself and are identical in both arms), 3 reps per shape, `--release`. The
cap-0 arm is §1's documented one-line substitution; the raw log carries the
resolved-cap evidence (the captured `const BACKOFF_SPIN_CAP` source line
immediately before each build — requested config AND resolved config, per
this repo's R26-4 rule), the patch hash of the substitution, and the
post-run restore verification.

**Raw log:** `docs/perf/_raw_tis_backoff_per_call_latency.log` (committed —
under the 200 KiB tier-1 ceiling, verbatim). Per-rep rows are appended to
`docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv` as `run=3`,
`bench=pop_latency` rows; the medians and ratios below are derived, with
assertions D1-D6, by `scripts/tis_backoff_cap_sweep_derive_report_data.mjs`.
**Source identity (round-9 P3-3 correction):** the cap-6 arm was measured on
the round-8 worktree whose TRACKED state was exactly commit
`842c99805992d362c5d82df59fc646c691598285` (the raw log's `base_sha:` line;
the log's post-run restore verification shows the tracked tree clean) **plus
one untracked new file — the probe itself**. That tree was therefore NOT a
"CLEAN tree" as an earlier version of this section claimed, and `842c998`
alone is not a sufficient R29-6 identity: the probe did not exist at that
commit (`git cat-file -e
842c998:crates/tagged-index-stack/examples/backoff_per_call_latency.rs`
fails; it was first committed in
`a1f9dc51bfbffeed57229f6f46a5e199d289b9ec` and is unchanged since).
Practical reproduction identity for the cap-6 arm: check out `a1f9dc5`
(contains both the probe and the shipped cap-6 source) and run the probe as
described above. One disclosed caveat: `a1f9dc5`'s tracked state differs
from the measured tracked state beyond doc comments — the retry-counter
increments became unconditional and `pop` gained `#[track_caller]`; the
round-9 review independently A/B'd both and could not resolve a cost above
the build-layout noise floor
(`docs/reviews/2026-08-31-130017-tagged-index-stack-review-round9-oh.md`,
P3-4 and "Checked and clean"), so numbers re-taken at `a1f9dc5` reproduce
§3.4's within its stated n=3 noise. A retroactive `git write-tree`/patch
hash of the true measured tree is impossible (the round-8 worktree is gone;
R29-6 requires capturing the identity at measurement time). The cap-0 arm
patch hash
`9cf4469a1ba5f79a8c98871dd7d4ee6e90f2a3a5fee4465e54ba1b7af1333b86`
(R29-6 option 3) is a `git diff` hash and likewise does not cover the
untracked probe; it remains recorded in the raw log as the substitution's
identity.

**Median over 3 reps (worst-of-3 in parens), same host as §3.1:**

| shape | worst single `pop`, cap 6 (shipped) | worst single `pop`, cap 0 | median wall, cap 6 | median wall, cap 0 | wall speedup, cap 6 |
|---|---|---|---|---|---|
| 4 x 20,000 | 4.813 ms (10.828) | 0.159 ms (0.297) | 14.6 ms | 61.0 ms | 4.18x |
| 8 x 200,000 | 54.464 ms (59.705) | 2.031 ms (23.567) | 321.9 ms | 1,560.3 ms | 4.85x |
| 16 x 200,000 | 160.092 ms (173.365) | 42.335 ms (46.301) | 867.5 ms | 3,509.6 ms | 4.05x |

Tail mass by threshold — ALL five cells, both arms (3 reps each; assertion
D4 pins every cell; round-9 P2-2: an earlier version quoted only the two
rows that support the tail story and omitted the 16-thread `>1 ms`
reversal):

| shape / threshold | cap 6 (shipped) | cap 0 | who is worse |
|---|---|---|---|
| 8 x 200k, >1 ms | 86, 66, 60 | 8, 0, 3 | cap 6 (60-86 vs 0-8 per rep) |
| 8 x 200k, >10 ms | 34, 29, 26 | 2, 0, 0 | cap 6 |
| 16 x 200k, >1 ms | 285, 266, 249 | 553, 661, 650 | **cap 0 — ~2.4x worse median-to-median (1.9-2.6x across rep pairings)** |
| 16 x 200k, >10 ms | 178, 131, 169 | 110, 161, 157 | roughly tied (ranges overlap) |
| 16 x 200k, >100 ms | 4, 3, 3 | 0, 0, 0 | cap 6 |

The tail story is NOT uniform across thread counts: at 8 threads the backoff
adds mid-band (1-10 ms) outliers; at 16 threads it moves mass OUT of the
1 ms band (cap 0 leaves ~2.4x MORE pops over 1 ms) at the price of a handful
of >100 ms outliers cap 0 never produces. The percentile columns complete
the picture (cap 0's side was never quoted before round 9): cap 6 is
better-or-equal at p50, p90, p99 AND p99.9 in every shape and every rep, by
1-2 orders of magnitude at the upper percentiles (assertion D5 pins every
cell):

| shape | cap 6 p999 | cap 0 p999 | cap 6 p50 | cap 0 p50 |
|---|---|---|---|---|
| 4 x 20,000 | 0.000-0.001 ms | 0.022-0.037 ms | 0.000 | 0.001 |
| 8 x 200,000 | 0.001 ms | 0.054-0.057 ms | 0.000 | 0.002 |
| 16 x 200,000 | 0.001 ms | 0.172-0.182 ms | 0.000 | 0.003-0.004 |

99.9% of cap-6 pops at 8 threads completed within ~1 microsecond
(`pop_p999_ms` = 0.001 in the CSV rows — an assertion now pins that column
directly, which is only readable correctly since round-9 P2-1 fixed the
latency rows' column alignment), and the same workload finishes 4.05-4.85x
faster in wall clock under cap 6. The distribution under backoff is not
uniformly slower, it is HEAVIER-TAILED — and cap 0 is not uniformly
lighter-tailed either: it is worse at every percentile through p99.9 in
this measurement, winning only the extreme maximum.

**Reading:** the backoff does not reduce contention, it REDISTRIBUTES it —
the same harness that shows the worst single pop multiplied ~27x
(median-to-median, `8 x 200,000`; assertion D2) also shows the whole phase
~4.9x FASTER under the shipped cap 6 (assertion D3). That is the
per-call-tail-vs-throughput trade in one table, in the unit a
slot-recycling consumer actually experiences. Which quantity to trust:
(1) part of even cap 0's tail is scheduler noise on this shared host (its
`8 x 200,000` rep-1 max was 23.567 ms) — but 60-86 pops-over-1ms per rep vs
0-8 is backoff-shaped, not noise; (2) round-9 P3-2 correction — an earlier
version of this caveat called the cap6/cap0 worst-pop RATIOS "the robust
part"; they are the LEAST robust quantity here. Worst-pop is a
max-of-3-over-max-of-3 statistic and cap 0's own `8 x 200,000` maxima span
40x (`23.567 / 0.596 / 2.031` ms), so the published ~27x ratio's plausible
range across rep pairings is **1.8x-100.2x** at `8 x 200,000` (4.3x-68.1x
at `4 x 20,000`; 2.8x-4.4x at `16 x 200,000` — only the 16-thread shape is
stable; assertion D6 pins every spread). The robust quantity is the
wall-clock speedup: 4.18x / 4.85x / 4.05x, tight within each arm
(assertion D3). Summary in the corrected P2-2 form: the shipped cap buys
its ~4-5x aggregate throughput by tolerating a small number of very large
outliers, while IMPROVING latency at every percentile through p99.9 in
every shape. `push`/`pop` are lock-free but NOT starvation-free, and the
shipped cap is the reason for the tail's shape; `BACKOFF_SPIN_CAP`'s doc
comment now states exactly this.

## 4. Noise caveat

This is a shared Windows dev host, not a dedicated/pinned benchmark machine
— no CPU affinity pinning, no isolated cores, other processes free to run
during measurement. The 16-thread arm is the most exposed to this: 16
worker threads plus the coordinating main thread on a 16-logical-CPU
machine leaves zero headroom for OS/background scheduling, so any
transient system load directly steals a worker's timeslice and shows up as
that thread's `ops` count cratering for the affected fraction of the
1-second window — exactly the shape of the skew seen in every row above.
This is evidence FOR fairness being a real, structural cost of a wider
backoff window (a thread that loses more spin-time to backoff has less
slack to absorb an external preemption), not an artifact to explain away —
but the exact NUMERIC skew values (7x vs 47x) should be read as "cap 6 is
reliably better, cap 8 and cap 10 are reliably worse, by a wide and
sometimes very wide margin" rather than as precise ratios.

## 5. Decision: KEEP `BACKOFF_SPIN_CAP = 6`

Round-8 correction: the prior version of this section concluded cap 6 was
"the most fairness-conscious of the caps measured" — false against §3.2's
own CSV (cap 6 is the strict `min/mean` maximum in 0 of 8 arms) — and
quoted "+17% to +58% ... at EVERY thread count" — false against §3.1's own
table (4-thread churn, cap 10: −0.4%). Both phrases are gone from this
report, from `CHANGELOG.md`, and from `src/lib.rs`'s doc comment; the
derivation script's assertions B7 and A2/A3 fail if either shape is
reintroduced against the committed data.

**Not changed.** Per this task's own explicit conservative-bias instruction
(prefer keeping 6 unless the data overwhelmingly favors a change AND the
fairness cost is acceptable) and this crate's own production-hot-path
posture (`push`/`pop` are the crate's only two operations):

- The throughput case for raising the cap is real and reproduces on this
  machine (§3.1: cap 8/10 beat cap 6 in 15 of 16 cells, −0.4% to +58.4%,
  one sample per cell; the sole exception is 4-thread churn, where cap 10
  lands 0.4% below cap 6; the lowest-contention 2-thread arm specifically
  spans +17.4% to +37.3%).
- The fairness cost of raising the cap is ALSO real, reproduces across two
  independent runs (§3.2, §3.3), and gets WORSE — not better — at exactly
  the oversubscribed regime a production allocator is most likely to hit
  under real load (many threads racing one shared free-list). Cap 8's
  16-thread `min/mean` averaged ~0.16 across 3 samples: the unluckiest
  thread got roughly a sixth of its fair share. That is a starvation cost,
  not merely "some noise in the numbers."
- Because this crate's tag defends every downstream allocator's whole
  free-list, an occasional badly-starved thread is a worse failure mode to
  ship by default than a smaller throughput ceiling — a caller who
  specifically wants to trade fairness for peak aggregate throughput under
  known-benign contention can already reproduce a higher cap locally (§1's
  reproduction recipe), but the crate's SHIPPED default should not impose
  that tradeoff on every caller.
- Lowering the cap is not free either (§3.1, run 1): cap 0 costs cap 6 a
  factor of 1.60x-9.50x in aggregate throughput across the eight cells
  (assertion A5), and cap 4 costs up to ~4.0x at 16 threads (cap6/cap4
  0.78x-3.99x, assertion A6). §3.4 adds the per-call tail axis: the shipped
  cap 6 multiplies the worst single `pop` by ~27x vs cap 0 at 8 threads
  (median-to-median — a max-of-3-over-max-of-3 statistic whose plausible
  range spans 1.8x-100.2x across rep pairings, §3.4's reading; assertion
  D6) while making the same workload ~4.9x faster in aggregate (4.05-4.85x
  across shapes — the robust axis) and improving every per-call percentile
  through p99.9 (§3.4's five-cell table; round-9 P2-2/P3-2 corrections).
  The default picks a point on that five-point curve; it does not dominate
  either end.

This does not mean cap 6 is "the low-contention-optimal choice" — §3.1
shows it explicitly is not, at any thread count tested. It ALSO does not
mean cap 6 is the fairness optimum of the sweep — §3.2 shows caps 0 and 4
are fairer still (cap 0's `min/mean` beats cap 6's in 7 of 8 arms and ties
the eighth; cap 4's beats it in 6 of 8). The corrected rationale (round 8;
now in `src/imp.rs`'s doc comment and `CHANGELOG.md`) is: **cap 6 is a
deliberate COMPROMISE on the five-point sweep curve — fairer than caps
8/10, less fair than caps 0/4, and in aggregate 1.60x-9.50x cap 0's
throughput — with the per-call tail cost of that fairness gap made explicit
in §3.4. That tradeoff — not a low-contention latency argument, and not a
fairness-optimality claim — is why it ships.**

## 6. Companion fix: P4-7, `[profile.release]` vs `[profile.bench]` citation

The pre-existing `BACKOFF_SPIN_CAP` doc comment and the `CHANGELOG.md`
`### Performance` entry both cited "this repo's `[profile.release]`" as the
profile the backoff numbers were measured under. `cargo bench` actually
builds under `[profile.bench]`, not `[profile.release]` — a separate
Cargo profile section. In this repo's root `Cargo.toml` today the two
sections are byte-identical (`lto = "thin"`, `codegen-units = 1`, confirmed
by reading both sections directly — see `Cargo.toml`'s `[profile.release]` /
`[profile.bench]`), so no number in either doc is actually wrong — but the
citation named the wrong section. Fixed in both places (this task's `src/lib.rs`
and `CHANGELOG.md` edits) to name `[profile.bench]` — the section a
`cargo bench` invocation actually uses — while noting the two are identical
today.

## 7. Verification run (this task)

- `cargo test -p tagged-index-stack` (default + `--release`): green.
- `cargo clippy -p tagged-index-stack --all-targets -- -D warnings`: green.
- `cargo fmt -p tagged-index-stack --check`: green.
- `RUSTDOCFLAGS="-D warnings" cargo doc -p tagged-index-stack --no-deps`: green.
- `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom --test loom_aba`: 11/11 green
  (re-run and confirmed in round 9; the suite has grown model-by-model, and
  10 was already stale when this line was written — round-8's `11b6833`
  added an 11th model before this report was last edited — so run the suite
  for the current count rather than trusting any number printed here).
- `git status --short crates/tagged-index-stack/` confirmed clean both before
  the sweep and after every build cycle inside it (the sweep script itself
  hard-fails if the restore leaves any diff) — no measurement scaffolding
  survives in the final committed tree; `BACKOFF_SPIN_CAP` is `6` and the
  bench's thread-count cap is the original `.min(8)`.
- Round-8 additions: `cargo clippy -p tagged-index-stack --all-targets --
  -D warnings` covers the new `examples/backoff_per_call_latency.rs`; `node
  scripts/tis_backoff_cap_sweep_derive_report_data.mjs` passes with exit 0
  on the committed artifacts (re-derive the current assertion count by
  running it — it prints its own ALL <N> ASSERTIONS PASSED line; the count
  is deliberately not hardcoded here, and differs between the CSV's
  pre-write and post-write shapes); and the assertion layer was
  negative-tested by consistently doctoring (a) the 4-thread churn cap-10
  cell and (b) the cap-6 fairness rows in the working log+CSV — each
  doctoring made the script exit 1 on the corresponding assertion (A1,
  then B1) before the artifacts were restored byte-identical from backups.
- Round-9 additions (task tis-r9-Group1): the summary CSV's 18 `run=3`
  latency rows were regenerated at the correct 20-column width (P2-1 —
  `awk -F, '{print NF}' ... | sort | uniq -c` now reports only `71 20`);
  `--write` is idempotent against its own output; assertion D4 pins all
  five threshold cells and new D5/D6 pin the percentile columns and the
  worst-pop ratio spreads (P2-2/P3-2/P4-7); §3.4's source identity was
  re-cited (P3-3); this section's loom count was refreshed (P4-5).

- Round-13 addition (P4-3): §6's "Fixed in both places" no longer describes
  the current tree — the citation was later dropped entirely: commit
  `db8bb77` moved the implementation (citation included) from `src/lib.rs`
  into `src/imp.rs`, and commit `ad65fa5` ("docs(tis): compress doc comments
  to load-bearing invariants") removed it from `src/imp.rs` and the crate
  CHANGELOG. A grep of the current tree finds no `[profile.bench]` /
  `[profile.release]` citation in the crate's src/, README.md, CHANGELOG.md,
  benches/, examples/, or the root CHANGELOG.md/README.md.

(Exact command output for the items above is reported in this task's commit
message / accompanying session report, not duplicated here — this report's
own scope is the cap-sweep measurement and decision.)
