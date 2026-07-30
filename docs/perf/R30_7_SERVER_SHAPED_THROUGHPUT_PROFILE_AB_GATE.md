# R30-7, Deliverable 4 — does the throughput profile's win hold in an application-shaped scenario?

**Task:** R30-7 (task #456), Round 30, Deliverable 4. Answers the task
brief's explicit question: the `(8, 32 MiB)` small-pool throughput recipe's
~22% win (README, R27-4,
[`R27_4_REAL_DEFAULT_AB_GATE.md`](R27_4_REAL_DEFAULT_AB_GATE.md)) rests on
ONE single-threaded, single-shot 1024 B teardown micro-benchmark. This gate
measures the SAME two configs (`SeferAlloc::new()` vs
`SeferAlloc::with_profile(Profile::Throughput)`) under a more
application-shaped workload — several concurrent "request handler" threads,
each allocating+freeing a MIX of small object sizes repeatedly in a
continuous cycle, not a single burst-then-teardown.

**Verdict: the win does NOT reproduce as a statistically distinguishable
effect in this workload shape — it is indistinguishable from noise, in
EITHER direction.** Paired A/B/B/A (20 pairs, 40 process launches):
`t = -0.119` (mean Δ = default − throughput = -7.44 ms, i.e. `default`
nominally faster on average), sign split 12/8 favoring `default`. Both are
far under `crit(p<0.05) = 2.101`. A same-vs-same honesty control
(`default` vs `default`) shows the SAME noise-band shape (`t = -1.039`,
sign 11/9) — confirming the harness is not manufacturing a false null out
of a real signal; the comparison genuinely has no detectable effect at this
workload's scale on this host. **The mechanism the throughput profile
targets WAS genuinely activated** (`decommit_calls_total = 40` — non-zero
— in every one of the 40 `default`-arm launches, the same activation
oracle R27-3/R27-4 require before trusting a pool-cap comparison), so this
is not a vacuous "the workload never touched the mechanism" null — the
`default` arm's small pool genuinely saturates and churns segments in this
workload too, but the resulting latency cost is swamped by other sources of
variance (thread spawn/join overhead, cross-thread scheduling, the
`large_cache_hits`/segment-reservation contention 8 threads introduce) at a
scale where the single-thread micro-benchmark's cleaner signal does not
carry over.

**This does not retract the README's `(8, 32 MiB)` recipe or the
`Profile::Throughput` preset** — R27-4's finding (single-threaded,
single-shot, one fixed size, `t=8.114`) stands as measured, for the shape
it measured. This gate adds the honest complementary data point the task
brief asked for: the win is workload-shape-dependent, and a caller
choosing `Profile::Throughput` for a genuinely concurrent, mixed-size,
continuous-churn workload should not assume the ~22% figure transfers
unchanged — measure their own workload if the win matters to their
decision.

> **Dated correction (2026-07-30, Round 30 review response — see
> `docs/reviews/2026-07-30-r30-full-review.md` §4 P1-2/P1-3, and §0.1/§0.2
> below for the full numbers and derivations).** Three claims in the
> summary above (unchanged) need a corrected reading:
> 1. "**The mechanism the throughput profile targets WAS genuinely
>    activated**" cites only the `default` arm's `decommit_calls_total`.
>    The `throughput` arm reports the SAME value (40, bit-identical) in
>    every one of its own 40 launches — the mechanism activated
>    IDENTICALLY in both arms, which is a materially different (and more
>    important) finding than "the workload touches the mechanism at all."
>    See §0.1.
> 2. "shows the **SAME noise-band shape**" (comparing the control to the
>    real comparison) is not supported by the two runs' own dispersion:
>    the control's `sd` is ~6.4× tighter and its mean per-launch runtime is
>    ~4× faster than the real comparison's. See §0.2.
> 3. "**the win is workload-shape-dependent**" and README's "Treat the
>    `~22%` figure as workload-shape-specific" both read as a confirmed
>    absence of effect. This comparison's own minimum detectable effect is
>    ≈18.8% of the mean (≈131 ms), so the correct reading is an
>    UNDERPOWERED null — it cannot rule out a real win or loss up to
>    roughly 15-19% at this workload's scale — not a confirmed "no
>    material effect here." See §0.2.
>
> None of this changes the report's own bottom line that R27-4's ~22%
> figure did not reproduce as a STATISTICALLY DISTINGUISHABLE effect in
> this workload at this sample size, and none of it retracts R27-4's
> original finding or the `Profile::Throughput` preset — both statements
> immediately above this note stand. What changes is the STRENGTH of
> claim the null result supports.

