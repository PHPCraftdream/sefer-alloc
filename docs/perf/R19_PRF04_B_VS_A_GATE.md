# R19 PRF04 B-vs-A thread-wave gate (run01) — bench / measurement-only

**Verdict: INCONCLUSIVE — the benefit threshold is met and no kill criterion fired, but the
KILL-1 A/A band (13.8 %) is wider than its 5 % limit on this loaded host, so no GO
is issued. Nothing changed in the allocator on `main`; the V4 candidate stays on its branch.**

Authority: the owner decision (question 3) in `docs/reviews/2026-10-11-owner-questions-decisions-ox.md`
and the preregistration `docs/perf/R19_PRF04_GATE2B_PREREG.md` (SHA-256 `27ba9e4148be61cc19304e16e37090603c12530f006f588c8379714531e3157b`),
which this run carried byte for byte (`prereg.md` in the run directory).

## 1. What was measured

- Entry point: a real `#[global_allocator] static GLOBAL: SeferAlloc`. The retention numbers come
  from the clean `production` example `r19_prf04_gate2b`; the first-allocating-call latencies come
  from the separate observer example `r19_prf04_gate2b_observer`, whose `GlobalAlloc` wrapper times
  the first allocating call per thread (TLS bind/claim included) and forwards everything else.
  Diagnostic `production bench-internals alloc-stats internals` builds validate the mechanism only;
  their timings never enter a verdict.
- Arm A = `175e7398db175569a65a7c77ff20511bec8e592b` (tree `bec4f59026d96614d2c41696a029b8d25e5123ac`), arm B = `9ad4ccf61dce8d078e51ad17e1a4d1a1eaf4e376` (tree `78b374f66c3ad815acbc9053b2d4f373e44ac6f2`) = A plus the
  V4 FREE-slot hint. Both worktrees were clean (patch SHA-256 `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` = SHA-256 of the
  empty input, so identity is the commit). The shared harness and runner were byte-identical between
  the arms; A is an ancestor of B; rustc `rustc 1.97.0 (2d8144b78 2026-07-07)`, cargo `cargo 1.97.0 (c980f4866 2026-06-30)`, target
  `x86_64-pc-windows-msvc`, `--locked`, release profile, identical on both arms.
- Schedule: 492 subprocess slots (wave 5 repetitions x 3 K x 2 W x 2 builds x 2 arms, 5 A/A wave
  blocks, 20 ABBA + 20 AAAA blocks each for R-live and R-churn, 32 diagnostic slots). Every slot
  passed its oracles (count deltas, claim sources, resolved configuration, config conflicts == 0,
  maintenance not running, exactly one first allocating call per worker, live gate order); there
  were 0 invalidation records.
- Host: win32 10.0.19045 x64, 11th Gen Intel(R) Core(TM) i7-11800H @ 2.30GHz.
  **The host was not quiet:** other projects' build and agent sessions were running during the
  measurement (about 82 % CPU load observed before it started); this was not controlled.

## 2. Preregistered rules applied

BENEFIT (K=32, W=128, median of 5): `commit_idle(B) <= commit_idle(A) - max(64 MiB, 25 % of A)`.
KILL-1 (R-live first-call p50): `|delta| <= 5 %`, no significant paired-t slowdown, A/A band `<= 5 %`
else INCONCLUSIVE. KILL-2 (R-churn wall): `delta <= max(2 %, A/A noise)`. KILL-3 (R-wave K=32/W=128,
median of five per-run first-call p99): `B <= 1.10 x A`, A/A band `<= 10 %` else INCONCLUSIVE.
A/A noise is the verdict statistic applied to same-A pairs (preregistration amendment A1, made
before any sample; the runner also needed a one-line fix for fresh worktrees, commit `175e7398`).

## 3. Result (verbatim from the aggregator, `report.md`)

INCONCLUSIVE

| axis | arm | k | w | n | commit_idle_median | rss_idle_median | wall_ns_median | first_call_p99_median | numerator | denominator | ratio | noise | bound | pairs | mean_delta | t_stat | p_value | inconclusive | kill |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| commit | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | 5374722048 | 5420187648 | 0.9916118033262601 | 0.000010579619571993934 | unavailable | unavailable | unavailable | unavailable | unavailable | false | false |
| live | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | -1127750 | 8735600 | -0.12909817299326892 | 0.13807496544795547 | 0.05 | 40 | -23615815 | -0.8940929990648315 | 0.37675680969157654 | true | false |
| churn | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | -53711500 | 3278041500 | -0.016385241004422915 | 0.04636067036426167 | 0.04636067036426167 | 40 | 266779962.5 | 0.8071316533663473 | 0.4244859100622661 | false | false |
| wave_p99 | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | unavailable | -3389200 | 4097600 | -0.8271183131589223 | 0.034231059652655424 | unavailable | unavailable | unavailable | unavailable | unavailable | false | false |

