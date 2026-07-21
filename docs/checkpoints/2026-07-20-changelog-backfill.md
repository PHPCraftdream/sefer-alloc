# Checkpoint — 2026-07-20 [CHANGELOG backfill in progress, R10 tasks queued]

## Session summary

`sefer-alloc` (D:\dev\rust\sefer-alloc), 100%-Rust allocator. This window opened
with `/resume` loading the prior checkpoint (`2026-07-20-r9-complete.md`), which
confirmed Round 9 (#224-#232) was fully done, reviewed, and pushed, with a single
unpushed checkpoint commit (`d26e042`) on top.

The user then asked to review an external `/fm` review of the R9 work. I
independently verified every claim in it against source (grep/read, not trust) and
found it substantively accurate: the `LARGE_ZERO_PASS_CALLS` counter added by R9-1
is genuinely un-gated (no `alloc-stats` cfg), medium-classes' native wall-clock
regression is genuinely unresolved (single noisy run, not proven noise), R9-9's
batch NO-GO genuinely lacks a warm-batch arm, R9-4's 1.5/1.75 MiB "consolation
prize" genuinely ignores the Large cache-hit path, and the R9-6 `changed_classes`
metric genuinely over-counts (sets the bit even for rejected ring entries). Also
confirmed the NUMA-directory-disabled and alignment-tax findings from the "further
radical speedup" section by reading the actual code.

The user asked what a Round 10 would contain; I proposed 7 tasks (R10-1 through
R10-7, priority order: gate the counter → fix the R9-6 metric → honest native A/B/B/A
gate for medium-classes → run-origin-oracle design+prototype to remove the
alignment tax → Large-cache-hit comparison for 1.5/1.75 MiB → NUMA directory
judge+design → batch API INCONCLUSIVE + warm-batch arm) and, on explicit request
("Заведи таски"), created tasks **#233-#239** with a sequential `blockedBy` chain
matching that priority order. None of these R10 tasks have been started — they are
still `pending`, waiting on a future explicit go-ahead.

Sidetrack 1: user asked about running `npm run bench:table` at normal (not idle) OS
process priority in a separate branch/process, "one command". I was mid-diagnosis
(a broken `powershell -Command "(Get-Process -Id $PID)..."` probe — `$PID` came
back empty in this Bash-tool's non-interactive shell) when the user redirected to a
higher-priority task; **this priority-probe thread was never resolved, no working
one-liner was produced**. If revisited: the likely correct shape is `cmd.exe /c
start "" /normal /b /wait cmd /c "npm run bench:table > out.log 2>&1"` (the `start`
built-in's own `/normal` priority switch, set at `CreateProcess` time, avoids the
race a `Start-Process -PassThru; $p.PriorityClass=...` approach would have), but
this was NOT tested end-to-end this session.

Sidetrack 2: user pasted a fresh `npm run bench:table` run (apparently produced
externally, exactly the kind of run the priority discussion was about — never
confirmed how they ran it). I synced **README.md**'s three headline wall-clock
tables (Cold-direct, Churn+write, Churn non-writing) plus every cross-referencing
prose paragraph (the "Honest verdict" bullets, "Where we still trail", the Cold
first-touch mini-table) to the new 2026-07-20 numbers — this mirrors the project's
own documented PERF-1 precedent (grep the whole file for every stale occurrence,
not just the headline table). **This README.md diff is still UNCOMMITTED** — it
was produced via direct `Edit` calls (not delegated), personally written, but not
yet reviewed-and-committed as a discrete step; it's just sitting in the working
tree. The user then asked about an apparent anomaly (`Churn + teardown` 1024B
showing SeferAlloc 2.24× *slower* than mimalloc, inconsistent with "we always win
big at large sizes") — I traced this to a deliberate, documented diagnostic bench
(`bench_global_alloc_churn_with_teardown`, doc comment at
`benches/global_alloc.rs:628-637`, NOT the headline metric, never added to
README) and, as a genuine drive-by fix, corrected a stale line-reference in
**`scripts/bench-table.mjs:55`** (pointed at `460-469`, the unrelated `Vec_push`
body; corrected to `628-637`). **This bench-table.mjs one-line fix is also still
UNCOMMITTED.**

The user then asked "did we improve performance across recent rounds" — I answered
using the ALREADY-EXISTING `docs/perf/R8_CROSS_VERSION_BENCH.md` (this file is
actually the R9-2 report, named R8 for historical reasons) rather than naively
diffing two noisy single-host runs: the real, controlled 0.2.1-vs-HEAD comparison
shows genuine wins (churn 64B-1024B ~1.45-2.03× faster, decommit-cycle ~292×,
working-set-cycle up to ~3.44×), but the report's own conclusion is that **Round 7**
is the source of essentially all of it — Round 8 was correctness/opt-in/
constant-factor work that didn't clear the noise floor on the default bundle, and
Round 9 (mostly research/design/diagnostic) didn't touch the hot path at all except
for R9-1 (a correctness fix, not a speedup) and R9-8 (a worst-case-bound
improvement, not a hot-path speedup).

**The main thread of this window**, still in flight: the user asked to bring
CHANGELOG.md up to date ("актуализируй changelog и доки"). Investigation (via
`git log --follow -- CHANGELOG.md`) found CHANGELOG.md had not actually been
touched since commit `087766e` (deep in Round 6, `R6-CQ-4` — NOT the more recent
"CI fixes" section a naive reading of the file's own top-of-Unreleased content
might suggest) — a **101-commit gap** to HEAD, spanning the tail of Round 6, all of
Round 7, all of Round 8, and all of Round 9. Zero breaking-change-marked commits
(`!:`) exist in this range, so no migration-note sections are needed. The user
explicitly chose (via `AskUserQuestion`): (1) full detailed style matching the rest
of the file (not a compressed summary), and (2) delegate the writing to `/crush`
(explicitly confirmed given the volume, per the standing "never invoke sub-agents
without explicit request" rule).

I split the 101-commit gap into 4 sequential sections with precise commit-range
boundaries (verified by reading the full oldest-to-newest commit list personally
before delegating) and created **TaskList #240-#243** with a `blockedBy` chain
(240→241→242→243, one `/crush` session at a time since all four write the same
CHANGELOG.md file — no worktree isolation used, sequential is safe and was judged
sufficient here):
- **#240 R6-tail** (`087766e..461fe8f`) — **DONE.** `/crush` session
  `changelog-r6tail` produced a 345-line insertion (0 deletions). I personally
  spot-verified 5 load-bearing numeric claims against the actual commit bodies via
  `git log -1 --format=%B <hash>` (unsafe-seam count staying at 46 across the whole
  range — verified via a real `grep` at both `087766e` and `461fe8f`; the
  ~129.5→5.98→4.52 MiB / ~28.6× commit-charge chain; "4,420 CPU-seconds over ~4
  minutes"; the loom-model-count "13→16" doc fix) — all matched verbatim. Committed
  as `6035e8e`. Task #240 marked `completed`.
- **#241 R7** (`c0c011f..c815927`) — **IN FLIGHT.** `/crush` session `changelog-r7`
  was launched (background, 60 min ceiling) right before this checkpoint was
  requested; **no result has been reviewed yet**. The prompt explicitly named the
  two NO-GO verdicts in this range that must not be dropped (incremental/lazy-commit
  B0-B5's NO-GO on first-heap commit, later superseded by the separate B6 win; and
  the ring-mpsc CRATE-P4-followup in-tree-swap verified NO-GO) and asked the final
  report to explicitly confirm both were captured.
- **#242 R8** (`af7b039..68f5da7`) — not started, blocked on #241.
- **#243 R9** (`860d897..d26e042`) — not started, blocked on #242. The task
  description already contains a full pre-verified fact sheet (all 9 R9 sub-round
  outcomes with their exact verdicts/numbers) since the orchestrator (this session)
  already has this from firsthand review earlier — a future continuation could
  plausibly write this section directly instead of delegating, since delegation
  would just be re-deriving already-known facts.

## Active goal

None. No `/goal` Stop hook is armed.

## TaskList

### in_progress
- #241 CHANGELOG: Round 7 секция (c0c011f..c815927) — `/crush` session
  `changelog-r7` running in background, not yet reviewed.

### pending
- #233 R10-1: гейтировать LARGE_ZERO_PASS_CALLS под alloc-stats
- #234 R10-3: исправить changed_classes метрику в R9-6 dirty routing (blockedBy #233)
- #235 R10-2: честный native A/B/B/A gate для medium-classes + realloc kill-gate (blockedBy #234)
- #236 R10-4: design + prototype run-origin oracle, снять alignment tax в carve_block (blockedBy #235)
- #237 R10-5: production wall-clock judge для 1.5/1.75 MiB против Large cache-hit (blockedBy #236)
- #238 R10-6: NUMA-aware directory judge + design (blockedBy #237)
- #239 R10-7: batch API — INCONCLUSIVE вердикт + warm-batch arm (blockedBy #238)
- #242 CHANGELOG: Round 8 секция (af7b039..68f5da7) (blockedBy #241)
- #243 CHANGELOG: Round 9 секция (860d897..d26e042) (blockedBy #242)

### recently completed
- #240 CHANGELOG: R6-tail секция (087766e..461fe8f) — committed `6035e8e`

## Decisions

- CHANGELOG backfill: detailed per-round style (not compressed), delegated to
  sequential `/crush` sessions (one at a time, same file — no worktree
  parallelism), 4 sections split at exact round boundaries the orchestrator
  identified personally before delegating.
- R10 task queue (#233-#239) created and ordered by priority, but deliberately
  **not started** — no `/babygoal`/`/babysit` cron armed for it; it waits for an
  explicit future go-ahead, per this session's standing "no sub-agent work without
  explicit request" rule.
- README.md wall-clock table sync and the `bench-table.mjs` stale-line-reference
  fix were done personally (direct `Edit`, not delegated) since they were small,
  mechanical, and the orchestrator already had the exact numbers in hand from the
  user's pasted bench output.
- Did not resolve the Windows-process-priority ("run bench:table at normal
  priority in one command") question — deprioritized mid-investigation in favor of
  the CHANGELOG request; no working command was produced.

## Open questions

- The Windows normal-priority one-shot bench-run command is still unresolved (see
  Session summary, Sidetrack 1). Likely shape sketched there but untested.
- Whether to commit the README.md + scripts/bench-table.mjs changes now sitting
  uncommitted in the working tree, or fold them into a later commit — not yet
  decided; they predate the CHANGELOG work and are unrelated to it, so they
  probably warrant their own small commit rather than being bundled into a
  CHANGELOG-section commit.
- Task #243's description already contains a full pre-verified R9 fact sheet;
  worth deciding at that point whether to still delegate to `/crush` or have the
  orchestrator write it directly (would save one delegation round-trip and one
  round of zero-trust re-verification, since the facts are already verified).

## Repo state

```
 M README.md
 M scripts/bench-table.mjs
?? _gad_suite.log
?? _r7_commits_dump.txt
?? ci_watch.log
?? ci_watch2.log
?? ci_watch_ffd3215.log
?? docs/perf/_raw_baseline_off.log
?? docs/perf/_raw_baseline_off_reduced.log
?? docs/perf/_raw_criterion_medium.log
?? docs/perf/_raw_criterion_production.log
?? docs/perf/_raw_firstalloc_medium.log
?? docs/perf/_raw_firstalloc_production.log
?? docs/perf/_raw_iai_medium.log
?? docs/perf/_raw_iai_production.log
?? docs/perf/_raw_medium_on.log
?? docs/perf/_raw_medium_on_reduced.log
?? docs/perf/_raw_r9_9_followup.log
?? docs/perf/_raw_r9_9_followup_run2.log
?? docs/perf/_raw_r9_9_followup_run3.log
```

```
6035e8e docs(changelog): document Round 6 tail (R6-CQ-5..7, R6-OPT-A1..A6, R6-OPT-P0-1..4, R6-REGRESSION)
d26e042 docs(checkpoints): land Round 9 completion checkpoint
5e467ec perf(bench): batch-alloc ceiling follow-up - small batches + real SeferAlloc arm (R9-9)
78fd98d fix(tests): account for medium-classes-wide under --all-features (R9-4 follow-up)
5a4ba62 fix(directory): per-class miss-streak + OOM-rescue scan for drift recovery (R9-8)
```

Local `main` is one commit ahead of `origin/main` (`6035e8e`, unpushed — the user's
"push only at the very end, after all tasks" instruction from earlier in this
session's lineage still applies; no push has been requested for this work yet).
The untracked `_raw_*.log`/`ci_watch*.log`/`_r7_commits_dump.txt` files are scratch
output, intentionally not committed (same established convention as prior
checkpoints).
