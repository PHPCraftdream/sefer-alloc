# R6-T11 — Tier-1 `own_cache` as a (key, root) pair array: iai A/B gate

Runtime change under test: `perf(runtime)` — the Tier-1 own-segment cache in
`SegmentTable` stores key and root in one `[(usize, *mut u8); 16]` array with
an empty key of `usize::MAX`, so a hit is one key comparison (PR #43).

## Identity and method

- Runner: `scripts/r20_r6_ab_iai_run.mjs` (`r20-gates.yml`, PR label `gate-run`),
  bench target `perf_gate_iai`, features `production bench-internals internals`.
- Arm A = `38bef3618d4c21c1eb21535f60aa5b2a8eda84f1` (merge-base, `main`),
  arm B = `ec8fcbabf71a7b5e59463f128b5bbcb13fb812c3` (PR head); each built in its
  own worktree and target directory. rustc 1.99.0 (b940084d7 2026-09-28),
  Linux 6.17.0-1022-azure (GitHub Actions).
- Entry layer: `perf_gate_iai` cells drive `HeapCore` through the production
  free/alloc paths that call `SegmentTable::canonical_base_of{,_mut}`; the
  probe-only cell isolates the Tier-1 lookup.
- Thresholds (max tolerated regression, %): 0.5 on
  `dealloc_contains_base_probe_only_16b`, `dealloc_free_only_16b{,_n8,_n16}`,
  `small_churn_16b{,_2n}`; every other cell is informational.

## Result

The script-generated table is committed verbatim as
`docs/perf/_raw_r6_t11_verdict.log` (106 cells, `VERDICT: PASS`); the per-cell
CSV is `docs/perf/R6_T11_OWN_CACHE_PAIR_LAYOUT_GATE_summary.csv`; raw iai
output for both arms is `docs/perf/_raw_r6_t11_iai_{A,B}.log`.

Reading of that table (all figures copied from the generated CSV):

- Tier-1 lookup cell `dealloc_contains_base_probe_only_16b`: 62957 → 62802 Ir
  (delta −155 of 62957).
- Gated free/churn cells move by −4 … +28 Ir (`dealloc_free_only_16b`: −4 of
  68545; `small_churn_16b`: +21 of 54445) — inside the threshold, no win.
- Informational cells with the largest relative change: `growth16_n` −77814 of
  21560868, `oscillating_live_set_16b` −290 of 77579; small costs on cold and
  recycle carves, e.g. `cold_alloc_free_256x16b` +186 of 145361,
  `recycle_alloc_free_256x16b` +351 of 240918.

## Verdict

PASS on the pre-set thresholds. The expected ~1–2 Ir per lookup shows up on
the isolated probe cell; end-to-end free and churn cells are flat, and cold
or recycle carves pay a few hundred Ir per 256 operations. No wall-clock
claim is made. Not a production feature-composition change.
