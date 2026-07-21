# Checkpoint — 2026-07-21 06:15 [r11-in-progress]

## Session summary
This session completed the full CHANGELOG backfill (Rounds 6 tail through 9, then Round 10) and the entire Round 10 external-review-follow-up queue (7 tasks, #233–#239, commits `b2ef79e`..`9611a56`, CHANGELOG documented in `1eacbd1`). Immediately after Round 10 landed, a fresh independent external review of Round 10 itself was fetched and personally read line-by-line against source (not delegated) — it was found to be accurate. It surfaced three real issues in the R10-7 batch primitive and the pre-existing `HeapOverflow` drain path, plus one strong new optimization idea, all captured as a new "Round 11" queue:

1. **P0 — `HeapCore::alloc_batch` M2 double-issue defect.** The magazine-drain step clears each block's residency bit immediately on pop, then the refill-remainder step's `is_in_magazine` guard can no longer see those blocks — worse, the orchestrator's OWN independent re-analysis (beyond what the review's prose said) found the fix needs a SECOND change too: `alloc_batch`'s refill predicate copy-pasted an `if k == c { return false; }` short-circuit from `refill_magazine_slow`, where it's justified by a documented invariant that does NOT hold in `alloc_batch`'s context (blocks of class `c` are already externally claimed in `out[0..filled]` before this predicate ever runs). Deferring the bit-clear alone is insufficient without also removing/reworking this shortcut — this finding is captured explicitly in the R11-1 task description and the crush prompt, since it goes beyond what the review's own text stated.
2. **P0 — `HeapOverflow` drain never syncs the segment directory** (pre-existing gap, made visible by R10-3's clarified reclaim-return semantics, not a regression). Up to ~256 MiB of wasted segment activity per class in the worst case before periodic rescan/OOM-rescue recovers.
3. **P1 — `HeapOverflow` drain drops the pool/release finalization signal** (`dec_live_and_maybe_decommit`'s `true` return is discarded), leaking segments out of the pool-cap budget.
4. **New optimization idea (not a bug):** a realloc-aware Small→Large promotion that would preserve `medium-classes`' plain-alloc density win while avoiding R10-2's ~2,111× realloc regression, by diverting GROWING reallocs (not plain allocs) to the Large path after a threshold, then using the existing OPT-G in-place-grow for subsequent steps.

The orchestrator created 8 new tasks (#244–#251) mirroring the review's own prioritized queue, wired sequentially via `blockedBy` (matching the established one-crush-at-a-time methodology from Round 10). A Stop-hook goal was armed mid-session: *"реализуй задачи с помощью /crush, между задачами делай коммиты, покрой их тестами. Если /crush войдет в пиковые часы, то переключайся на агентов @sh"* — this fired once already (a stop-hook-feedback message correctly pointed out that task creation alone doesn't satisfy "реализация"), prompting the orchestrator to immediately start #244 rather than wait for explicit user go-ahead.

R11-1 (#244) is now `in_progress`: the orchestrator wrote a very detailed crush prompt (`.crush/stdin/r11-1-fix-batch-m2.prompt`) encoding both required fix components precisely (defer bit-clear + fix the predicate shortcut + batched bulk-clear at the end for the performance bonus the review suggested), plus a mandatory red-before/green-after counterfactual test requirement, plus resolution of the `#[doc(hidden)] pub fn`-is-still-public-API documentation gap the review also flagged. A `/crush` session (`r11-1-fix-batch-m2`) was launched in the background (confirmed not in crush's peak-hours window — checked `crush providers list`, zai's peak is 08:00-12:00, current time was 06:11) and is still running as of this checkpoint. `/babysit` was re-armed (`c0c6b019`, 15-minute interval) since the TaskList went from empty back to 8 pending/in-progress tasks.

No repo-affecting work has landed yet in this "Round 11" phase — R11-1's diff has not been reviewed or committed. The user separately asked about OS process priority possibly reducing benchmark noise (answered: yes for scheduler-preemption-driven outliers, no for power-plan/thermal effects — offered to build a one-shot elevated-priority run command, not yet actioned) and requested (and received) a full narrative report of the Round 10 wave.

## Active goal
`реализуй задачи с помощью /crush , между задачами делай коммиты, покрой их тестами. Если /crush войдет в пиковые часы, то переключайся на ангетов @sh` — a session-scoped Stop hook is currently armed with this exact condition text. It will keep blocking session-stop until the R11 TaskList (#244–#251) is fully implemented, committed, and tested (or reduced to a state the hook's evaluator considers satisfying "реализация" — same bar as it applied to Round 10).

## TaskList
### in_progress
- #244 R11-1: fix batch M2 magazine-prefix double-issue defect (blockedBy: none) — `/crush` session `r11-1-fix-batch-m2` running now, not yet reviewed
### pending
- #245 R11-2: fix HeapOverflow directory publication + post-drain pool/release finalization (blockedBy: #244)
- #246 R11-3: prototype realloc-aware Small→Large promotion for medium-classes (blockedBy: #245)
- #247 R11-4: batch-optimize dealloc_batch (group by segment, batched bitmap RMW) (blockedBy: #246)
- #248 R11-5: cache current_node() to cut NUMA-aware syscall overhead (blockedBy: #247)
- #249 R11-6: node-indexed NUMA-aware segment directory (prototype) (blockedBy: #248)
- #250 R11-7: page-run layer for 1.25-2 MiB medium arena (blockedBy: #249)
- #251 R11-8: small virgin-zero skip for alloc_zeroed (last priority) (blockedBy: #250)
### recently completed
(none in this phase yet — Round 10's #233–#239 and the CHANGELOG tasks #240–#243 are all `completed` from the prior phase, not re-listed here since this checkpoint is scoped to the new Round 11 work)

## Decisions
- Followed the external review's own proposed task ordering verbatim for #244–#251's dependency chain (correctness fixes first, then the highest-value new optimization, then batch dealloc, then NUMA cheap-fix-before-expensive-fix, then the largest structural change, then the hardest-to-prove item last) rather than re-prioritizing.
- Independently re-derived the M2 fix's second required component (the `if k == c { return false; }` predicate shortcut) beyond what the review's prose stated, and fed this precise finding into the crush prompt rather than relying solely on the review's "defer bit-clear" framing, which alone would have been an incomplete fix.
- Chose to fold the `#[doc(hidden)]`-is-still-public-API documentation/API-boundary question into R11-1's scope (same file, same experimental surface) rather than spinning it out as its own task.
- Re-armed `/babysit` (new job `c0c6b019`) immediately upon the TaskList going non-empty again, per the established babygoal/babysit protocol.
- Confirmed crush is not in a provider peak-hours window before launching R11-1 (`crush providers list`showed zai's peak as 08:00-12:00, current time 06:11) — the goal's "switch to @sh if peak hours" clause was not yet triggered.

## Open questions
- None from the user pending an answer. The OS-priority benchmark-noise question was answered in full in-conversation; the user has not yet said whether to build the one-shot elevated-priority command — not currently blocking any active work.
- Whether/when to run `npm run check` + push to `origin/main` remains open, per the standing session convention (only on explicit request, likely once at the very end of all outstanding work, including this new Round 11 queue).

## Repo state
```
?? _gad_suite.log
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
?? docs/perf/_raw_r10_6_dir_off_nonnuma.log
?? docs/perf/_raw_r10_6_dir_on_nonnuma.log
?? docs/perf/_raw_r10_6_numa_sweep.log
?? docs/perf/_raw_r10_7_d_vs_f.log
?? docs/perf/_raw_r10_7_tcache_arm.log
?? docs/perf/_raw_r10_7_tcache_isolated.log
?? docs/perf/_raw_r10_7_warm_arm.log
?? docs/perf/_raw_r9_9_followup.log
?? docs/perf/_raw_r9_9_followup_run2.log
?? docs/perf/_raw_r9_9_followup_run3.log
```
(All untracked files are generated measurement/CI scratch artifacts from prior sessions' bench/test runs — not real pending changes. The repo tree is otherwise clean; R11-1's `/crush` session has not yet written anything to disk as of this checkpoint, or its output hasn't synced to this working tree snapshot.)

```
1eacbd1 docs(changelog): document Round 10 — external-review follow-up, self-correcting gates, batch API reversal
9611a56 feat(batch): tcache-aware batch primitive — GO, refutes R9-9's no-daylight premise (R10-7)
6a11c61 docs(readme): sync bench tables to 2026-07-20 run + fix stale bench-table.mjs line ref + tier-2 unsafe count 33->35
cab6573 docs(perf): NUMA-aware segment-directory scan cliff — measured 140x, CONDITIONAL GO design (R10-6)
fdd360d perf(medium-classes-wide): warm-vs-warm Large-cache-hit gate for 1.5/1.75 MiB (R10-5)
```
