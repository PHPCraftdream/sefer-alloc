# R19 PRF-03 candidate B (retain the Large route while cached) vs A - gate run 01

**Measurement-only (`bench`): no allocator source is changed by this report; candidate B stays on its branch and is NOT
merged. Verdict of the preregistered judge: INCONCLUSIVE; a GO cannot follow from this run** (10 of 27 iai cells fail
their frozen criterion, see below). Frozen protocol: `docs/perf/R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.md` including
amendments A1 and A2 (hashes in `R19_PRF03_LARGE_ROUTE_RETAIN_PREREG.sha256`), judge
`scripts/r19_prf03_large_route_retain_gate.mjs`, runner `scripts/r19_prf03_large_route_retain_run.mjs`.

## 1. What was measured

- Arms (clean committed trees, A an ancestor of B, one toolchain and environment digest for every process): A =
  `6904dced` (tree `146afdd7`), B = `217c5339` (tree `13e11ce1`, B = A + the PRF-03 candidate). Identities, binary
  SHA-256 and the retained-run hashes are in `R19_PRF03_B_run01/MANIFEST.txt`; they were captured before the first
  build and every observation started after the last capture.
- Entry points: iai cells run the `HeapCore` alloc/dealloc body through a registry lease (`production bench-internals
  internals`, no alloc-stats), the layer the preregistration names for the Ir axis; the native axis runs the real
  installed `#[global_allocator] SeferAlloc` (features exactly `production`), N = 1/4/8 threads, 9 fresh processes per
  arm A / independent A/A repeat / B per N (81 processes), each preceded by its own diagnostic twin that reads the
  resolved cache configuration (A2 item 6).
- Path-activation oracles (A2 items 4-5, primary and alloc-stats builds of the same source, outside the timed bodies):
  R1 positive in both arms shows slot occupancy 1 -> 0 -> 1, tail reserve/release 0/0, tail pointer equal to the last
  prefix pointer, bracketed incarnation delta A = 1 / B = 0, Large route census after the tail free A = 0 / B = 1, one
  paired cache hit; the 12 MiB negative control shows the miss signature (occupancy 1 -> 1 -> 2, reserve +1, zero hits)
  in both arms; R2 shows 33 reserves and 33 releases over the tail with 8 slots before and after and 8 hits (census
  A = 0 / B = 8); R3 shows the same root, 8 changed payloads, 0/0 reserve/release and one hit (census A = 0 / B = 1).
  Routed-miss scans are 0 in every paired run, equal across arms. Sidecar drain/rescue scans are NOT observed by this
  gate (reported as unobserved, never as zero). Table: `R19_PRF03_B_run01/oracle_summary.csv`.
- 358 retained processes (iai group runs 150, oracle/probe 46, twin 81, native 81), all validated by the runner
  aggregate; the feature-exact/inventory receipt is the personally reviewed `R19_PRF03_B_run01/review_receipt.json`.

## 2. Instruction counts (iai, deterministic)

`T = max(10, |C0 - A1|)` per cell; every cross pair of {A1, A2} x {B1, B2} must pass (A1 = A2 and B1 = B2 in every
cell, so each row shows one value per cross pair). R1: B - A <= -200; R2/R3: B - A <= T; controls: |B - A| <= T;
mimalloc: B - A = 0; affected cells report deltas only.

