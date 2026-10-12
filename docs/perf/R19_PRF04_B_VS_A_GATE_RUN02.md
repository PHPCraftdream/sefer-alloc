# R19 PRF04 B-vs-A thread-wave gate (run02) — bench / measurement-only

**Verdict: NO-GO by the frozen rules — KILL-1 (R-live first-call p50) fired: B's median is
19.08 % (1429800 / 7494400) ns above A's while the A/A band (4.33 %) is inside its 5 % limit.
The benefit threshold is met by a wide margin and KILL-2/KILL-3 did not fire. The same data show that the R-live axis
cannot resolve a 5 % difference at this sample size (section 4), which is recorded for the arbiter; the frozen verdict is
not reinterpreted here. Nothing changed in the allocator on `main`; the V4 candidate stays on its branch.**

Authority and protocol: unchanged from `R19_PRF04_B_VS_A_GATE.md` (run01): owner decision question 3 in
`docs/reviews/2026-10-11-owner-questions-decisions-ox.md`, preregistration `docs/perf/R19_PRF04_GATE2B_PREREG.md`
(SHA-256 `27ba9e4148be61cc19304e16e37090603c12530f006f588c8379714531e3157b`, carried byte for byte in the run directory), same arms and same harness.

## 1. What was measured

- The identical schedule of 492 subprocess slots as run01, on the identical arms: A = `175e7398db175569a65a7c77ff20511bec8e592b` (tree `bec4f59026d96614d2c41696a029b8d25e5123ac`),
  B = `9ad4ccf61dce8d078e51ad17e1a4d1a1eaf4e376` (tree `78b374f66c3ad815acbc9053b2d4f373e44ac6f2`) = A plus the V4 FREE-slot hint; both worktrees clean (empty patch), A an ancestor
  of B, identical rustc/cargo/target/features. Every slot passed its oracles, 0 invalidation records.
- Entry point: real `#[global_allocator] static GLOBAL: SeferAlloc` (clean `production` example for retention; separate
  observer example for the first allocating call per thread).
- Host: win32 10.0.19045 x64, 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz. This run started when the host CPU load
  was about 18 % (run01: about 82 %), but other sessions of this repository (one rush session) were still active; the
  A/A bands below are the evidence of how quiet it actually was.

## 2. Result (verbatim from the aggregator, `report.md`)

NO-GO

| axis | arm | k | w | n | commit_idle_median | rss_idle_median | wall_ns_median | first_call_p99_median | numerator | denominator | ratio | noise | bound | pairs | mean_delta | t_stat | p_value | inconclusive | kill |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| commit | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | 5374734336 | 5420199936 | 0.9916118223429313 | 0.000009824043818258212 | unavailable | unavailable | unavailable | unavailable | unavailable | false | false |
| live | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | 1429800 | 7494400 | 0.1907824508966695 | 0.043310022111379645 | 0.05 | 40 | -1229900 | -0.039506125232203315 | 0.9686884179886748 | false | true |
| churn | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | -157062600 | 2851333100 | -0.05508391846606768 | 0.0049853400936290156 | 0.02 | 40 | -124842825 | -0.9094053795814646 | 0.3687222973427694 | false | false |
| wave_p99 | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | -2559200 | 3005200 | -0.8515905763343538 | 0.04212980308896382 | unavailable | unavailable | unavailable | unavailable | unavailable | false | false |

Ratios: (B-A)/A; commit savings (A-B)/A. AA noise: |median(A2)-median(A1)|/median(A1) (commit: per-pair bytes vs max(64MiB,25%*A1)). Diagnostic timings excluded. Secondary cells excluded from verdict.

| arm | k | w | n | commit_idle_median | rss_idle_median | wall_ns_median | first_call_p99_median |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A | 2 | 16 | 5 | 24801280 | 21901312 | 35304100 | 106600 |
| A | 2 | 128 | 5 | 177881088 | 144179200 | 500347500 | 155700 |
| A | 8 | 16 | 5 | 155717632 | 125935616 | 174679100 | 255400 |
| A | 8 | 128 | 5 | 1226403840 | 978276352 | 2861328300 | 969400 |
| A | 32 | 16 | 5 | 680034304 | 542777344 | 1584352000 | 1693400 |
| A | 32 | 128 | 5 | 5420199936 | 2042642432 | 13637800600 | 3005200 |
| B | 2 | 16 | 5 | 4395008 | 5480448 | 39200100 | 66800 |
| B | 2 | 128 | 5 | 4407296 | 5505024 | 381190100 | 69600 |
| B | 8 | 16 | 5 | 12599296 | 12173312 | 101866400 | 232500 |
| B | 8 | 128 | 5 | 12623872 | 12263424 | 2478461100 | 220800 |
| B | 32 | 16 | 5 | 45309952 | 38547456 | 985310200 | 514100 |
| B | 32 | 128 | 5 | 45465600 | 35700736 | 10501423400 | 446000 |

