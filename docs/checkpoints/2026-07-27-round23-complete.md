# Checkpoint — 2026-07-27 12:10 [round23-complete]

## Session summary
Continuation of a long-running zero-trust review/fix cycle on sefer-alloc
(100%-Rust allocator). Session opened by resuming checkpoint
`2026-07-27-round23-queued.md` (Round 22 fully shipped and pushed through
`bc4aacf`; two more commits, `897aa26`/`6b4ac50`, sitting local-only from
before this session; Round 23's 7 substantive tasks — #370-376 — filed but
not yet started). The user then ran `/goal` (interrupted) followed by
`/babygoal` with the same instruction ("continue solving tasks via @sh
agents, commit between tasks"). Domain was already understood (no
investigation needed), so I went straight to arming `/babysit` (15 m
interval, cron job `81b07973`) and executing Round 23's 7 tasks
SEQUENTIALLY via the `Agent` tool (`subagent_type: sh`), each followed by
full personal zero-trust review (reading every diff, re-running
tests/clippy/fmt myself, and for two of the tasks personally constructing
and running my own mutation counterfactual rather than just trusting the
sub-agent's claimed one) before committing.

All 7 tasks landed as 7 separate commits:
- **R23-1** (`7f2a9ef`): fixed `contains_base`'s measurement isolation —
  the true isolated share of a real free's `Ir` is 8.8%, not R22-17's
  original 18.6% (which mixed in `segment_base_of_ptr` + call overhead).
- **R23-2** (`3cf2d66`): built a warm N/2N matched-workload gate to replace
  R22-15's asymmetric bootstrap-subtraction methodology for the
  Sefer-vs-mimalloc `Ir` ratio. Result MATERIALLY changes the headline, not
  just the decimals: hot-churn ratio flips from 1.326 to 0.896 (SeferAlloc
  becomes marginally CHEAPER than mimalloc per op), cold-carve shrinks from
  2.430 to ~2.0-2.08.
- **R23-3** (`315aa8a`): full orthogonal hot-path attribution. Headline
  finding: the free path's dominant cost is the own-thread free BODY (M2
  double-free oracles fused with the magazine push), 80.8% of a real
  free's `Ir` — more than 4x the routing prefix R22-17/R23-1 isolated.
  This revises the prior "cold-carve/recycle is the main remaining
  candidate" framing (recycle-pop turned out roughly on par with
  virgin-carve once matched to the same denominator). Added 4 new
  `#[doc(hidden)]` measurement hooks in `src/`. Caught and fixed a REAL
  test regression along the way: adding one new `unsafe fn` bumped the
  tier-2 unsafe-seam count 60→61, and README.md's own
  `readme_unsafe_inventory_counts_match_reality` tripwire test correctly
  went red until the README count was updated in the same commit.
- **R23-4** (`de7213d`): corrected my OWN R22-16 design doc's flawed
  "promotion-time neighbor-liveness check" blocker for Linux sub-region
  `mremap`. Personally re-derived (read `carve_block`/`carve_batch`'s
  forward-only bump advance and `decommit_empty_segment_impl`'s
  empty-only-reset gate myself, independently of the dispatched agent, both
  before AND during delegation) that a live carved block's byte range is
  provably exclusive for its whole lifetime — no runtime check needed.
  Verdict revised: NO-GO for whole-segment remap (unaffected); NO-GO for
  Windows (separate section-object-backing blocker); CONDITIONAL-GO for
  Linux sub-region remap specifically. Also surfaced a genuinely new,
  still-open nuance: today's memcpy-based promotion frees the source block
  through the ordinary `dealloc`→`BinTable` free-list path, so a future
  remap design must avoid routing a remap-vacated offset through ordinary
  free — monotonicity alone does not solve this "permanent hole" question.