| cell | kind | A1 | A2 | B1 | B2 | C0 | T | B-A (4 cross pairs) | pass |
|---|---|---:|---:|---:|---:|---:|---:|---|---|
| `small_churn_16b` | control | 56640 | 56640 | 56696 | 56696 | 56640 | 10 | 56 | **NO** |
| `alloc_magazine_prefill_only_16b` | control | 55916 | 55916 | 56029 | 56029 | 55916 | 10 | 113 | **NO** |
| `alloc_magazine_hit_only_16b` | control | 56274 | 56274 | 56374 | 56374 | 56274 | 10 | 100 | **NO** |
| `large_alloc_free_cycle` | affected | 49920 | 49920 | 49782 | 49782 | 49920 | 10 | -138 | yes |
| `large_cache_hit_only_4mib` | R1 | 1259 | 1259 | 824 | 824 | 1259 | 10 | -435 | yes |
| `large_cache_eviction_hit_only_41x4mib` | R2 | 109619 | 109619 | 109055 | 109055 | 109631 | 12 | -564 | yes |
| `large_cache_payload_hit_only_4mib` | R3 | 1287 | 1287 | 1438 | 1438 | 1299 | 12 | 151 | **NO** |
| `large_cache_free_slot_search_prefill_only` | affected | 63358 | 63358 | 63572 | 63572 | 63358 | 10 | 214 | yes |
| `large_cache_free_slot_search_cycle_only` | affected | 74518 | 74518 | 70737 | 70737 | 74518 | 10 | -3781 | yes |
| `realloc_grow` | affected | 582505 | 582505 | 582232 | 582232 | 582505 | 10 | -273 | yes |
| `cold_alloc_only_256x16b` | control | 114383 | 114383 | 114438 | 114438 | 114383 | 10 | 55 | **NO** |
| `cold_alloc_only_256x16b_2n` | control | 185497 | 185497 | 185552 | 185552 | 185497 | 10 | 55 | **NO** |
| `cold_alloc_only_256x16b_4n` | control | 326165 | 326165 | 326220 | 326220 | 326165 | 10 | 55 | **NO** |
| `churn_256b` | control | 56640 | 56640 | 56708 | 56708 | 56640 | 10 | 68 | **NO** |
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
| `mimalloc_recycle_alloc_free_256x16b` | mimalloc | 53020 | 53020 | 53027 | 53027 | 53020 | 10 | 7 | **NO** |
| `mimalloc_recycle_alloc_free_256x64b` | mimalloc | 54024 | 54024 | 54031 | 54031 | 54024 | 10 | 7 | **NO** |
| `mimalloc_bootstrap_proxy` | mimalloc | 13050 | 13050 | 13050 | 13050 | 13050 | 10 | 0 | yes |

Findings:

- **Benefit (R1) is met**: `large_cache_hit_only_4mib` 1259 -> 824 Ir, B - A = -435 (435/1259 = 34.55 % fewer
  instructions in the hit-minus-prefill window), threshold -200. R2 (eviction-dominated, 33 of 41 frees evict) is
  better by 564 of 109619 Ir (0.51 %).
- **R3 fails**: the changed-payload pair costs +151 Ir (151/1287 = 11.73 %) against T = 12. A retained route cannot be
  reused when the payload offset changes, so B discards it and registers a fresh one; the function-level profile
  (section 4) shows the extra work in `register_payload_parked` and `unregister_impl` versus A's `register_payload`
  and `unregister`, about 17 Ir more per cycle over the 9 cycles of the body.
