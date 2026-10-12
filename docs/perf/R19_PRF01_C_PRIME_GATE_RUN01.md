# R19 PRF-01 — sidecar summary C′ vs A, gate run01: rejected by the frozen Ir criteria (machine verdict INCOMPLETE)

Report class: **bench** (measurement and decision record). The candidate (`perf(runtime)` commit `6a8c9d97`,
arm B) was measured, **not accepted and not merged**. No production default changed by this report.

## 0. Headline

- **C′ is not accepted for merge: NO-GO by the frozen Ir criteria of two independent groups** — the hot-churn
  controls (Mode 1) and the producer price (Mode 2 (i)). The original machine verdict of the frozen judge is
  **INCOMPLETE** and is kept as such; the native axis did not reach a usable state.
- The benefit side is large and was met: multiseg_cold_256k B/A = 454,636 / 4,026,117 (Ir of arm B over Ir of
  arm A) = 0.1129; seg_cycle_decommit_256k B/A = 1,118,770 / 12,368,927 = 0.0905; growth16 slope of B =
  (Ir4n − Ir2n) / (Ir2n − Irn) = 35,782,898 / 16,687,226 = 2.1443 (limit ≤ 2.5).
- The cost side failed twice, both deterministic: every hot control moves by **+569 Ir** against
  T = max(10, |A1−A2|, |C0−A1|) = 10, and the producer pays (1,552,551 − 1,515,559) / 4096 remote Small frees =
  **+9.03125 Ir per free** against ≤ +6.
- Neither failure depends on the native axis, on the host being quiet, or on the clock; raising T or the +6 budget
  to admit the already-seen result is not done (arbiter ruling, see §6).

## 1. What was measured

Entry point: `SeferAlloc` as a `GlobalAlloc` → `HeapCore` (routed); Ir via the iai bench binary `perf_gate_iai`
(five arm labels A1, A2, B1, B2, C0; the pinned runner filters by bench file name, so the whole group of 106
functions ran per label and only the 24 frozen cells are retained as judge rows) and, for the native axis, a real installed `#[global_allocator]` example plus a same-source
twin carrying route/config readback. The twin transfers geometry to the native row; it is an inference from a
different binary, not independent proof inside the native binary.

| Item | Value |
|---|---|
| Arm A | clean tree of `main` `0fffe021ae3667b6010f6fe3c1990b7c033c4317` (tree `257b8828…`), identity `a7f18030…c1933` |
| Arm B | clean tree of `6a8c9d973f4e436383ec599dd00c98a6e5e9b7fd` (tree `b263cbbb…`), identity `0b56a0a7…ed1e`; differs from A by exactly the candidate commit |
| C0 | the A source built under a different path label (layout/alignment noise floor) |
| capture sha256 | `c7d733b9d97f1a1cd5bd83cd0de7fe582674d728b0744496251b85dc6ec2c726` |
| Host | WSL2 Linux 6.18.33 x86_64, i7-11800H, 8 cores, rustc 1.98.1, valgrind 3.22.0 (callgrind-3.22.0), iai-callgrind 0.14.2 |
| Evidence entries | 380 processes: 120 iai (24 cells × 5 labels), 5 producer controls, 216 native and twin, 39 oracle; hashed in `R19_PRF01_C_PRIME_RUN01/MANIFEST.txt` |

Every number below is derived by `scripts/r19_prf01_gate_excerpt.mjs`, which recomputes it from the judge input and
asserts it equal to the judge's own saved table row before writing it. Ir is deterministic: A1 = A2 = C0 and
B1 = B2 bit for bit in all 24 cells.

## 2. Frozen criteria

| criterion | operands | value | frozen limit | pass |
|---|---|---|---|---|
| multiseg_cold_256k B/A | 454636/4026117 | 0.112922 | ≤ 0.25 | yes |
| seg_cycle_decommit_256k B/A | 1118770/12368927 | 0.090450 | ≤ 0.25 | yes |
| growth16 slope B1 and B2 | 35782898/16687226 | 2.144329 | ≤ 2.5 | yes |
| hot control small_churn_16b, churn_256b, churn_write_256b, aligned_churn_640b_a128: \|B−A\| | +569 each vs T = 10 (4 cross pairs per cell) | 569 | ≤ T | **no (16 of 16 rows)** |
| mimalloc cells delta | 52 of 52 rows with delta 0 (13 cells × 4 pairs) | 0 | = 0 | yes |
| producer (IrB − IrA)/4096 | 36992/4096 | 9.03125 | ≤ 6 | **no (4 of 4 rows)** |
| `size_of::<SmallSidecar>()` B − A | 42496 − 41984 | 512 | = 512 | yes |
| oracle | A pending words 319,063 / 1,004,503 (multiseg / seg_cycle, frozen); B pending words 0 and summary words > 0 in the five benefit cells; producer B summary_load 4096, summary_rmw 64 | — | frozen | yes |

Full per-cell Ir, the 96 cross-pair rows and the oracle counters: `R19_PRF01_C_PRIME_RUN01/{iai_summary,iai_cross_pairs,criteria_summary,oracle_summary}.csv`
and `report_tables.md`. Publication (descriptive): 4,334,263 → 352,424 Ir, `discovery_records_reclaimed` 34 in both.

## 3. Where the cost comes from (diagnostic, not a criterion)

Function-level exclusive Ir diff of the retained A1/B1 profiles (`diagnostic_function_diff.txt`):

- Hot controls, +569 in each of the four cells = +512 `__memset_avx2_unaligned_erms` (the zero-initialisation of the
  512 extra sidecar bytes, counted per iteration by Callgrind) + 32 `RouteScan::next_cut` + 25
  `AllocCore::drain_segment_sidecar`. It is a fixed cost per sidecar initialisation, not a per-operation cost.
