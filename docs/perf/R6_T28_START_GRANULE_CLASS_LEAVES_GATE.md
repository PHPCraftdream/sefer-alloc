# R6-T28 gate — class leaves keep the class in the start granule only (iai A/B)

Correctness change (`fix`): the Small sidecar class leaves record a block's
class only in its start granule, so a cross-thread free of an interior pointer
no longer resolves to a class (PR #42). This gate checks the hot-path cost the
change could carry; it is not a speed claim.

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR label `gate-run`),
  bench target `perf_gate_iai`, features `production bench-internals internals`.
- Arm A = `27b188934e174a7b43c5ee0cd73a99dfe79bc60c` (merge-base, `main`),
  arm B = `271f8425cc0b26e2471bca7cbbe2ca92a2549355` (PR head); each built in its
  own worktree and target directory. rustc 1.99.1 (58cf8f9c3 2026-10-13),
  Linux 6.17.0-1022-azure (GitHub Actions).
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  alloc/free paths (`SeferAlloc`'s `#[global_allocator]` chain); the Small
  issue path (`alloc_core_small_impl.rs`) publishes into the sidecar the change
  edits.
- Thresholds (max tolerated regression, %), set before the run: 0.5 on
  `alloc_magazine_hit_only_16b`, `small_churn_16b{,_2n}`,
  `dealloc_free_only_16b`; 1.0 on `carve_batch_only_16b{,_2n}`,
  `cold_alloc_only_256x16b{,_2n}`, `recycle_alloc_only_256x16b`; every other
  cell is informational.

## Result

The script-generated table is committed verbatim as
`docs/perf/_raw_r6_t28_verdict.log` (106 cells, `VERDICT: PASS`); the per-cell
CSV is `docs/perf/R6_T28_START_GRANULE_CLASS_LEAVES_GATE_summary.csv`; raw iai
output for both arms is `docs/perf/_raw_r6_t28_iai_{A,B}.log`.

Reading of that table (all figures copied from the generated CSV):

- Carve-heavy cells drop with the number of carved blocks:
  `cold_alloc_only_256x16b` −8877 of 113034, `_2n` −16730 of 185044, `_4n`
  −32436 of 327504; `recycle_alloc_only_256x16b` −16138 of 210316;
  `dealloc_free_only_1088_16b_n17` −34169 of 341233.
- Hot cells move by a constant offset per bench: −1227 Ir on
  `alloc_magazine_hit_only_16b` (of 54404) and `small_churn_16b` (of 54849),
  −1024 Ir on every Large cell (e.g. `large_cache_hit_only_4mib` −1024 of
  57904). A constant offset across unrelated cells points at one-time setup,
  not per-operation work; the gate does not decompose it.
- `carve_batch_only_16b` −4 of 2757.
- Three cells grew: `realloc_grow` +8039 of 581188,
  `r19_prf01_joined_producer_same_word` +4096 of 1519652,
  `multiseg_cold_256k` +3152 of 4028198. All three are informational, and
  their cause is not identified here.
- The 13 `mimalloc_*` control cells have a delta of exactly 0.

## Verdict

PASS on the pre-set thresholds: the correctness fix carries no hot-path `Ir`
cost, and the carve cells get cheaper. Sidecar memory is not measured here;
the batch witness `tests/r11_ph3a_issue_transaction_batch.rs` pins the
sidecar growth at 256 bytes per fresh leaf. No wall-clock claim is made. Not
a production feature-composition change.
