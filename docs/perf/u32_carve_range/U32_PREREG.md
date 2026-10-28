# U32 — pre-registration (fixed before any time measurement of this variant)

Base: `main` @ `15f057fe`. Measurement branch `u32-meas` (ablation harness of
`gr1-meas` cherry-picked, no `src/` change). Candidate code: `u32` @
`70f48059`; the same diff is the judge patch
`docs/perf/b4_ablate/patches/u32_carve_range.patch`.

Variant `u32_carve_range`: `try_carve_batch` issues its contiguous bump run per
4 KiB leaf (`IssueRun::prepare_range` / `commit_range`): one leaf check and one
leaf resolve per leaf instead of per block; every block keeps its Release
class store, its state-byte store and its pointer write. Fault countdown and
refusal semantics are unchanged (`tests/u32_carve_issue_range.rs`).

Callgrind (WSL, growth16 probe, MEASURED before this file), total Ir:

| n | base | candidate | Δ |
|---:|---:|---:|---:|
| 32768 | 7 230 443 | 6 608 134 | −8.6 % |
| 65536 | 13 905 354 | 12 660 709 | −9.0 % |
| 131072 | 27 268 133 | 24 778 802 | −9.1 % |

Path activation: in the base profile `try_carve_batch` runs 8 192 times for
n = 131072 with 16-block refills, i.e. every one of the 131 072 blocks of the
fresh heap is carved. Instructions are not time; the verdict is the time
judges below.

## Judges

A. growth16 process judge (`scripts/u32_growth_judge.mjs`, probe
`examples/u32_growth_time_probe.rs`, `SeferAlloc::new()` = the
`#[global_allocator]` face): one fresh process per sample, 40 ABBA pairs per
arm set, effect = median ln(t_candidate / t_base) of total time, 95 % CI by
bootstrap over pairs. Runs: n = 131072 and n = 32768 (base vs candidate), and
a null run (base vs base) at n = 131072.

B. J1′ v2 ablation (`scripts/b4_ablate_run.mjs`, 20 processes × 2000 blocks,
all 11 kernels), effect = variant median − null median
(`scripts/b4_ablate_vs_null.mjs`), 95 % CI by process bootstrap.

## Thresholds

1. Win: judge A, n = 131072: CI upper < 0.
2. Calibration: judge A null run CI contains 0.
3. No harm: judge B pair16, pair64, larson effect ≤ +1 % and CI upper ≤ +1.5 %.
4. No harm, other kernels: not (CI lower > 0 AND effect > +2 %).

GO = 1 ∧ 2 ∧ 3 ∧ 4. Judge A n = 32768 is reported, not gated.

## Result of `u32_carve_range` (v1) — judge A, recorded before v2 was written

| run | effect % [95 % CI] |
|---|---|
| null, n = 131072, 40 pairs | −2.80 [−4.59, 4.51] |
| v1, n = 131072, 40 pairs | +4.18 [1.36, 6.82] |
| v1, n = 32768, 40 pairs | +1.05 [−5.24, 3.58] |

Threshold 1 fails (CI upper > 0): v1 is NO-GO; judge B was not run for it.
Suspected cause: `LeafCursor::blocks_in_leaf` divides (`div_ceil`, a 64-bit
`div` per leaf, one instruction to callgrind, tens of cycles to the CPU).

## v2 `u32_carve_range_v2` — pre-registration (before any measurement of v2)

Code: `u32` @ `bb8aacab` (v1 + `blocks_in_leaf` counts by stepping, no
divide); judge patch `docs/perf/b4_ablate/patches/u32_carve_range_v2.patch`.
The v1 null shows ±4.5 % process noise, so judge A for v2 uses 120 ABBA pairs
for both the null (base vs base) and v2 at n = 131072; n = 32768 stays at 40
pairs, reported only. Thresholds 1–4 and GO rule unchanged. Callgrind of v2 is
measured after this file and reported, not gated.
