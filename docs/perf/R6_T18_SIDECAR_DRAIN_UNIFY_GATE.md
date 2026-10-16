# R6-T18 gate — unified owner-side Small sidecar drain (iai A/B)

`refactor` change: the discovery drain (`find_segment.rs`), the bounded
maintenance step and the strict trim (`sidecar_drain.rs`) share one
cut-consume core, one finalization and one Large route retirement (PR #56).
No behavior change is intended; this gate checks that the refactor is
`Ir`-neutral on the pre-set cells.

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR label `gate-run`),
  bench target `perf_gate_iai`, features `production bench-internals internals`.
- Arm A = `6bf9919c7b8003aabc88cc74e8c2b39203b74226` (merge-base, `main`),
  arm B = `006058480e0f2a5cd3bc31c91091b0ae0055d987` (PR head at measurement);
  each built in its own worktree and target directory. rustc 1.99.1
  (58cf8f9c3 2026-10-13), Linux 6.17.0-1022-azure (GitHub Actions). The only
  later source commit on the PR adds `let _ = nonempty;` under the cfg where
  the r19 summary oracle is off (an unused-variable fix, no code generated).
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  alloc/free paths (`SeferAlloc`'s `#[global_allocator]` chain); the edited
  sites are the owner-side sidecar drains those paths call.
- Thresholds (max tolerated regression, %), set before the run: 0.5 on
  `cold_alloc_only_256x16b`, `recycle_alloc_only_256x16b`,
  `carve_batch_only_16b`, `small_churn_16b`, `alloc_magazine_hit_only_16b`,
  `dealloc_free_only_16b`, `large_alloc_free_cycle`,
  `large_cache_hit_only_4mib`; every other cell is informational.

## Result

The script-generated table is committed verbatim as
`docs/perf/_raw_r6_t18_verdict.log` (106 cells, `VERDICT: PASS`); the per-cell
CSV is `docs/perf/R6_T18_SIDECAR_DRAIN_UNIFY_GATE_summary.csv`; raw iai
output for both arms is `docs/perf/_raw_r6_t18_iai_{A,B}.log`.

Reading of that table (all figures copied from the generated CSV):

- The eight thresholded cells move by at most 10 `Ir`
  (`cold_alloc_only_256x16b` −10 of 102832, `dealloc_free_only_16b` +8 of
  65914); all PASS.
- The largest positive delta of any cell is +8 `Ir`.
- Seven informational cells drop by more than 1000 `Ir`:
  `seg_cycle_decommit_256k` −2007908 of 12367045, `multiseg_cold_256k`
  −637732 of 4027567, `r19_prf01_publication` −710916 of 4332455,
  `growth16_n` / `_2n` / `_4n` −1041410 of 20282585 / −4179970 of 53104091 /
  −16748546 of 156495976, `realloc_grow` −2830 of 588759.
- Every `mimalloc_*` control cell has a delta of exactly 0.

## Verdict

PASS on the pre-set thresholds. The drop in those seven informational cells
is not explained by this report: the exact-count drain oracles
(`r11_ph4c_ingress_consume_exactly_once_oracle`, `r19_prf01_sidecar_oracle`,
`r6_terminal_owner_drain`, `r6_sidecar_bitmap`) pass unchanged on the PR, so
the same records are consumed and the same segments released. The likely
cause is code generation around the shared helpers, but this was not checked
against the assembly. No speedup is claimed and no wall-clock measurement was
made. Not a production feature-composition change.
