# Checkpoint — 2026-07-25 22:40 [r17-complete]

## Session summary
Same long-running session that completed Rounds 13-16. After Round 16 the
user was asked whether to continue and chose to STOP; shortly after, the
user pasted a new external review of Rounds 14-16 into `/fm` and asked to
plan/prioritize/file tasks — this reopened the loop. Round 17 executed the
resulting 10-task queue (#318-#327, R17-1..R17-10) against
`docs/reviews/2026-07-24-r17-plan.md`, then closed with a CHANGELOG
wrap-up task (#328). All 11 tasks are now `completed`; the TaskList is
empty.

**Tooling switched delegation three times mid-round, each on explicit
instruction:** `Agent(sh)` → `/crush` (R17-2..R17-4) → `Agent(sh)` (R17-5
onward) → two `Agent(subagent_type="sx")` dispatches for #326/#327 →
`/crush` again for #328's wrap-up. Every switch was honored without
argument; every sub-agent's output (crush envelope or Agent final report)
was treated as a claim, not a receipt — personally re-verified via direct
diff reading, independent test/clippy/fmt re-runs, and (for R17-4, R17-9,
R17-10) an additional counterfactual or cross-check before accepting.

**Round 17's standout finding (R17-4, task #321):** investigating an open
question R14-4's own gate report had left unresolved (why `nopad`/
`floor512kib` got 0 large-cache hits/249 segments while `fixed2mib` got 232
hits/17 segments at an identical 4 MiB `usable`), a real segment-leak bug
was found: `HeapCore::dealloc_own_thread_with_base`'s fastbin magazine
dispatch keyed dealloc routing on `class_for(layout.size())` rather than
segment `kind`, so a Large segment promoted (R14-4) and then grown
in-place (OPT-G) to a small-classifying layout under `medium-classes`
never reached the Large dealloc branch and leaked 4 MiB every occurrence.
The first proposed fix was personally rejected for adding unconditional
hot-path cost to plain `production` (which cannot reach the leak scenario
at all); the revised fix `#[cfg]`-splits the check so `production`'s `iai`
numbers are byte-for-byte unchanged — confirmed directly, not taken on
faith.

**R17-10's design doc (task #327) corrected its own plan's premise**: the
plan's wording asked for "one directory-sync call per segment" as new
design surface, but reading the code showed `sync_directory_for_segment_
classes` has been batched exactly that way since R8-1 (task #214,
pre-dates this round) — not a gap. The genuine, narrower gap is
`dec_live_and_maybe_decommit` still running per-block instead of reusing
an already-proven batched sibling. Personally verified every `file:line`
citation against source before accepting the document, and caught one
factual slip before commit (an earlier draft called R17-4 "task #329,
still open" — it is task #321, already resolved, and touches disjoint
code) — corrected in the doc itself, not silently accepted.

**R17-9's follow-up (task #326) investigated, not just accepted, a
sub-agent's flake claim.** A `STATUS_STACK_BUFFER_OVERRUN` in
`tests/race_repro.rs::drain_reclaim_uaf_repro_tight_handoff` surfaced
during #326's verification. Rather than accept the sub-agent's "unrelated
pre-existing flake" claim at face value (this crash class is a Windows
stack-corruption/security-cookie detector, not an ordinary assertion
failure), a dedicated `@sx` investigation ran 80 process invocations
across three deliberately harsher load profiles (CPU busy-loop stressors,
a real concurrent `cargo check --all-features`, 4-way parallel full-binary
runs) — zero reproductions, confirming (not just repeating) the file
predates Round 17 (`ea3a4ba`, June 2026) and that this is now a second
confirmed one-off occurrence of the exact signature already seen once in
Round 14 (task #289). Documented in the test file's own header (mirroring
the established `regression_r4_3_teardown_trim.rs`/R16-6 precedent)
instead of filing a speculative follow-up task with nothing new to add.

