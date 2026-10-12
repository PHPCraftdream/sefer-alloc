# R19 PRF-03 candidate B2 (compact retained Large route) vs A - gate run 02

**Measurement-only (`bench`): no allocator source is changed by this report; candidate B2 stays on its branch
`r19-prf03-b2` and is NOT merged. Judge verdict: INCONCLUSIVE (native A/A noise above 5 % on 4 of 6 axes); the iai
axis fails the frozen criteria independently of that noise (R3 and the seven control cells), so a GO is impossible.**
Protocol: `docs/perf/R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.md` with amendments A1, A2 and A3 (hashes renewed in A3,
criteria unchanged), judge `scripts/r19_prf03_large_route_retain_gate.mjs`, runner
`scripts/r19_prf03_large_route_retain_run.mjs` (A3 harness). First run of the same protocol: `R19_PRF03_B_VS_A_GATE.md`
(candidate B, unchanged and not replaced).

## 1. What was measured

- Arms (clean committed trees, A an ancestor of B2, equal-length full paths `worktrees/prf03-arm-c` /
  `worktrees/prf03-arm-d`, one-character build labels, one toolchain and environment digest): A = `7e7da281` (tree
  `6b211f6d`), B2 = `82992739` (tree `c1888374`, B2 = A + the B candidate + the B2 compaction). Identities, binary
  SHA-256 and run hashes: `R19_PRF03_B2_run02/MANIFEST.txt`.
- Entry points as in run 01: iai cells run the `HeapCore` body through a registry lease, the native axis the real
  installed `#[global_allocator] SeferAlloc` (N = 1/4/8, nine fresh processes per arm A / A/A / B2 each preceded by its
  own diagnostic twin).
- Path-activation oracles (primary and alloc-stats builds of the same source, outside the timed bodies) are consistent
  in both arms exactly as in run 01: R1 slots 1 -> 0 -> 1 with tail reserve/release 0/0 and incarnation delta A = 1 /
  B2 = 0, census A = 0 / B2 = 1; the 12 MiB negative control shows the miss signature in both arms; R2 33/33 with 8 slots
  before and after (census A = 0 / B2 = 8); R3 same root, 8 payload changes, 0/0. Routed-miss scans 0, equal across
  arms. Table: `R19_PRF03_B2_run02/oracle_summary.csv`; personally reviewed receipt: `review_receipt.json`.
- 34 load samples were retained (A3 telemetry): host CPU load 12 .. 82 % over the run and 12 .. 63 % before the 27
  native triples (other projects' sessions and this repository's own rush sessions were running); WSL load average
  1.1 .. 2.7.

## 2. Instruction counts (iai, deterministic)

`T = max(10, |C0 - A1|)`; every cross pair of {A1, A2} x {B1, B2} must pass; R1: B - A <= -200; R2/R3: B - A <= T;
controls: |B - A| <= T; mimalloc: B - A = 0 (A1 = A2 and B1 = B2 in every cell, one value per row below).

