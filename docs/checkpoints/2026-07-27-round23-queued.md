# Checkpoint — 2026-07-27 06:30 [round23-queued]

## Session summary
Long-running zero-trust review/fix cycle on sefer-alloc (100%-Rust
allocator), now well into Round 23 planning after Round 22's 18 tasks
(#352-#369) fully landed and were pushed to `origin/main` on 2026-07-26.
Immediately after that push, the user asked for a 5-part sequence
(changelog update, checkpoint, commit-amend, push, "fix CI") which I
executed — but along the way `npm run check` (the project's own pre-push
gate) found a REAL bug: R22-2's OPT-H test scenario 3 hardcoded "9 objects
of 384 KiB fit in a fresh segment," true under
`production,medium-classes,alloc-stats` but false under `--all-features`
(only 8 fit there). Fixed via runtime capacity discovery instead of a
hardcoded assumption (commit `f2764f7`, since `git rebase -i` is forbidden
by project convention, this had to be a new commit rather than folded into
the original R22-2 commit via amend). Pushed, and the REAL GitHub Actions
CI then failed too — on the exact known ~1/3-flaky
`canary_survives_promotion_and_free_leaves_no_leak` test, triggered for
the first time by R22-1's own new CI row. Root-caused (process-global
`SEGMENTS_RESERVED_TOTAL`/`SEGMENTS_RELEASED_TOTAL` atomics racing across
concurrently-run test functions within one binary) and fixed via a
file-scoped `TEST_LOCK` mutex, matching this project's own established
serialization pattern (commit `bc4aacf`). Verified with 15 personal
repeated runs + the sub-agent's own ~200 runs, then re-pushed; both
GitHub Actions workflows (`CI`, `Kani verification`) went fully green.

