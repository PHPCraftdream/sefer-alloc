# Checkpoint — 2026-07-28 04:05 [round24-in-flight]

## Session summary
Continuation of the long-running zero-trust review/fix cycle on sefer-alloc
(100%-Rust allocator). This session resumed from `2026-07-27-round23-complete.md`
(Round 23 fully shipped and pushed through `6e0dbad`). The user then asked me
to study a fresh independent read-only review of Round 23 itself
(`docs/reviews/2026-07-27-r23-readonly-review.md`, two parts: an initial
review plus a self-appended "how exactly to speed up the code" addendum) — I
personally re-verified every concrete claim in it against current source
(overflow arithmetic, MIN_BLOCK bitmap math, the first-vs-last-warm doc/code
mismatch, the 4 KiB staging array, the production unsafe-surface concern,
the `hash_remove` "exact bound" wording) before filing anything, per this
project's zero-trust convention. All claims checked out. Filed 9 Round 24
tasks (#379-387) with priority/blocking order, then the user set a `/goal`
("continue solving tasks via `/crush` agents, commit between tasks") and
work proceeded through the queue via the ACTUAL `crush` CLI tool this time
(not the `Agent` tool used in earlier rounds) — `crush run` sessions
launched via `Bash`+`run_in_background`, monitored with
`crush sessions watch`, each followed by full personal zero-trust review
(reading every diff line-by-line, independently re-deriving key arithmetic
from raw `npm run iai` logs, re-running tests/clippy/fmt myself) before
committing.

**7 of 9 Round 24 tasks landed as 7 commits** (`7f2a9ef`..wait, actually
`14a86ce`..`7378160` — see Repo state below for the exact list):
- **R24-1** (`14a86ce`): corrected R23-3's own "80.8% = M2 oracles + push"
  headline — the bench arms actually measured a 64-block BATCH free hitting
  6 magazine-overflow events, not ordinary hot free. Also reworded R23-6's
  "exact bound" language to "deterministic regression threshold."
- **R24-2** (`3bc9c91`): decomposed free-path cost by magazine state —
  cheap non-overflow push ≈43-44 Ir, one overflow event =571 Ir (12.9x a
  cheap push), overflow is 61-69% of batch-free's own-thread-body cost.
  Ordinary interleaved hot free NEVER fires overflow at all.
- **R24-3** (`e530a9f`): attempted merging the overflow bitmap-clear
  pre-pass into `flush_run` — **NO-GO, measured +37 Ir/event regression**
  (the original fixed-length loop was already compiler-unrolled/CSE'd;
  the replacement's dynamic-length loop couldn't be). All code reverted.
- **R24-4** (`9dc0e22`): attempted a `SegmentBitmap::clear_many`/`set_many`
  bulk-mask primitive at `alloc_batch`'s deferred-clear site — **NO-GO,
  measured +14 Ir/block regression** (the RMWs it coalesced were already
  cheap, ~3-4 Ir each on a hot L1 line; the primitive's own per-offset
  bookkeeping cost more than it saved). All code reverted. Two bitmap-clear
  NO-GOs in a row — a real, useful negative result for this project.
- **R24-5** (`9a5b1f3`): split `cold_alloc_free_256x16b`'s bundled cost into
  alloc-only vs free-only halves (+ mimalloc's own matching split). Found
  the ~2x gap vs mimalloc is WILDLY lopsided: alloc-only 1.27x, free-only
  3.60x — the full-round "2.0x" was masking this. Free half is 61.5%
  overflow (matches R24-2's mechanism, confirms outcome (a) of the task's
  own three-way framing).
- **R24-6** (`6d4eec6`): moved 2 of 4 candidate `unsafe fn dbg_*` hooks
  behind a new `bench-internals` Cargo feature (not in `production`). A
  FIRST ATTEMPT at this task (via `crush`, before this commit) tried to gate
  essentially ALL `dbg_*` hooks project-wide and exploded into a 130+-file
  diff before hitting a context deadline — reverted entirely, nothing
  committed from it. Re-scoped narrowly (dispatched via `Agent` tool instead
  of `crush` this time, for tighter turn-by-turn oversight) to just the 2
  hooks with exactly 1 caller each; the 2 pre-existing hooks with ~20
  callers (`dbg_push_to_ring`, R6-MS-4) were left as-is with a doc-only
  justification instead.
- **R24-7** (`7378160`): fixed `dealloc_batch_small`'s doc comment, which
  falsely claimed the LAST `TCACHE_CAP` freed blocks stay magazine-warm —
  the implementation actually keeps the FIRST. Chose option (a) (doc fix
  only) over option (b) (rolling-buffer last-warm redesign) specifically
  BECAUSE of R24-3/R24-4's adjacent NO-GOs in the same cost category — did
  not even attempt (b), reasoning explicitly from the two just-established
  regressions rather than re-discovering the same trap a third time.

**Currently in flight, NOT yet committed:** R24-8 (task #386, TaskList
`in_progress`) — investigating (1) an ownership cache for repeated
same-segment `contains_base` lookups in `dealloc_batch_small`, and (2)
whether the 4 KiB stack staging array in that same function actually pays a
real zero-init cost or whether LLVM already elides it. The crush session for
this task hit its own `--timeout 60m` once already (provider error mid-work,
no partial changes lost since nothing had been written yet) and was resumed
with a 90m timeout. That resumed run's own background-completion
notification fired with "exit code 0" but `crush sessions why` showed the
session was STILL genuinely `running` (heartbeat 5s old) when checked
immediately after — apparently a `crush sessions watch` premature-detection
quirk, not a real completion. Re-issued `crush sessions watch c015f3b0`
a second time and am currently waiting on that. The working tree already
shows real, uncommitted partial progress from this session:
`benches/perf_gate_iai.rs` and `src/registry/heap_core_dealloc_batch.rs`
modified, plus a new untracked `tests/r24_8_dealloc_batch_multi_flush.rs` —
**none of this has been reviewed or verified yet**; do not trust it, review
it fresh against raw logs exactly like every other Round 24 task before
committing anything.

**Not yet started:** R24-9 (task #387) — restructure
`docs/perf/OPEN_ITEMS.md`/gate-report correction sections to be
current-state-first instead of append-only (the append-only pattern has now
produced several multi-layered correction chains, e.g. `contains_base`
18.6%→8.8%, R23-3's 80.8%→corrected in R24-1, that force a reader to reach
the END of a long entry to learn the truth). R24-11 (task #389) — a
follow-up to R24-10 (already completed, task #388): root-cause WHICH of
(pool cap / decay-tick eviction / batch-flush overhead) explains the
residual 2.64x wall-clock gap on `bench_global_alloc_churn_with_teardown`
at 1024B, by running `bench_working_set_cycle` alongside it and reading
`dbg_pooled_count`/`dbg_decommit_count`/`dbg_segments_released_total`
deltas.

**A recurring operational lesson from this session, worth remembering for
future rounds:** `crush run --timeout` firing on a large/complex task is
NOT a signal to abandon — per the `/crush` skill's own guidance, re-run
against the SAME `--session` id with a larger timeout. This happened twice
this session (R24-2 at first with 60m→90m, and R24-8 similarly) and both
times the resumed run completed the work correctly. Separately, `crush
sessions watch` occasionally reports "completed" prematurely on a resumed
session (observed once, R24-8) — always cross-check with `crush sessions
why <id>` before trusting a "completed"/"failed" notification when the
session was just resumed, and re-issue `watch` if the session turns out to
still genuinely be `running`.

## Active goal
A session-scoped `/goal` Stop hook is armed: **"продолжай решать задачи с
помощью агентов /crush, между задачами делай коммиты"** (continue solving
tasks via `/crush` agents, commit between tasks). This hook will block
session-stop until its condition is judged met — it has NOT been cleared,
so a fresh session picking this up should either continue satisfying it
(finish R24-8, R24-9, R24-11) or the user should explicitly clear/replace
it if priorities have changed.

## TaskList
### in_progress
- #386 R24-8: dealloc_batch -- last_base ownership cache and verify the 4 KiB staging array cost (uncommitted partial progress in working tree, unreviewed)

### pending
- #387 R24-9: restructure OPEN_ITEMS.md to current-state-first instead of append-only
- #389 R24-11: root-cause the residual 1024B teardown gap -- pool cap vs decay-tick vs batch-flush

### recently completed
- #385 R24-7: resolve dealloc_batch's first-vs-last-warm doc/code contradiction
- #384 R24-6: move measurement-only dbg_* hooks out of the production API/unsafe surface
- #383 R24-5: split cold alloc-only from cold free-only -- the ~2x gap is not localized
- #382 R24-4: add SegmentBitmap clear_many/set_many bulk-mask primitives (NO-GO)
- #381 R24-3: prototype flush_magazine_class -- merge the bitmap-clear pass into flush_class (NO-GO)
- #380 R24-2: decompose free by magazine state -- non-overflow push vs single overflow vs batch sizes
- #379 R24-1: correct R23-3's 80.8% headline -- it is batch-free-with-overflow, not ordinary hot free
- #388 R24-10: investigate churn+teardown 1024B wall-clock regression (Sefer 2.64x slower than mimalloc)

## Decisions
- Switched from the `Agent` tool (used throughout Rounds 22-23) to the actual
  `crush` CLI (`crush run` + `sessions watch`) for Round 24, per the user's
  explicit `/goal` wording ("агентов /crush"). Reverted to the `Agent` tool
  for R24-6's SECOND attempt specifically, after the first `crush`-based
  attempt exploded in scope — judged that tighter turn-by-turn oversight was
  worth more than `crush`'s background batching for that one narrowly-rescoped task.
- R24-3 and R24-4 both concluded honest NO-GO verdicts with full production
  code reverts rather than shipping a regression on the strength of a
  plausible-sounding arithmetic ceiling — validated the "measure in-context,
  not standalone" discipline this project has been building since R22-15.
- R24-7 explicitly declined to even ATTEMPT its "option (b)" (algorithmic
  redesign) given R24-3/R24-4's immediately-adjacent NO-GOs in the same cost
  category — reasoned from established local precedent rather than
  re-discovering the same trap a third time.
- R24-6's first (crush) attempt was fully reverted (git checkout --,
  git clean on scratch scripts) rather than trying to salvage or narrow it
  in place, once its 130+-file scope became clear — judged a clean restart
  with a much narrower brief was safer than trying to cut down an
  already-sprawling diff.

## Open questions
- None blocking from the user's side. Operationally: R24-8's crush session
  needs to be confirmed ACTUALLY complete (not just watch-reported-complete)
  before its diff can be reviewed and committed — this is the immediate next
  step once this checkpoint write finishes.

## Repo state
```
 M benches/perf_gate_iai.rs
 M src/registry/heap_core_dealloc_batch.rs
?? .claude/
?? docs/reviews/2026-07-27-r23-readonly-review.md
?? tests/r24_8_dealloc_batch_multi_flush.rs
```
(The `benches/`/`src/`/`tests/` modifications above are R24-8's own
IN-PROGRESS, UNREVIEWED work — not yet committed, not yet verified.)
```
7378160 docs(registry): fix dealloc_batch's warm-range doc claim -- first accepted blocks stay warm, not last (R24-7, task #385)
6d4eec6 feat(cargo): move two measurement-only unsafe hooks behind a new bench-internals feature (R24-6, task #384)
9a5b1f3 bench(perf): the cold ~2x mimalloc gap is overwhelmingly in the free half, 61.5% overflow (R24-5, task #383)
9dc0e22 bench(perf): SegmentBitmap bulk-mask primitive is a NO-GO -- measured +14 Ir/block regression (R24-4, task #382)
e530a9f bench(perf): flush_magazine_class merge is a NO-GO -- measured +37 Ir/overflow-event regression (R24-3, task #381)
```
Not checked this checkpoint: whether `origin/main` is behind `main` (no
push has been requested or performed this session — the last known push
was Round 23's `6e0dbad`, so `main` is very likely several commits ahead of
`origin/main` again, matching this project's standing "push only on
explicit request" convention).
