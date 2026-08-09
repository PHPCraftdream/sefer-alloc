# Checkpoint — 2026-08-09 [size-classes-round2-complete]

## Session summary

This checkpoint closes the SECOND (review-followup) round for `size-classes`
— the sixth and LAST crate in the standing `/rust-intel` audit sweep
(sefer-region → tagged-index-stack → racy-ptr-cell → aligned-vmem →
numa-shim → **size-classes**). The governing instruction for the whole
sweep: process one crate at a time, close ALL of its tasks before
advancing, commit between tasks, and after each crate's fixes land run
`/checkpoint`, update `CHANGELOG.md`, commit all markdown, and run an `@oh`
closing review — same pattern repeated for every crate, including a
SECOND pass when the closing review itself finds real defects (as it did
here, and as it did for every other crate in this sweep so far).

size-classes' FIRST round (5 fix-tasks #701/#728-731, post-work #752-754)
finished cleanly and task #755 (the `@oh` closing review) was launched.
The FIRST review agent (id `a00d7e2073ecf5fb3`) died mid-stream with
"Connection closed" after 1.7M ms and wrote no report — the identical
failure mode numa-shim's own closing review hit three times earlier this
session. Per the established recovery pattern, a completely fresh review
agent (id `aba2263839dfd5e96`) was launched with the same self-contained
prompt rather than attempting to resume the dead one, and this one
succeeded, writing `docs/reviews/2026-08-09-size-classes-round-closing-
review.md` (9 findings: F1-F3 HIGH, F4-F5 MEDIUM, F6-F9 LOW/INFO).

