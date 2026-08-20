# Correctness / CI-debt open items -- [T] Tracked tier -- crates.io publish-readiness: metadata, naming, dependencies, NO-GO audits

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_correctness_residuals.md`, `TRACKED_process_record.md`, `TRACKED_misc.md`) for the rest of
the tier.

**Criterion for this file:** A card belongs here if it is about a decision or blocker that gates a crate's crates.io publication -- naming/description/license/dependency one-way-door decisions, semver-coupling decisions, or a NO-GO verdict (and its blocking findings) from an independent pre-publication audit.

**Card count:** 9.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every one of the 42+ code/CI/script citations of this index across the
repo cites an item by NUMBER, never by topic or file, so
`docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index) now carries a complete
item-N to file lookup table covering all 70 numbers (including the
`59a`/`59b` sub-items) -- that table, not this file's name, is what keeps
the by-number citation convention working under a thematic split: the
lookup is two-hop (index table, then this file), but mechanical and
always correct.
(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
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

46. **CLOSED** by task #1053 (option (a): coupling accepted and documented,
    plus a `pub use aligned_vmem::Reservation;` re-export). See "Recently
    resolved" in RESOLVED.md for the full closure narrative.

85. **[T, filed 2026-08-11, task #821, renumbered from 46 by task #1143 (2026-08-19) — a duplicate item number, not a new item] `crates/sefer-region` depends on `captrack`
    v0.1.1 (exact-pinned) as a dev-dependency for a single ignored probe test.**
    **Renumbering note (task #1143):** this card was originally filed as item 46 by commit `7ee57a9` (2026-08-11), which appended it at the file's then-current end without checking that 46 was already taken — the numa-shim `reserve_on_node` item (filed 2026-08-09, task #778, two days earlier) had legitimately owned number 46 since before this card existed (confirmed via `git show 7ee57a9~1:docs/CORRECTNESS_OPEN_ITEMS.md`, which shows item 46 = numa-shim at that parent commit). The duplicate persisted through every later edit, including the R34-24 archive split, because the pairing test (`tests/no_stale_doc_references.rs::correctness_index_recently_resolved_pointers_carry_verdicts`) only checks the "Recently resolved" section's pointers, not active-tier item numbers, so nothing caught it. Renumbered to 85 (the next number above the highest then in use, 84) rather than shifting every intervening item, to avoid the exact cascading-renumber risk task #1109's index split already demonstrated (9 pointers truncated, 19 of 32 verdicts lost) when a purely mechanical edit touched more of the file than intended. No other document in this repo cites this card as "item 46" — every other reference to "item 46" in `docs/`, including all checkpoints, means the DIFFERENT numa-shim item in this same file, or `docs/perf/OPEN_ITEMS.md`'s own unrelated former item 46 (the Unix exact-reserve hit-rate item, since closed and moved to that file's "Recently resolved"); this card's own content is referenced elsewhere only by task number (#821) or commit (`7ee57a9`), never by item number, so the renumbering breaks no cross-reference (verified by `git grep -n "item 46"` across `docs/` — see below).
    `captrack` is a heavy dependency (proc macro, `ctor` constructor with a
    background thread, supply-chain side effects). The workspace-root
    `.cargo/config.toml` suppresses autodump via `CAPTRACK_AUTODUMP=0`, but that
    config does NOT travel with the published crate tarball — external consumers
    who build `sefer-region`'s tests will see the ctor run at process startup
    without that suppression. The dependency is intentional (the probe provides
    capacity telemetry for `Region<T>`'s `slotmap::SlotMap` backing that cannot
    be obtained any other way — see `tests/captrack_probe.rs`'s module doc), and
    a std-only alternative was drafted and explicitly declined in this task
    (the user decided to keep captrack rather than rewrite the probe). The
    mitigation applied is exact version pinning (`=0.1.1`), which eliminates the
    risk that a future minor/patch bump of captrack silently changes its
    side-effect profile without an explicit review.

    **Standalone-build verification (empirical, not reasoned):** A fresh copy
    of `crates/sefer-region/` was extracted to `C:\temp\sefer-region-standalone-test`,
    outside this workspace (no parent `[workspace]` Cargo.toml, no
    `.cargo/config.toml` with `CAPTRACK_AUTODUMP=0`). Commands run:
    - `cargo build --tests` — clean compile, no visible side effect
    - `cargo test --no-default-features --features std` — all tests passed,
      `captrack_probe` correctly ignored
    - No stray JSON files, no persistent background processes observed (the
      ctor's behavior was not directly instrumented, only its visible side
      effects; absence of obvious artifacts is NOT proof of absence, but it is
      the empirical baseline recorded here)

    **Status:** OPEN — the dependency and probe remain in place per explicit
    user decision; only exact-pinning mitigation was applied (not removal,
    not std-only replacement).

    **Next trigger:** revisit if captrack's own side-effect profile changes
    (e.g., future releases introduce additional ctor activity), or if a future
    round decides to swap it for the std-only alternative that was drafted and
    declined in task #821.

90. **[T, second instance of item 89's class — a NO-GO review landed with zero index cards] Commit `967b821` committed `docs/reviews/2026-08-19-aligned-vmem-publication-readiness-audit-r2.md` (independent read-only re-audit of `aligned-vmem`, verdict **NO-GO for publishing 0.2.0** — 0 Critical, 0 High, 4 Medium, 3 Low, 3 perf candidates, 1 coverage gap) and, per its own body, "filed as #1171 (decision), with #1172/#1173/#1174 sequenced behind it" — but filed no card in this index, `docs/perf/OPEN_ITEMS.md`, or either archive. Filed here (task #1175) exactly one round after item 89 named the same "filed as a follow-up" pattern in a different script, confirming it is a recurring class, not a one-off.** (Filed 2026-08-19, task #1175.)

    - **Status (refreshed 2026-08-20, task #1208):** OPEN — but for ONE reason only, and it is not an unexecuted fix. All four owners have LANDED: **#1172** (M1-hybrid + M3 split + M2-as-consequence), **#1180** (M4, `DecommitOutcome`), **#1173** (L1+L2) and **#1174** (C1, the real-HugeTLB memory-content oracle) are all complete. What remains is the single OPEN QUESTION below — adopted HugeTLB granularity ≠ 2 MiB — which is an OWNER DECISION (task **#1190**), not work. This card closes when #1190 is answered.
      **The line this replaces said "None of #1172/#1180/#1173/#1174 has landed yet", and stayed true-then/false-later for a full wave** — it was accurate when written (2026-08-19, task #1171) and was invalidated by the four landings without anyone updating it. That is the exact staleness class R34-24 created current-state cards to prevent, and the reason task #1208 exists as a separate, deliberately-LAST task in its wave: refreshing a Status line mid-wave only re-stales it (the task-#1193 lesson).
    - **Current-verdict:** `git grep -n "2026-08-19-aligned-vmem-publication-readiness-audit-r2"` returned zero hits before this edit (re-verified 2026-08-19, task #1175) — the review file existed, was fully committed (203 lines), and was referenced nowhere. `grep -n "from_raw_parts" docs/CORRECTNESS_OPEN_ITEMS.md` before this edit matched only item 53 (CLOSED, a different `granted_huge` finding from the 2026-08-14 pre-release review, not this one). Report is read-only by its own stated scope ("Режим: только чтение... без запуска тестов, сборки, cargo check, Clippy, Miri, benchmark и `cargo publish --dry-run`") — its findings below are UNVERIFIED BY EXECUTION, only by source reading; treat accordingly.
    - **The four Medium findings (report §M1–M4, `docs/reviews/2026-08-19-aligned-vmem-publication-readiness-audit-r2.md:39-117`), each blocking 0.2.0 publication because a post-publication fix would need a semver-incompatible change:**
      - **M1** — `Reservation::from_raw_parts`'s `granted_huge: bool` cannot describe an arbitrary adopted HugeTLB mapping (no encoded huge-page size or `munmap` granularity requirement); `Drop` silently leaks the whole mapping if `reservation_len` is not huge-page-multiple.
      - **M2** — an already-adopted huge mapping's decommit behavior depends on the CONSUMER's Cargo feature set: `is_huge()` is available without `huge-pages`, but the eligible-forward `MADV_DONTNEED` path is feature-gated, so the same live mapping is served or silently skipped depending on the caller's crate composition.
      - **M3** — `from_raw_parts`'s `# Safety` section mixes true memory-safety preconditions with functional/behavioral ones; the crate's own integration tests deliberately violate some of the mixed-in conditions while explaining why doing so is not UB — a self-contradictory contract.
      - **M4** — `try_decommit() -> Result<(), VmemError>` reports range validity, not decommit outcome; `Ok(())` cannot distinguish a Rust-level skip, kernel-rejected `madvise`/`VirtualFree`, accepted advisory, or actual RSS reclaim, despite rustdoc for `can_decommit_reclaim_and_zero()` suggesting the return value can be used to judge exactly that.
    - **Three Low findings (report §L1–L3, lines 119-146):** **L1** — poison-state `decommit()` is documented as a no-op but `debug_assert!(false, ...)` makes it a debug-build panic, undocumented in `# Panics` and unpinned by any test oracle. **L2** — `VmemError`'s single no-code sentinel is overloaded across at least six distinct causes (invalid args / OS refusal-no-code / zero grant / page-size query failure / fault-injection / mock backend), and `os_refusal_unknown_code`'s "FOUR sources" doc claim does not enumerate the real call sites. **L3** — **already covered by item 88 above (MSRV does not compile `aligned-vmem/huge-pages` — no `-p aligned-vmem` row in the `msrv` CI job); not a duplicate card, only cross-referenced here as the same gap re-surfacing from a second source.**
    - **Coverage gap (report, "Coverage gap" section, lines 170-174):** the real-HugeTLB CI job proves `MAP_HUGETLB` grant + eligible `madvise` dispatch, but reads no memory post-decommit and measures no RSS/pool reclaim — it distinguishes "kernel accepted the advice" from nothing, not from "memory was actually returned."
    - **This header's "3 perf candidates" (report §P1-P3), owners and content:** filed in `docs/perf/OPEN_ITEMS.md` item 57 (task #1182) — the perf-scoped index this material's own Scope belongs to, which the commit landing this card did not touch.
    - **Owning follow-up tasks (post-decision split):** **#1171** — DONE by this card: records the owner's M1/M3/M4 decision below and owns the OPEN QUESTION at the end of that block. **#1172** — phase 1: M1-hybrid + M3 split + M2-as-consequence, plus the stale `decommit_lazy` huge-advice fix (`crates/aligned-vmem/src/reservation.rs:637-639,891-895`) and the `lib.rs:62-63` precision edit (its point (4)). **#1180** — phase 2: `DecommitOutcome`, absorbing the third audit's §P2 dispatch-unification finding (one private helper, one `page_size` snapshot). **#1173** — L1+L2 (L3 redirects to item 88). **#1174** — strengthen the real-HugeTLB coverage gap (write→decommit→recommit/read-zero + RSS/pool observation).
    - **Owner decision on M1/M3/M4 (task #1171, decided 2026-08-19) — all three recommendations adopted in full, unchanged.** Three sources converged: the PUB-R2 report (the findings), the `@fh` consultation it prompted (the recommendations, previously recorded here as pending), and a third independently commissioned audit (`docs/reviews/2026-08-19-2148-aligned-vmem-publication-audit-Сол-кодекс.md`, revision `000c076`, verdict NO-GO, same four Medium findings) that reaches the same hybrid, the same safety/contract split, and the same `DecommitOutcome`, and restates both premise refutations below. Provenance caveat, stated so this card does not overclaim independence: that audit reported at 21:48 +02:00, AFTER the `@fh` recommendations were committed (`062a318`, 20:22 +02:00); its own §"Итог" cites "item 90" of THIS file by name and its boundary section lists the correctness open items among its inputs — so it demonstrably READ the `@fh` block it agrees with. It is therefore a CONCURRING review by an independently commissioned reader, NOT a blind re-derivation, and its agreement is weaker evidence than three genuinely independent derivations would be. What is genuinely its own contribution: the two ADDRESS-alignment conjuncts (`reservation`, `base`) it adds to the 2-MiB assert list the decision below adopts, on top of `@fh`'s original three fields (`len`, `reservation_len`, the offset `base − reservation`) — a genuine strengthening (see item 91's corrected note below for why), even though the resulting list is five NAMED quantities but only FOUR independent checks, not five independent ones (task #1196/OX6-L1 correction) — the 1 GiB-granularity argument behind the OPEN QUESTION below, the vendor-doc confirmations for M4 (Linux `madvise(2)`, Microsoft `VirtualFree`), and its §P1-P3 perf findings. Its file was UNTRACKED when this decision was recorded; task #1186 commits it together with its own index card — item 91 below, which carries the same provenance caveat and files the audit's genuinely-new findings. **Edit-history note (task #1205):** the "genuinely its own contribution" clause immediately above was REWRITTEN IN PLACE by task #1196, not appended to. `9e1a734`'s commit body says item 90's decision text was "left VERBATIM … the correction is an appended note", which is true of the `- **M1 — HYBRID**` sub-bullet below (untouched, with a note added under it) but NOT of this paragraph, which `git diff 9e1a734^ 9e1a734` shows as a `-`/`+` replacement. Recorded here because the non-retroactive convention this file follows is worth more than a tidy claim about having followed it; the in-place edit itself is left standing, since reverting it would restore a figure task #1196 correctly showed to be misleading.
      - **M1 — HYBRID** (none of the report's own options): keep `granted_huge: bool`, but narrow what `true` MEANS to "the mapping is in the crate's own 2 MiB format": (a) `from_raw_parts` asserts, on Linux/Android, that `len`, `reservation_len`, `reservation`, `base`, and the offset `base − reservation` are ALL 2 MiB multiples when `granted_huge == true`; (b) accepting `granted_huge: true` is gated behind the `huge-pages` feature — this CLOSES M2 as a consequence (with no feature there are no huge-flagged adopted mappings at all, so there is no divergent behavior to have); (c) rewrite the `reservation_len` rounding bullet (`crates/aligned-vmem/src/reservation.rs:1071-1078`) to state the HugeTLB exception explicitly. REJECTED: banning huge in `from_raw_parts` outright — breaks the documented `into_full_parts` → `from_raw_parts` round-trip, the only host-independent CI coverage of the huge branch. REJECTED, conditionally on the open question below: typed huge-granularity metadata — over-engineering while the crate implements exactly one size, `LINUX_HUGE_PAGE_SIZE = 2 MiB`.
        **Appended note (task #1196/OX6-L1, decision text above left verbatim per this file's non-retroactive convention):** the five names in (a) are not five independent checks — `reservation` and `base` both being 2-MiB multiples already implies their difference `base − reservation` is too, so the offset conjunct can never be the one that fails; the assert as SHIPPED (`crates/aligned-vmem/src/reservation.rs`) has FOUR independent checks, not five, and the code and its comments now say so. The decision itself is unaffected — the shipped assert still checks exactly the five names listed here; only the "five independent requirements" framing was corrected, not the check.
      - **M3 — SPLIT.** `# Safety` keeps ONLY memory-safety preconditions; the functional requirements (accuracy of `granted_huge`, Windows commit-state) move to a separate "Correctness contract" section stating the precise consequence of violating each. REJECTED: declaring the tested violations UB and rewriting the tests — that would assert something demonstrably false; `tests/granted_huge_reader_enumeration.rs` pins exactly the non-UB behavior.
      - **M4 — `Result<DecommitOutcome, VmemError>`** with `Skipped` / `Advised` / `Refused(VmemError)` behind ONE private helper. `Advised` means "the kernel accepted the advice", explicitly NOT "RSS was reclaimed" — that gap is owned by the coverage-gap item (#1174), not by M4. REJECTED: option (b) — renaming/documenting the function as a range-contract check only.
      - **Adopted blast radius (for #1172/#1180):** the M1-hybrid assert breaks `tests/reservation_decommit_contract.rs:405` (`method_try_decommit_reports_malformed_range_on_huge_flagged_reservation`, currently ungated — needs `#[cfg(feature = "huge-pages")]`; its sibling at `tests/decommit_capability.rs:490-492` is already gated). `numa-shim` is unaffected (always passes `granted_huge: false`). `DecommitOutcome` lands in phase 2 (#1180) across the backend files plus one new `src/decommit_outcome.rs` (one-file-one-export rule).
      - **Two PUB-R2 premises stand REFUTED (established by `@fh`; the third audit concurs, but had read the `@fh` block — see the provenance caveat above — so this is one derivation plus one agreement, not two independent ones. They generate no remediation work):** (1) "`Drop` silently loses the `munmap` error" is FALSE — under `bench-internals` it IS counted, in `UNIX_MUNMAP_FAILURES` (`crates/aligned-vmem/src/os/unix.rs:1103-1109`, task P2-6). Caveat both sources add: in the ordinary public API the result is still unobservable — a separate question, not an accounting defect. (2) The crate-level "every entry point has two forms" text is correctly scoped by `crates/aligned-vmem/src/lib.rs:55` ("every **reservation/commit** entry point"); the REAL inaccuracy is a DIFFERENT sentence, at `lib.rs:62-63` ("The infallible forms forward to the `try_*` forms"), which is wrong for the decommit family — that fix belongs to task #1172, point (4).
      - **Version: publish as 0.2.0, NOT 0.3.0** (0.2.0 has never reached crates.io; skipping to 0.3.0 would orphan existing CHANGELOG references for no semantic reason).
      - **OPEN QUESTION (owner: task #1190 — re-pointed by #1190 itself from #1171, which is DONE per the "Owning follow-up tasks" line above and so must not stand as a live owner per R34-24; raised by the third audit after the decision was taken; ANSWERED 2026-08-20, appended directly below):** does the crate intend to support ADOPTING HugeTLB mappings whose granularity is not 2 MiB (e.g. 1 GiB)? Today's default is NO — the crate nowhere promises such support — and under that default the hybrid stands as decided above. If the answer ever becomes YES, typed huge-granularity metadata must be introduced BEFORE the first publication (afterward it is a semver break), and task #1172's scope grows accordingly.
        **Appended answer (task #1190, owner decision 2026-08-20): NO.** `aligned-vmem` does not support adopting a HugeTLB mapping whose granularity is not 2 MiB; the 2-MiB assert is the contract, not a temporary narrowing. Decided pre-publication, so the `granted_huge: bool` contract is final as shipped — a future "yes", if it ever comes, would be ADDITIVE (a new constructor carrying typed huge-granularity metadata, not a relaxation of the existing assert presented as a bugfix), because `granted_huge: true` already means "this crate's own 2 MiB format" (#1172's narrowing) and stays truthful for that case. Recorded by #1190 in `crates/aligned-vmem/src/reservation.rs` (`from_raw_parts`'s `granted_huge`-accuracy and 2-MiB-multiple contract bullets) and `crates/aligned-vmem/CHANGELOG.md`.
    - **Next trigger:** landing commits for #1172 → #1180 → #1173/#1174 — move this Status line to reflect each as it lands (with commit SHAs) until all four are closed; and the owner's answer to the OPEN QUESTION in the decision block (if YES, typed metadata moves INTO #1172 and must land before the first publish — record the answer here when given).
    - **Evidence:** `docs/reviews/2026-08-19-aligned-vmem-publication-readiness-audit-r2.md` (full report, 203 lines, commit `967b821`); `docs/reviews/2026-08-19-2148-aligned-vmem-publication-audit-Сол-кодекс.md` (third audit, revision `000c076`, reported 2026-08-19 21:48 +02:00 — committed by task #1186 alongside its index card, item 91); commit `062a318` (task #1175, 2026-08-19 20:22 +02:00 — landed the `@fh` recommendations this decision adopts; their full rationale, including the `libc_madvise`/task-#1164 availability argument for M4, is preserved verbatim there); `crates/aligned-vmem/src/os/unix.rs:53-59,1103-1109` (huge-munmap-granularity mechanism + the existing `UNIX_MUNMAP_FAILURES` counter under `bench-internals`); `crates/aligned-vmem/src/lib.rs:55,62-63` (the correctly-scoped "reservation/commit" entry-point claim, and the actually-wrong infallible-forwards sentence); `crates/aligned-vmem/src/reservation.rs:637-639,891-895,1071-1078,1217-1229` (the stale `decommit_lazy` advice, the unqualified rounding bullet, and the `PAGE`-only assert); `tests/reservation_decommit_contract.rs:405` and `tests/decommit_capability.rs:490-492` (the test that will need gating under the M1-hybrid, and its already-gated sibling); `tests/granted_huge_reader_enumeration.rs` (pins the non-UB behavior M3's split preserves); item 53 above (the distinct, already-closed prior `granted_huge` finding, confirmed not a duplicate); item 88 above (owns L3/MSRV — not duplicated here).

91. **[T, third independent audit of `aligned-vmem` — its report indexed in the SAME change that commits it, per the item-89/90 class lesson] `docs/reviews/2026-08-19-2148-aligned-vmem-publication-audit-Сол-кодекс.md` (author Сол-кодекс; reported 2026-08-19 21:48:52 +02:00; revision `000c076`; verdict **NO-GO for publishing `aligned-vmem 0.2.0`** — 0 Critical, 0 High, 4 Medium (the blockers), 4 Low, 3 Performance, 2 Coverage) is a STATIC read-only audit: no build, tests, or CI gates were executed, so its findings are UNVERIFIED BY EXECUTION — source reading only. Its four Medium blockers are the SAME M1-M4 item 90 already owns (the report says so itself, its line 19), so this card files NO duplicate blockers — it records the re-confirmation, indexes the report, and assigns owners to the genuinely new findings.** (Filed 2026-08-19, task #1186.)

    - **Status (refreshed 2026-08-20, task #1208):** OPEN — a cross-reference/ownership hub, not a new defect class, and now down to ONE outstanding owner. Its stated closure condition was "item 90's four owners plus #1187-#1190 have all landed or been dispositioned": item 90's four (#1172/#1180/#1173/#1174) have landed, and of the genuinely-new findings **#1187** (rustdoc "incompatible outright" + the false `Drop` SAFETY premise), **#1188** (the `align > 2 MiB` pool-amplification measurement) and **#1189** (speculative-path counters + the release-HugeTLB oracle) are complete. **#1190** — the HugeTLB-granularity owner decision — is the only one left, and it is the SAME single blocker item 90 is now waiting on. Closes with #1190.
    - **Current-verdict:** NO-GO stands at `000c076` — all four Medium blockers re-derived there and still open. Mode is static: findings are NOT execution-confirmed; the report's §"Граница исследования" (its lines 216-222) enumerates exactly what was read (crate sources, Unix/Windows/miri/mock backends, public rustdoc, manifest, README/changelog, test oracles, CI, and THIS file's open items — its line 220).
    - **Independence caveat (mirrors item 90's; times and ancestry re-verified this task):** the audit was NOT blind. `git show -s --format='%ci' 062a318` → 2026-08-19 20:22:47 +0200 (the commit that landed the `@fh` recommendations into item 90); the report's header (its line 5) reads 2026-08-19 21:48:52 +02:00 at revision `000c076` (its line 7), and `git merge-base --is-ancestor 062a318 000c076` succeeds — the audited tree CONTAINS the `@fh` block. The report cites "item 90" of THIS file by name (its line 19) and lists the correctness open items among its inputs (its line 220): it demonstrably READ what it agrees with. It is a CONCURRING review by an independently commissioned reader, NOT a blind re-derivation, and its agreement is correspondingly weaker evidence — the same conclusion, in the same words, as item 90's caveat above; the two statements must not drift apart.
    - **M1-M4 — CONFIRMATION on a new revision, not new findings:** owners unchanged from item 90 — **#1172** (phase 1: M1-hybrid + M3 split + M2-as-consequence), **#1180** (phase 2: `DecommitOutcome`), **#1173** (L1+L2), **#1174** (C1 coverage). ONE blocker set, confirmed twice — item 90's decision block owns what was decided.
    - **Genuinely the audit's own (the value beyond concurrence) — corrected by task #1196/OX6-L1, see item 90's appended note above:** (a) the two ADDRESS-alignment conjuncts this audit added to `from_raw_parts`'s 2-MiB-multiple assert list — `reservation` and `base` — on top of `@fh`'s original three-field list (`len`, `reservation_len`, and the offset `base − reservation`; report line 73). This addition is genuine and strictly STRENGTHENS the check: under `@fh`'s three fields alone, a reservation at address 1 MiB with base at 3 MiB PASSES (offset = 2 MiB, a clean multiple) while BOTH addresses are individually 2-MiB-misaligned — and it is the ADDRESS alignment that `munmap(2)` on a `MAP_HUGETLB` mapping actually requires, not just the length. But the same two additions make `@fh`'s third field (the offset) REDUNDANT, not additive on top of it: `reservation` and `base` both being 2-MiB multiples already implies their difference is too, so the assert's five NAMED quantities resolve to FOUR independent checks, not five. (Task #1196 corrects the "five-field"/"all five" framing used here and in item 90's decision block above to this four-independent/five-named form; the fifth conjunct itself was not removed from the code — it stays for its panic-message diagnostics and because it would become load-bearing again if either address conjunct were ever weakened.) (b) the 1-GiB HugeTLB argument — an adopted 1-GiB mapping cannot be honestly served by a 2-MiB eligibility check, because a 2-MiB-multiple range can still be invalid for the mapping's real granularity (report line 65); this is what raised item 90's OPEN QUESTION, and the follow-up carrying it to the owner is **task #1190**; (c) vendor confirmations for M4 that Linux `madvise(2)` and Windows `VirtualFree` both return an observable success/failure result (report line 120, man7/Microsoft Learn links inline there); (d) §P1-P3 + §C1-C2 (owned below).
    - **New findings, each with an owner (nothing below duplicates item 90 or its tasks):**
      - **L4 (report lines 149-159), five stale doc/comment sites → task #1187** owns the two not already covered: `crates/aligned-vmem/src/api/decommit.rs:250-251` (published rustdoc of a public `unsafe fn` still calls the huge-page operation "incompatible outright", contradicting what task #1140 established — the Linux >= 5.18 eligible path) and the `Drop` SAFETY comment at `crates/aligned-vmem/src/reservation.rs:1262` (asserts the reservation "was returned by `reserve_aligned`", ignoring the public `from_raw_parts` adoption path). The two of the five ALREADY owned are `crates/aligned-vmem/src/lib.rs:62-63` and `crates/aligned-vmem/src/reservation.rs:637-639` — both inside #1172's scope, cross-referenced only. The fifth (moving long historical task-number narratives out of runtime modules) is a PROPOSAL to the owner — a convention change to decide, not filed as work.
      - **P1 (report lines 163-167) → task #1188:** with `align > 2 MiB` the generic path maps `size + align` and keeps the whole mapping — up to 2× the scarce, pre-allocated HugeTLB pool (the exact huge fast path exists only at `align == LINUX_HUGE_PAGE_SIZE`, 2 MiB); post-hoc huge-page trimming cannot remove the admission cost. Measure pool occupancy, fallback rate, and extra-syscall cost on the real-HugeTLB runner before optimizing.
      - **P2 — ABSORBED into #1180** (one private dispatch, one `page_size` snapshot); no separate owner, deliberately not re-filed.
      - **P3 + C2 (report lines 173-177 and 187-191 — NOT one contiguous span: §C1 sits between them at 181-185 and is owned by #1174) → task #1189:** speculative platform paths need measurement before any removal (Linux larger-huge retry before ordinary fallback; Windows large-page failed-request cost profile; generic 64-bit Unix over-reserve), and HugeTLB release has no direct oracle — `UNIX_MUNMAP_FAILURES` is never checked around `Drop` in the real-HugeTLB tests, so a release call that disappeared entirely would not redden the job.
      - **C1 — already #1174. L1, L2 — already #1173. L3 (MSRV) — already item 88.** Cross-references only; no duplicate cards filed.
    - **Next trigger:** landing commits for #1187/#1188/#1189, and the owner's answer to #1190 (record the answer in item 90's OPEN QUESTION block — that is where the decision lives, not here); once item 90's four owners and #1187-#1190 are all dispositioned, move this card to "Recently resolved" per the R34-24 convention.
    - **Evidence:** the report `docs/reviews/2026-08-19-2148-aligned-vmem-publication-audit-Сол-кодекс.md` (222 lines — `wc -l`, re-measured this task; the "223" figure that circulated in this wave's task briefs counted the trailing newline as a line and is wrong; untracked in the main checkout until the same operator commit that carries this card — the Cyrillic filename breaks no guard: no script in `scripts/` enumerates `docs/reviews/`, `verify-gate-report.mjs` scans `scripts/` + `docs/perf/` only, and the vmem guards walk only `.rs` files under `crates/`); timestamps and ancestry re-verified this task (`git show -s --format='%ci' 062a318` → 20:22:47 +0200; `git show -s --format='%ci' 000c0767416b96df11aa7bbb8b80efb4c09cb754` → 20:58:40 +0200; `git merge-base --is-ancestor` → exit 0); report lines 5, 7, 15, 19, 65, 73, 120, 220 re-read before citing; `crates/aligned-vmem/src/api/decommit.rs:250-251` and `crates/aligned-vmem/src/reservation.rs:1262` confirmed by `grep -n` this task ("incompatible outright" → line 251; "was returned by `reserve_aligned`" → line 1262); commit `a839d63` (task #1140, the Linux >= 5.18 huge-decommit eligibility work the `decommit.rs` sentence contradicts); item 90 above (the one confirmed blocker set; its independence caveat is this card's mirror); item 88 above (owns L3).

93. **[T, FOURTH independent audit of `aligned-vmem` — report committed and indexed in the SAME change, per the item-89/90 class lesson] `docs/reviews/2026-08-20-073908-aligned-vmem-publication-audit-Сол-кодекс.md` (author Сол-кодекс; reported 2026-08-20T07:39:08+02:00; revision `dc2ecdd`; verdict **NO-GO for publishing `aligned-vmem 0.2.0`** — 0 Critical, 1 High, 3 Medium, 3 Low, 3 Performance, 3 Coverage) is a STATIC read-only audit: no tests, build, `cargo check`, Clippy, Miri, benchmarks or `cargo publish --dry-run` were run, so its findings are UNVERIFIED BY EXECUTION unless this card says otherwise per finding.** (Filed 2026-08-20, task #1209.)

    - **Status (refreshed 2026-08-20, task #1208 — the wave is now COMPLETE except for one owner decision and the closing audit):** OPEN. Landed since filing: **#1210** (H1 — the UB-producing test deleted outright, `1f930e2`), **#1211** (M1 — docs half `05557a6`, `src/` half `1522d25`), **#1212** and **#1213** (`1522d25`), **#1214** (C2/C3/P1-P3 into the indexes, `dd1061e`). **#1207** resolved **NULL** (`7db9f08`): the `Unreleased`-in-tarball hazard it described is already covered by `.github/workflows/release.yml:301-309`; the task's premise came from a grep too narrow for the conclusion drawn from it. Still open: **#1190** (M3, the owner decision) and **#1216** (step 5, the fifth independent audit). Two follow-ups were spun off rather than absorbed silently: **#1219** (restore `Refused` coverage — #1210's honest cost) and **#1220** (four crates have no `CHANGELOG.md`, found while resolving #1207).
    - **Current-number-or-verdict:** NO-GO stands at `dc2ecdd`, and **is not lifted by this refresh** — lifting it is a separate decision that requires #1190 answered and #1216's audit run, per this card's own Next trigger. The blocking set is H1 + M1 + M2 + M3; the audit's own recommended release gate (its lines 168-176) is five steps, of which steps 1-4 are tasks **#1210** (H1, DONE), **#1211** (M1, DONE), **#1212** (M2, DONE) and **#1190** (M3, the owner decision that predates this audit, STILL OPEN), and step 5 — "run a NEW independent audit after the fixes" — is task **#1216**, which had no owner until this card was filed.
    - **Next trigger:** landing commits for #1210/#1211/#1212/#1213/#1214 and the owner's answer to #1190. When all of those are dispositioned, #1208 refreshes this card's Status alongside items 90/91, and only then is lifting the NO-GO a separate decision.
    - **Evidence:** the report itself (182 lines, `wc -l`, committed by this task); the H1 code site re-read by me directly (`crates/aligned-vmem/src/os/windows.rs:366`, `let addr = unsafe { base.add(start) };` against `far_start = 64 * 1024 * 1024` in `crates/aligned-vmem/tests/decommit_outcome.rs`); the M1 site re-read directly (`crates/aligned-vmem/README.md:56` still documents `try_decommit ... -> Result<(), VmemError>`); the M2 semver premise re-checked directly (`crates/aligned-vmem/src/decommit_outcome.rs:25` is `#[non_exhaustive]`); the C3 duplicate check (item **59b** already exists at this file's "Windows half of item 59" card — #1214 updates it rather than filing a new one).
    - **Independence and mode, stated so this card does not overclaim.** The audit ran read-only with no sub-agents and executed nothing. Its own §"Граница исследования" (line 180) records that HEAD advanced from `22a91cc` to `dc2ecdd` during the work and asserts the extra commit "only changes comments/guards around the test and does not remove H1". **Verified by me, not accepted:** `git show --stat dc2ecdd` touches `tests/decommit_outcome.rs`, `docs/CORRECTNESS_OPEN_ITEMS.md` and `tests/no_stale_doc_references.rs` (task #1206) and does not alter the `far_start` arithmetic — so H1 does stand at the audited revision.
    - **What this audit says the PREVIOUS wave actually closed (its lines 153-166), and why that matters:** it explicitly states the four earlier Medium findings must NOT be carried over mechanically — three are closed on the merits, and the huge-granularity question has narrowed to an explicit pre-release design decision. It credits eight closures by name: the `from_raw_parts` safety/correctness split; `granted_huge=true` no longer silently changing behavior with `huge-pages` off; the Linux/Android 2-MiB shape asserts; `try_decommit` returning `DecommitOutcome` with Unix/Windows wrappers preserving syscall refusal; page-size poison no longer a debug-only panic; the MSRV gate covering `aligned-vmem --all-features` plus `--no-run` test compile; real-HugeTLB CI gaining write → decommit → read-zero plus a release-attempt oracle; and release paths gaining attempt/failure counters. The NO-GO therefore NARROWED between the third and fourth audits rather than reproducing.
    - **Owner map (no duplicate cards filed — cross-references only):** H1 → **#1210** (also closes the audit's C1). M1 → **#1211**. M2 → **#1212**. M3 → **#1190** (pre-existing owner decision; the audit adds one requirement — remove the "not yet answered by the crate owner" wording from the published rustdoc, since an unresolved question on an unsafe boundary is itself a release defect). L1+L2+L3 → **#1213**. C2 + C3 + P1/P3 → **#1214**, which UPDATES item 59b for C3 rather than filing it new. C1 → already item 92 + #1210. P2 → already measured by task #1188 (perf item 57). The `## 0.2.0 - Unreleased` header the audit does not name is separately owned by **#1207**.
    - **Boundary of the audit's own reading (its line 178):** `Cargo.toml`, public exports and rustdoc, reservation/lazy ownership, raw-parts round trips, the decommit/recommit/commit/release API, the Unix/Windows/miri/mock backends, the error model, arithmetic/align checks, the feature/cfg matrix, README, CHANGELOG, test oracles, package/CI/MSRV/docs.rs gates, and the delta from the previous audit. Nothing outside that set was examined.

94. **[T, FIFTH independent audit of `aligned-vmem` — report committed and indexed in the SAME change, per the item-89/90 class lesson that #1186 and #1209 each had to relearn] `docs/reviews/2026-08-20-fifth-aligned-vmem-publication-audit.md` (agent `@oh`; revision `1b72e73`; verdict **NO-GO for publishing `aligned-vmem 0.2.0`** — 1 High, 3 Medium, 6 Low, 2 Info) is, unlike the third and fourth audits, an EXECUTING audit: it ran `cargo test`, `clippy`, `cargo doc` under two feature sets, `cargo package --list` and all six guard scripts, and it states per finding whether the evidence is executed or read.** (Filed 2026-08-20, task #1233.)

    - **Status (2026-08-20, at filing):** OPEN, but the blocking half is already closed. Landed since the audit reported: **#1225** (F1, `368d391`), **#1226** (F2, `3b5bea5`), **#1227** (F3, `2f9d7b9`), **#1228** (F4+F5, `3210f61`), **#1229** (F6+F7+F8, `dcbe0fc`), **#1230** (F10+F11, `56377ee`). **F9 was already owned** by task #1218 before this audit reported it, and that task's description now carries the audit's second, previously-unknown half (the `hasNonCommentChange` regex is blind to git's quoted `diff --git "a/…"` header, which MASKS the first bug by downgrading ERROR to WARNING — so fixing only one of the two turns a false warning into a false error). **F12 is #1190**, the owner decision, ANSWERED 2026-08-20 ("NO" — see that task) but not yet carried into the rustdoc, the card, or the CHANGELOG.
    - **Current-number-or-verdict:** **NO-GO stands**, and both of its grounds are now dispositioned differently: ground 2 (F1, M2 not closed on the publishable surface) is CLOSED by #1225; ground 1 (F12/M3, `src/reservation.rs` shipping "not yet answered by the crate owner" to docs.rs) is ANSWERED but NOT YET WRITTEN DOWN. Lifting the NO-GO remains a separate decision and requires, at minimum, that #1190's recorded answer reach the four places its own task description names.
    - **Next trigger:** #1190's answer landing in `src/reservation.rs`, item 90's card, and `CHANGELOG.md`. When that lands, this card and item 93's are refreshed together, and only then is lifting the NO-GO on the table.
    - **Evidence:** the report itself (committed by this task); F1 re-verified by me directly before acting on it — `git log --oneline -1 -S "OS/kernel accepted it" -- crates/aligned-vmem/README.md` resolves to `05557a6` (task #1211, this same wave), and `git show 1522d25 -- crates/aligned-vmem/src/decommit_outcome.rs | grep "^@@"` prints exactly one hunk, `@@ -48,21 +48,58 @@`, proving lines 10-24 were never touched by the task chartered to fix them; the census figures behind F11 re-run at three revisions (43 at HEAD, 44 at `4f4d9f4`, 43 at `1b72e73`), confirming "42" was wrong when written.
    - **What this audit found that the previous four did not, stated because it is the reason to keep running them:** every one of F1-F8 is a defect INSIDE the fixes of the audit before it, and F1 is the sharpest instance the campaign has produced — task #1211 ADDED the false sentence to `README.md` at 09:05 and task #1212 DELETED the identical sentence from `src/` as incorrect at 09:24, each treating the other's file as out of scope. Two tasks of one wave, nineteen minutes apart, on the crates.io front page.
    - **Independence and mode, stated so this card does not overclaim.** The audit ran read-only with no sub-agents, no git writes and no file edits, but DID execute code — this is the first of the five audits to do so, and its "Verified clean" section (~20 surfaces, including the lookup table re-derived 70/70 and card-body preservation re-derived across all three splits) is therefore evidence, not reasoning. Its own brief is reproduced in task #1216's description; the working copy `.audit5-brief.md` is scratch and deliberately not committed. **Filename is ASCII-only on purpose:** the third and fourth reports carry Cyrillic names, and those names are exactly what triggers the `verify-commit-prefixes.mjs` pair of bugs this audit reports as F9.
    - **[Added 2026-08-20, task #1234 — a dispositioned DECISION, not a fifth-audit finding; the audit did not report this] `1522d25`'s commit body overclaims one of its two "#1174 sites", and the decision is to leave `decommit_outcome.rs` WITHOUT a #1174 reference — deliberately.** The body (task #1212) says: *"Two sites still called task #1174 an OPEN gap — `decommit_outcome.rs`'s `Advised` doc and `reservation.rs`'s `try_decommit` payload list. Both now state what #1174 actually proved (zero-fill on readback …) and what it did NOT (physical reclaim …)."* Both halves verified this task, per half: for `reservation.rs` the claim is TRUE — a line naming #1174 was replaced by positive statements ("Task #1174 (closed) added zero-fill-on-readback … What #1174 did NOT"); for `decommit_outcome.rs` it is FALSE — the "open per task #1174" line was DELETED and nothing about what #1174 proved was added; the file's `1174` count goes 1 → 0 across that commit. The body cannot be edited, so the falsity is permanent no matter what any later commit does to the tree. **The shipped contract is unharmed** — re-read in full this task: `decommit_outcome.rs`'s type-level doc still opens "**None of the three variants is a claim about physical memory having actually been reclaimed**", and `Advised`'s own doc still carries "**Never a claim that physical pages were actually returned to the OS**, even on the native backend". **Decision — option (b) of the two posed: do NOT add a #1174 reference to `decommit_outcome.rs`.** Grounds: (1) that file documents the per-backend SEMANTICS of each variant (what "accepted" means on native/mock/miri), not CI coverage — task #1174 is a coverage fact about one CI test, and the surfaces that legitimately carry it are the ones whose subject IS that coverage chain; (2) the fact already lives in FIVE files (counted this task: `src/api/decommit.rs`, `src/api/reserve_aligned_huge.rs`, `src/reservation.rs`, plus `README.md` and `CHANGELOG.md` — the three-file count in the task brief undercounts), and this repository has a named drift class for one fact in N copies (#1161, six reproductions); (3) adding the reference now could not make the commit body true anyway — the body claims what `1522d25` ITSELF did, and that diff is immutable. **Next trigger:** none required; re-open only if a future reader mistakes the absent #1174 reference in `decommit_outcome.rs` for an oversight — this entry is the record that it is a decision. **Evidence (all run this task):** `for s in 1522d25^ 1522d25; do git show $s:crates/aligned-vmem/src/decommit_outcome.rs | grep -c 1174; done` → `1`, `0`; `git show 1522d25 -- crates/aligned-vmem/src/decommit_outcome.rs | grep -E "^[-+].*(1174|zero-fill|read.back|HugePages_Free)"` → exactly one deleted line, zero added; `git show 1522d25 -- crates/aligned-vmem/src/reservation.rs | grep -E "^[-+].*1174"` → one deletion, two additions; full re-read of current `decommit_outcome.rs` (both no-reclaim-claim sentences present); `grep -rc 1174` across the five files named above. **Why an append to item 94, not a new item 95:** the existing-owner check found item 93 records only "`M2 → #1212` DONE" and item 94's F1/F2 dissect OTHER `1522d25` body claims (the "OS/kernel accepted" sentence and the "certified clean" sub-claim), so no card owns this one — but a new item number would require the `docs/CORRECTNESS_OPEN_ITEMS.md` lookup-table row and card-count edits outside task #1234's file scope, leaving the index's "complete item-N → file lookup" claim false (the exact F3 class). Appending to the card whose Evidence block already carries the same `git show 1522d25 -- crates/aligned-vmem/src/decommit_outcome.rs` command follows the dated `[Added …]` precedent inside item 24.