| cell | kind | A1 | A2 | B1 | B2 | C0 | T | B-A (4 cross pairs) | pass |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| `small_churn_16b` | control | 56696 | 56696 | 56582 | 56582 | 56696 | 10 | -114 | **NO** |
| `alloc_magazine_prefill_only_16b` | control | 55973 | 55973 | 55863 | 55863 | 55973 | 10 | -110 | **NO** |
| `alloc_magazine_hit_only_16b` | control | 56331 | 56331 | 56219 | 56219 | 56331 | 10 | -112 | **NO** |
| `large_alloc_free_cycle` | affected | 49907 | 49907 | 49581 | 49581 | 49907 | 10 | -326 | yes |
| `large_cache_hit_only_4mib` | R1 | 1253 | 1253 | 760 | 760 | 1253 | 10 | -493 | yes |
| `large_cache_eviction_hit_only_41x4mib` | R2 | 109373 | 109373 | 108192 | 108192 | 109373 | 10 | -1181 | yes |
| `large_cache_payload_hit_only_4mib` | R3 | 1281 | 1281 | 1400 | 1400 | 1281 | 10 | 119 | **NO** |
| `large_cache_free_slot_search_prefill_only` | affected | 63304 | 63304 | 63220 | 63220 | 63304 | 10 | -84 | yes |
| `large_cache_free_slot_search_cycle_only` | affected | 74416 | 74416 | 69894 | 69894 | 74416 | 10 | -4522 | yes |
| `realloc_grow` | affected | 582915 | 582915 | 582406 | 582406 | 582915 | 10 | -509 | yes |
| `cold_alloc_only_256x16b` | control | 115384 | 115384 | 115277 | 115277 | 115384 | 10 | -107 | **NO** |
| `cold_alloc_only_256x16b_2n` | control | 187506 | 187506 | 187399 | 187399 | 187506 | 10 | -107 | **NO** |
| `cold_alloc_only_256x16b_4n` | control | 330190 | 330190 | 330083 | 330083 | 330190 | 10 | -107 | **NO** |
| `churn_256b` | control | 56696 | 56696 | 56582 | 56582 | 56696 | 10 | -114 | **NO** |
| `mimalloc_small_churn_16b` | mimalloc | 16629 | 16629 | 16629 | 16629 | 16629 | 10 | 0 | yes |
| `mimalloc_small_churn_16b_2n` | mimalloc | 21557 | 21557 | 21557 | 21557 | 21557 | 10 | 0 | yes |
| `mimalloc_churn_256b` | mimalloc | 16130 | 16130 | 16130 | 16130 | 16130 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x16b` | mimalloc | 32325 | 32325 | 32325 | 32325 | 32325 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x16b_2n` | mimalloc | 58389 | 58389 | 58389 | 58389 | 58389 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x16b_4n` | mimalloc | 106653 | 106653 | 106653 | 106653 | 106653 | 10 | 0 | yes |
| `mimalloc_cold_alloc_only_256x16b` | mimalloc | 24584 | 24584 | 24584 | 24584 | 24584 | 10 | 0 | yes |
| `mimalloc_cold_alloc_only_256x16b_2n` | mimalloc | 42968 | 42968 | 42968 | 42968 | 42968 | 10 | 0 | yes |
| `mimalloc_cold_alloc_only_256x16b_4n` | mimalloc | 75872 | 75872 | 75872 | 75872 | 75872 | 10 | 0 | yes |
| `mimalloc_cold_alloc_free_256x64b` | mimalloc | 33329 | 33329 | 33329 | 33329 | 33329 | 10 | 0 | yes |
| `mimalloc_recycle_alloc_free_256x16b` | mimalloc | 53020 | 53020 | 53020 | 53020 | 53020 | 10 | 0 | yes |
| `mimalloc_recycle_alloc_free_256x64b` | mimalloc | 54024 | 54024 | 54024 | 54024 | 54024 | 10 | 0 | yes |
| `mimalloc_bootstrap_proxy` | mimalloc | 13050 | 13050 | 13050 | 13050 | 13050 | 10 | 0 | yes |

Findings:

- **The A3 harness correction works**: with equal-length paths the `mimalloc` cells are exactly 0 in all 13 cells
  (run 01 had +7 in two of them), which confirms the +7 `memset` was a path-shape/alignment confounder and not
  allocator work. Residual differences in sefer cells are therefore real code differences.
- **Benefit met**: R1 `large_cache_hit_only_4mib` 1253 -> 760 Ir (B2 - A = -493; 493/1253 = 39.3 % fewer instructions
  in the hit-minus-prefill window, threshold -200); R2 (eviction-dominated) -1181 of 109373 Ir (1.08 %).
- **R3 fails**: the changed-payload pair costs +119 Ir (119/1281 = 9.3 %) against T = 10. A retained route cannot be
  reused when the payload offset changes, so it is discarded and registered afresh; the function-level profile shows the
  extra cost in `unregister_parking` and `RouteSlots::prepare` (the parking bookkeeping) that A does not have.
- **Controls fail in the opposite direction**: all seven small-class controls are CHEAPER by 107 ... 114 Ir
  (`__memcpy_avx_unaligned_erms` -101 and `AllocCore::new_with_config_and_owner` -13 per heap construction; fixed cost,
  not per-operation). The frozen control rule is two-sided (|B - A| <= T = 10), so an unrelated one-time saving of ~110
  Ir also fails it; this is a property of the criterion and is not reinterpreted here.
- Eight of 27 iai cells fail their frozen criterion: R3 and the seven controls (`iai_summary.csv` lists them); the other
  nineteen cells, including all thirteen mimalloc cells at exactly zero, pass.

## 3. Native wall-clock (real `#[global_allocator]`, per-process medians, never pooled)

