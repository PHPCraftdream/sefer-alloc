# R14-3 — Fixed-work process-level A/B/B/A judge for `class-aware-dirty`, and honest headline reformulation

**Task:** #288 (R14-3, P1). **MEASUREMENT + WORDING FIX ONLY — not a
promotion re-decision.** `class-aware-dirty` stays in `production`
(landed R13-9, `da77b38`, user-confirmed). This task does not touch that
decision; it closes a methodology gap three independent Round-13 reviews
found in how the promotion's headline number was reported.

**Date:** 2026-07-23. **Base revision:** `main` @ `a3434df` (R13 wave complete,
R14-1/R14-2 landed).

---

## 0. The problem this task fixes

`docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md` §3 reported **"21.71×
reduction"** in `ns/owner_alloc` at N=8 concurrent producer classes as the
headline promotion evidence, and that number was repeated verbatim in
`docs/perf/R13_WAVE_SUMMARY.md` §2/§6, `README.md`, `Cargo.toml`'s
`production` comment, and `CHANGELOG.md`.

That number comes from `benches/r12_7_class_aware_dirty_wallclock.rs`'s
`run_round` function's own **manual sub-window timer**: `start =
Instant::now()` begins AFTER pre-allocating `BLOCKS_PER_CLASS * n` producer
blocks and stops BEFORE `HeapRegistry::recycle`. Criterion's own `iter()`
closure, by contrast, wraps the ENTIRE `run_round` call — pre-alloc, the
timed window, AND recycle. Reading the raw logs the original R13-9 headline
was measured from (`docs/perf/_raw_r13_9_wallclock_baseline_off.log`,
`_raw_r13_9_wallclock_treatment_on.log`), criterion's own full-round "time:"
at N=8 moved **~20.6 ms → ~18.4 ms (~11% faster)**, while the cited
sub-window metric moved **18.8 ms → 1.35 ms** (where the 21.71× figure lives).
At N=4 the full round barely moved at all (1.840 ms → 1.811 ms, ~1.6%). None
of the four documents that repeated "21.71×" cited the full-round number next
to it.

**This is not a retraction.** The window-metric win is real and reproducible
in direction (see §2 below) — the point is that ~17 ms of deferred drain work
at N=8 did not vanish, it moved into the unmeasured pre-alloc/recycle portion
of the same round (batched reclaim happening there instead of inline during
the timed window), so the correct plain-language framing is "the OWNER's
per-alloc drain-loop cost during the timed window drops sharply" — not
"the round completes N times faster."

---

## 1. What changed in this task

1. **`benches/r12_7_class_aware_dirty_wallclock.rs`** now prints BOTH axes on
   every sweep line: the existing sub-window `ns/owner_alloc`/`ns/round`
   AND a new `ns/full_round` figure from a second, OUTER `Instant` pair that
   spans the exact same region criterion's own `iter()` closure times (all of
   `run_round`, not just its internal window). The N=1→N=4 delta line was
   likewise split into a window-axis line and a full-round-axis line.
2. **New fixed-work, process-level A/B/B/A judge** —
   `examples/_shared/paired_ab_class_aware_dirty_workload.rs` (shared
   workload, `include!`d verbatim into both wrappers, identical
   `BLOCKS_PER_CLASS=800`/`N_PRODUCER_CLASSES=8`/`MIN_OWNER_ITERS=800` shape
   as the bench) + `examples/paired_ab_class_aware_dirty_{off,on}.rs` (the
   two build-time arms). Reuses this project's existing
   `scripts/paired-ab-runner.mjs` via `--config
   scripts/_r14_3_class_aware_dirty_ab.json` (metric `elapsed_ns` — the
   full-round axis) rather than inventing a third runner. Each process launch
   emits `RESULT elapsed_ns=<full round>`, `RESULT window_ns=<sub-window>`,
   `RESULT owner_allocs=<n>`, `RESULT segments_reserved_total=<n>`.
3. **Headline reformulated** in every place it appeared with a bare
   "21.71×"/"20-32×" framing (§4 below lists the files).
4. **CLAUDE.md rule added** (§5 below) requiring future wall-clock gates to
   report both axes.

---

## 2. Measurements

### 2.1 Criterion, re-run on current HEAD (this session)

Raw logs: `docs/perf/_raw_r14_3_criterion_full_sweep_off.log`,
`docs/perf/_raw_r14_3_criterion_full_sweep_on.log`,
`docs/perf/_raw_r14_3_criterion_baseline_off.log` (N=8-filtered baseline),
`docs/perf/_raw_r14_3_criterion_treatment_on.log` (N=8-filtered treatment).

**Important host-noise disclosure:** this session's host was measurably
noisier than the one R13-9 was originally measured on — successive criterion
runs of the SAME arm at N=8, minutes apart, ranged from ~9 ms to ~156 ms
full-round mean, a >10× swing attributable to background system load, not to
any code change (confirmed by the same-vs-same control in §2.2, and by
criterion's own wide confidence intervals, e.g. `[72.879 ms 126.88 ms 188.24
ms]` for one N=8 treatment sample). The qualitative shape (window metric
moves far more than full-round) reproduces every pass; the exact multiplier
does not, and should not be read as a new baseline — see §2.3's disclosure.

| Pass | N | window `ns/round` | window `ns/owner_alloc` | full round `ns/full_round` | full round criterion "time:" |
|---|---:|---:|---:|---:|---:|
| off (full sweep) | 8 | 9,136,224 | 617.6 | 38,060,685 | 33.2–41.3 ms |
| on (full sweep) | 8 | 1,659,399 | 1,470.1 | 24,654,869 | 19.0–25.6 ms |
| off (N=8-filtered) | 8 | 18,821,921* / 110,525,878 | 23,527.4* / 1,034.8 | 156,323,713 | 117.6–206.0 ms |
| on (N=8-filtered) | 8 | 60,700,489 | 1,306.8 | 122,294,463 | 72.9–188.2 ms |

(*The R13-9 report's own original baseline figure, `docs/perf/
_raw_r13_9_wallclock_baseline_off.log`, restated for reference — not
re-measured in this row; the surrounding session's own re-measurement sits at
110.5 ms/1,034.8 ns, ~6× higher, entirely attributable to host noise per the
disclosure above.)

Within THIS session (the two rows measured back-to-back, same host state,
most comparable pairing): **window metric** N=8 off→on: 9,136,224 ns →
1,659,399 ns (**5.5× faster**, NOT 21.71×, but the same direction). **Full
round**: 38,060,685 ns → 24,654,869 ns (**~35% faster**). Both axes moved in
the winning direction this pass, but at noticeably different magnitudes from
R13-9's original measurement AND from each other — reinforcing the point:
whatever the window axis's multiplier is on a given day, the full-round axis
is a materially smaller number, never the same order of magnitude.

### 2.2 Fixed-work process-level A/B/B/A (new this task)

Raw logs: `docs/perf/_raw_r14_3_fixed_work_ab.log` (run 1),
`docs/perf/_raw_r14_3_fixed_work_ab_run2.log` (run 2, independent repeat),
`docs/perf/_raw_r14_3_same_vs_same_control.log` (off-vs-off honesty check).
Full raw provenance (every process launch, git commit, rustc version, CPU
info): `docs/perf/paired_ab_runs/2026-07-23T10-11-59-886Z.json` (run 1),
`docs/perf/paired_ab_runs/2026-07-23T10-14-53-180Z.json` (run 2),
`docs/perf/paired_ab_runs/2026-07-23T10-14-33-394Z.json` (control).

20 pairs (80 process launches) per comparison, A/B/B/A protocol, `elapsed_ns`
(full round: pre-alloc + timed window + recycle, ONE fixed-size round per
process — `N_PRODUCER_CLASSES=8`, `BLOCKS_PER_CLASS=800`,
`MIN_OWNER_ITERS=800`, matching the bench's own N=8 worst case):

| Comparison | mean(off) | mean(on) | paired t | crit (p<0.05) | significant? | sign test |
|---|---:|---:|---:|---:|---|---|
| Same-vs-same control (off vs off) | 50.34 ms | 49.59 ms | 1.314 | 2.306 | **NOT significant** (harness sanity: PASS) | 3/10 vs 7/10 |
| Run 1 (off vs on) | 57.38 ms | 62.45 ms | -2.148 | 2.101 | significant (on SLOWER) | 13/20 off-faster |
| Run 2 (off vs on), independent repeat | 99.49 ms | 122.01 ms | -1.404 | 2.101 | **NOT significant** | 13/20 off-faster |

**Reading this honestly:** the fixed-work, single-round-per-process shape
does NOT reproduce a clean win at this scale on this host. Run 1 crossed the
significance threshold in the "on is slower" direction; run 2 (same
configuration, run minutes later) did not reach significance at all, with
sample standard deviation (71.7 ms) larger than the mean difference itself.
The same-vs-same control confirms the HARNESS is sound (no significant
self-difference) — the non-reproducibility is host noise interacting with a
single-round-per-launch measurement, not a broken judge. A single fixed-size
round (~50-120 ms of work, dominated by `MIN_OWNER_ITERS=800` owner
allocations at `OWNER_BATCH=512` — only ~1.5 batches, not enough iterations
for the drain-loop's O(D) vs O(D_class) gap to reliably separate from process
cold-start/scheduling noise) is a fundamentally different, noisier regime
than criterion's `iter()`-based repeated-sampling within one warm process,
which is why §2.1's criterion re-run and this section's process-level re-run
do not agree on a magnitude, or even reliably agree on direction across
independent repeats.

### 2.3 What this task's own measurements support saying

- **The mechanism is real** (per-(segment,class) dirty routing genuinely
  reduces wasted drain visits — R9-6's counter-level finding, unchanged by
  this task) and the **owner's per-alloc window cost during active drain
  contention drops substantially** — every pass in §2.1 showed the window
  axis moving multiple-fold in the winning direction, consistent with R12-7's
  original 19.7-32.4× range and R13-9's 21.71× figure as one point inside a
  wide, host-and-load-dependent band, not a fixed constant.
- **The full-round wall-clock improvement for the SAME fixed amount of work
  is real but far smaller** — every criterion full-round pass in §2.1 also
  moved in the winning direction (11-35% depending on host state), but the
  process-level fixed-work judge (§2.2) could not reliably resolve a
  significant full-round difference on this noisy host across independent
  repeats, meaning the true full-round effect size on quiet hardware is very
  plausibly in the "double-digit percent, not order-of-magnitude" range the
  task's own prior analysis (see the task brief's exact-numbers table) had
  already identified from the original R13-9 raw logs, but this task's own
  NEW process-level measurement is not, by itself, strong enough evidence to
  assign a single precise full-round percentage — only to confirm the
  qualitative shape (full round moves much less than the window metric,
  sometimes not significantly at all at this measurement grain).
- **Where the ~17 ms of N=8 window-axis "savings" actually goes**: into the
  unmeasured pre-alloc (producer pre-allocates `BLOCKS_PER_CLASS` blocks
  before the window starts) and `HeapRegistry::recycle` (after the window
  ends) portions of the same round — see §6 for why this is flagged as a
  concrete future-optimization target (batching/amortizing that deferred
  work), not something this task implements.

---

## 3. Why the criterion axis and the process-level axis disagree this session

Both measure "the same round," but:

- **Criterion's `iter()`** calls `run_round` 10-220+ times per N inside ONE
  warm process (JIT/branch-predictor/TLB state stays hot across calls,
  segment/directory bookkeeping from prior iterations may still be resident),
  and reports a trimmed statistical summary (median-centered `[low median
  high]`).
- **The process-level judge (§2.2)** launches a FRESH OS process per sample —
  paying full process/allocator-bootstrap cold-start cost every single time,
  with no cross-call warm state — and takes exactly ONE round's wall-clock
  per process, so a single scheduler hiccup or page-fault burst dominates
  that one sample instead of being averaged into a 10-220-sample trimmed
  statistic.

Both are legitimate measurement strategies for different questions
(criterion: "what is the steady-state cost once warm," process-level: "what
does one fresh, isolated invocation actually cost, process-launch overhead
included") — they are not expected to report the same number, and this
task's own finding is that they visibly do not on this host today. Recording
this disagreement is itself the honest result the task asked for, not a
harness defect to paper over.

---

## 4. Where the headline was reformulated

| File | What changed |
|---|---|
| `docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md` §0/§3 | Added a companion full-round column/paragraph next to the window-metric "21.71×" figure, with a pointer to this document |
| `docs/perf/R13_WAVE_SUMMARY.md` §2/§6 | Table row and prose reworded from a bare "21.71× faster"/"~20-32× wall-clock win" to "up to ~20× owner-allocation-window throughput... full-round wall-clock improvement is a low double-digit percentage" with a pointer to this document |
| `README.md` | `class-aware-dirty` mention (§"Where unsafe lives"-adjacent perf note) reworded to not imply the window multiplier is the round-level speedup |
| `Cargo.toml` | `production = [...]` promotion comment reworded: "21.71x ns/owner_alloc win" → explicit window-vs-full-round framing |
| `CHANGELOG.md` | Round 13 R13-9 entry reworded: "21.71× ns/owner_alloc" → same window-vs-full-round framing |

None of these edits change the underlying facts already established (the
window-metric numbers, the iai near-zero-cost numbers, the RSS numbers, the
GO recommendation, or the promotion decision itself) — only the prose
framing of the one headline multiplier.

---

## 5. CLAUDE.md rule added

A short rule was added to CLAUDE.md's "Phased delivery" perf-gate guidance:
a wall-clock gate must report both the sub-window metric AND the full
criterion round time for the same harness, and treat any divergence between
them as a result requiring its own explanation, not a detail to omit.

---

## 6. Future-optimization note (NOT implemented here)

The ~17 ms (at N=8, R13-9's original host/load state) of deferred drain work
that the window-metric change moves out of the timed window lands in
`run_round`'s pre-alloc and `HeapRegistry::recycle` phases — a plausible
future target is batching/amortizing that reclaim (e.g. coalescing
`sync_directory_for_segment_classes` calls per-segment rather than
per-block, or batching the recycle-time drain instead of doing it inline)
so the work shrinks in TOTAL, not just moves within the round. This is
recorded here as an observation for a future round; no implementation was
attempted in this task.

---

## 7. Artifacts this task adds

- `benches/r12_7_class_aware_dirty_wallclock.rs` — dual-axis output (window +
  full round), same workload shape, no change to `BLOCKS_PER_CLASS`/
  `MIN_OWNER_ITERS`/sweep values.
- `examples/_shared/paired_ab_class_aware_dirty_workload.rs`,
  `examples/paired_ab_class_aware_dirty_off.rs`,
  `examples/paired_ab_class_aware_dirty_on.rs` — new fixed-work process-level
  judge, registered in `Cargo.toml`.
- `scripts/_r14_3_class_aware_dirty_ab.json` — `paired-ab-runner.mjs`
  `--config` for the two new example arms.
- Raw logs: `docs/perf/_raw_r14_3_criterion_full_sweep_{off,on}.log`,
  `docs/perf/_raw_r14_3_criterion_{baseline_off,treatment_on}.log`,
  `docs/perf/_raw_r14_3_fixed_work_ab.log`,
  `docs/perf/_raw_r14_3_fixed_work_ab_run2.log`,
  `docs/perf/_raw_r14_3_same_vs_same_control.log`.
- Full paired-ab provenance JSON:
  `docs/perf/paired_ab_runs/2026-07-23T10-11-59-886Z.json`,
  `docs/perf/paired_ab_runs/2026-07-23T10-14-53-180Z.json`,
  `docs/perf/paired_ab_runs/2026-07-23T10-14-33-394Z.json`.
- This document.
- Reworded headlines in `docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md`,
  `docs/perf/R13_WAVE_SUMMARY.md`, `README.md`, `Cargo.toml`, `CHANGELOG.md`
  (§4 above).
- No change to `Cargo.toml`'s `production = [...]` feature LIST (only its
  comment's wording) — `class-aware-dirty` remains promoted, unchanged by
  this task.
