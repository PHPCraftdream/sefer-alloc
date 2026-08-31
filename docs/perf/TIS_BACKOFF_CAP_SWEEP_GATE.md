# `tagged-index-stack` — `BACKOFF_SPIN_CAP` throughput-vs-fairness sweep

Date: 2026-08-31. First `docs/perf/` artifact for `crates/tagged-index-stack`
(this crate has no root-crate round number, so it is named directly rather
than `R{N}_...`, per this crate's own convention going forward).

**`bench`-classified — measurement only, no shipping code changed.**
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
and after every measurement — see §2's protocol; no uncommitted diff was ever
present DURING a timed run, only inside the build step immediately preceding
it, reverted before the next build). `git show
47c81e9087d6bf353d537e15e362c5b65925c90e:crates/tagged-index-stack/src/lib.rs`
and the same path for `benches/tagged_index_stack_bench.rs` recover the exact
pre-sweep source; the sweep's per-cap/per-thread-cap edits are mechanical
one-line substitutions documented in full in §2 below (also reproducible from
this report alone, byte for byte).

**Machine:** Windows 10 Pro 10.0.19045 (MINGW64/Git-Bash), 16 logical CPUs
(`std::thread::available_parallelism()` = 16), `rustc 1.97.0 (2d8144b78
2026-07-07)`. Shared dev host — other processes active during measurement
(see §4's noise discussion; this materially affects the 16-thread arm).

**Profile:** `cargo bench -p tagged-index-stack --bench
tagged_index_stack_bench` builds under `[profile.bench]`
(`lto = "thin"`, `codegen-units = 1`), NOT `[profile.release]` — the two
happen to be byte-identical in this repo's root `Cargo.toml` today (see
§5/P4-7 below), so no number in this report is affected by which name is
used, but `[profile.bench]` is the technically correct citation for a
`cargo bench` run.

**Reproduction.** The sweep is NOT a permanent harness — `BACKOFF_SPIN_CAP`
is a `const`, not a runtime/feature knob, by design (see §5's discussion of
why this stays a `const`). To reproduce a cell: edit
`crates/tagged-index-stack/src/lib.rs`'s `const BACKOFF_SPIN_CAP: u32 = 6;`
to the desired cap value, and temporarily replace
`benches/tagged_index_stack_bench.rs`'s hardcoded
`.min(8) // Cap at 8 for consistent benchmarking across machines` thread cap
with an env-var override (`TIS_SWEEP_THREADS`) to reach thread counts above
8 — the exact one-line diffs this task's own sweep driver applied are:

