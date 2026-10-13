# R20 PRF-01 sidecar summary C″ — v2 gate preregistration (document-delta over R19)

**Status: UNMEASURED for the gate.** No candidate Ir was ever taken under this protocol; no threshold
below was chosen or changed after seeing any C″ instruction count. This document is a DELTA over
`docs/perf/R19_PRF01_SIDECAR_SUMMARY_PREREG.md` (incl. Amendments A and B): everything not listed
under "Replaced" is inherited BY REFERENCE with unchanged thresholds, schemas and names. The R19
run01 verdict (C′ NO-GO: +569 Ir per hot control = one-time sidecar construction, and producer
+9.03125 Ir/free against ≤ +6) is **NOT recomputed or reinterpreted**. Report class: protocol only.

## Authority

- Owner decision 2026-10-12: control-geometry rule
  `docs/perf/R20_CONTROL_GEOMETRY_RULE.md` (frozen with the PRF-04 gate; its item 5 mandates the 2N/4N
  siblings this preregistration adds).
- Reviewer answers `docs/reviews/2026-10-12-r20-budgetk-and-rule3a-reviewer-answers.md`:
  (1) `budget_K = 0` Ir of NET work for the direct construction cell, tolerance = the qualified noise
  `T_s`, an unexplained gain is INVALID (never an automatic GO), and the first measurement question is
  whether the removed 4096-word `pending` pass actually disappeared; (2) tightened rule item 3a.
- R19 run01 record `docs/perf/R19_PRF01_C_PRIME_GATE_RUN01.md` (§3: +512 memset + 32 `next_cut` + 25
  `drain_segment_sidecar` per sidecar initialisation — the fixed term that motivated the rule).

## Inherited unchanged (by reference to the R19 preregistration)

- Entry point: `SeferAlloc` `GlobalAlloc` → `HeapCore`, routed; identity rules (clean committed arms,
  source captured before build, frozen hashes, no reconstructed identity).
- Mode 1 benefit thresholds: `multiseg_cold_256k` and `seg_cycle_decommit_256k` B/A ≤ 0.25 (exact
  BigInt: 4·IrB ≤ IrA); `growth16_{n,2n,4n}` slope of B ≤ 2.5 (2·num ≤ 5·den, denominator > 0).
- Benefit oracle: B `pending_words_visited == 0` and `summary_words_visited > 0`; A pending counts
  319 063 / 1 004 503 with the frozen inclusive ±1 % tolerance; `routed_scans` > 0 and equal over all
  five arms; `nonempty_cuts`/`discovery_records_reclaimed` = 0 in pure growth; count monotonicity
  `nonempty_cuts ≤ pending_words_exchanged ≤ pending_words_visited`.
- Mode 2 (i) producer price: `(IrB − IrA)/4096 ≤ +6` on the matched child-body boundary
  (`EntryPoint::Custom` on
  `<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window`), **budget not raised**; oracle on
  B: `0 < summary_rmw < childFreeCount (4096)` and `summary_load == 0` (arm A: both 0, unchanged —
  re-freeze 8 entry below); producerIrReceipt with Amendments A+B coverage (boundary symbol,
  `boundaryOverheadIr ∈ 1..16`, `workloadIr > 0`, removed-frees control, nm/disassembly/profile
  receipts) — exactly the R19 schema; C″ must satisfy the ≤ +6 budget anyway.
  **Evidence classification (re-freeze 9):** B's `summary_load == 0` is a SOURCE-DERIVED expectation,
  NOT a dynamic observation. The oracle's load counter is fed 0 by construction at `edf90675`
  (`src/alloc_core/segment/remote_bitmap/prf01_sidecar_oracle.rs` adds the supplied arguments; the
  summary `fetch_or` in `sidecar_bitmap.rs` is followed by `published(1, 0)`), so the `== 0`
  check is kept as FORMAT CONSISTENCY and cannot by itself catch an uninstrumented summary load. The
  absence of a producer summary preload is established by (a) reading the C″ source
  (`git show edf90675:src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs`: the summary half is
  called only when the pending `fetch_or` returned 0, `notify = prev == 0`; the owner scanner's
  summary `swap` is a consumer-side operation) and (b) the Loom model
  `tests/loom_r20_prf01_summary_transition.rs`. The runner retains the producer-boundary
  disassembly (`boundary/*-disassembly.log`) as a RETAINED ARTEFACT FOR MANUAL REVIEW of the
  no-separate-preload property; the judge does NOT verify this property from it (its validator checks
  symbol presence and callers only). The summary RMW's inherent read is not conflated with the
  prohibited standalone load.
