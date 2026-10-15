# R6-T31 / R6-T32 gate — pool decay on Large edges, lazy directory rebuild (iai A/B)

`fix(perf)` change (PR #48): `maybe_decay_small_pool` also runs next to
every `maybe_decay_large_cache` call (Large alloc, free, reclaim), and
`find_segment_with_free_impl` calls `maybe_materialize_directory` before its
fallback scan. This gate measures the cost of those two calls; it makes no
speed claim.

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR label `gate-run`),
  bench target `perf_gate_iai`, features `production bench-internals internals`.
- Arm A = `c8885b66526a3fcf99c39e88a876b414c84a6e76` (merge-base, `main`),
  arm B = `13aa9dbac5e3934c81fb623aa5e09b0db2f8017d` (PR head); each built in its
  own worktree and target directory. rustc 1.99.1 (58cf8f9c3 2026-10-13),
  Linux 6.17.0-1022-azure (GitHub Actions).
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  alloc/free paths (`SeferAlloc`'s `#[global_allocator]` chain), which reach
  both edited sites.
- Thresholds (max tolerated regression, %), set before the run: 1.0 on
  `large_alloc_free_cycle`, `large_cache_hit_only_4mib`, `realloc_grow`,
  `cold_alloc_only_256x16b`, `recycle_alloc_only_256x16b`; 0.5 on
  `small_churn_16b`, `alloc_magazine_hit_only_16b`, `dealloc_free_only_16b`;
  every other cell is informational.

## Result

The script-generated table is committed verbatim as
`docs/perf/_raw_r6_t31_verdict.log` (106 cells, `VERDICT: PASS`); the per-cell
CSV is `docs/perf/R6_T31_T32_POOL_DECAY_DIRECTORY_REBUILD_GATE_summary.csv`;
raw iai output for both arms is `docs/perf/_raw_r6_t31_iai_{A,B}.log`.

Reading of that table (all figures copied from the generated CSV):

- Gated cells: `large_cache_hit_only_4mib` +290 of 57632,
  `cold_alloc_only_256x16b` +368 of 109387, `recycle_alloc_only_256x16b`
  +368 of 203309, `realloc_grow` +340 of 579133, `large_alloc_free_cycle`
  +32 of 47673; hot Small cells +23 of 54178 (`alloc_magazine_hit_only_16b`)
  and +92 of 67967 (`dealloc_free_only_16b`).
- Largest informational costs: `large_cache_eviction_hit_only_41x4mib`
  +2624 of 296550, `large_cache_free_slot_search_cycle_only` +513 of 72285,
  the 1088-block free cells +1564 of about 325000.
- 72 of 106 cells grew, none shrank; the 13 `mimalloc_*` control cells have a
  delta of exactly 0.

## Verdict

PASS on the pre-set thresholds. The change costs a few hundred `Ir` per bench
round on the Large and segment-discovery paths; hot magazine hits move by
tens. The memory benefit of R6-T31 (an idle-but-live owner drains its small
pool) is covered by `tests/r6_t31_small_pool_idle_decay.rs` only; RSS is not
measured here. No wall-clock claim is made. Not a production
feature-composition change.
