# R20 PRF-01 sidecar-summary candidate C'' — official gate run (v2 protocol), verdict NO-GO

Measurement-only (`bench`): no shipping code changed. Candidate C'' is not merged.

## 0. Verdict

**NO-GO** by the frozen v2 judge (`scripts/r20_prf01_sidecar_summary_gate.mjs`, exit 1). The benefit cells
pass with large margin; two deterministic price checks fail. The native axis (Mode 2 (ii)) is DEFERRED by
protocol, so GO was unreachable in v2 regardless (best possible: `INCONCLUSIVE (native deferred)`).

| Check | Result | Numbers (numerator / denominator inline) |
|---|---|---|
| Mode 1 `multiseg_cold_256k` B/A ≤ 0.25 | pass | IrB 382 467 / IrA 4 022 898 = 0.095 |
| Mode 1 `seg_cycle_decommit_256k` B/A ≤ 0.25 | pass | IrB 895 801 / IrA 12 364 116 = 0.072 |
| Mode 1 `growth16` slope of B ≤ 2.5 | pass | 34 394 066 / 16 336 874 = 2.105 |
| Hot families (4 families × N/2N/4N), two-sided slope ≤ T_s = 10 | pass | slope delta 0 in every judged family |
| **Mode 2 (i) producer price ≤ +6 Ir per remote free** | **FAIL** | (IrB 1 577 127 − IrA 1 523 751) = +53 376 Ir / 4096 frees = **+13.03 Ir per free** |
| **Construction cell `r20_prf01_start_one_alloc_free_16b`, budget_K 0 + T_s 10** | **FAIL** | IrB 49 997 − IrA 49 356 = **+641 Ir** against 0 + 10 (noise ceiling 246 not exceeded: AA 0, C0 0) |
| sizeOf (sidecar bytes) | pass | 42 496 − 41 984 = 512 = expected 512 |
| Mode 2 (ii) native | DEFERRED | by protocol (batch-timer method, bare-metal host not available in CI) |

The producer price is +13.03 against a budget of +6 that the protocol says is not raised; R19 run01 measured
+9.03 for C' (36 992 / 4096). The construction cost (+641 Ir, fixed per instance, workload independent) has the
same shape as the R19 +569 Ir sidecar construction cost; the owner budget for a net construction cost is 0.

## 1. Provenance

- Dispatch: `r20-gates.yml` on `main` @ `09e05067461b12bcc5c0de65baeacc3bb8359343`,
  `runner=scripts/r20_prf01_sidecar_summary_run.mjs stages=prepare,build,oracle,measure slop=false`.
  GitHub Actions run 38431619600 (artifact `r20-gates-38431619600`, ~13.6 MB, GitHub retention applies).
- An earlier dispatch (run 38424122185, main `91aa0e9a`) aborted in `build` before any measurement
  (cleanliness check counted the stage's own out-dir); fixed and re-frozen as "re-freeze 10" in
  `R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md` before this run. No data existed in the aborted run.
- Candidate: requested `edf90675` (branch `r20-prf01-cdp-main`), picked onto the runner base as
  `5a6019f95986e31802653697083bf39987b4da5d` (see `manifest.json`: `pickRequested`, `pickedCommit`, `arches`).
- Acceptance: `node scripts/r20_prf01_verify_artifacts.mjs <out-dir>` on the downloaded artifact:
  PASS (84/84 anchored files verified, 403 receipt fields, ancestry chain agrees with `manifest.json`, binary
  pins agree with `builds.json`).
- Judge verdict file sha256 `789d727a9f072f951759ae45e76b7f09408a85e6bd322eaddfb2c83512d575ee`
  (`verdict.txt`), `summary.csv` sha256 `0c03931189a8bdf9d68d1abe6647dd8d7fe7bbeec6dd39f9dbffc844ea80cde0`.

## 2. What is committed

`docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01/`: `verdict.txt`, `summary.csv`, `manifest.json`, `builds.json`,
`input.json.gz`, `oracle.json.gz` (gzip of the judge input and the oracle record, tier-2 of the artifact
policy). Not committed (tier 3, ~95 MB): `boundary/`, `raw/`, `logs/`, `profiles/`, `oracle/` — they are
anchored by `input.json`'s `receiptHashes` and retrievable from the Actions artifact. `acceptance` over a
re-extracted `input.json` needs the full artifact (it re-hashes the anchored files).

## 3. Reading and non-claims

- Oracle checks passed in the judged rows (B visits no pending words and reads the summary; A counts the frozen
  319 063 / 1 004 503 pending words; equal `routed_scans`), so the Ir deltas above belong to the intended
  mechanism. `summary_load == 0` on B remains a source-derived expectation, not an observation.
- Deterministic Ir only; this run says nothing about native latency or cross-core contention.
- The result does not weaken the benefit finding: an order-of-magnitude Ir reduction on the cold multi-segment
  and decommit cycles. It rejects C'' as specified: its producer path costs more than the frozen price and the
  sidecar adds a fixed per-instance construction cost.

## 4. Next steps (owner decision)

1. Redesign the producer-side transition so the extra cost per remote free is ≤ +6 Ir and construction is
   net 0 (where the +641 comes from is not decomposed by this run: `profiles/` and `construction` profiles in the
   artifact are the starting point), then a new preregistered gate; or
2. Re-open the budgets with a stated principle (the arbiter rejected a post-hoc 3 % budget on 2026-10-12);
3. Keep PRF-01 closed as NO-GO with this evidence.

Index: `docs/perf/OPEN_ITEMS.md` item 82 status updated in the same commit.