- `size_of::<SmallSidecar>()` B − A == exactly 512 (assert).
- Publication cell `r19_prf01_publication` and its oracle; the 13 mimalloc cells with ΔIr = 0 exactly
  (all cross pairs); `strictTrim` records no cell.
- Arm identities: A1/A2 = one A binary, B1/B2 = one B binary, C0 = A source under a different path
  label with the REGISTERED build difference `-C debuginfo=line-tables-only`; A/B/C full worktree
  paths of equal length with one-letter labels; B = A + one cherry-pick of `edf90675` (branch
  `r20-prf01-cdp`; verified to apply cleanly onto this base) and the pick must not touch the judge,
  the runner or this document.
- Native axis method (for the deferred future run): installed `#[global_allocator]`, fan-in
  1/2/4/8, nine fresh processes per arm and n, median of nine per-process metrics, never pooled —
  with the R19 run01 lesson applied: the **batch-timer** replaces the per-sample timer so that a
  single free's quantised 0-ns delta cannot reach the judge.

## Replaced (relative to R19)

1. **Hot controls (rule §2).** The four hot controls `small_churn_16b`, `churn_256b`,
   `churn_write_256b`, `aligned_churn_640b_a128` are no longer judged by the whole-body
   `|B−A| ≤ T` (that check judged the one-time construction as a per-operation tax in run01). Each
   family gets N/2N/4N siblings in the NEW iai target `benches/r20_prf01_controls_iai.rs`
   (bodies in `benches/support/r20_prf01_controls.rs`; `perf_gate_iai.rs` must not grow —
   `node scripts/verify-structure-limits.mjs`). Judged per family and cross pair:
   - slope `s_X = Ir_X(2N) − Ir_X(N)`; two-sided `|s_B − s_A1| ≤ T_s` and `|s_B − s_A2| ≤ T_s` with
     `T_s = max(10, |s_A1 − s_A2|, |s_C0 − s_A1|)` (rule §2.2);
   - linearity `|(Ir(4N) − Ir(2N)) − 2·(Ir(2N) − Ir(N))| ≤ T_s` for EVERY arm; hot churn is expected
     affine; a violation is **INVALID** (rule §2.3). No 3a exception is registered: no non-affinity of
     these families was ever characterised on A, so the 3a route is not open for this gate;
   - note recorded BEFORE data, corrected by re-freeze 9 (before any measurement): each arm's
     linearity is judged against ITS OWN N/2N slope (`|(Ir(4N) − Ir(2N)) − 2·s_arm| ≤ T_s`), so a pure
     affine shift of B by δ does NOT algebraically break B's linearity (an affine B keeps its own
     second window equal to twice its own first window). The check that actually catches a B slope
     shift is the separate two-sided slope bound `|s_B − s_A| ≤ T_s`. The two checks are independent
     and either can fail alone; neither introduces a hidden threshold. An earlier draft of this note
     claimed a B slope shift "always breaks linearity"; that was algebraically wrong and is corrected
     here.
   - N = 64 (= `CHURN_OPS`), 2N = 128 (= `CHURN_OPS_2N`), 4N = 256; op counts and
     workCount/blockBytes/alignment declarations are Amendment A declarations (workCount
     64/128/256; blockBytes 16/256/256/640; alignment 8/8/8/128; `segmentCount` null with reason).
