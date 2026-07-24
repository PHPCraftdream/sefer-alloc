# Checkpoint — 2026-07-24 08:30 [r16-complete]

## Session summary
This is the same long-running session that completed Round 13, 14, and 15.
Round 16 (tasks #311-#316, R16-1..R16-6) executed the follow-up queue
against an `@fh` review of the Round 15 wave, per the user's standing
instruction to keep iterating past each review cycle. Every task followed
the session's standing zero-trust discipline: delegate to `@sh` sub-agents
(except two small, well-scoped items — R16-2's one-line doc fix and R16-5's
comment+accessor — done directly), personally read every diff, personally
re-run tests under the exact feature combinations each change targets,
personally reproduce red/green counterfactuals where relevant, run clippy
across default/experimental/`--all-features` + fmt + `no_stale_doc_references`
before marking each task complete.

**Review verification before filing tasks:** personally verified the two
highest-severity Round 15 review findings against source BEFORE trusting
the rest and filing tasks: (1) `grep -n "medium-classes" .github/workflows/ci.yml`
confirmed zero CI coverage for the promotion mechanism (P1-1); (2)
`grep -n "^production = \[" Cargo.toml` confirmed `class-aware-dirty`/
`alloc-segment-directory` ARE in `production`, proving the review's CHANGELOG
factual-error claim (P1-2) correct — fixed immediately as a standalone
hotfix commit (`ed8f955`) before starting the Round 16 task queue proper.

**All 6 planned tasks (#311-#316) completed, personally verified, committed:**

- **R16-1** (`5f45b37`+`4a3ab19`, #311, P1, most substantive): restored CI
  coverage for the `medium-classes` realloc promotion mechanism — two new
  feature-isolation rows exercising both sides of R15-3's `#[cfg]` split,
  which had left the mechanism entirely untested per-PR since R15-3 landed.
  Personally re-ran both new combinations locally across all four relevant
  test files, confirmed the non-early-return branches genuinely execute.
- **R16-2** (`87d0412`, #312, P2): fixed a 58/55-class labeling error R15-5
  itself introduced in `dirty_by_class.rs` — done directly (trivial,
  arithmetic already verified by hand).
- **R16-3** (`7d082ad`, #313, P2): retroactively added the missing
  machine-readable summary CSV for R15-1's report — done directly.
- **R16-4** (`afa6b1d`, #314, P2): a `callgrind_annotate`/`objdump` diagnosis
  root-caused R15-1's flat +61,4xx Ir bootstrap-cost delta to two
  `MAX_SEGMENTS`-scaled zero-fill loops in `bootstrap::primordial()` —
  personally re-verified the byte-delta arithmetic reconciles exactly to
  the observed Ir delta (49,152 + 12,288 = 61,440).
- **R16-5** (`eedc111`, #315, P3): fixed a self-contradicting comment plus
  added a `dbg_promotion_compiled()` cross-check accessor and canary test —
  done directly. **During this task's verification, discovered a one-off
  test failure** (`regression_r4_3_teardown_trim.rs`) under heavy background
  CPU load, confirmed via `git stash`+bisect that it predates Round 16 and
  is unrelated to any Round 16 change — filed as #316 rather than silently
  ignored.
- **R16-6** (`56ed79f`, #316, P3, flake investigation): traced the
  production release path end-to-end, ruled out several hypotheses, could
  not reproduce under 450+ stress iterations — documented honestly as an
  open question rather than papered over with a retry/sleep band-aid.

**No feature-list change this round** — `Cargo.toml`'s `production = [...]`
unchanged. Unsafe-seam inventory unchanged: 76 total (20 tier-1, 56 tier-2).

**Standing promotion question resolved:** asked the user via
`AskUserQuestion` whether to keep R14-6's clean-GO `large-reserved-capacity`
growth-factor-4x fix as-is (the one hanging promotion decision carried
across Rounds 14-15) — user confirmed keep as-is. No code change (already
shipped at 4x); this closes the standing open item.

**CHANGELOG updated** (this task, #317) — full Round 16 entry matching the
established Round 14/15 format.

**Not yet done as of this checkpoint:** commit CHANGELOG + this checkpoint,
`npm run check`, push to `origin/main`, watch CI, then decide whether to
launch another review cycle or pause and report status to the user (Round
16 was itself entirely review-driven — mostly closed process/doc findings
from Round 15's review — worth checking in with the user rather than
automatically spinning up a Round 17 review with diminishing findings).

## Active goal
Same standing situation as prior checkpoints: a `/goal` Stop hook is
nominally active with text mentioning `@fm` (long superseded by the user's
`@fh` override and the "keep iterating" follow-up instruction). Session
continues on the user's direct instruction, not the original literal goal
text.

## TaskList
### in_progress
- #317 Round 16 wrap-up: CHANGELOG (done), checkpoint (this file), commit,
  push, npm run check — still pending

Note: the live TaskList was found completely empty (no tasks at all,
including previously-completed ones) immediately before #317 was created —
an unexplained side effect, not a data-loss event (all Round 16 work is
intact and verified in `git log`). Re-created #317 to track the wrap-up.

## Decisions
- R16-1 was prioritized first (P1) per the review's own recommendation —
  it was the only finding where a real behavioral/compile regression could
  live unnoticed until a future manual run of the right feature combination.
- R16-4's diagnosis found and corrected an inaccuracy in R15-1's own root-
  cause attribution (checked the wrong call frame) rather than merely
  confirming the original hypothesis — the report was corrected in place
  (struck through, not deleted) with the new evidence.
- R16-6 deliberately closed as an honest open question rather than forcing
  a fix — matches this project's established precedent (`tls_heap_teardown_ordering_stress.rs`'s
  own "Honesty about what this test is" section) for load-sensitive flakes
  that resist time-boxed investigation.
- Two of the six tasks (R16-2, R16-5) were done directly rather than
  delegated to `@sh` — both were small enough (one-line fix; comment +
  accessor + canary test) that delegation overhead would have exceeded the
  work itself.

## Open questions
- None outstanding from the user's side. The R14-6 promotion question
  (carried since Round 14) was resolved this session via `AskUserQuestion`.
- Process question for the orchestrator, not the user: whether continuing
  to spin up review→task-queue cycles indefinitely (Round 17, 18, ...) is
  still the right cadence now that findings have shifted from P1 substantive
  gaps (R14-9's sidecar hole, R15-3's promotion pessimization) toward P2/P3
  process hygiene (doc-drift, missing CSVs, stale comments) — worth
  surfacing to the user after this wrap-up rather than deciding unilaterally.

## Repo state
```
 M CHANGELOG.md
```

```
56ed79f docs(tests): document known flakiness in regression_r4_3_teardown_trim under load (R16-6, task #316)
eedc111 chore(registry): fix stale promotion cfg comment + add HAS_PROMOTION cross-check accessor (R16-5, task #315)
afa6b1d docs(perf): callgrind_annotate diagnosis of R15-1's +61.4K Ir flat delta (R16-4, task #314)
7d082ad docs(perf): add retroactive machine-readable summary for R15-1 (R16-3, task #313)
87d0412 docs(alloc-core): fix 58/55-class footprint arithmetic in dirty_by_class.rs (R16-2, task #312)
4a3ab19 test(r14-4-promotion): correct docs for the R15-3 HAS_PROMOTION split (R16-1, task #311, P2-1)
```

Local `main` is 10 commits ahead of `origin/main` (`ed8f955`..`56ed79f`,
Round 16's full range plus the hotfix) — NOT yet pushed. This wrap-up task
(#317) will run `npm run check`, commit CHANGELOG+checkpoint, push, and
watch the resulting CI run.
