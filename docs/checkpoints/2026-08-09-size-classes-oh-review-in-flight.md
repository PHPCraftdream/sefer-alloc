# Checkpoint — 2026-08-09 [size-classes-oh-review-in-flight]

## Session summary

This session has been executing a standing, multi-turn instruction
(established via an earlier `/babygoal` invocation, re-confirmed multiple
times across the session's history): process the `/rust-intel` audit sweep
one crate at a time, never advance to the next crate until ALL of the
current crate's tasks are closed; after each crate's fixes land, run
`/checkpoint`, update `CHANGELOG.md`, commit all markdown, and run an `@oh`
closing review — repeating this pattern for every crate in the sequence
(sefer-region → tagged-index-stack → racy-ptr-cell → aligned-vmem →
numa-shim → size-classes). A second standing instruction: act
independently on maintainer-level API/architecture decisions, documenting
the reasoning in commit messages rather than asking the user.

**Where things stand right now:** five of six crates are FULLY complete
(fix-tasks + post-work chain + closing review + any review follow-ups, all
landed and committed). The sixth and LAST crate, `size-classes`, has its 5
fix-tasks (#701, #728-731) and its post-work chain through the checkpoint/
CHANGELOG/markdown-commit steps (#752-754) all done and committed. The
FINAL step of the entire six-crate sweep — task #755, the `@oh` closing
review for size-classes' round — was just launched as a background agent
(agent id `a00d7e2073ecf5fb3`, prompt asked it to verify all 5 commits'
counterfactual claims, re-derive the hand-computed golden-value arithmetic
independently, check for `#[non_exhaustive]` enforcement gaps, and
cross-check the newly-added CHANGELOG sections for accuracy — including
retroactively verifying the numa-shim-round-closing-review-follow-up
commits `f97bf1d`/`fd2a3bb` this same CHANGELOG update described for the
first time). **This review has NOT yet returned a result** — the session
was interrupted (user ran `/checkpoint`) before the agent's completion
notification arrived. This checkpoint exists specifically so a
fresh session or a resumed one can pick up the pending review without
re-launching a duplicate agent.

**What happened immediately before size-classes' round:** numa-shim's own
round was closed out completely first, including its `@oh` closing review
(task #751), which found a genuine HIGH defect — task #723's
`OnceLock<Vec<Vec<u8>>>` topology cache performed heap allocation on the
exact `AllocCore::alloc` path this repo's own M5 invariant declares
reentrancy-free, which under a real Linux `#[global_allocator]` +
`numa-aware` deployment would alias a `&mut HeapCore` (UB) and then
deadlock via a reentrant `OnceLock::get_or_init`. Task #777 fixed this by
redesigning the cache as a fixed-size, allocation-free struct (`[[u8;
1024]; 64]` + `[usize; 64]`), eliminating the heap allocation entirely.
Task #778 closed the review's remaining 12 findings (F2-F13): corrected an
INVERTED Windows `VirtualAllocExNuma` mechanism claim (the code's own
comments said `node` "has no effect" on the reserve call and "takes
effect" on the commit call — backwards per Microsoft's actual documented
`nndPreferred` contract), added a genuine `VirtualQuery`-based regression
test for the earlier `#724` commit-charge fix (the round's OWN prior
"empirically verified" evidence was shown to pass identically against the
reverted bug — a real gap in that earlier verification), filed 4 new open
items plus closed 1 stale one in `docs/CORRECTNESS_OPEN_ITEMS.md`, and
closed 8 further LOW/INFO hygiene findings including adding clippy CI
coverage this crate had zero of. This "genuine HIGH bug shipped, caught by
closing review" pattern has now recurred in every single crate's round
this sweep has processed (tagged-index-stack #771, racy-ptr-cell #773,
aligned-vmem #775, numa-shim #777) — the `@oh`-review step is empirically
earning its keep every round, which is why size-classes' own pending
review (#755) should NOT be skipped even though this round's fixes all
looked clean during personal zero-trust verification.

**size-classes' round itself:** 5 fix-tasks, all committed with genuine
EMPIRICAL zero-trust counterfactuals (this crate, unlike numa-shim, has no
platform-`#[cfg]` gating at all — every fix runs natively on this Windows
session). Task #701 (the crate's own audit's highest-severity finding — a
bare `cur * num` multiply in the geometric-advance step could silently
wrap in a release profile, masked by the min-step fallback into a
valid-looking-but-wrong table; fixed with `checked_mul`/`checked_add`;
notably the counterfactual needed `--release` specifically, since debug
mode alone still panicked for an unrelated reason — a separate untouched
bare `+` in the min-step fallback trips debug's own overflow-checks
regardless of the multiply's guard state). Task #728 (added
`#[non_exhaustive]` + a `const fn Params::new(...)` constructor to the
all-pub-field `Params` config struct before this crate's first publish;
updated 10 construction sites workspace-wide including this repo's own
root `src/alloc_core/size_classes.rs`). Task #729 (documented + added a
`debug_assert!` for `class_for`'s previously-unchecked
power-of-two-`align` precondition, silently violated by both its fast and
slow paths for a non-pow2 align). Task #730 (3 test-hygiene fixes: an
ambiguous `#[should_panic]` substring that could match the wrong panic
site; a circular table-geometry oracle sharing its rounding formula with
the code under test, closed with 8 hand-derived golden values; an
`is_huge` test whose comment promised cross-scheme proof the body never
delivered). Task #731 (4 small doc/validation residuals: an unasserted
growth denominator, `size2class_len`'s missing guard, a contradictory
struct-level no-panics claim, a README understatement of the `extras`
preconditions).

## Active goal

None — no `/goal` Stop hook is armed in this session. Progress is tracked
via the TaskList per the standing `/babygoal`-established pattern (a
`# babysit tick` cron job resumes work on stalls; this checkpoint does NOT
disarm that cron).

## TaskList

### in_progress
- #755 Post-work (size-classes): run @oh final review of all round work — **background agent already launched, id `a00d7e2073ecf5fb3`, result not yet returned as of this checkpoint.** Do NOT re-launch a duplicate review agent; either wait for the existing agent's completion notification, or if resuming in a context where that agent is no longer reachable, check whether `docs/reviews/2026-08-09-size-classes-round-closing-review.md` already exists on disk before deciding whether to relaunch.

### pending
- #656-661 publish-readiness tasks for all six crates (independently gated, not part of the active sweep order)
- #662-663, #756-768 bench-scale-tool / captrack assessment tasks (independently gated behind each crate's own closing review)
- #673 sefer-region contended-SyncRegion measurement — perpetually deferred, unverified-no-defect item, not part of the active sweep
- Any follow-up tasks task #755's review generates once it returns (not yet created — depends on the review's findings)

### recently completed
- #754 Post-work (size-classes): commit all markdown docs from this round
- #753 Post-work (size-classes): update CHANGELOG.md with the round
- #752 Post-work (size-classes): /checkpoint after #701,728-731 land
- #731 size-classes: 4 doc/validation residuals
- #730 size-classes: 3 test-hygiene defects
- #729 size-classes: class_for non-pow2-align precondition
- #728 size-classes: Params non_exhaustive + const constructor decision
- #701 size-classes: geometric-advance overflow fix
- #778 numa-shim: F2-F13 round-closing-review bundle
- #777 numa-shim: F1 (HIGH) OnceLock topology cache reentrancy fix

## Decisions

- Task #728's Params API decision: `#[non_exhaustive]` + `const fn new(...)`
  over declaring the struct frozen — field growth is plausible (audit
  named `small_align_max` as an obvious future knob).
- Task #729's precondition-violation severity: `debug_assert!`, not a
  release-active `assert!`, since the failure mode is a wrong class choice,
  not memory unsafety — explicitly contrasted with task #701's promotion
  to a release-active assert for a table-corruption-shaped defect.
- Task #723/#777 (numa-shim): chose to eliminate the heap allocation from
  the topology cache entirely (fixed-size struct) rather than guard
  against the reentrancy with a fail-open mechanism, per the review's own
  suggested closure — removes the hazard structurally.
- Task #726/numa-shim's §C10 mock-feature-unification finding: applied
  the SAME doc-only policy already decided for aligned-vmem's identical
  finding (task #715), per that commit's own explicit note that the
  policy should carry over.

## Open questions

None outstanding from the user's perspective — all maintainer-level
decisions above were made independently per this session's standing "act
independently" instruction. The only open item is procedural: task #755's
`@oh` review result is pending and needs to be read/acted on when it
returns (expect it to generate 1-2 follow-up tasks, matching every prior
crate's pattern in this sweep, before size-classes' round — and the whole
six-crate sweep — can be marked genuinely complete).

## Repo state

```
(clean — nothing to commit, working tree clean)
```
```
9018c07 docs: commit checkpoint after size-classes round fully closed (task #754)
d1a4031 docs: update CHANGELOG.md with the numa-shim closing-review follow-ups and the size-classes remediation round (task #753)
9d2d2fa fix(perf): 4 doc/validation residuals -- unasserted growth denominator, size2class_len's missing guard, contradictory no-panic claim, README understates extras preconditions (task #731)
d07102a test(size-classes): 3 test-hygiene defects -- ambiguous should_panic substring, circular table oracle, is_huge under-delivering on its own comment (task #730)
5741243 fix(perf): class_for's non-pow2-align precondition was undocumented and unchecked (task #729)
a80ba49 fix(perf): decide Params' publish-blocking API posture -- non_exhaustive + const constructor (task #728)
7ffeba5 fix(perf): geometric-advance overflow was silently masked into a wrong-but-valid-looking table (task #701)
fd2a3bb fix(perf), docs, test, CI: F2-F13 bundle from the numa-shim round-closing review (task #778)
```