**Date:** 2026-07-30. **Base revision measured:** `main` @
`1272a522a45acdbb58dd6b0dede946b1ced12fa6` (the paired-ab-runner's own
`git_commit` field, captured automatically at measurement time) + this
task's uncommitted working tree (the profile/example/doc additions this
same task landed) — per CLAUDE.md's R29-6 immutable-source-identity rule,
citing the exact base SHA the provenance JSON recorded is the honest
record available; the working tree is landed in the commit this report is
part of, making the tree state resolvable going forward from that commit.
**Commit that lands this report:** `b5efe8ce6099d33987f7811edc4f2411686d9bfc`
(filled in by a same-day follow-up commit, per the same chicken-and-egg
pattern `1272a52`/R30-6 established — a commit cannot cite its own SHA
inside its own tree; per CLAUDE.md's R29-6 rule this landing-commit SHA,
not the base-SHA-plus-uncommitted-tree citation above, is this report's
actual immutable source identity: `git show
b5efe8ce6099d33987f7811edc4f2411686d9bfc:docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md`
recovers this exact report, and the same SHA recovers the exact
`examples/r30_7_*`/`examples/_shared/r30_7_*` source this report measured).
**Platform:** native Windows 10 Pro
x86-64, 11th Gen Intel Core i7-11800H @ 2.30GHz (8 cores / 16 logical) —
the same shared host as R27-3/R27-4/R29-13/R30-6 (this host is shared with
other concurrent agent work during this session; the paired A/B/B/A
protocol plus the same-vs-same control are exactly the machinery this
project uses to separate a real effect from that noise, per R27-4's
established precedent). **Feature set:** `production alloc-stats` (real
`#[global_allocator]`, no `bench-internals` needed — both binaries use only
`SeferAlloc::stats()`, the public always-`production`-available diagnostic
surface, matching R30-6's latency-axis binaries).

---

## 0. Headline numbers

| comparison | n pairs | mean Δ (default − throughput) | t | crit(p<0.05) | significant | sign split (default/throughput) |
|---|---:|---:|---:|---:|---|---|
| default vs throughput | 20 | -7.44 ms | -0.119 | 2.101 | **NO** | 12 / 8 |
| default vs default (control) | 20 | -10.09 ms | -1.039 | 2.101 | **NO** | 11 / 9 |

Both `t` values sit in the same noise band; the control's sign split
(11/9) is not meaningfully tighter than the real comparison's (12/8),
confirming the harness resolves a genuine null rather than manufacturing
one. See `docs/perf/_raw_r30_7_server_shaped_ab.log` (real comparison) and
`docs/perf/_raw_r30_7_server_shaped_control.log` (control) for the full 40
raw process launches each, and
`docs/perf/paired_ab_runs/2026-07-30T12-56-39-878Z.json` /
`2026-07-30T12-58-24-246Z.json` for the complete provenance (every sample,
git commit, rustc version, CPU info, feature set).

**Activation oracle** (the R26-4-style "did the mechanism this comparison
targets actually fire" check, applied here per the task's own
`decommit_calls_total` guidance): every one of the 40 `default`-arm
launches reports `decommit_calls_total = 40` (non-zero, and IDENTICAL
across every launch — the workload's fixed shape drives the same amount of
decommit churn every run), proving the `default` config's small pool
genuinely saturates and cycles segments under this workload — the same
mechanism R27-4 measured a ~22% win from eliminating. This is not a
workload that never touches the pool-cap boundary; the mechanism fires,
the effect on wall-clock is simply not distinguishable from noise at this
concurrency/scale.

> **Dated correction (2026-07-30, Round 30 review response — see
> `docs/reviews/2026-07-30-r30-full-review.md` §4 P1-2, and this report's
> new §0.1 immediately below for the full per-arm numbers).** The
> "Activation oracle" paragraph immediately above (unchanged) reads only
> the `default` arm's `decommit_calls_total` and concludes "the mechanism
> fires" as if that alone validates the comparison. It does not: the
> `throughput` arm's `decommit_calls_total` is **also 40, bit-identical to
> `default`'s, in every one of its own 40 launches** — the mechanism
> `Profile::Throughput` exists to eliminate (R27-4's original single-thread
> finding is 9→0 decommit calls/run) was measured here at 40→40, i.e. a
> ZERO mechanism delta between arms at this workload's scale. The
> corrected reading is: this oracle rules out ONE vacuity mode (the
> workload never touches the pool-cap boundary at all — it does, in both
> arms) but does NOT rule out the other, more consequential one (the
> treatment never actually changed the mechanism in this workload shape) —
> see §0.1 and §2's new hypothesis 0.

---

## 0.1 Per-arm mechanism-activation evidence (added 2026-07-30, review response)

**This section closes P1-2 of `docs/reviews/2026-07-30-r30-full-review.md`
§4: the original §0 "Activation oracle" paragraph above printed only the
`default` arm's `decommit_calls_total`, which is not sufficient evidence
that the compared mechanism actually differs between arms.** Re-parsing
`docs/perf/_raw_r30_7_server_shaped_ab.log` (80 `RESULT` records: 40
`default` + 40 `throughput`) for BOTH arms:

| arm | n launches | `decommit_calls_total` distinct values | `large_cache_hits` distinct values | `segments_reserved_total` range |
|---|---:|---|---|---|
| `default` | 40 | `[40]` — bit-identical every launch | `[45]` — bit-identical every launch | 68–72 |
| `throughput` | 40 | `[40]` — bit-identical every launch, IDENTICAL to `default`'s | `[45]` — bit-identical every launch, IDENTICAL to `default`'s | 62–72 |

**The mechanism delta between arms is ZERO at this workload.**
`Profile::Throughput`'s whole design point is a small-pool cap large
enough to absorb the workload's peak segment demand so decommit/reserve
churn drops to (ideally) zero — R27-4's original single-threaded
micro-benchmark measured exactly that, 9 decommit calls/run under
`default` vs 0 under `throughput`. Here, both arms report 40 decommit
calls per launch, every launch, with no exceptions. Whatever this
workload's `~-7.44 ms` nominal (non-significant) mean difference reflects,
it is not a reduction in decommit/reserve churn — the treatment did not
change the mechanism this comparison exists to measure, in this workload
shape.

This is stated here as **hypothesis 0**, ahead of §2's three noise-based
hypotheses (thread-lifecycle overhead, cross-thread registry contention,
mixed-size pressure spreading): **the simplest explanation for the null
result is that the profile change did not change the mechanism being
measured in this workload shape at all — not that it changed the
mechanism but the wall-clock effect was swamped by noise.** A
`pool_segments`/`pool_byte_cap` pair large enough to matter for a
single-threaded 256-object/batch churn (R27-4's shape) may simply never
become the binding constraint once 8 threads' concurrent working sets are
each large enough (§1's ~24.1 MiB/thread/round, deliberately calibrated to
exceed `default`'s pool) that BOTH configs churn segments continuously —
i.e. `throughput`'s larger cap may still be undersized relative to THIS
workload's peak demand, not merely under-exercised by it. This gate does
not distinguish "the cap is still too small at this scale" from "some
other structural reason the two configs converge under 8-way concurrency"
— either would produce the observed 40=40 result — but it does rule out
the reading in the original §0 paragraph above, that a non-zero
`decommit_calls_total` on the `default` arm alone is sufficient evidence
the comparison is non-vacuous.

For completeness, the config almost certainly *did* take effect as
compiled — `segments_reserved_total` reaches as low as 62 in the
`throughput` arm and never below 68 in the `default` arm (table above),
which is consistent with §3.4 point 2's original reasoning that a
mis-wired config would show a starker between-arm difference — but this is
a weak, incidental signal about config resolution, not the mechanism
oracle §0's original paragraph claimed it was.

---

## 0.2 Statistical power and the control's noise regime (added 2026-07-30, review response)

**This section closes P1-3 of `docs/reviews/2026-07-30-r30-full-review.md`
§4: §0's original table and control never stated the comparison's minimum
detectable effect, and the control's own dispersion/runtime do not match
the real comparison's, weakening its value as a noise-floor
characterization.** Both points independently verified against this
report's own committed
`docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv` and the
two raw logs:

**Minimum detectable effect.** With `se = 62.39 ms` (the real comparison's
own standard error, from the summary CSV) and `crit(p<0.05) = 2.101`
(§0's own stated critical value, 18 degrees of freedom, two-tailed), the
smallest difference this comparison could distinguish from zero at p<0.05
is `2.101 × 62.39 ms ≈ 131.1 ms`. The real comparison's own combined
per-launch mean elapsed time (both arms, all 80 launches) is ≈697 ms
(≈695 ms as stated in this report's headline, ≈694 ms for `default` alone
and ≈701 ms for `throughput` alone) — so the minimum detectable effect is
**≈131 ms / ≈697 ms ≈ 18.8% of the mean**. The 95% confidence interval on
the mean Δ (`-7.44 ms ± 2.101 × 62.39 ms`) is **`[-138.5 ms, +123.6 ms]`**.
**This study can barely exclude R27-4's ~22% figure and cannot exclude a
real win or loss up to roughly 15-19% at this sample size** — the null
result is a real null (the point estimate is close to zero and the sign
flips between the real comparison and its own control), but it is an
UNDERPOWERED null, not evidence that no effect up to R27-4's own magnitude
exists at this workload's scale.

**The same-vs-same control's regime does not match the real comparison's.**
From the same summary CSV:

| run | n pairs | mean Δ | sd | se | t | mean per-launch elapsed |
|---|---:|---:|---:|---:|---:|---:|
| `default` vs `throughput` (real) | 20 | -7.44 ms | 279.02 ms | 62.39 ms | -0.119 | ≈697 ms |
| `default` vs `default` (control) | 20 | -10.09 ms | 43.41 ms | 9.71 ms | -1.039 | ≈171 ms |

The control's standard deviation is **~6.4× tighter** (43.41 ms vs
279.02 ms) and its mean per-launch runtime is **~4× faster** (≈171 ms vs
≈697 ms) than the real comparison it is meant to validate — both computed
directly from the raw per-launch `elapsed_ns` values in
`docs/perf/_raw_r30_7_server_shaped_ab.log` (real: min 136 ms, max
1,910 ms) vs `docs/perf/_raw_r30_7_server_shaped_control.log` (control:
min 115 ms, max 420 ms). The report's own text discloses "this host is
shared with other concurrent agent work," and these two runs were not
taken under comparable host load. **The control demonstrates the harness's
own self-consistency (it does not manufacture a false positive out of
nothing) more than it characterizes the real comparison's noise floor** —
the only statistic genuinely comparable between the two is the qualitative
sign-split pattern (both non-significant, similar rough magnitude of `t`),
not the variance or absolute timing.

**Correction to this report's own framing.** This report's own top-summary
"the win is workload-shape-dependent" and README's "Treat the `~22%`
figure as workload-shape-specific" should be read together with the above
(also cross-referenced from the dated correction note directly under this
report's top summary): this gate
found a genuine, real null at the ~131 ms/~18.8% resolution it could
measure, but it did **not** establish that no material win exists at this
workload shape — only that if one exists, it is smaller than roughly 15-19%
of the mean, or this particular 20-pair sample happened to land near zero
by chance within that resolution. A future task wanting a tighter bound
would need either more pairs, a quieter host, or both — not attempted
here, since a re-run is explicitly out of scope for this correction pass
(this section restates and verifies numbers already present in the
committed CSV/logs; it does not gather new measurement).

---

## 1. Workload — why this is a materially different shape from R27-4's

R27-4's micro-benchmark (`examples/r27_4_real_default_ab_cap4/_cap8.rs`):
ONE thread, ONE fixed size (1024 B), `churn_prefill`/`churn_step`/
`churn_teardown` per labelled batch (allocate 256 objects, churn 1024 ops,
free all 256 — a full teardown every batch), 8 timed batches of 120 cycles
each.

This gate's workload
(`examples/_shared/r30_7_server_shaped_workload.rs`, `include!`d into both
`examples/r30_7_throughput_profile_server_ab_default.rs` and
`_throughput.rs`):

- **8 concurrent threads** (`THREADS = 8`), each simulating an independent
  request handler — genuinely overlapping allocation traffic, not a single
  serial stream.
- **A mix of 4 realistic sizes per object** (`SIZE_CLASSES = [64, 256,
  1024, 4096]` bytes — header/struct/buffer/page-sized), round-robin, not
  one fixed size.
- **A large enough per-round working set to overflow the default pool.**
  `OBJS_PER_ROUND = 18,504` objects/thread/round spread across the 4 size
  classes — peak concurrently-live bytes per thread per round ≈
  `18504 / 4 × (64+256+1024+4096)` ≈ **24.1 MiB**, deliberately calibrated
  to exceed the small pool's default 16 MiB (cap 4) byte ceiling, mirroring
  R27-4's own calibration principle (its batch-120 shape was chosen
  specifically to overflow a 4-segment pool — a working set that fits
  inside the default pool would never activate the mechanism under test).
- **A mid-round churn step** (`CHURN_FRACTION = 1/2`): half the round's
  objects are freed and replaced with a fresh random size before the round
  ends, reproducing a connection-pool-style mix of short- and
  slightly-longer-lived objects rather than every object sharing identical
  lifetime.
- **`ROUNDS = 6` continuous rounds per thread** — a repeated cycle for the
  whole timed region (not one burst-then-teardown), each round fully
  freeing its own working set before the next round starts (bounding peak
  memory per thread while still repeating the alloc/touch/churn/free cycle).
- **End-to-end wall-clock for the WHOLE concurrent run**, timed from just
  before `thread::spawn` to just after every handle is `join`ed — the
  metric an application actually cares about (total time to process a
  fixed amount of concurrent work), not a per-thread average.

Both binaries run the IDENTICAL workload body (`include!`d verbatim,
mirroring this project's established `paired_ab_*` pattern for
guaranteeing the only difference between arms is the installed
`#[global_allocator]` config) — the only difference is which `SeferAlloc`
static is installed:

```text
// default:
static GLOBAL: SeferAlloc = SeferAlloc::new();

// throughput:
static GLOBAL: SeferAlloc = SeferAlloc::with_profile(Profile::Throughput);
```

---

## 2. Why the win vanishes at this scale — a plausible mechanism, not proven root cause

This report does NOT claim to have isolated the exact reason the effect
disappears (that would need a separate, targeted Stage-1 measurement, out
of this task's scope) — but the workload's own structural differences from
R27-4's shape suggest at least three plausible contributors, stated as
hypotheses:

1. **Thread spawn/join overhead and cross-thread scheduling variance are
   themselves large relative to the pool-cap effect at this concurrency.**
   R27-4's single-thread shape has zero thread-lifecycle cost in its timed
   region; this gate's 8-way `thread::spawn`+`join` per launch introduces
   OS scheduling variance (visible in the raw log's wide `elapsed_ns`
   range, e.g. samples from ~136 ms to ~1.9 s in the SAME comparison) that
   is large enough to swamp a ~20 ms mean effect.
2. **8 threads' allocation traffic contends on shared registry/large-cache
   infrastructure** in ways a single thread never does, potentially
   diluting or masking a per-thread pool-cap effect that was cleanly
   visible in isolation.
3. **The mixed-size workload spreads pressure across more size classes**
   than R27-4's single fixed size, potentially changing which segments
   saturate and when relative to the single-size shape's more uniform
   churn pattern.

None of these is confirmed here — they are the honest candidate
explanations for why an activation-proven mechanism (§0's
`decommit_calls_total` check) produces no measurable wall-clock signal at
this scale, offered so a future task that wants to pursue this further has
concrete starting hypotheses rather than none.

> **Dated addition (2026-07-30, Round 30 review response — see
> `docs/reviews/2026-07-30-r30-full-review.md` §4 P1-2).** The three
> hypotheses above all implicitly assume the premise "an activation-proven
> mechanism produces no measurable wall-clock signal" — i.e. that the
> mechanism DID activate and differ between arms, and something else
> (noise, contention, scheduling) swamped its effect. §0.1 (added in the
> same review response) shows that premise does not hold here:
> `decommit_calls_total` is bit-identical (40) between the `default` and
> `throughput` arms in every one of their 40 launches each — the mechanism
> did not merely activate, it activated IDENTICALLY in both arms. This
> should be read as **hypothesis 0, logically prior to hypotheses 1-3
> above**: the simplest explanation for the null is that the profile
> change did not change the mechanism being measured in this workload
> shape at all (§0.1's `throughput`-cap-still-undersized-at-scale
> candidate explanation), not that it changed the mechanism but the
> wall-clock effect was swamped by noise. Hypotheses 1-3 remain honest
> candidate explanations for residual noise/variance in general, but they
> are not needed to explain why THIS mechanism's effect is absent — a
> 40=40 decommit-call count is already a sufficient, and more direct,
> explanation than "swamped by noise" for a near-zero mean latency delta.

---

## 3. Methodology

### 3.1 Two real `#[global_allocator]` binaries, one shared workload body

Mirrors R27-4's and R30-6's established pattern (separate compile-time-
configured binaries, since `SeferAlloc::with_config`/`with_profile` bake
their config into a `static` initialiser — no runtime selection is
possible for the real global-allocator entry point):

- `examples/r30_7_throughput_profile_server_ab_default.rs` — installs
  `SeferAlloc::new()`.
- `examples/r30_7_throughput_profile_server_ab_throughput.rs` — installs
  `SeferAlloc::with_profile(Profile::Throughput)`.
- `examples/_shared/r30_7_server_shaped_workload.rs` — the workload body,
  `include!`d verbatim into both (byte-identical between arms, the same
  discipline `examples/_shared/paired_ab_workload.rs`'s module doc
  establishes and this project's other `paired_ab_*`-family probes follow).

### 3.2 Paired A/B/B/A protocol via `scripts/paired-ab-runner.mjs`

```text
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,throughput
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,default   # same-vs-same control
```

20 pairs (this project's real-claim threshold — not the `--quick` 4-pair
smoke count), A/B/B/A block alternation (averages out monotonic host
drift, R5-R2's established rationale, reused verbatim by R27-4/R30-6), a
hand-rolled paired t-test + sign test computed by the runner itself. The
`docs/perf/r30_7_server_shaped_run.json` config file
(`scripts/paired-ab-runner.mjs`'s `--config` mechanism) names both arms'
prebuilt binary paths and a sanity gate
(`segments_reserved_total > 0` for both arms — both genuinely install a
real allocator and reserve real segments).

### 3.3 Why a within-process-launch A/B via the existing runner, not a new subprocess-per-arm sweep harness

The task brief explicitly permits either "the established paired A/B
methodology... if that fits, or a simpler within-process A/B if the
workload shape makes subprocess isolation unnecessary — your call, but
justify it." This gate uses the EXISTING `paired-ab-runner.mjs`
process-level protocol (each launch is its own fresh OS process, same as
R27-4/R30-6) rather than a same-process sweep, for the same reason
R27-4/R30-6 did: a real `#[global_allocator]` config is baked at compile
time into a `static`, so there is no way to select a config at runtime
within one process — a subprocess-per-arm (here, subprocess-per-LAUNCH,
20 pairs = 40 launches per comparison) is the only way to compare two
different compiled configs at all, not an extra caution layered on top of
an otherwise-avoidable choice. This is NOT the `HeapRegistry`-slot-reuse
hazard R26-4's rule targets (that hazard is specific to same-process
config sweeps via `claim_with_config`); this gate never uses
`claim_with_config` at all — both binaries are ordinary real
`#[global_allocator]` processes, structurally immune to that hazard by the
same reasoning R27-4 §1/R30-6 §1.1 already established for their own
real-allocator latency axes.

### 3.4 Config-identity fields (R26-4 contract, applied to this real-allocator entry point)

1. **REQUESTED** — the `Profile::Throughput` / `SeferAlloc::new()` constant
   compiled into each binary (source-visible, not runtime-configurable).
2. **RESOLVED** — proven structurally: a compile-time `static` has no
   runtime resolution step (identical reasoning to R30-6 §1.6's latency
   axis); each binary's own `decommit_calls_total`/`segments_reserved_total`
   readout (non-zero, consistent per-arm) confirms the compiled config took
   effect (a mis-wired config would show materially different segment
   counts between arms, which the raw log does NOT show — both arms
   reserve comparable segment counts, 62–72, under the same workload).
3. **Config-conflict counter** — does not apply at this entry point (no
   registry-slot reuse is possible; each process is one fixed `static`,
   identical to R27-4's/R30-6's own reasoning for this same entry-point
   shape).
4. **Process identity** — subprocess-isolated (`paired-ab-runner.mjs`
   spawns a fresh process per launch, 40 launches per comparison).

---

## 4. What this gate does NOT claim

- **Does not retract R27-4's finding.** R27-4's ~22% win, measured on its
  own single-threaded single-shot shape, is unaffected — this gate
  measures a DIFFERENT workload shape and reports its own (null) result
  for that shape, not a re-measurement of R27-4's original claim.
- **Not an exhaustive characterization of "does the throughput profile
  help under concurrency."** This gate picked ONE representative
  application-shaped scenario (8 threads, 4-size mix, continuous
  multi-round churn) per the task brief's "one well-chosen additional
  workload shape is sufficient" instruction — a different thread count,
  size mix, or churn pattern could plausibly show a different result
  (including a real win or a real loss); this gate does not claim to have
  swept that space.
- **No root-cause isolation for WHY the effect vanishes** — §2's three
  hypotheses are plausible candidates, not measured/proven causes. A
  future task wanting to pursue this would need a targeted Stage-1
  measurement (per-thread decommit/segment counters, thread-timing
  breakdown) this gate does not attempt.
- **Does not change the `Profile::Throughput` preset or the README
  recipe.** No `src/` default changed by this gate; `Profile::Throughput`'s
  values (`(8, 32 MiB)` + 64 MiB headroom) remain as landed in this same
  task's Deliverable 1.

---

## 5. Files changed

| file | change |
|---|---|
| `examples/r30_7_throughput_profile_server_ab_default.rs` | new — real `#[global_allocator]` arm, `SeferAlloc::new()` |
| `examples/r30_7_throughput_profile_server_ab_throughput.rs` | new — real `#[global_allocator]` arm, `SeferAlloc::with_profile(Profile::Throughput)` |
| `examples/_shared/r30_7_server_shaped_workload.rs` | new — shared multi-thread, mixed-size, continuous-cycle workload body, `include!`d into both binaries above |
| `Cargo.toml` | added two `[[example]]` entries (`required-features = ["alloc-global", "alloc-xthread", "alloc-decommit"]`, matching the r27_4/r30_6_latency sibling pattern) |
| `docs/perf/r30_7_server_shaped_run.json` | new — the `--config` file for `paired-ab-runner.mjs` |
| `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_GATE.md` | this report (new) |
| `docs/perf/R30_7_SERVER_SHAPED_THROUGHPUT_PROFILE_AB_summary.csv` | machine-readable summary (new) |
| `docs/perf/_raw_r30_7_server_shaped_ab.log` | raw `paired-ab-runner.mjs` stdout, real comparison, 40 launches (`.gitignore`d by default — `git add -f` at commit time) |
| `docs/perf/_raw_r30_7_server_shaped_control.log` | raw stdout, same-vs-same control, 40 launches (`.gitignore`d — `git add -f`) |
| `docs/perf/paired_ab_runs/2026-07-30T12-56-39-878Z.json` / `2026-07-30T12-58-24-246Z.json` | runner provenance JSONs (`.gitignore`d — `git add -f`) |

**No production source default changed.**

---

## 6. Reproduce

```text
cargo build --release --example r30_7_throughput_profile_server_ab_default --example r30_7_throughput_profile_server_ab_throughput --features "production alloc-stats"
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,throughput
node scripts/paired-ab-runner.mjs --config docs/perf/r30_7_server_shaped_run.json --arms default,default   # same-vs-same control
```

40 process launches per comparison (20 pairs × 2 arms), each running an
8-thread × 6-round × ~18,500-object-per-round workload — measured wall
clock for the full 40-launch real comparison plus the 40-launch control:
low single digit minutes total on this 16-core host (well inside
CLAUDE.md's "minutes, not hours" budget for this class of gate).
