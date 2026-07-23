# Checkpoint — 2026-07-23 03:15 [r13-in-progress]

## Session summary
This session resumed from the `2026-07-22-r13-planned.md` checkpoint (Round
13 planning complete, 10 tasks #271-280 filed from two independent reviews of
Round 12) and executed the queue via `@sh` sequential sub-agents under
`/babygoal`, with a babysit cron (`edb97149`, 15 min) covering the whole run.
Every task followed the session's standing zero-trust discipline: read every
diff line-by-line personally, re-run tests myself (never trust the
sub-agent's own "tests passed" claim), reproduce red/green counterfactuals
for safety-relevant fixes, run clippy×(default/experimental/all-features)
+ fmt, and only mark a task `completed` in the TaskList myself after
finishing verification — several sub-agents prematurely marked their own
tasks complete and were reverted to `in_progress` each time, per established
session discipline.

**Completed and committed this session** (9 of 10 planned tasks, plus 2 bugs
discovered mid-verification and fixed as their own tasks):
- **#271/R13-1** (`e2d84f7`) — coarse-only OOM-transition latch for
  `class-aware-dirty`, closing a lost-signal gap between sidecar-OOM pushes
  and later successful materialisation. Loom-verified (7 tests), red/green
  counterfactual personally reproduced.
- **#284/R13-11** (`da037f2`) — found DURING #271's verification: a
  deterministic (not flaky) lost-wakeup test failure in
  `class_aware_dirty_routing.rs`, reproduced even on the original R12-7
  commit. Root-caused to a TEST bug (small_cur's own refill-batch leftover
  masking the intended cross-thread-reclaim path), not a production defect —
  fixed via a burn-down loop before the measured assertion.
- **#272/R13-2** (`a3434df`) — NUMA directory bucket-slot reuse (an
  `active_bits_by_node` counter frees a slot once every bit a node ever set
  goes back to 0) + `clear_bit` switched from the registering
  `node_bucket_mut` to read-only `node_bucket` (a second, independently-found
  defect) + stale NUMA-disabled-directory-lookup comment fixed.
- **#273/R13-3** (`9886780`) — virgin-zero-skip threaded through the
  magazine (`PerClass::virgin_mask`) instead of bypassing it, recovering both
  the tcache fast path AND the drain prelude (`drain_heap_overflow`) a
  calloc-only workload had been silently never running — a real resource-
  retention defect (N1), not just a perf regression. Wall-clock gate: no
  statistically significant difference at n=10 on this single-threaded
  synthetic bench (reported honestly, not oversold) — the fix's
  justification rests on the resource-retention fix, not a big number.
- **#285/R13-12** (`e7617d1`) — found DURING #273's verification: a genuine
  pre-existing compile error (`alloc-xthread`+`fastbin`+`alloc-decommit`
  without `alloc-segment-directory` = E0599 in `drain_heap_overflow`),
  confirmed via `git stash` to predate R13-3 entirely. Fixed by gating the
  two call sites, mirroring the existing pattern at every sibling call site.
