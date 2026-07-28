# Checkpoint — 2026-07-28 07:30 [round24-complete]

## Session summary
Continuation of the zero-trust review/fix cycle on sefer-alloc (100%-Rust
allocator). This session resumed from `2026-07-28-round24-in-flight.md`
(Round 24 mid-flight, task #386/R24-8 running in a `crush` session with
unreviewed partial changes). Since then, all 3 remaining Round 24 tasks were
completed, each via a `crush run` session (with one resume-after-timeout
each for R24-9 and R24-11, matching the established "own `--timeout` fired
→ resume same `--session` id with a larger timeout" pattern), followed by
full personal zero-trust review (every diff read line-by-line, every
measured number independently re-verified against raw logs, `cargo
fmt`/`clippy`/`test` re-run personally) before committing:

- **R24-8** (`839b4af`) — `dealloc_batch`'s `STAGE_CAP` 512→64. Investigation
  1 (ownership cache for repeated same-segment `contains_base` lookups) was
  a NO-GO (+3/-44 Ir, codegen noise). Investigation 2 (4 KiB staging-array
  zero-init) was a real GO: LLVM-IR proof (`--emit=llvm-ir`) confirmed the
  memset is NOT elided (address escapes into `flush_class`), and shrinking
  the array removed a constant -4,065 Ir/call (-47.7%/-24.2% at N=16/64).
  My own re-run of the full test suite caught a real failure the delegated
  session's "PASS" claim had missed — `docs/ARCHITECTURE.md`'s stale
  tests/*.rs file count (221→222) — fixed in the same commit.
- **R24-9** (`ce17311`) — restructured `docs/perf/OPEN_ITEMS.md` (all 12
  items) and 3 gate reports (R22_15/R22_16/R22_17) to lead with a compact
  current-state card/box before the historical append-only narrative. Zero
  items moved to "Recently resolved" — the delegated session found the
  task's own premise ("closed items sit in `[A]` Active") didn't match the
  file's actual state (only one, still-open, item lives there) and
  correctly reported that instead of forcing a false close. All cited
  numbers in the new cards independently re-verified against each report's
  own correction section before committing.
- **R24-11** (`9594570`) — root-caused R24-10's (task #388, completed
  earlier, no commit — pure investigation) 1024B `bench_global_alloc_churn_
  with_teardown` residual (2.64-2.69x slower than mimalloc) to the
  small-segment pool's 4-segment cap being genuinely exceeded by this
  bench's stress shape (248 decommit events, measured via exact process-wide
  counters), ruling out decay-tick (~3 orders of magnitude off) and
  batch-flush (size-flat, present at parity sizes too). Config-tuning
  finding, not a defect; no production default changed. One raw-log
  artifact (stray zero-delta `working_set_cycle` lines inside the
  `churn_with_teardown` log) was investigated and resolved as an explainable
  criterion-filter-mechanics side effect (the harness function runs but its
  `bench_function` calls are skipped when the id doesn't match the filter
  string), not a data-integrity problem — confirmed by symmetric evidence in
  the reverse-filtered log.

**Round 24 is now fully complete: all 11 tasks (#379-389) closed, 9 commits
on `main` (`14a86ce`..`9594570`).**

After Round 24 closed, the user gave a new compound instruction (this
session's current, still-in-progress work): update CHANGELOG.md, checkpoint,
commit all outstanding markdown, push, and "наладь ci" (get CI in order).
CHANGELOG.md has been updated with a new "### Round 24" section (inserted
above "### Round 23", following the R24-9-established current-state-first /
Runtime-improvements-vs-Measurement-tooling split format) — R24-1 was
already documented under Round 23 (as a correction of a Round 23 finding,
via the append-correction convention applied to the changelog too), so the
new section covers R24-2 through R24-11 and cross-references R24-1's
existing entry rather than duplicating it. This checkpoint write is itself
part of that compound instruction's second step.

## Active goal
The prior session's `/goal` ("продолжай решать задачи с помощью агентов
/crush, между задачами делай коммиты") is now satisfied — the TaskList queue
it was watching (#386, #387, #389) is empty of pending/in_progress items
from Round 24. No new `/goal` has been set this turn. The user's current
compound request (changelog + checkpoint + commit + push + CI) is being
tracked via ordinary TaskList items (#390-394), not a `/goal` Stop hook.

## TaskList
### in_progress
- #391 Run /checkpoint (this write)

### pending
- #392 Commit all outstanding markdown files (CHANGELOG.md update, this
  checkpoint, `docs/reviews/2026-07-27-r23-readonly-review.md` which has
  been untracked all session)
- #393 Push to origin/main (local is 9+ commits ahead of the last known push
  at Round 23's `6e0dbad`; push has not been requested until now)
- #394 Investigate and fix CI ("наладь ci") — not yet started; scope
  undetermined, needs investigation into current GitHub Actions status
  before any fix is attempted

### recently completed
- #390 Update CHANGELOG.md for Round 24
- #389 R24-11: root-cause the residual 1024B teardown gap
- #388 R24-10: investigate churn+teardown 1024B wall-clock regression
- #387 R24-9: restructure OPEN_ITEMS.md to current-state-first
- #386 R24-8: dealloc_batch STAGE_CAP + ownership cache investigation
- #385 R24-7, #384 R24-6, #383 R24-5, #382 R24-4, #381 R24-3, #380 R24-2,
  #379 R24-1 (all Round 24 tasks — see prior checkpoint / CHANGELOG.md
  "### Round 24" for full detail)

## Decisions
- CHANGELOG.md's new Round 24 entry does NOT duplicate R24-1's already-
  existing bullet (embedded under Round 23, since it corrects a Round 23
  finding) — cross-referenced instead, preserving the append-once
  convention rather than writing the same finding twice in two places.
- R24-9's "current-state-first" restructuring format was extended to
  R24-11's own new `OPEN_ITEMS.md` entry (item 13) rather than reverting to
  the old append-only style for new items going forward — the format is now
  the live convention, not a one-time migration.
- The stray zero-delta `working_set_cycle` lines found inside R24-11's
  `_raw_r24_11_churn_with_teardown.log` during personal review were judged
  NOT to require log regeneration or a truncation-marker edit, after
  confirming (via the symmetric artifact in the reverse-filtered log) that
  they are an inherent, harmless side effect of how criterion's harness
  invokes every registered bench function regardless of the `--filter`
  argument, not a fabrication or a real discrepancy with the report's cited
  numbers.

## Open questions
- **Scope of "наладь ci"** — the user's phrase is a general "get CI in
  order" instruction with no specific symptom named. Task #394 will need to
  investigate actual GitHub Actions run status (or the workflow YAML files
  for staleness against Round 24's changes, e.g. the new `bench-internals`
  feature from R24-6, before deciding what concretely needs fixing) before
  any change is made — nothing has been investigated yet as of this
  checkpoint.
- No other open questions from the user's side.

## Repo state
```
 M CHANGELOG.md
?? .claude/
?? docs/checkpoints/2026-07-28-round24-in-flight.md
?? docs/checkpoints/2026-07-28-round24-complete.md
?? docs/reviews/2026-07-27-r23-readonly-review.md
```
```
9594570 bench(perf): root-cause the 1024B churn+teardown residual as pool-cap-exceeded, not decay/batch-flush (R24-11, task #389)
ce17311 docs(perf): make OPEN_ITEMS.md and 3 gate reports current-state-first (R24-9, task #387)
839b4af perf(registry): reduce dealloc_batch STAGE_CAP 512->64, eliminating a real 4 KiB zero-init cost (R24-8, task #386)
7378160 docs(registry): fix dealloc_batch's warm-range doc claim -- first accepted blocks stay warm, not last (R24-7, task #385)
6d4eec6 feat(cargo): move two measurement-only unsafe hooks behind a new bench-internals feature (R24-6, task #384)
```
Local `main` is well ahead of `origin/main` (last known push: Round 23's
`6e0dbad`) — push is now explicitly requested (task #393) and will happen
once #392's commit lands.