2. **Construction (rule §2.4, reviewer answer 1).** Direct cell `r20_prf01_start_one_alloc_free_16b`:
   `SeferAlloc::new()` plus exactly ONE alloc/free pair of 16 B (the first sidecar registration). No
   intercept is ever derived from the churn families. One-sided `Ir_B − Ir_A ≤ budget_K` with
   **`budget_K = 0` Ir of net work** (the accepted +512 B layout must be paid for by the removed
   4096-word `pending` write loop; a pure regression of one-time work is not accepted) plus the
   qualified noise `T_s` of that cell (same T_s formula on the cell's raw Ir).
   **Anti-false-gain:** if `Ir_B − Ir_A < −T_s`, the judge requires a `constructionGainReceipt`
   `{path, sha256, removedPassObserved: true, decomposition}` on BOTH B rows — the retained Callgrind
   A/B functional decomposition showing the removed pass disappear; missing receipt → **INCOMPLETE**;
   a receipt that does not observe the removed pass (`removedPassObserved !== true`, bad path/sha)
   → **INVALID**. `budget_K` is per registration; the cell performs exactly one registration.
   **Receipt completion WITHOUT re-measurement (re-freeze 9):** the runner retains the construction
   cell's Callgrind profiles before deleting the observation home
   (`profiles/<run>-construction-<i>.out` plus `profiles/<run>-construction-inventory.json` with
   per-file sha256). If the gate returns INCOMPLETE on a missing `constructionGainReceipt`, a human
   annotates the retained profiles (e.g. `callgrind_annotate`), writes the decomposition, adds
   `constructionGainReceipt {path, sha256, removedPassObserved: true, decomposition}` to BOTH B rows
   in `input.json`, and re-runs ONLY the judge (`--input`); the judge is deterministic and no
   measurement is repeated. `scripts/r20_prf01_verify_artifacts.mjs` (acceptance step, see
   "Arches, runner, freeze" below) checks the receipt's file existence, its sha256 and a nonempty
   `decomposition`. Without the receipt the verdict honestly stays INCOMPLETE; the gain guard
   itself is not relaxed.
3. **Layout-noise ceiling (PRF-04 lesson).** For every hot family window (the raw N cell and both
   windows Ir(2N)−Ir(N), Ir(4N)−Ir(2N)) and for the construction cell: `|A1 − A2|` and `|C0 − A1|`
   must each be ≤ `max(10, ⌊A1/200⌋)` (numerator the absolute difference, denominator the A1 value of
   that quantity). Above the ceiling the verdict is **INCONCLUSIVE**; the ceiling never widens any
   budget or T_s and is never raised after data.
4. **Verdicts.** Priority: INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO.
   AS IMPLEMENTED, stated to match the code (re-freeze 9): a hot family WITHOUT its 2N/4N sibling
   rows fails row collection (exactly one row per arm per cell) and yields **INCOMPLETE** — the
   judge's family-level UNJUDGEABLE branch is unreachable for missing rows (noted in the judge
   source), so UNJUDGEABLE is in practice reachable only for a MISSING CONSTRUCTION CELL (excluded
   from the other checks, listed in the table, gate verdict then never GO). A missing row of any
   other judged cell is INCOMPLETE. Any `counterfactual` field in a real (non-rehearsal) input is
   INVALID.
5. **Native axis Mode 2 (ii) is DEFERRED**, not launched in v2 (the R19 run01 WSL2 evidence: 13 of
   108 native rows had p50Ns = 0 — clock quantisation). The judge requires the input marker
   `nativeDeferred {deferred: true, method}` naming the batch-timer method and returns
   `mode2Native = DEFERRED`; global verdict with all deterministic axes passed is
   **`INCONCLUSIVE (native deferred)`** — never GO. Any deterministic failure (Mode 1, Mode 2 (i),
   mimalloc, sizeOf) is still NO-GO/INVALID. The deferred method: batch-timer over a fixed remote-free
   batch, per-process p50/p99 of batch times divided by the batch size, on a quiet bare-metal host.

## Cells and inventory (judge-frozen, 33 names)

5 benefit + 12 hot-family cells (`r20_prf01_{small_churn_16b,churn_256b,churn_write_256b,aligned_churn_640b_a128}_{n,2n,4n}`)
+ 1 construction + 13 mimalloc + publication + producer = 33 names × 5 arms = 165 rows.
The iai target contributes 20 cells (5 benefit + 13 mimalloc + publication + producer); the controls
target contributes 13 cells (12 hot + construction). The original
whole-body hot cells of `perf_gate_iai.rs` still execute (the runner runs the whole group per label)
but are NOT judge rows in v2; their Ir is retained in the raw logs only.