- Producer: the whole +36,992 sits in `RoutePin::publish_small` — 159,744 Ir in A (39.0 per free) against 196,736
  in B (48.03 per free) over the same 4096 frees; the new summary load (and its occasional RMW) is that +9.03.

These are the two numbers a redesign would have to move (smaller or lazily-initialised summary, cheaper per-free
summary touch) or that the owner would have to re-budget in a new protocol.

## 4. Native axis and why the judge stopped

The frozen judge requires every native `p50Ns`, `p99Ns` and `nsPerOp` to be strictly positive (judge line 213) and
divides by the arm-A median (line 66 `ratio`, line 70 `median`). **13 of 108 native rows have `p50Ns == 0`** (arm
A 4, AA 7, B 2, all at n = 1; their `p99Ns` is 104–105 and `nsPerOp` 78–98). The per-sample timer deltas of a single
free are plausibly quantised to 0 or 100 ns by the WSL2 clocksource; that mechanism is **inferred**, the raw
per-sample deltas and the clocksource were not recorded, so it is not proved here. The judge therefore returned
**INCOMPLETE for all modes** before evaluating the native axis, keeping the Ir tables it had already computed
(`judge_output.json`, `tables.iai`, `tables.slope`, `tables.oracle`, `tables.size`; native table empty).

Descriptive native medians (processes per arm and n = 9; not judged; `native_summary.csv`) are wide: for example
n = 8 arm A `nsPerOp` ranges 280–1019 over 18 A+AA processes, so even a defined p50 would likely have ended
INCONCLUSIVE. This is a harness limitation: the native schema did not allow for a zero elapsed per-sample value.

No Amendment C (a judge fix that would allow a non-negative p50 and mark a zero-median comparison as not
evaluable) was made: the rejection above does not need it, and it would be a post-hoc re-evaluation after seeing the
Ir results. If machine mode verdicts are wanted for the record, the arbiter's conditions are in §6.

## 5. Process record and disclosures

1. Preflight chain a01 → b01 → b02 → b03: see `R19_PRF01_PRODUCER_PREFLIGHT_B03.md` (one Callgrind launch consumed,
   Amendment B coverage shown achievable: control overhead 1 Ir, inside the frozen 1..16).
2. Harness/runner committed as `2a0671d7`; the preflight report as `0fffe021`; arms were created from that tree and
   the candidate cherry-picked onto it (`6a8c9d97`; its src/tests/scripts/ARCHITECTURE diff hash equals the previously
   verified candidate commit `856580e7`; only README counters differ). On the new base: 32 doc-reference tests, the
   discovery tests, the oracle-cfg tests, both clippy rows and the dbg-hook verifier were green.
3. Independent pre-measurement identity review (`identity_review_before_samples.json`) → **one** fresh full run
   (`run-start.json`, no retries; arms A, B, C0 and the 216 native processes in the frozen schedule).
4. **Post-measurement runner fix (disclosed):** the run's own aggregation step failed ~4 s after `bundle.json` was
   written (`run_stage_error_first_attempt.json`): it re-parsed the retained dispatcher-source stdout as JSON after the
   public path redaction had corrupted a string value starting with `//`. All measurements were already complete and
   retained. The fix reads that JSON from the hash-chained private raw stdout of the same process (`rawHashes.stdout`
   must match), with a synthetic fixture that reproduces the corruption; no measurement, selection or threshold was
   touched. The aggregation with the fixed runner then validated all 380 evidence entries and the linkage, and ended at
   the missing independent review (`review_receipt.json`, produced after re-deriving feature sets, producer profiles,
   oracle counters, inventory and segment-count semantics from the raw rows).
5. The runner self-test is 140 synthetic checks and covers every runner defect found along the way (four in the
   preflight, one in aggregation).
6. Metadata caveats: the environment receipt's iai-runner version probe holds an error text ("No version
   information found for iai-callgrind…") and `valgrindVersion` is filled from the whole environment text; the real
   versions are established by the profile headers (callgrind-3.22.0) and the runner-source receipt (0.14.2). The
   count "380" is the number of evidence entries; the whole iai group (106 functions) ran per label, the judge decision rows are the 24 frozen cells.
7. Not verified here: the exact mechanism of the zero p50 (§4); the corrected-judge machine modes.

## 6. Arbiter ruling (reviewer role, session q-prf01-native) and recording

- C′ in the measured version is not merged. Three levels are recorded separately: original machine verdict
  **INCOMPLETE** (all modes); unchanged-Ir evaluation **mode1 NO-GO, mode2Iai NO-GO**; native axis **not evaluable**.
  If an Amendment C were approved and run as a clearly labelled post-hoc re-evaluation, the expected modes are
  `{mode1: NO-GO, mode2Iai: NO-GO, mode2Native: INCONCLUSIVE}` and the global verdict would be INCONCLUSIVE, not
  NO-GO, because the judge gives `noisy` precedence (judge lines 223–224). Such an amendment must not weaken the
  global `positive()`/`ratio()`, must also fix `median()`, must keep zeros out of denominators, and needs an
  independent diff review and a separate artefact bound to the old input.
- T = 10 and the +6 budget are not raised to accept an already-seen result; new budgets are an owner decision for a
  future protocol. A quiet native host would not remove either Ir failure.
- Branches `r19-prf01-impl` and `r19-prf01-gate-b` are kept as evidence anchors; the measured commit is not rewritten.

## 7. Open question for the owner (HUMAN)

Keep C′ rejected and either (a) redesign to remove the fixed +512 B zero-init cost and the ~9 Ir per remote free
(benefit side has large margin: 0.09–0.11 against 0.25), or (b) open a new preregistered protocol with owner-set
budgets for the hot-control constant and the producer price, replacing the per-sample native timer by a batched one.
