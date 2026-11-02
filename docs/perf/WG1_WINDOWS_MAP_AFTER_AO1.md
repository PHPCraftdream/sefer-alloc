# WG-1 step 1 — Windows sefer / mimalloc map on `main` after AO-1 and WILD-2 (`bench`, measurement-only, no code, no default changed)

Measurement-only: no `perf(runtime)` or `perf(opt-in)` change. It refreshes the Windows judge half of `LM1_LOSS_MAP.md`
(taken on `main` `c495edc9`, before AO-1 and WILD-2) so the next lever for the growth and wild shapes is chosen from the
current state.

**Entry point under test:** a static `SeferAlloc` instance reached through `#[inline(never)]` `GlobalAlloc` shims at the
`__rust_alloc` boundary (header of `examples/g1_wild_judge.rs`), arms `sefer` and `mi` in one binary, one process per run, the
process's own global allocator stays `System`. It is not the process's registered `#[global_allocator]`; it is the same
`GlobalAlloc` methods on the same allocator type, which is the layer the changes since LM-1 ship at.

## Design

* Source identity: `main` `338875ab93398a379857059e4f487b93317b43c8`, judge binary hash in `docs/perf/wg1/ident.txt`
  (`--release --features production`; LM-1 used `production internals`, which adds diagnostics only).
* Seven scenarios, scale 1, seed 1; 12 blocks; per block and arm the minimum `ns_per_op` over 3 back-to-back repeats;
  scenario order shuffled per block with a fixed seed; the arm order alternates by block (`docs/perf/wg1/run.mjs`).
* Per block `d = ln(sefer / mi)`; the cell is `exp(mean d)` with a two-sided 95 % t-interval (n − 1 = 11 degrees of freedom,
  quantile 2.201). There is no second copy of the sefer binary in this run, so there is no A/A noise floor; the LM-1 column is a
  separate session on a different binary and is context, not a paired comparison.

## Result (`node scripts/wg1_map_report.mjs`, derived from `docs/perf/wg1/win_raw.jsonl`)

| Scenario | sefer / mimalloc on `main` now (exp mean d) | 95 % CI | LM-1 reading before AO-1 and WILD-2 | LM-1 95 % CI | blocks x repeats |
|---|---:|---|---:|---|---|
| grow64 | 1.212 | 1.186–1.239 | 1.730 | 1.678–1.784 | 12 x 3 |
| grow1k | 1.090 | 1.039–1.144 | 1.490 | 1.412–1.573 | 12 x 3 |
| regrow | 1.352 | 1.282–1.425 | 1.412 | 1.326–1.504 | 12 x 3 |
| growmix | 1.328 | 1.253–1.408 | 1.300 | 1.234–1.368 | 12 x 3 |
| wild | 1.405 | 1.337–1.477 | 1.520 | 1.439–1.605 | 12 x 3 |
| phase | 1.141 | 1.101–1.183 | 1.143 | 1.099–1.188 | 12 x 3 |
| large | 1.223 | 1.082–1.383 | 1.278 | 1.182–1.381 | 12 x 3 |

## Reading

* **Sefer is still slower than mimalloc in all seven scenarios in the judge window** (every interval is above 1.0).
* **grow64 and grow1k moved most** (1.730 → 1.212 and 1.490 → 1.090), as the AO-1 campaign predicted (AO-1 alone: 0.750 and
  0.770 of the base, `AO1_ASCENDING_ISSUE.md`). grow1k is now within 9 % of mimalloc.
* **`wild` moved little** (1.520 → 1.405; the intervals touch at 1.439–1.477), in the direction WILD-2 measured on Linux and
  far smaller than the gap itself. **regrow, growmix, phase and large did not move** within their intervals.
* **The largest remaining gaps are `wild` 1.405, `regrow` 1.352 and `growmix` 1.328.** Their attributions already on record:
  `wild`/`phase` — the free of small blocks inside a heap with many live segments (memo misses on 88.8 % of the lane
  frees, `WILD1_GAP_DECOMPOSITION.md`); `growmix` — cache misses on segment metadata in the mixed free (`GM-1`); `regrow` — the
  release of emptied segments in `free`, whose retention policy was measured at process level in
  `RET1_RETAIN_PROCESS_LEVEL.md` (3–5 % at pool 1024, so `Retain` is not the lever).

## Limits

* One session, shared machine, no A/A in this run; the intervals describe block-to-block spread of this session only.
* Judge window only; the whole-process view of `RET1_RETAIN_PROCESS_LEVEL.md` shows the window flatters a retaining allocator
  by an amount that depends on how much the process still holds at exit.
* Windows only; Linux is not refreshed here.

## Identity and reproduction

* `docs/perf/wg1/ident.txt` (source SHA, binary hash), raw per-run data `docs/perf/wg1/win_raw.jsonl`, summary
  `docs/perf/WG1_WINDOWS_MAP_AFTER_AO1_summary.csv`
  (`node scripts/wg1_map_report.mjs --csv docs/perf/WG1_WINDOWS_MAP_AFTER_AO1_summary.csv`).
* Re-run: `cargo build --release --example g1_wild_judge --features production`, then
  `node docs/perf/wg1/run.mjs <exe> <out.jsonl> 12 3`.
