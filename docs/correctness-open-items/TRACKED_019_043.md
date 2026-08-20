# Correctness / CI-debt open items — [T] Tracked tier (items 19-43)

**Part of the split index.** This file holds the full text of **[T]**
(tracked, not yet actioned) cards **19 through 43** (numbers 30-40 and
42 are not present — item 42 is `[A]`-tier and lives in `ACTIVE.md`;
30-40 were never assigned to a `[T]` card at all, per this index's
existing renumbering history — see item 86's card in
`TRACKED_044_093.md` for that history). Start at
`docs/CORRECTNESS_OPEN_ITEMS.md` for the purpose/scope/convention
header and the round-start reading order; come here for these
specific card bodies. See `docs/correctness-open-items/ACTIVE.md` for the
**[A]** tier, `docs/correctness-open-items/RESOLVED.md` for the closure
trail, and the sibling `TRACKED_005_008.md` / `TRACKED_009_018.md` /
`TRACKED_044_093.md` files for the rest of the **[T]** tier's number
ranges.

**Why split by number range, not by topic (task #1221, 2026-08-20):**
this file is one of four that together replace the single
`docs/correctness-open-items/TRACKED.md` (2,322 lines, task #1217),
which had itself grown past CLAUDE.md's R34-24 ~1,000-line threshold.
Every one of the 42+ code/CI/script citations of this index across the
repo cites an item by NUMBER (`` `docs/CORRECTNESS_OPEN_ITEMS.md` item
N ``), never by line or topic — so a number-range filename is a
one-hop lookup with no translation table required. Ranges were chosen
to balance by LINE COUNT, not by card count: this file is 13 cards /
~573 lines; see the sibling files for the other three ranges (4
cards/~638 lines; 10 cards/~518 lines; 50 cards/~577 lines). (Split
2026-08-20, task #1221.)

---

19. **[T, filed 2026-08-04, R34-2/task #521] MSRV caveat — the `msrv` CI job
    runs `cargo check --all-features`, never `cargo test`, so an
    MSRV-incompatible construct reachable only from a `#[cfg(test)]`-only or
    dev-dependency path would not be caught
    (`docs/reviews/2026-08-04-release-stabilization-audit.md` §5).** The audit
    calls this "acceptable, but worth stating in the release notes." Filed
    because no Round-34 task owns release-notes writing; the action is a
    one-line release-notes caveat ("MSRV is enforced as `cargo check`, not
    `cargo test` — a `#[cfg(test)]`-only or dev-dep construct incompatible
    with rustc 1.88 would not be caught by CI"), not a CI change.

    **Status: RESOLVED (2026-08-05, task #597/K2, commit `f43600d`).** The
    exact caveat this item requested landed verbatim in `CHANGELOG.md`'s
    new "Known limitations (as of this release)" subsection. Left in
    place rather than moved/renumbered — see item 16's identical note
    above; both fall under task M2/#623's broader numbering-cleanup scope.

    **Update (2026-08-06, task #612/K17):** the "better" option gap-audit
    R16 separately named (a bounded `cargo test --no-run` on MSRV) was also
    implemented — the `msrv` CI job now runs `cargo test --no-run
    --all-features` in addition to `cargo check --all-features`, verified
    feasible first (exit 0, ~6 minutes build-only; the full dev-dependency
    graph genuinely compiles under 1.88). This narrows, but does not fully
    close, the gap this item's caveat describes: build-only coverage now
    exists for every `#[cfg(test)]` path and dev-dependency, but the tests
    still aren't EXECUTED on 1.88 (only compiled) — a construct that
    compiles but panics/behaves differently only under 1.88 at runtime
    would still slip through. The release-notes caveat above remains
    accurate as stated and is not being reworded.

20. **[T, filed 2026-08-04, R34-2/task #521] F11 residual — Round 31's
    CHANGELOG section still carries the "Runtime improvements this round: 0"
    collision shape, and Rounds 31/32 are out of section order
    (`docs/reviews/2026-08-03-round33-readonly-review.md` G6 [P3]).** R33-7
    (task #512) closed F11 for Round 32 (split its runtime improvements into
    their own subsection with an accurate count), but Round 31's section at
    `CHANGELOG.md:36` still reads "**Runtime improvements this round: 0.**"
    two lines above a `#### Runtime improvements` heading whose bullets
    include R31-10's promoted trim API — the exact shape F11 described. Section
    ordering is also wrong: `grep -n "^### Round"` gives 33, 31, **32**, 30 —
    newest-first everywhere except 31/32 are swapped (pre-existing, but R33-7
    restructured both sections without fixing it). Both are one-commit
    structural fixes to `CHANGELOG.md`; filed here (reporting-honesty/process
    scope) so a future round inherits the residual rather than re-discovering
    it.

21. **[T, filed 2026-08-05, task #562, G1-bonus/`docs/reviews/2026-08-05-r34-review-remediation-readonly-review.md`] Two pre-existing Round-34 commits fail the repo's own `verify-commit-prefixes.mjs` R30-12 taxonomy lint — `43115cf` and `5c1142f` — and the Round-34 closing review's claim that the taxonomy was "correctly applied throughout" was inaccurate.**

    - **Status:** OPEN — not fixed. Fixing requires rewriting two commit
      messages that already have descendants (a rebase-scoped operation),
      which this task deliberately did not perform — see "Next trigger"
      below.
    - **Current-number-or-verdict:** confirmed FAILURE (not a warning) for
      both SHAs, independently re-run over the full Round-34 span
      (`40241b0..c5db553`, the R34 base boundary through the R34 closing
      commit, deliberately excluding the review-remediation-wave commits
      that came after `c5db553`):

      ```
      [verify-commit-prefixes] range: 40241b0..c5db553  (43 commit(s) total)
      [verify-commit-prefixes] linted 43 commit(s)

      [verify-commit-prefixes] 8 WARNING(s) (direction 2 — hidden runtime change?):
        ... (8 warnings, all pre-existing bench:/docs:-prefixed commits touching
        Cargo.toml/.gitignore/package.json or a bench-internals-gated diagnostic
        accessor in src/ — not this item's subject)

      [verify-commit-prefixes] 2 FAILURE(s) (direction 1 — R30-12 taxonomy violation):
        - 43115cf "fix(perf): correct R34-11 CSV's base_commit off-by-one (parent -> landing SHA)" — prefix claims a shipping/opt-in code fix in perf-sensitive code, but every changed path is under docs/examples/benches/tests/scripts/ (1 path(s): docs/perf/R34_11_CATCHUP_DECAY_GATE_summary.csv); use bench: or docs(config): instead if no shipping/opt-in code actually changed.
        - 5c1142f "fix(perf): correct R34-10 CSV's base_commit off-by-one (parent -> landing SHA)" — prefix claims a shipping/opt-in code fix in perf-sensitive code, but every changed path is under docs/examples/benches/tests/scripts/ (1 path(s): docs/perf/R34_10_SPARSE_DECAY_GATE_summary.csv); use bench: or docs(config): instead if no shipping/opt-in code actually changed.

      [verify-commit-prefixes] FAILED — see CLAUDE.md's R30-12 rule ("Active rules" section) for the full five-prefix taxonomy (perf(runtime) / perf(opt-in) / bench / docs(config) / fix(perf)).
      ```

      Independently re-confirmed via `git show <sha> --stat` for both, not
      taken on the script's word alone: `43115cf` changes exactly
      `docs/perf/R34_11_CATCHUP_DECAY_GATE_summary.csv` (1 file, 1
      insertion, 1 deletion — a single metadata column,
      `base_commit`); `5c1142f` changes exactly
      `docs/perf/R34_10_SPARSE_DECAY_GATE_summary.csv` (1 file, 24 changed
      lines — the same `base_commit` column across all 24 data rows). Both
      commit bodies confirm in their own words that only the provenance
      column changed and "every peak_gap/segment/RSS/ops-late number in the
      committed CSV was already correct." Neither touches any path under
      `src/`, `crates/`, or any shipping/opt-in feature-gated code — the
      correct prefix per CLAUDE.md's R30-12 five-slot taxonomy for each is
      **`docs(config):`** (an existing report/config artifact corrected, no
      code changed at all — not `bench:`, since no judge/harness/probe code
      itself changed either, only a derived CSV's metadata field) or,
      failing that, `fix(perf)` would only be correct if the taxonomy's own
      wording ("shipping or opt-in code changed... but NO speedup is
      measured or claimed") were met, which it is not here: no code at all
      changed in either commit.
    - **What was inaccurate:** `docs/reviews/2026-08-05-round34-readonly-review.md`
      §7 stated "Commit-prefix taxonomy (R30-12): correctly applied
      throughout" for Round 34. That statement is contradicted by the repo's
      own lint for these two commits, which predate that review (`43115cf`
      and `5c1142f` both land inside the `40241b0..c5db553` Round-34 span
      the review itself was scoped to). This was surfaced as a "bonus
      finding" by a LATER independent review
      (`docs/reviews/2026-08-05-r34-review-remediation-readonly-review.md`,
      finding G1, its closing §2 paragraph beginning "Additionally — a
      finding the prior review missed") while auditing the unrelated
      review-remediation wave that followed Round 34 — that wave's own
      `73817ee` (task #548; reworded by the later G1 rebase, task #555, to
      its current SHA `5e75032` — cited here by its ORIGINAL SHA since this
      paragraph describes the state at time of writing, before that rebase
      ran) independently introduced a THIRD `fix(perf):` taxonomy failure of
      the identical shape (CSV-only doc-report edit), which is tracked
      separately (see task #555's disposition, now completed — it reworded
      `73817ee`/`a4dc38e`/`d46c349` but did NOT extend back to `43115cf`/
      `5c1142f`, so this item's own "Next trigger" below remains open; not
      duplicated here since it postdates the `40241b0..c5db553` Round-34
      span this item is scoped to).
    - **Why not fixed here:** rewriting `43115cf` or `5c1142f`'s commit
      message requires a rebase that touches history deeper than, and with
      more descendant commits on top of, the review-remediation wave's own
      already-risky `73817ee` rebase scope (task #555; at time of writing
      that rebase was itself still deliberately deferred as a rebase-free
      decision — it has since run, task #555 is now completed, and its
      3-commit scope did NOT extend back to `43115cf`/`5c1142f`, so the
      analysis below is unchanged). Per this task's explicit scope, the fix
      here is documentation-only: record the finding accurately so it is
      not lost, not perform the rebase.
    - **Next trigger:** reopen and actually rewrite both commit messages
      (to `docs(config):`) when a rebase touching this era of history
      happens for another reason (task #555/G1's rebase already ran —
      commit `73817ee` reworded to `5e75032` — but did not reach back this
      far; this item stays open until a FUTURE rebase covers `43115cf`/
      `5c1142f` too), or when explicitly requested by the maintainer. Until then this
      card is the durable record that `npm run check`'s
      `verify-commit-prefixes` step is red on these two SHAs whenever a
      range including them is linted (e.g. the default `@{u}..HEAD` range,
      once these commits are within it), and that the Round-34 closing
      review's taxonomy claim needs this correction appended wherever it is
      read.
    - **Evidence:** `node scripts/verify-commit-prefixes.mjs 40241b0..c5db553`
      (quoted verbatim above, run 2026-08-05); `git show 43115cf --stat`;
      `git show 5c1142f --stat`; `docs/reviews/2026-08-05-r34-review-remediation-readonly-review.md`
      §2 (G1) and §10 point 1; `docs/reviews/2026-08-05-round34-readonly-review.md`
      §7.
    - **Update (2026-08-05, task #601/K6, release-readiness map): the "Next
      trigger" premise above has changed — do NOT rebase.** A push later
      the same day (20:50:33Z) moved `origin/main` to include both
      `43115cf` and `5c1142f` as ancestors (verified: `git merge-base
      --is-ancestor <sha> origin/main` succeeds for both). The default lint
      range (`@{u}..HEAD`) no longer contains either commit, so `node
      scripts/verify-commit-prefixes.mjs` with no explicit range now
      **PASSES** — independently re-run and confirmed. CI's own
      `commit-prefix-lint` job is PR-scoped only
      (`.github/workflows/ci.yml`, `if: github.event_name ==
      'pull_request'`), so it was never blocking a direct push either. The
      practical consequence this card previously documented ("red whenever
      a range including them is linted, e.g. the default range") no longer
      holds day-to-day. This does NOT close the underlying taxonomy defect
      (both commits still literally have the wrong prefix) — it changes
      the cost/benefit of the "Next trigger" fix: rewriting `43115cf`/
      `5c1142f` now means rewriting PUBLISHED history on `origin/main`, not
      unpushed local commits. Recommendation revised: leave both commits
      as accepted historical debt (this card is the durable record) rather
      than rebase published history for a cosmetic prefix issue; only
      revisit if a future rebase touching this era of history happens for
      an unrelated, independently-justified reason.

22. **[T, filed 2026-08-05, task #575/H5, `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H5] `RemoteFreeRing::DrainHeadPublish`'s panic-safety guard is unwind-safe for already-fully-processed elements but NOT exactly-once for the element in flight when a panic occurs — a documented residual (Sol-F5, task #567) never cross-filed into this index.**

    - **Status:** OPEN, residual — not a proven bug, no known reachable
      trigger, filed for tracking per this index's own convention (a
      doc-comment naming a follow-up must also be cross-filed here so a
      future round inherits it without re-deriving from the source).
    - **Current-number-or-verdict:** by inspection, the current production
      `reclaim` closures (`AllocCore::reclaim_offset` /
      `AllocCore::reclaim_offset_checked`,
      `src/alloc_core/alloc_core_small_reclaim.rs`) do not panic after
      mutating state on their current code paths — no `unwrap`/`expect`/
      `panic!`/unchecked indexing on the mutation-bearing paths. This is an
      observation about the code AS WRITTEN, not a structural guarantee: the
      type system does not prevent a future `reclaim` closure from
      panicking after a mutation. `RemoteFreeRing::drain`'s loop body calls
      `reclaim(off)` BEFORE clearing the slot and BEFORE
      advancing/publishing `h` — so a reclaim that mutates state and then
      panics leaves the slot non-empty and `h` one short; a
      `catch_unwind`-resuming caller would re-pass that same `off` to
      `reclaim`, i.e. `reclaim` could run twice for the in-flight element.
    - **Why not currently exploitable:** any unwind that escapes through the
      `GlobalAlloc` entry points still aborts the process
      (`src/global/sefer_alloc.rs`'s panic-tripwire docs), so this replay
      window is reachable only through a direct/internal `catch_unwind`
      around `drain` — not through ordinary allocator usage.
    - **What would close it structurally:** a two-phase/idempotent reclaim
      protocol (clear-then-reclaim, or a reclaim that can be safely retried
      against an already-cleared slot), or an explicit poison/skip policy
      for the in-flight element on unwind — out of scope for the
      `DrainHeadPublish` guard itself, which only ever publishes `h` values
      fully advanced past a cleared slot.
    - **Next trigger:** reopen and design the two-phase protocol if a future
      `reclaim` closure gains fallible/panicking code on a mutation-bearing
      path, or if a direct/internal `catch_unwind` caller around `drain` is
      ever added to production code (currently none exists).
    - **Evidence:** `src/alloc_core/remote_free_ring.rs`'s
      `DrainHeadPublish` doc comment (the "Exact contract (Sol-F5, task
      #567 ...)" section, ~lines 861-900);
      `docs/reviews/2026-08-05-sol-release-readonly-review.md` finding F5;
      `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding
      H5.

23. **[T, filed 2026-08-05, task #575/H5, `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H5] `InitStateGuard`'s unwind rollback does not distinguish a pre-write unwind (nothing to clean up) from a post-write unwind (a live `HeapCore` already sits in `FALLBACK`) — a documented residual (Sol-F6, task #568) never cross-filed into this index.**

    - **Status:** OPEN, residual — not a proven bug, no currently-reachable
      trigger, filed for tracking per this index's own convention.
    - **Current-number-or-verdict:** the guard's `Drop` unconditionally
      rolls `INIT_STATE` back to `UNINIT` on an armed unwind, regardless of
      whether the unwind happened before or after the in-place `write(hc)`.
      A post-write unwind lets the next CAS winner `write` a fresh
      `HeapCore` on top of the old one WITHOUT running the old value's
      `Drop` (`AllocCore::Drop`, `src/alloc_core/alloc_core.rs`, releases
      the heap's segment reservations) — so skipping it leaks them. The
      guard therefore guarantees "no permanent `INITIALIZING` livelock", NOT
      "`Drop` always runs for an already-written `HeapCore`".
    - **Why not currently exploitable:** as of this writing, the only unwind
      source in the guarded region between `write(hc)` and the `READY`
      publish is the `internals`-gated test-injection panic, deliberately
      placed BEFORE `HeapCore::new`; `bind_thread_free`
      (`src/registry/heap_core_ownership.rs`) is a plain field assignment
      and cannot panic. So the post-write window is not currently reachable
      by any known panic source in the initialization path — but it is NOT
      structurally closed: a future change adding fallible code between
      `write(hc)` and the `READY` store would silently reopen it.
    - **What would close it structurally:** making the guard aware of
      whether `HeapCore` was written, so an armed unwind after that point
      drops the stale value or poisons the slot instead of just rolling
      back to `UNINIT`.
    - **Next trigger:** reopen and implement the write-aware guard if a
      future change adds fallible/panicking code between `write(hc)` and
      the `READY` store in the guarded region (currently none exists).
    - **Evidence:** `src/global/fallback.rs`'s `InitStateGuard` doc comment
      (the "What this guard does NOT guarantee (Sol-F6, task #568)" section,
      ~lines 375-399); `docs/reviews/2026-08-05-sol-release-readonly-review.md`
      finding F6; `docs/reviews/2026-08-05-sol-remediation-readonly-review.md`
      finding H5.

24. **[T, filed 2026-08-06, task #627/S4, `docs/reviews/2026-08-06-sprint-closing-readonly-review.md` finding S4] `README.md:515` claims all 11 workspace members are "a real crates.io crate someone can `cargo add` on its own" — at least 3 are not published.**

    - **Status:** OPEN — not fixed. Documentation-only issue; no code
      change needed, only a README correction or a publish action.
    - **Current-number-or-verdict:** confirmed via crates.io API
      (`docs/plans/2026-08-05-release-execution-map.md`'s own
      [П]-verified table, independently re-confirmed by
      `docs/reviews/2026-08-05-fh-release-readiness-verification-review.md`):
      `racy-ptr-cell`, `size-classes`, and `tagged-index-stack` are NOT
      published on crates.io. As of commit `2a75d91` (task #648/P14),
      `.github/workflows/release.yml` gained tag patterns and
      `workflow_dispatch` dropdown options for all three, so the file now
      lists 8 crates: `aligned-vmem`, `sefer-region`, `malloc-bench-rs`,
      `numa-shim`, `racy-ptr-cell`, `size-classes`, `tagged-index-stack`,
      `sefer-alloc` — release-workflow plumbing exists for every crate this
      item originally flagged as missing it. This does NOT close the item:
      none of the three has actually been published to crates.io yet (the
      headline claim below is unchanged), and 3 more workspace members
      (`globalalloc-model`, `proc-memstat`, `proc-probe` — never checked
      against crates.io) still have no release-workflow entry at all.
      `README.md`'s crate-count table now lists 10, not 11, workspace
      members (the eleventh, `ring-mpsc`, was removed from the workspace
      entirely — user-requested, 2026-08-06, task #655: it had zero
      production consumers and its one filed use case, the in-tree
      `RemoteFreeRing`/`HeapOverflow` swap, was independently found NO-GO —
      see `docs/crate_extraction/CRATE_P4_FOLLOWUP_NOGO.md`. This shrinks,
      but does not close, this item's scope: one fewer unpublished crate to
      track, from 4 down to 3). `README.md`'s badge table now shows badges
      only for the remaining 10.
    - **Why filed instead of fixed here:** the fix depends on the same
      publish-DAG decision already deferred this sprint by explicit user
      instruction (tasks K3/K4/K9/L2/L3/L5, "path dependencies stay local
      for now, publish before release") — publishing the missing crates
      resolves it one way, rewriting the README claim resolves it the
      other way, and which one happens is a release-planning decision, not
      a code fix to make preemptively.
    - **Why this needed its own item:** the finding was already recorded in
      `docs/plans/2026-08-05-release-execution-map.md` (§"Не мои решения")
      and `docs/checkpoints/2026-08-06-0015.md`, but NEITHER file is
      consulted by CLAUDE.md's own "Round start: check BOTH open-items
      indexes" convention — only `docs/CORRECTNESS_OPEN_ITEMS.md` and
      `docs/perf/OPEN_ITEMS.md` are. Without this entry the finding would
      have been invisible to a future round despite being fully
      documented elsewhere — exactly the failure mode that convention
      exists to prevent (R18-8/R22-3 precedent, cited in this file's own
      "Round start" rule).
    - **Next trigger:** resolve as part of the deferred publish-DAG pass
      (K3/#598) — either publish the missing crates before 0.3.0 ships, or
      rewrite `README.md:515` to something like "ten
      independently-publishable building blocks; N of them are published
      on crates.io today" and remove badges for unpublished ones.
    - **Evidence:** `README.md:515`, `:545-555`;
      `docs/plans/2026-08-05-release-execution-map.md` §"Ход B" table and
      §"Не мои решения" item 4;
      `docs/reviews/2026-08-06-sprint-closing-readonly-review.md` finding S4.
    - **[Added 2026-08-20, task #1207] A SECOND, HARDER blocker for the
      same three crates (plus `numa-shim`), found while investigating the
      release guard: none of the four has a `CHANGELOG.md` AT ALL.**
      Measured directly — `test -f crates/<c>/CHANGELOG.md` returns MISSING
      for `racy-ptr-cell`, `size-classes`, `tagged-index-stack` and
      `numa-shim`; only `aligned-vmem` has one. This is not cosmetic:
      `.github/workflows/release.yml:268-272` resolves the CHANGELOG path
      from `cargo metadata` by package name and then fails CLOSED
      (`::error::CHANGELOG not found at $CHANGELOG` / `exit 1`) — a
      fail-closed branch task #1115 added deliberately. So a publish
      attempt for any of the four stops at that step, before `cargo
      publish` runs. The guard is behaving correctly; the files simply do
      not exist. Owner: **task #1220** (write real per-crate changelogs
      reconstructed from `git log -- crates/<c>/`, not stubs).
    - **[Added 2026-08-20, task #1207] Release-guard status, recorded
      because the task that raised it was based on MY OWN too-narrow
      verification and resolved NULL.** The claim under investigation was
      that `## 0.2.0 - Unreleased` would ship in the tarball with no gate
      catching it. The supporting evidence was `grep -rn "Unreleased"
      scripts/*.mjs .github/workflows/ci.yml` → zero hits, which is TRUE
      but was generalised into "no guard anywhere looks at that word".
      The guard exists: `.github/workflows/release.yml:301-309`, and its
      own comment names `## 0.2.0 - Unreleased` (aligned-vmem) as one of
      the two heading shapes it is written to catch, case-insensitively,
      after an anchored per-version section match that requires exactly one
      section. Counterfactual run against the guard body extracted
      verbatim: with today's header it exits 1 ("Stamp the real release
      date"); with a dated header it exits 0; a CHANGELOG carrying a
      legitimate `## Unreleased` section for FUTURE work alongside a dated
      released section still passes, so it is not a false-positive
      generator. **Recorded decision:** the version header is dated
      immediately before `cargo publish`, not now — dating it today would
      fabricate a release date, which is the exact defect task #1099/I2
      (`5dc4385`) already had to remove once. That decision is held by the
      guard, not by intention. **Honest limitation:** the guard lives only
      in `release.yml` under `dry_run != 'true'`, so neither `npm run
      check` nor `ci.yml` ever exercises it — a contributor sees it fire
      for the first time during a real release. That is not the task-#1131
      "guard nothing runs" class (it does run, at exactly the moment the
      decision above refers to), but it is stated here rather than left to
      read as full CI coverage.

25. **[T, filed 2026-08-06, task #653/P19, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P3-4 item 1] `TaggedIndex<INDEX_BITS>` rejecting `INDEX_BITS > 32` at compile time (F1, task #638) has no automated compile-fail test — CI coverage gap, honestly recorded but unfiled until now.**

    - **Status:** OPEN — not fixed. CI-coverage gap only; the underlying
      compile-time guard itself (`_CHECK_BITS`) is already correct and
      shipped (task #638, commit `d78625b`).
    - **Current-number-or-verdict:** `crates/tagged-index-stack/src/lib.rs`
      (the `_CHECK_BITS` const, ~lines 179-195) enforces `INDEX_BITS in
      1..=32` via a `const` `assert!`, so `TaggedIndex::<33>` (or any width
      above 32) fails `cargo build`. No `trybuild`-style (or equivalent)
      automated test pins this failure — it was manually verified once and
      the gap explicitly recorded as a code comment:
      `crates/tagged-index-stack/tests/stack_unit.rs` (~lines 137-144) says
      "this crate has no trybuild (or similar compile-fail) test
      infrastructure wired up, so `INDEX_BITS > 32` failing to compile is
      NOT pinned by an automated test. ... This is a known, honestly-recorded
      coverage gap, not a silent omission."
    - **Why filed instead of fixed here:** adding `trybuild` (or an
      equivalent compile-fail harness) is new test infrastructure for one
      crate, not a bookkeeping fix — out of scope for a bookkeeping-only
      task; a real coverage-closing task should own it.
    - **Next trigger:** add a `trybuild`-style compile-fail test asserting
      `TaggedIndex::<33>` (or `TaggedIndexStack<33, _>`, whichever the
      crate's public generic surface exposes) fails to compile with the
      `_CHECK_BITS` assertion message, OR document an explicit accepted-risk
      rationale if compile-fail infra is judged not worth adding for a
      single-crate, single-assertion case.
    - **Evidence:** `crates/tagged-index-stack/src/lib.rs` ~lines 179-195
      (`_CHECK_BITS`); `crates/tagged-index-stack/tests/stack_unit.rs`
      ~lines 137-144 (the recorded-gap comment, from task #638, commit
      `d78625b`); `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md`
      finding P3-4 item 1.

26. **[T, filed 2026-08-06, task #653/P19, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P3-4 item 2] The numa-shim macOS+miri `mod platform` duplicate-definition fix (dc003c9) is structurally sound but empirically unconfirmed until the new `numa-shim-macos-miri` CI job actually runs on real macOS.**

    - **Status:** OPEN — pending empirical confirmation. The fix itself
      (adding `not(miri)` to the macOS platform-stub `cfg`, matching the
      three sibling platform blocks) is landed and reasoned-through-correct.
    - **Current-number-or-verdict:** commit `dc003c957b40baacaa147ff35e81884e27b0b1b4`'s
      own body states its local verification was done on Windows (no macOS
      box available) and explicitly does NOT exercise the macOS
      `not(miri)` arm or the macOS+miri crossing itself — "that
      verification depends on the new `numa-shim-macos-miri` CI job
      actually running on `macos-latest`." The closing review
      (`docs/reviews/2026-08-06-numa-shim-publish-readiness-review.md` /
      the sweep-closing review's own re-check) independently verified the
      fix is structurally correct via cfg-disjointness analysis (the macOS
      stub and the `cfg(miri)` any-OS stub can no longer both satisfy their
      `cfg` simultaneously), but static analysis is not the same as a real
      `cargo miri test` run on `macos-latest` actually going green.
    - **Why filed instead of fixed here:** there is nothing to "fix" — this
      is a pending-confirmation trigger, not a defect. It only needed
      filing so a future round doesn't have to re-derive from the commit
      body that confirmation is still outstanding.
    - **Next trigger:** confirm the `numa-shim-macos-miri` job
      (`.github/workflows/ci.yml`) runs green on its first real GitHub
      Actions execution (it is a per-PR job, so this should happen on the
      next PR/push that touches a path triggering it, or can be confirmed
      via `workflow_dispatch`/inspecting the Actions run history directly).
    - **Evidence:** commit `dc003c957b40baacaa147ff35e81884e27b0b1b4`'s
      full commit body (verification section); `.github/workflows/ci.yml`
      `numa-shim-macos-miri` job; `crates/numa-shim/src/lib.rs` (the `not(miri)`
      guard on the macOS platform-stub `cfg`, ~line 763);
      `docs/reviews/2026-08-06-numa-shim-publish-readiness-review.md`;
      `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md`
      finding P3-4 item 2.

27. **[T, filed 2026-08-06, task #653/P19, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P3-4 item 3] tagged-index-stack's `compile_error!` guard for unsupported `target_has_atomic` widths doesn't suppress the cascading `E0432` unresolved-import error on the same build — a deliberate, unrecorded tradeoff.**

    - **Status:** OPEN — low-priority polish, deliberately deferred, not an
      oversight. Purely cosmetic (the build already fails either way on an
      unsupported target; the only difference is whether the FIRST error a
      user sees is the clear named-reason `compile_error!` or that error
      followed by a cascade of confusing `E0432`s).
    - **Current-number-or-verdict:** commit
      `300b41f97a0e7c85310e5ed53dcbf289414e779f`'s own body: adding the
      `#[cfg(not(target_has_atomic = "64"))] compile_error!` guard (F2) does
      fire first and gives a clear, named-reason error on an unsupported
      target — a real (if small) behavior improvement. But it does not
      suppress the subsequent cascading `E0432` unresolved-import error
      that still follows on the same build, because `compile_error!` does
      not halt the rest of module compilation. Fully suppressing the
      cascade would require `#[cfg(target_has_atomic = "64")]`-gating every
      downstream item in the file — "judged too intrusive for the benefit
      on an already-broken build," per the commit body.
    - **Why filed instead of fixed here:** it is a conscious, defensible,
      already-reasoned-through tradeoff, not a bug — filing it only so the
      decision is recorded somewhere indexed instead of living solely in
      one commit message.
    - **Next trigger:** none required; revisit only if a future contributor
      finds the cascading `E0432` output genuinely confusing enough in
      practice to justify the `cfg`-gating cost across the file. Low
      priority, no forcing deadline.
    - **Evidence:** commit `300b41f97a0e7c85310e5ed53dcbf289414e779f`'s
      full commit body; `crates/tagged-index-stack/src/lib.rs` (the
      `compile_error!` guard);
      `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md`
      finding P3-4 item 3.

28. **[T, filed 2026-08-06, task #653/P19, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P3-4 item 4] Two one-way-door publish decisions for `racy-ptr-cell` — its name and its 383-character `description` — were surfaced in commit `9ecada3`'s body but never recorded anywhere indexed, and become permanent the moment the crate first publishes.**

    - **Status:** OPEN — needs a maintainer decision before `racy-ptr-cell`'s
      first publish to crates.io. No code change; a naming/metadata call.
    - **Current-number-or-verdict:** commit
      `9ecada3d25bcbdf33e9b184c4233685e5b6a243f`'s body, §"Not addressed
      here": (a) the crate name `racy-ptr-cell` reads to a newcomer as "has
      data races" — the OPPOSITE of the guarantee the crate actually
      provides (a lock-free, race-safe exactly-once cell) — and was
      confirmed free on crates.io as of the original review date (subject
      to re-confirmation closer to actual publish time, since crates.io
      names can be claimed by others in the interim); (b) `Cargo.toml`'s
      `description` field is 383 characters, long for a crates.io listing
      (crates.io does not hard-limit description length, but long
      descriptions truncate awkwardly in search-result UI). Neither is
      recorded anywhere indexed prior to this filing, nor in K3/#598's own
      task description.
    - **Why filed instead of fixed here:** both are one-way-door naming/
      metadata decisions requiring maintainer judgment (a rename affects
      every existing reference across the workspace and any external
      consumer once published; a description rewrite is a content call) —
      not something to resolve unilaterally in a bookkeeping-only task.
    - **Next trigger:** resolve as part of the deferred publish-DAG pass
      (K3/#598), before `racy-ptr-cell`'s first `cargo publish` — decide
      whether to rename the crate (and if so, to what) and whether to
      shorten `description`, then re-verify crates.io name availability
      immediately before the actual publish action (names can be claimed
      by others between now and then).
    - **Evidence:** commit `9ecada3d25bcbdf33e9b184c4233685e5b6a243f`'s full
      commit body, §"Not addressed here"; `crates/racy-ptr-cell/Cargo.toml`
      (`name`, `description` fields);
      `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md`
      finding P3-4 item 4.

29. **[T, filed 2026-08-06, task #654/P20, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P4-12] `#![deny(missing_docs)]` on 4 about-to-be-published crates is a one-way-door tradeoff versus the more common `warn` + CI `-D warnings` convention — no conscious publish-time decision recorded.**

    - **Status:** OPEN — low severity, pre-publish-decision item. No code
      defect: all four crates compile clean today at 100% doc coverage.
    - **Current-number-or-verdict:** `#![deny(missing_docs)]` was added to
      `racy-ptr-cell` (commit `9ecada3`, task #642) and to `sefer-region`,
      `size-classes`, `tagged-index-stack` (commit `7c8621f`, task #651).
      All four verified at 100% doc coverage as of the commits above, and
      all four compile clean today. The tradeoff: `deny` (vs. the more
      common ecosystem convention of a lib-level `warn` plus CI-level `-D
      warnings`) means that once a crate is PUBLISHED, a future rustc
      release that widens what counts as `missing_docs` would turn a
      downstream consumer's `cargo build` of that already-published,
      pinned version red — with no recourse for the consumer, since they
      cannot edit a crate they don't own. `warn` would not have this
      failure mode (a widened lint would show as a new warning on
      recompilation of the crate's own source, not retroactively break an
      already-published, unmodified version's build for downstream
      consumers). None of these four crates has been published yet, so
      the tradeoff is still avoidable.
    - **Why filed instead of fixed here:** this is a deliberate policy
      choice between two defensible conventions (`deny` vs. `warn` + CI
      gate), not a bug — it needs a conscious maintainer decision before
      first/next publish, not a unilateral edit in a bookkeeping-only task.
    - **Next trigger:** before any of `racy-ptr-cell` / `sefer-region` /
      `size-classes` / `tagged-index-stack`'s first (or next) `cargo
      publish`, decide whether to keep `#![deny(missing_docs)]` as-is
      (accepting the one-way-door risk) or downgrade to `#![warn(missing_docs)]`
      plus an equivalent CI-level `-D warnings` gate (matching the more
      common ecosystem convention, avoiding the retroactive-break failure
      mode). Natural to fold into the deferred publish-DAG pass (K3/#598).
    - **Evidence:** commit `9ecada3d25bcbdf33e9b184c4233685e5b6a243f`
      (`racy-ptr-cell`); commit `7c8621f` (`sefer-region`, `size-classes`,
      `tagged-index-stack`); `crates/racy-ptr-cell/src/lib.rs`,
      `crates/sefer-region/src/lib.rs` (the `sefer-region` package),
      `crates/size-classes/src/lib.rs`,
      `crates/tagged-index-stack/src/lib.rs` (each crate's
      `#![deny(missing_docs)]` attribute);
      `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md`
      finding P4-12.

41. **CLOSED** by task #1057 (dedicated per-PR `aligned-vmem-miri` CI job added). See "Recently resolved" in RESOLVED.md for the full closure narrative.

43. **Deferred verification — `aligned-vmem`'s per-OS `_SC_PAGESIZE`
    constant table (task #714) is REASONED-FROM-SPEC for 4 of 6 affected
    targets, never empirically executed.** (Filed 2026-08-09, task
    #776/F13, round-closing review of the aligned-vmem round.)

    - **Status:** PARTIALLY OPEN — macOS half CLOSED (see "Recently
      resolved" #43 for the closure narrative and CI citation). The other
      3 targets (FreeBSD, NetBSD/OpenBSD, DragonFly) remain OPEN — no
      action needed unless a runner becomes available; filed so the gap
      is visible rather than silently load-bearing on an unverified
      constant.
    - **Current-number-or-verdict:** macOS family = 29, empirically
      confirmed correct (see "Recently resolved" #43). FreeBSD/DragonFly =
      47 and NetBSD/OpenBSD = 28 remain NOT independently executed on real
      hardware — reasoned from each OS's own `sys/unistd.h` header value,
      cross-compile-checked via `cargo check --target
      x86_64-unknown-{freebsd,netbsd}` only, which confirms the code
      COMPILES but not that the numeric constant is correct;
      `x86_64-unknown-dragonfly`/`x86_64-unknown-openbsd` have no prebuilt
      rustup std component on this session's Windows host, so those two are
      not even cross-compile-checked, only reasoned by sharing the
      identical cfg arm as their verified-by-citation siblings. A wrong
      `_SC_PAGESIZE` value would cause `page_size()` to query the WRONG
      name via `sysconf`, silently returning garbage (or an unrelated
      system parameter's value) on any of these targets if the
      header-citation reasoning is wrong — note also that `page_size()`'s
      own silent fallback to `PAGE` (4 KiB) on an implausible value means
      the crate's OWN generic test (`page_size_is_a_valid_os_page`, `is
      power of two && >= PAGE`) would NOT have caught a wrong macOS
      constant either, since the fallback value passes that check too;
      this is exactly why the (now-resolved) macOS test asserted the
      exact 16 KiB value rather than the generic invariant. A second,
      distinct consequence of this same unverified-constant gap was found
      and fixed in round 8 (task #897, finding U1):
      `try_reserve_aligned_exact` (`crates/aligned-vmem/src/os/unix.rs`, post-split home, task #1082) used to skip
      its own alignment-of-the-returned-base check whenever
      `align <= page_size()`, reasoning that `mmap` always returns
      page-aligned addresses in that range — true only if `page_size()`
      is itself <= the real OS page size. A wrong `_SC_PAGESIZE` constant
      returning a power-of-two ABOVE the real page size on one of the
      still-open BSD targets would have made that skip silently return a
      base NOT aligned to the requested `align`, violating
      `Reservation::as_ptr()`'s documented alignment guarantee with no
      error and no diagnostic. Fixed by making the check unconditional
      (the `align > page_size()` conjunct measured zero syscalls saved —
      see the fix's own comment in `crates/aligned-vmem/src/os/unix.rs` (moved there by task #1055's split) — so removing it is free).
    - **Evidence:** `crates/aligned-vmem/src/os/unix.rs`'s `_SC_PAGESIZE` constant
      definition and its own doc comment cite the per-OS header values
      directly; no BSD runner exists in `.github/workflows/ci.yml`'s
      current matrix for this crate.
    - **Next trigger:** BSD half only — if/when a FreeBSD, NetBSD,
      DragonFly, or OpenBSD CI runner becomes available for this crate (or
      this repo gains one for any purpose), run `page_size()` on it and
      assert the returned value matches the platform's actual page size
      (typically 4 KiB on all four, making a silent wrong-constant bug
      hard to notice without an explicit assertion against the OS's own
      reported value via a DIFFERENT API, e.g. comparing against
      `/proc/self/status` or equivalent, not just checking the result is a
      power of two).