Body correspondence of the new cells with the originals (verified by reading
`benches/perf_gate_iai.rs`): identical statement order `SeferAlloc::new()` → `Layout::from_size_align`
→ loop `alloc` → `black_box(ptr)` → null check → `dealloc`; `churn_write_256b` keeps the two
`write_volatile` stores of `0xA5A5_A5A5_A5A5_A5A5`; sizes/alignments identical; only the op count is
a parameter (64/128/256). The `2n` shapes match the existing byte-identical `small_churn_16b_2n`
pattern (`CHURN_OPS_2N`).

## Arches, runner, freeze

- A = clean committed HEAD of the worktree holding the runner, judge and this document (the runner
  refuses a dirty tree); B = A + cherry-pick `edf90675`; C0 = A source, third worktree, equal-length
  path, `-C debuginfo=line-tables-only` (registered; changes no code, only makes the binary distinct).
- The runner (`scripts/r20_prf01_sidecar_summary_run.mjs`) stages `--prepare/--build/--oracle/
  --measure/--rehearsal/--cleanup` with `--slop` as negative control, takes the repository root from
  `git rev-parse --git-common-dir`, refuses a dirty base, redacts private paths (`X:\…`, `/mnt/x/…`,
  `/home/…`) from every stored text, reads the frozen hash list from the judge (`--hashes` /
  `--check-hashes`), and runs natively on Linux (CI) or through `wsl.exe` on a Windows host.
  `--rehearsal` runs the full pipeline with the iai arm B = A′ (A source under label B, distinguished
  by the registered flag); the oracle stage still uses the real B build for path activation only.
- Frozen set (SHA256 in the companion `.sha256`, this document first): this document, the judge, the
  runner, the acceptance script `scripts/r20_prf01_verify_artifacts.mjs`, `docs/perf/R20_CONTROL_GEOMETRY_RULE.md`, `benches/r20_prf01_controls_iai.rs`,
  `benches/support/r20_prf01_controls.rs`, `benches/support/r19_prf01.rs`, `benches/perf_gate_iai.rs`,
  `tests/r19_prf01_sidecar_oracle.rs`, `examples/r19_prf01_oracle.rs`. Companion excludes itself.
  Any edit requires an explicit re-freeze BEFORE any measurement.
- Per-stage identity binding (implemented, re-freeze 9): the `--build`, `--oracle` and
  `--measure` stages each RECOMPUTE the arch identity (HEAD/tree/identitySha256, clean-tree) of
  every arm A, B, C — in rehearsal including the B = A′ proxy and the real-B `oracleB` — and
  compare it with `manifest.json`; any mismatch aborts the stage immediately, as does a moved
  runner HEAD. Stage files are bound to one another: `builds.json` records `manifestSha256`,
  `oracle.json` records `manifestSha256` and `buildsSha256`, and `--measure` refuses to
  run on any mismatch. The official out-dir
  `docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01` lives in the PRIMARY checkout and is NOT part of
  the worktrees A/B/C; the cleanliness/identity checks deliberately do not depend on it. Residual
  limitation: the R19 two-phase capture/identity-review protocol is still not reproduced; producer
  boundary receipts (nm/disassembly/profile, Amendment B coverage) are produced by the measure stage
  per arm as in R19.
- Receipt-hash anchoring and acceptance (re-freeze 9): the runner writes `receiptHashes` into
  `input.json` — the sha256 of every out-dir file referenced by any receipt field
  (`environment.log`, `manifest.json`, `builds.json`, `oracle.json`, `logs/*`,
  `boundary/*`, `profiles/*`, `oracle/*`) plus the retained construction inventories. The
  judge treats the field as optional schema and NEVER uses it in the verdict. The acceptance step
  AFTER the judge is `node scripts/r20_prf01_verify_artifacts.mjs <out-dir>`: it verifies every
  `receiptHashes` entry (existence + sha256), that every receipt string field of the input is
  anchored in the map and exists, ancestry vs manifest, input binary pins vs `builds.json`, and —
  when present — the `constructionGainReceipt` (path exists, sha256 matches, nonempty
  `decomposition`). The orchestrator must run it (exit 0) BEFORE treating the official verdict as
  usable.
