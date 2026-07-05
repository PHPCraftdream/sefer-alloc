# iai-callgrind Ir baseline

Deterministic instruction-count (`Ir`) baseline for `benches/perf_gate_iai.rs`,
the reference future perf work (e.g. W4 `carve_batch`) diffs against.

- **Commit:** post-W3 (`alloc-stats` gating; the W2 tombstone-rebuild is
  Ir-neutral). Original baseline was `4e139b0`; W3 gated the per-hit stats bump
  out of `production`, moving every hit-heavy bench BELOW the original baseline
  (small_churn −59, cold_16b −236, recycle_16b −477). W4 (`carve_batch`) diffs
  against THIS table.
- **Features:** `production` (`alloc-global` + `alloc-xthread` + `alloc-decommit`
  + `fastbin`) — the same set the CI perf-gate benches with, so these numbers
  match `.github/workflows/perf-gate.yml`. (`stats` counters are OFF in
  `production`; add `alloc-stats` to restore them at ~+59/+236/+477 Ir.)
- **How to reproduce:** `npm run iai` (from repo root). Drives the Linux-only
  bench through WSL under `valgrind --tool=callgrind` (`scripts/iai.mjs`).
- **Runner:** `iai-callgrind-runner 0.14.2` in WSL (pinned `^0.14`, matching
  `iai-callgrind = "0.14"` in `Cargo.toml`); valgrind 3.22.0.
- **Determinism:** `Ir` is callgrind instructions-retired — deterministic
  run-to-run on the same binary+input. Two back-to-back full runs produced
  BYTE-IDENTICAL `Ir` for all 10 benches. That determinism is what makes this a
  judge on this Windows dev host (wall-clock is noise; `Ir` is not).

## Baseline (Ir per bench function)

| bench function              |         Ir |
| --------------------------- | ---------: |
| small_churn_16b             |     81,170 |
| aligned_churn_640b_a128     |     81,049 |
| large_alloc_free_cycle      |     72,345 |
| realloc_grow                |  1,521,067 |
| cold_alloc_free_256x16b     |    129,863 |
| cold_alloc_free_256x64b     |    129,373 |
| recycle_alloc_free_256x16b  |    182,150 |
| recycle_alloc_free_256x64b  |    181,678 |
| churn_256b                  |     81,045 |
| churn_write_256b            |     81,173 |