- **R23-5** (`4a4500a`, message later corrected via `--amend` — the
  original had shell backtick-substitution damage): closed all 11
  pre-existing `cargo clippy --features "hardened medium-classes" -D
  warnings` dead-code errors, stable since R19-1. All 11 were genuine
  `#[cfg(...)]` predicate mismatches (item gated one way, sole consumer
  gated a different way); none were true orphans, nothing deleted. Added a
  CI clippy row for this combo, closing a gap R22-1 deliberately left open.
- **R23-6** (`37393fe`): replaced one of two flaky coarse wall-clock tests
  with a deterministic `alloc-stats`-gated scan-step counter
  (`HASH_REMOVE_MAX_SCAN_STEPS`) for `SegmentTable::hash_remove`'s
  backward-shift scan — verified non-vacuous via a mutation counterfactual
  I personally constructed and ran myself (not just trusting the agent's
  claim), independently of the one the sub-agent also ran. The second test
  (`own_thread_free_is_subquadratic`) has NO clean deterministic
  replacement — the guard it protects is an unconditional O(1) bitmap test
  with no loop to instrument — honestly demoted to `#[ignore]` rather than
  forcing a fake counter. A prior mutex-based fix proposal (from an earlier
  review) was correctly NOT used — a mutex only serializes within one test
  process, but the flakiness source is cross-process CPU contention.
- **R23-7** (`eb6c392`): decision-only — investigated whether a more
  realistic batch-API benchmark than what already exists (R10-7's
  `batch_tcache` arm, measured against the real warm `SeferAlloc` scalar
  path) could be cheaply built; concluded no, wrote a decision record with
  an explicit 3-trigger falsifiability clause instead of building a
  redundant 4th-generation microbench. Also caught and fixed, in the same
  pass, a 12-round-stale `OPEN_ITEMS.md` item (R9-9's warm-batch-arm ask,
  actually resolved by R10-7 the very next round, but never marked closed).

A recurring operational anomaly was observed and handled three times this
session: tasks #371, #372, and #374 each flipped to `completed` status on
their own, apparently mid-flight while their dispatched `Agent` sub-call
was still running (before I had reviewed/committed anything) — most likely
the `/babysit` cron tick firing during a long (20-40 min) foreground
`Agent` call and misreading state. Each time, I caught this via `TaskList`/
`TaskGet` before trusting it, reverted the status to `in_progress`,
completed my own zero-trust review and commit, and only then re-marked it
`completed`. This did not affect correctness of the actual work, only
task-tracking hygiene — flagged here in case the pattern recurs and needs
root-causing later.

After Round 23's TaskList emptied, the babysit cron's own tick prompt fired
(`# babysit tick`), correctly observed `pending + in_progress == 0`, found
its own job id (`81b07973`) via `CronList`, and self-deleted via
`CronDelete` per its own designed stop condition — reported "TaskList
empty — babysit done." No heartbeat is currently armed.

## Active goal
None. The `/babygoal` heartbeat (babysit cron `81b07973`) self-deleted once
Round 23's TaskList emptied, per its own designed stop condition. No
`/goal` Stop hook is in force.

## TaskList
Empty — all 9 items from this session (#370-378, i.e. Round 23's 7
substantive tasks plus the 2 small fixes already completed before this
checkpoint) are `completed`. `TaskList` currently returns "No tasks found."

### recently completed (this session, chronological)
- #370 R23-1: fix contains_base measurement isolation
- #371 R23-2: warm N/2N matched Sefer-vs-mimalloc gate
- #372 R23-3: split hot alloc/free into orthogonal attribution arms
- #373 R23-4: reopen Linux sub-region mremap as CONDITIONAL-GO
- #374 R23-5: close 11 clippy dead-code errors under hardened+medium-classes
- #375 R23-6: fix/demote the two flaky coarse-wall-clock tests
- #376 R23-7: batch API downstream-consumer decision record
- #377 R23-8 (completed earlier, pre-dates this session's active work window)
- #378 R23-9 (completed earlier, pre-dates this session's active work window)

