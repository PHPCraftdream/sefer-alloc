# Source review round 12 (fxx): remediation manifest

Review report: `docs/reviews/2026-10-06-063308-src-review-fxx-round-12.md`
(committed as `d74fe5e5`, docs-only). Counts: P0=0 / P1=0 / P2=1 / P3=4 / P4=5.
The review is source-only; no release GO is implied.

## 1. Commits in the round

Derived, not hand-transcribed: `git log --reverse --format='%h|%s' d74fe5e5^..5ee118d2`
(category = the subject prefix, R30-12 taxonomy; the closing docs commit that adds this
file follows the range).

| commit | category | subject |
|---|---|---|
| `d74fe5e5` | `docs` | add independent src review round 12 (fxx) |
| `c4725071` | `fix` | yield to the scheduler in the fallback-heap init wait loop (round 12 R12-04) |
| `ea47a10c` | `build` | keep tagged-index-stack out of the production graph and fix the unsafe-seam inventory (round 12 R12-06/07/08) |
| `0228d150` | `fix` | drop foreign frees of never-issued sidecar granules before publication (round 12 R12-01) |
| `7232598b` | `perf(runtime)` | use the segment-base mask in clear_magazine_on_issue (round 12 R12-02) |
| `3cf802e6` | `test` | pin routed directory-miss behaviour and refresh stale directory/bootstrap docs (round 12 R12-03/09/10) |
| `a4b0ff05` | `checkpoint` | 2026-10-06-1026 |
| `2600b337` | `perf(opt-in)` | bound the Large-ingress probe on alloc_batch misses (round 12 R12-05) |
| `7d3b3e84` | `test` | drop the unused owner address in the R12-01 test (clippy --all-features row) |
| `516358fa` | `test` | move the Large-phase state-machine tests out of src/ and drop a stale dead_code allow (round 12 R12-10) |
| `5ee118d2` | `bench` | count routed negative-directory miss-scan outcomes (round 12 O-4, data only) |

## 2. Net `production` default-feature impact

Feature composition of `production`: **unchanged** (no feature added or removed).
Dependency graph: `tagged-index-stack` left the `alloc-global` closure (R12-08, build
change only; it remains a `cfg(any(loom, kani))` target dependency). Source change visible
to `production`: R12-02 (hot path, `perf(runtime)`), R12-01 (cross-thread free publication
guard), R12-04 (cold init loop). R12-05 touches only `batch-api` (`perf(opt-in)`); the one
`production`-visible edit there is the visibility of a private helper (`pub(super)`), no
codegen change.

## 3. Measured deltas

- R12-02 (`perf(runtime)`, deterministic callgrind `Ir`, no wall-clock claim): per-hit clear cost
  22.5625 -> 12.1875 Ir (361/16 -> 195/16 Ir over MAGAZINE_FILL=16); `small_churn_16b`
  58,201 -> 57,571 Ir (-630, -1.082%); layout-noise control max 12 Ir.
  `docs/perf/R12_02_MAGAZINE_MASK_GATE.md`.
- R12-05 (`perf(opt-in)`, scan counts only, no latency claim): 64 active Large routes ->
  4 inspections per Small batch miss (was 64); 9 routes -> 4 per Large batch (was 9).
  `docs/perf/R12_05_BATCH_LARGE_BOUNDED_GATE.md`.
- No other commit in the round claims a runtime/RSS number.

## 4. Verdict per item

| item | verdict |
|---|---|
| R12-01 (P2) foreign free of a never-issued granule aborts the owner | SHIPPED, partial; interior-pointer residual = correctness item 166 |
| R12-02 (P3) + O-1 magazine-hit lookup | SHIPPED (GO on the Ir axis) |
| R12-03 (P3) + O-4 trusted-negative directory | CORRECTION (docs + characterization test) and INFRASTRUCTURE (counters); trusting the negative = NO-GO for now, perf item 82 |
| R12-04 (P3) + O-5 init-loser spin | SHIPPED |
| R12-05 (P3) + O-2 batch Large sweep | SHIPPED (opt-in); realloc keeps the full sweep |
| R12-06 / R12-07 / R12-08 (P4) seam inventory, comments, dead dependency | CORRECTION; loom/kani model residual = correctness item 167 |
| R12-09 (P4) slot zero-state docs | CORRECTION |
| R12-10 (P4) inline test modules, stale `allow(dead_code)` | CORRECTION |
| O-3 (granule-issued check in `publish_small`) | folded into R12-01; no separate cross-thread-free A/B was measured |

## 5. Raw logs committed this round

7 files, 253,171 bytes in total (largest 52,060 bytes, all under the 200 KiB
per-file ceiling): `docs/perf/_raw_r12_02_{A1,A2,B1,B2}.log`,
`_raw_r12_02_C0_control_unpatched_other_path.log`, `_raw_r12_05_batch_counter.log`,
`_raw_r12_05_mutant_full_drain.log`.