**Anomaly from the prior in-progress checkpoint, now resolved by
context:** commit `5709c24` (#324/R17-7's work) had appeared in `git log`
without a dispatch visible in this session's tool-call history at the time
of the last checkpoint. This remains genuinely unexplained — not
re-investigated further this round since the content was already
independently verified sound and no recurrence has been observed since.
Flagged here again for continuity rather than silently dropped.

**All 12 Round 17 commits (`70a8f2f`..`cbebd45`, plus wrap-up `a99314b`)
are local-only — NOT pushed.** No push has been requested this round.

## Active goal
The most recent `/goal`/`/babygoal` condition in force: "продолжай решать
задачи с помощью агентов /crush, между задачами делай коммиты" (continue
solving tasks via /crush agents, commit between tasks). A `/babygoal`-armed
session-only babysit cron (`b8871125`, every 15 min, off-minute
`7,22,37,52 * * * *`) is active; per its own tick logic it will self-delete
on its next fire since the TaskList is now empty (`pending + in_progress ==
0`). No action needed from the user to clear it — this is expected,
automatic cleanup, not a bug if the job disappears on its own.

## TaskList
### completed (this round)
- #318 R17-1 — `sidecar::reserve_zeroed_with` raw-pointer fixup (`70a8f2f`)
- #319 R17-2 — `os.rs` directory-read helpers → `unsafe fn` (`f65015a`)
- #320 R17-3 — bootstrap zero-loops `cfg(miri)`-gated (`b8612bc`)
- #321 R17-4 — Large-segment leak root-caused + fixed (`1b761f4`)
- #322 R17-5 — pad-comment (closed as side effect of `1b761f4`, no separate commit)
- #323 R17-6 — stale `segment_table.rs`/`register()` literals (`d8f9c9b`+`fbc48a5`)
- #324 R17-7 — class-aware-dirty re-verification (`5709c24`)
- #325 R17-8 — deterministic `trim_for_recycle` oracle (`ea8ff86`)
- #326 R17-9 — large-cache-extended budget 1280→256 MiB + race_repro follow-up (`1117198`+`6b55198`)
- #327 R17-10 — batched deferred reclaim design doc (`cbebd45`)
- #328 — CHANGELOG wrap-up (`a99314b`)

No pending or in_progress tasks remain.

## Decisions
- Rejected R17-4's first-draft fix (unconditional `kind_at` check) for
  adding permanent hot-path cost to `production` for an unreachable
  scenario there — sent back for a `#[cfg]`-split, verified byte-identical
  `production` numbers after the revision.
- Did not treat R17-9's sub-agent flake claim as sufficient on its own —
  dispatched an independent 80-invocation reproduction attempt before
  accepting "environmental flake, second occurrence" as the documented
  conclusion.
- Corrected R17-10's design doc in place (wrong task number, wrong "still
  open" status for R17-4) rather than accepting a plausible-sounding but
  factually wrong cross-reference.
- Did not file a new task for the `5709c24` provenance anomaly — no
  recurrence, content already verified, and further investigation has no
  concrete next step; left as a flagged-but-not-actioned note.

## Open questions
- `5709c24`'s exact provenance (how it was produced without a visible
  dispatch) remains unexplained. Not blocking; not recurring.
- Whether to launch a Round 18 review cycle, or pause here — not yet
  raised with the user this round; Round 17's findings have continued the
  Round 16 trend of shifting from P1 substantive gaps toward P2/P3 process
  hygiene and one genuine P1 bug (R17-4), plus one design-only P3 item
  (R17-10) that itself recommends a two-stage measure-before-build gate
  for its own next step rather than jumping straight to implementation.

## Repo state
```
?? docs/checkpoints/2026-07-24-r17-in-progress.md
```
(untouched artifact from the mid-round checkpoint; not part of this
round's tracked work, left as-is)

```
a99314b docs: Round 17 CHANGELOG entry (#318-#327)
cbebd45 docs(perf): batched deferred reclaim design doc (R17-10, task #327)
6b55198 docs(tests): document second race_repro.rs load-sensitive flake occurrence (R17-9 follow-up, task #326)
1117198 fix(alloc-core): reduce large-cache-extended default budget from 1280 MiB/heap (R17-9, task #326)
ea8ff86 test(alloc-core): add deterministic trim_for_recycle release oracle (R17-8, task #325)
```

Local `main` is 12 commits ahead of `origin/main` (`70a8f2f`..`a99314b`,
Round 17's full range plus wrap-up) — NOT pushed. Push only on a separate
explicit request per standing project instructions.
