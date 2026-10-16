# R6-T10 / R6-T39 gate — bounded Large-sweep probe, table refill count, witness-root Large free (iai A/B)

`perf(runtime)` change (PR #49): `HeapCore`'s Large alloc probes at most 4
Large sidecar slots through the persistent `large_hot_cursor` instead of
sweeping every live Large slot, as `alloc_batch` already did (R12-05); the
rotating probe retires remote Large publications over successive allocs
(R6-T10). The refill count comes from the `FREE_PARK_CAP` table (PF-7),
own-thread Large free passes the witness root (PF-10), and magazine slot
indices are masked (R6-T13).

A first version of the PR also ran a full sweep on every Large cache miss
before an OS reservation. It broke the R12-05 bounded-probe contract
(`tests/r11_p3_large_hot_late.rs`, `tests/r12_05_batch_large_bounded.rs`) and
was removed; the numbers below are for the final code.

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR label
  `gate-run`), features `production bench-internals internals`, rustc 1.99.1
  (58cf8f9c3 2026-10-13), Linux 6.17.0-1022-azure (GitHub Actions). Each arm
  is built in its own worktree and target directory.
- Run 1, bench target `perf_gate_iai`: A =
  `36ea9f26fecf2b8dc4555de0e33b93978ee3db38` (merge-base, `main`), B =
  `b86ab03af2eb05e28d46a390cb14f6576eb49926` (PR head). Thresholds (max
  tolerated regression, %), set before the run: 0.5 on
  `large_alloc_free_cycle`, `large_cache_hit_only_4mib`,
  `large_cache_eviction_hit_only_41x4mib`,
  `large_cache_eviction_prefill_only_41x4mib`, `cold_alloc_only_256x16b`,
  `recycle_alloc_only_256x16b`, `alloc_magazine_hit_only_16b`,
  `small_churn_16b`, `dealloc_free_only_16b`, `dealloc_free_only_16b_n8`,
  `dealloc_free_only_16b_n16`; 1.0 on `realloc_grow`; every other cell is
  informational.
- Run 2, bench target `r6_live_large_iai` (added in PR #51 so both arms carry
  it): A = `36ea9f26fecf2b8dc4555de0e33b93978ee3db38`, B =
  `dfc47d60e3d941f3a936ab258ca83069cdf5be78` (B differs from run 1's B only in
  a test file). No thresholds; informational.
  `large_cache_live16_prefill_only` pins 16 live 8 MiB Large objects and
  warms one 4 MiB cache slot; `large_cache_live16_hit_only` adds 16 cache-hit
  rounds.
- Entry layer: both targets drive `HeapCore::alloc`/`dealloc`, the chain the
  `#[global_allocator]` uses.

## Result

Script-generated tables, committed verbatim:
`docs/perf/_raw_r6_t10_perf_gate_verdict.log` (run 1, 106 cells,
`VERDICT: PASS`) and `docs/perf/_raw_r6_t10_live16_verdict.log` (run 2).
Per-cell CSV for both runs: `docs/perf/R6_T10_LARGE_SWEEP_PROBE_GATE_summary.csv`.
Raw iai output: `docs/perf/_raw_r6_t10_{perf_gate,live16}_iai_{A,B}.log`.

Run 1 (figures copied from the generated CSV):

- Many live Large objects: `large_cache_eviction_hit_only_41x4mib` −70333 of
  298163, `large_cache_eviction_prefill_only_41x4mib` −35189 of 188272.
- Refill cells: `cold_alloc_only_256x16b` −320 of 100913,
  `recycle_alloc_only_256x16b` −186 of 187574.
- Few live Large objects: `large_cache_hit_only_4mib` +46 of 56911,
  `large_alloc_free_cycle` +7 of 46716.
- 37 of 106 cells grew, the largest by +300 of 77059
  (`oscillating_live_set_16b`); 35 shrank.

Run 2:

| cell | A Ir | B Ir |
|---|---|---|
| `large_cache_live16_prefill_only` | 78517 | 74899 |
| `large_cache_live16_hit_only` | 114091 | 100308 |

Cost of one cache hit with 16 live Large objects, (hit − prefill) / 16 hits:
A = (114091 − 78517) / 16 = 2223 Ir; B = (100308 − 74899) / 16 = 1588 Ir.

## Verdict

PASS on run 1's pre-set thresholds. With many live Large objects every Large
alloc gets cheaper, cache hit or miss, because it no longer visits every live
Large slot; with one or two live Large objects the change is within +46 Ir.
No wall-clock claim is made. Not a production feature-composition change.