- `--slop` is allowed ONLY together with `--rehearsal`; an official measure with `--slop` is
  refused by the runner. When applied, the counterfactual is recorded durably in `input.json`
  (`counterfactual: {slop: true, …}`) and as a comment line in `summary.csv`, and the judge
  returns INVALID for ANY `counterfactual` field in a non-rehearsal input.
- Rehearsal self-identification (re-freeze 9): the judge result carries `rehearsal: true` and its
  `modes.mode1`/`modes.mode2Iai` values are rewritten with the same `REHEARSAL: ` prefix as the
  global verdict. Only the complete global verdict plus run identity is authoritative.
- Tool versions: the producer receipts record the iai-callgrind-runner version DERIVED from the
  captured environment (`iai-callgrind-runner --version` in `environment.log`), not a literal;
  the CI workflow pins the runner at `^0.14`. Valgrind's version is captured in the same
  `environment.log`. At the 2 MiB `RAW_LOG_LIMIT` the runner FIRST stores the redacted log,
  then fails, so the evidence survives the abort.
- Dispatch recipe (frozen): after the PR merges, dispatch the BASELINE ref on `main` with
  `runner=scripts/r20_prf01_sidecar_summary_run.mjs` and
  `stages=prepare,build,oracle,measure` (no rehearsal, no cleanup), `slop=false`. The candidate
  `edf90675` must remain reachable in the checkout (branch `r20-prf01-cdp-main` is not deleted).
  Artifacts are captured from the out-dir BEFORE any cleanup. Technical-failure rules: a failure
  BEFORE `input.json` is written does not consume the one-shot, provided no candidate-B Ir was
  produced; once `input.json` exists, the result stands as recorded.

## Rehearsal contract

If present, `rehearsal` must be boolean; the runner emits `rehearsal: true` only in rehearsal.
Only the two A/B distinct-source-identity and distinct-measurement-commit assertions are relaxed.
A distinct, validated `B.oracleSource` is required only in rehearsal and forbidden otherwise; B oracle
linkages are checked against it, while all other checks remain unchanged. Verdicts are prefixed
`REHEARSAL: ` except INVALID, INCOMPLETE and UNJUDGEABLE. Prefixed verdicts and the unprefixed
`INCONCLUSIVE (native deferred)` exit 0; all other verdicts exit 1. Rehearsal produces no usable
GO/NO-GO gate result. The judge result additionally self-identifies (re-freeze 9): it carries
`rehearsal: true` and rewrites `modes.mode1`/`modes.mode2Iai` with the same `REHEARSAL: `
prefix as the global verdict, so mode-field-only consumers cannot mislabel rehearsal data.

## Not covered (stated, not hidden)

- The native Mode 2 (ii) axis (deferred, see above); contention/CAS states and multi-producer fan-in
  Ir behaviour; Loom/correctness obligations of the summary algorithm; RSS, cache footprint and
  wall-clock latency (Ir does not cover them).
- A function-level Callgrind decomposition of the whole A/B sidecar path beyond the required
  construction-gain receipt; a selector-only cell with an absolute Ir budget.
- `summary_load == 0` on the producer cell is a format-consistency check over a source-derived
  expectation, not an Ir-level proof of "no summary read" (see the Mode 2 (i) evidence
  classification); no judge-side check automates the producer disassembly review — it is a retained
  artefact for manual review.
- If the candidate changes sidecar layout or initialisation again, this A-characterisation no longer
  qualifies that combination; a new identity and preregistration are required. The R19 numbers are
  never recomputed.

## Re-freeze log (before any measurement)

