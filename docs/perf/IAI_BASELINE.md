# iai-callgrind Ir baseline

Deterministic instruction-count (`Ir`) baseline for `benches/perf_gate_iai.rs`,
the reference future perf work (e.g. W4 `carve_batch`) diffs against.

- **Commit:** `4e139b0` (`4e139b00ffe7ce7c8f63d3a4a22c214c44d24e78`, `main`)
- **Features:** `production` (`alloc-global` + `alloc-xthread` + `alloc-decommit`
  + `fastbin`) — the same set the CI perf-gate benches with, so these numbers
  match `.github/workflows/perf-gate.yml`.
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
| small_churn_16b             |     81,229 |
| aligned_churn_640b_a128     |     81,108 |
| large_alloc_free_cycle      |     72,325 |
| realloc_grow                |  1,520,382 |
| cold_alloc_free_256x16b     |    130,099 |
| cold_alloc_free_256x64b     |    129,609 |
| recycle_alloc_free_256x16b  |    182,627 |
| recycle_alloc_free_256x64b  |    182,155 |
| churn_256b                  |     81,104 |
| churn_write_256b            |     81,232 |
