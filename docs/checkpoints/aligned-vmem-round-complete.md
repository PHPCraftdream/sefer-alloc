# Checkpoint — 2026-08-09 [aligned-vmem-round-complete]

## Session summary

This session executes a `/babygoal`-driven crate-by-crate sweep of the workspace
sub-crates' `/rust-intel` audit findings from 2026-08-07, per the user's standing
instruction: process ONE crate at a time, never move to the next until ALL of the
current crate's tasks are closed; after each crate, `/checkpoint` + update
CHANGELOG.md + commit markdown + `@oh` closing review, then repeat. The sequence
is `sefer-region → tagged-index-stack → racy-ptr-cell → (workspace loom hoist,
task #711) → aligned-vmem → numa-shim → size-classes`, enforced structurally via
the TaskList's `blockedBy` graph.

**sefer-region, tagged-index-stack, and racy-ptr-cell are all fully closed**,
including every follow-up task each crate's own round-closing `@oh` review
surfaced.

**aligned-vmem's entire fix-task group is now closed**: #699 (HIGH), #711
(workspace loom hoist), #712, #713, #714, #715, #716, #717, #718, #719 — all 10
tasks landed as individual commits (`6b18834` through `55e71b0`), each with a
genuine zero-trust counterfactual verification before commit (temporarily
reverting the fix, confirming the associated test fails for the RIGHT reason,
then restoring). Two REAL, previously-undiscovered bugs were caught and fixed
purely as side effects of this round's own mandated re-verification, not the
tasks' original scope: (1) a vacuous-under-miri test in task #713's own earlier
work (fixed with `#[cfg_attr(miri, ignore)]`); (2) an eager-vs-lazy evaluation
bug in task #718's own `fetch_update` closure (`then_some` evaluates its
argument eagerly, causing an underflow panic that 4 of 5 tests in the affected
file immediately caught before commit).

Task #718 (`fault_injection`'s Relaxed payload-then-flag publish + non-atomic
FAIL_NEXT decrement) required genuinely trying — and failing — to build a
reliable regression test for a real hardware-scheduling-timescale race: neither
an 8-thread/200-call design nor a 32-thread/200-round `Barrier`-synchronized
design could reproduce the pre-fix race on this Windows host (10 runs, zero
failures against genuinely racy code) because real OS thread wake-up jitter
after a barrier release is orders of magnitude wider than the actual race
window. This was documented HONESTLY in the test's own doc comment rather than
overclaiming coverage — the test's real value is proving the fix introduces no
regression under concurrent load, not "catches a reintroduced race"; the actual
soundness guarantee rests on `fetch_update`'s atomic-by-construction semantics.

Task #719 closed 7 independent small hygiene residuals in one commit: a missing
SAFETY comment, two undocumented `let _ =` syscall discards, a blanket
`dead_code` allow that should have been narrowed to `mock`-only (matching an
established task #646/F8 precedent that missed this one item), a Drop-reachable
panic in `from_raw_parts` (fixed by validating `align` immediately at the
unsafe call site instead of deferring to `Drop`-time `Layout` construction
under miri — genuinely dangerous since a second panic during an active unwind
aborts the whole process), an untested `unsafe impl Send for Reservation {}`
(added a compile-time assertion mirroring `sefer-region`'s established
pattern), an `off_t` ABI-shape risk on the `mmap` FFI declaration (documented,
not code-fixed — narrowing deferred per CLAUDE.md's "don't design for
hypothetical future requirements" since no 32-bit target is currently
supported/tested), and a factually wrong doc claim ("`from_raw_parts` is the
inverse of `into_parts`" — false; the true structural complement of
`into_parts` is `release`, which shares its exact 3-tuple signature).

Every fix this session's aligned-vmem work touched was verified via: the native
Windows test suite (all green across every relevant feature combination),
`cargo clippy --all-features --all-targets -D warnings` (clean), `cargo fmt
--check` (clean), cross-compilation on `x86_64-unknown-{linux-gnu,freebsd,
netbsd}` (all clean; `x86_64-unknown-dragonfly`/`x86_64-unknown-openbsd` have
no prebuilt rustup std component on this host — REASONED-FROM-SPEC only via
their identical cfg arms to verified siblings), and `cargo +nightly miri test`
across the relevant feature combinations (all green except the pre-existing,
already-documented intentional leak in `leak_zeroed_pages_is_zeroed_and_static`,
tracked separately as `docs/CORRECTNESS_OPEN_ITEMS.md` item 41).

**Currently in flight: task #744 (`/checkpoint` — this file), the first of
aligned-vmem's own post-work chain** (#744 checkpoint → #745 CHANGELOG →
#746 commit-markdown → #747 `@oh` closing review). Once #747 lands, the sweep
advances to numa-shim's fix-task group (#697, #720-727).

A `# babysit tick` cron (job `a46e52be`, every 15 min, off-minute) has been
driving forward progress across ticks with no new user instruction needed
between them, correctly recognizing "signal present" (recent commits/edits)
on every tick during this session and continuing the in-flight work rather
than restarting cold.

## Active goal

Stop-hook condition (verbatim, from the original `/babygoal` invocation):
"идем по одному крейту. Н переходим к другому пока по текущему не закрыли все
задачи. реализуй задачи sefer-region с помощью /crush, между тасками делай
коммиты. После завершения всей работы сделай /checkpoint, обнови чейнджлог,
закомить все мд и запусти ревью агента @oh - пусть проверит всю работу - так
по каждому крейту"

(Note: sefer-region itself — the crate literally named in this text — is
already fully closed; per an explicit exchange with the user mid-session, "так
по каждому крейту" ["do this for each crate"] confirms the SAME pattern
[checkpoint → changelog → commit markdown → `@oh` review per crate] applies to
every crate in the sweep, not just sefer-region.)

## TaskList

### in_progress
- #656 sefer-region — verify/prepare for crates.io republish (perpetually
  blocked on a maintainer publish decision only the user can make; not part of
  the active sweep)
