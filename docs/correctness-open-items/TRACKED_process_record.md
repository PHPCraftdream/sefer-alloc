# Correctness / CI-debt open items -- [T] Tracked tier -- commit-message / count / citation record corrections

**Part of the split index.** This file holds the full text of every **[T]**
(tracked, not yet actioned) card whose subject matches this file's own
criterion (below). Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the round-start reading order, and for
the complete item-number to file lookup table; come here for these specific
card bodies. See `docs/correctness-open-items/ACTIVE.md` for the **[A]**
tier, `docs/correctness-open-items/RESOLVED.md` for the closure trail, and
the sibling `[T]`-tier files (`TRACKED_hook_safety.md`, `TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`, `TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`, `TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`, `TRACKED_misc.md`) for the rest of
the tier.

**Criterion for this file:** A card belongs here if its entire content is a RECORD correction -- a wrong count, a wrong citation, a commit-prefix taxonomy mis-slot, or a "filed as a follow-up" claim that was never actually filed -- needing no code, test, or script change, only an accurate durable record.

**Card count:** 11.

**Why split by theme, not by item-number range (task #1222, 2026-08-20):**
task #1221 (same day) split the former single `TRACKED.md` into four
number-range files, balanced by line count. The owner rejected that split
and asked for a thematic split instead -- grouping cards by what they are
actually ABOUT, derived from reading all 70 cards rather than assumed.
Every citation of this index that points at ONE SPECIFIC ITEM carries
that item's number, in the form `` `docs/CORRECTNESS_OPEN_ITEMS.md`
item N `` -- task #1227 repaired the seven in `aligned-vmem` that did
not, and two outside it were still open as of that task (both are
recorded in the thin index's Structure section). Citations that point
at the FILE as a whole, at a named SECTION, or at a CLASS of items
rather than one item carry no item number and never needed one (task
#1227's finding; until #1236 these headers overclaimed it as a
universal, asserting that no citation ever pointed at anything but
an item number). Only the numbered citations depend on where item
numbers live, and `docs/CORRECTNESS_OPEN_ITEMS.md` (the thin index)
carries the complete, mechanically generated item-N -> file lookup
table covering EVERY `[T]`-tier number (including the `59a`/`59b`
sub-items) that keeps them resolving -- that table, not this file's
name, is what makes the thematic split safe: the lookup is two-hop
(index table, then this file), but mechanical and always correct. No citing-file
count is typed in this header on purpose: the "42+" typed here at
the split was already 43 (census against the split commit) -- #1230
removed it from one of these nine headers, #1236 from the other
eight; compare against this command's output, never a hardcoded
count:

```text
git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l
```

(Split 2026-08-20, task #1222, superseding task #1221's number-range
split the same day.)

---
10. **[T, filed 2026-07-31, UNVERIFIED-BY-ME findings from the Round 32 full
    independent review (`docs/reviews/2026-07-31-r32-full-review.md` §11
    P2-1, P2-6, P2-7, P2-8, P2-11)]** Five P2 findings — NOT independently
    re-verified before filing, per this file's own convention (item 9 above
    is the direct precedent, one round earlier). The round's three P1s
    (P1-1/P1-2/P1-3, all against R31-10) WERE independently re-verified and
    fixed directly in the same session — see the review itself and
    `CHANGELOG.md`'s Round 31 entry for what changed; not filed here.
    - **P2-1 — README's per-file `unsafe` inventory row for
      `src/registry/heap_core_diag.rs` drifted; the tripwire cannot see it.**
      The review's claim: `README.md:594` states 6 hooks for that file; the
      real count is **7** — R31-6 (task #469) added
      `dbg_decomp_recommit_payload` there and correctly bumped the
      AGGREGATE totals (66→68) and the `alloc_core_small_pool.rs` row (2→3),
      but left this file's own row (and its 6-hook prose enumeration)
      untouched. `tests/no_stale_doc_references.rs` asserts only the three
      aggregate tokens, never per-file rows, so this class of drift is
      invisible to CI by construction.
    - **P2-6 — `CHANGELOG.md` covers 1 of Round 32's 11 tasks, and now
      contains a claim `docs/CORRECTNESS_OPEN_ITEMS.md` item 9's own
      resolution has since made stale.** The review's claim: only `38fbe8f`
      (R31-10) touched `CHANGELOG.md`; absent entirely are R31-8's new
      CLAUDE.md rule, three new process tools
      (`verify-gate-report.mjs`/`verify-commit-prefixes.mjs`/
      `tests/ci_clippy_matrix_consistency.rs`), R31-6's correctness fix, and
      all ten fixed review-P2 repairs (R31-14a/b). The existing Round-31
      CHANGELOG bullet still says "the other 11 P2s were filed, not fixed"
      — no longer true for ten of them.
    - **P2-7 — `tests/r31_10_trim_current_thread_api.rs`'s AC1 test asserts
      equality on a process-wide counter across a window its sibling tests
      in the same file can perturb.** The review's claim:
      `SeferAlloc::stats()` is documented process-wide;
      `ac1_trim_empties_pool_and_evicts_large_cache` asserts
      `released_after_cache == released_before` across an alloc+dealloc
      window while libtest runs the file's tests concurrently by default —
      `ac3`'s two threads and `ac4`'s spawned thread(s) can each increment
      `segments_released_total` via their own trims/`AbandonGuard::drop`.
      Low-probability real flake vector, not yet observed. Suggested fix per
      the review: assert a delta computed by the same thread around its own
      trim, or serialise the file's tests.
    - **P2-8 — `ba52822`'s commit subject `fix(examples):` under-declares
      its diff, and the R31-5c lint structurally cannot catch this shape.**
      The review's claim: that commit adds two new `pub unsafe fn` hooks to
      `src/` and edits README's `unsafe` inventory under a subject naming
      only `examples`. `verify-commit-prefixes.mjs`'s direction-2 WARN
      applies only to `bench(...)`/`docs(...)` prefixes; a `fix(...)`
      subject lands in the `'other'` bucket, explicitly out of the lint's
      scope (consistent with R30-12's letter, which governs `perf` commits
      specifically) — but it is the same reader-misleading shape the rule
      exists to prevent for `perf`.
    - **P2-11 — a Round 32 task committed before its own `npm run check`
      finished, and created/removed two scratch commits directly on
      `main`.** The review's claim: `eb6935b` (R31-5c) honestly states in
      its own message that the full test+iai tail of `npm run check` was
      "still completing... at commit time" — a literal deviation from
      CLAUDE.md's "Between phases: run tests and commit" (the tree is
      green now, independently re-confirmed by the review; no harm
      resulted). The same task also created and removed two scratch commits
      (`8eae855`/`3dc528d`) via `git reset --soft` directly on `main`,
      visible only in `git reflog` — nothing was lost and history stayed
      linear, but a shared-workspace round should prefer a scratch branch
      or worktree for that kind of manoeuvre going forward.

    **[P2-6 RESOLVED — 2026-08-02, task #489 ledger housekeeping.]**
    Independently re-verified against `CHANGELOG.md`'s actual Round 31
    section (not just trusted commit `e124a48`'s own message): all of the
    content P2-6 named as missing is now present as CHANGELOG bullets —
    R31-8 (task #472, the same-workload-regime CLAUDE.md rule), R31-5a
    (task #480, `scripts/verify-gate-report.mjs`), R31-5c (task #482,
    `scripts/verify-commit-prefixes.mjs`), R31-5b (task #481, the four
    WARN-level checks + `scripts/capture-measurement-identity.mjs`), R31-11
    (task #475, `tests/ci_clippy_matrix_consistency.rs`), R31-6 (task #469,
    the Windows decommit-crash correctness fix), and both R31-14a/R31-14b
    (tasks #483/#484, the 10 fixed review-P2 repairs) all have their own
    bullets (`CHANGELOG.md` lines 32, 34-40 in the `[Unreleased]` → Round 31
    section as of this check). The stale "the other 11 P2s were filed, not
    fixed" wording P2-6 flagged has its own in-place `**UPDATE (Round 32,
    tasks #483/#484): 10 of these 11 were independently re-verified and
    FIXED**` correction already present in the same bullet (line 31). P2-6
    is RESOLVED; the other four findings in this item (P2-1, P2-7, P2-8,
    P2-11) remain open and unverified — this note closes only P2-6, per
    this file's append-only convention (do not silently drop the other
    four from the bundle).

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

67. **Commit `4a6c77e`'s body describes a test branch that does not exist; the
   text is unrewritable, so it is recorded here instead.** (Filed 2026-08-16,
   task #1035, finding F12 of
   `docs/reviews/2026-08-16-aligned-vmem-r6-wave-review.md`.)

   - **Status:** OPEN as a citation hazard only — no code or doc in the tree is
     wrong. Nothing to fix; this card exists so a future round does not quote
     the commit body as a description of the code.
   - **Current number/verdict:** `4a6c77e`'s message says of the rewritten
     `into_full_parts_preserves_granted_huge`: "A huge-page arm runs when the
     OS actually grants huge pages, with an else arm asserting instance ==
     platform so the test is not vacuous on a runner without a hugetlb pool."
     The test in `crates/aligned-vmem/tests/smoke.rs` has no such branch — it is a flat
     sequence of metadata assertions (`full_parts.granted_huge ==
     original_is_huge`, then field-by-field equality, then a `!reconstructed
     .is_huge()` check). Verified for this card by reading the current test,
     not from the report. The described if/else was presumably planned and
     then not written, or belonged to a different test.
   - **Scope check performed:** the wording exists ONLY in the immutable commit
     body. `grep -niE "huge-page arm|else arm|not vacuous on a runner"` across
     both CHANGELOGs, this index, `crates/aligned-vmem/tests/*.rs` and
     `crates/aligned-vmem/src/*.rs` returns nothing. Git history is not rewritten for
     this — the same non-retroactive posture CLAUDE.md already takes for the
     R30-12 commit-prefix taxonomy ("Explicitly NOT a history rewrite").
   - **Next trigger:** none. Close this card if the cited test is ever rewritten
     to actually have the branch, or if the commit ceases to be referenced.
   - **Evidence:** the immutable commit body itself (`git show -s 4a6c77e` —
     wording re-verified present at task #1060, 2026-08-17); the current
     `into_full_parts_preserves_granted_huge` in
     `crates/aligned-vmem/tests/smoke.rs` (flat assertion sequence, including
     its `#[cfg(feature = "huge-pages")]` Case 2 — a recorded `is_huge()`
     value with a field-equality assert, not the if/else platform arm the
     commit body describes; re-read at task #1060); and the "Scope check
     performed" grep above, re-run at task #1060 RECURSIVELY over the whole
     `crates/aligned-vmem/src/` tree (the original `src/*.rs` glob predates
     the task #1055 split and would now miss `src/api/`, `src/os/`,
     `src/bench_internals/`) — still matching only inside this card's own
     quotation of the commit body.
   Full history: this entry.

68. **CLOSED** by task #1052 (option (c): asymmetry accepted as-is, no rename).
   See "Recently resolved" in RESOLVED.md for the full closure narrative.

78. **[T, record correction — lint half, closed on filing] SEVEN R30-12 commit-prefix mis-slots recorded — six grandfathered in `scripts/verify-commit-prefixes.mjs`, one same-wave inconsistency; history is not rewritten, so the correction lives here (sub-cards 1–3 filed by task #1117; sub-cards 4–5 added by task #1123, which found the task-#1117 card had never been updated when the fourth suppression entry appeared, and that `fb7dac8` — the one commit already in `origin/main`, hence the one with the strongest claim to a durable record — appeared in NO index at all; sub-card 6 added by task #1238 — the first UNPUSHED entry, a heuristic false-positive exemption rather than a genuine mis-slot on landed history, grandfathered with the owner's reword option explicitly left open; sub-card 7 added by task #1335 — a second heuristic false-positive exemption, `e25ec74`, whose only non-comment `src/`-adjacent delta is a `Cargo.toml` `description =` metadata string, not shipping code).** (Filed 2026-08-19; this card records ONLY the lint/record half — the remediation half belongs to a different agent's scope.)

    - **A round-start reader needs to take NO ACTION on this card.** It is a record, not an open defect: no code, test, or script needs to change as a consequence of it. It exists so a future reviewer reading `git log` for the wave is not misled by the six prefixes below.
    - **Status:** CLOSED on filing (record only).
    - **Current-number-or-verdict: six mis-slots, each premise independently re-verified for this card, not taken on the filing task's word:**
      1. **`09f4d16`, `docs(vmem):` — but it changed the public `Display` of `VmemError`.** The only non-doc-comment line in the commit's whole `src/` delta is the `None =>` arm in `crates/aligned-vmem/src/error.rs`, whose string changed from `"OS virtual-memory error (unknown OS error code)"` to a longer message (re-verified: `git show 09f4d16 -- crates/aligned-vmem/src/error.rs` filtered to non-`///` lines shows exactly that one hunk). That is runtime-observable behavior of a public type on a crate about to be published; R30-12's `docs(...)` slot requires "no code changed at all", so the correct slot was `fix(vmem)`. **The repo's own lint caught it and was skimmed past**: `node scripts/verify-commit-prefixes.mjs a61c582..HEAD` printed a direction-2 WARNING naming `crates/aligned-vmem/src/error.rs` among 09f4d16's out-of-scope paths and still ended `[verify-commit-prefixes] PASS (with warnings above)` with exit code 0 (re-run for this card; output quoted in item 21's established format — the warning reads `... 4 changed path(s) fall outside docs/examples/benches/tests/scripts/: crates/aligned-vmem/README.md, crates/aligned-vmem/src/error.rs, crates/aligned-vmem/src/fault_injection.rs, crates/aligned-vmem/src/os/unix.rs — verify no shipping/opt-in behavior actually changed ...`). A direction-2 warning on a `src/` path that IS a real behavior change is exactly what that warning exists to force a human to check.
      2. **`b11d8be`, `fix(perf):` — nothing executable changed.** Re-verified: `git show b11d8be -- crates/aligned-vmem/src/reservation.rs | grep -E '^[+-]' | grep -vE '^[+-]\s*///' | grep -vE '^(\+\+\+|---)'` is EMPTY (exit 1 — no lines). The commit's four files are a doc comment (`reservation.rs`), a Node script fix (`scripts/verify-vmem-page-constant-call-sites.mjs`), an index re-citation, and a test assertion MESSAGE (`tests/large_reserved_capacity.rs`). R30-12's `fix(perf)` slot claims a correctness/consistency fix in shipping or opt-in code; correct slot was `docs(...)` or `test`.
      3. **`66baf1f` vs `2a35bca` — same category, two prefixes, one wave.** Re-verified via `git show --stat`: both commits touch exactly one file under `crates/aligned-vmem/tests/` (`66baf1f` → `tests/lazy_reservation_debug.rs`, `2a35bca` → `tests/granted_huge_reader_enumeration.rs`), yet the first is prefixed `fix(vmem):` and the second `test(vmem):`. Both are test-only corrections and should have carried the same prefix. (Not grandfathered — neither prefix is an R30-12 taxonomy violation on its own.)
      4. **`c766951`, `fix(perf):` — a commit with NO src/ path at all** (`git show c766951 --name-only --format=` = `docs/CORRECTNESS_OPEN_ITEMS.md` + `tests/no_stale_doc_references.rs`, re-verified for this sub-card). `fix(perf)` claims a shipping/opt-in code fix in perf-sensitive code; the correct slot was `bench:` or `docs(...)`. Caught by `verify-commit-prefixes.mjs` itself within an hour of the check landing; the record commit (`c766951`, which created this very card) landed BEFORE the lint commit and the card listed only three mis-slots — this fourth entry was added by task #1123, closing the gap between the suppression list's "Recorded in item 78" reason line and what item 78 actually said.
      5. **`fb7dac8`, `docs(vmem):` — but its `src/` delta includes a changed assert! panic-message string.** Re-verified: `git show fb7dac8 -- crates/vmem/src/lib.rs` filtered to non-`///` changed lines yields exactly two added string-continuation lines inside the `assert!` message ("NOTE: alignment to runtime page_size() is NOT checked …" / "caller must ensure base/reservation are page_size()-aligned …") — runtime-observable panic text under a `docs(...)` prefix, the same defect class as sub-card 1; correct slot was `fix(vmem)`. This is the ONLY grandfathered commit already in `origin/main` (verified: `git merge-base --is-ancestor fb7dac8 origin/main` exits 0), hence the only genuinely un-amendable one, hence the one with the strongest claim to a durable index record — and until this sub-card it appeared in NO index: `git grep -n "c766951\|fb7dac8"` across both indexes and both CHANGELOGs returned nothing, its sole record being the suppression list itself, exactly the "grandfathered without a durable reason" shape the task-#1114 commit body claimed to be correcting. CLAUDE.md's own rule ("When a gate report / commit / review newly flags an open item, add it to the appropriate index in the same commit") is the authority this sub-card belatedly satisfies.
      6. **`2f9d7b9`, `docs:` — but its non-comment `src/` delta is two string-literal continuation lines inside a `compile_error!`.** Re-verified (task #1238, unpushed range): the four out-of-scope paths split cleanly — `api/commit_range.rs`, `api/decommit.rs`, `api/recommit.rs` each carry exactly one `///` line pair (task #1227's citation repairs: "item 6", "item 48"), and `os/unix.rs` carries seven changed lines of which five are `//` comments and exactly TWO are non-comment — the continuation lines of the string literal inside the `#[cfg(all(unix, not(miri), any(target_arch = "mips", "mips64")))] compile_error!`, whose `docs/CORRECTNESS_OPEN_ITEMS.md` citation gained "item 62" and whose line wrap moved. `hasNonCommentChange` cannot see string state (a `--unified=0` diff carries no opening quote), so those two lines read as code and direction 2 ERRORs — the `fb7dac8` class (sub-card 5) through a different macro. This pre-dates the `db248bb` path-quoting fix: the pristine script at `db248bb^`, run from a scratch copy outside the repo, fails IDENTICALLY on the same range (same 12 commits, same 6 warnings, the same single FAILURE naming `2f9d7b9`, exit 1 — verified before any edit was made). Grandfathered by task #1238 rather than teaching the guard string-literal awareness: deciding "this line is inside a string" without parsing Rust means reconstructing quote state a zero-context hunk does not carry — raw strings, escaped quotes, char-vs-lifetime ambiguity, comments that themselves contain quotes — and every failure mode of that reconstruction is a SILENT miss, a real code line classified as prose, i.e. the `09f4d16` defect passing green; "both guards hardened last wave claimed properties they did not have" is task #1126's own commit subject and the most-reproduced guard defect in this repository's history. A loud false FAILURE blocks a push until a human looks — that is the guard working, not failing; note also that the lexing fix is not even locally inert: `fb7dac8`'s two changed lines are pure string continuations too, so it would have un-failed an already-adjudicated record, and the self-cleaning stale-entry check would then have forced that entry's removal as a side effect of a parser change — churn in a record for a fix that touched neither. TWO honest differences from sub-card 5, both recorded: (1) `fb7dac8` had LANDED in `origin/main` (un-amendable); `2f9d7b9` is unpushed and its owner may still reword it by rebase — but no clearly-honest prefix passes: `docs:`/`docs(vmem):` hit this same ERROR, `fix(perf)` (which would pass the lint) would assert a shipping-code fix that did not happen, and sub-cards 1/5's `fix(vmem)` verdict does not transfer cleanly because nothing functional was fixed — a citation inside a deliberately-compile-failing diagnostic was corrected. (2) `fb7dac8`'s changed string was runtime-observable panic text that ships in supported-target binaries; this string exists only on MIPS targets, where the crate deliberately fails to compile — no behavior of any compiling target changed. The exemption is self-cleaning like every other: if the owner rebases/rewords the commit, remove the entry and this sub-card stands as the record.
      7. **`e25ec74`, `docs(numa-shim):` — but the commit touches `crates/numa-shim/Cargo.toml` and `crates/numa-shim/src/lib.rs`, both outside the docs/examples/benches/tests/scripts allowlist.** Re-verified (task #1335, unpushed range): `git show --unified=0 --format= e25ec74 -- crates/numa-shim/Cargo.toml crates/numa-shim/src/lib.rs` shows the ENTIRE `src/lib.rs` delta is `//!`/`///` doc-comment lines (already correctly recognized as comment by `hasNonCommentChange`'s regex) — three doc-comment prose corrections (the crate's "zero C library dependencies" overclaim; two "no syscalls at all"/"pure in-memory" overclaims on `current_node`/`current_node_resolution`, task #1333's own F9/F10-docs fix). The ONE non-comment changed line in the whole commit is `Cargo.toml`'s `description = "..."` key-value line — genuine TOML content (not a `#`-comment, so `hasNonCommentChange`'s regex correctly does not suppress it), but it is crates.io-facing PACKAGE METADATA PROSE, not shipping/opt-in code: the field's value changed from "100% Rust ... no C/C++ libraries" to "... zero third-party C/C++ dependencies ...", the exact same wording correction the doc-comment lines already carry, kept in sync per this crate's own established "keep the three [Cargo.toml/src/lib.rs/README.md] in sync" convention. `docs(numa-shim):` is the honest prefix — no shipping/opt-in behavior changed anywhere in the commit; `Cargo.toml`'s `version` field is untouched. Same defect class as sub-card 6 (`2f9d7b9`): the guard's blind spot is that `key = "prose value"` TOML lines and Rust string-literal continuation lines are BOTH non-comment by the regex's necessarily-dumb definition, yet both can be pure prose with zero code-behavior content — teaching the guard to special-case "Cargo.toml `description` field" would be exactly the kind of guard-cleverness this repository's own most-reproduced guard defect (task #1126: "both guards hardened last wave claimed properties they did not have") warns against, so this is grandfathered rather than the check taught a new exception path. Grandfathered with the same self-cleaning posture as every other entry: if the owner rebases/rewords the commit, remove this entry and sub-card 7 stands as the record.
    - **Next trigger:** none — closed on filing. If the R30-12 taxonomy ever gains a mechanical "same-wave prefix consistency" check, this card is its first regression fixture.
    - **Evidence:** the `git show`/`--stat`/`--name-only` commands quoted in the sub-cards above, re-run 2026-08-19 on branch `vmem-p4-l` for sub-cards 4–5 (sub-cards 1–3 re-run on `vmem-p3-j` at filing); `git grep -n "c766951\|fb7dac8"` returning nothing (pre-this-card); `git merge-base --is-ancestor fb7dac8 origin/main`; `node scripts/verify-commit-prefixes.mjs a61c582..HEAD` (PASS with 3 direction-2 warnings, exit 0); CLAUDE.md's R30-12 rule ("Active rules" section). Sub-card 6 (task #1238, 2026-08-20, unpushed range): the per-file filter `git show 2f9d7b9 --unified=0 --format= -- crates/aligned-vmem/src/` piped through the other sub-cards' comment filter (three files one `///` pair each, `os/unix.rs` five `//` lines + exactly two string-continuation lines); and the pristine-script check — `git show db248bb^:scripts/verify-commit-prefixes.mjs` into a scratch file outside the repo (with a stub `lib.mjs` pinning `REPO_ROOT` to this checkout; the script itself byte-identical) — run on the same `@{u}..HEAD` range, printing the identical 12-commits/6-warnings/1-FAILURE output with exit 1. Sub-card 7 (task #1335, 2026-08-25, unpushed range): `git show --unified=0 --format= e25ec74 -- crates/numa-shim/Cargo.toml crates/numa-shim/src/lib.rs`, personally re-read line by line — every `src/lib.rs` hunk starts with `//!`/`///`; the sole non-comment hunk is `Cargo.toml`'s `description =` line.

79. **[T, record correction — closed on filing] Commit `2cedbbb`'s body (task #1113, the `&mut self` breaking change) understated the cost and justified it with a non-sequitur; the DECISION stands, the stated JUSTIFICATION was wrong and the count was off by one.** (Filed 2026-08-19, task #1123; every premise below re-verified on branch `vmem-p4-l`, not taken on the review's word.)

    - **A round-start reader needs to take NO ACTION on this card.** The `&mut self` signatures are correct and stay; this card corrects only the record, per CLAUDE.md's non-retroactive posture — the commit is not amended.
    - **Status:** CLOSED on filing (record only).
    - **Current-number-or-verdict, three corrections to one commit body:**
      1. **The `Send`/`Sync` justification is a NON-SEQUITUR.** The body says: "`Reservation` is `Send` but not `Sync`, so `&self` mutators never enabled a sharing pattern that `&mut self` takes away." `Sync` governs CROSS-THREAD sharing only; the entire cost of the change is SINGLE-THREAD aliasing, which `Sync` says nothing about. Re-verified by compiling a three-pattern probe against HEAD (all three give `E0596`/`E0596`-class errors; at `2cedbbb^` the same three compiled, since `git show 2cedbbb^:crates/aligned-vmem/src/reservation.rs` shows all seven mutators took `&self` then): (a) a struct owning a `Reservation` exposing a `&self` method that mutates it; (b) `Rc<Reservation>` calling a mutator through the shared handle (`cannot borrow as mutable … DerefMut is required`); (c) a `Fn` (not `FnMut`) closure capturing by shared borrow (`cannot borrow *b as mutable, as it is behind a & reference`). None of the three is cross-thread; none is exotic — the `Rc` and `Fn`-closure shapes are ordinary refactoring dead-ends the old signatures permitted. The factual half of the sentence is true (`unsafe impl Send for Reservation` exists, no `Sync` impl — re-verified at `reservation.rs`), but it does not support the conclusion drawn from it, and the real cost (three legitimate single-threaded patterns) went unstated.
      2. **"24 test bindings" is actually 25.** The body says "24" twice ("all fixed by adding `mut` to 24 receiver bindings" / "Seven signature lines and 24 bindings"). Re-verified by pairing every added line against its removed counterpart with `mut` removed (`decommit_capability.rs` 2, `lazy_commit.rs` 1, `reservation_decommit_contract.rs` 9, `round_trip_contract.rs` 1, `smoke.rs` 12 — total **25**).
      3. **The other counts: two CONFIRMED, one WRONG in the commit body — and this sub-card's own first draft was a worse error than the one it corrected.** "Exactly TWO method-form call sites in `src/`" — CONFIRMED (both in `crates/aligned-vmem/src/lazy_reservation.rs`; `reservation.rs` itself has zero). "the root crate, `numa-shim`, `examples/` and `benches/` have ZERO" — CONFIRMED (`git show 2cedbbb --name-only` touches nothing outside `crates/aligned-vmem`). "59 test call sites across 5 files" — WRONG, and the correction history here is itself the lesson. The agent that filed this card counted **56 call sites** (per file: `decommit_capability` 6 / `lazy_commit` 1 / `reservation_decommit_contract` 31 / `round_trip_contract` 2 / `smoke` 16) and honestly flagged the 59-vs-56 gap as counting-method-dependent rather than declaring the commit body wrong. That doubt was CORRECT in both method and result — and it was overruled at merge time by text asserting 59 was "CONFIRMED, twice, independently" and explaining the 56 as a receiver-shaped pattern (`\w+\.method\(`) missing non-identifier receivers. That explanation was FABRICATED — asserted without ever running it. Re-measured at `2cedbbb` (task #1125): `git grep -c` over the five test files yields 6/2/33/2/16 = 59 because it counts matching LINES, and three of those lines are comments, not calls — `lazy_commit.rs:686` (`// is that \`r.decommit(...)\` above did NOT panic — that proves the`), `reservation_decommit_contract.rs:260` (`/// M2 (task #1084): \`r.decommit(1, 1)\` — EMPTY, IN bounds, MISALIGNED — must`), and `reservation_decommit_contract.rs:298` (`/// early-return sat ahead of all validation, so \`r.try_decommit(1, 3)\` on a`). Excluding comment lines: 6/1/31/2/16 = **56 call sites; 59 lines mentioning a call**. The fabricated receiver-pattern story is refuted by its own instrument: the blamed pattern `\w+\.(decommit|...)\(` actually yields **59** matches, not 56 — the pattern the card blamed for the LOW number produces the HIGH number, so it cannot be the cause; nothing about receiver shape was ever in play. The deeper failure this card must preserve: "twice, independently" was worthless, because both counts — the @oh reviewer's and the orchestrator's merge-time re-run — used the SAME line-counting instrument (`git grep -c`, matching lines). Re-running the same tool is not a second opinion; it is the same artifact repeated, and here it overruled a correct doubt. `2cedbbb`'s commit body is left as-is per CLAUDE.md R30-12 (non-retroactive; and no git history writes were made for this correction).

         git grep -c -E "\.(decommit|try_decommit|decommit_lazy|recommit|try_recommit|commit_range|try_commit_range)\(" 2cedbbb -- crates/aligned-vmem/tests
         decommit_capability 6 · lazy_commit 2 · reservation_decommit_contract 33 · round_trip_contract 2 · smoke 16  =  59 LINES
         minus the three comment lines above  =  56 CALL SITES
    - **Next trigger:** none — closed on filing. If the reviewer's original probe scripts are ever committed as fixtures, sub-card 1's three patterns are the regression set.
    - **Evidence:** `git show 2cedbbb^:crates/aligned-vmem/src/reservation.rs | grep 'pub fn decommit('` (→ `&self`); the three-pattern probe compiled at HEAD (transient, not committed — patterns quoted in sub-card 1 verbatim so anyone can rebuild it); `git show 2cedbbb --numstat` (the five test files); the per-file `mut`-addition pairing census; `git show 2cedbbb --name-only --format=`.

81. **[T, record correction — closed on filing] Commit `64aa491`'s body (task #1130, F3+F5+F6) says "the flag [is] set at the five prefix-failure push sites" in `scripts/verify-commit-prefixes.mjs`; there are FOUR.** (Filed 2026-08-19 as task #1133/F5; re-verified by grep by task #1131 before this card was written.)

    - **A round-start reader needs to take NO ACTION on this card.** The script's BEHAVIOUR is right — only the count in the commit body is wrong; `64aa491` stays unamended per CLAUDE.md R30-12's non-retroactive posture (no history rewrites for record corrections).
    - **Status:** CLOSED on filing (record only).
    - **Current-number-or-verdict: FOUR `failures.push` sites carry `taxonomy: true` — lines 509, 528, 545, 569** (re-verified: `rg -n 'failures\.push' scripts/verify-commit-prefixes.mjs` yields six matches; of the five literal object sites, the fifth — line ~647 — is the stale-GRANDFATHERED-entry failure and correctly carries NO `taxonomy: true` flag, because it is not a commit-prefix-classification failure; line 653 spreads pre-built `structuralFailures` and is not a literal site). The `failures.some(f => f.taxonomy)` gating that commit added keys on exactly those four sites, so its described behaviour ("a run whose failures are all structural gets a relevant message") holds with four sites exactly as it would with five — the count in the body was prose, not load-bearing.
    - **Next trigger:** none — closed on filing.
    - **Evidence:** `rg -n 'failures\.push' scripts/verify-commit-prefixes.mjs` → lines 509/528/545/569/647/653; the surrounding lines of each of the five literal sites read for the `taxonomy: true` flag (present only at 509/528/545/569); commit `64aa491` body, line "with the flag set at the five prefix-failure push sites."

83. **[T, LOW (F3) + INFO (F4) — record correction, closed on filing] Commit `db9444f`'s body (task #1133) claims the re-wrapped CHANGELOG entry has "zero flagged lines (was four)" — true only under a carve-out the body never names — and justifies the one short line it introduced with a wrong figure, in a commit whose entire subject was that a previous commit's figures were wrong.** (Filed 2026-08-19, task #1136 F3+F4; both parts re-measured mechanically at `64aa491` and `db9444f`, greedy fill to 78 with backtick code spans atomic, first-atom = the next line's first whitespace-delimited token with spans kept whole including gluing punctuation.)

    - **Update (task #1138/F5+F6, 2026-08-19): this card's OWN two figures needed correction — the entry's line range was off by one (`:111`-`:132` stated, `:111`-`:133` true, matching this card's own "23-line entry" count), and the F4 replacement figure for the code span (48) was itself wrong (46 — see the rewritten F4 sub-point below for the three-generation history: 52 → 48 → 46).** A round-start reader still needs to take NO ACTION — both corrections are prose/count-only, the published CHANGELOG text is unaffected, and this card stays CLOSED on filing per its original scope; the corrections are folded into the sub-points below rather than left as a separate stale layer, since this card is itself a record of exactly this defect class and a reader should not have to cross-reference a later item to get the current numbers.
    - **A round-start reader needs to take NO ACTION on this card.** The published CHANGELOG text itself is correct and render-byte-identical across the rewrap (independently re-verified below); only the commit body's self-description is wrong. `db9444f` stays unamended per CLAUDE.md R30-12's non-retroactive posture — this card is the record.
    - **Status:** CLOSED on filing (record only).
    - **Current-number-or-verdict — two corrections to one commit body:**
      1. **F3: "zero flagged lines (was four)" rests on an unnamed carve-out.** Pure greedy at `64aa491` (PRE) flags FIVE lines, not four: `:113` len 73 (next atom `is`, 2, would fit at 76), `:114` len 70 (`"Fixed"`, 7, fits at 78), `:115` len 71 (`(task`, 5, fits at 77), `:116` len 38 (`independent`, 11, fits at 50), and `:123` len 53 (`**Migration:**`, 14, fits at 68). The four lines the body names — `:113`-`:116`, with exactly those would-fit widths — are enumerated correctly; what is missing is the fifth. `:123` is flagged in BOTH states (PRE and HEAD carry the identical line `  (` + the `error[E0596]` code span + `).` followed by `  **Migration:** bind …`), and it is the only line between HEAD's true count of one and the claimed zero. Both of the body's figures — "was four" (PRE) and "zero" (HEAD) — are reachable only by silently treating `:123` as a terminal for its sentence group, an exception the body never states. Applied consistently to both sides, the carve-out makes the DELTA honest (4 → 0); left unstated, it makes the ABSOLUTE claim ("zero flagged lines") false, and `:123` remains 15 columns short of greedy at HEAD (53 + 1 + 14 = 68 ≤ 78). The carve-out is not a rendering boundary: the entire 23-line entry (`:111`-`:133`) is ONE Markdown paragraph — no blank line anywhere inside it, including between `:123` and `:124` — which `db9444f`'s own byte-identity check demonstrates by joining all 23 lines. A sentence-group terminal is a defensible wrap style; a metric with an unnamed style exception is not the metric the prose describes.
      2. **F4: the "52-char code span" figure — corrected once to 48 by task #1136, and that correction was ALSO wrong; the true figure is 46. Three generations of one defect are recorded here.** The original `db9444f` body says `:122=31` "IS maximal: the next atomic token is a 52-char code span that fits in no 78-window behind that much prose" (generation 1: **52**). Task #1136 corrected this card to say the code span is 48 chars (generation 2: **48**) — but 48 is the length of the span WITH its backtick delimiters included (`` `error[E0596]: cannot borrow *shared as mutable` ``, backtick-to-backtick inclusive, measured 48 chars), not the code text *between* the backticks, which is what "the code span" denotes in ordinary usage and what this card's own words ("the code SPAN itself, ... between its backticks") say it is measuring. Measured directly (generation 3, this correction): `:123` raw length 53; the full glued ATOM (`(` + backtick-delimited span + backtick + `).`) is 51 chars; the span WITH its two backticks is 48 chars; the span text WITHOUT the backticks — `error[E0596]: cannot borrow *shared as mutable`, the actual code quoted — is **46** chars (`echo -n 'error[E0596]: cannot borrow *shared as mutable' | wc -c` → 46). So three figures coexist for three different substrings of the same line, and none of them is 52: the atom is 51, the delimited span (backticks included) is 48, the code text alone (backticks excluded, what "code span... between its backticks" describes) is 46. The CONCLUSION is unaffected by any of these three generations: `:122` len 31, and even under the smallest of the three candidate widths (46 + 2 backticks + 1 leading space + 1 trailing `).` needs the full 51-char atom to actually wrap), 31 + 1 + 51 = 83 > 78, so `:122` is genuinely maximal and splitting the code span was rightly refused. The three-generation history is the more instructive fact than any single figure: `db9444f` (task #1133) introduced 52 while correcting a PRIOR commit's wrong counts; task #1136 corrected 52 → 48 while filing THIS card, whose own subject is "figures wrong in a commit about figures being wrong" — and got its own replacement figure wrong, by conflating "the span between the backticks" (what its prose said) with "the span including the backticks" (what its arithmetic actually measured); this card's third pass corrects 48 → 46 and keeps the figure and its gloss consistent with each other. A defect that survives being corrected twice, each correction landing in a commit whose subject is correcting the same class of error, is not a fluke of one commit — it is evidence that measuring a quoted code span's length by eye, without a `wc -c`/`len()` check committed alongside the prose, reproduces the mistake it is trying to fix.
    - **Next trigger:** none — closed on filing. If the 78-column wrap rule is ever mechanized into a guard script, the sentence-group-terminal exception must be an explicitly named rule (not an implicit judgement), and `:123` of this entry is its first regression fixture; `:122` is the counter-fixture proving genuine maximality is still detectable.
    - **Evidence:** line-length/first-atom re-derivation at both revisions (`git show 64aa491:crates/aligned-vmem/CHANGELOG.md` and HEAD, entry at `:111`-`:133` in both — NOT `:111`-`:132`; `:133` is `  free \`pub unsafe fn\` API, which is unaffected.` and `:134` opens the next bullet with `- `, confirmed by direct line read at both revisions; this off-by-one, in a card about off-by-one errors, is corrected here by task #1137/F5) — PRE flagged `[113, 114, 115, 116, 123]`, HEAD flagged `[123]`, HEAD with `:123` excluded `[ ]`; `git diff 64aa491..db9444f -- crates/aligned-vmem/CHANGELOG.md` (the 10-line reflow hunk); whitespace-normalised sha256 of the 23-line entry (the correct `:111`-`:133` range — the cited hash reproduces ONLY over this range, not over `:111`-`:132`) = `9bedbba57ff1179917e97930d711b6db2c668dd3500f324dde933c567731279e` at BOTH revisions, matching the prefix `db9444f`'s own body quotes (byte-identity independently reproduced); HEAD `:121`-`:124` read directly (no blank line between `:123` and `:124`); commit `db9444f` body ("Post-fix the entry has zero flagged lines (was four)"; "the next atomic token is a 52-char code span").

86. **[T, process/indexing-hygiene — decision recorded, deliberately deferred, THEN REVERSED] Both `docs/CORRECTNESS_OPEN_ITEMS.md` (2,423 lines) and `docs/perf/OPEN_ITEMS.md` (2,393 lines) are again past the ~1,000-line threshold CLAUDE.md's R34-24 rule flags for another archive split — this task explicitly decided NOT to perform that split then, and recorded why; that decision was itself reversed the next day.** (Filed 2026-08-19, task #1143. Reversed 2026-08-20, task #1217.)

    - **Status:** CLOSED for the correctness half — REVERSED by the owner and executed, task #1217, 2026-08-20. `docs/CORRECTNESS_OPEN_ITEMS.md` (then 2,555 lines, having grown further overnight) was split into a folder, `docs/correctness-open-items/` (`ACTIVE.md` for `[A]`, `TRACKED.md` for `[T]` — where this card then lived; that file was replaced the same day, first by #1221's four item-number-range files and then by #1222's nine thematic per-subject files (neither `TRACKED.md` nor the number-range files exist any longer), and this card now lives among the process-record cards — `RESOLVED.md` for the closure-trail pointers, `ARCHIVE.md` moved from the old `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`), with `docs/CORRECTNESS_OPEN_ITEMS.md` itself kept as a thin index/table-of-contents so the code/CI/script citations of that exact path (plus every doc citation of "item N") keep resolving unchanged — the count of citing files is deliberately not typed here (task #1217's commit body typed "42", and that figure was already wrong at the split it described; the reproducible census is the one the thin index itself carries: `git grep -l "docs/CORRECTNESS_OPEN_ITEMS\.md" -- ':!docs/' | wc -l`). The `docs/perf/OPEN_ITEMS.md` half of this card's original scope was NOT touched by task #1217 (out of that task's declared scope, exactly as this card's original text below explains) and remains OPEN under the original deferral reasoning.
    - **Why the reversal, and why so soon:** the owner explicitly asked for the split (task #1217's brief opens "The owner wants `docs/CORRECTNESS_OPEN_ITEMS.md` turned from ONE FILE into A FOLDER" and states this reverses item 86 by name) — an owner is entitled to revisit their own deferral once new information (continued growth past 2,555 lines, and this campaign's own repeated invocation of item 86's original deferral as a precedent for NOT splitting) changes the balance of the three original reasons. Reassessed against those three original reasons at reversal time: (1) **Scope** — task #1217's declared scope covers exactly the correctness-index files (`docs/CORRECTNESS_OPEN_ITEMS.md`, the new `docs/correctness-open-items/` folder, the retired `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`, and the doc-parsing tests in `tests/no_stale_doc_references.rs`) — `docs/perf/OPEN_ITEMS.md` stays out of scope, so the original scope objection is honored, not overridden, for the perf half. (2) **Demonstrated risk** — task #1109's original truncation defect (9 pointers mid-heading, 19/32 verdicts lost) is the reason task #1217 treats this as a "not mechanical, needs a dedicated careful pass" job: full per-card enumeration before cutting, byte-preserving `sed` line-range slices (verified boundary-clean before any file was written), and a mandatory counterfactual re-run of the self-check test (see the Verification section of task #1217's own report) — the exact discipline item 86's own point (2) said a rushed multi-point task would lack. (3) **No structural urgency** — still true in the narrow sense (a long file misleads no one about status), but the owner's stated preference is that round-start reading speed is itself worth optimizing now, given the index has grown a further ~130 lines since this card was filed with no sign of the growth stopping.
    - **Current-number-or-verdict:** `docs/CORRECTNESS_OPEN_ITEMS.md` = 2,423 lines at filing (task #1143), 2,555 lines at reversal (task #1217, this worktree's copy — the shared checkout had already grown to 2,566 with one additional item, 93, landed by a concurrent task; see task #1217's own report for that discrepancy). Split into `docs/correctness-open-items/`, whose CURRENT shape is `ACTIVE.md`, nine thematic `TRACKED_*.md` files, `RESOLVED.md` and `ARCHIVE.md`. Task #1217's own four-file shape (`{ACTIVE,TRACKED,RESOLVED,ARCHIVE}.md`) survived only hours — #1221 replaced `TRACKED.md` with four item-number-range files and #1222 replaced those with the nine thematic ones — and is recorded as the dated history it is in the Status bullet above. Stated as the CURRENT shape here because this bullet is labelled `Current-number-or-verdict`, and R34-24 requires that field to read as current state; task #1230 found it still naming a layout that had stopped existing on the same day it was written.
    - **Next trigger:** a future round may perform the analogous split for `docs/perf/OPEN_ITEMS.md` — that half of this card's ORIGINAL deferral still applies unchanged (see the un-edited three-reasons paragraph immediately below, preserved for its own historical record), and per this card's own point (2) above, must first write a `docs/perf/OPEN_ITEMS.md`-scoped pointer-resolution self-check test before or in the same commit as any such split, since none exists today.
    - **Evidence (reversal):** task #1217's brief and report (the split's card/directory census, the `tests/no_stale_doc_references.rs` updates pointing at the new file paths, and the counterfactual re-run showing the pointer-resolution test still fails correctly on a deliberately corrupted number).
    - **Original card text, preserved verbatim below this line for the historical record (task #1143, 2026-08-19) — the "Why this task defers" reasoning it contains is quoted, not re-derived, by the reversal note above:**
    - **Current-number-or-verdict (original, task #1143):** `docs/CORRECTNESS_OPEN_ITEMS.md` = 2,423 lines (already has a sibling archive, `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`, 1,512 lines, from the task #1109 split); `docs/perf/OPEN_ITEMS.md` = 2,393 lines (already has a sibling archive, `docs/perf/OPEN_ITEMS_ARCHIVE.md`, from the R29-6 split). Both mains are roughly 2.4× the ~1,000-line threshold. Re-splitting either main file further (moving more current-tier card bodies into their archives, the same mechanism R34-24 describes) would reduce both back toward the threshold.
    - **Why this task defers rather than performs the split (original, task #1143):** three independent reasons, each sufficient on its own. (1) **Scope:** this task's declared edit scope is exactly three files (`docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`, `docs/perf/OPEN_ITEMS.md`) — a correct split of `docs/perf/OPEN_ITEMS.md` requires writing INTO `docs/perf/OPEN_ITEMS_ARCHIVE.md`, which is outside that scope (this task already reverted one such edit made in the course of fixing item 32 above, once the scope constraint was reconsidered — see that item's history for the reverted `§ D32` archive section). (2) **Demonstrated risk:** this campaign's own history shows a mechanical index split is NOT a safe, routine operation — task #1109's R34-24 split of the correctness index truncated 9 "Recently resolved" pointers mid-heading and lost the verdict on 19 of 32 cards, discovered only later by task #1116 and fully corrected by task #1123's stronger self-checking test (`tests/no_stale_doc_references.rs::correctness_index_recently_resolved_pointers_carry_verdicts`, which this task re-ran clean — see the Verification section of this task's own report). A rushed re-split, folded into a 9-point remediation task alongside eight unrelated corrections, is exactly the condition (time pressure, divided attention, no dedicated review pass) that produced the original defect. (3) **No structural urgency:** unlike a broken pointer or a duplicate item number (both fixed elsewhere in this same task), file length alone does not silently mislead a reader the way those defects do — a 2,400-line file is slower to read end-to-end (the cost R34-24's own rule is written against) but does not misstate any item's status. The cost of deferring one more round is "round-start reads stay slower than ideal," not "a reader is told something false."
    - **Next trigger (original, task #1143):** a future round performs the split as ITS OWN dedicated task (not folded into a multi-point remediation), following the established R29-6/R34-24 mechanism (move full card bodies to the sibling archive, leave a one-line current-state pointer in the main file) — and, per the lesson task #1109 → #1116 → #1123 already taught this campaign, must re-run (or write, for `docs/perf/OPEN_ITEMS.md`, which currently has no equivalent) a pointer-resolution self-check test IMMEDIATELY after the split, in the same commit, rather than trusting the mechanical edit was correct. `docs/perf/OPEN_ITEMS.md` has no `no_stale_doc_references.rs`-style self-check today (confirmed: that test only opens `docs/CORRECTNESS_OPEN_ITEMS.md`/`docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`) — a perf-index split without first adding an equivalent check would repeat task #1109's mistake with no safety net at all, a strictly worse position than the correctness index was in.
    - **Evidence (original, task #1143):** `wc -l docs/CORRECTNESS_OPEN_ITEMS.md docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md docs/perf/OPEN_ITEMS.md` (this task, 2026-08-19); CLAUDE.md's R34-24 "Phased delivery" bullet (the split rule and its ~1,000-line trigger); task #1109's split commit and task #1123's correction (referenced throughout this index's other cards, e.g. items 81-83); `tests/no_stale_doc_references.rs::correctness_index_recently_resolved_pointers_carry_verdicts` (the self-check that exists for the correctness index only — now re-pointed at `docs/correctness-open-items/RESOLVED.md` by task #1217, see that task's Evidence above).

89. **[T, "filed as a follow-up" asserted an index write that never happened] Task #1137's recommendation — ban bare step numbers in `scripts/check-all.mjs` in-body cross-references, converting them to symbol/step-name references while keeping the header's own numbered list — was "recorded but NOT implemented" at filing (commit `ce6ed88`) and then re-described as already "filed as a follow-up" by commit `740abe6` and re-deferred again ("Not implemented here; it remains the orchestrator's call") by commit `4356e23`, but no card for it exists anywhere in this index, `docs/perf/OPEN_ITEMS.md`, or `docs/perf/OPEN_ITEMS_ARCHIVE.md`.** (Filed 2026-08-19, task #1155.)

    - **Status:** OPEN — genuinely never filed until now, confirmed by direct search, not merely mislaid.
    - **Current-verdict:** searched both indexes and both archives (`docs/perf/OPEN_ITEMS_ARCHIVE.md`; `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`) for `bare step|step-number|step numbering|numbered step|decay` (case-insensitive) — every hit in `docs/CORRECTNESS_OPEN_ITEMS.md` and `docs/perf/OPEN_ITEMS.md`/its archive is either an unrelated `maybe_decay_large_cache` cache-decay item or the ALREADY-CLOSED, textually-similar-but-distinct item about `check-all.mjs`'s HEADER numbering (`docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md:1493-1495`, a different defect: the step-count summary in the file's header comment being out of order, not in-body cross-references decaying). `git show ce6ed88 --stat` confirms task #1137's own landing commit touched only `.github/workflows/ci.yml` and `scripts/check-all.mjs` — no index file. `git show 740abe6 --stat` and `git show 4356e23 --stat` (the two commits whose bodies say "filed as a follow-up" / "remains the orchestrator's call") confirm neither touches `docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/perf/OPEN_ITEMS.md`, or either archive for this topic either — `740abe6` touches no index at all; `4356e23`'s only index edit is item 87 above (a different card, the sentinel-guard scope record). So "filed" in both commit bodies asserted an index write that never occurred, in a wave otherwise specifically about closing exactly this kind of gap (R22-3).
    - **Next trigger (task #1137's own recommendation, PARTIAL-adoption form, quoted from `ce6ed88`'s commit body):** convert `scripts/check-all.mjs`'s in-body numeric cross-references (e.g. "step 43", "from 39") to symbol/step-name references (e.g. "the forced-page decomp-hook row" instead of "step 44") — these are what actually decay on every insertion, confirmed three times in the file's own history (39→42→43 across tasks #1131/#1142). Keep the header's own numbered list as-is — it is the thing being referenced, not a stale reference itself, and its counts are independently cross-checked by the runtime banner deriving from `steps.length`. This is a repo-wide style decision task #1137's own brief deliberately reserved ("that is a repo-wide decision, and the brief reserved it") rather than one this card's filing is authorized to make unilaterally — the trigger for a future round is to make that decision and, if adopted, do the conversion in one pass across the file rather than piecemeal per future insertion (piecemeal is exactly how the current three-generations-of-stale-number history accumulated).
    - **Why this index, not `docs/perf/OPEN_ITEMS.md`:** `docs/perf/OPEN_ITEMS.md`'s own `## Scope` section states it "covers `docs/perf/*.md` only (gate reports + perf design docs)" and explicitly excludes "code `TODO`/`FIXME` comments, roadmap wishes" unless a perf gate report flags them. This item is about documentation-reference decay in a build/CI script (`scripts/check-all.mjs`), flagged in commit message bodies (`ce6ed88`, `740abe6`, `4356e23`) — not in any `docs/perf/*.md` gate report. `docs/CORRECTNESS_OPEN_ITEMS.md`'s own scope is "correctness bugs, flaky tests, and CI-coverage gaps flagged from ANY source (commit messages, code comments, reviews)" — this is precisely a CI/build-tooling correctness (documentation-accuracy) concern flagged in commit messages, matching this index's scope exactly.
    - **Evidence:** `git show ce6ed88` (task #1137, origin of the recommendation and its "recorded but NOT implemented" disposition); `git show 740abe6` (task #1142, "filed as a follow-up rather than done here"); `git show 4356e23` (task #1150, "Not implemented here; it remains the orchestrator's call"); `git log 4356e23..HEAD -- docs/CORRECTNESS_OPEN_ITEMS.md` (empty, confirming no later commit filed it either); `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md:1493-1495` (the distinct, already-closed header-numbering defect, confirmed not a duplicate); `docs/perf/OPEN_ITEMS.md:51-59` (the `## Scope` text this card's placement is justified against).
