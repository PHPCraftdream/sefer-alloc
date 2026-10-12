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
  `<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window`), **budget not raised**; oracle
  `summary_rmw > 0` and `summary_load > 0` separately on B; producerIrReceipt with Amendments A+B
  coverage (boundary symbol, `boundaryOverheadIr ∈ 1..16`, `workloadIr > 0`, removed-frees control,
  nm/disassembly/profile receipts) — exactly the R19 schema; C″ has NO summary load in A-terms (the
  bit is set only on a 0→non-0 word transition), and must satisfy the ≤ +6 budget anyway.
- `size_of::<SmallSidecar>()` B − A == exactly 512 (assert).
- Publication cell `r19_prf01_publication` and its oracle; the 13 mimalloc cells with ΔIr = 0 exactly
  (all cross pairs); `strictTrim` records no cell.
- Arm identities: A1/A2 = one A binary, B1/B2 = one B binary, C0 = A source under a different path
  label with the REGISTERED build difference `-C debuginfo=line-tables-only`; A/B/C full worktree
  paths of equal length with one-letter labels; B = A + one cherry-pick of `3033c499` (branch
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
   - note recorded BEFORE data: with the affine 4N cell present, a B slope shift of δ also shifts B's
     second window by δ, so `|δ| > T_s` always breaks B's linearity — the two-sided bound and the
     linearity check fail together; this is a property of the rule, not a hidden threshold.
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
3. **Layout-noise ceiling (PRF-04 lesson).** For every hot family window (the raw N cell and both
   windows Ir(2N)−Ir(N), Ir(4N)−Ir(2N)) and for the construction cell: `|A1 − A2|` and `|C0 − A1|`
   must each be ≤ `max(10, ⌊A1/200⌋)` (numerator the absolute difference, denominator the A1 value of
   that quantity). Above the ceiling the verdict is **INCONCLUSIVE**; the ceiling never widens any
   budget or T_s and is never raised after data.
4. **Verdicts.** Priority: INCOMPLETE > INVALID > NO-GO > INCONCLUSIVE > UNJUDGEABLE > GO.
   UNJUDGEABLE (a hot family without its 2N/4N siblings, or a missing construction cell) is excluded
   from the other checks, listed in the table, and the gate verdict is then never GO. A missing row
   of any other judged cell is INCOMPLETE.
5. **Native axis Mode 2 (ii) is DEFERRED**, not launched in v2 (the R19 run01 WSL2 evidence: 13 of
   108 native rows had p50Ns = 0 — clock quantisation). The judge requires the input marker
   `nativeDeferred {deferred: true, method}` naming the batch-timer method and returns
   `mode2Native = DEFERRED`; global verdict with all deterministic axes passed is
   **`INCONCLUSIVE (native deferred)`** — never GO. Any deterministic failure (Mode 1, Mode 2 (i),
   mimalloc, sizeOf) is still NO-GO/INVALID. The deferred method: batch-timer over a fixed remote-free
   batch, per-process p50/p99 of batch times divided by the batch size, on a quiet bare-metal host.

## Cells and inventory (judge-frozen, 32 names)

5 benefit + 12 hot-family cells (`r20_prf01_{small_churn_16b,churn_256b,churn_write_256b,aligned_churn_640b_a128}_{n,2n,4n}`)
+ 1 construction + 13 mimalloc + publication + producer = 32 names × 5 arms = 160 rows. The original
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
  refuses a dirty tree); B = A + cherry-pick `3033c499`; C0 = A source, third worktree, equal-length
  path, `-C debuginfo=line-tables-only` (registered; changes no code, only makes the binary distinct).
- The runner (`scripts/r20_prf01_sidecar_summary_run.mjs`) stages `--prepare/--build/--oracle/
  --measure/--rehearsal/--cleanup` with `--slop` as negative control, takes the repository root from
  `git rev-parse --git-common-dir`, refuses a dirty base, redacts private paths (`X:\…`, `/mnt/x/…`,
  `/home/…`) from every stored text, reads the frozen hash list from the judge (`--hashes` /
  `--check-hashes`), and runs natively on Linux (CI) or through `wsl.exe` on a Windows host.
  `--rehearsal` runs the full pipeline with the iai arm B = A′ (A source under label B, distinguished
  by the registered flag); the oracle stage still uses the real B build for path activation only.
- Frozen set (SHA256 in the companion `.sha256`, this document first): this document, the judge, the
  runner, `docs/perf/R20_CONTROL_GEOMETRY_RULE.md`, `benches/r20_prf01_controls_iai.rs`,
  `benches/support/r20_prf01_controls.rs`, `benches/support/r19_prf01.rs`, `benches/perf_gate_iai.rs`,
  `tests/r19_prf01_sidecar_oracle.rs`, `examples/r19_prf01_oracle.rs`. Companion excludes itself.
  Any edit requires an explicit re-freeze BEFORE any measurement.
- Known limitation disclosed: the runner does NOT reproduce the full R19 two-phase
  capture/identity-review protocol; identity is bound per stage (prepare manifest + per-run binary
  SHA256 + clean-tree checks), and the producer boundary receipts (nm/disassembly/profile,
  Amendment B coverage) are produced by the runner's measure stage per arm as in R19.

## Not covered (stated, not hidden)

- The native Mode 2 (ii) axis (deferred, see above); contention/CAS states and multi-producer fan-in
  Ir behaviour; Loom/correctness obligations of the summary algorithm; RSS, cache footprint and
  wall-clock latency (Ir does not cover them).
- A function-level Callgrind decomposition of the whole A/B sidecar path beyond the required
  construction-gain receipt; a selector-only cell with an absolute Ir budget.
- If the candidate changes sidecar layout or initialisation again, this A-characterisation no longer
  qualifies that combination; a new identity and preregistration are required. The R19 numbers are
  never recomputed.
