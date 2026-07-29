# Checkpoint — 2026-07-29 16:xx [round28-complete]

## Session summary

This session executed Round 27 (tasks #419–429, 11 tasks) and Round 28 (tasks #430–431, 2 tasks) of sefer-alloc's ongoing perf/correctness improvement cycle, then performed the end-of-round wrap-up ritual (CHANGELOG, checkpoint, commit, push, CI check).

**Round 27** was triggered by an independent readonly review of Round 26 (`docs/reviews/2026-07-28-r26-readonly-review.md`). Key findings, each personally re-verified against source before filing a task: the pending pool-cap default-change proposal ("promote `DEFAULT_POOL_SEGMENTS` 4→8") was a literal no-op because the effective cap is `min(pool_segments, pool_byte_cap/SEGMENT)` and the byte cap already forced it to 4 (R27-1); the real decision is a paired `(4,16MiB)→(8,32MiB)` change; R26-9's "no RSS cost" closure premise was itself refuted by R26-3's own raw log (R27-2); the proper retention gate (R27-3) found cap8 retains ~+8 MiB/heap post-teardown, victim-activation-proven, scaling to ~+255 MiB at 32 heaps, no idle decay; the latency win (~22%) was reconfirmed at the real paired byte caps through the real `#[global_allocator]` (R27-4); an adaptive pool-budget design was written and self-critically recommended AGAINST building (R27-5) — keep the safe 4/16 MiB default, document an 8/32 MiB opt-in recipe; R27-6 removed ~250 lines of dead NO-GO code (R26-7's lazy staging array) left behind in violation of the project's own rule; R27-7 fixed my own gating-rule miss from the prior round (3 diagnostic hooks needed `bench-internals`); R27-8 corrected a timed-workload description (9 batches/1080 cycles, not 8/960); R27-9 wired the argv-roundtrip regression test into the actual `npm run check` gate and made `shell:true` an executable throw, catching 5 pre-existing violations; R27-10 removed a soundness-adjacent bitmap-clear hook after its region hit 4 consecutive NO-GOs; R27-11 evaluated (did not open) a reservation-only overflow-tier design since one of its two required triggers was unmeasured.

**Round 28** answered the two remaining open items from both `docs/perf/OPEN_ITEMS.md` and `docs/CORRECTNESS_OPEN_ITEMS.md`. R28-1 isolated `flush_class`'s standalone Ir cost (449 Ir, 56.1 Ir/block, 77.3% of one overflow event), closing a "Next trigger" open since R24-2 — verdict: the magazine-overflow region is likely exhausted for further micro-optimization (5th data point after 4 NO-GOs). R28-2 strengthened `canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound assertion from "no double-release" to "no leak" via a new per-base observable, verified non-vacuous twice against two structurally different free paths. **My own zero-trust review of R28-2 caught and fixed a real bug the delegated work never tested**: the strengthened block needed `alloc-decommit`, which the CI-tested `hardened medium-classes` combination doesn't enable — the as-delivered diff failed to compile there. Fixed by narrowing the new block's `#[cfg]` gate rather than widening the file's, verified via direct `cargo test --no-run --features "hardened medium-classes"` before and after.

Both R27-10 (bitmap-clear-invariant task) and R28-1 crush/sh-agent sessions hit interruptions mid-run (a `zai` provider peak-hours refusal for `/crush`, and a user-initiated stop for one `sh` agent run) — both were recovered cleanly: the peak-hours block was handled by pausing `/crush` entirely (per explicit user instruction) and switching to the `sh` subagent type for the rest of the session, with a one-shot cron alarm (`91e2d94f`, fires 2026-07-29 12:00) to remind a return to `/crush`; the interrupted `sh` run left no partial file state (confirmed via `git status`/`git diff` before relaunching).

Currently in progress: the user's wrap-up request ("обнови чейнджлог, сделай /checkpoint, сделай коммит всех мд, пуш, наладь ci") — CHANGELOG.md has been updated with full Round 26/27/28 entries (Round 26 was previously undocumented — this session discovered and backfilled it), this checkpoint file is being written now. Still to do: commit the markdown files, push, and check/fix CI.

## Active goal

A session-scoped Stop hook was active earlier this session with condition "доделать задачи" (finish the tasks) — it should have auto-cleared once Round 28's TaskList emptied (both R28-1/R28-2 completed and the babysit cron self-deleted on an empty TaskList). Not re-confirmed independently in this checkpoint.

## TaskList

Empty. Round 27 (#419–429) and Round 28 (#430–431) are both fully completed and committed. No pending/in_progress/blocked tasks at checkpoint time.

## Decisions

- **CHANGELOG backfill order:** discovered Round 26 had no CHANGELOG entry at all (last documented round was 25) — wrote Round 26, 27, and 28 entries together in this session rather than treating Round 26 as out of scope, since the user's "update the changelog" request implies bringing it current, not just appending the latest round.
- **R28-2's CI-compatibility fix:** chose to narrow the new leak-proof block's own `#[cfg]` gate (`alloc-decommit + alloc-xthread`) rather than widen the test file's top-level gate — preserves the `hardened medium-classes` CI row's existing (weaker) coverage of the file instead of silently dropping the test from that combination.
- **Peak-hours `/crush` handling:** per explicit user instruction, parked `/crush` entirely until a 12:00 cron alarm fires, using the `sh` Agent subagent type for R28-1/R28-2 instead of retrying `/crush` with an unauthorized `--allow-peak-hours` bypass.
- **R27-11 (reservation-only overflow tier):** did not open a design despite one of its two required triggers firing, because the task's own rule requires BOTH triggers — filed the idea into `OPEN_ITEMS.md`'s `[D]` tier with the exact missing measurement instead of guessing.

## Open questions

- Whether the `91e2d94f` one-shot 12:00 cron alarm (session-only) is still needed — it was scheduled specifically to prompt a return to `/crush`, but the session has since fully switched to `sh` agents for the rest of Round 27/28 without further `/crush` use. Left armed; harmless if it fires with nothing left to do.
- CI state ("наладь ci") has not yet been checked in this session — the wrap-up request's CI step is still outstanding as of this checkpoint.

## Repo state

```
 M CHANGELOG.md
?? .claude/
?? docs/checkpoints/2026-07-28-round26-planned.md
?? docs/checkpoints/2026-07-29-round27-in-progress.md
?? docs/reviews/2026-07-28-r25-readonly-review.md
?? docs/reviews/2026-07-28-r26-readonly-review.md
```

```
5d81f64 test(r14-4): strengthen the leak-bound assertion to prove no leak, not just no double-release (R28-2, task #431)
eb0b6b8 perf(registry): isolate flush_class's standalone Ir cost in the magazine-overflow free path (R28-1, task #430)
66282c3 docs(perf): evaluate R27-11's reservation-only overflow tier trigger — not opened (task #429)
07d03d8 fix(registry): remove dbg_overflow_bitmap_clear_pass and its bench arm (R27-10, task #428)
8352fb5 chore(scripts): wire the argv-roundtrip test into npm run check; make shell:true an executable throw (R27-9, task #427)
```