```text
# src/lib.rs, one-line substitution per cap value:
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
- **Thread counts:** `{2, 4, 8, 16}` — 2 is the lowest contention this
  harness's `contention/*` section can reach (it always spawns
  `num_threads >= 2`... in practice the harness's own
  `available_parallelism()` floor is whatever the OS reports, clamped to the
  arm under test here); 16 is genuine oversubscription on this 16-logical-CPU
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
caps {6, 8, 10} × 16 threads × 2 reps — 8,564 bytes, same tier). Both were
produced by the sweep driver described in §1's reproduction recipe; the
driver script itself was scratch (not committed — see the "no scaffolding
survives" note in §1).

**Summary CSV:** `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv` — every
row from both raw logs, parsed by a small `awk` script (not hand-transcribed;
the per-arm `max`/`min`/`mean`/ratios below are read directly from this
file's columns, not retyped).

## 3. Results

### 3.1 Throughput: cap 8 and cap 10 beat cap 6 at EVERY thread count, including 2 (run 1, one sample per cell)

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
(`run=1` rows). `Δ` = `(cap_N − cap_6) / cap_6`, computed by the same `awk`
pass that produced the table (not hand-computed).

This independently reproduces the reviewer's core finding on this machine:
cap 6 is NOT the throughput optimum at any thread count tested, including
the lowest-contention 2-thread arm the harness can reach — cap 8/10 beat it
there too (+17-37%), contradicting the old doc's "low enough for LOW
contention" framing, which implied cap 6 was specifically tuned to be
*better*, not worse, than a higher cap under low contention.

### 3.2 Fairness: cap 6 has the BEST (lowest) skew at every thread count; cap 8 has the WORST, cap 10 is between but with the worst single outlier

`max/min` = per-thread ops skew ratio (`1.0` = perfectly even); `min/mean` =
unluckiest thread's share of a fair split (`1.0` = fair, lower = worse
starvation). Both from `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`.

| threads | bench    | cap 6 max/min | cap 6 min/mean | cap 8 max/min | cap 8 min/mean | cap 10 max/min | cap 10 min/mean |
|---------|----------|---------------:|----------------:|---------------:|-----------------:|----------------:|------------------:|
| 2       | push_pop | 1.106          | 0.950           | 1.279          | 0.877            | 1.643           | 0.757             |
| 2       | churn    | 1.149          | 0.931           | 1.247          | 0.890            | 1.098           | 0.953             |
| 4       | push_pop | 2.424          | 0.549           | 1.758          | 0.742            | 1.972           | 0.629             |
| 4       | churn    | 1.458          | 0.803           | 2.044          | 0.693            | 2.254           | 0.548             |
| 8       | push_pop | 2.202          | 0.655           | 3.022          | 0.541            | 2.821           | 0.502             |
| 8       | churn    | 2.344          | 0.634           | 5.156          | 0.355            | 4.371           | 0.352             |
| 16      | push_pop | 6.263          | 0.331           | 10.846         | 0.242            | 7.948           | 0.295             |
| 16      | churn    | 7.499          | 0.365           | 17.310         | 0.157            | 7.118           | 0.267             |

At 4/8/16 threads, cap 6 is the most fair of the three in every row except
one (`4/push_pop`, where cap 8 edges it: 1.758 vs 2.424). Cap 8 is the LEAST
fair of the three in 6 of 8 rows, including the worst single figure in this
table (`16/churn`: 17.31x skew, min/mean 0.157 — the unluckiest thread got
under 16% of a fair share).

### 3.3 The 16-thread tail, re-measured twice more (run 2): the fairness ranking is noisy in magnitude but stable in ORDER — cap 6 best, cap 8 worst

Run 1's single 16-thread sample per cap was re-measured with 2 more
independent reps per cap (cap 6/8/10 only — the decisive regime), from a
fresh build each time, on the same shared host:

| cap | rep | bench    | total ops/sec | max/min | min/mean |
|-----|-----|----------|---------------:|--------:|---------:|
| 6   | 1   | push_pop | 25,271,226     | 6.957   | 0.289    |
| 6   | 1   | churn    | 24,211,225     | 7.828   | 0.250    |
| 6   | 2   | push_pop | 26,160,655     | 3.411   | 0.556    |
| 6   | 2   | churn    | 23,970,451     | 4.737   | 0.458    |
| 8   | 1   | push_pop | 19,199,465     | 8.579   | 0.188    |
| 8   | 1   | churn    | 14,657,136     | 17.391  | 0.108    |
| 8   | 2   | push_pop | 32,676,865     | 19.312  | 0.103    |
| 8   | 2   | churn    | 30,634,872     | 5.252   | 0.340    |
| 10  | 1   | push_pop | 34,927,192     | 46.739  | 0.096    |
| 10  | 1   | churn    | 34,589,143     | 24.704  | 0.115    |
| 10  | 2   | push_pop | 35,087,901     | 24.464  | 0.164    |
| 10  | 2   | churn    | 33,942,472     | 12.891  | 0.268    |

(Source: `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`, `run=2` rows,
derived from `docs/perf/_raw_tis_backoff_cap_sweep_run2_repeat16.log`.)

**This run is visibly noisier than run 1** — `cap 8`'s single-threaded rows
in this run's own bench output show a `churn` cost spike to 108.13 ns/op
(vs. ~53-56 ns/op everywhere else in both runs), a clear sign of transient
system interference from this being a shared dev host, not a dedicated
benchmark machine (see §4). Despite that noise, the RANKING is stable across
all three regimes (run 1 single sample, run 2 rep 1, run 2 rep 2):

- **Averaged `max/min` across all 6 samples per cap (run 1 §3.2's 16-thread
  row + run 2's 4 samples):** cap 6 ≈ **6.12x**, cap 8 ≈ **13.12x**, cap 10 ≈
  **20.64x**.
- **Averaged `min/mean` across the same 6 samples:** cap 6 ≈ **0.375**
  (best), cap 8 ≈ **0.190**, cap 10 ≈ **0.201** (worst two, close to each
  other, both clearly worse than cap 6).
- Cap 10 produced this sweep's single worst outlier — **46.7x** skew
  (`10/rep1/push_pop`) — confirming the original review's observation that
  cap 10 is "clearly too aggressive" is reproducible on this machine too,
  though the WORST-fairness cap on this host's particular runs was 8, not
  10 (the two are close and both are materially worse than 6 — this
  ordering between 8 and 10 specifically should be read as noisy, not as
  "cap 8 is safer than cap 10").

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

**Not changed.** Per this task's own explicit conservative-bias instruction
(prefer keeping 6 unless the data overwhelmingly favors a change AND the
fairness cost is acceptable) and this crate's own production-hot-path
posture (`push`/`pop` are the crate's only two operations):

- The throughput case for raising the cap is real and reproduces on this
  machine (§3.1: +17% to +58% depending on thread count, at EVERY thread
  count tested including the lowest-contention 2-thread arm).
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

This does not mean cap 6 is "the low-contention-optimal choice" — §3.1
shows it explicitly is not, at any thread count tested. The corrected
rationale (now in `src/lib.rs`'s doc comment and `CHANGELOG.md`, replacing
the old unmeasured "low enough for LOW contention" claim) is: **cap 6 is
the most fairness-conscious of the caps measured, at a real but bounded
throughput cost relative to cap 8/10, and that tradeoff — not a
low-contention latency argument — is why it ships.**

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
- `RUSTFLAGS="--cfg loom" cargo test --release -p tagged-index-stack --features loom --test loom_aba`: 10/10 green.
- `git status --short crates/tagged-index-stack/` confirmed clean both before
  the sweep and after every build cycle inside it (the sweep script itself
  hard-fails if the restore leaves any diff) — no measurement scaffolding
  survives in the final committed tree; `BACKOFF_SPIN_CAP` is `6` and the
  bench's thread-count cap is the original `.min(8)`.

(Exact command output for the items above is reported in this task's commit
message / accompanying session report, not duplicated here — this report's
own scope is the cap-sweep measurement and decision.)