Ratios: (B-A)/A; commit savings (A-B)/A. AA noise: |median(A2)-median(A1)|/median(A1) (commit: per-pair bytes vs max(64MiB,25%*A1)). Diagnostic timings excluded. Secondary cells excluded from verdict.

| arm | k | w | n | commit_idle_median | rss_idle_median | wall_ns_median | first_call_p99_median |
| --- | --- | --- | --- | --- | --- | --- | --- |
| A | 2 | 16 | 5 | 24801280 | 21950464 | 47284400 | 206000 |
| A | 2 | 128 | 5 | 177917952 | 144306176 | 410317500 | 226700 |
| A | 8 | 16 | 5 | 155717632 | 125947904 | 127685000 | 1424600 |
| A | 8 | 128 | 5 | 1226412032 | 978587648 | 2026221700 | 533000 |
| A | 32 | 16 | 5 | 680058880 | 542846976 | 954529400 | 3083200 |
| A | 32 | 128 | 5 | 5420187648 | 4317294592 | 14269873000 | 4097600 |
| B | 2 | 16 | 5 | 4411392 | 5509120 | 44461500 | 103200 |
| B | 2 | 128 | 5 | 4411392 | 5517312 | 294276600 | 82400 |
| B | 8 | 16 | 5 | 12599296 | 12185600 | 116841500 | 238900 |
| B | 8 | 128 | 5 | 12652544 | 12288000 | 2017664100 | 251600 |
| B | 32 | 16 | 5 | 45350912 | 38608896 | 728985400 | 982900 |
| B | 32 | 128 | 5 | 45465600 | 38805504 | 8851544700 | 708400 |

## 4. Reading

- BENEFIT met: idle commit-charge at K=32, W=128 fell by 99.2 % (5374722048 / 5420187648),
  from 5420187648 B (A) to 45465600 B (B), i.e. 5169.1 MiB to 43.4 MiB; the five same-A
  pairs differ by at most 0.0011 % (A/A commit band, far inside max(64 MiB, 25 % of A1)).
- KILL-3 not fired: median first-allocating-call p99 changed by -82.7 % (-3389200 / 4097600)
  ns (negative = B faster); A/A band 3.4 % <= 10 %.
- KILL-2 not fired: median R-churn wall changed by -1.6 % (-53711500 / 3278041500) ns;
  the bound was max(2 %, A/A noise 4.64 %) = 4.64 %.
- KILL-1 inconclusive: median R-live first-call p50 changed by -12.9 % (-1127750 / 8735600)
  ns (B lower), paired t = -0.894, p = 0.377 over 40 pairs (not
  significant), but the A/A band is 13.8 % against the 5 % limit. The rule is explicit:
  the axis is INCONCLUSIVE and GO is not issued until a run on a non-noisy host.

Not claimed: no GO; no statement about hosts other than this loaded Windows 10 machine; the secondary
cells in the table are descriptive and excluded from the verdict; commit-charge is not a continuous
OS peak; V4 is **not** on `main`.

## 5. Artifacts and storage tier

Committed in `docs/perf/R19_PRF04_B_run01/`: `identity.json`, `prereg.md`, `schedule.json`,
`A.source.patch` and `B.source.patch` (both empty: clean arms), `summary.csv`, `report.md`,
`verdict.json`, `samples_scalar.csv` and `MANIFEST.txt`. `samples_scalar.csv` (one row per slot, the
scalar columns plus per-slot first-call p50/p99) is derived from the raw samples by
`scripts/r19_prf04_gate2b_excerpt.mjs`, which asserts that it re-derives the numerator and denominator
of all four verdict axes. The raw `samples.json` (4147759 bytes uncompressed, above the 2 MiB
ceiling of the artifact-storage policy, tier 3) and the 492 per-slot files it concatenates are **not**
committed; `MANIFEST.txt` records the SHA-256. They were retained only on the measuring machine; no
external store was available, so independent verification of the raw arrays needs a rerun.

Reproduce: from a checkout of the pair, `node scripts/r19_prf04_gate2b.mjs --run <outdir> --arm-a <A worktree>
--arm-b <B worktree>`, then `node scripts/r19_prf04_gate2b_excerpt.mjs <outdir>`.

## 6. Next

Rerun the unchanged schedule when the host is quiet (target: the live A/A band at or below 5 %). A GO
needs BENEFIT, no kill, valid oracles and every band within its limit; then the V4 commit
(`9ad4ccf61dce8d078e51ad17e1a4d1a1eaf4e376`) is cherry-picked as `perf(runtime)`. If the band stays above 5 % on the quietest host
available, the band itself goes back to the arbiter; it is not widened here.