While all this was happening, TWO independent read-only reviews were
produced by background agents and handed to me by the user for study:
(1) `docs/reviews/2026-07-26-r22-readonly-review.md`, reviewing the whole
of Round 22 (`b6af12d..610f915`) — found real methodology gaps in R22-17's
`contains_base=18.6%` claim (not truly isolated — mixes in
`segment_base_of_ptr` + call overhead) and R22-15's mimalloc-ratio
(`1.3x-2.4x`, built on an asymmetric bootstrap-constant subtraction), AND
found a genuine LOGIC ERROR in my own R22-16 design doc (the "promotion-
time neighbor-liveness check" blocker for Linux sub-region `mremap` is
based on a false premise — I personally re-verified carve-block bump
monotonicity + empty-segment-only reset and confirmed the reviewer is
right: a live carved block's byte range is provably exclusive for its
whole lifetime, so Linux sub-region remap should be CONDITIONAL-GO, not
NO-GO). (2) `docs/reviews/2026-07-27-post-r22-followups-readonly-review.md`,
reviewing just the two urgent CI-fix commits (`f2764f7`/`bc4aacf`) — found
(all personally re-verified, all confirmed real): my own `f2764f7` fix
mislabeled itself "R22-15 (task #366)" six times, colliding with the
ALREADY-claimed identifier for the real mimalloc-arm commit (`ff48029`);
`docs/CORRECTNESS_OPEN_ITEMS.md`'s flaky-test-resolved entry falsely
claimed "Files changed: ... only" one file when the same commit touched
two; that same entry's "RESOLVED" framing risked implying leak-detection
is now robust, when the underlying assertion
(`released_delta <= reserved_delta`) only ever proved no double-release,
never no-leak (a gap pre-dating the mutex fix, correctly left untouched
by it, but under-caveated in the resolved-trail wording); a doc comment
in the new test-fix claimed to stop "without carving" a spilling object,
when the code actually does call `a.alloc()` (which carves) before
checking whether to count it; and my own proposed mutex-based fix for the
OTHER two (unrelated) flaky wall-clock tests (tracked as
`docs/CORRECTNESS_OPEN_ITEMS.md` item 3 / TaskList #375/R23-6) would NOT
actually work, since those two are flaky from CROSS-PROCESS CPU
contention (multiple test binaries + the CI runner's own load), not a
same-process race a mutex could serialize.

Every one of these findings across both reviews was personally
re-verified against real source (grep, reading the actual code, running
the actual mutation counterfactuals) before acting on it — none were
accepted at face value, consistent with this session's zero-trust
discipline throughout. 7 Round 23 tasks were filed from the first review
(#370-#376, R23-1 through R23-7), then 2 more from the second review
(#377/#378, R23-8/R23-9) which were immediately executed and closed
(both were small, mechanical, doc/comment-only fixes I dispatched via
`@sh` and personally verified before committing — commit `6b4ac50`).
`docs/perf/OPEN_ITEMS.md` was also actualized (commit `897aa26`) to note
that R22-17's `18.6%` and R22-15's `1.3x-2.4x` figures are provisional
pending R23-1/R23-2's corrections, and to add a new `[D]`-tier entry for
R22-16's design doc (which previously had no entry of its own despite
having a real CONDITIONAL-GO sub-path) — flagging that its current NO-GO
verdict needs R23-4's correction before being cited further. This required
careful renumbering of the `[D]`/`[L]` tiers and fixing the one
cross-reference that renumbering touched (learned directly from R22-6's
own earlier near-miss on this exact class of bug, within this same
session).

**Current state**: `main` is 2 commits ahead of `origin/main`
(`897aa26`, `6b4ac50` — the OPEN_ITEMS.md actualization and the R23-8/9
fixes) and has NOT been pushed since the user's last explicit push
request (which covered only up through `bc4aacf`). Per the project's
own standing rule (never push without an explicit, separate request),
these two commits are intentionally sitting locally unpushed until the
user asks. `checkpoint-watch` was armed for this project this session
(`.claude/settings.json`, a Stop-hook hint at 90% context usage) — this
is a NEW untracked file (`.claude/` shows as untracked in `git status`).

## Active goal
No `/goal`/Stop-hook is currently armed. The earlier session-scoped goal
("продолжай решать задачи с помощью агентов @sh, между задачами делай
коммиты") was satisfied when Round 22's TaskList emptied and has not been
re-armed since; the babysit cron self-deleted on that same condition.
Round 23's 7 tasks (5 still pending) are being worked directly in
conversation, not under a `/babygoal`-style heartbeat.

## TaskList
### pending
- #370 R23-1: fix contains_base measurement isolation (base_of_ptr + call overhead mixed in)
- #371 R23-2: warm N/2N matched Sefer-vs-mimalloc gate (fix bootstrap-subtraction asymmetry)
- #372 R23-3: split hot alloc / hot free / cold alloc / cold free into separate attribution arms  (blockedBy: #371)
- #373 R23-4: reopen Linux sub-region mremap as CONDITIONAL-GO -- correct R22-16's flawed neighbor-liveness argument
- #374 R23-5: close the 11 clippy dead-code errors under hardened+medium-classes
- #375 R23-6: fix or serialize the two newly-found coarse-wall-clock flaky tests (description updated: original TEST_LOCK-mutex proposal corrected to a deterministic-counter + non-blocking-signal approach, per the second review's finding)
- #376 R23-7: batch API downstream consumer + real usage measurement

### recently completed
- #377 R23-8: fix R22-15/task#366 identifier collision in f2764f7's test comments
- #378 R23-9: fix inaccurate claims in the flaky-test resolved entry + test-comment imprecision
- #368 R22-17, #369 R22-18, #367 R22-16, #366 R22-15, #365 R22-14 (Round 22, all completed 2026-07-26)

## Decisions
- Chose a NEW commit (not `git rebase -i` amend) to fold the scenario-3
  `--all-features` fix into history, since interactive rebase is
  forbidden by this project's own git-safety convention -- even though
  the fix logically belongs to R22-2/task #353.
- Chose to personally re-verify EVERY finding from both new reviews
  against real source before creating any task or making any edit --
  this caught that my own R22-16 design doc's core NO-GO reasoning was
  flawed (a rare and significant self-correction), and confirmed (not
  just trusted) the identifier-collision and doc-accuracy findings in
  the second review.
- Chose to immediately execute R23-8/R23-9 (small, mechanical, low-risk
  doc/comment fixes) rather than leave them queued, since they were
  cheap and directly related to files already fresh in context; left
  the larger, more substantial Round 23 tasks (R23-1 through R23-7)
  queued for later dispatch.
- Chose to correct R23-6/task #375's OWN description in place (not
  create a duplicate task) once the second review showed the originally
  proposed mutex fix wouldn't address the actual cross-process
  contention root cause.
- Did NOT push the two most recent commits (`897aa26`, `6b4ac50`) --
  the user's last explicit push request covered only through `bc4aacf`;
  per the project's standing rule, push requires a separate explicit
  ask each time.

## Open questions
- None currently blocking -- the two most recent commits are sitting
  locally, intentionally unpushed, awaiting the user's own initiative to
  ask for a push (not something I should surface as a question, per the
  standing rule that push only happens on explicit request; the user
  already knows this from repeated earlier reinforcement this session).
- Round 23's remaining 7 tasks (#370-376) have not yet been started --
  no in_progress task exists right now. The next natural action, absent
  further user instruction, would be to begin dispatching them in
  priority order (R23-1/R23-2 measurement-methodology fixes first, since
  OPEN_ITEMS.md's own entries now explicitly flag they're "provisional
  pending" these corrections).

## Repo state
```
?? .claude/
?? docs/checkpoints/2026-07-26-r22-complete-pre-push.md
?? docs/checkpoints/2026-07-26-r22-reviews-in-flight.md
?? docs/reviews/2026-07-26-crush-review-r19-r21.md
?? docs/reviews/2026-07-26-oh-review-r19-r21.md
?? docs/reviews/2026-07-26-r19-r21-readonly-review.md
?? docs/reviews/2026-07-26-r22-plan.md
?? docs/reviews/2026-07-26-r22-readonly-review.md
?? docs/reviews/2026-07-27-post-r22-followups-readonly-review.md
```
```
6b4ac50 docs+test: fix identifier collision and doc-accuracy issues from post-R22 review (tasks #377/#378)
897aa26 docs(perf): actualize OPEN_ITEMS.md against Round 23's queued corrections
bc4aacf test: serialize r14_4_promotion_free_correctness.rs against process-global stats race (urgent CI fix)
f2764f7 test(alloc-core): fix R22-2 scenario 3's --all-features carve-count assumption (pre-push, follow-up to task #353)
610f915 bench(perf): measure contains_base's share of free's Ir -- MATERIAL, 18.6% (R22-17, task #368)
```
Local `main` is **2 commits ahead of `origin/main`** (`bc4aacf` is the
last pushed commit; `897aa26` and `6b4ac50` are local-only, unpushed).