**The review found `main` was genuinely RED** — a first for this sweep
(every prior crate's closing review found a real bug, but none had
actually broken CI on `main` at the time of review). F1: task #729's new
`debug_assert!(align.is_power_of_two())` in `class_for` fired against an
EXISTING, CI-covered root test (`tests/medium_classes_correctness.rs`'s
`item1_mib_alignment_resolves_to_small_not_large`, which looped
`MEDIUM_SIZES` — 3 of 6 values non-power-of-two — as the `align`
argument), breaking 3 whole-suite CI feature rows; #729's own justifying
rustdoc claim ("every real caller in this repo derives `align` from
`Layout`, which already guarantees power-of-two") was simply false, and
one grep would have shown it. F2: `size-classes` had ZERO `cargo test`
step anywhere in CI — only a `cargo build --target thumbv7em-none-eabi`
cross-build that compiles no test target — the identical gap class this
same sweep already closed for tagged-index-stack (#772) and racy-ptr-cell
(#773), but nobody had checked the sixth crate. F3: the natural fix for
F2 (add a `--release` CI row, mirroring the racy-ptr-cell pattern)
immediately exposed a third bug — task #729's `#[should_panic]` test
targeted a `debug_assert!` that compiles away entirely in `--release`.

Task #779 (this session, commit `2ca3537`) fixed all three together:
restricted the align-axis loop in the root test to the pow2 members of
`MEDIUM_SIZES` (the size-axis loop already covered all six values
correctly), corrected the false rustdoc claim, gated the release-
incompatible test behind `#[cfg(debug_assertions)]`, and added both a
debug and `--release` `cargo test -p size-classes` row to `ci.yml`. Every
piece was zero-trust counterfactual-verified personally: reverted the
test-loop restriction and confirmed the new CI row reproduces the exact
F1 panic message; reverted the `#[cfg(debug_assertions)]` gate and
confirmed `--release` fails exactly as F3 predicted; re-ran all three
previously-failing root CI feature combinations
(`hardened medium-classes internals`; `--all-features`;
`production medium-classes exact-span-large internals`) and confirmed all
green. `main` is no longer red.

Task #780 (commit `ab269a5`) closed the remaining 6 findings: F4 (MEDIUM)
— the geometric-advance min-step fallback's bare `+` shared the exact
overflow hazard task #701 fixed on its two neighbours (#701's own commit
message had named this exact line and left it), fixed with the same
`checked_add` pattern plus a new regression test, counterfactual-verified
by reverting and confirming the reverted state fails under `--release`
with the same silent-wrap signature F4 described; F5 (MEDIUM) — the
crate's README example, the crates.io/docs.rs front page, still used the
struct-literal syntax task #728 made a hard compile error, fixed and a
`#[non_exhaustive]`/`Params::new` note added; F6 (LOW) — the root
workspace's own shim over the crate (`src/alloc_core/size_classes.rs`)
still carried stale "no panics" doc claims #731 had already corrected on
the crate side; F7/F8/F9 (LOW/INFO) — three append-only CHANGELOG
corrections (a dropped numa-shim publish-gate caveat, an off-by-one
construction-site count, and a wrong "const-eval-time or debug-only"
characterization of two release-active runtime guards).

Both tasks verified clean: `cargo test -p size-classes --all-features`
(11/11 debug, 10/10 release — the extra debug test is F3's
`#[cfg(debug_assertions)]`-gated test), `cargo clippy -p size-classes
--all-features --all-targets -- -D warnings` clean, `cargo fmt -p
size-classes -- --check` clean, and `cargo build -p sefer-alloc --features
"production internals"` confirms the root crate still compiles against
the shim doc changes. `scripts/verify-commit-prefixes.mjs` PASSed after
both commits, with only pre-existing unrelated warning noise.

This is the LAST piece of work in the entire six-crate `/rust-intel`
sweep. Once this checkpoint, the CHANGELOG update (#782), and the
markdown commit (#783) land, the whole sweep — sefer-region through
size-classes, every crate's fix round AND every crate's closing-review
follow-up round — is genuinely complete, with no further `@oh` review
planned for this second round (per this sweep's established practice: one
review per crate closes the loop; review-followup fixes are not
themselves re-reviewed, since none of the prior 4 crates' followup rounds
were either).

## Active goal

None — no `/goal` Stop hook is armed in this session. Progress is tracked
via the TaskList per the standing `/babygoal`-established pattern (a
`# babysit tick` cron job resumes work on stalls).

## TaskList

### in_progress
- #781 Post-work (size-classes round 2): /checkpoint after F1-F9 land — this task, being closed by this very checkpoint write

### pending
- #782 Post-work (size-classes round 2): update CHANGELOG.md with the round (blockedBy: #781, now unblocked)
- #783 Post-work (size-classes round 2): commit all markdown docs from this round (blockedBy: #782)
- #656-661 publish-readiness tasks for all six crates (independently gated, not part of the active sweep order)
- #662-663, #756-768 bench-scale-tool / captrack assessment tasks (independently gated behind each crate's own closing review — #761/size-classes is now fully unblockable once #781-783 close)
- #673 sefer-region contended-SyncRegion measurement — perpetually deferred, unverified-no-defect item, not part of the active sweep

### recently completed
- #780 size-classes: F4-F9 bundle (MEDIUM/LOW/INFO) from round-closing review
- #779 size-classes: F1+F2+F3 (HIGH) — main was red
- #755 Post-work (size-classes): run @oh final review of all round work (2 attempts — first agent died mid-stream, second succeeded)
- #754/#753/#752 size-classes round-1 post-work
- #701/#728/#729/#730/#731 size-classes round-1 fix-tasks

## Decisions

- **Review-agent recovery pattern reconfirmed**: when a background `@oh`
  review agent dies mid-stream with a connection error and writes no
  report, do NOT attempt to resume it — launch a completely fresh agent
  with the same self-contained prompt. This is the second time this exact
  failure mode has hit this session's closing reviews (numa-shim's #751
  needed 3 resume attempts before abandoning; size-classes' #755 died
  once and a fresh agent succeeded on the first try) — resuming a dead
  background agent's session appears to be unreliable in this environment,
  fresh launches are not.
- **F1's fix chose "change the test" over "widen class_for's contract"**
  (per the review's own explicit recommendation): the medium_classes test
  was using medium *sizes* as *aligns* opportunistically, not because
  non-pow2 align support is an intended feature — restricting the
  align-axis loop to pow2 values preserves #729's precondition instead of
  weakening it, and the size-axis loop two lines below already covers all
  six MEDIUM_SIZES values correctly (size, unlike align, carries no pow2
  precondition).
- **F3's fix chose `#[cfg(debug_assertions)]`-gating the test over
  promoting the guard to a release-active `assert!`**: promoting would
  have been a hot-path behavior change turning F1 into a shipped release
  panic instead of a debug-only test failure — explicitly the wrong
  direction per this project's own debug_assert-vs-assert severity
  convention (the failure mode is a suboptimal class choice, not memory
  unsafety).
- **No second closing review for this round-2 followup work**: matches
  the pattern already established by every prior crate in this sweep
  (tagged-index-stack, racy-ptr-cell, aligned-vmem, numa-shim) — the
  closing review's own followup fixes are not themselves put through
  another `@oh` review round.

## Open questions

None outstanding from the user's perspective — all decisions above were
made independently per this session's standing "act independently"
instruction. No procedural items remain open either: #781-783 are the
last three tasks needed to close the entire six-crate sweep, and none of
them depend on anything not already landed.

## Repo state

```
?? docs/checkpoints/2026-08-09-size-classes-oh-review-in-flight.md
?? docs/reviews/2026-08-09-size-classes-round-closing-review.md
```
```
ab269a5 fix(perf), docs, P2-P4: size-classes F4-F9 bundle from round-closing review (task #780)
2ca3537 fix(perf), test, CI: size-classes F1+F2+F3 (HIGH) from round-closing review -- main was red (task #779)
9018c07 docs: commit checkpoint after size-classes round fully closed (task #754)
d1a4031 docs: update CHANGELOG.md with the numa-shim closing-review follow-ups and the size-classes remediation round (task #753)
9d2d2fa fix(perf): 4 doc/validation residuals -- unasserted growth denominator, size2class_len's missing guard, contradictory no-panic claim, README understates extras preconditions (task #731)
d07102a test(size-classes): 3 test-hygiene defects -- ambiguous should_panic substring, circular table oracle, is_huge under-delivering on its own comment (task #730)
```
