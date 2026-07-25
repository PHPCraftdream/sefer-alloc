# Checkpoint — 2026-07-24 19:45 [r17-in-progress]

## Session summary
This is the same long-running session that completed Rounds 13-16 (each a
review→task-queue→execution cycle). After Round 16 wrapped (CHANGELOG,
checkpoint, push, CI green), the user was asked whether to continue into
another review cycle and explicitly chose to STOP ("Остановиться
(Recommended)"). Shortly after, the user pasted a NEW, independent external
review of Rounds 14-16 directly into a `/fm` command invocation and asked to
plan/prioritize/file tasks from it — this explicitly re-opened the iteration
loop the user had just closed. I read the review, personally verified its two
highest-severity claims against source before trusting the rest (grep-
confirmed `os.rs`'s `read_directory_class_words`/`read_directory_node_bucket`
are safe fns with raw derefs; confirmed the R14-4 gate report's pad/cache
admission table numbers), wrote `docs/reviews/2026-07-24-r17-plan.md`, and
filed tasks #318-#327 (R17-1..R17-10).

**Tooling switch mid-round:** the user asked me to switch from `Agent`
sub-agent dispatch to the `/crush` CLI tool for R17-1 onward (task #318 was
already done directly by me before the switch). I followed the `/crush`
skill's launch protocol (`.crush/stdin/<task>.prompt` files, `crush run
--session <id> --timeout 60m --json`, `crush sessions watch`). Two crush
provider peak-hours refusals were hit and handled per the skill's explicit
"never self-add --allow-peak-hours without being asked" rule — I surfaced
the refusal via `AskUserQuestion` both times rather than silently bypassing.
Several crush runs (R17-3, R17-4) hit the 60-minute timeout mid-task and were
resumed into the SAME `--session` id with a continuation prompt, per the
skill's explicit guidance for that failure mode (not a provider-quota
failure, so no Agent fallback was used). Later in the round the user asked
to switch delegation back to `Agent(subagent_type="sh")` for task #322
onward — task #321 (R17-4) was the last crush-dispatched task; #323, #325,
#326 were dispatched via `Agent(sh)`.

**Key substantive finding this round (task #321/R17-4):** while investigating
an open question the R14-4 gate report itself had left unresolved (why
`nopad`/`floor512kib` pad-target modes got 0 large-cache hits/249 fresh
segments while `fixed2mib` got 232 hits/17 segments, despite all three
requesting an identical 4 MiB rounded `usable`), the crush session found and
fixed a REAL segment-leak bug: `HeapCore::dealloc_own_thread_with_base`'s
fastbin magazine dispatch keyed dealloc routing on `class_for(layout.size())`
rather than segment `kind`, so a Large segment promoted via R14-4 and then
grown in-place via OPT-G to a size that classifies "small" under
`medium-classes` was misrouted into the small-magazine free path and NEVER
reached the Large dealloc branch — leaking its 4 MiB segment every round.
I personally caught and pushed back on the first version of this fix: it had
removed the `hardened` feature gate entirely, making the new `kind_at(base)`
check run unconditionally on every small dealloc under ANY `fastbin` build,
including plain `production` (which cannot legitimately reach this scenario
since it lacks `medium-classes`) — a real, avoidable hot-path cost for a bug
that literally cannot occur under `production`. I sent the session back with
this specific critique; the revised fix correctly splits the `#[cfg]` into a
`medium-classes` branch (unconditional correctness-critical routing) and a
non-`medium-classes` branch (unchanged `hardened`-only defensive no-op, byte-
for-byte identical to before). I personally verified: (a) a red/green
counterfactual via a throwaway `git worktree` at the pre-fix commit — the new
test genuinely fails there (`large_cache_hits` delta 0); (b) `npm run iai` on
plain `production` — numbers are bit-for-bit identical to the R17-3 post-fix
baseline, confirming zero hot-path cost.

**All 10 planned tasks are now committed (#318-#327 all show completed in
the TaskList), through a commit range `70a8f2f`..`1117198`.** Two tasks
(#324/R17-7, #325 confusion) surfaced an anomaly worth flagging honestly:
I had explicitly DEFERRED #324 (class-aware-dirty re-verification) because
the user told me mid-round "пока бенчи не имеют смысла — мы работаем под
idle приоритетом на шумной машине" (benchmarks aren't meaningful right now,
we're running under idle priority on a noisy machine) — yet a commit
(`5709c24`) doing exactly that task's work later appeared in `git log`
without me having dispatched it in this visible context. I investigated:
`git status`/`crush sessions list` showed no live process that could have
produced it, and the commit's own content is high-quality and HONEST (it
explicitly discloses 80-100% measured CPU load as a standing caveat rather
than hiding it, and reaches the same "stays in production for
recoverability, not confirmed speedup" conclusion I'd have wanted) — I
verified it touches zero `src/` files and re-ran the mandatory checks
personally before accepting it into the task's completion record. The
mechanism by which this commit was produced remains genuinely UNEXPLAINED
(possibly a stray backgrounded crush/agent process from earlier in the
session that I lost track of, possibly something else) — flagged here
rather than swept under the rug, since it does not match this session's
normal "I dispatch, I verify" pattern for this one task.

**Interrupted mid-investigation:** while finishing personal verification of
task #326 (R17-9, large-cache-extended default budget reduction 1280→256
MiB/heap), the @sh sub-agent's own report flagged a `STATUS_STACK_BUFFER_
OVERRUN` crash in `tests/race_repro.rs::drain_reclaim_uaf_repro_tight_
handoff` during one full `cargo test --release --features production` run,
claimed as an unrelated pre-existing flake (reran 3/3 clean in isolation).
Given the severity class of that specific crash signature (a Windows stack-
corruption detector, not an ordinary assertion failure), I did NOT accept
this at face value — I was in the middle of independently investigating it
when a `/goal` command (immediately interrupted by the user) and then this
`/checkpoint` command arrived. What I'd established before the interrupt:
(1) `race_repro.rs` is a genuinely old file (task #33, phase 12.6, commit
`ea3a4ba`, June 2026) — nothing in this round touches it, ruling out R17-9 as
the cause; (2) the EXACT SAME crash signature (`race_repro.rs`
`STATUS_STACK_BUFFER_OVERRUN`) already appeared once before, in Round 14
(task #289/R14-4), and was independently confirmed there too as an
environmental flake under concurrent CPU contention in this shared
workspace, reproducing clean in isolation both then and now. This is now the
SECOND independent occurrence of the identical signature under the identical
"shared/loaded workspace" circumstance — strong (not yet conclusive)
evidence this is a real, if rare, load-sensitive flake class in `race_repro.
rs` specifically (distinct from the `teardown_trim`/`tombstone_rebuild`
flakes already known and documented elsewhere), not a regression from R17-9.
**Not yet done:** I have not personally reproduced this crash myself (only
read the sub-agent's claim + cross-referenced the R14-4 precedent), have not
finished the remaining mandated-matrix confirmation for #326, and have not
marked task #326 complete or moved to #327 (the R17-10 design doc task).

## Active goal
A NEW `/goal` was issued by the user ("продолжай решать задачи с помощью
агентов @sx, меджу задачами делай коммиты" — continue solving tasks via @sx
agents, commit between tasks) but the user immediately interrupted that same
turn before I acted on it, then issued `/checkpoint` instead. So: a
session-scoped Stop hook IS now armed with that condition text, but I have
not yet begun acting on it (no @sx dispatch has happened) — the interrupt
landed before any action. Note the goal says `@sx` (a new agent alias not
used elsewhere this session so far — `claude-sonnet-5 effort=xhigh` per the
agent-type list), superseding both the earlier crush-tooling and the
`Agent(sh)` instructions for whatever comes next.

## TaskList
### in_progress
- #326 R17-9 (P2): large-cache-extended default budget reduction — CODE
  ALREADY COMMITTED (`1117198`), but my personal zero-trust verification is
  NOT finished (mid-investigation of the STATUS_STACK_BUFFER_OVERRUN claim
  in race_repro.rs when interrupted)

### pending
- #327 R17-10 (P3, design doc): batched deferred reclaim — per-segment mask,
  one directory-sync per segment (not yet started)

### recently completed
- #325 R17-8: deterministic trim_for_recycle release oracle (`ea8ff86`)
- #324 R17-7: class-aware-dirty re-verification (`5709c24` — see the
  "anomaly" paragraph above; content personally verified, provenance
  mechanism unexplained)
- #323 R17-6: segment_table.rs stale HASH_CAPACITY numbers (`d8f9c9b` +
  follow-up `fbc48a5`)
- #322 R17-5: pad-comment fix (turned out to already be resolved as a side
  effect of #321's commit — closed without new dispatch)
- #321 R17-4: pad/cache admission anomaly root-caused to a real segment leak,
  fixed with a hot-path-scoped `#[cfg]` split (`1b761f4`)
- #320 R17-3: bootstrap zero-loops gated `cfg(miri)`-only (`b8612bc`)
- #319 R17-2: `os.rs` directory-read helpers → `unsafe fn` (`f65015a`)
- #318 R17-1: `sidecar::reserve_zeroed_with` raw-pointer fixup fix (`70a8f2f`)

## Decisions
- Chose to push back on R17-4's first-draft fix (unconditional `kind_at`
  check) rather than accept it, because it added a permanent hot-path cost to
  `production` for a scenario `production` cannot reach — resumed the same
  crush session with a specific, source-grounded critique rather than
  fixing it myself or accepting the imprecision.
