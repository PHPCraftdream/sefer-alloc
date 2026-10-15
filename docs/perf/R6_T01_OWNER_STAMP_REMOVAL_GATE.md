# R6-T01 gate — removing the dead `owner_state` stamp (iai A/B)

`perf(runtime)` change: `stamp_segment_owner`, the `last_stamped_segment`
dedupe field and the Large cache-hit `owner_state` reset are removed (the
header word had no production reader), so `finish_magazine_refill` walks the
refilled blocks once instead of twice (PR #45).

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR label `gate-run`),
  bench target `perf_gate_iai`, features `production bench-internals internals`.
- Arm A = `27b188934e174a7b43c5ee0cd73a99dfe79bc60c` (merge-base, `main`),
  arm B = `dcd35d256df4ff8691be34f0ec6065b1d78540a8` (PR head); each built in its
  own worktree and target directory. rustc 1.99.1 (58cf8f9c3 2026-10-13),
  Linux 6.17.0-1022-azure (GitHub Actions).
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  alloc/free paths (`SeferAlloc`'s `#[global_allocator]` chain); the magazine
  refill (`finish_magazine_refill`) and the Large cache hit are the edited
  sites.
- Thresholds (max tolerated regression, %), set before the run: 0.5 on
  `cold_alloc_only_256x16b{,_2n,_4n}`, `recycle_alloc_only_256x16b`,
  `large_alloc_free_cycle`, `large_cache_hit_only_4mib`,
  `small_churn_16b{,_2n}`, `alloc_magazine_hit_only_16b`,
  `dealloc_free_only_16b`; every other cell is informational.

## Result

The script-generated table is committed verbatim as
`docs/perf/_raw_r6_t01_verdict.log` (106 cells, `VERDICT: PASS`); the per-cell
CSV is `docs/perf/R6_T01_OWNER_STAMP_REMOVAL_GATE_summary.csv`; raw iai
output for both arms is `docs/perf/_raw_r6_t01_iai_{A,B}.log`.

Reading of that table (all figures copied from the generated CSV):

- Refill-miss cells drop linearly with the number of refills:
  `cold_alloc_only_256x16b` −3652 of 113034, `_2n` −7268 of 185044, `_4n`
  −14500 of 327504; `recycle_alloc_only_256x16b` −7012 of 210316.
- Largest relative change: the 1088-block free cells, e.g.
  `dealloc_free_only_1088_16b_n17` −15441 of 341233.
- Large cells: `large_alloc_free_cycle` −36 of 47699,
  `large_cache_hit_only_4mib` −298 of 57904.
- No cell of the 106 has a positive delta; the 13 `mimalloc_*` control cells
  have a delta of exactly 0.

## Verdict

PASS on the pre-set thresholds. The removed second pass shows up as an `Ir`
drop proportional to the number of refilled blocks; hot magazine hits and
frees move by a few hundred `Ir` per bench round. No wall-clock claim is made.
Not a production feature-composition change.
