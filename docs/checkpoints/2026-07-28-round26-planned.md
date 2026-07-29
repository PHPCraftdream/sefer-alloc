# Checkpoint — 2026-07-28 15:35 [round26-planned]

## Session summary
Continuation of the zero-trust review/fix cycle on sefer-alloc (100%-Rust
allocator). This session shipped Round 25 in full (10 tasks, #395-404,
commits `667cfe7`..`a6fbaf2`), then ran the standard wrap-up sequence the
user requests at the end of every round: updated CHANGELOG.md with a new
"### Round 25" section (0 runtime improvements — every task was a
correctness fix, measurement, design, or docs/process work), wrote
`docs/checkpoints/2026-07-28-round25-complete.md`, committed all outstanding
markdown (`9ddb062`), pushed, and started investigating CI health (task
#408, still in progress at this checkpoint — see below).

**Mid-CI-investigation, the user (via `/oh`) delivered a fresh independent
read-only review of Round 25 itself**
(`docs/reviews/2026-07-28-r25-readonly-review.md`) and asked me to study it
and plan Round 26. I read it in full and personally re-verified its central
(P0) claim against source before accepting it, per this project's zero-trust
convention — **the claim held, and it identifies a real methodological bug
I personally missed during R25-5's own zero-trust review**:

`examples/r25_5_pool_cap_sweep_probe.rs`'s RSS/multi-thread axis
(`measure_rss_axis`, spawning threads that call `SeferAlloc::with_config`)
does NOT verify the resolved pool cap. This allocator's registry
deliberately keeps first-materialisation-wins config for a slot's whole
process lifetime (`src/registry/heap_registry.rs:248-251`: "the slot's
existing config... silently wins" on a recycled-slot re-claim; `:263`
increments a `CONFIG_CONFLICTS` counter; `:285`'s `debug_assert!` is
compiled OUT of release builds — and the probe was run via `cargo run
--release`, so the loud signal was silent). R25-5 ran caps 4→8→16→32
sequentially IN ONE PROCESS, at 1/8/32 threads each; when an arm's threads
exit, `HeapRegistry::recycle` pushes their already-configured slots onto
`free_slots`, and the NEXT arm's threads can pop those recycled slots and
silently keep the OLD config. So rows labelled cap=8/16/32 may have all
actually run cap=4 — which would perfectly explain the "cap=8/16/32 are
statistically flat with each other" result the R25-5 report treated as
evidence of a real demand plateau. I verified this by reading the actual
registry source myself (not taking the review's word for it), and
separately confirmed the LATENCY axis is unaffected (it uses `AllocCore::
new_with_config` directly, no registry, and self-verifies via
`assert_eq!(resolved_cap, pool_segments)` at `measure_latency_axis`'s own
line ~375-376) — so "cap 8 eliminates the decommit cliff" survives, but
"cap 8 is also cheaper on RSS" does not, and R25-6's closure (which rested
entirely on the RSS axis showing no tradeoff) is now unsupported and must
be reopened.

**This is now a confirmed, real defect that reached a pushed `main` and
propagated into 5 documents** (the R25-5 report itself, its summary CSV,
`OPEN_ITEMS.md` item 13, `CHANGELOG.md`'s Round 25 section, and this same
session's own `docs/checkpoints/2026-07-28-round25-complete.md`) plus the
R25-6 closure commit's own reasoning. None of these corrections have been
made yet — Round 26 is filed but not started.

I also independently re-verified several of the review's OTHER claims (the
STAGE_CAP=64 validation surviving to N=1024, the FLUSH_N=8 rejection
holding, the NUMA-directory closure being correct) and did not find
problems with those — the review's P0 finding is the one substantive new
defect, not a wholesale re-litigation of Round 25.

**Filed 9 Round 26 tasks (#410-418)** with explicit priority and
`blockedBy` dependencies matching each task's own trigger condition (full
list in TaskList below). None have been started yet — this checkpoint was
requested immediately after filing, before any Round 26 work began.

**Task #408 ("наладь ci") is still genuinely in progress** at the moment of
this checkpoint: three `gh run watch` background processes are tracking the
CI runs for the last few Round 25/wrap-up commits (all still `in_progress`
on GitHub Actions as of this write — they typically take ~30 min), and a
`npm run check` local run is also still executing (currently in its test
suite phase). Real, confirmed CI failures WERE found on 4 historical
commits (R25-3 through R25-6, `0465c97`..`6a75874`) — all `E0601 main
function not found` on two example probes (`r25_3_flush_n_oscillating_
probe.rs`, `r25_5_pool_cap_sweep_probe.rs`) that were missing a `Cargo.toml
[[example]] required-features` entry every other diagnostic example in
this project already has. This was ALREADY fixed in R25-7's own commit
(`2148efc`, confirmed via that commit's own CI run showing `success`) —
so the historical red marks are on now-superseded commits, not the current
HEAD. The three background watchers are the last confirmation step that
HEAD itself (`9ddb062`) and the two most recent Round 25 commits are
genuinely green in real CI, not just locally.

## Active goal
None. The `/babygoal`-armed `babysit` cron from the Round 25 work session
already self-deleted once that round's TaskList emptied (confirmed via a
real babysit tick reporting "TaskList empty — babysit done"). No new
goal/babysit has been armed for Round 26 — the task queue exists but
execution has not started, and no standing directive is currently forcing
this session to continue past a stop.

## TaskList
### in_progress
- #408 Investigate and fix CI ("наладь ci") — 4 historical CI failures
  confirmed root-caused and already fixed by R25-7; 3 `gh run watch`
  processes + 1 `npm run check` run still confirming current HEAD is green

### pending
- #410 R26-1 (P0): rebuild the pool-cap RSS gate with subprocess-per-arm
  isolation
- #411 R26-2 (P0): correct the invalid R25-5 RSS claim across all 5
  documents that now carry it
- #412 R26-3 (P1): production-shaped teardown A/B/B/A for pool cap 4 vs 8
  (blockedBy: #410)
- #413 R26-4 (P1): make configuration identity a required field of every
  benchmark result (blockedBy: #411)
- #414 R26-5 (P2): strengthen the batch multi-flush oracle from aggregate
  live_count to per-offset state
- #415 R26-6 (P2): tighten `dbg_overflow_bitmap_clear_pass`'s `# Safety`
  contract to match its actual caller
- #416 R26-7 (P2): safe lazy batch-staging prototype for
  `dealloc_batch_small`
- #417 R26-8 (P3): replace `scripts/lib.mjs`'s manual shell quoting with
  direct argv execution
- #418 R26-9 (Conditional on R26-1): adaptive/process-wide pool budget
  design (blockedBy: #410)

### recently completed
- #409 Push to origin/main
- #407 Commit all outstanding markdown files
- #406 Run /checkpoint (the Round 25 one, `2026-07-28-round25-complete.md`)
- #405 Update CHANGELOG.md for Round 25

## Decisions
- **Deliberately left #411 (fix the invalid documents) NOT blocked by #410
  (rebuild the measurement)** — the documents on pushed `main` currently
  state an unproven claim as established fact right now; that gets
  corrected immediately rather than waiting for new measurements, following
  this project's append-correction convention (downgrade the claim, don't
  wait for the replacement data to also be ready).
- **Did not file the review's "automate the benchmark-hook policy" (a
  source/API linter) or "reduce report duplication" recommendations as
  Round 26 tasks** — judged the first as already reasonably covered by
  R25-10's written rule + the ZERO-TRUST checklist item (a linter is
  separate infra with unproven payoff), and the second as not a discrete,
  actionable task (R24-9's current-state-first effort already is that
  ongoing work, not a new one-off).
- **Personally re-verified the review's P0 claim against source before
  accepting any of it**, rather than trusting a review's own confident tone
  — this is the same zero-trust discipline applied to every delegated
  agent's output this session, extended to an unsolicited external review
  too. The verification (reading `heap_registry.rs`'s actual re-claim
  logic and the probe's own two axes) took a few direct tool calls and
  fully confirmed the claim before any task was filed on it.

## Open questions
None from the user's side. Operationally: task #408's three CI watchers
and the local `npm run check` run have not yet reported back — the next
step once they do is to confirm current HEAD is fully green (expected,
given R25-7's fix and its own passing CI run) and close out #408, then
begin Round 26 starting with #410/#411 (the two P0s, per the user's
established pattern this session of working the queue in priority order,
typically via the actual `crush` CLI when its `zai` provider isn't in a
peak-hours window).

## Repo state
```
?? .claude/
?? docs/reviews/2026-07-28-r25-readonly-review.md
```
(This checkpoint file itself, `docs/checkpoints/2026-07-28-round26-planned.md`,
will also show as untracked once this write completes — not yet committed,
per this skill's own "do NOT add the file to git automatically" instruction.)
```
9ddb062 docs: Round 25 CHANGELOG entry, session checkpoint, R24 readonly review
a6fbaf2 docs(perf): close R25-9 -- the NUMA directory cliff it targets was already fixed in R11-6 (task #403)
1b5479e docs(perf): run-encoded free batch design -- CONDITIONAL-GO, but not for the region that motivated it (R25-8, task #402)
52fafe0 docs(claude): codify the benchmark-only dbg_* hook safety rule R25-1 fixed one instance of (R25-10, task #404)
2148efc bench(perf): STAGE_CAP=64 confirmed clean at every measured N up to 1024 (R25-7, task #401)
```
Local `main` and `origin/main` are in sync (last push was this session's
`9ddb062`) — no outstanding push. `docs/reviews/2026-07-28-r25-readonly-
review.md` is untracked (delivered mid-session, not yet committed — it will
naturally get swept up in whichever Round 26 task's commit touches
`docs/perf/` first, or committed standalone if none does before the next
wrap-up cycle).
