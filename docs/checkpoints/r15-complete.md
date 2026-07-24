# Checkpoint — 2026-07-24 05:10 [r15-complete]

## Session summary
This is the same long-running session that completed Round 13 and Round 14.
Round 15 (tasks #303-#308, R15-1..R15-6) executed the follow-up queue against
an `@fh` review of the Round 14 wave, per the user's standing instruction
(established after Round 14 finished) to keep iterating past each review:
file leaf tasks from the findings and EXECUTE them, not just report them.
The Round 15 plan is recorded in `docs/reviews/2026-07-24-r15-plan.md`
(committed alongside this checkpoint). Every task followed the session's
standing zero-trust discipline: delegate to `@sh` sub-agents, personally
read every diff line-by-line, personally re-run the tests under the exact
feature combination each change targets, personally reproduce red/green
counterfactuals for safety/behavior-relevant changes (R15-3's promotion
gate, R15-4's loom litmus test — both independently confirmed via a real
red-before/green-after reproduction, not trusted from the sub-agent's own
claim), run clippy across default/experimental/`--all-features` + fmt +
`no_stale_doc_references` before marking each task complete.

**All 6 planned tasks (#303-#308) are completed, personally verified, and
committed:**

- **R15-1** (`7224670`, #303, P1): post-raise perf baseline for
  `MAX_SEGMENTS=4096` — drain-scan cost is a flat, bootstrap-attributed Ir
  bump (not the scan itself); the real finding is sidecar footprint scaling
  exactly ×4 as the `WORDS_PER_CLASS`/`DIRTY_BITMAP_WORDS` formula predicts
  (both opt-in features, bounded cost). New `dbg_words_per_class()` accessor.
- **R15-2** (`6745350`, #304, P1): `sidecar::reserve_zeroed_with` → `unsafe fn`
  with explicit `# Safety` — closed the exact class of gap R14-9's sidecar
  primitive was built to eliminate. Sole call site (`os.rs`) updated.
  README unsafe-inventory counts unchanged (76 total: 20 tier-1/56 tier-2 —
  personally reproduced via the sanctioned grep both before and after).
- **R15-3** (`4de4ef2`, #305, P2, largest/most substantive): the
  `medium-classes` × `exact-span-large` realloc-promotion headroom
  pessimization — root-caused and fixed at the SOURCE via a compile-time
  `#[cfg]` gate (not a third test-weakening, per explicit user instruction).
  Promotion now compiles in only when it structurally cannot regress
  (`!exact-span-large || (large-reserved-capacity && !numa-aware)`). Verified
  with a personal red/green counterfactual: applied the new tests against
  the OLD (pre-fix) production code in a throwaway `git worktree` and
  confirmed 2 genuine test failures — proving the fix and its tests are
  real, not vacuous.
- **R15-4** (`2662f38`, #306, P2): unjoined loom litmus test for
  `sidecar_oom_latch`'s Acquire/Release pairing — the existing R14-2 test
  joined both producers before its consumer visit, so `join()`'s own
  happens-before made the assertion pass under any ordering. New spin-wait
  (no `join()`) test + a permanent `#[should_panic]` Relaxed-ordering
  counterfactual proving non-vacuity — personally re-ran under loom
  (`RUSTFLAGS="--cfg loom"`), confirmed 10/10 pass including both new tests.
- **R15-5** (`e262559`, #307, P2): pure doc-drift cleanup — 5 files' stale
  `WORDS_PER_CLASS=16`/`MAX_SEGMENTS=1024`/KiB-footprint numbers corrected
  to match the post-R14-7 reality (×4 growth). Personally confirmed the
  diff touches only comment lines, zero executable-code changes.
  Arithmetic independently re-verified by hand.
- **R15-6** (`4643e9a`, #308, P3): compile-time `align_of::<T>() <= PAGE`
  assert added to `sidecar::reserve`/`reserve_zeroed_with` — formalizes a
  prose-only doc claim; `const { assert!(...) }` inline block (MSRV 1.88,
  stable since 1.79).

**No feature-list change this round** — `Cargo.toml`'s `production = [...]`
is byte-identical to the end of Round 14.

**CHANGELOG updated** (this task, #309, not yet committed at the time this
checkpoint is written — both will be committed together in the same commit
as this checkpoint file and the Round 15 plan doc, per the established
Round 13→14 and Round 14→15 boundary pattern).

**Not yet done as of this checkpoint:** commit CHANGELOG + this checkpoint +
`docs/reviews/2026-07-24-r15-plan.md` + the stale interim checkpoint
`docs/checkpoints/2026-07-24-r15-in-progress.md`, run `npm run check`, push
to `origin/main`, watch CI, then launch the review agent for Round 15 (the
user's standing instruction implies another review cycle should follow, but
the exact review agent for THIS round has not been explicitly specified by
the user the way `@fh` was explicitly named for Round 14 — defaulting to
`@fh` again unless told otherwise, since that is the most recently
explicitly-requested reviewer and no override has been given for Round 15).
Task #309 (this wrap-up) remains `in_progress` until all of this completes.

## Active goal
Same standing situation as the prior two checkpoints: a `/goal` Stop hook is
nominally active with text mentioning `@fm` (superseded by the user's
explicit `@fh` override, itself now a round old) — the session is operating
on the user's direct follow-up instruction to keep iterating past each
review cycle, not on the original goal text's literal condition.

## TaskList
### in_progress
- #309 R15 wrap-up: CHANGELOG (done), checkpoint (this file), commit docs,
  push, then review + follow-up tasks — commit/push/review steps still
  pending

### recently completed
- #308 R15-6: sidecar align_of::<T>() <= PAGE compile-time assert
- #307 R15-5: doc-drift cleanup after MAX_SEGMENTS=4096 raise
- #306 R15-4: unjoined loom litmus test for sidecar_oom_latch
- #305 R15-3: medium-classes × exact-span-large headroom pessimization fixed at the source
- #304 R15-2: sidecar::reserve_zeroed_with → unsafe fn
- #303 R15-1: MAX_SEGMENTS=4096 post-raise perf baseline + drain-scan cost measurement

## Decisions
- R15-3 was resolved via a genuine code-level `#[cfg]` gate rather than a
  third round of test-assertion weakening — explicitly required by the user
  ("НЕ третье подряд ослабление теста ... либо реальный код-фикс, либо явный
  измеренный вердикт"). Verified the fix is real (not cosmetic) via a
  personal red/green counterfactual using a throwaway `git worktree`.
- R15-4's non-vacuity was proven with a PERMANENT `#[should_panic]`
  counterfactual test (mirroring the file's own established pattern for
  `no_latch_visit_and_drain`/`class_scoped_visit_and_partial_drain`) rather
  than a temporary hand-edit-and-revert — keeps the non-vacuity proof
  re-runnable by anyone, not just documented in a commit message.
  Independently re-ran this myself under loom to confirm 10/10 pass.
  Same convention as `no_latch_visit_and_drain`/`class_scoped_visit_and_partial_drain`.
- R15-1 deliberately did NOT refresh README/`IAI_BASELINE.md` — reasoned
  that no `production` composition changed this round, so there is nothing
  to re-pin the canonical table against (the project rule requiring a
  refresh triggers specifically on `production` changes).
- Chose to keep the CHANGELOG/checkpoint/plan-doc commit pattern identical
  to the Round 13→14 and Round 14→15 boundaries: planning docs stay
  uncommitted until the wave's own wrap-up task commits them together with
  the checkpoint, not committed incrementally mid-wave.

## Open questions
- Still unresolved from Round 14, carried forward: whether to promote
  R14-6's clean-GO `large-reserved-capacity` growth-factor-4x recommendation
  via `AskUserQuestion` — not yet asked, not blocking Round 15's queue or
  this wrap-up.
- Which review agent to use for Round 15's review step: defaulting to `@fh`
  (most recently explicitly requested) absent an explicit override for this
  round.

## Repo state
```
 M CHANGELOG.md
?? docs/checkpoints/2026-07-24-r15-in-progress.md
?? docs/checkpoints/r15-complete.md
?? docs/reviews/2026-07-24-r15-plan.md
```

```
4643e9a fix(alloc-core): add compile-time align_of::<T>() <= PAGE assert to sidecar reserve fns (R15-6, task #308)
e262559 docs(alloc-core): fix stale MAX_SEGMENTS=1024/WORDS_PER_CLASS=16 doc numbers after R14-7 raise (R15-5, task #307)
2662f38 test(alloc-core): add unjoined loom litmus test for sidecar_oom_latch Acquire/Release pairing (R15-4, task #306)
4de4ef2 fix(alloc-core): gate medium-classes realloc promotion off under zero-headroom exact-span-large (R15-3, task #305)
6745350 fix(alloc-core): make sidecar::reserve_zeroed_with an unsafe fn (R15-2, task #304)
7224670 perf(docs): measure MAX_SEGMENTS=4096 drain-scan + sidecar footprint (R15-1, task #303)
```

Local `main` is 6 commits ahead of `origin/main` (Round 15's full range,
`7224670`..`4643e9a`) — NOT yet pushed. This wrap-up task (#309) will run
`npm run check`, commit the docs above, push, and watch the resulting CI run.