| n | metric | median A | median AA | median B | A/A noise (num/den) | limit | B vs A (num/den) | B<=A(1+noise) |
|---:|---|---:|---:|---:|---:|---:|---|---|
| 1 | nsPerOp | 169.8 | 202.7 | 166.4 | 19.38 % (32.9/169.8) | 5 % | -3.4/169.8 = -2.0 % | yes |
| 1 | p99Ns | 207 | 310 | 207 | 49.76 % (103/207) | 5 % | 0/207 = 0.0 % | yes |
| 4 | nsPerOp | 128.7 | 116.2 | 85.0 | 9.72 % (12.5/128.7) | 5 % | -43.7/128.7 = -33.9 % | yes |
| 4 | p99Ns | 517 | 517 | 207 | 0.00 % (0/517) | 5 % | -310/517 = -60.0 % | yes |
| 8 | nsPerOp | 143.0 | 146.7 | 81.0 | 2.60 % (3.7/143.0) | 5 % | -62.0/143.0 = -43.4 % | yes |
| 8 | p99Ns | 1447 | 1551 | 207 | 7.19 % (104/1447) | 5 % | -1240/1447 = -85.7 % | yes |

iai cells failing their frozen criterion: 8/27 = 29.63 %; native axes with A/A noise above 5 %: 4/6 = 66.67 %.

Four of six axes have A/A noise above the frozen 5 % (n = 1 ns/op 19.38 %, n = 1 p99 49.76 %, n = 4 ns/op 9.72 %,
n = 8 p99 7.19 %), so the verdict rule gives INCONCLUSIVE and no speedup is claimed. The direction of every axis is in
B2's favour, and at n = 4 and n = 8 the improvement is large (ns/op -33.9 % and -43.4 %, p99 -60.0 % and -85.7 %), but
the n = 8 p99 axis moved by 7.19 % between two independent runs of the same A binary, and the n = 1 A/A repeat differs
from A by 19.38 % / 49.76 %, i.e. this host did not provide the quiet window the protocol requires.

## 4. What the two runs establish

- Established across run 01 (candidate B) and run 02 (candidate B2): retaining the Large route while the block is cached
  removes about 35-39 % of the instructions of a cache hit, the benefit is oracle-verified, and the native direction
  favours the candidate at every N; the compaction of B2 removed the +48 Ir fixed construction cost of B.
- Not established: any GO. Under the frozen criteria the changed-payload regime regresses by +119 .. +151 Ir per nine
  cycles (about 13-17 Ir per cycle) for both candidates, because the parked route must be discarded and re-created;
  the compaction narrowed but could not remove it, and the two-sided control rule fails on an unrelated one-time saving.
- Candidate B2 is therefore not merged. Whether the owner wishes to re-scope the R3 and control criteria (a new
  preregistered protocol version, never a retroactive threshold fit) is recorded in `docs/perf/OPEN_ITEMS.md` item 86.

## 5. Reproduction and retention

```text
node scripts/r19_prf03_large_route_retain_run.mjs --run OUTDIR --arm-a <A worktree> --arm-b <B2 worktree>
node scripts/r19_prf03_large_route_retain_run.mjs --aggregate OUTDIR --review-receipt review.json
node scripts/r19_prf03_gate_excerpt.mjs --run OUTDIR --judge judge_output.json --review review.json --out <dir>
```

The run directory (tier 3, about 90 MB) is not committed; `MANIFEST.txt` carries its hashes. Committed: the summary CSV
tables (recomputed and asserted against the judge output by `scripts/r19_prf03_gate_excerpt.mjs`), the verbatim judge
output, the review receipt, `input.json.gz` and the function-level diagnostic diff.