- **#275/R13-5** (`0f3b608`) — feature-isolated CI rows (the exact combos
  that would have caught #284 and #285), `loom_class_aware_dirty.rs` wired
  into the `loom-xthread` CI job (was silently never running — a second
  instance of task #204's original miss), plus a new structural guard
  (`tests/no_stale_loom_files.rs`) that fails CI if any `tests/loom_*.rs`
  file is ever unreferenced again.
- **#274/R13-4** (`6018cf8`) — page-run verdict corrected from "SUPERSEDED"
  to "DEFERRED — no demonstrated production victim yet" (both
  `exact-span-large` and `medium-classes-wide` are still opt-in; production
  gets no RSS benefit from either yet).
- **#276/R13-6** (`3829d82`) — production A/B gate for
  `exact-span-large`+`large-reserved-capacity`. Headline finding: iai's
  `realloc_grow` bench shows **+102.3% instructions** under the pair vs
  plain `production` on a doubling-cadence workload — the pair is net
  SLOWER, not just "still catching up". Root cause: `large-reserved-
  capacity`'s fixed 2× geometric `reserved_capacity` ceiling re-trips on
  almost every doubling step. **Recommendation: CONDITIONAL-GO** (not
  unconditional) — no promotion action taken, matches the honest verdict.
- **#277/R13-7** (`df636ff`) — new opt-in `large-cache-extended` feature:
  widens the Large free-cache from 8 to 40 slots via a lazily-materialised
  sidecar (mirrors the `SegmentDirectory`/`PerClassDirty` pattern). Judge:
  88.89%→100.00% hit rate, 23437ns→237ns per op (~99×) on a genuine 9-size
  overflow workload. **Non-trivial recovery arc**: the agent's connection
  dropped mid-task; I found substantial real uncommitted work, and during
  verification personally discovered AND fixed three escalating measurement
  bugs myself (not delegated): (1) a stale doc-comment reference to a
  nonexistent test file; (2) the original judge's round-robin pattern +
  wrongly-claimed-non-aliasing 24-size list made it measure nothing (fixed
  via a batch alloc-all/dealloc-all pattern); (3) the FIRST fix's list was
  itself vacuous under `--all-features` because `medium-classes-wide` raises
  the Small/Large boundary to ~1.75 MiB, reclassifying 3 of 10 "safe" sizes
  as Small — fixed by computing sizes AT RUNTIME from the actual boundary
  (`dbg_small_class_count`/`dbg_block_size`), mirroring task #265/R12-14's
  established "make density-agnostic" convention rather than excluding the
  combination.
- **#278/R13-8** (`874650b`) — judge on 256-2048 live 260 KiB-2 MiB objects.
  Found a real, 100%-reproducible `MAX_SEGMENTS` wall at exactly 1023 live
  Large objects in every feature arm — this materially updates R13-4's
  "no demonstrated page-run victim" verdict: a victim now exists for THIS
  size band, though `exact-span-large` already closes the RSS/commit side
  and there's no non-linear wall-clock cost approaching the wall, so the doc
  recommends trying a simpler expandable `SegmentTable`/raised `MAX_SEGMENTS`
  before reaching for page-run's larger design budget. Also confirmed the
  extended Large cache (#277) is irrelevant to a STATIC live-set scenario
  (0 hits — the cache only helps turnover-shaped workloads), not a
  contradiction with #277's own real ~99-278× turnover win.

**In progress right now (#279/R13-9) — mid-verification of a promotion
already user-confirmed, NOT YET COMMITTED:**
`class-aware-dirty` (per-(segment,class) dirty-bit routing) got a **GO**
recommendation after a full A/B gate (`docs/perf/R13_9_CLASS_AWARE_DIRTY_
PRODUCTION_GATE.md`, commit `bebd902`): 21.71× ns/owner_alloc at N=8
(re-measured on top of R13-1's latch fix, inside R12-7's own pre-latch
19.7-32.4× range — the latch does not erode the win), iai confirms +0.00% to
+0.02% Ir on 12 non-remote benches (zero cost outside cross-thread paths),
~8 KiB RSS sidecar per materialised heap (corrects R12-7's own doc, which
said 6.1 KiB raw `size_of` — actual page-rounded footprint is 8 KiB/2
pages). I personally re-verified every raw-log number, re-ran the exact CI
feature-isolation command (green, after correctly applying the documented
`--skip r9_6_class_aware_dirty_waste_ratio_scales_with_class_count` — an
EXPECTED skip, not a workaround, per that test's own R12-7-era module doc),
and confirmed `production+numa-aware` still compiles together. **Presented
the promotion decision to the user via AskUserQuestion — user chose "Да,
включить сейчас" (yes, promote now).**

I then edited `Cargo.toml` myself (not delegated) to add `class-aware-dirty`
to `production = [...]`, with a comment citing the R13-9 gate numbers. I
started `cargo test --release --features production` (background task
`bkli8sll1`) to verify the new production composition end-to-end, and
`cargo build --features "production numa-aware"` (confirmed compiles) — the
production-test-suite run's result was NOT yet observed when the user
interrupted with `/checkpoint`. **This is the single loose thread**: the
Cargo.toml edit is real and uncommitted; whether the full production test
suite passes with `class-aware-dirty` now baked in has not yet been
confirmed by me.

**Not yet started:** #280 (R13-10, process improvements: wave A/B/B/A
report convention, raw-log tracking policy, comment hygiene — blocked on
nothing, just not reached), #281 (CHANGELOG for Round 13, blocked on #279
and #280), #282 (final checkpoint — this file is an INTERIM checkpoint, not
that final one), #283 (launch `@fm` review agent on the whole Round 13 wave,
per the original `/babygoal` instruction "в конце обнови чейнджлог, сделай
/checkpoint и запусти @fm ревью агента").

**Known but not yet auto-generated**: doc-staleness self-heal
(`tests/no_stale_doc_references.rs`) may need another README.md/
ARCHITECTURE.md pass once #279's Cargo.toml change is committed alongside
any new files — not yet checked for this specific edit (Cargo.toml alone
doesn't need it, but worth a final `no_stale_doc_references` run before
closing #279).

## Active goal
None via `/goal` — this session runs on `/babygoal`'s TaskList-driven model.
A babysit cron (`edb97149`, every 15 min, session-only) has been ticking
throughout and resuming/reporting on #279 across several ticks earlier in
the session; it is presumably still armed (not explicitly checked in this
exact moment, since `/checkpoint` interrupted before that would happen
naturally).

## TaskList
### in_progress
- #279 R13-9: решение о promotion class-aware-dirty в production (после R13-1 и R13-5)

### pending
- #280 R13-10 (процесс): wave-отчётность — production A/B/B/A отчёт, машинно-читаемые результаты, гигиена комментариев
- #281 R13: обновить CHANGELOG.md за Round 13  (blockedBy: #279, #280)
- #282 R13: взять чекпойнт сессии по итогам Round 13 (/checkpoint)  (blockedBy: #281)
- #283 R13: запустить @fm ревью-агента по итогам Round 13  (blockedBy: #282)

### recently completed
- #271 R13-1 (P0): coarse-only OOM latch, class-aware-dirty
- #284 R13-11 (P0, found mid-verification): lost-wakeup test bug, not production
- #272 R13-2 (P1): NUMA bucket-slot reuse + clear_bit fix
- #273 R13-3 (P1): virgin-zero-skip through magazine + drain prelude
- #285 R13-12 (P1, found mid-verification): drain_heap_overflow compile gap
- #275 R13-5 (P1): feature-isolated CI + loom wiring + inventory guard
- #274 R13-4 (P1): page-run SUPERSEDED -> DEFERRED
- #276 R13-6: exact-span-large+large-reserved-capacity gate, CONDITIONAL-GO
- #277 R13-7: Large cache extended to 40 slots, ~99x turnover win
- #278 R13-8: 256-2048 live-object judge, found real MAX_SEGMENTS=1023 wall

## Decisions
- User confirmed (AskUserQuestion) promoting `class-aware-dirty` into
  `production` — the GO recommendation from #279's A/B gate was accepted as-
  is, no further conditions requested.
- User earlier confirmed (from the prior session, still standing) promoting
  `primordial-lazy-commit` into production during Round 12 — unrelated to
  this session but part of the same `production` list's history.
- #276's `exact-span-large`+`large-reserved-capacity` pair got a
  CONDITIONAL-GO (not GO) recommendation and was deliberately NOT promoted
  — the iai `realloc_grow` regression (+102.3%) is real and unresolved; no
  user prompt was made since the gate itself didn't clear to an unconditional
  recommendation (matches the task's own instruction: only ask about
  promotion if the gate is clean).
- Chose to fix three self-discovered measurement bugs in #277 personally
  (not by re-delegating to a fresh sub-agent) given their tightly-scoped,
  mechanical nature (stale doc reference, then two rounds of size-list
  correctness) — each was verified with its own red/green run before moving
  on.
- Chose runtime-computed size lists (querying `dbg_small_class_count`/
  `dbg_block_size` at test/example start) over hardcoded constants for both
  #277 and #278's new test/example code, following the precedent task #265/
  R12-14 set for surviving `--all-features`'s `medium-classes-wide` boundary
  shift — treated as this project's now-established convention for this
  exact class of feature-interaction bug, not a one-off improvisation.

## Open questions
- None from the user's side — the one open item is MY OWN unfinished
  verification of #279 (does `cargo test --release --features production`
  pass with `class-aware-dirty` now in the default composition?), not a
  question for the user.

## Repo state
```
 M Cargo.toml
```
(Plus 22 untracked `docs/perf/_raw_*.log` scratch files from this and prior
sessions' bench/test runs — not real pending changes, never committed, same
as every prior checkpoint this project.)

```
bebd902 docs(perf): production A/B gate for class-aware-dirty (R13-9, task #279)
874650b docs(perf): judge 256-2048 live 260 KiB-2 MiB objects (R13-8, task #278)
df636ff feat(alloc-core): extend the Large cache beyond 8 slots via a lazy sidecar (R13-7)
3829d82 docs(perf): production A/B gate for exact-span-large + large-reserved-capacity (R13-6, task #276)
6018cf8 docs(perf): correct page-run verdict from SUPERSEDED to DEFERRED (R13-4, task #274)
```

Local `main` has NOT been pushed this session (no push requested yet — the
last confirmed push was earlier, at the end of Round 12). The uncommitted
`Cargo.toml` edit (class-aware-dirty -> production) is the only real pending
change; it will be committed as part of finishing #279, after the
in-flight production test suite run is confirmed green.
