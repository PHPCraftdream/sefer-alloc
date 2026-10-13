# R20 PRF-04 claim-path Ir gate, run01: GO on the deterministic axes (candidate V4′ is not merged by this report)

Report class: **bench** (measurement and decision record, measurement-only). The candidate (`perf(runtime)` commit
`6951a9d0`, branch `r20-prf04-v4prime`) is **not merged** by this report. No production default changed. The
preregistration is `docs/perf/R20_PRF04_CLAIM_PATH_PREREG.md` (frozen, hashes in
`R20_PRF04_CLAIM_PATH_PREREG.sha256`); the judge is `scripts/r20_prf04_claim_path_gate.mjs`, the executor
`scripts/r20_prf04_claim_path_run.mjs`, the retained evidence `docs/perf/R20_PRF04_CLAIM_PATH_RUN01/`.

## 0. Headline

- The new primary KILL-1 of the arbiter ruling (deterministic Ir of the claim path, replacing the rejected
  320-ABBA estimator) **passes**: the frozen judge returned **GO**. Candidate B adds **+8 Ir to the start cell**
  (budget_K = T_s = 10), **+8 Ir per cold EMPTY-first-claim** (+0.02 %, budget +5 %) and **+15 Ir per recycled
  claim+release pair** (+0.46 %, budget +5 %).
- The central mechanism is exercised and pays: in the wave state (64 FREE slots, then N claims) main serves one claim
  by hint and **bumps N−1 new heaps**, while B serves N−1 claims from its FREE-slot hint set with **zero new heaps**;
  per claim the Ir falls from about 37.7 k to about 3.2 k (**−34 463 Ir, −91.40 %** per claim, see §2).
- What this GO is **not**: it is the deterministic cost axis only. The native axes of the preregistration (live
  first-call latency, churn wall, wave p99, idle commit charge) stay descriptive and **unevaluated here** (they need a
  quiet bare-metal host with a nanosecond clock). In R19 run02 the native BENEFIT was met (−99.2 % idle commit charge)
  and the native R-live p50 fired (+19.08 %); that +19 % is not explained by +8..+15 Ir per operation and remains an
  open native question, not a closed one.
- The 5 % budget cannot resolve a selection-logic delta of tens of Ir (it is about 161 Ir per recycled pair and
  about 1.9 k Ir per cold claim); the table below shows the actual deltas (8 and 15 Ir), which a tighter absolute budget
  would have judged. Whether to merge V4′ is the owner's decision (§6).

## 1. What was measured

