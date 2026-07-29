# Checkpoint — 2026-07-29 06:52 [round27-in-progress]

## Session summary
Continuation of the zero-trust review/fix cycle on sefer-alloc (100%-Rust
allocator). This session opened with `/oh` delivering an independent
read-only review of Round 26
(`docs/reviews/2026-07-28-r26-readonly-review.md`) — read in full, and
every substantive claim was personally re-verified against source before
any task was filed on it (the `min(pool_segments, pool_byte_cap/SEGMENT)`
resolution formula, the `RSS_BATCH_SIZE=50` probe gap, the raw-log
`rss_after_kib=` grep counts, the `maybe_decay_small_pool` event-driven
mechanism, the `release_or_pool_empty_segment` cap-full branch location,
the background-thread anti-precedent citations). The review found two P0
defects: (1) the pending "promote `DEFAULT_POOL_SEGMENTS` 4→8" default
proposal is a literal no-op because the effective cap resolves as
`min(segments, bytes/SEGMENT)` and the byte cap was never raised in
tandem; (2) my own R26-9 closure (this session's prior round) claimed "no
cap-specific RSS cost" based on a probe that never proved victim
activation — R26-3's own raw log already showed cap8 retaining ~4,100 KiB
more, contradicting that closure.

Filed 11 Round 27 tasks (#419-429) with explicit priority and `blockedBy`
dependencies, then worked the entire queue via `crush` (per the user's
standing `/babygoal` directive "продолжай решать задачи с помощью
агентов /crush, между задачами делай коммиты"), with full personal
zero-trust review — every diff read line-by-line, every raw-log number
cross-checked, the pre-push gate (`cargo test --features production`)
personally re-run — before every commit. **10 of 11 tasks are now
committed** (`3425610`..`8352fb5`); task #428 (R27-10) is actively running
in a `crush` session as this checkpoint is written, and task #429
(R27-11) is Conditional and has not been evaluated yet.

**The round's central, load-bearing finding (R27-3, commit `9e96fd3`)**:
a new subprocess-per-arm retention probe, run at the pressure-producing
batch-120 workload with victim activation HARD-ASSERTED for both arms
(cap-4 must show `decommit_delta > 0`; cap-8 must show `pooled_hw_max > 4`
AND `decommit_delta == 0` — every one of 18 child processes passed both
checks), found cap 8 genuinely retains **~+8 MiB per materialised heap**
post-teardown (not the "RSS-neutral" my own R26-9 claimed), scaling
linearly to ~+255 MiB at 32 concurrent heaps, and does NOT self-decay
during idle (the small-pool decay is event-driven only — fires on
`reserve_small_segment`, confirmed by reading
`alloc_core_small_pool.rs::maybe_decay_small_pool` myself). R27-4 (commit
`7d60ee4`) then confirmed the ~22% latency win survives at the REAL
paired byte cap (not R26-3's 256 MiB measurement ceiling) through the
real `#[global_allocator]`. R27-5 (commit `9a851d7`) used both to write a
genuinely self-critical adaptive-pool-budget design that recommends
AGAINST building it — the design is sound on paper but its headline
benefit is unproven under every workload this project has ever measured
(all uniform-pressure; the token budget only helps under uneven pressure,
which nothing exhibits) and its hardest sub-problem (idle shrink-back) is
unsolved within this project's documented no-background-thread
anti-precedent. Recommendation: keep the 4/16 MiB default, document an
8/32 MiB throughput recipe as a separate future task.

**Two real, unrelated pre-existing debts were found and fixed
out-of-band** while zero-trust-verifying task #427 (R27-9)'s `npm run
check` run: `examples/r27_3_pool_retention_gate.rs` (my own R27-3/R27-7
commits) had accumulated rustfmt drift (fixed standalone, `b7affb7`) and
a `clippy::ptr_arg` lint (fixed standalone, `f56d61b`) — neither related
to R27-9's actual scripts-only scope, both genuinely blocking `npm run
check`'s green run, both verified independently before fixing.

**Housekeeping tasks completed this round**: R27-6 (`3ed24a9`) removed
R26-7's retained NO-GO lazy-stage implementation (~250 duplicated unsafe
lines + 9 bench arms) per this project's own benchmark-hook-cleanup rule
I wrote in R25-10 and then violated in R26-7 — caught by this round's
review. R27-7 (`fdeeb89`) re-gated three Round-26 diagnostic hooks
(`dbg_tcache_contains`, `dbg_pool_cap`, `dbg_is_free_for`) behind
`bench-internals` — another rule I wrote (R25-10) and then violated
myself when reviewing R26-1/R26-5. R27-8 (`e9e2c01`) corrected R26-3's
"8 timed batches" description to the actual "9 timed batches/1080
cycles" (a description error only — both arms measured the same shape,
so the published numbers are unaffected; R27-4 independently confirmed
this by fixing the timing correctly from scratch in new files and
finding the per-batch delta unchanged). R27-9 (`8352fb5`) wired the
argv-roundtrip regression test into `npm run check` as step 0, made the
`shell:true` prohibition an executable throw instead of a comment, and in
auditing every `run()` caller for the new throw, found and fixed 5
pre-existing `shell:isWin` callers R26-8 had missed.

**Task #428 (R27-10) is IN PROGRESS as this checkpoint is written**: a
`crush` session (`r27-10-bitmap-clear-invariant`, background job
`b1r09dobf`) is resolving the review's P2 finding that
`dealloc_overflow_bitmap_clear_only_16b`'s bench leaves the magazine
slots and bitmap in a temporarily-disagreeing state with no explicit
cleanup — harmless under `iai`'s per-process isolation, but undocumented
as a precondition. The task offered 3 options (delete the hook; restore
state + matching baseline cleanup; document process-termination as an
explicit postcondition) with a stated default recommendation of deletion
(matching the R27-6 precedent — the region has 4 confirmed NO-GOs now).
Live `git status` at checkpoint time shows this session's IN-FLIGHT,
UNCOMMITTED edits: `Cargo.toml`, `README.md`, `benches/perf_gate_iai.rs`,
`docs/perf/OPEN_ITEMS.md`, `docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE.md`,
`docs/perf/R24_3_FLUSH_MAGAZINE_CLASS_GATE.md`,
`docs/perf/R24_4_BULK_MASK_PRIMITIVES_GATE.md`, and
`src/registry/heap_core_diag.rs` — the file set strongly suggests option
(a) (deletion) was chosen, and that the hook's Ir figure was historically
cited in those three R24 gate reports, now getting append-only correction
notes. This has NOT yet been reviewed or committed — the next step once
the crush session completes is the same zero-trust process every prior
task in this round got: read every diff, cross-check any cited numbers
against raw logs, re-run the pre-push gate myself, then commit.

## Active goal
Session-scoped Stop-hook goal is armed (via `/goal`, re-confirmed via a
second `/babygoal`): "продолжай решать задачи с помощью агентов /crush,
между задачами делай коммиты" (continue solving tasks via /crush agents,
commit between tasks). A `babysit` cron is also armed
(`3e1562c9`, `7,22,37,52 * * * *`) tracking the same TaskList.

## TaskList
### in_progress
- #428 R27-10 (P2): resolve `dbg_overflow_bitmap_clear_pass`'s benchmark
  leaving a temporarily-broken magazine invariant — crush session
  `r27-10-bitmap-clear-invariant` actively running, uncommitted edits
  present in the working tree (see Session summary for the exact file
  list)

### pending
- #429 R27-11 (Conditional on R27-3/R27-4): design a reservation-only
  overflow tier — ONLY if committed retention proves too expensive.
  R27-3/R27-4 have landed with real numbers (~+8 MiB/heap retention,
  ~22% latency win) but this task's own trigger condition ("committed
  retention proves too expensive") has not yet been explicitly
  evaluated against them — that evaluation is the next task after #428
  closes.

### recently completed (this round, in order)
- #427 R27-9 (P2): wire argv-roundtrip into `npm run check`, executable
  `shell:true` throw (`8352fb5`)
- #426 R27-8 (P2): correct R26-3's timed-workload description (`e9e2c01`)
- #425 R27-7 (P2): gate 3 Round-26 diagnostic hooks behind
  `bench-internals` (`fdeeb89`)
- #424 R27-6 (P1): remove R26-7's NO-GO lazy-stage implementation
  (`3ed24a9`)
- #423 R27-5 (P1): adaptive pool-budget design, CONDITIONAL-GO-on-paper /
  recommend against (`9a851d7`)

## Decisions
- **R27-1's malformed default-change proposal was fixed as a
  task-framing/test correction, not a default promotion** — a new test
  (`tests/small_segment_pool.rs::paired_knob_promotion_is_not_a_noop`)
  proves the paired vs one-knob distinction; no `src/` default touched.
- **R27-3 deliberately measured the REAL candidate configs `(4,16MiB)`
  vs `(8,32MiB)`, not a 256 MiB measurement ceiling** — the whole point
  of the retention gate was to prove the capacity difference was
  actually exercised (victim activation), which a generous ceiling would
  have masked.
- **R27-5's design recommends Option 1 (keep the safe default) over the
  adaptive design it was asked to write** — earned from the data (every
  measured workload is uniform-pressure, so the token budget's benefit
  is unproven) and the complexity trade-off, not defaulted to the more
  sophisticated-sounding option.
- **R27-6 scoped the eager-baseline-arm question explicitly**: kept the
  4 eager `dealloc_batch_fresh_{0,1,8,17}_16b` arms (they measure
  shipping code, fill a real N-grid gap) while deleting the 9 lazy
  arms (duplicate unsafe code, no compensating benefit) — a real
  distinction, not a blanket keep-or-delete.
- **The two pre-existing rustfmt/clippy issues found during R27-9's
  verification were fixed as standalone commits**, not folded into
  R27-9's own commit — they were unrelated to R27-9's scripts-only scope
  and deserved their own attribution/history entry.

## Open questions
None from the user's side. Operationally: task #428's crush session is
still running as this checkpoint is written — its result has not yet
been zero-trust reviewed or committed. The immediate next step once it
completes is the same review discipline every prior R27 task received.

## Repo state
```
 M Cargo.toml
 M README.md
 M benches/perf_gate_iai.rs
 M docs/perf/OPEN_ITEMS.md
 M docs/perf/R24_2_FREE_BY_MAGAZINE_STATE_GATE.md
 M docs/perf/R24_3_FLUSH_MAGAZINE_CLASS_GATE.md
 M docs/perf/R24_4_BULK_MASK_PRIMITIVES_GATE.md
 M src/registry/heap_core_diag.rs
?? .claude/
?? docs/checkpoints/2026-07-28-round26-planned.md
?? docs/reviews/2026-07-28-r25-readonly-review.md
?? docs/reviews/2026-07-28-r26-readonly-review.md
```
(The 8 modified files above are task #428's IN-FLIGHT, UNCOMMITTED crush
output — not yet reviewed. `docs/reviews/2026-07-28-r26-readonly-review.md`
is this round's driving review, still untracked pending a wrap-up commit.
`docs/checkpoints/2026-07-28-round26-planned.md` and
`docs/reviews/2026-07-28-r25-readonly-review.md` are untracked leftovers
from the prior round's session, not yet swept into a commit.)
```
8352fb5 chore(scripts): wire the argv-roundtrip test into npm run check; make shell:true an executable throw (R27-9, task #427)
f56d61b style: fix clippy::ptr_arg lint in r27_3_pool_retention_gate.rs
b7affb7 style: fix pre-existing rustfmt drift in r27_3_pool_retention_gate.rs
e9e2c01 docs(perf): correct R26-3's timed-workload description -- 9 batches/1080 cycles, not 8/960 (R27-8, task #426)
fdeeb89 chore(registry): gate the three Round-26 diagnostic hooks behind bench-internals (R27-7, task #425)
```
Local `main` and `origin/main` sync status not checked this checkpoint —
no push has been requested or performed this session; local is likely
several commits ahead of the last known push.