> **Terminology and corrections (re-freeze 9), applied to the entries below without rewriting
> them:** "still no measurement" means NO CANDIDATE PRIMARY Ir under this official protocol. The
> rehearsals plainly measured A, A′ (the iai arm labelled B) and C0, and ran the real-B oracle for
> path activation; all of that is disclosed in the entries. The ninth rehearsal was still running in
> CI when this re-freeze was frozen; the orchestrator appends its record. Correction to the second
> entry's wording: `git diff 3033c499 edf90675 -- src` is TWO changed re-export cfg lines plus
> THREE added cfg import lines — five lines total, all cfg-only ("three cfg-only lines" there is
> imprecise); unchanged production codegen of the `production` build was inspected, not
> experimentally verified.

- 2026-10-13: the first CI rehearsal (r20-gates run 38356350870, native Linux) aborted in the runner's
  `publicText`, which converted the repository root with the Windows-only `winToWsl` on every
  platform (`AssertionError: not an absolute Windows path`). Fix: `toHost(REPO_ROOT)`, the identity on
  Linux. No measurement existed at that point, so the runner hash and this document were re-frozen in
  the companion `.sha256`.
- 2026-10-13 (second rehearsal, run 38356819349): `git cherry-pick 3033c499` onto the current gate tip
  conflicted in `README.md` (test/bench inventory counts that moved after C'' was cut). The candidate is
  therefore re-cut as ONE commit `edf90675` (branch `r20-prf01-cdp-main`): a squash of `r20-prf01-cdp`
  (3033c499 + its CI/test/docs fixes) applied onto this gate branch tip with only the README counts
  resolved (390 tests / 92 examples / 28 benches / 15 root Loom models, checked against disk).
  `git diff 3033c499 edf90675 -- src` is limited to three cfg-only lines (the re-export gate inherited from
  main and the gated `PausedPublish` imports); production codegen of the `production` build is unchanged.
  The runner's `PICK` is now `edf90675`; hashes re-frozen, still no measurement.
- 2026-10-13 (third rehearsal, run 38357969386): prepare/build/oracle passed on native Linux; `measure`
  parsed 0 of 20 cells from the iai output of run A1 and aborted before storing the raw text. The runner
  now writes the unparsed output to `logs/<run>-<target>.unparsed.log` (and its head into the assertion
  message) before failing; diagnostics only, no change to any measured quantity. Re-frozen, still no
  measurement.
- 2026-10-13 (fourth rehearsal, run 38360504407): the retained raw output showed the cause of the 0/20 parse.
  The runner runs the iai target with `IAI_CALLGRIND_LOG=debug`; the old header regex
  `^(\w+::)+name\b` also matched the debug lines `iai_callgrind_runner::runner::tool: Debug: ...` and
  overwrote the current cell name before the `Instructions:` line. The header regex now requires a bare
  `crate::group::cell` line (optional trailing arguments, nothing after the name but whitespace). Replayed
  offline on the captured output it returns `small_churn_16b = 54293`, the value printed by iai.
  Parser only; no measured quantity changed. Re-frozen, still no measurement.
- 2026-10-13 (fifth rehearsal, run 38361458401): parsing now works; the A1 iai output with
  `IAI_CALLGRIND_LOG=debug` plus valgrind `--verbose` exceeded the runner's 400 KiB per-log cap
  (`raw log A1/perf_gate_iai over 409600 bytes`). The cap is raised to the CLAUDE.md tier-3 ceiling of 2 MiB
  and the message now prints the actual size. Logs between 200 KiB and 2 MiB are gzipped (or truncated with
  a marker) when committed as evidence, hashes recorded on the uncompressed text. Cap only; re-frozen,
  still no measurement.
- 2026-10-13 (sixth rehearsal, run 38362638571): `perf_gate_iai` parsed 20/20 cells; the controls bench
  parsed 0/13 because its command, unlike the other iai invocations, did not set `IAI_CALLGRIND_COLOR=never`
  and the colour escapes broke the cell-header match (raw output retained by the new diagnostics showed
  all 13 `Instructions:` blocks). The controls command now sets it, and `parseIr` also strips ANSI
  sequences before matching. Output formatting only; re-frozen, still no measurement.