Entry point: `HeapRegistry::dbg_claim_lease()` (body identical to `claim_lease()`, the call the TLS bind makes on a
thread's first allocating call) on the collected thread, plus the release by dropping the lease; not the
`#[global_allocator]` layer (the reason is in the preregistration: callgrind collects 0 Ir in spawned worker threads
under the default toggle).

| Item | Value |
|---|---|
| Arm A | clean committed tree of `r20-prf04-gate` @ `f5194740` (main `e2a5099a` + this harness), tree `840e0ea21d0fc0e1797b1a3174c2522aca4d84a8` |
| Arm B | A + one cherry-pick of `6951a9d0` = commit `ef0d2657` (kept as branch `r20-prf04-gate-b`), tree `ec918dbc57d844dde85b099d7d0bf53989897e23` |
| C0 | the A source built in a third worktree under a different path label, with the registered flag `-C debuginfo=line-tables-only` (distinct binary; tree = A) |
| Paths | `worktrees/r20p4/{A,B,C}`, full path length 41 in all three |
| Host | WSL2 Linux 6.18.33.2 x86_64, i7-11800H, rustc 1.98.1, valgrind 3.22.0, iai-callgrind 0.14.2 (`R20_PRF04_CLAIM_PATH_RUN01/environment.log`) |
| Features | iai cells `production internals`; oracle `production bench-internals internals` |
| Runs | A1, A2 (one A binary), B1, B2 (one B binary), C0: 5 iai processes × 10 cells; 20 oracle processes; per-cell Ir in `R20_PRF04_CLAIM_PATH_RUN01/summary.csv` (derived by the runner from the judge input) |

## 2. Derived tables

Derived by `scripts/r20_prf04_gate_excerpt.mjs` from the retained judge input and asserted equal to the judge's saved
rows (numerator and denominator of every percentage are the columns named in the header). Ir is deterministic:
A1 = A2 and B1 = B2 in every cell, and C0 = A.

<!-- BEGIN DERIVED (scripts/r20_prf04_gate_excerpt.mjs) -->

Judge verdict (retained `docs/perf/R20_PRF04_CLAIM_PATH_RUN01/verdict.txt`): **GO** on the deterministic axes. A1 = A2 and B1 = B2 bit for bit in all 10 cells, and C0 = A for every cell the table uses.

Construction (start cell, one-sided, budget_K = T_s): Ir A = 47135, Ir B = 47143, B − A = 8 against budget_K = 10 (numerator 8, denominator budget 10).

| family | window | claims | Ir A | Ir B | B − A (numerator) | A (denominator) | B/A − 1 | per claim (B − A)/claims |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| cold | w0 (0→n) | 16 claims | 601882 | 602010 | 128 | 601882 | +0.02% | 8 |
| cold | w1 (n→2n) | 16 claims | 601642 | 601770 | 128 | 601642 | +0.02% | 8 |
| cold | w2 (2n→4n) | 32 claims | 1203200 | 1203456 | 256 | 1203200 | +0.02% | 8 |
| wave | w0 (0→n) | 16 claims | 3000311 | 2459372 | -540939 | 3000311 | -18.03% | -540939/16 |
| wave | w1 (n→2n) | 16 claims | 603264 | 51856 | -551408 | 603264 | -91.40% | -34463 |
| wave | w2 (2n→4n) | 32 claims | 1205965 | 103721 | -1102244 | 1205965 | -91.40% | -1102244/32 |
| recycled | w0 (n→2n) | 16 claim+release pairs | 51632 | 51872 | 240 | 51632 | +0.46% | 15 |

Path oracle (retained `oracle.json`, bound to each arm identity and oracle binary; counts over the measured phase):

| arm | family | N | hint claims | bump claims | free-set claims | new heaps | counters |
|---|---|---:|---:|---:|---:|---:|---:|
| A | start | 1 | 0 | 1 | 0 | 1 | 4 |
| A | cold | 16 | 0 | 16 | 0 | 16 | 4 |
| A | cold | 32 | 0 | 32 | 0 | 32 | 4 |
| A | cold | 64 | 0 | 64 | 0 | 64 | 4 |
| A | recycled | 16 | 16 | 0 | 0 | 0 | 4 |
| A | recycled | 32 | 32 | 0 | 0 | 0 | 4 |
| A | recycled | 64 | 64 | 0 | 0 | 0 | 4 |
| A | wave | 16 | 1 | 15 | 0 | 15 | 4 |
| A | wave | 32 | 1 | 31 | 0 | 31 | 4 |
| A | wave | 64 | 1 | 63 | 0 | 63 | 4 |
| B | start | 1 | 0 | 1 | 0 | 1 | 5 |
| B | cold | 16 | 0 | 16 | 0 | 16 | 5 |
| B | cold | 32 | 0 | 32 | 0 | 32 | 5 |
| B | cold | 64 | 0 | 64 | 0 | 64 | 5 |
| B | recycled | 16 | 16 | 0 | 0 | 0 | 5 |
| B | recycled | 32 | 32 | 0 | 0 | 0 | 5 |
| B | recycled | 64 | 64 | 0 | 0 | 0 | 5 |
| B | wave | 16 | 1 | 0 | 15 | 0 | 5 |
| B | wave | 32 | 1 | 0 | 31 | 0 | 5 |
| B | wave | 64 | 1 | 0 | 63 | 0 | 5 |

<!-- END DERIVED -->

Reading notes (the arithmetic is in the table; these are interpretations):

- Cold `w0 = Ir(N)` is the whole 16-claim body (claims 1..16 plus the constant). The three cold windows give the same
  +8 Ir per claim (128/16, 128/16, 256/32), so B's bookkeeping on the bump path is constant, not index-dependent in the
  registered range.
- The recycled pair includes both the claim through the reuse hint and the release; +240 over 16 pairs is +15 per pair.
- Wave `w1` gives −34 463 per claim exactly (−551 408/16); `w2` gives −1 102 244 over 32 claims = −34 445.1 per claim
  (not an integer because main's bump cost has the period-4 glibc pattern of the preregistration). `w0` also contains the warmup
  (64 claims and 64 releases): its saving, −540 939, is smaller than 16 × the `w1` per-claim saving by 10 469 Ir.
  That remainder holds the 64 warmup releases and the 64 warmup claims' +8 each (512); **B's release path in a wave was
  not gated separately, and this subtraction is an inference, not a measurement** (see §5).

## 3. Verdict and criteria

All checks of the frozen judge passed: construction (B − A = 8 ≤ budget_K = 10), R-claim in every window and pair
(largest +0.46 % against +5 %), recycled linearity (deviation 0 ≤ 10 for all five arms), the layout-noise ceiling
(`|A1−A2| = |C0−A1| = 0` everywhere), and the per-arm path oracle (the wave on B went through the free set in every
size). The judge's output is retained verbatim in `R20_PRF04_CLAIM_PATH_RUN01/verdict.txt`.

## 4. Process record and disclosures

1. A first cell design (spawned threads through `GlobalAlloc`) measured nothing of the claim (worker-thread callgrind
   files hold 0 Ir) and was discarded before any gate run; the corrected design and the cold non-affinity finding
   (glibc `calloc` pattern) are in the preregistration, with the A-only evidence logs frozen.
2. A worker delivered the first runner; zero-trust review found and the orchestrator fixed: arm worktrees nested under
   `worktrees/worktrees`, a hard-coded base commit, private paths in stored logs, and cells that had never been
   compiled on Linux. Two independent reviewer reads (2026-10-12) led to the start cell, the wave family, the per-arm
   bound oracle, the layout-noise ceiling and the widened frozen set; see
   `docs/reviews/2026-10-12-r20-budgetk-and-rule3a-reviewer-answers.md`.
3. Candidate B was built and its path oracle run once during runner development (compile and counters only, no
   instruction count); those artefacts were discarded. A rehearsal with an A′ stand-in for B returned GO and, with a
   +6 % injection, NO-GO (`--rehearsal`, `--slop`). The official run was executed **once** from HEAD `f5194740` after
   `--cleanup`, with the frozen hashes verified, no retries and no threshold change.
4. The judge's self-test and nine counterfactual mutants of the judge's rules (each turns the self-test red) were run
   before the freeze.

## 5. Not covered

- Native axes (see §0). Concurrency: contention and CAS losses on the free-slot set, stale-summary races, capacity and
  saturation states, reused heaps carrying a previous owner's state.
- **Release-only cost in a wave** (inference in §2) and a **selector-only cell with an absolute Ir budget**: a tighter
  instrument would have judged the 8 and 15 Ir per operation directly; it needs its own preregistration.
- If PRF-01 changes sidecar layout or initialisation, the A-only characterization behind the cold windows no longer
  qualifies that combination (new identity, new preregistration).

## 6. Decision for the owner

V4′ passes the deterministic KILL-1 and its native BENEFIT was met in R19 run02. Open before a merge: (a) approve the
merge as a `perf(runtime)` change of the registry claim path (then a full `npm run check` on the merged tree and a CI
confirmation after a push are required), keeping the native R-live +19.08 % as a standing caveat to re-measure on a quiet
host; or (b) keep V4′ on its branch until the native axes can run. This report does not decide.