## Decisions
- Executed Round 23's 7 tasks strictly SEQUENTIALLY (one `Agent` dispatch
  in flight at a time), not in parallel — each needed full personal
  zero-trust review before the next could safely build on a clean tree.
- For R23-4 (correcting my own prior R22-16 error) and R23-6 (verifying a
  new test's non-vacuousness), personally re-derived/re-ran the key
  verification step myself rather than relying solely on the dispatched
  sub-agent's own claimed verification — caught nothing wrong in either
  case, but this was a deliberate belt-and-suspenders choice given the
  stakes (R23-4 corrects a previously-committed conclusion; R23-6 adds a
  new test whose only value is that it's non-vacuous).
- R23-3, R23-5, R23-6, R23-7 each independently CHOSE to fix a real
  problem found incidentally mid-task (README unsafe-count drift, a latent
  test-file predicate mismatch, a 12-round-stale OPEN_ITEMS.md entry)
  rather than deferring it to a new tracked item — judged cheap/adjacent
  enough in each case to fix immediately alongside the task's main scope.
- Chose NOT to force a synthetic "more realistic" batch-API benchmark for
  R23-7 once investigation showed R10-7 already cleared that bar — an
  honest decision record was judged better than a redundant 4th-generation
  microbench, per this project's own explicit house style on the point.
- Amended `4a4500a`'s (R23-5) commit message after noticing shell
  backtick-substitution had eaten several backtick-quoted code spans —
  amend used deliberately (tip commit, not a buried one; the project's
  `git rebase -i` prohibition does not apply to a plain `--amend` of HEAD).

## Open questions
- None blocking. `main` is 9 commits ahead of `origin/main` (through
  `eb6c392`) — intentionally NOT pushed; the standing project rule is that
  push only happens on a separate, explicit user request, and none has
  been made since before this session's work began.
- The recurring "#371/#372/#374 self-completed mid-flight" anomaly (see
  Session summary) was worked around every time it occurred but not
  root-caused — worth investigating if it recurs in a future session, though
  it caused no actual correctness problem this time.
- No new Round 24 task list exists yet — Round 23 is fully closed, and
  nothing has been queued to replace it. The natural next step, absent
  further instruction, would be another `docs/perf/OPEN_ITEMS.md` +
  `docs/CORRECTNESS_OPEN_ITEMS.md` round-start review per this project's
  own convention, but that has not been initiated.

## Repo state
```
?? .claude/
?? docs/checkpoints/2026-07-26-r22-complete-pre-push.md
?? docs/checkpoints/2026-07-26-r22-reviews-in-flight.md
?? docs/checkpoints/2026-07-27-round23-queued.md
?? docs/reviews/2026-07-26-crush-review-r19-r21.md
?? docs/reviews/2026-07-26-oh-review-r19-r21.md
?? docs/reviews/2026-07-26-r19-r21-readonly-review.md
?? docs/reviews/2026-07-26-r22-plan.md
?? docs/reviews/2026-07-26-r22-readonly-review.md
?? docs/reviews/2026-07-27-post-r22-followups-readonly-review.md
```
```
eb6c392 docs(perf): batch API downstream-consumer decision -- no new benchmark needed (R23-7, task #376)
37393fe test(alloc-core): replace flaky wall-clock backshift test with a deterministic scan-step counter (R23-6, task #375)
4a4500a fix(clippy): close 11 pre-existing dead-code errors under hardened+medium-classes (R23-5, task #374)
de7213d docs(perf): correct R22-16's flawed neighbor-liveness blocker -- Linux sub-region remap now CONDITIONAL-GO (R23-4, task #373)
315aa8a diag+bench(perf): isolate free path's dominant cost -- own-thread body, 80.8% (R23-3, task #372)
```
Local `main` is **9 commits ahead of `origin/main`** (through `eb6c392`);
nothing has been pushed this session.