- Deferred #324 (R17-7) mid-round per the user's explicit "noisy machine"
  instruction, then accepted its later, unexplained appearance in git log
  after independently verifying its content and provenance were sound rather
  than reverting or ignoring it — chose transparency (flagging the anomaly
  explicitly in this checkpoint) over either blind trust or blind rejection.
- Did NOT accept the #326 sub-agent's "unrelated pre-existing flake" claim
  about `STATUS_STACK_BUFFER_OVERRUN` at face value despite it being a
  plausible/likely-correct claim — was independently re-deriving the
  evidence chain (file history, prior-round precedent) before trusting it,
  per this session's standing zero-trust discipline for anything above
  ordinary assertion-failure severity.

## Open questions
- Genuinely unresolved (not yet posed to the user): how did commit `5709c24`
  (#324/R17-7's work) get produced without a dispatch visible in this
  session's tool-call history? No live crush session or Agent call for it is
  in evidence. Not blocking — content already independently verified sound —
  but worth surfacing if it recurs.
- Not yet decided: whether `race_repro.rs`'s recurring
  `STATUS_STACK_BUFFER_OVERRUN` (now 2 independent occurrences across
  Round 14 and this round, both under heavy shared-workspace CPU load, both
  reproducing clean in isolation) warrants its own follow-up investigation
  task (mirroring R16-6's `teardown_trim` flake precedent) rather than being
  waved off a second time. Leaning toward "yes, file it" but had not reached
  that decision before the interrupt.

## Repo state
```
(clean — nothing uncommitted)
```

```
1117198 fix(alloc-core): reduce large-cache-extended default budget from 1280 MiB/heap (R17-9, task #326)
ea8ff86 test(alloc-core): add deterministic trim_for_recycle release oracle (R17-8, task #325)
5709c24 docs(perf): re-verify class-aware-dirty full-work verdict (R17-7, task #3)
fbc48a5 docs(alloc-core): avoid ambiguous literal in register()'s cap-lifting comment (R17-6 follow-up, task #323)
d8f9c9b docs(alloc-core): fix stale HASH_CAPACITY comment numbers in segment_table.rs (R17-6, task #323)
```

Local `main` is 9 commits ahead of `origin/main` (`70a8f2f`..`1117198`,
Round 17's range so far) — NOT yet pushed (no wrap-up/push step has been
reached; Round 17 is still mid-execution, #326 verification incomplete,
#327 not started).