- **Seven `control` cells fail by +55 ... +113 Ir** against T = 10, and two **mimalloc** cells by +7 against the
  required exact zero. The function-level diff (section 4) attributes the sefer-cell deltas to libc
  `__memcpy_avx_unaligned_erms` (+48 Ir, a larger by-value copy; consistent with, but not separately verified as, the
  cache slot type growing by the route handle) and `__memset_avx2_unaligned_erms` (+7 Ir); the **same +7 memset appears in the mimalloc cells**, which
  execute none of B's code. That +7 is therefore an environment/alignment effect between two differently named
  worktrees (the C0 binary runs from A's directory and cannot see it), not allocator work, and it also bounds what the
  `control` and `mimalloc` rules can resolve on this harness: arms must be run from identically shaped paths in a
  follow-up run.

## 3. Native wall-clock (real `#[global_allocator]`, per-process medians, never pooled)

| n | metric | median A | median AA | median B | A/A noise (num/den) | limit | B vs A (num/den) | B<=A(1+noise) |
|---:|---|---:|---:|---:|---:|---:|---|---|
| 1 | nsPerOp | 177.3 | 174.3 | 144.0 | 1.68 % (3.0/177.3) | 5 % | -33.3/177.3 = -18.8 % | yes |
| 1 | p99Ns | 310 | 309 | 207 | 0.32 % (1/310) | 5 % | -103/310 = -33.2 % | yes |
| 4 | nsPerOp | 155.7 | 138.2 | 115.0 | 11.25 % (17.5/155.7) | 5 % | -40.7/155.7 = -26.2 % | yes |
| 4 | p99Ns | 721 | 720 | 309 | 0.14 % (1/721) | 5 % | -412/721 = -57.1 % | yes |
| 8 | nsPerOp | 128.2 | 139.2 | 104.5 | 8.60 % (11.0/128.2) | 5 % | -23.7/128.2 = -18.5 % | yes |
| 8 | p99Ns | 1133 | 1236 | 309 | 9.09 % (103/1133) | 5 % | -824/1133 = -72.7 % | yes |

iai cells failing their frozen criterion: 10/27 = 37.04 %; native axes with A/A noise above 5 %: 3/6 = 50.00 %.

Verdict rule: noise = |median(AA) - median(A)| / median(A) must not exceed 5 % on any axis, otherwise INCONCLUSIVE and
no GO. Three of the six axes (n = 4 ns/op 11.25 %, n = 8 ns/op 8.60 %, n = 8 p99 9.09 %) exceed it, on a host that other
projects' sessions were loading between roughly 47 % and 99 % CPU during the run. The direction of every axis is in B's
favour (ns/op -18.8 / -26.2 / -18.5 %, p99 -33.2 / -57.1 / -72.7 % at n = 1 / 4 / 8) and the three quiet axes pass by
a wide margin, but under the frozen rules this is NOT reported as a speedup, and the p99 improvement at n = 8 sits on
an axis whose own A/A repeat moved by 9.09 %.

## 4. Function-level attribution (diagnostic, not a gate input)

`R19_PRF03_B_run01/diagnostic_function_diff.txt`, produced by `scripts/r19_prf03_cg_diff.mjs` from the retained
Callgrind profiles (exclusive Ir per function, A1 vs B1; the sum of per-function deltas is asserted equal to the total
delta). It explains the small-class control deltas and the R3 delta quoted above and is the evidence behind the
statement that the mimalloc +7 is not B's code.

## 5. What this run does and does not establish

- Established: the path-activation oracles are consistent in both arms, the R1 route-retention benefit is real and
  large in instructions (-34.55 % of the R1 window), and the native direction favours B at every N.
- Not established: any GO. Ten of 27 iai cells fail their frozen criteria and half of the native axes are
  INCONCLUSIVE; with these results the judge could only output NO-GO or INCONCLUSIVE.
- Candidate B is therefore not merged. The remaining decisions (accept NO-GO, or revise the candidate and/or the
  harness and gate again) are recorded in `docs/perf/OPEN_ITEMS.md` item 86.
- Harness note for any rerun: run the arms from identically sized worktree paths; the native phase needs a quiet host.

## 6. Reproduction and retention

```text
node scripts/r19_prf03_large_route_retain_run.mjs --run OUTDIR --arm-a <clean A worktree> --arm-b <clean B worktree>
node scripts/r19_prf03_large_route_retain_run.mjs --aggregate OUTDIR --review-receipt review.json
node scripts/r19_prf03_gate_excerpt.mjs --run OUTDIR --judge judge_output.json --review review.json --out <dir>
```

The run directory (about 89 MB, of which `bundle.json` 2.6 MB and the iai audit output 12 MB) is tier 3 under the
artifact-storage policy and is not committed; `MANIFEST.txt` carries its content hashes. Committed: the summary CSV
tables derived by `scripts/r19_prf03_gate_excerpt.mjs` (every number in sections 2 and 3 is recomputed there and
asserted against the judge's own output), the verbatim judge output, the review receipt, `input.json.gz` (the exact
judge input, 460803 bytes uncompressed) and the diagnostic function diff.

Post-collection note (no data was changed): the first aggregate attempt stopped on a strict ordering check between the
Linux (WSL) in-process clock and the Windows host clock (measured offset -0.27 .. +0.34 s over the 358 processes); the
aggregate now takes the ordering against the captures from the host receipt and allows +-1 s for the in-process clock.
The runner source is retained inside the run bundle exactly as it was at measurement time.