- 2026-10-13 (seventh rehearsal, run 38368482309, plus a static review of the never-executed remainder of
  `measure`): rehearsal 7 passed prepare/build/oracle and both iai measurements and failed at the producer
  boundary of C0 (the C arm had no producer-control build). A static pass over the rest of `measure`
  against the judge found, and this re-freeze fixes, before any measurement:
  (1) runner: C builds the producer-control binary; measurements execute the recorded executables
  directly (the former `cargo bench` re-invocation could rebuild a different binary than the recorded
  one); per-label oracle observations (unique run ids; C0 reuses A's oracle binary explicitly); measured
  PIDs, profile inventories and string receipts; CSV derived from the final input;
  (2) judge: controls-target rows are checked against the controls binary pins (new
  `identities.C0.controlsBinarySha256`), not the iai binary; an explicit `rehearsal` contract (section
  above) replaces the impossible requirement that a B = A′ rehearsal pass the real-run A/B identity
  asserts; counts corrected to 33 names / 165 rows;
  (3) judge: the hot-family slope check was one-sided although the rule and this document say two-sided
  (`|s_B − s_A| ≤ T_s`); it now uses the absolute value. Selftest gained mutants for each of these (149
  checks); the two-sided mutant fails against the previous one-sided code (counterfactual run).
  No measured quantity changed; still no measurement.
- 2026-10-13 (eighth rehearsal, run 38377475462): the pipeline now ran end to end (prepare, build, oracle,
  all iai and controls measurements, producer boundary passes, `input.json`, `summary.csv`, judge) and the
  judge returned INCOMPLETE on `assert.ok(r.oracle.summary_load>0)` for the producer cell of arm B. That
  assertion was inherited from R19's candidate C′, which loaded the summary on every free (R19 run: B
  `summary_load` 4096, `summary_rmw` 64). The v2 candidate C″ sets the summary bit only when the pending
  `fetch_or` returned 0 and never reads the summary on the producer path; the real B oracle shows
  `summary_rmw` 64 and `summary_load` 0. The expectation for arm B on the producer cell is now
  `0 < summary_rmw < childFreeCount (4096)` and `summary_load == 0` (arm A: both 0, unchanged); selftest
  mutants: rmw 0, rmw 4096 (notify on every free), load 1. Replaying the retained rehearsal input through
  the updated judge gives `REHEARSAL: NO-GO` (expected: B = A′ cannot show the benefit), exit 0.
  Judge expectation only; still no measurement.
- 2026-10-13 (re-freeze 9, after an independent read-only review of the gate and before the official run):
  the review's MAJOR findings are closed in the runner, judge and this document: `summary_load == 0` is
  classified as source-derived (not an observation); construction-cell Callgrind profiles are retained so the
  `constructionGainReceipt` can be completed without re-measuring; build/oracle/measure re-verify the captured
  arm identities and bind builds.json/oracle.json to the manifest; `input.receiptHashes` plus the new frozen
  acceptance script `scripts/r20_prf01_verify_artifacts.mjs` authenticate the receipt files after the judge;
  `--slop` is rehearsal-only and a `counterfactual` marker makes a real input INVALID; rehearsal results carry
  a marker and prefixed mode fields; prose contradictions are corrected (see the Dispatch recipe section).
  Selftest 157 checks; the new mutants fail on the previous judge (counterfactual). Eighth/ninth rehearsal
  runs (38377475462, 38382265194) preceded this edit; the ninth ran the previous code. No candidate
  primary Ir exists under the official protocol.
- 2026-10-13 (re-freeze 10, first official dispatch, run 38424122185 on main `91aa0e9a`): the run aborted in
  the `build` stage with `runner worktree must be clean and committed`. `prepare` had already written
  `manifest.json` into the official out-dir `docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01/`, which sits
  inside the checkout, and `baseHead()` counted that untracked directory as dirt. The rehearsals used the
  git-ignored `target/r20p1-rehearsal`, so this path was first exercised by the official run. Fix:
  `baseHead()` excludes the stage's own out-dir from the cleanliness check (pathspec `:(exclude)<out-dir>`);
  any other uncommitted or untracked path still fails it. No oracle or measure stage ran and no measured
  quantity exists; the aborted run produced only `manifest.json`. Runner hash and this document re-frozen.
