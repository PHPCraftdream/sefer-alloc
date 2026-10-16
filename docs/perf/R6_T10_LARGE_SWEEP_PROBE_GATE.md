# R6-T10 / R6-T39 gate — bounded Large-sweep probe, table refill count, witness-root Large free (iai A/B)

`perf(runtime)` change (PR #49): a Large alloc probes at most 4 Large sidecar
slots through a persistent cursor instead of sweeping every live Large slot;
a cache miss runs one full sweep and retries the cache before any OS
reservation (R6-T10). The refill count comes from the `FREE_PARK_CAP` table
(PF-7), own-thread Large free passes the witness root (PF-10), and magazine
slot indices are masked (R6-T13).

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR label
  `gate-run`), features `production bench-internals internals`, rustc 1.99.1
  (58cf8f9c3 2026-10-13), Linux 6.17.0-1022-azure (GitHub Actions). Each arm
  is built in its own worktree and target directory.
- Run 1, bench target `perf_gate_iai`: A =
  `1a3fe7d16ea4dcb867ec4a5b267fe58c8e934772` (merge-base, `main`), B =
  `ad91471dab23192fda115f930c6f240f74bf9c13` (PR head). Thresholds (max
  tolerated regression, %), set before the run: 0.5 on
  `large_alloc_free_cycle`, `large_cache_hit_only_4mib`,
  `cold_alloc_only_256x16b`, `recycle_alloc_only_256x16b`,
  `alloc_magazine_hit_only_16b`, `small_churn_16b`, `dealloc_free_only_16b`,
  `dealloc_free_only_16b_n8`, `dealloc_free_only_16b_n16`; 1.0 on
  `realloc_grow`; every other cell is informational.
- Run 2, bench target `r6_live_large_iai` (added in PR #51 so both arms carry
  it): A = `36ea9f26fecf2b8dc4555de0e33b93978ee3db38`, B =
  `d0216d18dad8545b1e21203932bfba377d8bcc7b`. No thresholds; informational.
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

- Gated cells: `cold_alloc_only_256x16b` −320 of 100545,
  `recycle_alloc_only_256x16b` −186 of 187206, `dealloc_free_only_16b` +40 of
  65223, `large_alloc_free_cycle` +91 of 46684, `large_cache_hit_only_4mib`
  +163 of 56621.
- Largest informational costs, both on the Large cache-miss path:
  `large_cache_eviction_hit_only_41x4mib` +14852 of 295539,
  `large_cache_eviction_prefill_only_41x4mib` +8435 of 186960.

Run 2:

| cell | A Ir | B Ir |
|---|---|---|
| `large_cache_live16_prefill_only` | 78517 | 82991 |
| `large_cache_live16_hit_only` | 114091 | 108432 |

Cost of one cache hit with 16 live Large objects, (hit − prefill) / 16 hits:
A = (114091 − 78517) / 16 = 2223 Ir; B = (108432 − 82991) / 16 = 1590 Ir.
The prefill arm's 17 allocs are cache misses and cost +4474 Ir in total.

## Verdict

PASS on run 1's pre-set thresholds. The change trades Large cache-miss cost
for cache-hit cost: with 16 live Large objects a cache hit costs 1590 Ir
instead of 2223 Ir, while each Large alloc that misses the cache pays the
bounded probe on top of the full sweep (+4474 Ir over 17 misses in run 2,
+14852 Ir over the 82 allocs of the eviction cell in run 1). The miss-path
cost is filed as perf item 101. No wall-clock claim is made. Not a production
feature-composition change.
