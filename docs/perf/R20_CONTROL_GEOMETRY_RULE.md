# R20 control-geometry rule for Ir gates — per-operation neutrality is judged on the N/2N slope, construction is a separate one-sided cell

Report class: **protocol rule only** (no measurement, no runtime change). It is referenced by every R20 preregistration
(PRF-01 v2, PRF-03 if redesigned, PRF-04 claim-path cells) and is frozen with the first of them. It is **not
retroactive**: the R19 verdicts (PRF-01 C′ NO-GO, PRF-03 B/B2 NO-GO) are unchanged.

## 1. Why a rule is needed

Owner decision 2026-10-12: do as the arbiter recommended
(`docs/reviews/2026-10-12-prf-open-questions-reviewer-answers.md` §4). Two R19 observations show that a whole-body
"hot control" can judge a one-time construction cost as if it were a per-operation tax:

- PRF-01 run01: the four hot controls (`small_churn_16b`, `churn_256b`, `churn_write_256b`, `aligned_churn_640b_a128`)
  all moved by the **same** +569 Ir (`docs/perf/R19_PRF01_C_PRIME_RUN01/iai_summary.csv`) although they run different
  operations and sizes. `small_churn_16b` builds `SeferAlloc::new()` and then runs `CHURN_OPS` = 64 operations
  (`benches/perf_gate_iai.rs`). An identical shift across different workloads is a construction term with a slope of
  zero; the Sefer `small_churn_16b_2n` sibling that would show the slope was not in the PRF-01 inventory (only its
  mimalloc twin was), so this reading is an inference until the slope cell exists.
- PRF-03 run02 candidate B2: the seven small-class controls were cheaper by 107..114 Ir (a one-time construction
  saving: memcpy −101, `new_with_config_and_owner` −13), which the two-sided whole-body bound `|delta| <= T` still failed
  (`docs/perf/R19_PRF03_B2_VS_A_GATE.md`).

The repository already has the method: R23-2 derives the per-operation cost as `c = (Ir(2N) − Ir(N)) / N`, which cancels
the one-time constant algebraically (`docs/perf/R23_2_WARM_N_2N_MIMALLOC_GATE.md`). This rule applies it to controls.

## 2. The rule (prospective; thresholds are fixed by the preregistration before any B build exists)

For a control family `F` with cells at `N`, `2N` (and `4N` where the family has one) and for each arm `X`
(`A1`, `A2`, `B`, `C0` as in the preregistration):

1. **Slope.** `s_X = Ir_X(2N) − Ir_X(N)` (Ir for `N` extra operations).
2. **Per-operation neutrality (two-sided).** `|s_B − s_A1| <= T_s` and `|s_B − s_A2| <= T_s`, with
   `T_s = max(10, |s_A1 − s_A2|, |s_C0 − s_A1|)` — the same structure as the R19 `T`, applied to the slope.
3. **Linearity check.** Where a `4N` cell exists, `|(Ir(4N) − Ir(2N)) − 2·(Ir(2N) − Ir(N))| <= T_s` for every arm;
   if it fails the family is **INVALID** (warm-up is not linear) and yields no verdict.
4. **Construction (one-sided, separate cell).** `K_X = Ir_X(N) − s_X` is the one-time part (construction, bootstrap,
   first scan). `K_B − K_A <= budget_K`, where `budget_K` is written into the preregistration by the owner **before**
   the B build exists, together with the principle it comes from (for example the accepted layout change), never from a
   seen result. A candidate that adds no construction work gets `budget_K = T_s`.
5. **Inventory.** Every control family in a gate inventory carries its `2N` sibling (and `4N` for cold/first-touch
   families). A family without siblings is listed as **UNJUDGEABLE** and is excluded from the verdict, never silently
   treated as a pass.
6. **Producer/consumer price.** A per-event budget (for example PRF-01's `<= +6 Ir` per remote free) stays computed on
   its own window cell with the fixed construction overhead cancelled as in PRF-01 Amendment B; it is not replaced by
   this rule and its budget is not changed.

## 3. What this rule does not do

- It does not raise or lower any R19 threshold and does not reinterpret any R19 number.
- It does not choose `budget_K`; that is an owner input per preregistration.
- It does not replace the path-activation oracle, the entry-point statement or the single derivation script required
  by CLAUDE.md; those still apply to each preregistration.
