# Checkpoint — 2026-08-09 [crate-by-crate-rust-intel-remediation]

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
surfaced (tagged-index-stack's review caught a genuine HIGH defect in
already-"done" work — the untagged ABA loom counterfactual's oracle was itself
schedule-dependent; racy-ptr-cell's review found 1 HIGH CI-coverage gap + 12
lower-severity findings, all closed same-round). Task #711 (hoisting
`loom = "0.7"` into `[workspace.dependencies]`, closing a 3-manifest version-drift
hazard) is also closed.

**aligned-vmem is now 5/8 fix-tasks closed** (#699 HIGH, #712, #713, #714, #715),
**currently in progress on #716** (huge-pages has zero test coverage; a
`tests/lazy_commit.rs` assertion is an uninitialized-memory read under miri).
Remaining after #716: #717 (README provenance-guarantee doc claim), #718
(fault_injection's `Relaxed` ordering), #719 (7 hygiene residuals), then the
post-work chain (#744 checkpoint → #745 CHANGELOG → #746 commit-md → #747 `@oh`
review) before numa-shim's group (#697/#720-727) unblocks.

Every fix this round has been verified with a genuine counterfactual (temporarily
reverting the fix, confirming the associated test fails for the right reason)
before commit, per this project's zero-trust-review discipline. Several fixes
required BSD/Linux/hugetlb-specific reasoning this session's Windows host cannot
execute — each such commit states explicitly which parts are "REASONED-FROM-SPEC,
NOT empirically verified" (cross-compile-checked on real target triples:
x86_64-unknown-linux-gnu, x86_64-unknown-freebsd, x86_64-unknown-netbsd) versus
what was actually run and observed. Two real, previously-undiscovered bugs were
found and fixed as SIDE EFFECTS of this round's own zero-trust re-verification
(not the tasks' original scope): a vacuous test in task #713's own new suite that
broke under miri (fixed with `#[cfg_attr(miri, ignore)]`, documented why), and
aligned-vmem's total absence of `cargo miri test -p aligned-vmem` CI coverage
(filed as `docs/CORRECTNESS_OPEN_ITEMS.md` item 41, out of scope to fix here).

A `/babygoal` Stop-hook condition is armed (see below) requiring the full
crate-by-crate sweep to complete; a `# babysit tick` cron (job `a46e52be`,
every 15 min, off-minute) is also armed and has been driving forward progress
across ticks with no new user instruction needed between them.

**Immediately in flight when this checkpoint was written:** mid-way through
task #716's fix (2) — `crates/vmem/tests/lazy_commit.rs:287`'s
`assert_eq!(base.read(), 0, ...)` reads offset 0, which the test never writes;
under miri's `std::alloc`-based fallback (does NOT zero memory, unlike a real
OS's fresh pages) this is a genuine uninitialized read. I had just re-read the
established `#[cfg(not(any(miri, feature = "mock")))]` gate pattern from
`tests/smoke.rs:72` (the sibling case this exact defect mirrors) and was about
to apply the same gate to `lazy_commit.rs:287`. NO file edits had been made yet
for task #716 at checkpoint time — `git status` is clean. Task #716's other
half (fix 1: zero test coverage for the huge-pages API) is largely already
addressed by `tests/huge_pages.rs`, which task #714 created as a side effect
(the file's own header says task #716 owns "building out the rest of this
feature's test suite" — worth reviewing whether more coverage is still needed,
e.g. explicit `Call::ReserveHuge` mock-recording and `fail_next_reserve`
injection-through-the-huge-path tests per #716's own FIX list item (c)).

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
every crate in the sweep, not just sefer-region. The hook's own literal-text
matcher does not yet recognize this, but the user's own follow-up in this
session accepted the "continuing to aligned-vmem is correct" framing.)

## TaskList

### in_progress
- #656 sefer-region — verify/prepare for crates.io republish (perpetually
  blocked on a maintainer publish decision only the user can make; not part of
  the active sweep)
- #716 aligned-vmem: huge-pages has zero tests + a miri-UB assertion in
  tests/lazy_commit.rs (blockedBy: none — ready; mid-fix, see summary above)