- #744 Post-work (aligned-vmem): `/checkpoint` after #699,711-719 land — this
  task, being closed by writing this very file

### pending (next up, in strict blockedBy order)
- #745 Post-work (aligned-vmem): update CHANGELOG.md with the round
- #746 Post-work (aligned-vmem): commit all markdown docs from this round
- #747 Post-work (aligned-vmem): run @oh final review of all round work —
  unblocks numa-shim's group
- #697,720-727 numa-shim fixes (0 HIGH, real bugs incl. mbind maxnode
  off-by-one) — blocked by #747
- #748-751 Post-work (numa-shim) — #751 unblocks size-classes group (LAST
  crate in the sweep)
- #701,728-731 size-classes fixes (0 HIGH, cleanest crate) — blocked by #751
- #752-755 Post-work (size-classes) — #755 is the final task of the entire
  crate-by-crate sweep
- #657-661,756-768 publish-readiness / bench-scale-tool / captrack tasks —
  independently gated behind each crate's own closing review, not part of the
  strictly sequential crate sweep itself
- #673 sefer-region: deliberately deferred future-decision-gate, not part of
  the active sweep

### recently completed (this session)
- #719 aligned-vmem: 7 hygiene residuals (commit `55e71b0`)
- #718 aligned-vmem: fault_injection's two data-race hazards (commit `b8b70fb`)
- #717 aligned-vmem: strict-provenance fix for the two native over-reserve
  paths (commit `94aef18`)
- #716 aligned-vmem: huge-pages mock coverage + miri-UB test fix (commit
  `81ecfe3`)
- #715 aligned-vmem: mock's Call variants + feature-unification hazard
  (commit `e5f6700`)
- #714 aligned-vmem: BSD `_SC_PAGESIZE` + hugetlb munmap leak (commit `2e7f4f5`)
- #713 aligned-vmem: VmemError errno-capture timing (commits `131355a`,
  `d6b72b1`)
- #712 aligned-vmem: recommit/commit_range contract-violation clamp (commit
  `54089fa`)
- #699 aligned-vmem: fault_injection.rs zero-CI-coverage (HIGH) (commit
  `6b18834`)
- #711 Workspace: loom dependency hoist (commit `56d0764`)

## Decisions

- **Task #718's regression-test honesty**: after empirically confirming (10
  runs across two test designs, including a 32-thread `Barrier`-synchronized
  version) that the pre-fix racy code could NOT be made to fail on this
  Windows host, chose to document this limitation explicitly in the test's own
  doc comment rather than either (a) silently shipping a test that LOOKS like
  a regression guard but isn't, or (b) abandoning the test entirely. The test
  stays as a genuine "no regression under real concurrent load" proof plus an
  executable spec of the intended contract; the actual soundness claim rests
  on `fetch_update`'s documented atomic semantics, not the test's ability to
  have caught the old bug.
- **Task #719's off_t fix**: chose documentation over a per-OS/per-arch code
  fix — `i64` is correct for every currently-supported/tested target, and the
  crate has no stated 32-bit-Unix support goal, so narrowing the type now
  would be designing for a hypothetical requirement (explicitly against
  CLAUDE.md's own stated principle).
- **Task #719's Drop-reachable panic fix**: chose to validate `align` at
  `from_raw_parts`'s own call site (an `assert!`) rather than either leaving
  the Drop-time panic as-is or trying to make Drop itself infallible (not
  possible — `Drop::drop` has no error-return channel). This makes the
  eventual Drop-time `Layout::from_size_align` call provably infallible for
  any value that passed through this crate's own construction paths.

## Open questions

None new since the last checkpoint
(`docs/checkpoints/2026-08-09-crate-sweep-aligned-vmem.md`). Carried over,
still unresolved: the sefer-region 0.1.1 version-bump decision (task #656's
gate — genuinely requires the user), and
`docs/reviews/2026-08-05-release-readiness-gap-audit.md`'s NO-GO verdict for a
0.3.0 root-crate release (summarized to the user earlier in the session, no
task filed yet).

## Repo state

```
?? docs/checkpoints/2026-08-09-crate-sweep-aligned-vmem.md
```

```
55e71b0 fix(perf): close 7 aligned-vmem hygiene residuals from the round-closing audit (task #719)
b8b70fb fix(perf): close two real data-race hazards in fault_injection's atomics (task #718)
94aef18 fix(perf): replace exposed-address as-cast round-trips with strict provenance in the two native over-reserve paths (task #717)
81ecfe3 fix(perf): close aligned-vmem huge-pages mock coverage gap + a miri-UB test assertion (task #716)
e5f6700 fix(perf): decide two publish-blocking API questions for mock (task #715, two MEDIUM findings)
```