## 3. Reading against the frozen rules

- BENEFIT met: idle commit charge at K=32, W=128 fell by 99.16 % (5374734336 / 5420199936), from 5420199936 B (A) to
  45465600 B (B), i.e. 5169.1 MiB to 43.4 MiB (run01: -99.2 %); A/A commit band 0.0010 %.
- KILL-3 not fired: first-allocating-call p99 changed by -85.16 % (-2559200 / 3005200) ns (B faster); A/A band
  4.21 % <= 10 %.
- KILL-2 not fired: R-churn wall changed by -5.51 % (-157062600 / 2851333100) ns (B faster); bound
  max(2 %, A/A noise 0.50 %) = 2.00 %.
- **KILL-1 fired**: R-live first-call p50, median of per-process medians, changed by 19.08 % (1429800 / 7494400) ns
  (B higher; threshold 5 %), A/A band 4.33 % <= 5 %, so the axis is conclusive under the rule. The paired
  t-test over 40 pairs does not support a slowdown: mean paired delta -1229900 ns (B lower), t = -0.040,
  p = 0.969. The rule fires on the median ratio alone ("ratio > bound OR significant positive paired mean").

## 4. Dispersion of the R-live axis (diagnostic, not a gate input)

`scripts/r19_prf04_live_dispersion.mjs` on `samples_scalar.csv` (40 A and 40 B processes of the ab series, per-process
first-call p50 of the 64-thread live wave):

| arm | median | Q1 | Q3 | mean | max |
|---|---:|---:|---:|---:|---:|
| A | 7494400 | 3926400 | 11256900 | 46183843 | 535028200 |
| B | 8924200 | 6606800 | 11583900 | 44953943 | 531672200 |

The interquartile range of each arm is about 98 % of its median (A) and
56 % (B), with single processes of 0.1 s to 0.5 s. The observed
median difference is 1429800 ns; its 95 % bootstrap interval (4000 resamples, fixed seed,
unpaired) is [-402700, 3213150] ns, i.e. it contains 0 and is roughly
+-24 % of the A median wide. A 5 % bound
(about 374720 ns) is far below that resolution; the A/A band of 4.33 % in this run (run01: 13.8 %) is a
single same-A pair of medians and varies by a factor of three between runs of the same code. The frozen estimator
therefore decides this axis largely by chance in either direction.

## 5. What this run does and does not establish

- Established: the idle-commit benefit of V4 reproduces (-99.2 % in both runs), the wave p99 and churn axes favour B,
  and under the frozen KILL-1 rule the V4 candidate is NO-GO on this data.
- Not established: that V4 slows live threads. The data are compatible with no change and with a change of several
  milliseconds in either direction.
- V4 is not merged. Any change to the KILL-1 estimator (for example a confidence-interval test with a preregistered
  sample size) is a protocol change for the arbiter, to be preregistered before new data; it cannot be applied to
  these samples.

## 6. Artifacts and storage tier

Committed in `docs/perf/R19_PRF04_B_run02/`: `identity.json`, `prereg.md`, `schedule.json`, the empty source patches,
`summary.csv`, `report.md`, `verdict.json`, `samples_scalar.csv` (derived from the raw samples by
`scripts/r19_prf04_gate2b_excerpt.mjs`, which asserts that it re-derives the numerator and denominator of all four
axes) and `MANIFEST.txt`. The raw `samples.json` (4141735 bytes, tier 3) and the 492 per-slot files are not committed;
the manifest carries the SHA-256. Reproduce: `node scripts/r19_prf04_gate2b.mjs --run <outdir> --arm-a <A worktree> --arm-b <B worktree>`.