### pending (next up, in strict blockedBy order)
- #717 aligned-vmem: README's "no as-usize round-trips" provenance guarantee
  contradicted by native base-pointer constructions
- #718 aligned-vmem: fault_injection's Relaxed payload-then-flag publish +
  non-atomic FAIL_NEXT decrement
- #719 aligned-vmem: 7 hygiene residuals (missing SAFETY comment, undocumented
  `let _ =` discards, blanket dead_code, Drop-reachable panic, untested Send,
  off_t shape, from_raw_parts inverse claim)
- #744-747 Post-work (aligned-vmem): checkpoint → CHANGELOG → commit-md → `@oh`
  review (blocked by #716-719 respectively/transitively) — #747 unblocks
  numa-shim's group
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
- #773,774 racy-ptr-cell round-closing-review follow-ups
- #700,706-710 racy-ptr-cell's own 6 fix tasks

## Decisions

- **Task #713's mock-feature-unification question (task #715) resolved
  independently**: chose the MINIMUM fix (doc warnings in Cargo.toml/lib.rs/
  README) over the audit's offered "stronger" fix (convert `mock` from a
  Cargo feature to a `--cfg vmem_mock` RUSTFLAGS flag, matching this repo's
  own `cfg(loom)`/`cfg(kani)` precedent) — reasoning: zero real external
  consumers exist today (first publish still pending), so the doc fix closes
  the realistic case (a careless workspace-internal dev-dependency) at
  near-zero cost, while the cfg-flag conversion is a large mechanical rewrite
  of this crate's whole test-invocation surface + CI matrix for a currently
  speculative benefit. Recorded explicitly that the SAME policy (doc-only,
  not cfg-flag) should apply to numa-shim's identical §C10 finding when that
  crate's round is reached, per the audit's own request for one consistent
  policy across both crates.
- **Task #714's hugetlb-leak fix**: chose "reject misaligned size/align
  up-front" (`VmemError::invalid_argument()`) over "round size/trim
  boundaries up and record the rounded reservation_len" — reasoning: the
  reject strategy is provably correct by construction (both size AND align
  huge-page-aligned ⇒ every subsequent munmap call is provably huge-page-
  aligned too, reasoned through explicitly in the code comment), whereas a
  rounding strategy would need runtime probing of the actual configured huge
  page size (no existing crate infrastructure for that) to be correct beyond
  the common-default-2-MiB case.
- **Task #714's BSD `_SC_PAGESIZE` values**: trusted the audit's own citation
  (47 for FreeBSD/DragonFly, 28 for NetBSD/OpenBSD) as matching independent
  recollection of each OS's `sys/unistd.h`; verified only by successful
  cross-compilation on 2 of the 4 BSDs (FreeBSD, NetBSD — the other two have
  no prebuilt rustup std component on this Windows host, but share the exact
  same cfg arm as their verified siblings).

## Open questions

None new since the last checkpoint (`docs/checkpoints/2026-08-09-0600.md`).
Carried over, still unresolved: the sefer-region 0.1.1 version-bump decision
(task #656's gate — genuinely requires the user), and
`docs/reviews/2026-08-05-release-readiness-gap-audit.md`'s NO-GO verdict for a
0.3.0 root-crate release (summarized to the user earlier in the session, no
task filed yet).

## Repo state

```
(clean — nothing uncommitted at checkpoint time)
```

```
e5f6700 fix(perf): decide two publish-blocking API questions for mock (task #715, two MEDIUM findings)
2e7f4f5 fix(perf): correct _SC_PAGESIZE on all four BSDs, reject misaligned hugetlb requests instead of leaking (task #714, two MEDIUM §F1 findings)
d6b72b1 docs: file aligned-vmem's missing miri CI coverage (task #713 zero-trust discovery)
131355a fix(perf): capture VmemError immediately, before cleanup FFI can clobber errno (task #713, MEDIUM)
54089fa fix(perf): stop clamping a contract VIOLATION to the SUCCESS sentinel in recommit/commit_range (task #712, MEDIUM, already crashed an in-repo consumer)
```
