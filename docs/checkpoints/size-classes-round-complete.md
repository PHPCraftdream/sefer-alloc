# Checkpoint — 2026-08-09 [size-classes-round-complete]

## Session summary

This checkpoint closes size-classes' crate-by-crate remediation round, the
SIXTH and LAST crate in the standing `/rust-intel` audit sweep sequence
(sefer-region → tagged-index-stack → racy-ptr-cell → aligned-vmem →
numa-shim → **size-classes**). The governing instruction for the whole
sweep (established earlier in this session, still standing): process one
crate at a time, close ALL of its tasks before advancing, commit between
tasks, and after each crate's fixes land run `/checkpoint`, update
`CHANGELOG.md`, commit all markdown, and run an `@oh` closing review — same
pattern repeated for every crate. A second standing instruction: act
independently on maintainer-level API/architecture decisions, documenting
the reasoning in commit messages rather than surfacing via AskUserQuestion.

Before starting size-classes, this session finished numa-shim's own round
completely: its 9 fix-tasks (#697, #720-727), its post-work chain
(#748-751), and — critically — its closing-review follow-ups (#777-778).
The `@oh` closing review for numa-shim (task #751) found a genuine HIGH
defect: task #723's `OnceLock<Vec<Vec<u8>>>` topology cache performed heap
allocation on the exact `AllocCore::alloc` path that `sefer-alloc`'s own M5
invariant declares reentrancy-free, which — under a real Linux
`#[global_allocator]` + `numa-aware` deployment — would alias a `&mut
HeapCore` (UB) and then deadlock via a reentrant `OnceLock::get_or_init`.
Task #777 fixed this by redesigning the cache as a fixed-size,
allocation-free `Topology` struct (`[[u8; 1024]; 64]` + `[usize; 64]`),
removing the reentrancy hazard structurally rather than guarding against
it. Task #778 closed the review's remaining 12 findings (F2-F13): corrected
an inverted Windows `VirtualAllocExNuma` mechanism claim, added a genuine
`VirtualQuery`-based regression test for task #724's commit-charge fix
(counterfactual-verified — the round's OWN prior "empirically verified"
test was shown to pass identically against the reverted bug), filed four
new open items plus closed a stale one in `docs/CORRECTNESS_OPEN_ITEMS.md`,
and closed 8 further LOW/INFO doc/hygiene findings. This closed the "genuine
HIGH bug shipped and caught by closing review" pattern that has now
recurred in every crate's round this sweep (tagged-index-stack's #771,
racy-ptr-cell's #773, aligned-vmem's #775, numa-shim's #777) — the
`@oh`-review step is empirically earning its keep every single round.

size-classes' round then ran cleanly with NO closing-review follow-up bugs
found yet (task #755, the closing review itself, has not run as of this
checkpoint — that is the very next step). Five fix-tasks landed:
task #701 (the crate's own audit's highest-severity finding, MEDIUM §B26 —
the geometric-advance step's bare `cur * num` multiply could silently wrap
in a release profile, with the `next <= cur` min-step fallback masking the
wrap into a valid-looking-but-wrong table; fixed with `checked_mul`/
`checked_add` plus a real overflow-triggering regression test,
counterfactual-verified by reverting to `wrapping_mul` and confirming the
bug reproduces under `--release` specifically — debug mode alone was NOT a
valid counterfactual here, since a separate untouched bare `+` in the
min-step fallback still traps under debug's own overflow-checks, a genuine
methodological subtlety this task's own commit message documents in
detail); task #728 (a maintainer-level publish-blocking API decision,
§C1a — added `#[non_exhaustive]` to the all-pub-field `Params` config
struct plus a `const fn Params::new(...)` constructor in the same commit,
since plain `#[non_exhaustive]` alone would make the type unconstructable
in `const` context; updated all 10 construction sites across the
workspace including this repo's own root `src/alloc_core/size_classes.rs`,
confirmed real enforcement via the compiler's own E0639 errors before the
conversion); task #729 (§F2/§B26 — `class_for`'s undocumented
power-of-two-`align` precondition, silently violated by both its fast and
slow paths for a non-pow2 align; documented the precondition and added a
`debug_assert!`, deliberately NOT a release-active `assert!` since the
failure mode here is a suboptimal class choice, not memory unsafety —
contrast task #701's promotion to a release-active assert for a
table-corruption-shaped defect); task #730 (three test-hygiene defects,
§D1/§D1a/§F1 — an ambiguous `#[should_panic]` substring that could
coincidentally match a wrong panic site, counterfactual-verified by
corrupting the setup path and confirming the tightened test correctly
fails on the wrong message; a circular table-geometry oracle sharing its
exact rounding formula with the code under test, closed with 8
hand-derived golden values computed independently by hand arithmetic in
the test's own comment; an `is_huge` test whose comment promised
cross-scheme parameterization proof the test body never actually
delivered, closed by building a genuine second scheme and asserting
opposite verdicts, counterfactual-verified by temporarily hardcoding
`is_huge` and confirming the new assertion catches it); and task #731
(four small INFO doc/validation residuals, batched — an unasserted growth
denominator, `size2class_len`'s missing `min_block` power-of-two guard
(the crate's one previously-unguarded `pub fn`), a struct-level "no
panics on the lookup path" claim directly contradicted by `block_size`'s
own documented panic, and a README line understating the machine-checked
`Params::extras` preconditions). Every fix in this crate's round got a
genuine zero-trust counterfactual verification (revert, confirm the
regression test fails for the right reason, restore, confirm zero net
diff) — this crate's Rust code all runs natively on this Windows session
host (no Linux-only gating unlike numa-shim), so every verification here
is EMPIRICAL, not REASONED-FROM-SPEC.

All native verification used: `cargo test -p size-classes --all-features`,
`cargo clippy -p size-classes --all-features --all-targets -- -D
warnings`, `cargo build -p size-classes --target thumbv7em-none-eabi`
(this crate's own advertised `no_std` bare-metal target), `cargo fmt -p
size-classes -- --check`, and `cargo doc -p size-classes --all-features
--no-deps`. Every task additionally re-verified the ONE real in-repo
consumer — this workspace's own root `sefer-alloc` crate, which
instantiates `size-classes` via `src/alloc_core/size_classes.rs` — still
compiles and lints clean under `--features "production internals"` after
each change. `scripts/verify-commit-prefixes.mjs` was run after every
commit and PASSed each time, with only pre-existing warning noise from
commits outside this session's own work.

Immediately before this checkpoint, task #752 (this checkpoint) was
marked in_progress. The remaining post-work chain for size-classes is
#753 (update CHANGELOG.md with the round), #754 (commit all markdown from
this round), and #755 (run the `@oh` closing review) — none of these have
started yet. After #755 closes (including any follow-up fix tasks the
review generates, per the established pattern from every prior crate —
each of the five prior crates in this sweep needed at least one HIGH
closing-review follow-up), this is the LAST task of the entire six-crate
`/rust-intel` sweep this session has been executing.

## Active goal

None — no `/goal` Stop hook is currently armed in this session. Progress
is tracked via the TaskList per the standing `/babygoal`-established
pattern (a `# babysit tick` cron job resumes work on stalls).

## TaskList

### in_progress
- #752 Post-work (size-classes): /checkpoint after #701,728-731 land — this task, being closed by this very checkpoint write

### pending
- #753 Post-work (size-classes): update CHANGELOG.md with the round (blockedBy: none, next up)
- #754 Post-work (size-classes): commit all markdown docs from this round
- #755 Post-work (size-classes): run @oh final review of all round work — the LAST task-generating step of the entire 6-crate sweep
- #656-661 publish-readiness tasks for all six crates (independently gated, not part of the active sweep order)
- #662-663, #756-768 bench-scale-tool / captrack assessment tasks (independently gated behind each crate's own closing review — numa-shim's #756-757 and size-classes' #761 are now unblockable once #751/#755 close)
- #673 sefer-region contended-SyncRegion measurement — perpetually deferred, unverified-no-defect item, not part of the active sweep

### recently completed
- #731 size-classes: 4 doc/validation residuals
- #730 size-classes: 3 test-hygiene defects
- #729 size-classes: class_for non-pow2-align precondition
- #728 size-classes: Params non_exhaustive + const constructor decision
- #701 size-classes: geometric-advance overflow fix
- #778 numa-shim: F2-F13 round-closing-review bundle
- #777 numa-shim: F1 (HIGH) OnceLock topology cache reentrancy fix
- #751/#750/#749/#748 numa-shim post-work chain

## Decisions

- Task #728's Params API decision: chose `#[non_exhaustive]` + a `const fn
  new(...)` constructor over the alternative (explicitly declaring the
  struct's field set frozen) — field growth is plausible (the audit itself
  named `small_align_max` as an obvious future knob currently hardwired to
  `min_block`), matching this sweep's established pattern for identical
  decisions (aligned-vmem's task #715, numa-shim's task #726).
- Task #729's precondition-violation severity: chose `debug_assert!` over
  a release-active `assert!` for `class_for`'s non-pow2-align guard,
  explicitly because the failure mode is a suboptimal/wrong class choice,
  not memory unsafety or table corruption — deliberately different from
  task #701's promotion to a release-active assert, with the distinction
  documented inline in both commits.
- Task #701's counterfactual methodology: debug mode alone was
  insufficient to prove the pre-fix bug (a separate untouched bare `+` in
  the min-step fallback still traps under debug's own overflow-checks
  regardless of the multiply's guard state) — the counterfactual had to
  run under `--release` specifically to observe the genuine silent-wrap
  bug the audit describes. This is recorded in the commit message as a
  methodological note for future rounds facing similar layered-overflow
  scenarios.

## Open questions

None outstanding — no user-input-blocking questions arose this round; all
maintainer-level decisions above were made independently per this
session's standing "act independently" instruction and documented in
each task's own commit message.

## Repo state

```
(clean — nothing to commit, working tree clean)
```
```
9d2d2fa fix(perf): 4 doc/validation residuals -- unasserted growth denominator, size2class_len's missing guard, contradictory no-panic claim, README understates extras preconditions (task #731)
d07102a test(size-classes): 3 test-hygiene defects -- ambiguous should_panic substring, circular table oracle, is_huge under-delivering on its own comment (task #730)
5741243 fix(perf): class_for's non-pow2-align precondition was undocumented and unchecked (task #729)
a80ba49 fix(perf): decide Params' publish-blocking API posture -- non_exhaustive + const constructor (task #728)
7ffeba5 fix(perf): geometric-advance overflow was silently masked into a wrong-but-valid-looking table (task #701)
```
