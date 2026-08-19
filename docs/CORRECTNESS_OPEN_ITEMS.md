# Correctness / CI-debt open items — cross-round tracking index

**Purpose.** A single durable, session-surviving checklist of correctness,
flakiness, and CI-coverage-gap items that a commit message, code comment, or
review doc has flagged as *open / follow-up / "left for later"* — the sibling
to `docs/perf/OPEN_ITEMS.md`, which durably tracks the analogous class of item
but ONLY for `docs/perf/*.md` gate reports and perf design docs (see that
file's own `## Scope`). This file exists because R19-1 (task #337, commit
`46ea2db`)'s own commit message flagged TWO follow-ups — a flaky test and a
clippy dead-code combo — that then existed NOWHERE durable: not in
`OPEN_ITEMS.md` (out of its scope by design — it is not a perf gate report),
not in `CHANGELOG.md`, not anywhere else. Two independent reviews
(`docs/reviews/2026-07-26-crush-review-r19-r21.md` §4 P2 and
`docs/reviews/2026-07-26-oh-review-r19-r21.md` §4.1) both independently
rediscovered this gap, and the flaky item was then independently reproduced
TWICE MORE in Round 22 itself (once during task #352's CI verification, once
during task #356's test run) before this file existed to catch it. This file
is the fix: option (b) from both reviews (a sibling index), not a widening of
`OPEN_ITEMS.md`'s own scope — that file's perf-only narrowness is a deliberate,
working design choice for its own domain and stays intact.

**Scope.** This index covers correctness bugs, flaky tests, and CI-coverage
gaps that originate from ANY source — commit message follow-up notes, code
comments (`TODO`/`FIXME`), or review-doc findings — not just
`docs/perf/*.md` reports. It is the correctness/CI-debt counterpart to
`docs/perf/OPEN_ITEMS.md`, which stays scoped to perf gate reports and perf
design docs only; see that file's own `## Scope` for the boundary and its
cross-link back to this file. When in doubt which index an item belongs in:
if it is about wall-clock/Ir/memory numbers or a perf design's
CONDITIONAL-GO trigger, it belongs in `docs/perf/OPEN_ITEMS.md`; if it is
about a test that can fail spuriously, a lint/build combo that is not
clean, or a correctness contract, it belongs here.

**Convention (mandatory — see CLAUDE.md "Phased delivery").**

1. **Round start:** before forming a new round's task queue, read this file
   end-to-end (alongside `docs/perf/OPEN_ITEMS.md`) and decide, for each open
   item, whether this round closes it, defers it (with a one-line reason
   appended), or leaves it. An item must not be silently ignored — every
   round either moves it or explicitly re-defers it.
2. **When you close an item:** move its entry to §"Recently resolved" with
   the closing round + task number + one-line evidence (commit / doc that
   records the resolution). Do NOT delete the entry — the closure trail is
   itself the artifact that lets a future reviewer confirm an item was
   actually addressed, not just forgotten again.
3. **When a new commit, comment, or review flags a correctness/CI-debt
   follow-up:** add it here in the same commit (or an immediate follow-up
   commit), with a citation back to its origin (commit SHA / file:line). A
   flag that lives only inside a single commit message body or code comment
   is exactly the failure mode this index exists to prevent.

**Tier key.** **[A]** active — a real next step a round should consider
taking. **[T]** tracked-not-actioned — genuinely reproduced/confirmed but
intentionally not yet scheduled for a fix (root-cause investigation or a
scoping decision is the pending step, not implementation).

---

## Open items

### [A] Active — next steps for an in-progress or imminent round

1. **Recurring process gap: every review-campaign round must get a CHANGELOG.md entry before the round is considered closed.** This has recurred **nine** times across the aligned-vmem campaign alone (rounds 1-9; see the Current-number bullet for the per-round breakdown and which were caught within their own round). The first three recurrences, in detail:
   - Round 1 (tasks #842-850, closed by task #857/commit `7663811`): W16 flagged the missing CHANGELOG entry — the round's own CHANGELOG text said "that follow-up round (tasks #851-857) is tracked separately and will get its own CHANGELOG entry once complete," but the entry itself was never written. F11 (task #863) documented the recurrence: "The #851–#857 round has no CHANGELOG entry. `CHANGELOG.md:275` states it explicitly... This is W16's finding recurring one round later; nothing in `docs/perf/OPEN_ITEMS.md` or `docs/CORRECTNESS_OPEN_ITEMS.md` tracks it, so a fresh session inherits no memory of it."
   - Round 2 (tasks #851-857): The exact same gap reproduced immediately after F11 was filed. Task #863 eventually closed it by writing the entry, but only because a round-3 review (F11) caught it again.
   - Round 3 (tasks #858-864): The gap reproduced a THIRD time — this index entry exists BECAUSE the task description that spawned it explicitly flags it: "Round 3 (tasks #858-864, the previous round of this same aligned-vmem review campaign) has NO CHANGELOG.md entry — this is the THIRD consecutive round with this exact gap: W16 flagged it for the #842-850 campaign (round 1) and it was eventually closed; F11 flagged it again for #851-857 (round 2) and closed it (CHANGELOG.md:304-312, written by task #863); #858-864 (round 3) reproduced the SAME gap immediately, and nobody caught it until this round-4 review."

   **The gap is a process hygiene failure, not a correctness defect:** no code is broken, but a historical record that CLAUDE.md's own "Round start: check BOTH open-items indexes" rule depends on for cross-round continuity is missing. The fix is a standing rule, not a one-off entry.

   **Proposed standing rule addition to CLAUDE.md under "Phased delivery":** add a new bullet after the existing "Every phase is delivered with tests" / "Between phases: run tests and commit" / "After each phase — ZERO-TRUST review" sequence: **"Every round that lands ≥ 1 commit and is closed by a review must have a CHANGELOG.md entry written in the same closing task (or an immediate follow-up commit) before the round is considered complete. The entry must cite real, verified commit SHAs from `git log` and describe what actually shipped — do not defer it to a later round. This is the same discipline that prevents the recurring 'missing CHANGELOG' gap that occurred three times across the aligned-vmem review campaign (tasks #842-850 / #851-857 / #858-864), where each round's own closing text said 'a follow-up CHANGELOG entry is owed' but the entry was never written until the NEXT round's review caught the gap again."**

   If this rule is accepted, this index entry moves to "Recently resolved" with the closing citation being the CLAUDE.md commit that added the bullet. If rejected, this entry stays open and serves as the recurring reminder the existing "Round start: check BOTH open-items indexes" rule assumes exists but does not.

   - **Status:** OPEN — awaiting human decision on whether to add the standing rule to CLAUDE.md. Rounds 4 and 5 had the entry written in the round's own closing pass (task #872, task #879); rounds 6, 7, 8 and 9 did NOT — none of those rounds' own remediation tasks wrote a CHANGELOG section, and all four were caught only by the dedicated CLOSING review (SC3, TC1, UC1, V2C6), one step later than rounds 4/5. The underlying gap (the entry is not written by the task that should own it) still occurs every round; the catch-and-close-within-round mechanism held for two rounds and then did not, four times in a row.
   - **Current number:** 3 confirmed recurrences that went uncaught until the NEXT round (aligned-vmem rounds 1, 2, and 3); round 4 (tasks #867-874, caught by CR10) and round 5 (tasks #875-879, caught by QC9) are a 4th and 5th instance, both caught and closed within their own round; round 6 (tasks #880-886) is a 6th instance, caught by SC3 in the round-6 closing review rather than by the round's own remediation; round 7 (tasks #888-894) is a 7th instance, caught by TC1 the identical way; round 8 (tasks #897-903) is an 8th instance, caught by UC1 the identical way; round 9 (tasks #906-907) is a **9th instance**, caught by V2C6 in the round-9 closing review — so the "within-round catch" streak was 2 rounds long (4, 5), broke at round 6, and has now failed to recur for FOUR consecutive rounds (6, 7, 8, 9, all caught only by their closing review). This is the strongest evidence yet that the standing rule (not just the closing-review habit) is needed: the closing review is itself optional per-round, and rounds 6-9 show the gap reappears every time the round's own remediation doesn't happen to include a CHANGELOG-writing task — which, absent the standing rule, is every round's default state, not an occasional lapse.
   - **Next trigger:** Any round that closes with ≥ 1 commit and no CHANGELOG.md entry — if the standing rule is NOT adopted, this item stays open as the durable reminder; if adopted, the rule itself prevents recurrence
   - **Evidence:** F11 in `docs/reviews/2026-08-12-aligned-vmem-round3-review.md` (lines 370-392); the task description for the current task citing "this is the THIRD consecutive round with this exact gap"; commit `c14bd3a` (task #863) closed the round-2 gap after round-3's F11 caught it; commit `7663811` (task #857) closed the round-1 gap after round-2's own W16 caught it; CR10 in `docs/reviews/2026-08-12-aligned-vmem-round4-closing-review.md` caught round 4's own instance before the round was considered closed; QC9 in `docs/reviews/2026-08-13-aligned-vmem-round5-closing-review.md` caught round 5's own instance the same way; SC3 in `docs/reviews/2026-08-13-aligned-vmem-round6-closing-review.md` caught round 6's own instance one step later (the closing review, not the round's own remediation); TC1 in `docs/reviews/2026-08-13-aligned-vmem-round7-closing-review.md` caught round 7's own instance the identical way; UC1 in `docs/reviews/2026-08-13-aligned-vmem-round8-closing-review.md` caught round 8's own instance the identical way; V2C6 in `docs/reviews/2026-08-13-aligned-vmem-round9-closing-review.md` caught round 9's own instance the identical way

2. **Review-doc commit convention: the aligned-vmem campaign commits its readonly review docs; this is a campaign-specific convention, distinct from and NOT contradicted by the root-crate R34 campaign's opposite convention.** (Filed round 8, task #904, finding UC2 of `docs/reviews/2026-08-13-aligned-vmem-round8-closing-review.md`, after that review flagged what looked like two contradictory conventions in this repository.) Investigated and resolved as two SEPARATE, correctly-scoped conventions, not a conflict needing unification:
   - **The aligned-vmem review campaign (this campaign, rounds 1-8) commits its review docs.** Established by round 3/task #863 (`c14bd3a` committed `docs/reviews/2026-08-12-aligned-vmem-round3-review.md`), reconfirmed every round since: round 4's closing commit `7c6e4be`, round 5's `e60e46a`, round 6's `1dbd6b4`, round 7's `8380607` all explicitly commit that round's review doc(s) as part of the closing pass. Rationale: each doc is cited by `file:line`-style path from `docs/CORRECTNESS_OPEN_ITEMS.md` and `CHANGELOG.md` entries this SAME campaign writes, so an uncommitted doc breaks its own campaign's citations (exactly what UC2 caught for round 8, before this fix).
   - **The root-crate R34 review campaign (a different, unrelated readonly-review campaign, using `/crush`/`@fh`-style delegation over the whole `sefer-alloc` root crate rather than this one `aligned-vmem` sub-crate) does NOT commit its review docs.** Established explicitly by R34-2/task #521 (`CHANGELOG.md`'s own text: "this project's established convention that readonly review reports stay uncommitted local artifacts" — R34-2 self-corrected via `git rm --cached` after accidentally committing two review reports). This is also the convention the project's `/research` skill documents for its own generated reports ("reports stay local, uncommitted artifacts").
   - **These do not need to be unified.** They differ because their artifacts are consumed differently: R34's reports are read once during their own round and not re-cited by path from a durable index afterward (verified: `docs/CORRECTNESS_OPEN_ITEMS.md`'s R34-era entries cite task numbers and commit SHAs, not `docs/reviews/*.md` paths); this campaign's reports ARE re-cited by path, repeatedly, across many subsequent rounds (item 48 alone cites 4 different rounds' review docs by path). A convention that fits one artifact class does not need to fit the other.
   - **A round-start reader needs to take NO ACTION on this card.** **PERMANENT DECISION RECORD — deliberately kept in the active listing rather than archived (task #1111, R34-24 audit):** it is RESOLVED and needs no work, but its value is that a round-start reader meets it right before writing a review doc, which is exactly the moment the question it answers ("commit it or not?") arises. Archiving it would put the answer one hop away from the moment of the question. **Status:** RESOLVED — no code/process change needed beyond this clarifying note, so a future round does not re-investigate the apparent conflict from scratch. Round 8's two review docs (`docs/reviews/2026-08-13-aligned-vmem-round8-review.md`, `...-round8-closing-review.md`) are committed in the same commit as this note, per this campaign's own established convention.
   - **Evidence:** `docs/reviews/2026-08-13-aligned-vmem-round8-closing-review.md` finding UC2; `git log --oneline -- docs/reviews/2026-08-12-aligned-vmem-round3-review.md` etc. (all four prior rounds' docs resolve to real commits); `CHANGELOG.md`'s R34-2 bullet (the root-crate campaign's stated convention).

42. **`numa-shim`'s `mock` Cargo-feature-unification hazard remains a Cargo
    feature (deliberately deferred) — the aligned-vmem half of this item is
    CLOSED, see "Recently resolved" below.** (Filed 2026-08-09, task
    #776/F13, round-closing review of the aligned-vmem round; moved into
    the `[A]` tier 2026-08-14, task #934/C-9; aligned-vmem half resolved
    2026-08-16, task #962 — this card now covers ONLY the numa-shim half.)

    - **Status:** OPEN, not urgent — numa-shim has not yet had its first
      crates.io publish (task #657, itself blocked), so the "free to convert
      only until first publish" deadline that made the aligned-vmem half
      URGENT has not fired for numa-shim yet. `crates/numa-shim/Cargo.toml`'s
      `mock = []` feature retains the same Cargo-feature-unification hazard
      the aligned-vmem half had (documented in its own `Cargo.toml` comment,
      explicitly cross-referencing the aligned-vmem conversion as proof the
      `--cfg` approach works — see task #962's own commit).
    - **Next trigger:** settle the `--cfg` conversion decision for numa-shim
      before its own first publish (task #657) — same maintainer call, same
      mechanical shape (Cargo.toml + cfg-gated call sites + CI rows) as the
      aligned-vmem conversion task #962 just completed, which is now the
      reference implementation to follow.
    - **Evidence:** `crates/numa-shim/Cargo.toml`'s `mock = []` feature and its
      own doc comment (updated by task #962 to cite the aligned-vmem
      precedent); `crates/aligned-vmem/Cargo.toml`'s `[lints.rust] unexpected_cfgs`
      + the removed `mock = []` (the pattern to mirror).

62. **MIPS targets: release decision to fail compilation at compile time rather than accept buildable-but-broken targets.** (Filed 2026-08-16, task #1017, finding R4-1 of `docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md`.) MIPS (both `mips` and `mips64`) uses different `MAP_ANON`/`MAP_HUGETLB` constant values than the `asm-generic/mman-common.h` values this crate hardcodes for Linux: MIPS defines `MAP_ANONYMOUS = 0x0800` and `MAP_HUGETLB = 0x80000`, while the crate uses `0x20` and `0x40000` respectively. With the wrong constants, every `reserve_aligned` call fails closed at runtime with `EBADF` (invalid file descriptor) because `libc_mmap` issues `mmap(..., MAP_PRIVATE, -1, 0)` with no anonymous flag properly set, but the failure is silent (no diagnostic points to the constant error). The crate previously documented MIPS as "not supported" but still allowed compilation, publishing a buildable-but-broken target.

   - **A round-start reader needs to take NO ACTION on this card.** **PERMANENT DECISION RECORD — deliberately kept in the active listing rather than archived (task #1111, R34-24 audit):** the `compile_error!` in `crates/aligned-vmem/src/os/unix.rs` points a MIPS user AT THIS CARD by design, so archiving it would break the diagnostic's own destination — the one reader who most needs it arrives here from a failed build, not from a round-start read. **Status:** RESOLVED (release decision applied) — MIPS targets now fail compilation with a clear diagnostic (`compile_error!`) that explains the constant mismatch and points to this index entry for the decision record. The crate already uses the same pattern for unsupported Unix targets (task #918/finding H2C7); this extends it to architecture-specific broken targets. Adding MIPS support requires adding a `#[cfg(any(target_arch = "mips", target_arch = "mips64"))]` arm with the correct MIPS-specific constant values, gated on that architecture only.
   - **Decision rationale:** Fail-fast at compile time is preferable to publishing a buildable target that fails silently at runtime on every reservation call. The `EBADF` failure is documented but opaque to downstream users; a compile-time error with a diagnostic pointing at the specific problem (wrong constants) makes it explicit what must change to add support.
   - **Evidence:** the `compile_error!` gated on `any(target_arch = "mips", target_arch = "mips64")` in `crates/aligned-vmem/src/os/unix.rs` (post-split home, task #1082; `crates/aligned-vmem/src/lib.rs` at filing); the **MIPS** bullet in `crates/aligned-vmem/README.md`'s "Reasoned-from-spec targets" section (now marked "not supported (compile_error!)"); `docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md` finding R4-1 (evidence root)

11. **Coverage/process gap: `npm run check`'s
    clippy gate did not catch pre-existing example/test lint+compile errors
    that CI's clippy job caught.** _(The BUGS this item originally enumerated
    — the E0601 in `r31_10_trim_cost_gate` and the `doc_lazy_continuation` in
    `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs:257` —
    plus three further latent failures unmasked once those cleared (E0432/E0599
    in `r31_3_large_cache_extended_narrow_on` and `r31_8_large_cache_scan_isolation_*`
    from incomplete `required-features` missing `alloc-decommit`, and a
    `clippy::int_plus_one` in `tests/remote_ring_shadow_head.rs:165`) — were ALL
    fixed by R33-1/task #506, commit `e526517befbf5a0cd0ca1a7ee62f9d84ffe509ee`; see
    "Recently resolved" §6 below.
    This remaining open half is the coverage GAP, not the bugs.)_
    `scripts/check-all.mjs` HAS run all five ci.yml clippy rows since R30-5
    (task #454), so the local gate should have caught at least the failures
    under the default/experimental/`--all-features` combos it exercises — yet
    the offending commits landed red. Follow-up: determine why (procedural —
    pushed without running `npm run check`; or an as-yet-undetected drift
    between the local matrix and ci.yml) and tighten enforcement so a red
    `cargo clippy --all-targets -- -D warnings` row cannot land again
    regardless of which of the five rows breaks.

    **R33-2 update (task #507, 2026-08-03) — ROOT CAUSE FOUND; this is NOT a
    coverage gap.** Direct investigation (git archaeology + infrastructure
    audit) establishes the cause is PROCEDURAL, on two independent grounds,
    and rules out the alternatives the original framing left open:

    - **NOT a coverage gap.** `scripts/check-all.mjs` runs all five ci.yml
      clippy rows (GENERATED from `PER_PR_ROWS`, byte-identical argv, since
      R30-5/task #454), pinned by `tests/ci_clippy_matrix_consistency.rs`. The
      original "coverage/process gap" framing above was a misdiagnosis of the
      *symptom* (red rows landed) as a *hole in the gate*; the gate has no
      hole. The item's "coverage" half is therefore CLOSED.
    - **NOT toolchain drift, NOT a later-commit reintroduction.** Three of the
      five failures (E0601 `r31_10_trim_cost_gate`, E0432/E0599
      `r31_3_large_cache_extended_narrow_on`, E0599 `r31_8_large_cache_scan_*`)
      are rustc *compile errors*, not clippy lints — they cannot be caused by
      clippy tightening and would fail under any toolchain the moment the file
      was introduced; `git log -S` shows each was introduced WITH its
      file/line in its own round (`0985e22d1075135bb9740b23a457d32742d2a072`
      R31-3 = 70 commits pre-fix; `4f897237cf6e4bcbe6a722f5c124890e15f07e82`
      task #488 = 36 commits; `e6bbc6acbc3f01b649d70b02bd41b4f664dc822e`
      R32-1 = 30 commits; `d38bf73c63fa989eace81e659a3844b98f6656c5`
      task #502 = 9 commits), not reintroduced by an unrelated later
      change. The two lints (`doc_lazy_continuation`, `int_plus_one`) are
      long-stable. No `rust-toolchain.toml` exists to have drifted.
    - **The actual cause: the "run `npm run check` before every push"
      convention (CLAUDE.md) was not followed for those pushes, AND the async
      CI red signal that should have been the safety net went unwatched.** This
      repo has NO enforcement of the convention — no git hooks (`.git/hooks/`
      holds only samples; `core.hooksPath` unset), no husky/lint-staged, and no
      required status check blocks a direct push to `main` (direct-commit model;
      CI runs *after* the push).

    **Disposition / hardening (R33-2):** a mandatory pre-push git hook was
    considered and rejected as out-of-character for this repo's
    convention-by-discipline culture (CLAUDE.md uses zero hooks; a hook that
    silently blocks pushes a developer doesn't know about is itself a footgun)
    and low-effectiveness in practice (the developers who skip the gate are
    exactly those who won't install an opt-in hook). The implemented measure is
    the appropriately-scoped one for this repo: CLAUDE.md's "Before every push:
    `npm run check`" section is strengthened with (a) the diagnosed root cause,
    (b) a correction of its own stale "three feature-matrix entries" text (it
    has been five clippy rows since R30-5), and (c) the genuinely-missing piece
    — a **post-push "confirm CI went green" step** (CI is the only async safety
    net, runs an unpinned toolchain/OS the local gate cannot reproduce, and is
    the thing that eventually caught this — main was red for up to 70 commits
    purely because nobody watched the post-push run). The airtight ceiling —
    GitHub branch protection requiring the `clippy` check before merge — is
    recommended but is repo-settings-side, outside any file a commit can touch.
    Residual OPEN: re-scoped to "maintain the post-push CI-watch discipline now
    in CLAUDE.md"; the original "coverage-gap" follow-up is closed (there was
    no gap to tighten).

13. **Root-caused: `git worktree add` +
    this environment's global `CARGO_TARGET_DIR` can leave STALE test
    binaries that fail with misleading errors after the worktree is
    removed — a real hazard for the worktree-isolation BEFORE/AFTER
    measurement pattern this file's sibling `docs/perf/OPEN_ITEMS.md` (and
    CLAUDE.md's R29-6/"bench-profile pinning" rules) already establish as
    standard practice.** This environment sets `CARGO_TARGET_DIR=D:\dev\rust\.cargo-target`
    globally (`env | grep CARGO_TARGET_DIR`) — a location OUTSIDE any
    single worktree, shared by every `cargo` invocation regardless of
    which worktree's `CARGO_MANIFEST_DIR` ran it. At least 4 test files
    (`tests/ci_clippy_matrix_consistency.rs`, `tests/dbg_hook_safety_tripwire.rs`,
    `tests/no_stale_doc_references.rs`, `tests/no_stale_loom_files.rs`) use
    `env!("CARGO_MANIFEST_DIR")` — a COMPILE-TIME constant baked into the
    compiled test binary. During task #498's own verification, two
    `git worktree add`s were created and removed (for BEFORE-measurement
    isolation and for a flaky-test baseline check), each building into the
    same shared `CARGO_TARGET_DIR`. After both worktrees were removed, the
    NEXT `cargo test --features production` run against the main tree
    intermittently reused a stale compiled test binary (cargo's fingerprint
    matched on identical SOURCE content, not on which worktree produced the
    binary) whose baked-in `CARGO_MANIFEST_DIR` pointed at one of the
    now-deleted worktree paths — producing `read scripts/check-matrix.mjs:
    NotFound` and `panicked ... "no source files found"` errors that look
    like real test failures but are pure build-cache staleness. Confirmed
    the fix: `touch <file>.rs` (or any edit) on each of the 4 affected test
    files forces a rebuild and the failures disappear; a subsequent full
    suite run was clean. **Not itself investigated for a permanent fix**
    (e.g. a per-worktree `CARGO_TARGET_DIR`, or a documented "run `cargo
    clean -p sefer-alloc --profile test` after removing a measurement
    worktree" step) — filed here so a future round doing BEFORE/AFTER
    worktree-isolated measurement (the R14-10/R29-6-established pattern)
    knows to either use a worktree-local `CARGO_TARGET_DIR` override or
    force-touch/rebuild the `env!(CARGO_MANIFEST_DIR)`-dependent test files
    after removing a scratch worktree, rather than re-diagnosing this from
    scratch.
### [T] Tracked, not yet actioned

_(item 1, the `canary_survives_promotion_and_free_leaves_no_leak` flaky test,
was resolved by an urgent CI-fix task — see "Recently resolved" below.)_

_(item 2, the 11 `--features "hardened medium-classes"` clippy dead-code
errors, was resolved by R23-5 (task #374) — see "Recently resolved" below.)_

_(item 3, the two flaky coarse-wall-clock tests, was resolved by R23-6
(task #375) — see "Recently resolved" below.)_

_(item 4, `canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
assertion proving no double-release but not no leak, was resolved by R28-2
(task #431) — see "Recently resolved" below.)_

5. **Findings from the R29 post-round independent readonly review
   (`docs/reviews/2026-07-29-r29-readonly-review.md`) not yet independently
   re-verified or actioned beyond this index entry.** The review's two P0/P1
   build breaks (a missing iai-arm stub, an ungated dead-code pair) and the
   R29-16 wall-clock bench design bug were independently confirmed and
   fixed/corrected the same day (see `CHANGELOG.md`'s Round 29 entry and
   `docs/perf/OPEN_ITEMS.md` item 25 for those). The following were NOT
   independently re-verified before filing — flagged here at the review's
   own confidence/severity, for a future round to check and either action or
   dismiss:
   - **[P2 → CONFIRMED P1, 2026-07-30] `AllocCore::dbg_decomp_full_cycle`**
     (`src/alloc_core/alloc_core_small_pool.rs:1014`, R29-3/task #434) is a
     SAFE `pub fn` that calls `reserve_small_segment` then
     `release_or_pool_empty_segment` on the freshly-reserved base.
     **My original text here (below, struck) was FACTUALLY WRONG and is
     corrected in place; the review's claim was right.**
     > ~~"A first-pass trace during this session's own review suggested
     > `small_cur` is likely never touched by this hook at all (the
     > assignment it would need to collide with lives in a codepath
     > `dbg_decomp_full_cycle` doesn't call)."~~

     **Corrected trace (2026-07-30, verified line-by-line after a SECOND
     independent review — `docs/reviews/2026-07-30-r29-followup-readonly-review.md`
     §1.2 — reached the same conclusion and explicitly flagged my note as
     "based on a mistaken call trace"):** `self.small_cur = base;` is the
     **last statement of `reserve_small_segment` itself**
     (`alloc_core_small.rs:2210`, inside the fn spanning 1848–2212) — i.e. it
     lives in exactly the function the hook calls, not in a different
     codepath. My error was assuming the assignment belonged to
     `alloc_small_with_virgin` (its caller) rather than to the callee. The
     full confirmed sequence:
     1. `dbg_decomp_full_cycle` → `reserve_small_segment()` → sets
        `self.small_cur = base` (`alloc_core_small.rs:2210`).
     2. → `release_or_pool_empty_segment(base)`
        (`alloc_core_small_pool.rs:333`). Pool full ⇒
        `release_empty_segment_now(...)` + `self.table.recycle(base)`
        (`:380-381`) — the OS reservation is RELEASED and the slot recycled.
     3. Neither function clears or restores `small_cur`. It now points at
        unmapped memory.
     4. The next ordinary small alloc starts at
        `alloc_small_with_virgin`'s step 1,
        `self.pop_free(self.small_cur, ...)` (`alloc_core_small.rs:278`) — a
        read through the released segment's header.

     **Not hypothetical:** `examples/r29_3_decomposition_gate.rs:82-84`
     DELIBERATELY pre-fills the pool (`for _ in 0..(pool_cap + 2)`, its own
     comment: "not the pool-push path") specifically to drive the release
     branch, then loops the hook N=200 times. The only reason there is no
     in-tree crash is that the harness never performs an ordinary small
     allocation afterward — that is caller luck, not a sound API.
     `dbg_decomp_reserve_and_keep` + `dbg_decomp_release` has the identical
     state hazard (marking only the raw-pointer half `unsafe` expresses the
     pointer contract but not the cursor invariant).

     **Why R29-9's tripwire missed it:** the scanner only selects safe
     `pub fn dbg_*` whose signature text contains `*mut`/`*const`.
     `dbg_decomp_full_cycle(&mut self) -> bool` takes no pointer and returns
     none, so it is structurally out of scope — the zero-argument
     state-invalidating hole listed in the `[P3]` tripwire item below, now
     with a confirmed live instance.

     **Needs (Round 30, correctness-before-measurement):** either a
     measurement-only reservation primitive that does the OS/table/metadata
     work without touching the live `small_cur`, or save-and-restore of the
     prior cursor with an assertion that the restored base is still
     registered (preferred over merely making the hook `unsafe`, which would
     leave the allocator unusable-by-contract). Plus a counterfactual test:
     fill the pool → call the hook → perform and free a normal small alloc on
     the same heap; it must fail before the fix and pass after.

     **[FIXED, R30-1/task #450, 2026-07-30.]** Took fix option 1 (the
     "best" option in the task spec) — a measurement-only reservation
     primitive that never touches `small_cur` at all, rather than option 2
     (save/restore) or option 3 (`unsafe fn` + do-not-alloc-after contract).
     Option 1 was chosen because it was structurally cheap here:
     `reserve_small_segment`'s ENTIRE cursor-publishing side effect was
     already isolated to its literal last statement
     (`self.small_cur = base;`, immediately before `Some(base)`), with
     nothing earlier in the function reading `small_cur` and nothing after
     it depending on the write — so the function split cleanly into a new
     `pub(super) fn reserve_small_segment_impl(&mut self) -> Option<*mut u8>`
     (`alloc_core_small.rs`, everything BEFORE that last line) and a
     one-line `reserve_small_segment` wrapper (`let base =
     self.reserve_small_segment_impl()?; self.small_cur = base; Some(base)`)
     kept for the three production callers (`alloc_small`,
     `alloc_small_with_virgin`, `refill_class_bump_impl`), which still need
     the publish. `dbg_decomp_full_cycle` and `dbg_decomp_reserve_and_keep`
     (`alloc_core_small_pool.rs`) now call `reserve_small_segment_impl`
     instead, so `small_cur` is never touched by either hook and cannot be
     left dangling by them, at any pool fill level, however many times they
     run. `dbg_decomp_release` additionally got a defence-in-depth
     `debug_assert!(base != self.small_cur, ...)` (not reachable today, but
     cheap). Added `tests/r30_1_decomp_full_cycle_cursor_safety.rs` — two
     tests, one per hook pair named in the task, each: fills the pool to
     capacity, drives the release branch repeatedly via the hook, then
     performs an ordinary alloc + write + readback + free on the SAME heap.
     **Verified non-vacuous**: temporarily reverted the two hooks' call
     sites back to `reserve_small_segment()` (the pre-fix code) and reran —
     `full_cycle_hook_leaves_small_cur_valid_for_ordinary_alloc` crashed the
     whole test process with `STATUS_ACCESS_VIOLATION` (Windows hard fault,
     exit code `0xc0000005`), a genuine use-after-free through the dangling
     cursor — then re-applied the fix and confirmed both tests pass. Full
     `cargo test --features "bench-internals alloc-global alloc-xthread
     alloc-decommit fastbin alloc-segment-directory primordial-lazy-commit
     class-aware-dirty"` is green (228 test binaries, 0 failures) including
     this new test; `cargo clippy` clean on both `--features production`
     and `--features "production bench-internals"`; `cargo fmt --check`
     clean. The R29-3 gate (`docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`)
     was re-run post-fix on its original WSL2/Linux measurement platform —
     verdict unchanged (trigger 2 still does not fire); see that doc's new
     §8 append-only correction section, which also documents a SEPARATE,
     pre-existing, unrelated finding surfaced during that re-verification
     (a native-Windows crash in the example's decommit/refault arm, caused
     by Windows `MEM_DECOMMIT` semantics differing from Linux
     `MADV_DONTNEED` — confirmed unrelated to this fix and filed as item 6
     below rather than fixed here).
   - **[P3 → partly CONFIRMED, 2026-07-30] `has_bench_internals_cfg()` accepts
     any cfg attribute whose TEXT merely contains `"bench-internals"`** — so
     `not(feature = "bench-internals")` and a permissive
     `any(feature = "bench-internals", ...)` would both be accepted as if
     they gated the hook (second review, §1.3). Read and confirmed by
     inspection. No live false-accept exists today (no `dbg_*` hook currently
     uses either shape), so this is a latent scanner weakness, not a live
     hole — but it is a substring test standing in for a cfg-predicate
     parse. Fix alongside the scope widening below.

     **[FIXED, R30-2/task #451, 2026-07-30.]** Replaced the substring test
     with a small hand-written recursive-descent parser
     (`CfgParser`/`CfgExpr`/`parse_cfg_inner`/`requires_bench_internals` in
     `tests/dbg_hook_safety_tripwire.rs`) for the actual cfg-predicate
     grammar subset this project uses: `feature = "x"`, `all(...)`,
     `any(...)`, `not(...)`, nested and comma-separated. `syn` is NOT a
     dev-dependency of this crate (checked `Cargo.toml`'s
     `[dev-dependencies]` before hand-rolling this — no `syn` entry), so a
     small hand-rolled parser was the right call over adding a dependency
     for a test-only predicate check. `requires_bench_internals` implements
     exactly the rule the review specified: `all(...)` counts if ANY child
     requires the feature (conjunction — one required child forces it on);
     `any(...)` NEVER counts, even in the degenerate case where every branch
     happens to require it individually (a deliberate refusal to reward the
     more permissive shape, since rewarding it would reopen exactly the
     `any(feature = "bench-internals", X)` hole); `not(...)` never counts.
     Two new dedicated tests: `cfg_parser_rejects_negated_and_optional_or_bench_internals`
     (unit-tests the parser directly against both target shapes plus the
     genuine-gate shapes already used in this crate, including nested
     `all(all(...))`) and `no_dbg_hook_cfg_uses_negated_or_optional_or_bench_internals_shape`
     (re-confirms, using the NEW structural parser rather than a substring
     match, that no current `dbg_*` hook in the crate actually uses either
     adversarial shape — the same "no live false-accept today" fact the
     review found by manual inspection, now asserted mechanically going
     forward). Both pass; see the R30-2 commit for verification details.
   - **[P3] `tests/dbg_hook_safety_tripwire.rs`'s allowlist may have scope
     holes**, per the review: possible misclassification of `any`/`not`
     `#[cfg]` predicates, hooks keyed by an integer parameter rather than a
     pointer, and zero-argument hooks that still return a raw pointer — none
     independently re-verified this session. If real, these are gaps in the
     R29-9 tripwire's own coverage (task #440), not yet a confirmed live
     soundness hole.

     **[FIXED, R30-2/task #451, 2026-07-30.]** Confirmed the scope gap was
     real and live, not merely theoretical: R30-1 (task #450, commit
     `25433c3`) found and fixed a CONFIRMED soundness bug
     (`AllocCore::dbg_decomp_full_cycle`, a zero-argument, no-raw-pointer,
     `&mut self -> bool` hook) that was structurally invisible to the R29-9
     scanner, exactly the "zero-argument hook" gap this item flagged as a
     possibility. Redesigned the tripwire's policy to be shape-independent
     per the review's own recommendation: every crate-public `dbg_*` hook
     (any signature shape — raw pointer, zero-arg `&mut self`,
     `usize`/index-keyed, or an integer-encoded-address return) is now
     enumerated by `scan_file` (which no longer branches on `*mut`/`*const`
     substrings at all) and must land in exactly one of three buckets:
     `PURE_OBSERVERS` (read-only, no justification needed beyond "read-only"),
     `SAFE_MUTATORS` (safe hooks that mutate allocator/ring state, each with
     a one-line invariant justification — bounds check, delegation to the
     identical production code path, or a correctness-inert policy/heuristic
     knob), or `UNSAFE_HOOKS` (already-`unsafe fn`, enumerated for exhaustive
     accounting only, not a new safety argument). Rebuilt the allowlist from
     scratch by enumerating every `pub fn dbg_*`/`pub unsafe fn dbg_*` in
     `src/` and `crates/` (~140 hooks) and reading each function BODY (not
     just its signature) to classify it — not guessed from names. Two hooks
     surfaced during that re-classification as worth flagging explicitly
     rather than silently accepted: `remote_free_ring.rs::dbg_set_cursors`
     and `heap_overflow.rs::dbg_reserve_unpublished_for_test` both mutate a
     REAL production ring's cursors under a documented "quiescent ring"
     precondition that is enforced only by a `debug_assert!` (compiled out
     in `--release`), not a release-surviving guard — allowlisted with an
     explicit `[DEBUG_ASSERT ONLY]` tag in their justification (misuse can
     only corrupt the ring's own bookkeeping — lost/miscounted entries —
     never dereference a caller pointer or write outside the ring's own
     cursor words, so accepted as SAFE_MUTATORS rather than escalated to
     KNOWN_UNFIXED, but flagged so a future reviewer does not have to
     re-derive that distinction). **Non-vacuity proof**: added
     `widened_scanner_catches_r30_1_shape_zero_arg_mutator`, which feeds
     `scan_file` a synthetic in-memory fixture mimicking the exact pre-fix
     `dbg_decomp_full_cycle` shape (`pub fn ..._SCRATCH_FIXTURE(&mut self)
     -> bool`, no cfg, no raw pointer anywhere in the signature — the
     fixture never touches real `src/`) and asserts the widened scanner
     finds it, classifies it safe/ungated, and that it is not allowlisted
     (i.e. it would surface as a tripwire failure) — then separately asserts
     the fixture's source text genuinely contains neither `*mut` nor
     `*const`, proving the OLD R29-9 scanner would have silently skipped it.
     Verification: `cargo test --features "bench-internals alloc-global
     alloc-xthread alloc-decommit fastbin alloc-segment-directory
     primordial-lazy-commit class-aware-dirty" --test
     dbg_hook_safety_tripwire` green (4 tests); full `cargo test --features
     "production bench-internals"` green (0 failures); `cargo clippy
     --features "production bench-internals" --tests -- -D warnings` clean;
     `cargo fmt --check` clean. R29-9's original commit message claim that
     it "closes the bug class for good" is corrected by this entry and by
     the widened test file's own header doc comment — the bug class
     recurred through a shape the original scanner didn't model, and R30-2
     makes the check shape-independent instead of re-asserting closure.
   - **[P3] R29-1's replacement leak-bound invariant** (`segments_released_total
     <= segments_reserved_total`, task #432) may be "near-unfalsifiable" per
     the review, meaning most of the file's actual leak coverage now rests
     on the `alloc-decommit + alloc-xthread`-gated per-base diagnostic block
     rather than the global counter itself. Not independently re-verified;
     if true this narrows (but per R29-1's own scope note does not
     eliminate) what the global invariant alone catches.

     **[FIXED, R30-11/task #460, commit `a32acf9`.]** Confirmed the review
     right: the cumulative invariant proves no impossible double/over-release
     occurred, but a MISSING release only makes it MORE comfortably true, so
     it has zero leak-detection power on its own — exactly the concern this
     item flagged. R29-1's LOGIC was untouched (still correct, not
     re-litigated); the defect was that
     `tests/r14_4_promotion_free_correctness.rs`'s single combined test
     function, `canary_survives_promotion_and_free_leaves_no_leak`, kept a
     name promising "no leak" in EVERY feature combination it compiled
     under — including the CI-tested `hardened medium-classes` row
     (`hardened = ["fastbin"]` = `alloc-global + alloc-xthread`, WITHOUT
     `alloc-decommit`), where only the cumulative check (renamed, see below)
     exists and the real per-base leak proof does not compile at all. Split
     into two `#[test]`s, each named for exactly what it proves:
       - `canary_survives_promotion_and_free_no_double_release` — always
         compiled under the file's top-level gate; canary survival + the
         cumulative `segments_released_total <= segments_reserved_total`
         invariant (renamed from the ambiguous `no_double_release` framing
         to an explicit "over-release", not "leak", claim in both the local
         variable name and the assertion failure message) + no corruption.
         Never claims "no leak".
       - `canary_survives_promotion_and_free_leaves_no_leak_per_base` — NEW
         name, gated `alloc-decommit + alloc-xthread` (unchanged gate,
         unchanged assertion logic from the pre-existing per-base block);
         the genuine per-base leak proof, compiled only where its
         diagnostic surface (`dbg_live_count_for`, `alloc-decommit`-gated)
         exists.
     **Confirmed no stronger diagnostic is available for `hardened
     medium-classes`** (deliverable 3(b) — investigated, not assumed):
     `dbg_contains_base` alone (`alloc-global + alloc-xthread`, available
     under `hardened`) cannot substitute for `live_count`, because without
     `alloc-decommit` the small-segment release/pool machinery itself
     (`dec_live_and_maybe_decommit` / `dec_live_batch_and_maybe_decommit`,
     `src/alloc_core/alloc_core_small_pool.rs`) is entirely
     `#[cfg(feature = "alloc-decommit")]` — small/medium segments are never
     released or live-count-tracked at all under that combo, so
     `dbg_contains_base` would read `true` forever regardless of whether a
     leak occurred. This is an honest, documented gap (module doc + the
     `no_double_release` test's own doc comment in
     `tests/r14_4_promotion_free_correctness.rs`), not a silently accepted
     one.
     **Non-vacuity re-verified (both of R28-2's original counterfactual
     paths, against the restructured file):**
       - **Large-promoted path** (`production medium-classes`): disabled
         `self.table.unregister(base)` in the cache-admitted leg of
         `AllocCore::dealloc`'s Large branch
         (`src/alloc_core/alloc_core.rs`, `#[cfg(any())]`) — reproduces
         R28-2's own documented alternate outcome at this exact site: a
         deterministic `STATUS_ACCESS_VIOLATION` crash (both `--release`
         and debug profiles), because the segment becomes genuinely
         double-owned (still in `large_cache` AND left registered) and
         `dbg_trim_current_thread`'s `evict_all` double-frees it. A crash is
         still a detected, non-vacuous `cargo test` failure (nonzero exit,
         reported as failed) — reverted cleanly (`git diff` empty on
         `src/`), test passes again.
       - **Medium-ladder path** (`production medium-classes
         exact-span-large`): disabled the `dec_live_batch_and_maybe_decommit`
         block in `flush_run` (`src/alloc_core/alloc_core_small_magazine.rs`,
         `#[cfg(any())]`) — clean assertion failure,
         `live_count went from Some(2) to Some(2)`, exactly the "no change
         at all" signature the assertion's own doc comment predicts.
         Reverted cleanly (`git diff` empty on `src/`), test passes again.
     Verified personally: all three relevant combos compile and pass
     (`hardened medium-classes` — 2 tests, no per-base test present;
     `production medium-classes` and `production medium-classes
     exact-span-large` — 3 tests each, per-base test present and passing);
     `cargo clippy --features "hardened medium-classes" --all-targets -- -D
     warnings` and `cargo clippy --features "production bench-internals"
     --all-targets -- -D warnings` both clean; `cargo fmt --check` clean.
     No `src/` behavior change (the two counterfactual breaks used for
     non-vacuity verification were both reverted before this commit — `git
     diff` on `src/` is empty). No version bumps.

6. **[T, filed 2026-07-30 during R30-1/task #450's verification]
   `examples/r29_3_decomposition_gate.rs` crashes with
   `STATUS_ACCESS_VIOLATION` when run NATIVELY on Windows** (as opposed to
   under WSL2/Linux, which is where this example's own gate report,
   `docs/perf/R29_3_DECOMMIT_RESERVE_DECOMPOSITION_GATE.md`, has always
   measured — see that doc's "Platform measured" line). The crash is in
   Measurement B: the `write_volatile` re-fault loop immediately after
   `HeapCore::dbg_decomp_decommit_payload`. Root cause: Windows
   `MEM_DECOMMIT` (`crates/aligned-vmem/src/os/windows.rs`'s
   `decommit_pages_impl` — post-split home, task #1082) genuinely UNMAPS the payload pages, unlike Linux
   `MADV_DONTNEED`, which keeps the VA mapping resident and transparently
   re-faults a fresh zero page on next write. The example's Measurement B
   loop assumes the Linux semantics unconditionally (write-after-decommit
   silently re-faults); on Windows a write to a decommitted-but-not-yet-
   recommitted page is a hard access violation without an explicit
   `VirtualAlloc(..., MEM_COMMIT, ...)` recommit call, which the example
   never makes. **Confirmed unrelated to R30-1's `small_cur` fix**: the
   crash reproduces identically with that fix applied or reverted, and
   lives in a code path (`dbg_decomp_decommit_payload` → `os::decommit_pages`
   → `crates/aligned-vmem`) R30-1's diff never touches; isolated by running just
   the R30-1-relevant hooks' pre-fill/A/C/A' loops (which never call
   `dbg_decomp_decommit_payload`) natively on Windows for hundreds of
   iterations with no crash. **Needs (future round):** either gate
   Measurement B's re-fault loop on `cfg(not(windows))` with an honest
   "irreducible floor not measured on this platform" note, or add the
   missing `VirtualAlloc(MEM_COMMIT)` recommit call before the
   `write_volatile` loop so the measurement is platform-correct everywhere
   (this would also make Measurement B's timing include the ACTUAL Windows
   recommit cost — currently assumed `0 ns` "implicit" per the doc's own
   §2 table, which is a Linux-only claim; Windows `MEM_COMMIT` is a real
   syscall, not implicit).

7. **[T, filed 2026-07-30, R30-10/task #459]
   `dbg_decomp_reserve_and_keep`/`dbg_decomp_release`
   (`src/alloc_core/alloc_core_small_pool.rs:1070-1115`) mint-then-redeem a
   bare `*mut u8` segment base with only a `debug_assert!` (compiled out in
   `--release`) guarding against releasing the live `small_cur` cursor —
   the same hazard class R30-1 (task #450) fixed for `dbg_decomp_full_cycle`,
   still standing on a weaker, non-release-surviving backstop for this
   specific pair. R30-10's design evaluation
   (`docs/design/R30_10_MEASUREMENT_HOOK_ISOLATION_DESIGN.md` §5) found this
   is the ONE current hook pair in the crate that both mints a NEW raw
   pointer via a `dbg_*` call and requires the caller to hold and later hand
   it back — the shape a typed, non-forgeable, move-consumed handle
   (`ReservedSmallSegment`, sketched in that document's §5.2-5.3) would fix
   structurally: a forged handle becomes uncomputable (private field +
   `pub(crate)` constructor) and a double-release becomes a compile error
   (E0382, moved value) instead of an unchecked runtime hazard. NOT
   implemented this round — the retrofit is a small (~5-file) but
   NOT-zero-risk diff (touches `AllocCore`'s definition, `HeapCore`'s
   forwarding delegate in `heap_core_diag.rs:854-857`, and both real
   callers, `examples/r29_3_decomposition_gate.rs` and R30-1's OWN
   counterfactual regression test `tests/r30_1_decomp_full_cycle_cursor_safety.rs`)
   that deserves its own review as the first typed-handle pattern in this
   codebase, not a same-task rubber stamp alongside the design doc that
   proposes it. **Trigger to action:** either (a) a 6th confirmed instance
   of the R25-1/R29-7/R29-8/R29-17/R30-1 "safe `dbg_*` hook touches live
   allocator state unsoundly" bug class, or (b) any future task adding a
   SECOND mint-then-redeem raw-pointer `dbg_*` pair to the inventory
   (enumerated in `tests/dbg_hook_safety_tripwire.rs`), at which point one
   handle type amortizes across both pairs. Full crate-wide hook
   relocation into one module — the OTHER piece of the architecture this
   task evaluated — was declined outright, not deferred: measured at
   102-139 distinct `tests/`/`examples/`/`benches/` files touched (4-5x the
   ~26-file footprint R24-6/task #384 already declined for a SINGLE hook,
   `dbg_push_to_ring`), AND independently shown to not address the actual
   defect mechanism in any of the five real incidents (each was fixed by
   changing hook BODY/signature, never by relocation — see the design
   doc's §3 table). Not re-opened by this trigger; would need a materially
   different argument (e.g. a demonstrated scatter-caused maintenance cost,
   not just a recurrence of the already-explained bug class) to revisit.

   **[FIXED, R31-4/task #467, commit `ca9aba9`, 2026-07-30/31.]**
   Implemented `ReservedSmallSegment` exactly per §5.2-5.3's sketch, in a new
   one-export file (`src/alloc_core/reserved_small_segment.rs`, per this
   project's file-structure rule): a private `base: *mut u8` field, a
   `pub(super)` constructor (`new_from_reservation`) reachable only from
   `AllocCore`'s own reservation path inside `alloc_core_small_pool.rs` — no
   `pub` constructor exists anywhere in the crate, so a handle cannot be
   forged from an arbitrary address — and a `pub(super) fn into_base(self)
   -> *mut u8` that consumes the handle by value (`core::mem::forget`ting
   `self` first to disarm the `Drop` leak-detector, since this consumption
   IS the release, not a leak). `dbg_decomp_reserve_and_keep` now returns
   `Option<ReservedSmallSegment>`; `dbg_decomp_release` now takes
   `ReservedSmallSegment` by value and is NO LONGER `unsafe fn` — the
   precondition that used to live in an `unsafe fn`'s `# Safety` contract is
   now upheld by the type itself. Calling `dbg_decomp_release(handle)` twice
   on the same binding is `rustc` error E0382 ("use of moved value") at
   COMPILE time — verified as the actual mechanism (not merely asserted) by
   confirming `ReservedSmallSegment` derives no `Copy`/`Clone` and carries a
   `Drop` impl, which makes `Copy` a hard compiler-rejected combination, not
   a project convention that could silently lapse. The existing
   `debug_assert!(base != self.small_cur, ...)` R30-1 added stays as
   secondary defence-in-depth. A `#[doc(hidden)] pub fn base(&self) -> *mut
   u8` read-only accessor (the established test-only-export pattern) lets
   `examples/r29_3_decomposition_gate.rs` read the payload address between
   reserve and release for its `write_volatile` measurement, without
   weakening the unforgeability guarantee (reading a value out is not
   constructing a new handle). Updated exactly the ~5 files the design doc
   estimated: `src/alloc_core/alloc_core_small_pool.rs` (the two hook
   definitions), `src/registry/heap_core_diag.rs` (the `HeapCore` forwarding
   delegates), `examples/r29_3_decomposition_gate.rs`,
   `tests/r30_1_decomp_full_cycle_cursor_safety.rs` (R30-1's own
   counterfactual regression test — re-verified still passes, both its
   tests, after the retrofit), and `tests/dbg_hook_safety_tripwire.rs`
   (removed both `dbg_decomp_release` entries from `UNSAFE_HOOKS` — the hook
   is safe now and stays `bench-internals`-gated, so it needs no
   `SAFE_MUTATORS` entry either). New counterfactual test
   `tests/r31_4_reserved_small_segment_handle.rs`: checked `Cargo.toml`'s
   `[dev-dependencies]` first — no `trybuild` or equivalent compile-fail
   harness exists in this crate — so per this task's own instruction, no new
   test-tooling dependency was added; instead the file thoroughly exercises
   the legitimate single-use and repeated-use (16-cycle) paths and documents,
   in both a code comment and `ReservedSmallSegment`'s own module doc,
   exactly why a second release call cannot compile. Full verification:
   `cargo test --features "production bench-internals alloc-stats"` green
   (230 test binaries, 0 failed); `cargo test --features "bench-internals
   alloc-global alloc-xthread alloc-decommit fastbin alloc-segment-directory
   primordial-lazy-commit class-aware-dirty" --test dbg_hook_safety_tripwire`
   green (7 tests); `cargo clippy --features "production bench-internals
   alloc-stats" --all-targets -- -D warnings` clean; `cargo clippy --features
   production -- -D warnings` clean, confirmed via a throwaway compile probe
   that `HeapCore::dbg_large_cache_hits` and (transitively)
   `dbg_decomp_reserve_and_keep`/`dbg_decomp_release` are genuinely absent
   from a plain `production` build; `cargo fmt --check` clean. Fixed two
   resulting doc-drift failures as a side effect (test-file count 227→228,
   README tier-2 `#[allow(unsafe_code)]` site count 68→66 — see item 8's
   entry below for why the count dropped by 2, not the expected-from-this-
   item-alone 1). No `production` feature composition changed.

   **[REOPENED then RE-FIXED, R31-15/task #486, 2026-08-01.]** The R31-4
   "FIXED" verdict directly above was PARTIAL, not complete: it closed
   unforgeability and double-release, but NOT owner-binding — a third,
   separate hazard the R31-4 entry's own prose never claimed to address (it
   describes the forgery and double-release guarantees specifically, never
   an owner check). CONFIRMED (independently verified by the task filer
   before filing, not just a review's claim) as a real, safe-reachable P0:
   `AllocCore::dbg_decomp_release(&mut self, handle: ReservedSmallSegment)`
   was a **safe** `pub fn`, and `ReservedSmallSegment` stored only a
   `base: *mut u8` with no owner identity — nothing stopped
   `core_b.dbg_decomp_release(h)` where `h` was reserved on `core_a`, both
   calls type-checking and compiling as ordinary safe code. Verified
   non-vacuously by temporarily reverting the R31-4-era source (`git stash`
   on just the three touched `src/` files) and confirming a throwaway probe
   test performed the cross-core release with **no panic on any build
   profile** — the pre-fix code had zero owner-related guard, release or
   debug. Fixed two ways, layered:
   1. **Structural owner token.** A new `bench-internals`-gated field
      `AllocCore::dbg_reservation_owner_id: u64`, stamped once at
      construction from a process-wide monotonic `AtomicU64` counter
      (`DBG_RESERVATION_OWNER_ID_COUNTER`, `alloc_core.rs`) — deliberately
      NOT the `&self` address (an `AllocCore` can move: it is returned by
      value from `AllocCore::new()` and lives inline in every registry
      `HeapSlot`, so two different logical `AllocCore`s can occupy the same
      address at different times over a process's life). `ReservedSmallSegment`
      gained a matching private `owner_id: u64` field, stamped by
      `dbg_decomp_reserve_and_keep` from the minting core's id.
      `dbg_decomp_release` compares the handle's `owner_id()` against its
      own `dbg_reservation_owner_id` via a release-build `assert_eq!` (NOT
      `debug_assert!` — a check compiled out in `--release` would defeat the
      point) before ever touching `self`'s pool/directory/`SegmentTable`
      state.
   2. **`unsafe fn` again**, with a `# Safety` doc contract, as defence-in-
      depth for what the owner-id check cannot see (the segment must still
      be live/unreleased) — matching the established pattern
      (`HeapCore::dbg_dealloc_own_thread_with_base`). Both `dbg_decomp_release`
      entries (`AllocCore`'s and `HeapCore`'s delegation) moved back into
      `tests/dbg_hook_safety_tripwire.rs`'s `UNSAFE_HOOKS`.
   A genuine correctness bug was found and fixed WHILE building this fix's
   own counterfactual test: asserting BEFORE consuming the handle (`handle`
   still a live local with its leak-detecting `Drop` impl armed) made the
   `assert_eq!` panic unwind straight into `ReservedSmallSegment::drop`'s
   own `debug_assert!(false, "dropped without going through release")` — a
   panic-during-panic, which Rust aborts on unconditionally, observed as a
   raw `STATUS_STACK_BUFFER_OVERRUN` process abort on Windows instead of a
   clean single panic. Fixed by reading `owner_id` and calling `into_base()`
   (disarming `Drop`) BEFORE the `assert_eq!`; the mismatch path still never
   reaches `self.release_or_pool_empty_segment` (the assert fires first),
   so this ordering fix only prevents the separate double-panic-abort
   failure mode, it does not weaken the rejection itself. New counterfactual
   test `tests/r31_15_reserved_small_segment_cross_core_release.rs`: a
   genuine two-`AllocCore` cross-core release (`#[should_panic(expected =
   "handle was reserved by a DIFFERENT AllocCore")]`), a same-core positive
   control (proving the guard doesn't false-positive on the legitimate
   path), and a source-text check that `HeapCore::dbg_decomp_release` stays
   a pure 1-line forward to `AllocCore::dbg_decomp_release` (justifying why
   no separate registry-bound two-heap counterfactual was built). Full
   verification: `cargo test --features "alloc-decommit bench-internals"`
   green for the four directly-touched test files; `cargo test --features
   production` green (`no_stale_doc_references.rs` initially caught the two
   expected doc-drift failures below, both fixed in the same commit);
   `cargo build --features production` / `--all-features` clean; `cargo
   clippy --tests -- -D warnings` / `--features experimental` /
   `--all-features` (the three real CI matrix entries) all clean; `cargo fmt
   --check` clean. Doc-drift fixed as a side effect: test-file count
   230→231 (`docs/ARCHITECTURE.md`), README tier-2 `#[allow(unsafe_code)]`
   site count 68→70 (exactly +2: the new `dbg_decomp_release` item-level
   allow at both the `AllocCore` and `HeapCore` layers). No `production`
   feature composition changed — the new `dbg_reservation_owner_id` field
   and its counter are `bench-internals`-gated, costing nothing in any
   `production`/default build (this crate's `AllocCore` lives inline in
   every `HeapSlot`, `MAX_HEAPS = 4096`, so an always-present field would
   have multiplied its size by 4096 regardless of whether any caller ever
   reaches the decomposition hooks — gating avoids that cost entirely).

8. **[T, filed 2026-07-30, UNVERIFIED-BY-ME findings from the Round 30 full
   independent review (`docs/reviews/2026-07-30-r30-full-review.md` §5
   P2-1/P2-2)]** The following two P2 findings were NOT independently
   re-verified before filing — flagged here at the review's own
   confidence/severity, for a future round to check and either action or
   dismiss, per this file's own convention (item 5 above is the precedent
   for this exact "filed, not fixed" pattern):
   - **P2-1 — `has_bench_internals_cfg` (`tests/dbg_hook_safety_tripwire.rs:657`)
     accepts `#[cfg_attr(...)]` as if it were a genuine `#[cfg(...)]` gate,
     latent instance of the same substring-match class R30-2 (task #451)
     fixed for two other shapes.** The review's claim: the parser's 5-byte
     prefix match `#[cfg` also matches `#[cfg_attr(`, and the parser then
     reads `cfg_attr`'s first argument (its *predicate*, not a gate
     condition on the attribute's own presence) as if it were a `cfg`
     predicate — the review states it proved this by extracting lines
     471-702 verbatim into a standalone `rustc` binary outside this repo
     and observing `cfg_attr(feature = "bench-internals", allow(dead_code))`
     parse as `true` (i.e. treated as a genuine gate). The review also
     states no live instance exists today (no `cfg_attr` in `src/` or
     `crates/` mentions `bench-internals`, per its own grep) — i.e. this
     is a latent parser gap, not a currently-exploitable hole, the same
     status R30-2 itself gave the two `cfg` shapes it did fix. Suggested
     fix per the review: match the literal `#[cfg(` (including the open
     paren) instead of the shorter `#[cfg` prefix.
   - **P2-2 — `HeapCore::dbg_large_cache_hits` (new, R30-6/task #455) is
     gated `alloc-decommit` alone, not `all(alloc-decommit,
     bench-internals)` like its four sibling measurement delegations in
     the same file.** The review's claim:
     `src/registry/heap_core_diag.rs:352-357` gates the hook on
     `alloc-decommit` alone (justified in its own doc comment as "matching
     `AllocCore::dbg_large_cache_hits`'s own gate exactly"), which is
     inside `production` (`Cargo.toml:399`) and so widens a `production`
     build's safe public surface; the same file's other four measurement
     delegations (`dbg_pool_cap`, `dbg_segment_state_reconciliation`,
     `dbg_large_cache_used`, `dbg_large_cache_slot_sizes`) are each gated
     `all(alloc-decommit, bench-internals)` and each cite "no production
     caller -> R25-10 sub-rule 2" — the CLAUDE.md benchmark-hook rule that
     any hook with no production caller MUST default to `bench-internals`
     unless it is the one sanctioned `dbg_push_to_ring` exception. The
     review notes this is NOT a soundness issue (the hook is
     `&self -> u64`, read-only, no pointer parameter, no mutation) and
     that `tests/dbg_hook_safety_tripwire.rs`'s `PURE_OBSERVERS` list
     already includes it (R30-6 added it there), so the R30-2 tripwire
     itself is satisfied — the finding is specifically that "the
     delegated method's pre-existing gate" is the reasoning CLAUDE.md's
     rule 2 rejects for NEW hooks, applied here to a genuinely new hook.
     Suggested fix per the review: add `feature = "bench-internals"` to
     its `cfg` and adjust the tripwire's gate-list accordingly (the
     review states the R30-6 probe that calls it already requires
     `bench-internals`, so nothing else should break).
   - **Next trigger:** independent re-verification of both claims (re-run
     the review's standalone `rustc` cfg-parser extraction for P2-1;
     re-read `heap_core_diag.rs:302-373` and the tripwire's gate-list for
     P2-2), then either apply the review's suggested one-line fixes or
     record a reasoned dismissal, in a future round.
   - **Evidence:** `docs/reviews/2026-07-30-r30-full-review.md` §5 P2-1,
     P2-2 (the review's own text is the only source cited here — this
     entry is a filing, not an independent confirmation).

   **[FIXED, R31-4/task #467, commit `ca9aba9`, 2026-07-31.]**
   Both claims independently re-verified before fixing, per the "Next
   trigger" instruction above.

   - **P2-1 confirmed and fixed.** Re-derived the review's claim directly
     (not by re-running its external `rustc` extraction, but by tracing
     `has_bench_internals_cfg`'s own logic): the 5-byte match `&bytes[i..i +
     5] == b"#[cfg"` matches the prefix of `#[cfg_attr(`, and
     `parse_cfg_inner` parses only the FIRST term of the parenthesised text
     that follows — for `#[cfg_attr(feature = "bench-internals",
     allow(dead_code))]` that is `feature = "bench-internals"`, which
     `requires_bench_internals` correctly reports `true` for, wrongly
     treating a non-gating `cfg_attr` predicate as a genuine gate. Fixed by
     requiring the literal 6-byte `#[cfg(` (open paren included), which
     structurally cannot match `#[cfg_attr(` (7th byte is `_`, not the
     paren the match now requires at position 6). Two new tests added:
     `has_bench_internals_cfg_rejects_cfg_attr_shape` (direct unit proof
     against the exact adversarial string, plus a regression guard that a
     genuine `#[cfg(feature = "bench-internals")]` is still accepted) and
     `scan_file_treats_cfg_attr_bench_internals_hook_as_ungated` (end-to-end
     proof via the real `scan_file` classifier against a synthetic
     `cfg_attr`-decorated hook fixture — confirms it surfaces as UNGATED,
     the correct conservative behavior, not silently accepted as gated). A
     third test, `no_dbg_hook_cfg_uses_cfg_attr_bench_internals_shape`,
     re-confirms (mechanically, going forward) the review's own finding that
     no CURRENT hook in `src/`/`crates/` uses this shape — this was a
     latent scanner gap, not a live false-accept, matching the review's own
     assessed severity.
   - **P2-2 confirmed and fixed.** Re-read `heap_core_diag.rs`'s
     `dbg_large_cache_hits` and confirmed the review's claim exactly: gated
     `#[cfg(feature = "alloc-decommit")]` alone (inside `production`), while
     its four siblings in the same file (`dbg_pool_cap`,
     `dbg_segment_state_reconciliation`, `dbg_large_cache_used`,
     `dbg_large_cache_slot_sizes`) are each gated `all(alloc-decommit,
     bench-internals)`. Tightened to match. Verified BOTH current callers
     (`examples/r30_6_large_cache_headroom_ab_gate.rs`,
     `examples/r31_1_large_cache_headroom_crossing_regime_gate.rs`) already
     list `bench-internals` in their `Cargo.toml` `required-features` — the
     review's own prediction ("nothing else should break") held, confirmed
     rather than assumed. Removed `"src/registry/heap_core_diag.rs::dbg_large_cache_hits"`
     from `tests/dbg_hook_safety_tripwire.rs`'s `PURE_OBSERVERS` list (a
     gated hook is tracked in neither allowlist — `scan_file` only feeds
     ungated hooks into the allowlist-diff check). Confirmed the hook is
     genuinely unreachable under plain `production` via a throwaway compile
     probe (`E0599: no method named 'dbg_large_cache_hits' found` when
     building against `--features production` alone, deleted after
     confirming) rather than assuming the `#[cfg]` change alone was
     sufficient proof. This also fixed the two doc-drift test failures item
     7's entry above flags: removing `dbg_decomp_release`'s TWO `unsafe fn`
     `#[allow(unsafe_code)]` item-scoped sites (one in `AllocCore`, one in
     `HeapCore`'s delegation — item 7's retrofit, not this item's gating
     change) dropped README's tier-2 count from 68 to 66; the new
     `tests/r31_4_reserved_small_segment_handle.rs` file brought
     `docs/ARCHITECTURE.md`'s tracked test-file count from 227 to 228. Both
     docs updated to match; `tests/no_stale_doc_references.rs` green.
   - **Verification (both P2-1 and P2-2 together):** `cargo test --features
     "production bench-internals alloc-stats"` green (230 test binaries, 0
     failed, including the 3 new P2-1 tests); `cargo test --features
     "bench-internals alloc-global alloc-xthread alloc-decommit fastbin
     alloc-segment-directory primordial-lazy-commit class-aware-dirty"
     --test dbg_hook_safety_tripwire` green (7 tests — confirms the
     tripwire's allowlist is accurate after the P2-2 gating change); `cargo
     clippy --features "production bench-internals alloc-stats"
     --all-targets -- -D warnings` clean; `cargo clippy --features
     production -- -D warnings` clean; `cargo fmt --check` clean.

9. **[T, filed 2026-07-31, UNVERIFIED-BY-ME findings from the Round 31 full
   independent review (`docs/reviews/2026-07-31-r31-full-review.md` §7
   P2-4, P2-5, P2-11, P2-12)]** The following four P2 findings were NOT
   independently re-verified before filing — flagged here at the review's
   own confidence/severity, for a future round to check and either action
   or dismiss, per this file's own convention (item 8 above is the direct
   precedent for this exact "filed, not fixed" pattern, one round earlier).
   Note: the review's P2-6 (`ReservedSmallSegment` should be `#[must_use]`)
   is NOT filed here — it was fixed directly in the same task that filed
   this item (one-line, zero-risk, per the task brief's own instruction to
   check first) — see the Round 31 review-response CHANGELOG entry.
   - **P2-4 — `ReservedSmallSegment`'s `pub(super)` scoping doc claim is
     wrong in three places.** The review's claim:
     `src/alloc_core/reserved_small_segment.rs:23-27` and `:80-85` say
     `new_from_reservation` is "callable only from within
     `alloc_core_small_pool.rs`'s own module tree," and `:108-112` says
     `into_base` is "not exposed outside this module tree" — both
     overstate. Actual scope is `pub(in crate::alloc_core)` (since
     `reserved_small_segment` is declared `pub mod` as a direct child of
     `alloc_core` in `src/alloc_core/mod.rs:99`), reachable from every
     sibling module under `alloc_core` (`alloc_core_large.rs`,
     `alloc_core_small.rs`, `alloc_core_small_magazine.rs`, …), not just
     `alloc_core_small_pool.rs` — Rust has no sibling-module-only
     visibility, so the stated scoping is not even expressible. The review
     states this is NOT a live exploit (whole-repo grep found exactly one
     caller of each) and the load-bearing property (external
     unforgeability across the crate boundary) is unaffected — a
     documentation-only defect. Suggested fix per the review (doc-only):
     "reachable from anywhere inside `alloc_core`; in practice called from
     exactly one site (`alloc_core_small_pool.rs:1095`). Rust has no
     sibling-module-only visibility, so this is the tightest expressible
     bound."
   - **P2-5 — the double-release counterfactual test has a cheap runtime
     check its own file's two-options analysis missed.** The review's
     claim: `tests/r31_4_reserved_small_segment_handle.rs` weighs exactly
     two options (`trybuild` vs. prose) for proving a compile-error
     property, but a third exists at zero cost:
     `assert!(core::mem::needs_drop::<ReservedSmallSegment>())` —
     `needs_drop` is callable at runtime, and a type with a `Drop` impl can
     never be `Copy` (a hard rustc rule), so combined with the file's
     existing by-value-signature exercise this is the complete
     compile-error argument, and unlike the prose it would actually FAIL if
     a future refactor removed `Drop` and added `Copy`.
   - **P2-11 — `AllocCore::dbg_large_cache_hits` remains a safe `pub fn` in
     a plain `production` build, unlike its `HeapCore`-level sibling R31-4
     tightened.** The review's claim, verified by its own out-of-tree
     compile probe: `AllocCore::dbg_large_cache_hits` compiles against
     `features = ["production"]` alone (R31-4/item 8 P2-2 above tightened
     only the `HeapCore` delegation, not this one). It is allowlisted in
     `tests/dbg_hook_safety_tripwire.rs`'s `PURE_OBSERVERS`
     (`:213`) and is a zero-argument `&self` counter read with no pointer
     and no mutation, so the review calls it a *sanctioned* exception under
     the tripwire — but notes CLAUDE.md's benchmark-hook rule 2 ("no
     production caller ⇒ MUST default to `bench-internals`") applies to it
     by the identical reasoning R31-4 used against its own sibling, and the
     R31-4 commit does not say why the pair was split. Suggested fix per
     the review: one sentence of justification, or a matching tightening
     to `all(alloc-decommit, bench-internals)`.
   - **P2-12 — the R31-4 retrofit narrowed tripwire coverage of the exact
     hook shape it hardened.** The review's claim: `scan_file`
     (`tests/dbg_hook_safety_tripwire.rs:814`) matches only `pub fn dbg_` /
     `pub unsafe fn dbg_`; the raw-pointer RETURN that used to live on
     `dbg_decomp_reserve_and_keep` (and was therefore scanned) now lives on
     `ReservedSmallSegment::base(&self) -> *mut u8`, a differently-named
     method the scanner's name-prefix match cannot see. The review calls
     this harmless today (`bench-internals`-gated; returns a pointer the
     caller already legitimately holds) but a coverage gap for the scanner
     going forward. Suggested fix per the review: rename to `dbg_base()`,
     or widen the scanner to also enumerate `#[doc(hidden)] pub fn`
     returning `*mut`/`*const` on measurement-only types.
   - **Next trigger:** independent re-verification of each sub-finding
     (re-read the `mod.rs` declarations for P2-4's visibility claim;
     confirm `needs_drop::<ReservedSmallSegment>()` for P2-5; re-run the
     review's out-of-tree compile probe for P2-11; re-read `scan_file`'s
     match logic for P2-12), then either apply the review's suggested fixes
     or record a reasoned dismissal, in a future round. None of these
     threaten correctness per the review's own text.
   - **Evidence:** `docs/reviews/2026-07-31-r31-full-review.md` §7 P2-4,
     P2-5, P2-11, P2-12 (the review's own text is the only source cited
     here — this entry is a filing, not an independent confirmation).

   **[FIXED, R31-14b/task #484, 2026-07-31.]**
   All four claims independently re-verified before fixing, per the "Next
   trigger" instruction above.

   - **P2-4 confirmed and fixed (doc-only).** Re-read `src/alloc_core/mod.rs`
     directly: `reserved_small_segment` is declared `pub mod` as a direct
     child of `alloc_core` (line 99), a SIBLING of `alloc_core_small_pool`
     (declared `mod alloc_core_small_pool` at line 22), not nested inside
     it — confirming `pub(super)` on `new_from_reservation`/`into_base`
     resolves to `pub(in crate::alloc_core)`, reachable from every module
     under `alloc_core`. Confirmed the single real caller via
     `grep -n "new_from_reservation\|into_base"
     src/alloc_core/alloc_core_small_pool.rs` → lines 1095 and 1117 exactly.
     Fixed all three overstated doc-comment locations
     (`reserved_small_segment.rs:23-27`, `:80-85`, `:108-112`) to state
     "reachable from anywhere inside `alloc_core`... Rust has no
     sibling-module-only visibility, so this is the tightest expressible
     bound," with the exact caller line numbers cited, matching the
     review's own suggested wording.
   - **P2-5 confirmed and fixed.** Re-read
     `tests/r31_4_reserved_small_segment_handle.rs` and confirmed it weighed
     exactly two options (trybuild vs. prose), no `needs_drop` check.
     Verified the runtime counterfactual independently: compiled a
     throwaway `struct NoDrop { x: *mut u8 }` (no `Drop` impl) and confirmed
     `core::mem::needs_drop::<NoDrop>()` returns `false` — proving the new
     assertion is non-vacuous (it WOULD fail if `ReservedSmallSegment` lost
     its `Drop` impl), not merely a decoration. Added
     `reserved_small_segment_needs_drop_so_it_cannot_be_copy` (a new `#[test]`
     asserting `core::mem::needs_drop::<ReservedSmallSegment>()`) plus a
     documented "option 3" in the file's module doc explaining the argument
     and citing this review finding.
   - **P2-11 confirmed; decision: keep as a sanctioned exception, add
     justification (not tighten).** Re-verified `AllocCore::dbg_large_cache_hits`
     (`src/alloc_core/alloc_core_large_cache.rs:544`) is gated
     `#[cfg(feature = "alloc-decommit")]` alone — reachable in plain
     `production`. Unlike its `HeapCore` sibling (R31-4/item 8 P2-2 above,
     which had ZERO callers outside `bench-internals`-gated examples before
     tightening), this method has genuine `#[test]` regression callers that
     run in a plain `production` test build without `bench-internals`:
     `tests/alloc_zeroed_fresh_large_skip.rs` and
     `tests/regression_large_cache_span_usable_stable.rs` both gate only on
     `#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]` and
     assert on this method's return value — confirmed by running
     `cargo test --features production --test alloc_zeroed_fresh_large_skip
     --test regression_large_cache_span_usable_stable`, both green.
     Tightening to `bench-internals` would break these two real test files.
     CLAUDE.md's benchmark-hook rule 2 ("no production caller ⇒
     `bench-internals`") does not apply here precisely because a production
     caller (the test binary) DOES exist, which is the deciding difference
     from the `HeapCore` sibling's case. Added a doc-comment paragraph to
     `dbg_large_cache_hits` explaining this asymmetry explicitly, so a
     future reader does not have to re-derive it.
   - **P2-12 confirmed and fixed.** Re-read `tests/dbg_hook_safety_tripwire.rs`'s
     `scan_file` (`:814`, `trimmed.starts_with("pub fn dbg_")`) and confirmed
     it structurally cannot match `pub fn base`. Renamed
     `ReservedSmallSegment::base` → `dbg_base` and updated all call sites
     (`tests/r31_4_reserved_small_segment_handle.rs` ×3,
     `examples/r29_3_decomposition_gate.rs` ×3, confirmed via a repo-wide
     `grep -rn "handle\.base()\|h2\.base()"` returning zero hits post-fix).
     The rename alone surfaced a SECOND, related gap the review did not
     flag: the tripwire scans the attribute block immediately preceding
     each `pub fn dbg_*` line, not the enclosing `impl` block's own `#[cfg]`
     — `dbg_base` was gated only at the `impl ReservedSmallSegment` level,
     so after the rename `cargo test --features "production bench-internals
     alloc-stats" --test dbg_hook_safety_tripwire` genuinely FAILED
     ("NEW unaccounted-for SAFE, non-bench-internals-gated hooks:
     ...::dbg_base") until a redundant per-method
     `#[cfg(all(feature = "alloc-decommit", feature = "bench-internals"))]`
     was added directly on `dbg_base` — confirming both that the tripwire
     genuinely works end-to-end and that repeating the gate per-item (the
     established pattern elsewhere in this crate, e.g.
     `heap_core_diag.rs`'s methods) is required, not optional decoration.
   - **Verification (all four together):** `cargo build --features
     "production bench-internals alloc-stats" --all-targets` clean;
     `cargo test --features "production bench-internals alloc-stats"` green
     (231 test-binary result lines, 0 failed); `cargo test --features
     production --test alloc_zeroed_fresh_large_skip --test
     regression_large_cache_span_usable_stable --test
     regression_large_cache_multi_size_cycle` green; `cargo clippy
     --features "production bench-internals alloc-stats" --all-targets -- -D
     warnings` clean; `cargo clippy --features production -- -D warnings`
     clean; `cargo clippy --features experimental --all-targets -- -D
     warnings` clean; `cargo clippy --all-features --all-targets -- -D
     warnings` clean; `cargo fmt --check` clean.

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

12. **[T, filed 2026-08-02, task #498] `xthread_large_double_free_no_double_reclaim`
    (`tests/regression_xthread_large_free_no_leak.rs`) failed once during a
    full `cargo test --features production` run, not reproduced on 7
    subsequent runs.** One full-suite run (during task #498's own
    verification pass) reported: `assertion `left == right` failed:
    expected exactly 50 reclaims (one per distinct double-freed segment),
    got 42` — a plausible cross-thread reclaim-counting race under system
    load (this test spawns real OS threads and races a remote double-free
    against the owner's deferred-free drain; see the test file's own module
    doc for the exact shape). NOT reproduced on: 5 consecutive isolated
    `--test regression_xthread_large_free_no_leak -- --test-threads=1` runs,
    1 full-suite re-run of the exact same tree that produced the original
    failure, and 1 full-suite run of the PRE-task-#498 base commit
    (`2dfeaa3`) in an isolated worktree (also clean) — i.e. this is not
    caused by task #498's diff (the base commit, entirely unmodified, was
    tested clean in the same session) and is not reliably reproducible
    on-demand, consistent with a genuine low-probability timing flake in
    the test's own concurrency shape rather than a real bug. Not
    investigated further here (out of task #498's scope; the task's own
    diff does not touch `heap_core.rs`'s deferred-free stack or
    `reclaim_large_segment`'s deposit/release logic — only the header
    WRITE inside the already-registered-or-not-yet-registered window, which
    this specific test's counter never observes). Filed per this file's own
    convention so a future round can watch for a repeat and, if one occurs,
    has this occurrence on record as the first data point.

    **Status: RESOLVED (2026-08-06, task #605/K10).** The above paragraph's
    own "the counter never observes this window" reasoning was wrong — not
    about THIS test's immediate window, but about state carried forward
    from an EARLIER test in the same process via the large-cache. Root
    cause identified with full confidence, not merely hypothesized: task
    #498's own commit `eb2463a` ("large-cache HIT arm writes 4 SegmentHeader
    fields instead of the whole 144-byte struct") replaced a full-struct
    header rewrite on large-cache reuse with 4 targeted field writes
    (magic/large_size/large_align/bump), silently dropping the implicit
    reset of `owner_state`/`owner_thread_free`/`deferred_next` the old
    full-struct write used to perform. A segment that had gone through the
    cross-thread deferred-free path (as several do in this file's OTHER
    tests, `xthread_large_free_reclaims_segments_no_leak` in particular,
    which runs earlier in the same serialized test binary) retains a
    non-`ABANDONED_TAIL` `deferred_next` link value; when the large-cache
    later hands that same segment back out as a "fresh" allocation (a cache
    hit) for THIS test's first loop, and the remote thread subsequently
    frees it, `push_large_deferred_free`'s double-push claim CAS (which
    requires the link word to read `ABANDONED_TAIL`) fails on the FIRST
    free attempt — not the second, deliberate double-free — silently
    dropping that segment from the deferred-free stack entirely. Each
    dropped segment is one fewer reclaim than expected: exactly the
    "got 42, not 50" undercount symptom, for however many of the 50
    allocations happened to land on a stale cache hit in that run.

    This defect was independently found and fixed two days later by an
    unrelated task — R34-14/task #533, commit `7ef5a465cc23e20c518f9163520640aebc7a7ee0`
    ("reset owner/deferred fields on large-cache hit") — whose own commit
    body describes the identical mechanism verbatim ("a segment that went
    through the deferred-large-free path retains a non-`ABANDONED_TAIL`
    link value ... push_large_deferred_free's CAS from `ABANDONED_TAIL`
    FAILS") and ships a dedicated counterfactual regression test,
    `tests/r34_14_deferred_next_reset_on_cache_hit.rs`, that reproduces
    the silent-drop with the reset removed and passes with it restored.
    Nobody connected that fix to closing THIS item at the time — R34-14 was
    framed entirely around its own symptom (a permanent leak), not this
    flake.

    Verified, not merely inferred: (1) `git merge-base --is-ancestor
    7ef5a46 HEAD` confirms the fix is an ancestor of current `HEAD`; (2)
    `cargo test --release --test regression_xthread_large_free_no_leak
    --features "production internals" -- --test-threads=1
    xthread_large_double_free_no_double_reclaim` run 5 consecutive times,
    all green; (3) `cargo test --release --test
    r34_14_deferred_next_reset_on_cache_hit --features "production
    internals"` — the dedicated counterfactual — passes on current `HEAD`.
    No further action needed; this item required no NEW fix, only
    identifying that an already-landed one (for a differently-described
    symptom) already closed it.


14. **[T, filed 2026-08-02, task #499] Flaky (pre-existing, NOT caused by
    task #499's changes) —
    `tests/regression_xthread_large_free_layout_mismatch.rs`'s
    `xthread_large_free_tiny_size_huge_align_is_reclaimed` fails when run as
    part of its own 5-test file (`cargo test --test
    regression_xthread_large_free_layout_mismatch`, default parallel test
    threads) but passes reliably when run in isolation
    (`... xthread_large_free_tiny_size_huge_align_is_reclaimed`, single
    test). Failure shape: `a legitimate tiny-size/huge-align cross-thread
    free was NOT reclaimed (delta 0)` — `DBG_LARGE_XTHREAD_RECLAIMED` did
    not advance the expected amount, at `tests/regression_xthread_large_free_layout_mismatch.rs:334`.
    **Confirmed pre-existing and unrelated to task #499's `maybe_decay_large_cache`
    stride-throttle change:** reproduced identically (same failure, same
    line) on a clean `git worktree add` at commit `48fed64355f03181c6a89f42cab636b800994c7f`
    (the commit immediately BEFORE task #499's changes) with its own
    isolated `CARGO_TARGET_DIR`, ruling out both task #499's own diff and
    cross-contamination from other agents' concurrent builds in this shared
    workspace as the cause. The test uses `SerialGuard::acquire()` (a
    `TEST_LOCK`-style serialization primitive, per this file's own item-13
    citation of the same pattern) but the failure's within-file-only
    reproduction (5/5 runs failed when run with its siblings; 3/3 runs
    passed in isolation, `cargo test ... regression_xthread_large_free_layout_mismatch`
    invoked 3 times back-to-back) points at test-order or shared
    process-wide-counter (`DBG_LARGE_XTHREAD_RECLAIMED` is itself a
    process-wide static, per the test's own imports) interaction with a
    sibling test in the same binary, not a genuine reclaim-logic regression.
    **Not root-caused further** (which sibling test's ordering/timing
    causes the interaction, and whether `SerialGuard` has a gap) — filed
    here so a future round investigating cross-thread reclaim correctness
    or CI flakiness in this file starts from "already reproduced as
    pre-existing, isolated-run-clean" instead of re-diagnosing from
    scratch.

_(item 35 (renumbered from a collision, task #623/M2 — see that item's own
history for the prior "15"/"16" mislabel), the F-2 provenance-asymmetry
hypothesis, was resolved-negative by R34-5 (task #524) — see "Recently
resolved" below.)_

16. **[T, filed 2026-08-04, R34-2/task #521] Cross-thread routing's documented
    residual (caller-contract-violation surface) needs to reach the release
    notes (`docs/reviews/2026-08-04-release-stabilization-audit.md` F-3 [low]).**
    `dealloc_foreign_routing` (`src/registry/heap_core_xthread.rs:858-1007`)
    reads and writes foreign segment memory under a "magic != 0" guard only;
    the code documents honestly (`:864-885`) that a live-foreign vs
    already-released segment is O(1)-indistinguishable, so a double free of a
    released segment is "fundamentally UB … not fixed by this change" — the
    standard caller-contract residual every allocator has. The action item is
    NOT a code fix (none is needed — for a single legitimate cross-thread free,
    `live_count ≥ 1` until the owner's drain reclaims, so the segment cannot be
    released underneath the freer); the action is to **state this residual in
    the release notes** so a downstream reader knows the documented limitation.
    Filed because no Round-34 task owns release-notes writing.

    **Status: RESOLVED (2026-08-05, task #597/K2, commit `f43600d`).** The
    exact action this item requested — a release-notes statement of the
    residual — landed in `CHANGELOG.md`'s new "Known limitations (as of
    this release)" subsection. Left in place rather than moved to "Recently
    resolved" / renumbered (that structural cleanup, spanning several
    pre-existing item-numbering gaps in this file, is task M2/#623's
    broader scope, not duplicated here item-by-item).

17. **[T, filed 2026-08-04, R34-2/task #521] Five tier-1 `unsafe` seams have
    no miri, no loom, and no kani harness — covered by ordinary integration
    tests only (`docs/reviews/2026-08-04-release-stabilization-audit.md` G3
    [medium]).** The five: `global::sefer_alloc` (the `unsafe impl GlobalAlloc`
    itself), `global::fallback` (the `static mut MaybeUninit<HeapCore>` +
    spinlock), `registry::heap_slot` (the single load-bearing `unsafe impl
    Sync` in the crate), `alloc_core::sidecar` (the shared lazily-materialised
    sidecar deref boundary, on the `production` path via
    `alloc-segment-directory` + `class-aware-dirty`), and
    `alloc_core::large_cache_extended`. Additionally, `alloc_core::dirty_by_class`
    has `loom_class_aware_dirty` but per ci.yml's own note that model uses
    hand-rolled `loom::sync` atomics, not the real `PerClassDirty`/`RacyPtrCell`
    types — so the real sidecar deref is unmodelled there too. For a
    stabilization release, adding at least a miri or loom harness to each
    (especially `sidecar`, which is on the `production` path) closes the
    largest remaining verification-coverage gaps.

    **Status: PARTIALLY RESOLVED (2026-08-06, task #606/K11) — 2 of 5 claims
    corrected, 2 real CI-wiring gaps closed, 2 seams remain genuinely
    uncovered (accepted risk, see below).**

    - **`registry::heap_slot`'s claim was already stale when filed.**
      `tests/regression_xthread_thread_free_alias_miri.rs` (its own header
      comment: "`Sync` `HeapSlot`, mirroring W3) is required") already
      exercises the exact `unsafe impl Sync for HeapSlot` this item names,
      under real cross-thread miri, and was already wired into
      `ci.yml`'s `miri-plain` job (line ~973) before this item was even
      filed. No action needed — the original claim was simply wrong.
    - **`alloc_core::sidecar` / `alloc_core::large_cache_extended` — real
      gap, partially closed.** `tests/segment_directory_a5_miri.rs` (R7-A5's
      own miri target) already existed, already passed, and genuinely
      exercises the shared `alloc_core::sidecar::OwnedSidecar` primitive
      (via `os::reserve_directory_sidecar`/`deref_directory_sidecar`, which
      call `sidecar::reserve_zeroed_with`/`sidecar::deref` directly — same
      primitive `large_cache_extended.rs` calls) — but was never wired
      into any CI job, so it never actually ran. Wired into `miri-core`
      as a new step (commit `4dd0624`). Residual gap, explicitly NOT
      closed: this test is BELOW-threshold only (`table.count() < 32`,
      the sidecar never actually materialises) — the test's own header
      comment explains why the full materialised path (reserve, rebuild,
      lookup, set/clear bits, 32+ segments) is impractically slow under
      miri and is instead covered only by NATIVE tests
      (`segment_directory_a1.rs`/`_a2.rs`/`_a3.rs`/`_a5.rs`/`_a5_proptest.rs`).
      The materialised-path `sidecar::reserve`/`deref` calls themselves —
      the actual UB-sensitive boundary — remain unproven under miri.
      Writing a miri-tractable materialised-threshold test (a lower
      test-only threshold, or a direct unit-level `OwnedSidecar` miri test
      that bypasses the 32-segment precondition entirely) is real
      follow-up work, not attempted here.
    - **A second, unrelated CI-wiring gap found and closed in the same
      pass**: `tests/remote_fanin.rs`'s `remote_fanin_miri_minimal_retry_ub_check`
      (a purpose-built minimal miri UB-detection harness for
      `push_with_overflow_retry`'s retry path, per its own doc comment
      "Harness 3: minimal miri UB-detection target") also existed, already
      passed, and was also never wired into any CI job. Wired into
      `miri-core` as its own step (commit `4dd0624`) — kept separate from
      the pre-existing `reclaim_offset_unit` step rather than combined
      with a positional test-name filter, after that combination was tried
      first and found to silently zero `reclaim_offset_unit`'s own test
      out of its run ("0 passed ... 1 filtered out") — the exact
      false-PASS shape `miri-core`'s own header comment already documents
      from a prior incident (a bare positional filter matching nothing
      while still reporting green). Caught before landing, not shipped.
    - **`global::sefer_alloc` (the `unsafe impl GlobalAlloc` boundary
      itself) and `global::fallback` (the `static mut MaybeUninit<HeapCore>`
      plus spinlock) — genuinely zero miri/loom/kani coverage, confirmed by
      direct grep across `src/` and `tests/`, ACCEPTED AS RESIDUAL RISK for
      this release rather than closed.** Both are exercised extensively by
      ORDINARY (non-miri/loom) integration tests (`tests/global_alloc.rs`,
      `tests/global_alloc_mt.rs`, `tests/global_alloc_installed.rs`, and
      indirectly by the whole test suite, since `SeferAlloc` is the
      `#[global_allocator]` under `--features production`) — functional or
      logic bugs in these paths would be caught. What miri/loom
      specifically add beyond that — Stacked/Tree Borrows aliasing
      violations, data races invisible without a memory model, the exact
      class of bug `heap_slot`'s own dedicated test above was written to
      catch for a DIFFERENT boundary — remain unproven here. Rationale for
      accepting this rather than blocking release: (a) `global::sefer_alloc`'s
      own trait impl is a thin TLS-lookup-and-dispatch wrapper (the heavy
      unsafe logic it delegates to — `HeapCore::alloc`/`dealloc` — already
      has substantial miri coverage via `reclaim_offset_unit`,
      `decommit_miri_cycle`, and now
      `remote_fanin_miri_minimal_retry_ub_check` above); (b)
      `global::fallback`'s pre-TLS/post-teardown windows are, by the
      module's own doc comment, rare and effectively single-threaded in
      practice, narrowing the real-world UB surface relative to the hot
      per-thread path. Writing dedicated miri/loom harnesses for both
      remains real, valuable follow-up work, not attempted here — this
      status update is the explicit "record the accepted residual risk"
      resolution K11's own filing offered as an alternative to full
      harness-writing.

18. **[T, filed 2026-08-04, R34-2/task #521] kani proves only the smallest
    seam and a deprecated tier — two highest-value CBMC-reachable properties
    are unproven (`docs/reviews/2026-08-04-release-stabilization-audit.md` G4
    [low]).** `src/kani_proofs.rs` covers `alloc_core::node` primitives and
    `concurrent::hand` (the research tier). The two unproven high-value
    properties are: (a) the ring's wrap arithmetic — that
    `t.wrapping_sub(h) < RING_CAP` is an invariant of the push/drain pair
    across the `u32::MAX → 0` boundary; and (b) `pack_entry`/`unpack_entry`
    (both hardened and non-hardened packings) round-trip and never produce
    `RING_SLOT_EMPTY` over the full real input ranges. Both are pure
    arithmetic with no pointers — ideal kani targets — and both are currently
    protected only by unit tests plus `const _: () = assert!` on the *bounds*,
    not on the *round trip*.

    **Status: RESOLVED (2026-08-06, task #611/K16, commit `772b36d`).** Both
    (a) and (b) now have real, verified Kani proofs in `src/kani_proofs.rs`:
    `ring_wrap_proofs` (2 harnesses, generalising
    `tests/regression_ring_cursor_wrap.rs`'s hand-picked wrap-boundary values
    into an exhaustive proof over every `u32` head and every occupancy
    `0..=RING_CAP`) and `ring_entry_pack_proofs` (4 harnesses: round-trip +
    `RING_SLOT_EMPTY`-never-collides, for both the non-hardened and
    `hardened`-only packings). All 6 verified via a real `cargo kani` run
    (kani-verifier 0.67.0 under WSL2 — Kani does not support Windows at all,
    confirmed: `kani-verifier` fails to even compile under
    `x86_64-pc-windows-msvc`) and one counterfactually confirmed non-vacuous
    (a deliberately injected off-by-one bug was caught as `FAILURE`, then
    reverted and reverified `SUCCESS`).

    **Also discovered and fixed in the same task**: Kani had NEVER been
    wired into any CI job before this — the 13 pre-existing proof harnesses
    in `src/kani_proofs.rs` (`node_proofs`, `hand_proofs`, `pack_proofs`)
    were never continuously re-verified either, only run by hand at
    authoring time. Added a new `kani` CI job running all 19 harnesses
    (13 pre-existing + 6 new) per-PR — measured at ~30s total, comparable to
    this workflow's existing miri jobs.

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

41. **CLOSED** by task #1057 (dedicated per-PR `aligned-vmem-miri` CI job added). See "Recently resolved" below for the full closure narrative.

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

44. **Deferred verification — `numa-shim`'s mbind path (`lib.rs:531`, the
    crate's key selling point) has no behavioral oracle anywhere.** (Filed
    2026-08-09, task #778/F4, round-closing review of the numa-shim round —
    a distinct §D1a MEDIUM audit finding from the cpumap-parser one task
    #721 closed; #721's own commit message declines this half explicitly
    but only in commit prose, exactly the failure mode this index exists to
    prevent.)

    - **Status:** OPEN — no test on this repository's own CI asserts the
      `mbind(2)` syscall this crate wraps actually succeeds or has the
      documented effect.
    - **Current-number-or-verdict:** `bind_range_impl_linux`'s `mbind`
      return value is silently discarded by design; every test suite that
      touches this path asserts only no-panic (`tests/smoke.rs`) or that a
      `MockCall::BindRange` record was emitted (`tests/mock_dispatch.rs` —
      a declaration, not a behavioral proof). Mutating `SYS_MBIND` or
      scrambling the argument marshalling would leave every current test
      green.
    - **Evidence:** `docs/reviews/2026-08-07-numa-shim-rust-intel-audit.md`
      §D1a (`lib.rs:531`); confirmed still true by
      `docs/reviews/2026-08-09-numa-shim-round-closing-review.md`, which
      re-checked this specific gap during the round-closing review and
      found no test filled it in the round's 9 commits.
    - **Next trigger:** add an env-guarded Linux test (the weekly
      `numa-real-kernel` CI job is the natural home) asserting the
      `mbind(2)` syscall return is `0` for a valid single-node bind (a
      wrong syscall number yields `-1`/`ENOSYS` and goes red) and/or a
      `get_mempolicy(2)` readback asserting `MPOL_PREFERRED` with the
      expected nodemask — this would also be the only test capable of
      catching a future `maxnode`/marshalling regression of the exact
      shape task #697 fixed.

45. **`numa-shim`'s `CURRENT_NODE_SLOT: RefCell<u32>` where a `Cell<u32>`
    would do, and its accessor still uses a panicking `borrow_mut()`.**
    (Filed 2026-08-09, task #778/F4, round-closing review — audit §A2,
    INFO, left untouched by task #726's visibility narrowing.)

    - **Status:** OPEN — cosmetic/defensive, not a live bug.
    - **Current-number-or-verdict:** `crates/numa-shim/src/lib.rs`'s
      `CURRENT_NODE_SLOT` thread-local is `RefCell<u32>`; `Cell<u32>` would
      be strictly sufficient (only ever `get`/`set` a `Copy` value) and
      would structurally rule out the §B17 reentrant-borrow hazard this
      same module documents and defends against for its sibling `CALLS`
      cell (`record()`'s `try_borrow_mut`) — `set_current_node` still calls
      a PANICKING `RefCell::borrow_mut()` (`crates/numa-shim/src/lib.rs:149` as
      of task #726), not the `try_borrow_mut` pattern its sibling was
      deliberately given.
    - **Evidence:**
      `docs/reviews/2026-08-07-numa-shim-rust-intel-audit.md` §A2.
    - **Next trigger:** low priority — fold into any future edit that
      touches the `mock` module's thread-locals (e.g. a future `CALLS_CAP`
      follow-up, item 46's public-surface work, or a `mock` API revision
      before first publish, task #657).

46. **CLOSED** by task #1053 (option (a): coupling accepted and documented,
    plus a `pub use aligned_vmem::Reservation;` re-export). See "Recently
    resolved" below for the full closure narrative.

47. **`numa-shim`'s entire round (tasks #697/#720-727) is REASONED-FROM-SPEC
    for its Linux-only code, never empirically executed on this session's
    host.** (Filed 2026-08-09, task #778/F4, round-closing review —
    `aligned-vmem`'s round filed the analogous gap as item 43; `numa-shim`'s
    had no counterpart until now.)

    - **Status:** OPEN — no action needed unless a Linux runner with
      `#[global_allocator]`-installed test binaries becomes available;
      filed so the gap is visible rather than silently load-bearing.
    - **Current-number-or-verdict:** tasks #697 (`mbind` `maxnode`
      arithmetic), #720 (cpumap loop-to-EOF read), and #723/#777 (the
      `OnceLock`-based topology cache and its allocation-free redesign) are
      all `#[cfg(all(target_os = "linux", not(miri)))]`-gated and have
      NEVER executed on this session's Windows host — verified only via
      `cargo check`/`clippy --target x86_64-unknown-linux-gnu` (confirms
      the code COMPILES and type-checks, not that its runtime behavior
      matches the stated reasoning) plus careful manual derivation from
      kernel/API documentation. This is not hypothetical risk: task #777
      itself exists because task #723's REASONED-FROM-SPEC design had a
      real defect (a reentrancy deadlock) that compiled cleanly, passed
      every test this session could run, and was only found by a
      round-closing review reasoning about a deployment scenario
      (`#[global_allocator]` + `numa-aware` on real Linux) this session
      cannot construct.
    - **Evidence:**
      `docs/reviews/2026-08-09-numa-shim-round-closing-review.md` §5 (the
      review's own explicit confirmation that the verification-honesty
      distinction was maintained consistently, which is a STATEMENT about
      what was labeled correctly, not a substitute for the missing
      execution); the weekly `numa-real-kernel` CI job (`.github/workflows/ci.yml`)
      exercises real Linux but its test binaries do not install
      `#[global_allocator]` (grep-verified), so it cannot reproduce a
      reentrancy scenario like the one #777 fixed even though it does run
      on real Linux hardware.
    - **Next trigger:** if/when this repo gains a Linux CI runner (or a
      local `crush`/agent session with Linux execution access) capable of
      running `cargo test -p numa-shim --all-features` AND a real
      `#[global_allocator] = SeferAlloc` + `numa-aware` allocation
      workload together, use it to (a) empirically confirm #697/#720's
      REASONED-FROM-SPEC fixes behave as derived, and (b) add the
      integration-level regression test item 44 above also asks for —
      both share the same missing infrastructure.

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

48. **`aligned-vmem`'s `decommit()` silently fails to release physical memory (or zero-fill on `recommit`) on macOS — `MADV_DONTNEED` is advisory-only for anonymous memory on Darwin, unlike Linux.** First confirmed as a REAL, failing test (not just a documented risk) by CI on 2026-08-13, the FIRST time this crate's real (non-mock, non-miri) test suite ever ran against real macOS CI — round 4 (task #867/R1) added the CI row that finally exercises the real macOS backend instead of the `mock` stub, but the push to `origin/main` was deferred for two more rounds, so this is the first time the row actually executed. `decommit_recommit_roundtrip` (`crates/aligned-vmem/tests/smoke.rs`) failed: a byte written before `decommit`+`recommit` (`0x77` = 119) was still present after the cycle, where Linux/Windows both correctly read back `0`. **The underlying hazard itself was NOT newly discovered — it was already known repo-wide since Round 9** (see "Prior knowledge" below); only this extracted crate's own docs/tests had never reflected it until the fix commit below. `9c777bc`'s commit message calling this "a real, previously-undiscovered functional gap" is accurate only about this crate's own docs/tests, not about the repository as a whole — corrected here (round 6, task #883) after an independent review flagged the overstatement.
    - **Prior knowledge (repo-wide, pre-dating this "discovery" by multiple rounds):** the exact same hazard — Darwin `MADV_DONTNEED` being advisory/lazy with no zero-fill guarantee — was already documented in at least four places before this item was filed: `.github/workflows/ci.yml` (the `test-macos` job's own comment above its aligned-vmem test rows: "MADV_DONTNEED on Darwin is advisory/lazy (no zero-fill guarantee)" — re-anchored by content at task #1060, since the workflow's growth had rotted the old line reference); `src/alloc_core/alloc_core_small_pool.rs` (a production code comment, currently around lines 1002-1021, stating the same fact as the load-bearing risk area for the `virgin-zero-skip` feature); and two `virgin-zero-skip` design docs, `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` (around lines 115-116 and 358) and `docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` (around line 32), whose entire safety argument is built on this fact. The honest story is: the repo knew this when `aligned-vmem` was extracted from `src/alloc_core/os.rs`, the extraction lost that knowledge, and CI finally made the gap fail loudly rather than "discovering" something new.
    - **R9_5 mis-citation, also corrected here:** `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:115-116` cites "`crates/aligned-vmem/src/lib.rs` §decommit note" as its source for this fact. `git log --oneline -S "advisory" -- crates/aligned-vmem/src/lib.rs` shows that note was created BY commit `9c777bc` (dated 2026-08-13) — i.e. R9_5's citation was unverifiable/forward-referencing a note that did not exist when R9_5 was written (2026-07-20). It is now accurate as of `9c777bc`, purely by coincidence of the fix landing there. See the one-line notes added to both design docs (R9_5 near lines 115-116/358, R11_8 near line 32) in this same task. **Post-split update (task #1060, 2026-08-17):** the cited "`crates/aligned-vmem/src/lib.rs` §decommit note" — `decommit()`'s rustdoc Darwin caveat — now lives in `crates/aligned-vmem/src/api/decommit.rs` (its "Darwin zero-fill gap" paragraph) after task #1055 (commit `a4b8e50`) split the former monolith; the `git log` quotation above is left as written because it accurately describes the pre-split tree it was run against.
    - **Status:** OPEN — mitigated across more surface than the original `9c777bc` fix covered. As of round 6 (tasks #880-886): the test is scoped to not assert the false guarantee on the Darwin family (macOS/iOS/tvOS/watchOS); `decommit()`'s rustdoc, `recommit()`'s rustdoc, `decommit_lazy()`'s rustdoc, the crate-root module doc, `recommit_pages_impl`'s code comment, AND `README.md`'s new "Platform caveats" section all carry a consistent Darwin-scoped caveat (task #880/S1, task #881/S5, task #885/S7); the empirical root-cause oracle (task #882/S2) has now run on real macOS CI and settled the H1-vs-H2 question (see Root cause below, updated round 7 / task #888); `decommit_lazy_roundtrip`'s own vacuousness (S4's second limb) is recorded below, not yet fixed. The underlying functional gap itself is NOT fixed. `decommit()`'s core purpose — "return page-granular physical backing to the OS" — is silently unmet on the Darwin family for ordinary (non-huge) reservations: RSS does not decrease, and re-access after `recommit` returns stale data instead of a fresh zero page.
    - **Current-number-or-verdict:** confirmed via real CI (`test macos (production)` job, run `31676133649`, landing SHA `e60e46a`) — deterministic, not flaky (byte value matches exactly what was written before decommit). Linux (`aligned-vmem package gates`, `test workspace members`) and Windows (`test windows (production)`) both passed the same assertion in the same run, confirming the guarantee genuinely holds on those two platforms and the gap is Darwin-specific.
    - **Root cause:** `recommit_pages_impl`'s Unix implementation (`crates/aligned-vmem/src/os/unix.rs` — its home since the task #1055 split, re-verified at task #1060; `#[cfg(all(unix, not(miri)))]`) is an unconditional no-op for ALL Unix platforms, justified by a comment claiming "re-access after MADV_DONTNEED is implicit — fresh zeroed pages on demand" — true on Linux, false on the Darwin family. `decommit`'s own eager path calls `madvise(MADV_DONTNEED)` uniformly across all Unix too. **This explanation was ASSERTED, not ESTABLISHED, when first written (task #882):** it was inferred from a single failing byte, which was equally consistent with a different hypothesis — the `madvise(2)` syscall itself FAILING on that CI runner for an unrelated reason (H2), since `libc_madvise` discards `madvise`'s return value by design (task #719) and nothing in the crate could previously distinguish "syscall succeeded but Darwin's semantics didn't reclaim the pages" (H1) from "syscall itself failed" (H2). Task #882 added an empirical oracle: under the `bench-internals` feature, `libc_madvise` now also records attempt/success counts into two new `#[doc(hidden)]` statics, `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` (accessors `aligned_vmem::unix_madvise_attempts()`/`unix_madvise_successes()`, reset via the existing `reset_bench_internals_counters()`), and a new macOS-gated test, `macos_decommit_madvise_syscall_actually_succeeds` (`crates/aligned-vmem/tests/smoke.rs`), that asserts the `madvise` syscall itself returns success (`0`) for BOTH the eager (`decommit`, `MADV_DONTNEED`) and lazy (`decommit_lazy`, `MADV_FREE_REUSABLE`) call sites. **Updated round 7 (task #888, finding T1):** commit `1dbd6b4` was pushed and CI run `31692217669` (job `94421845398`, `test macos (production)`, image `macos-26-arm64`) ran green — `macos_decommit_madvise_syscall_actually_succeeds` passed, with `unix_madvise_attempts() == 2 && unix_madvise_successes() == 2`, ruling out H2 (the syscall itself did not fail). **This does NOT by itself confirm H1** — the H1 argument has two halves observed in TWO DIFFERENT CI runs: the stale-byte evidence (`decommit_recommit_roundtrip`'s pre-scoping failure) comes from run `31676133649`/commit `e60e46a`, before that assertion was scoped off Darwin, while the madvise-success evidence above comes from run `31692217669`/commit `1dbd6b4`; no single run has observed both the stale byte AND the successful `madvise` syscall in the same process. **Correct wording: H2 is ruled out by run `31692217669`; combined with run `31676133649`'s stale byte, H1 (advisory-only semantics) is the only remaining explanation** — NOT "H1 confirmed by CI".
    - **Darwin lazy-path alternative fix (round-6 review S9, spec-read, not verified on hardware, not a recommendation to implement without further review):** three connected observations. (1) On Darwin, `decommit_lazy` issues `MADV_FREE_REUSABLE` but nothing in this crate ever issues the paired `MADV_FREE_REUSE` before re-touching pages (`recommit_pages_impl`'s Unix implementation is an unconditional `Ok(())`, confirmed by reading it) — Apple documents these as a required pair, so this is a physical-footprint-accounting-drift concern (not a memory-safety issue), distinct from this item's own eager-`decommit` finding. (2) `decommit_lazy`'s own rustdoc describes the general "lazy is cheaper, reclaimed only under pressure" ordering (Linux `MADV_FREE` semantics), but on macOS/iOS specifically that ordering is INVERTED on the RSS axis: `MADV_FREE_REUSABLE` drops footprint immediately there, while eager `decommit`'s `MADV_DONTNEED` drops nothing at all — the opposite of the general case (on tvOS/watchOS, `decommit_lazy` falls back to the same `MADV_DONTNEED` as the eager path, so there both are equally no-ops). (3) Because of (2), a cheaper but PARTIAL alternative to the `MAP_FIXED` re-map idea below exists and is worth recording: route macOS/iOS's eager `decommit` to `MADV_FREE_REUSABLE` and issue `MADV_FREE_REUSE` from `recommit` — this would close the "return physical backing to the OS" half of `decommit`'s promise on macOS/iOS but NOT the "reads as zero" half (since `MADV_FREE_REUSABLE` preserves contents if the pages are re-touched before reclaim). **tvOS/watchOS coverage (round 7, task #895, TC3 — synchronized with `decommit_lazy`'s rustdoc and `madv_free_advice`'s doc comment, which this bullet must keep agreeing with if either changes):** this crate's cfg currently only names macOS/iOS for `MADV_FREE_REUSABLE`, so as written this alternative would not extend to tvOS/watchOS — but `MADV_FREE_REUSABLE`'s value comes from XNU, the kernel all four Darwin targets share, so it MAY work identically there too; this is REASONED-FROM-SPEC, not verified on tvOS/watchOS hardware or a tvOS/watchOS build target (neither is available to this crate's CI), not an established "no `MADV_FREE_REUSABLE` there" fact. Only re-mapping is confirmed to close both halves on all four targets; the lazy-path alternative's tvOS/watchOS coverage is an open question, not a settled no.
    - **Next trigger:** a future round should implement a real Darwin fix — the standard technique is re-`mmap`(`MAP_FIXED | MAP_ANONYMOUS`) over the decommitted range instead of (or in addition to) `madvise`, which forces the kernel to actually replace the mapping with fresh zero pages; needs its own safety analysis (interaction with concurrent access to the same reservation, `is_huge()` state, the existing `huge_pages` feature's `MAP_HUGETLB` path) and its own review round rather than a rushed fix under a CI-green-checking task. Until then, `decommit()`'s Darwin-family behavior should be treated as "hint only, no RSS/zero-fill guarantee" — the same posture already documented for the huge-page case.
    - **S4 remainder (round-6 closing review SC1, partially fixed):** the round-6 review's S4 finding had two limbs — "macOS lost its only decommit effect-oracle" (closed by task #882's new counters/test above) and "`decommit_lazy_roundtrip` (`crates/aligned-vmem/tests/smoke.rs`) is vacuous on EVERY platform, not just macOS" (still not fixed — that test only checks a post-recommit write/read round-trips, never whether `madvise` had any effect; its rustdoc previously claimed otherwise, corrected in the closing pass). The new oracle's counters are `unix`-wide, not macOS-specific (`libc_madvise` is `#[cfg(all(unix, not(miri)))]`), so the same assertion style would close the Linux half too — but no CI row currently runs `bench-internals` against the real (non-mock) Unix backend on Linux (the Linux rows in `ci.yml` are default-features, `--all-features` which turns `mock` on, or `fault-injection lazy-commit` without `bench-internals`). Closing this fully needs either a new Linux CI row or accepting the gap stays macOS-only for now. **Stale premise corrected (task #1060, 2026-08-17):** the sentence above described the pre-task-#962 CI, when `mock` was still a Cargo feature and `--all-features` selected the mock backend. Since task #962 (`mock` became the `--cfg aligned_vmem_mock` build flag), the `aligned-vmem-gates` job's `cargo test -p aligned-vmem --all-features` row on ubuntu-latest — and `test-workspace`'s aligned-vmem `--all-features` row — run `bench-internals` against the REAL Unix backend on Linux, so the "new Linux CI row" this bullet asked for already exists. What is still missing is only a Linux-side oracle TEST: the existing oracle (`macos_decommit_madvise_syscall_actually_succeeds`, `crates/aligned-vmem/tests/smoke.rs`) is `target_os = "macos"`-gated, and its own doc comment still repeats the stale no-Linux-row premise — a source file, outside this doc-only task's reach.
    - **Evidence:** CI run `31676133649` (`gh run view 31676133649 --json jobs`), job `test macos (production)`, step "Run cargo test -p aligned-vmem --features ... --no-fail-fast", failure at `crates/aligned-vmem/tests/smoke.rs:174` (pre-fix line number) — `assertion left == right failed: recommitted page must be zeroed / left: 119 / right: 0`; fixed in commit (this task's own commit, landing after `e60e46a`). Discovery-framing and mis-citation correction: `docs/reviews/2026-08-13-aligned-vmem-round6-review.md` finding S3 (task #883). Round-6 closing review: `docs/reviews/2026-08-13-aligned-vmem-round6-closing-review.md`, findings SC1-SC10.

49. **CLOSED** by task #997 (P3-8 pass 2). See 'Recently resolved' below for the full closure narrative.

50. **`aligned-vmem` — `page_size()`'s OWN end-to-end wiring is untestable in-process (the extracted pure guard IS tested).** (Filed round 8, task #903, finding U11 of `docs/reviews/2026-08-13-aligned-vmem-round8-review.md`. The U10 half — Windows `bench-internals` reserve-path counters — was closed per task #917: see "Recently resolved" §50-U10 below.)
    - **Status:** OPEN — record-only; no code fix in scope for either half (see each half's own note on why a cheap fix is not appropriate here).
    - **Headline corrected by task #1061 (2026-08-18).** The card was titled "the rejection branch has never executed anywhere" until now. That was FALSIFIED and had been for months: task #949/T-5 extracted the pure guard as the public `validate_page_size`, and `validate_page_size_falls_back_on_invalid_values` (`crates/aligned-vmem/tests/smoke.rs:645`) drives the REJECTION branch three times per run — `0`, `5`, `2048` — in every CI row that builds `bench-internals`. Task #1060's R34-24 audit recorded the supersession inside the bullets below but left the headline standing, so the card still READ as an untested-guard item; per this repo's own current-state-index rule a stale header is itself the defect. What remains genuinely open is narrower and is what the title now says: `page_size()`'s own wiring (private query fn, `PAGE_SIZE_CACHE` latched process-wide on first call) cannot be re-exercised in-process **through its own default build** — see the task #1143 correction below for the one qualification to "cannot be re-exercised".
    - **Correction (task #1143, 2026-08-19): "no injection seam" is FALSE as an unqualified claim — a seam was added one round after task #1061 wrote that sentence, and this card was never updated.** `crates/aligned-vmem/src/page_size_override.rs` (added task #1080, 2026-08-18, ~5 hours after task #1061 filed this correction — commit `5daa90c`; `set_page_size_override`, lines 110-137) is exactly such a seam: it writes directly into the same `pub(crate) static PAGE_SIZE_CACHE: AtomicUsize` that `page_size()` reads first (`crates/aligned-vmem/src/page_size.rs`'s `page_size()` returns the cache immediately on a nonzero load, without re-querying or re-validating), so calling `set_page_size_override(Some(ps))` then `crate::page_size()` re-exercises `page_size()`'s own end-to-end return path — repeatedly, in one process — and `set_page_size_override(None)` resets the sentinel so the NEXT `page_size()` call re-queries the OS for real. The seam ALSO internally calls the same `query_os_page_size()` + `validate_page_size_impl()` pair `page_size()` itself calls (via `real_os_page_size_fresh`, the real-page floor check), so both halves of `page_size()`'s wiring are reachable from a test — cfg-gated behind the build-time `aligned_vmem_page_size_override` flag (deliberately not a Cargo feature, matching the `mock` precedent item 42 closed), not compiled into a default build. **What remains genuinely untested is narrower still than the pre-correction card said:** no test in the current tree actually calls `set_page_size_override` to drive `page_size()`'s REJECTION branch specifically (the seam's own existing consumers, e.g. `tests/lazy_initial_commit_forced_page.rs` and `tests/decomp_hooks_forced_page.rs`, only ever pass valid accepted values to force a larger simulated page) — so the rejection path through `page_size()` itself (as opposed to through the already-tested standalone `validate_page_size`) is untested IN PRACTICE, but "no injection seam exists" was wrong; "no test currently uses the existing seam for this" is the accurate residual claim.
    - **Current-number-or-verdict, U11 half (`page_size()` guard):** the guard inside `page_size()` (the pure `validate_page_size_impl`) and `query_os_page_size()`'s three `#[cfg]` arms — all in `crates/aligned-vmem/src/page_size.rs` since the task #1055 split, together with the `PAGE_SIZE_CACHE` atomic (citation kept symbol-based per the task #908/V2C3 convention; the line ranges this bullet carried were rotted by the split and are dropped) — are untested end-to-end IN PRACTICE (no test currently drives `page_size()`'s own rejection branch via the existing `page_size_override` seam; see the task #1143 correction above for why "no injection seam" itself was false), and outside the `aligned_vmem_page_size_override` cfg build, the result caches in the process-wide `PAGE_SIZE_CACHE` atomic on first call with no seam at all, so no test can re-run the guard within one process on an ordinary build. The guard's REJECTION branch (queried `< PAGE`, or non-power-of-two) had never executed in any test or CI run in this repository as of filing. **Superseded (task #949/T-5, 2026-08-14 — one day after this card was filed; surfaced by the task #1060 current-state audit):** the pure-function extraction the "Why not fixed now" bullet below deferred HAS landed, under the name `validate_page_size` (a `bench-internals`-gated `pub fn` in `crates/aligned-vmem/src/validate_page_size.rs`, delegating to `validate_page_size_impl`), and the rejection branch is now EXECUTED by `validate_page_size_falls_back_on_invalid_values` (`crates/aligned-vmem/tests/smoke.rs`: `0`/`5`/`2048` all rejected to `PAGE`), which runs in CI wherever `bench-internals` meets the real backend (the `aligned-vmem-gates` and `test-workspace` `--all-features` rows on ubuntu-latest, and the `test-macos` full-feature row). What remains genuinely untested is only `page_size()`'s own wiring exercised THROUGH `page_size()` itself (as opposed to through the standalone `validate_page_size`); the U11 half's closure decision is left to a future round — this audit is doc-only and closes nothing. Item 43's card cites this guard as the reason a wrong `_SC_PAGESIZE` constant "silently returns garbage" rather than crashing. **Corrected round-8 closing review (task #904, UC3):** this bullet originally said round 8's U1 finding "makes the guard's untested acceptance-side load-bearing" in the present tense — stale on arrival, since task #897 (merge `491afe9`, this same round) already removed that dependency by making `try_reserve_aligned_exact`'s alignment check unconditional (it no longer consults `page_size()` at all). The guard's acceptance side is now cosmetic, not load-bearing.
    - **Why not fixed now:** the cheap remedies each have a real cost: a `#[cfg(feature = "bench-internals")] fn dbg_set_page_size_for_test` would be a safe `pub fn` mutating a value the alignment fast path trusts, precisely the shape CLAUDE.md's benchmark-hook rule warns against; splitting the guard into a pure `fn sanitize_page_size(queried: usize) -> usize` and testing that in isolation is the clean version and costs one small refactor, deliberately left for a future round rather than folded into this hygiene bundle. (That extraction has since landed, differently named — task #949/T-5's `validate_page_size`/`validate_page_size_impl`; see the supersession note in the bullet above.)
    - **Next trigger:** round 8's U1 fix already landed (`491afe9`, this same round), so this guard's untested acceptance side is purely cosmetic now; the remaining trigger is the `sanitize_page_size` extraction described above, which would make the REJECTION branch testable without a benchmark-hook-shaped seam — a trigger since discharged by task #949/T-5 (the extraction landed as `validate_page_size`/`validate_page_size_impl`, and the rejection branch is now tested; see above), leaving only the future-round closure decision on the U11 half and, optionally, end-to-end coverage of `page_size()`'s own wiring.
    - **Evidence:** `docs/reviews/2026-08-13-aligned-vmem-round8-review.md` finding U11 (filed INFO, not code defect).

51. **CLOSED** by task #1059, **REOPENED by task #1071** (the #1059 closure narrative's assertion that the local gate now catches this bug class was falsified by the very next push: both cross-target rows always ran with huge-pages on, the mock arm stayed excluded, and a stale cargo cache could still print OK for a row that linted nothing — see the CORRECTION block in "Recently resolved" §51), **re-CLOSED by task #1071** (bench-internals-only cross clippy row + mock clippy row + per-run `cargo clean -p aligned-vmem` invalidation (host and --target forms) + an `expectWork` proof-of-work guard on every aligned-vmem row). See "Recently resolved" below.

52. **`decommit_lazy` leaves free BSD reclaim on the table** (Filed 2026-08-14, task #934/C-9, from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` finding V-4.) **CLOSED** — see "Recently resolved" below for the full closure narrative.

53. **`Reservation::from_raw_parts` hard-codes `granted_huge: false`, creating a fail-open hazard when callers follow documented decommit advice** (Filed 2026-08-14, task #934/C-9, sub-observation about item 48 from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`.) **CLOSED** — see "Recently resolved" below for the full closure narrative.

54. **CLOSED** by task #1058 (V-29's tautological `min_page_equals_page` deleted — the compiler enforces the `MIN_PAGE == PAGE` alias and the untouched `min_page_is_4kib` pins the concrete value; V-31's two genuine gaps filled — `ReservationParts` derived `PartialEq`/`Eq` and `leak_zeroed_pages` exact-multiple size now tested; V-31's other three sub-findings were stale-card non-gaps, already covered or moot). See "Recently resolved" below for the full closure narrative.

55. **`sefer-region`'s packaged benchmark can attempt a write outside its own
   package root when run standalone** (`crates/sefer-region/benches/region_bench.rs`)
   (Filed 2026-08-09, task #792, from the static release audit's finding F14.)
   Moved from the "Recently resolved" section to `[T]` in this same edit — the item's
   own text explicitly explains why it cannot be closed (no fix reachable from
   `sefer-region`'s own source; requires an upstream `bench-scale-tool` change), so marking it
   "**OPEN**" in a section whose header explicitly says "do not re-list as open" was a miscategorization.

   - **Root cause, confirmed against `bench-scale-tool` 0.1.0's actual
     source** (not just its doc comments): `Harness` exposes no public API
     to override where its manifest lives. `manifest_path()` (private to
     that crate) walks up from `CARGO_MANIFEST_DIR` for the nearest
     `[workspace]`-declaring `Cargo.toml`, falling back to
     `<crate>/../../bench-iters.txt` only when no such ancestor exists
     (e.g. this crate extracted standalone from a published tarball). A
     plain `cargo bench` run (no `--calibrate` flag) still attempts
     `save_manifest` at the end whenever any workload was JIT-calibrated
     on the spot (the self-healing path for a missing manifest entry) —
     which is every workload on a fresh/missing manifest. For a
     standalone-extracted package this targets a path OUTSIDE the package
     root.
   - **Why not closed:** there is no fix reachable from `sefer-region`'s
     own source. The harness's manifest routing lives entirely in the
     separately-published `bench-scale-tool` crate (a registry dependency,
     not a workspace member this repo vendors or can patch). Closing this
     for real requires either an upstream `bench-scale-tool` change (a
     public API to override the manifest path, or a "read-only, never
     write" mode) or a CI-side fix (extracting the packaged tarball into
     an isolated temp directory and observing the actual failure/success
     mode empirically, rather than assuming from source reading alone).
   - **Mitigating:** `load_manifest` on a missing/unreadable path returns
     an empty map rather than crashing, and every workload self-heals via
     a 1-second JIT calibration pass rather than aborting — so a
     standalone `cargo bench` run does not fail outright; the exposure is
     the ATTEMPTED write outside the package root (which may itself fail
     silently on a read-only extraction, or succeed and pollute an
     ancestor directory on a writable one), not a guaranteed crash.
   - **Next trigger:** either (a) `bench-scale-tool` ships a version with
     a manifest-path override API sefer-region's `region_bench.rs` can
     use, or (b) someone actually performs the isolated-tarball-extraction
     verification this item currently only reasons about from source, and
     records the real observed behavior.
   - **Evidence:** `docs/reviews/2026-08-09-sefer-region-static-release-audit.md`
     finding F14; `crates/sefer-region/benches/region_bench.rs` (the benchmark file
     itself, which now documents the exposure honestly rather than claiming
     a fix that does not exist).

58. **[T] CI-coverage gap: i686-gnu and i686-musl targets are compile-only verified, runtime exact/huge path never executes in CI.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.)

    The CI workflow's `aligned-vmem-gates` job (its `rustup target add i686-unknown-linux-{gnu,musl}` + `cargo check --target` steps) runs `cargo check --target i686-unknown-linux-{gnu,musl} --all-targets` for compile-time verification of the 32-bit Unix exact-size path and the FFI `off_t` type-correctness fixes (item 44, task #914), but does NOT run `cargo test --target i686-...` to execute the runtime behavior. The `try_reserve_aligned_exact` path (actual gate, re-read off the source on 2026-08-17 rather than restated from this card's own earlier text: `#[cfg(all(unix, not(miri), target_pointer_width = "32"))]`) and the 32-bit `OffT`-corrected `mmap` calls are therefore never exercised under actual 32-bit execution in CI, only verified for compile-time correctness. **Correction (task #1045, finding R7-9):** this card previously described the gate as `not(target_pointer_width = "64")` plus `not(target_os = "android")`, i.e. as EXCLUDING Android. It does not — there is no `target_os` clause in the gate at all, so 32-bit Android is INSIDE this path, and the CI-coverage gap this card describes therefore covers Android as well. The exclusion never existed; it appears to have been carried over from a neighbouring huge-page gate, which is where a `target_os` clause does live.

    - **Status:** OPEN — not urgent, because the compile-only check catches the known FFI ABI mismatch risk (item 44), but the runtime exact-size reservation path is unverified on 32-bit targets.
    - **Current-number-or-verdict (re-verified off `.github/workflows/ci.yml` at task #1060, 2026-08-17):** the i686 coverage is still exactly two compile-only steps in the `aligned-vmem-gates` job — `cargo check --target i686-unknown-linux-gnu` and `cargo check --target i686-unknown-linux-musl`, both `--all-targets --features "lazy-commit huge-pages fault-injection bench-internals" -p aligned-vmem` — and a `grep i686` across `.github/workflows/` finds no `cargo test --target i686-*` step in any job, so the 32-bit runtime path still never executes in CI (32-bit Android included, per the task #1045/R7-9 correction above). The #1045 gate itself re-verified post-split: `try_reserve_aligned_exact` in `crates/aligned-vmem/src/os/unix.rs` still carries exactly `#[cfg(all(unix, not(miri), target_pointer_width = "32"))]` — no `target_os` clause; the split moved the file, not the gate.
    - **Next trigger:** when a 32-bit Linux runtime test runner becomes available in CI (e.g., via GitHub Actions `i686-unknown-linux-gnu` self-hosted runner or QEMU-based emulation), add a `cargo test --target i686-unknown-linux-gnu` step to the `aligned-vmem-gates` job. Until then, the compile-only check is the best available coverage.
    - **Evidence:** the `cargo check --target i686-unknown-linux-{gnu,musl} --all-targets` steps in `.github/workflows/ci.yml`'s `aligned-vmem-gates` job (compile-only); `crates/aligned-vmem/src/os/unix.rs` `try_reserve_aligned_exact` function (the 32-bit-gated exact-size reservation path; its home since the task #1055 split); item 44 (the `OffT` fix that this compile-only check guards against regressions).

59. **[T] CI-coverage gap: Linux MAP_HUGETLB and Windows MEM_LARGE_PAGES success paths depend on configured hugetlb pool or SeLockMemoryPrivilege, neither present in standard CI.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.)

    The Linux `MAP_HUGETLB` success branch (`libc_mmap` with `MAP_HUGETLB | MAP_HUGE_2MB`) only succeeds when the system has a configured hugetlb pool with 2 MiB huge pages available. Standard GitHub Actions `ubuntu-latest` runners do NOT pre-configure any hugetlb pool (`/proc/sys/vm/nr_hugepages` defaults to 0 on these runners), so all huge-page reservation attempts fall back to ordinary 4 KiB pages in CI. Similarly, the Windows `MEM_LARGE_PAGES` success branch requires `SeLockMemoryPrivilege` and appropriate large-page configuration, which standard `windows-latest` runners do not provide. The happy paths for both platforms are therefore never exercised in CI, only the fallback paths.

    - **Status:** OPEN — not urgent, because the fallback path is the same code exercised on non-huge-page systems, and the `is_huge()` predicate correctly reports the failure (test arm verifies `granted_huge == false`). However, the actual huge-page success path is unverified in CI.
    - **Current-number-or-verdict (re-verified off `.github/workflows/` at task #1060, 2026-08-17):** unchanged — a grep for `nr_hugepages`/`hugepages`/`SeLockMemoryPrivilege` across all four workflow files returns zero hits (no step anywhere configures a hugetlb pool or large-page privilege), and every runner is a standard `ubuntu-latest`/`windows-latest`/`macos-latest` image, so both success branches (`libc_mmap`'s `MAP_HUGETLB | MAP_HUGE_2MB` grant; `winapi_virtual_reserve`'s `MEM_LARGE_PAGES` grant) still never execute in CI — only the fallback paths do.
    - **Next trigger:** when a CI runner with configured hugetlb pool (Linux) or SeLockMemoryPrivilege (Windows) becomes available, add a dedicated `test-huge-pages` job that runs `cargo test --features huge-pages` with appropriate hugetlb pool setup (e.g., `sudo sysctl vm.nr_hugepages=128` on Linux). Until then, the success paths remain unverified.
    - **Evidence:** `crates/aligned-vmem/src/os/unix.rs` `libc_mmap` function (Linux `MAP_HUGETLB | MAP_HUGE_2MB` handling); `crates/aligned-vmem/src/os/windows.rs` `winapi_virtual_reserve` function (Windows `MEM_LARGE_PAGES` handling); `crates/aligned-vmem/tests/huge_pages.rs` (the test that would exercise the success path if hugetlb were available); `/proc/sys/vm/nr_hugepages` on standard runners (observed to be 0).

60. **[T] CI-coverage gap: BSD, Android, tvOS and watchOS branches are reasoned-from-spec, not empirically executed on real hardware.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.)

    The BSD platforms (FreeBSD, NetBSD, OpenBSD, DragonFly) are only compile-verified; no BSD CI runner exists (`.github/workflows/ci.yml` has no BSD job). The `decommit_lazy` and `_SC_PAGESIZE` handling for these platforms are based on reading headers and POSIX spec, not empirical runs. Similarly, Android, tvOS and watchOS platforms are not represented in CI; only macOS covers part of the Darwin family (iOS, tvOS, watchOS are missing). This is an honest limit of the current CI infrastructure, not a discovered bug.

    - **Status:** OPEN — not urgent, because the unconditional alignment check in `unix_reserve` prevents violation of reserve alignment even if `_SC_PAGESIZE` values were wrong, and decommit correctness is verified on Linux/macOS which are the primary deployment targets. However, the BSD/Darwin/Android branches remain empirically unverified.
    - **Current-number-or-verdict (re-verified off `.github/workflows/ci.yml` at task #1060, 2026-08-17):** unchanged — all 39 jobs in `ci.yml` run on standard `ubuntu-latest` (34), `windows-latest` (2), or `macos-latest` (3) images; the only target matrix (`multi-arch`) covers just `x86_64-unknown-linux-gnu` and `aarch64-unknown-linux-gnu`; no BSD, Android, iOS, tvOS, or watchOS job or target exists anywhere in the workflow. These branches remain reasoned-from-spec.
    - **Next trigger:** when a FreeBSD/NetBSD/OpenBSD/DragonFly CI runner becomes available (self-hosted or via a service like Cirrus CI), add a `test-bsd` job. Similarly, if iOS/tvOS/watchOS or Android CI runners become available, add corresponding jobs. Until then, these platforms remain spec-verified only.
    - **Evidence:** item 43 (BSD `_SC_PAGESIZE` values, partially open); item 48 (Darwin `MADV_DONTNEED` behavior, partially open); `.github/workflows/ci.yml` (no BSD/iOS/tvOS/watchOS/Android jobs); `crates/aligned-vmem/src/os/unix.rs` platform-specific constants (FreeBSD/NetBSD/OpenBSD/DragonFly `_SC_PAGESIZE` values, Android-specific handling, Darwin-family `decommit_lazy` behavior — i.e. the per-OS `_SC_PAGESIZE` cfg-match plus `madv_free_advice`'s Darwin arms, at this path since the task #1055 split; the `decommit_lazy` API wrapper itself is `crates/aligned-vmem/src/api/decommit_lazy.rs`).

61. **CLOSED** by task #1057 (same fix as item 41 — the new `aligned-vmem-miri` CI job runs the interpreter, closing this runtime-semantics phrasing of the same gap). See "Recently resolved" below for the full closure narrative.

63. **Flaky test — `shadow_path_activation_oracle_fast_and_slow_both_reachable` scheduler-sensitive percentage thresholds.** See "Recently resolved" §3 for full resolution.

64. **Follow-up from commit 66b8508 (task #1030): `npm run check` lacks a `cargo test -p aligned-vmem` row with DEFAULT features, though ci.yml has a separate job that tests workspace members with default features.** — **CLOSED**, see "Recently resolved" §#64 below.

65. **CI-coverage gap: `aligned-vmem-gates` job added three steps (cargo doc with RUSTDOCFLAGS="-D warnings", cargo publish --dry-run, cargo semver-checks check-release) that are NOT covered by `npm run check`.** — **CLOSED**, see "Recently resolved" §#65 below.

66. **`Reservation` carried no committed-length state, so a lazy handle's committed prefix was a DOCUMENTED contract rather than a CHECKABLE one (R6-1 variant 3 / R7-2).** — **CLOSED** by the new `LazyReservation` type (task #1051; its `as_reservation()` accessor re-opened the hole from safe code and was deleted by task #1104/H1), see "Recently resolved" §#66 below — including why all five options this card previously listed were set aside for a sixth.

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
   See "Recently resolved" below for the full closure narrative.

69. **CLOSED** by task #1063 (added the missing `serial_guard()` call to `windows_virtualfree_release_failures_accessor_exists`). See "Recently resolved" below for the full closure narrative.

70. **Root-crate gate rows can replay test binaries baked by a DIFFERENT git worktree of this repo (shared `CARGO_TARGET_DIR` + compile-time-baked `CARGO_MANIFEST_DIR`): a false RED when the baking worktree was deleted (misleading `Os { code: 3, kind: NotFound }` panics) and a false GREEN while it still exists (tests silently validate the wrong tree).** The same cache-replay class task #1071 closed for the aligned-vmem rows, with the sign flipped, on the root crate — where #1071's `expectWork` guard explicitly does not reach (its documented boundary is the `aligned-vmem` package rows only).

   - **Status:** MITIGATED (task #1073, 2026-08-18) — not fully closed. A tripwire test (`tests/no_stale_cross_checkout_artifacts.rs`: a cargo-run test binary's cwd is the INVOKING package root, so `env!("CARGO_MANIFEST_DIR") != current_dir()` detects any foreign-baked binary, whichever cargo freshness quirk let it replay) plus a gate-level output sniffer (`scripts/stale-artifact-diagnosis.mjs`, wired into `scripts/check-all.mjs`'s failure path and self-tested as a gate step) catch the condition wherever a root-crate test binary executes. The residual gaps below are real, measured, and deliberately not paid for.
   - **Current number / verdict:** counterfactually verified in BOTH directions with zero `Compiling` lines (pure cache replay, `Finished ... 0.5s`): (a) false GREEN — `cargo test --features pinning --test ci_clippy_matrix_consistency` in `sefer-alloc-wt-1073` printed `ok` from a binary baked in a temp worktree whose `.github/workflows/ci.yml` had been made to differ (the pass validated the wrong tree); (b) false RED — after `git worktree remove` of that temp worktree, the same command reproduced the observed 2026-08-17 failure byte-for-byte: `panicked at tests\ci_clippy_matrix_consistency.rs:239:10: read scripts/check-matrix.mjs: Os { code: 3, kind: NotFound, message: "The system cannot find the path specified." }`. The tripwire fired in both states with messages naming the cause, both paths, and the remediation (`touch` the test source or `cargo clean -p sefer-alloc`); the sniffer fired on the literal poisoned output. Root mechanism (observed in the fingerprint files, not inferred): cargo's `CheckDepInfo` freshness compares each dep file's mtime against the dep-info file's own mtime using RELATIVE paths (the pinning unit's dep-info lists just `tests\ci_clippy_matrix_consistency.rs`), so a checkout whose sources are OLDER than another worktree's dep-info stamps as "fresh" and replays that worktree's binary — file content is never compared. The natural trigger is simply: create worktree A, later create worktree B and build there, then return to A (A's sources are older than B's build). The observed instance: the shared cache held four `ci_clippy_matrix_consistency-*.exe`, all baked with `D:\dev\rust\sefer-alloc-wt-1069` (a since-removed worktree).
   - **Next trigger:** any tripwire failure of `tests/no_stale_cross_checkout_artifacts.rs`, or any gate failure accompanied by the `[check-all] DIAGNOSIS (task #1073)` line (the sniffer; needed because `cargo test` aborts at the first failing test target — observed — so a `NotFound`-panicking sibling can shade the tripwire binary in the same invocation). A future round wanting to close the clippy-row residual below should measure first.
   - **Evidence:** the task #1073 commit (this one); `tests/no_stale_cross_checkout_artifacts.rs`; `scripts/stale-artifact-diagnosis.mjs` (4/4 self-test fixtures, including the literal observed panic text); `scripts/check-all.mjs` (DIAGNOSIS block + `stale-artifact-diagnosis` step); the counterfactual transcript in the task #1073 commit body. Full baked-path inventory (24 pre-existing `env!("CARGO_MANIFEST_DIR")` code sites: 17 in root `tests/` across 6 files, 1 in `crates/sefer-region/tests/`, 6 in `crates/*/benches/` harnesses — plus the tripwire's own 25th site, and one `env!("CARGO_TARGET_TMPDIR")` site that is target-dir-relative and harmless) is in the commit body.
   - Residual, deliberately uncovered: (1) root-crate **clippy/check rows** — a stale clippy cache replays with no test execution (#1071's hole-3 class, root-crate half); covering them would need `cargo clean -p sefer-alloc` + `expectWork` on ~10 feature rows, rejected on measured cost (65 s cold for ONE feature row in a scratch target dir — `cargo test --no-run --features pinning`, 1.3 GB, sccache hit rate 0% — and #1071's own +47 s/+5.4% reference for a far smaller crate). (2) **bench targets** with baked paths (5 subcrate harnesses + `crates/aligned-vmem/benches/vmem_bench.rs`) — not run by `npm run check` (iai uses its own WSL-local `/tmp/sefer-iai` target dir), exposed only to cross-worktree manual `cargo bench`. (3) `crates/sefer-region/tests/bench_ids_isolatable.rs` — subcrate tests run only under CI's workspace-member job (fresh checkout per run, not exposed). (`CARGO_TARGET_TMPDIR` in `captrack_probe.rs` is target-dir-relative and identical across checkouts — not a cross-checkout hazard.) Rejected options with costs: per-worktree `CARGO_TARGET_DIR` — besides the recurring 65 s/1.3 GB-per-row cold cost, structurally unavailable as a committed setting because the HKCU user env var overrides `.cargo/config.toml`'s `build.target-dir` (cargo env precedence); `expectWork` root rows — cost above; discipline-only protocol (`cargo clean -p` after every `git worktree remove`) — rejected as discipline-dependent, the failure mode that already recurred twice in this campaign.

72. **[T] CI-coverage gap, PARTIALLY CLOSED: `aligned-vmem` has a `--release` test row in `.github/workflows/ci.yml`, but it only exercises the MOCK backend on one narrow test target — the REAL-backend release half (the unix madvise oracle in `smoke.rs`, and a plain `--all-features --release` row) still does not run anywhere in CI.** (Filed 2026-08-18, task #1079; numbered 72 because 71 was taken by task #1078's doc-drift-guard item in "Recently resolved" below. **Corrected 2026-08-19 (task #1143): the card's own "zero `--release` rows, unchanged" verdict was FALSIFIED by task #1086, landing the SAME DAY roughly eight hours after this card was filed — and task #1086's own commit body/comments never updated this card, leaving a stale "unchanged" claim standing for a full day. `scripts/check-all.mjs`'s comment on the mock-release local step (added by that same task #1086) explicitly references this card's "recorded plain-`--release` estimate" as covering a row that does not activate the mock cfg — i.e. the commit that refuted this card's claim also cited the card by number and still left it unedited. This is not a stale card nobody read; it is a card the very refuting work pointed at and skipped fixing.**)

    Every `cargo test -p aligned-vmem` row in the workflow **was** a debug-profile run at filing time (`aligned-vmem-gates` rows at :180/:184, Windows :907/:914, macOS :947/:953, workspace-members :985, Linux :1028/:1036 — the mock rows among these differ only in `RUSTFLAGS`, not in profile). Four sibling crates in the SAME workflow carry release rows, each added by a rust-intel round-closing review for exactly this profile-divergence class: `tagged-index-stack` (task #772/F4 — an `assert!` promoted from `debug_assert!` that a debug-only suite cannot verify), `size-classes`, `racy-ptr-cell` (task #773/F1), `sefer-region` (checked-arithmetic profile divergence). **(Correction, task #1143: 8 of the 9 line numbers this paragraph originally cited — :184/:907/:914/:947/:953/:985/:1028/:1036 — do not point at `cargo test -p aligned-vmem` rows at all; re-checked directly against current `.github/workflows/ci.yml`, they are comment lines belonging to OTHER jobs' explanatory text (e.g. `:1036` is the unrelated `cargo check --features "production numa-aware"` row). Only `:180` (`cargo test -p aligned-vmem --all-features`, debug profile) is a real aligned-vmem test row at that citation. The sibling-crate release-row line numbers in the same sentence were not re-checked here and may have the same defect — treat them as unverified, not re-confirmed.)** The profile-sensitive tests a release row would activate already exist and are gated `not(debug_assertions)`: `smoke.rs`'s `decommit_contract_violation_never_reaches_madvise` (unix real-syscall oracle, free functions), `mock.rs`'s `decommit_release_silently_skips_contract_violating_offsets` (mock call-log oracle, free functions), and — since task #1079 — `tests/reservation_decommit_contract.rs`'s method-layer release tests (`method_silently_skips_a_violated_range_in_release`, `method_records_nothing_for_a_violated_range_in_release_mock`).

    - **Status:** PARTIALLY CLOSED (task #1086, same day as filing). A release row now exists (`.github/workflows/ci.yml:208`, inside the `aligned-vmem-gates` job): `cargo test --release -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --test reservation_decommit_contract`, under `RUSTFLAGS: "--cfg aligned_vmem_mock"` — i.e. MOCK backend only, and scoped to one test target (`reservation_decommit_contract`), not `--all-features`/the whole suite. It activates `method_records_nothing_for_a_violated_range_in_release_mock` (the mock-layer release oracle). It does NOT activate `smoke.rs`'s real-backend unix madvise oracle (`decommit_contract_violation_never_reaches_madvise`) or `mock.rs`'s free-function mock oracle (`decommit_release_silently_skips_contract_violating_offsets`, different test binary from the one the new row scopes to) — those still execute in no CI test, on any runner, in the release profile.
    - **Current-number-or-verdict (re-verified directly against `.github/workflows/ci.yml` at task #1143, 2026-08-19):** ONE `--release -p aligned-vmem` row exists, mock-backend, single-test-target (`:208`, added by task #1086, commit `d58bd67`, the same day as this card's filing). Zero real-backend `--release` rows exist — `grep -n "cargo test --release -p aligned-vmem\|cargo test -p aligned-vmem --release" .github/workflows/ci.yml` returns exactly the one mock row and nothing else.
    - **Next trigger:** owner decision on the costed recommendation below (still open — the real-backend half of the original ask is unaddressed); any future CI-only release-profile regression in the decommit family; or any future `debug_assert!`-vs-`assert!` promotion in this crate (the tagged-index-stack task #772/F4 and racy-ptr-cell task #773/F1 precedents).
    - **Evidence:** `.github/workflows/ci.yml:208` (the mock-release row added by task #1086, commit `d58bd67`); `scripts/check-all.mjs`'s mock-release local step comment (added in the same task #1086 commit), which explicitly cites "correctness item 72's recorded plain-`--release` estimate" as covering a row that does not activate the mock cfg — the refuting task's own text pointed at this card without updating it; `crates/aligned-vmem/tests/try_decommit.rs`'s `decommit_debug_asserts_on_a_contract_violation` (the debug half, CI-green today via the debug rows); task #1079's original local runs — `cargo test -p aligned-vmem --all-features --release` after `cargo clean -p aligned-vmem` (real `Compiling aligned-vmem` line confirmed; all tests green including the release-gated ones; 47 s wall on this Windows host INCLUDING the full clean rebuild) and `RUSTFLAGS="--cfg=aligned_vmem_mock" cargo test -p aligned-vmem --features "lazy-commit huge-pages fault-injection bench-internals" --release` (mock+release combination green); `crates/aligned-vmem/tests/reservation_decommit_contract.rs` module doc ("Profile mechanics" paragraph).
    - **Costed recommendation (owner's call — CI is not edited unilaterally per task #1079's brief; still applies to the residual real-backend gap):** add one row `cargo test -p aligned-vmem --all-features --release --no-fail-fast` to the existing `aligned-vmem-gates` job (or the Linux workspace-members job). Local reference: 47 s wall including a full clean rebuild on this host; steady-state CI cost should be roughly one extra compile-and-run of the crate's test suite in release, comparable to the debug `--all-features` row CI already pays at `:180`. The real-backend unix oracle (`smoke.rs`'s madvise counter) cannot be exercised any other way — the mock-layer release row task #1086 already added does not substitute for it; only the no-record-on-violation half of the MOCK layer would additionally need a broader (`--all-features` rather than single-test-target) mock+release row if that is ever wanted too — in debug those calls panic before reaching the recorder, so no debug test can pin it.

73. **[T] Residual blind spots of the two page-constant-class defences (task #1080): the static guard's provenance rule still accepts PAGE-free opaque method calls without walking their bodies, and the forced-page runtime test executes only on real Windows (not miri) builds with the `--cfg aligned_vmem_page_size_override` flag.** (Filed 2026-08-18, task #1080; the task closed the F5/F7 gap — the guard raised zero candidates from `src/` and the #1074 regression test pinned the helper's arithmetic, not the call sites — and this card records only what the two rebuilt defences still do NOT see, so a future round inherits the honest boundary rather than a false sense of total coverage.)

    The guard's `src/`-only provenance rule (strict for the `initial_commit` position, pagefree elsewhere) resolves identifiers through same-file `let` bindings and one-depth parameter→caller chains, but a PAGE-free method or qualified call (`meta.committed_payload_end_of()`, `SegLayout::small_decommit_start()`) is accepted as an opaque invariant-carried value — its body is not walked unless the chain leads back to a raw `PAGE` token or a non-approved raw expression. `crates/**` (the seam crates and their tests) remains under the fold-only rule: `crates/aligned-vmem/tests/lazy_commit.rs`'s `#[cfg(windows)]`-gated `let initial = PAGE;` shape is still invisible by design. The runtime complement (`tests/lazy_initial_commit_forced_page.rs`, forcing `page_size()` to 64 KiB via the `--cfg` seam) runs its forced-page half only under `windows ∧ not(miri) ∧ not(numa-aware) ∧ (a lazy-commit feature) ∧ the cfg flag` — i.e. the new `check-all` step and the `test-windows` CI row, not the plain feature rows, and never on the macOS/Linux runners where the eager fallback arm compiles instead.

    - **Status:** OPEN (tracked residue, deliberate): every escape of the class so far (#1067, #1074 ×6 sites, #1075, #1077) is now covered by at least one of the two defences — confirmed by injected-defect counterfactuals at task #1080 (three production call-site reverts: each failed the new test and/or the guard, while the old helper-pinning test stayed green under the same revert).
    - **Current-number-or-verdict:** guard — `18 src/ arg(s) checked, 0 prod finding(s)` on the current tree with ONE justified marker (`src/alloc_core/os.rs`'s `reserve_capacity_exact` call: both args value-proven page-multiples but form-opaque — see the marker comment and the guard header); runtime — forced-page test green under `production internals` and `production small-segment-lazy-commit internals`, both with the cfg flag, after `cargo clean -p sefer-alloc`.
    - **Next trigger:** any sixth escape of the compile-time-`PAGE` class (a new `src/` vmem-facing call-site family, or a revert that neither defence catches — which would mean the residue graduated from tracked to live); any proposal to walk method-call RHS bodies in the guard (the cost is real parsing complexity in a regex-shaped tool — the reason binding+caller-chain resolution was chosen over full dataflow); any need to run the forced-page observation on non-Windows hosts (today impossible by construction: the lazy two-phase reservation only exists on real Windows).
    - **Evidence:** task #1080's commit (the guard's new "Production provenance rule" header section + fixtures F13–F15/P13–P18; `tests/lazy_initial_commit_forced_page.rs`'s own "Scope and soundness" section stating the Windows-only boundary and why); counterfactual quotes in the task #1080 commit body (bootstrap reserve revert → `assertion failed: initial_commit != 0` at `bootstrap.rs:113` + guard `[production provenance]` finding at `os.rs:317`; bootstrap stamp revert → frontier `left: 454656 / right: 458752`; small-segment reserve revert → allocation failure + two guard findings at `alloc_core_small.rs:2012/2031`; the old test green under every one of those reverts).

    **Update (task #1081, 2026-08-18):** the guard's opaque-call blind spot produced its first confirmed REAL instance: `SegLayout::small_meta_end()` (PAGE-free, opaque) at the two R29-3 decommit/recommit hooks in `src/alloc_core/alloc_core_small_pool.rs` — invisible to the guard, made loud only by task #1072's tripwire on a >4 KiB-page host. Fixed by task #1081 together with two accessor siblings (`dbg_decomp_payload_range`, `dbg_decomp_page_size`) and the reconcile hook's committed-bytes accounting; pinned by `tests/decomp_hooks_forced_page.rs` (forced 16/64 KiB arms driving the REAL hooks, wired into check-all and the ci.yml test-windows forced-page row — so this instance's class now has a runtime defence the guard alone did not provide). The guard itself is unchanged — walking method-call bodies remains open per this card's Next trigger.

74. **[T] Task #1081 residual coverage boundary: the fixed decomp-hook family is pinned by a SIMULATED 16/64 KiB page (never real >4 KiB hardware). [Claim corrected by task #1087, finding M4: this card's and commit `b98edb0`'s former assertion that the `dbg_segment_state_reconciliation` committed-bytes fix "has no runtime oracle" because the retain-decommitted state is "unreachable" was FALSE — `AllocCore::dbg_force_decommit_retain_for` (same `internals`+`alloc-decommit`+`bench-internals` gate, driving exactly the `release_follows == false` leg, already exercised by `tests/alloc_zeroed_virgin_small_skip.rs` since R12-10/#261 + R29-8/#439) makes the state test-reachable today; the oracle now exists as `tests/segment_state_reconciliation_oracle.rs`.]** (Filed 2026-08-18, task #1081 / findings F6+F10b; oracle claim corrected 2026-08-18, task #1087/M4; the first REAL instance of item 73's "opaque PAGE-free method call" guard blind spot — `SegLayout::small_meta_end()` — found and fixed: the two R29-3 decommit/recommit hooks plus their two accessor siblings in `src/alloc_core/alloc_core_small_pool.rs` now route through the runtime-page-safe `small_decommit_start()` / `aligned_vmem::page_size()`, pinned by `tests/decomp_hooks_forced_page.rs`.)

    - **Status:** OPEN (tracked residue, NARROWED by task #1087/M4): the FIX is fully landed and counterfactually verified (hook revert → literal tripwire panic, quoted in the task #1081 commit body; restore → green); the reconciliation `meta_bytes` fix NOW HAS a runtime oracle too (`tests/segment_state_reconciliation_oracle.rs`, task #1087 — counterfactually verified in BOTH policy worlds); what stays open is only what the forced-page seam cannot see (real >4 KiB hardware) — there is no longer any oracle gap on this item.
    - **Current-number-or-verdict:** `tests/decomp_hooks_forced_page.rs` green under `RUSTFLAGS="--cfg aligned_vmem_page_size_override" cargo test --features "production internals bench-internals" --test decomp_hooks_forced_page` on this 4 KiB-page Windows host (both the forced 16 KiB and 64 KiB arms drive the REAL hooks); `tests/segment_state_reconciliation_oracle.rs` green in both policy worlds — real-page `cargo test --features "production internals bench-internals small-segment-lazy-commit"` and forced-page `RUSTFLAGS="--cfg aligned_vmem_page_size_override" cargo test --features "production internals bench-internals"` (eager) plus the same with `small-segment-lazy-commit` added (lazy) — with the forced-retain leg's activation proven per segment (`dbg_is_decommitted_for == Some(true)` on both; lazy frontier reset via `dbg_committed_payload_end_for`) and the counterfactual observed by task #1087: reverting `meta_bytes` to the tight `small_meta_end()` fails BOTH worlds (lazy: committed 147456 vs 671744; forced 16 KiB: 147456 vs 163840; forced 64 KiB: 147456 vs 262144), restore → green. The card's former closing sentence ("verified by reading only: the `small_decommitted_retained` state has zero production callers today ... so no test can drive a segment into that state") was FALSE and is removed: zero PRODUCTION callers was true, but `dbg_force_decommit_retain_for` (same gate, present since R12-10/task #261, `pub unsafe fn` since R29-8/task #439) drives the leg from a test — the state was never unreachable.
    - **Next trigger:** a real >4 KiB-page host (e.g. the macOS ARM64 CI runner) executing any `bench-internals` example (`examples/r29_3_*` / `r32_13_*`) — the pre-fix panic is only ever OBSERVED there, never on this host; any sixth escape of item 73's opaque-call class. (The former third trigger — "any future policy enabling the retain-decommit path (which would give `meta_bytes` a reachable state and thus a possible oracle)" — is RETIRED by task #1087: the state was always test-reachable via the force hook, and the oracle now exists.)
    - **Evidence:** task #1081's commit body (counterfactual quote + sweep inventory with a per-site verdict — NOTE its closing "NO runtime oracle ... verified by reading only" line is the M4 falsehood corrected above; history is not rewritten, the correction lives here); `tests/decomp_hooks_forced_page.rs`'s module doc; `tests/segment_state_reconciliation_oracle.rs` (task #1087) and its counterfactual outputs; the corrected source comment in `dbg_segment_state_reconciliation`'s decommitted arm; item 73's Update note above.

76. **[T] CI-coverage gap: `tests/large_reserved_capacity.rs`'s `single_growth_commit_boundary_is_real_page_safe` (the task #1077 page-safety boundary oracle, itself upgraded to a true path-activation oracle by task #1088/L8) executes in NO local gate row — `npm run check`'s cargo-test rows either lack `large-reserved-capacity` or (on `--all-features`) enable `numa-aware`, which cfg-compiles the test out — and in exactly ONE CI row: `test-windows`' `cargo test --features "production exact-span-large large-reserved-capacity internals"` (`.github/workflows/ci.yml`).** (Filed 2026-08-18, task #1088, INFO-13.)

    - **Status:** OPEN (tracked coverage gap; the test itself is green wherever it runs).
    - **Current-number-or-verdict:** verified by grep — `large-reserved-capacity` appears in `scripts/check-all.mjs` ZERO times; in `.github/workflows/ci.yml` exactly once as an enabled feature (the test-windows row above; its two other mentions are comments). The `production` bundle does not include `large-reserved-capacity` (`Cargo.toml`), and `--all-features` pulls in `numa-aware`, which the test's `#[cfg(all(feature = "large-reserved-capacity", not(feature = "numa-aware")))]` excludes. So the oracle runs only on pushed Windows CI, never locally pre-push — a revert of the task #1077 fix stays invisible to the mandatory local gate.
    - **Next trigger:** the next task that touches `scripts/check-all.mjs`'s cargo-test rows adds a scoped local row mirroring the CI combo without `numa-aware`, e.g. `cargo test --features "internals large-reserved-capacity" --test large_reserved_capacity` (`alloc-core` comes transitively via `exact-span-large`), carrying `expectTest: 'single_growth_commit_boundary_is_real_page_safe'` per the task #1086/M6 convention (test name verified BY SYMBOL, deliberately not by line — `grep -n "fn single_growth_commit_boundary_is_real_page_safe" tests/large_reserved_capacity.rs`. This citation originally read `:238` and was already stale within its own wave: task #1099/I1 added a regime-pin doc block above that test and moved it to `:252`, so the card broke its own evidence pointer between two commits of the same batch — corrected by task #1108, and by symbol so it cannot recur). Deferred once already (2026-08-18, finding L6): the original blocker — "check-all.mjs was outside task #1088's agent scope, owned by a concurrent sibling task" — expired when sibling task #1086 landed in the same wave, edited `scripts/check-all.mjs` extensively (181 lines), and did not add this row; task #1095 then touched the same rows again (adding the page-size-override floor row) without adding this one either. The card therefore now tracks the row itself, not a scope boundary: it is next-in-line for any future check-all cargo-test-row edit, and a round that edits those rows must either add it or re-defer it here explicitly.
    - **Evidence:** `grep -n "large-reserved-capacity" scripts/check-all.mjs .github/workflows/ci.yml`; `Cargo.toml` (`production = [...]` without `large-reserved-capacity`; `large-reserved-capacity = ["exact-span-large", "aligned-vmem/lazy-commit"]`); the test's own cfg gate in `tests/large_reserved_capacity.rs`.

78. **[T, record correction — lint half, closed on filing] FIVE R30-12 commit-prefix mis-slots recorded — four grandfathered in `scripts/verify-commit-prefixes.mjs`, one same-wave inconsistency; history is not rewritten, so the correction lives here (sub-cards 1–3 filed by task #1117; sub-cards 4–5 added by task #1123, which found the task-#1117 card had never been updated when the fourth suppression entry appeared, and that `fb7dac8` — the one commit already in `origin/main`, hence the one with the strongest claim to a durable record — appeared in NO index at all).** (Filed 2026-08-19; this card records ONLY the lint/record half — the remediation half belongs to a different agent's scope.)

    - **A round-start reader needs to take NO ACTION on this card.** It is a record, not an open defect: no code, test, or script needs to change as a consequence of it. It exists so a future reviewer reading `git log` for the wave is not misled by the five prefixes below.
    - **Status:** CLOSED on filing (record only).
    - **Current-number-or-verdict: five mis-slots, each premise independently re-verified for this card, not taken on the filing task's word:**
      1. **`09f4d16`, `docs(vmem):` — but it changed the public `Display` of `VmemError`.** The only non-doc-comment line in the commit's whole `src/` delta is the `None =>` arm in `crates/aligned-vmem/src/error.rs`, whose string changed from `"OS virtual-memory error (unknown OS error code)"` to a longer message (re-verified: `git show 09f4d16 -- crates/aligned-vmem/src/error.rs` filtered to non-`///` lines shows exactly that one hunk). That is runtime-observable behavior of a public type on a crate about to be published; R30-12's `docs(...)` slot requires "no code changed at all", so the correct slot was `fix(vmem)`. **The repo's own lint caught it and was skimmed past**: `node scripts/verify-commit-prefixes.mjs a61c582..HEAD` printed a direction-2 WARNING naming `crates/aligned-vmem/src/error.rs` among 09f4d16's out-of-scope paths and still ended `[verify-commit-prefixes] PASS (with warnings above)` with exit code 0 (re-run for this card; output quoted in item 21's established format — the warning reads `... 4 changed path(s) fall outside docs/examples/benches/tests/scripts/: crates/aligned-vmem/README.md, crates/aligned-vmem/src/error.rs, crates/aligned-vmem/src/fault_injection.rs, crates/aligned-vmem/src/os/unix.rs — verify no shipping/opt-in behavior actually changed ...`). A direction-2 warning on a `src/` path that IS a real behavior change is exactly what that warning exists to force a human to check.
      2. **`b11d8be`, `fix(perf):` — nothing executable changed.** Re-verified: `git show b11d8be -- crates/aligned-vmem/src/reservation.rs | grep -E '^[+-]' | grep -vE '^[+-]\s*///' | grep -vE '^(\+\+\+|---)'` is EMPTY (exit 1 — no lines). The commit's four files are a doc comment (`reservation.rs`), a Node script fix (`scripts/verify-vmem-page-constant-call-sites.mjs`), an index re-citation, and a test assertion MESSAGE (`tests/large_reserved_capacity.rs`). R30-12's `fix(perf)` slot claims a correctness/consistency fix in shipping or opt-in code; correct slot was `docs(...)` or `test`.
      3. **`66baf1f` vs `2a35bca` — same category, two prefixes, one wave.** Re-verified via `git show --stat`: both commits touch exactly one file under `crates/aligned-vmem/tests/` (`66baf1f` → `tests/lazy_reservation_debug.rs`, `2a35bca` → `tests/granted_huge_reader_enumeration.rs`), yet the first is prefixed `fix(vmem):` and the second `test(vmem):`. Both are test-only corrections and should have carried the same prefix. (Not grandfathered — neither prefix is an R30-12 taxonomy violation on its own.)
      4. **`c766951`, `fix(perf):` — a commit with NO src/ path at all** (`git show c766951 --name-only --format=` = `docs/CORRECTNESS_OPEN_ITEMS.md` + `tests/no_stale_doc_references.rs`, re-verified for this sub-card). `fix(perf)` claims a shipping/opt-in code fix in perf-sensitive code; the correct slot was `bench:` or `docs(...)`. Caught by `verify-commit-prefixes.mjs` itself within an hour of the check landing; the record commit (`c766951`, which created this very card) landed BEFORE the lint commit and the card listed only three mis-slots — this fourth entry was added by task #1123, closing the gap between the suppression list's "Recorded in item 78" reason line and what item 78 actually said.
      5. **`fb7dac8`, `docs(vmem):` — but its `src/` delta includes a changed assert! panic-message string.** Re-verified: `git show fb7dac8 -- crates/vmem/src/lib.rs` filtered to non-`///` changed lines yields exactly two added string-continuation lines inside the `assert!` message ("NOTE: alignment to runtime page_size() is NOT checked …" / "caller must ensure base/reservation are page_size()-aligned …") — runtime-observable panic text under a `docs(...)` prefix, the same defect class as sub-card 1; correct slot was `fix(vmem)`. This is the ONLY grandfathered commit already in `origin/main` (verified: `git merge-base --is-ancestor fb7dac8 origin/main` exits 0), hence the only genuinely un-amendable one, hence the one with the strongest claim to a durable index record — and until this sub-card it appeared in NO index: `git grep -n "c766951\|fb7dac8"` across both indexes and both CHANGELOGs returned nothing, its sole record being the suppression list itself, exactly the "grandfathered without a durable reason" shape the task-#1114 commit body claimed to be correcting. CLAUDE.md's own rule ("When a gate report / commit / review newly flags an open item, add it to the appropriate index in the same commit") is the authority this sub-card belatedly satisfies.
    - **Next trigger:** none — closed on filing. If the R30-12 taxonomy ever gains a mechanical "same-wave prefix consistency" check, this card is its first regression fixture.
    - **Evidence:** the `git show`/`--stat`/`--name-only` commands quoted in the sub-cards above, re-run 2026-08-19 on branch `vmem-p4-l` for sub-cards 4–5 (sub-cards 1–3 re-run on `vmem-p3-j` at filing); `git grep -n "c766951\|fb7dac8"` returning nothing (pre-this-card); `git merge-base --is-ancestor fb7dac8 origin/main`; `node scripts/verify-commit-prefixes.mjs a61c582..HEAD` (PASS with 3 direction-2 warnings, exit 0); CLAUDE.md's R30-12 rule ("Active rules" section).

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

80. **[T, gap closed by the filing task + permanent record] `scripts/vmem-linux-android-pairing-guard.mjs` ran under NO gate from its creation (M1 wave, task #1105) until task #1131 — seven tasks (#1105, #1114, #1121, #1122, #1128, #1129, #1130) hardened a guard that no gate ever executed, and the creating agent's own report flagged the missing wiring at creation time without it reaching any index: the R22-3 class recurring.** (Filed 2026-08-19, task #1131; the wiring is closed by that same task. This card is the permanent record of the gap plus the two honest downgrades the gap forces.)

    - **Status:** CLOSED (wired by task #1131, both halves in the same edit): (1) `scripts/check-all.mjs` runs the guard as a step immediately after its sibling `vmem-doc-drift-guard` (the sibling keeps step 36; the pairing guard is the new step 37, with steps 38-46 renumbered to match); (2) `.github/workflows/ci.yml`'s `check-matrix` job runs `node scripts/vmem-linux-android-pairing-guard.mjs`. The ci.yml row is load-bearing, not redundant: `check-all.mjs` is the LOCAL pre-push gate and is NOT invoked by any CI job (verified — every one of its six mentions in ci.yml is a comment; `npm run check` is likewise never run by CI), so a local step alone would have left the guard gate-dead in every push whose author skipped `npm run check` — exactly the R33-2 failure mode (70 commits red on `main` because pushes bypassed the local gate).
    - **What did not hold while unwired, stated plainly:** the guard's header claims the KNOWN_DRIFT allowlist is SELF-CLEANING ("an entry that no longer matches anything is itself a FAILURE … so the list cannot silently rot") and that the 13-entry debt list is "printed in every OK run so the debt stays visible, never silent". From creation until task #1131 neither property could hold in practice: nothing ran the check, so stale entries could rot silently and the printed debt was invisible. All eight waves of hardening (#1105→#1130) were each verified by MANUAL invocation only — the counterfactuals in those commits' bodies are real but were never exercised by any gate.
    - **R22-3 class, recurring:** the creating agent's report (`.crush/stdin/p1-b.report.md:51`, a session artifact in the main checkout `D:/dev/rust/sefer-alloc`, not committed to the repo) stated the guard "is not in `scripts/check-all.mjs` / `ci.yml` (agent C's files) — one `<1s` step `node scripts/vmem-linux-android-pairing-guard.mjs` needs adding by C or the orchestrator", adding that no test placement was needed. The follow-up lived only in a report body, reached neither this index nor any other durable record, and was never picked up — exactly the failure mode this file exists to prevent (the header's R19-1 origin, and CLAUDE.md's "add it to the appropriate index in the same commit" rule). The `<1s` runtime claim itself was accurate: 0.289 s measured on the clean tree at wiring time.
    - **Honest downgrade of #1129 (`abe6237`):** its stated harm — appending a realistic `[target."cfg(target_os = \"android\")".dependencies]` section to `crates/aligned-vmem/Cargo.toml` "flips two live drift entries to stale, whose documented remedy is deletion" — was reachable only through MANUAL invocation of the guard, since no gate ran it. The bug was real and the fix (#1129's blank-sentinel window terminator) correct; but while the guard was unwired the harm could not fire in any automated gate, only in the manual runs that were the sole verification path. The MEDIUM rating #1129 was given is therefore WEAKER than stated — the guard invited the deletion mistake only to a human who happened to run it by hand. Not invalidated: those manual runs were the only verification the eight waves had, so the fix still protected the only path that existed.
    - **Next trigger:** none — the wiring is closed; the residual is this record (the R22-3 recurrence note and the #1129 rating downgrade are permanent decision records for how much weight to give findings whose harm path is manual-only).
    - **Evidence:** `rg -n 'vmem-linux-android-pairing-guard' scripts/check-all.mjs .github/workflows/ci.yml package.json` → empty, re-run by task #1131 immediately before editing (both legs of the finding independently confirmed); the seven tasks' commits touching the guard (`abbf27b` #1105, `4116c26` #1114, `51816b4` #1121+#1122, `2e52800` #1128, `abe6237` #1129+#1130, `64aa491` #1130); `.crush/stdin/p1-b.report.md:39` (counterfactual record) and `:51` (the wiring note); clean-tree run at wiring time: exit 0, "scanned 37 .rs files … 13 known-drift allowlist entries active", 0.289 s; the task #1131 counterfactual (scratch copy outside the worktree, `Android` dropped from the README `decommit_lazy` row) → exit 1 naming that row.

81. **[T, record correction — closed on filing] Commit `64aa491`'s body (task #1130, F3+F5+F6) says "the flag [is] set at the five prefix-failure push sites" in `scripts/verify-commit-prefixes.mjs`; there are FOUR.** (Filed 2026-08-19 as task #1133/F5; re-verified by grep by task #1131 before this card was written.)

    - **A round-start reader needs to take NO ACTION on this card.** The script's BEHAVIOUR is right — only the count in the commit body is wrong; `64aa491` stays unamended per CLAUDE.md R30-12's non-retroactive posture (no history rewrites for record corrections).
    - **Status:** CLOSED on filing (record only).
    - **Current-number-or-verdict: FOUR `failures.push` sites carry `taxonomy: true` — lines 509, 528, 545, 569** (re-verified: `rg -n 'failures\.push' scripts/verify-commit-prefixes.mjs` yields six matches; of the five literal object sites, the fifth — line ~647 — is the stale-GRANDFATHERED-entry failure and correctly carries NO `taxonomy: true` flag, because it is not a commit-prefix-classification failure; line 653 spreads pre-built `structuralFailures` and is not a literal site). The `failures.some(f => f.taxonomy)` gating that commit added keys on exactly those four sites, so its described behaviour ("a run whose failures are all structural gets a relevant message") holds with four sites exactly as it would with five — the count in the body was prose, not load-bearing.
    - **Next trigger:** none — closed on filing.
    - **Evidence:** `rg -n 'failures\.push' scripts/verify-commit-prefixes.mjs` → lines 509/528/545/569/647/653; the surrounding lines of each of the five literal sites read for the `taxonomy: true` flag (present only at 509/528/545/569); commit `64aa491` body, line "with the flag set at the five prefix-failure push sites."

82. **[T, MEDIUM — census defect with a live coverage residue] Commit `826f150`'s 56-script census (task #1131) classified script coverage by "invoked by ANY gate" and concluded "a one-off AMONG GUARDS" — but under the coverage standard that same commit itself established and published in item 80, EIGHT local-gate step scripts (not zero, and not the seven the review that flagged this named) had no CI coverage at all; the census also claimed a record it had not made, and its three buckets do not sum to its own denominator of 56. Its substantive conclusion survives; the conclusion that drove the decision does not.** (Filed 2026-08-19, task #1135 record half. Every number below re-measured at commit `db9444f` — the worktree was clean, so worktree == `db9444f`, and the load-bearing greps were re-pinned to `git show db9444f:...` objects so concurrent same-wave ci.yml/check-all.mjs edits cannot shift the citation — not taken from the review's text, which itself proved wrong on two counts, both recorded below.)

    - **Update (task #1137/F3, 2026-08-19): the eighth-script recount inside sub-card (a) was itself corrected — the count of "eight local-gate steps with no non-comment ci.yml row naming that script" is right, but ONE of those eight, `aligned-vmem-semver-check-optional.mjs`, is covered-by-equivalent, not an open gap.** That script is a skip-if-tool-absent WRAPPER around `cargo semver-checks check-release --package aligned-vmem` (re-read in full: `scripts/aligned-vmem-semver-check-optional.mjs`'s header comment and `checkSemver()` body), and that exact command already runs unconditionally as its own CI step in `ci.yml`'s `aligned-vmem-gates` job (`ci.yml:289`; the local step's own comment already says "matches the `aligned-vmem-gates` job's third step", `check-all.mjs:577`). A gap in which SCRIPT NAME has a ci.yml row is not the same as a gap in which INVARIANT is protected — the wave itself already drew this exact line one script over when `0464369` excluded `stale-artifact-diagnosis.mjs` as "provably vacuous". The residue is **seven scripts**, not eight; see the correction inline in sub-card (a) below and the restated Next trigger. **A round-start reader needs to take action on sub-card (a)'s seven-script residue only** — the CI-coverage gap with its own Next trigger below; sub-cards (b) and (c) are record corrections needing no code, test, or script change, and the "what survives" block is context, not a to-do.
    - **Update (task #1143, 2026-08-19): task #1137's "seven-script residue" was ALREADY STALE the moment it was written — the wiring work it called "concurrent task #1136" was in fact task #1135 (commit `0464369`), which landed at 06:26:31, TWO MINUTES BEFORE task #1136 (`9b3d24b`, 06:28:18) wrote this very card at 06:28:18 filing "seven" as the live residue.** `0464369`'s own diff (re-checked directly against `.github/workflows/ci.yml`) added real `- run:` rows for FOUR of the seven: `verify-alloc-core-dbg-internals-exhaustive.mjs` (`:597`), `vmem-doc-drift-guard.mjs` (`:605`), `verify-aligned-vmem-bench-internals-exhaustive.mjs` (`:613`), `verify-vmem-page-constant-call-sites.mjs` (`:625`) — all four confirmed present as non-comment `- run: node scripts/...` rows in the current tree. The remaining THREE — `argv-roundtrip-test.mjs`, `verify-internals-negative-boundary.mjs`, `stale-artifact-diagnosis.mjs` — were explicitly EXCLUDED by `0464369`'s own commit body (self-test-only, shells-out-to-cargo, and reads-no-repo-files reasons respectively, each with its own rationale stated there) and remain genuinely unwired. **The true current residue is THREE scripts, not seven.** This is not sub-card (a)'s arithmetic being wrong a second time — the seven-script COUNT at `db9444f`'s measurement time was correct; the defect is that the very next commit (`0464369`, task #1135) that reduced it to three landed simultaneously with (and just before) the commit (`9b3d24b`, task #1136) that filed this card citing the pre-reduction number as current and pointing at the wiring task by the wrong number ("#1136" — the docs task that wrote this card — instead of "#1135", the actual wiring commit). Neither task #1137 (08:28:38, same day) nor task #1138 (09:30:27) caught this despite both editing this same card afterward — both corrected other figures in sub-card (a)/(d) without re-running the per-script ci.yml check the Next trigger below already specified.
    - **Status:** OPEN on a THREE-script residue (`argv-roundtrip-test.mjs`, `verify-internals-negative-boundary.mjs`, `stale-artifact-diagnosis.mjs` — deliberately excluded by task #1135/commit `0464369`, not merely unwired-so-far); `aligned-vmem-semver-check-optional.mjs` is CLOSED as covered-by-equivalent (task #1137/F3, `ci.yml:289`); FOUR of the original seven (`verify-alloc-core-dbg-internals-exhaustive.mjs`, `vmem-doc-drift-guard.mjs`, `verify-aligned-vmem-bench-internals-exhaustive.mjs`, `verify-vmem-page-constant-call-sites.mjs`) are CLOSED — wired by task #1135/commit `0464369`, `ci.yml:597/605/613/625`; sub-cards (b), (c) and (d) CLOSED on filing by this card. Any future round revisiting the three-script residue should re-decide each on its own merits (the exclusion reasons in `0464369`'s body are a recommendation, not a permanent ruling) rather than treat "excluded once" as "settled forever".
    - **Current-number-or-verdict — four defects recorded, one live residue (sub-card (d) added by task #1138/F4, a record correction to a related same-wave commit, not a fourth census defect proper):**
      1. **(a) The predicate erased the exact distinction the commit itself said was load-bearing.** The census counted a script "covered" if ANY gate invoked it, and `check-all.mjs` membership was enough. But `826f150`'s own index text (item 80's Status card, written in the same commit) states the opposite standard: the ci.yml row is "load-bearing, not redundant" because `check-all.mjs` is the LOCAL pre-push gate, invoked by NO CI job, and pushes bypass it — the R33-2 failure mode (`main` red across up to 70 commits). Under that standard, "runs in check-all" is not coverage; it is precisely the exposure the wave existed to close. Re-measured at `db9444f`: of the 13 scripts `check-all.mjs` runs as steps, SEVEN have zero non-comment run-rows in ci.yml AND no equivalent substantive check running elsewhere in ci.yml — `argv-roundtrip-test.mjs` (check-all step at `:273`), `verify-internals-negative-boundary.mjs` (`:603`), `verify-alloc-core-dbg-internals-exhaustive.mjs` (`:618`), `vmem-doc-drift-guard.mjs` (`:671`), `verify-aligned-vmem-bench-internals-exhaustive.mjs` (`:705`), `stale-artifact-diagnosis.mjs` (`:718`, and in `--self-test` mode only, so the real diagnostic never runs under any gate), `verify-vmem-page-constant-call-sites.mjs` (`:738`). (check-all's final step `iai.mjs` at `:980` also has no CI row, but is environment-gated by design — WSL + valgrind, skipped with a warning — and is deliberately not part of this gap claim.) After `826f150`, the pairing guard is indeed the only vmem text guard with a CI row; `vmem-doc-drift-guard`'s single ci.yml mention is a comment (`:506`). **The review that flagged this defect listed SEVEN scripts and missed `aligned-vmem-semver-check-optional.mjs`** (`:583`) — the RECOUNT was right (there are eight local-gate step scripts with no non-comment ci.yml row naming that exact script), **but the conclusion drawn from the recount was wrong, and this is the more important correction: the eighth script is not an eighth gap.** `aligned-vmem-semver-check-optional.mjs` is, by its own header comment and its own `checkSemver()` body (read in full for this correction), a skip-if-tool-absent WRAPPER around exactly one command — `cargo semver-checks check-release --package aligned-vmem` — and that exact command already runs, unconditionally, as its own step in ci.yml's `aligned-vmem-gates` job (`ci.yml:289`; the local step's own comment even says so: "matches the `aligned-vmem-gates` job's third step", `check-all.mjs:577`). The wrapper's non-vacuous branch (tool present → run the real check → propagate its exit code) is not a DIFFERENT check CI never runs; it is the SAME check CI runs unconditionally, guarded locally only because a contributor's machine might lack `cargo-semver-checks`. That is a difference in **script-name coverage**, not in **protection**: the substantive invariant already has a mandatory CI row under a different filename. This is the same standard the wave itself already applied one script over: commit `0464369` (task #1135, the wiring half of this same finding) EXCLUDED `stale-artifact-diagnosis.mjs` from its four added rows specifically because a bare CI invocation "could never fail: provably vacuous" — a script can be guard-SHAPED (`process.exit(1)` present, a real repo invariant checked) and still be the wrong thing to give a redundant row. `aligned-vmem-semver-check-optional.mjs` fails the OPPOSITE way: its own substantive check already has a CI row, just spelled `cargo semver-checks check-release --package aligned-vmem` instead of the wrapper's filename. **The seven-script list above is therefore the corrected residue, not eight** — `aligned-vmem-semver-check-optional.mjs` is COVERED-BY-EQUIVALENT (`ci.yml:289`), reclassified out of the open gap by this correction, not left pending on a future wiring task. **The lesson, stated plainly because it is the orchestrator's own and it recurs: the recount found a NAME missing from a list; it never checked whether the thing that name denotes is already protected under a different name. Being right that "seven" undercounted did not make "eight, and all eight need rows" the right conclusion — the corrected count is eight NAMES with no row for that name, of which one denotes an already-covered invariant and seven do not.**
      2. **(b) "recorded not fixed" was false — the R22-3 class, committed by the very commit diagnosing the R22-3 class.** `826f150`'s body says the sibling guard's local-only status was "out of scope here, recorded not fixed". Verified by grep at filing time (pre-this-card): `vmem-doc-drift-guard` appears in this index ONLY at `:2282` (an incidental mention inside item 80's closure text, which records the PAIRING guard's wiring, not the sibling's gap) and in two long-CLOSED items (56, 71 — both about different defects, both closed 2026-08-16/18); in `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` only inside those two items' closure narratives; in `docs/perf/OPEN_ITEMS.md` zero times. No index recorded the CI gap; it lived only in a commit body — the exact failure mode ("flagged in a report/commit body, reached no index") that `826f150` was written to diagnose for the creating agent's report, and a direct violation of CLAUDE.md's "when a gate report / commit / review newly flags an open item, add it to the appropriate index in the same commit". `826f150` even quotes CLAUDE.md's rule at the creating agent; its own body then did the same thing the rule forbids. This card is that record, one wave late; sub-card (a) is what it should have said.
      3. **(c) The buckets do not partition the denominator: 17 + 10 + 28 = 55, and there are 56 scripts. The unaccounted 56th is `scripts/capture-measurement-identity.mjs`.** Re-derived by set arithmetic with every membership verified by a NON-COMMENT reference: the gate∪npm union has exactly 27 members — check-all's 13 step scripts, plus `check-all.mjs` itself (npm `check`), plus `run-check-matrix.mjs` (ci.yml `:487`), plus the transitive `lib.mjs`/`check-matrix.mjs` imports, plus 10 package.json-only entrypoints (`loom`, `miri`, `tsan`, `asan`, `fuzz`, `first-alloc-bench`, `dealloc-only-bench`, `paired-ab-runner`, `r34_7_causal_harness`, `bench-table`) — so 29 scripts are unwired, and 28 of those 29 carry the `rN_*` round prefix that the census's "measurement/derivation one-offs" description denotes, exactly matching its stated 28. The 29th, `capture-measurement-identity.mjs`, is the only unwired script with no round prefix: a standing R29-6 identity-capture helper (recommended by `verify-gate-report.mjs`'s header comments and cited by CLAUDE.md's provenance rule) that no gate, no npm script, and no other script invokes — its only references anywhere are comments (`verify-gate-report.mjs:104`/`:1169`; **`:1224` is a template-literal fragment inside a diagnostic message in executable code, NOT a comment — see the Evidence line below for the reading; the substantive claim here is unaffected: `capture-measurement-identity.mjs` still has zero references from anything that INVOKES it, comment or code — `:1224` merely NAMES the script in a warning string, it does not call it**), `.gitignore:33`, its own header, and docs). It is neither a gate step, nor an npm entrypoint, nor an rN-prefixed one-off, so it fell out of all three buckets — and it is also the only unwired script that is NOT a one-off at all, which is why its absence from the census's residue analysis matters slightly more than an off-by-one arithmetic quirk. Residual uncertainty, stated plainly: the census was an uncommitted one-off (its per-bucket lists are unrecoverable), so this identification rests on verified set arithmetic over the reference relation, not on reading the census's own output; under either defensible placement of `iai.mjs` (gate bucket, since check-all runs it directly at `:980`, or npm bucket, since package.json lists it) the unwired bucket is still 29 and the census's 28 still equals exactly the round-prefixed subset, so the identification is robust to that ambiguity.
      4. **(d) `0464369`'s own body cites "all ten `check-all` mentions were comments" — a figure matching NEITHER revision it could plausibly mean.** (Filed 2026-08-19, task #1138/F4.) `0464369`'s commit body draws an analogy: "The trap that hid it is worth naming: `vmem-doc-drift-guard`'s ONLY ci.yml mention (line 506) is a COMMENT. That is the same shape as #1131's own finding, where all ten `check-all` mentions were comments." But `grep -c "check-all" .github/workflows/ci.yml` measured across the three candidate revisions gives 6/9/9, never 10: at `826f150~1` (before task #1131's wiring commit) there are 6 lines containing "check-all", all 6 comments; at `826f150` (task #1131's own landing commit) there are 9; at `db9444f` (unchanged from `826f150` on this file) still 9. Lines equal occurrences at all three revisions (each match is on its own line, so this is NOT the task-#1125 lines-vs-matches counting-method class), so "ten" is not a miscount of either count by that mechanism — it simply matches no measured state. **Six is exactly what item 80's own Status card (written by task #1131, the commit `0464369` is describing) already says**: "every one of its six mentions in ci.yml is a comment" (`db9444f:docs/CORRECTNESS_OPEN_ITEMS.md:2282`, this index, quoted verbatim) — i.e. the correct figure was already sitting in this same file, one item up, when `0464369`'s body was written. The substantive analogy `0464369` draws (a script's sole ci.yml mention being a comment, not a real row) is unaffected by which of 6/9/10 is cited — item 80's six comment-only mentions (pre-wiring) and `vmem-doc-drift-guard`'s one comment-only mention are the same SHAPE of trap regardless of the exact count — but the number itself is simply wrong and is corrected here.
    - **What survives — recorded so the card says which kind of failure this was:** the census's SUBSTANTIVE conclusion was independently re-derived and HOLDS: no fully-unwired script is guard-shaped, and no unwired script's silence disables a standing protection. Re-measured here, with two corrections to the review's own re-derivation (the review's numbers were also off, both in the direction that makes the surviving conclusion stronger, not weaker): its "only three contain `process.exit(1)`" is actually SIX — `r10_2_medium_gate.mjs:167`, `r10_5_large_cache_gate.mjs:239`, `r34_23_realloc_direct_harness.mjs:275`, `r34_23_vec_harness.mjs:303` as top-level `main().catch()` handlers, `capture-measurement-identity.mjs:183` as a top-level `catch`, `r31_2_derive_report_data.mjs:27` as a usage-argument check — all six are error propagation in one-off drivers, none a red/green verdict exit that any gate consumes; and its seven-script CI-gap list missed one (sub-card (a)). `r10_2_medium_gate.mjs` and `r10_5_large_cache_gate.mjs` are the only unwired scripts named "gate", and both are one-command reproducers that print a verdict for a human. This is therefore a CORRECT conclusion reached by a flawed census, not a wrong conclusion — but the conclusion that mattered for the decision ("a one-off AMONG GUARDS") is exactly the one the predicate defect invalidates: the true uncovered set was never "zero guard-shaped scripts", it was "every local-gate step that CI does not run", which at `db9444f` was eight scripts.
    - **Next trigger (corrected by task #1143, 2026-08-19 — see the Update above):** task #1135/commit `0464369` already wired FOUR of the original seven — `verify-alloc-core-dbg-internals-exhaustive.mjs`, `vmem-doc-drift-guard.mjs`, `verify-aligned-vmem-bench-internals-exhaustive.mjs`, `verify-vmem-page-constant-call-sites.mjs` (confirmed present as non-comment `- run:` rows at `ci.yml:597/605/613/625`). The residual trigger now applies to only the **three-script** remainder — `argv-roundtrip-test.mjs`, `verify-internals-negative-boundary.mjs`, `stale-artifact-diagnosis.mjs` — each deliberately excluded by `0464369`'s own commit body (self-test/shells-out-to-cargo/reads-no-repo-files reasons respectively). `aligned-vmem-semver-check-optional.mjs` remains EXCLUDED (covered-by-equivalent via `ci.yml:289`'s unconditional `cargo semver-checks check-release --package aligned-vmem` step, not an open gap) — a future wiring task adding a redundant `cargo semver-checks` invocation under that script's own name would not close anything real. A future round revisiting the three should re-derive the per-script row check fresh (`grep -nE "run: .*scripts/" .github/workflows/ci.yml`) rather than trust either this card's or `0464369`'s snapshot, per item 76's add-or-re-defer-explicitly convention. If all three are resolved (or explicitly re-deferred with a reason), this card closes in full.
    - **Evidence:** per-script `grep -c` of each name against `.github/workflows/ci.yml` and `scripts/check-all.mjs`, then re-pinned to `git show db9444f:.github/workflows/ci.yml` / `db9444f:scripts/check-all.mjs` (worktree clean at measurement time; `git status --short` empty); `grep -nE 'run: .*scripts/'` on `db9444f:ci.yml` → exactly five rows (`:487` run-check-matrix, `:495` verify-perf-gate-stubs, `:504` verify-gate-report, `:520` vmem-linux-android-pairing-guard, `:557` verify-commit-prefixes); `grep -n "semver-checks check-release" .github/workflows/ci.yml` → `:289` (`--package sefer-region` sibling at `:104` is a different package, not evidence for this crate) — confirms the wrapper's non-vacuous branch is already covered; `scripts/aligned-vmem-semver-check-optional.mjs` read in full (its header comment and `checkSemver()` body: skip-if-absent, else run the exact `ci.yml:289` command and propagate its exit code); `scripts/check-all.mjs:572-578`'s own step comment ("matches the `aligned-vmem-gates` job's third step"); commit `0464369` body (the `stale-artifact-diagnosis` exclusion precedent: "A bare CI row could never fail: provably vacuous"); `grep -n "vmem-doc-drift-guard" docs/CORRECTNESS_OPEN_ITEMS.md docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md docs/perf/OPEN_ITEMS.md` (pre-this-card: `:2282` incidental + items 56/71 closed; perf index zero hits); `ls scripts/*.mjs | wc -l` → 56, and `git ls-tree 826f150 --name-only scripts/ | grep -c '\.mjs$'` → 56 (`db9444f` touched only `crates/aligned-vmem/CHANGELOG.md`, so the census-time and filing-time sets are identical); the per-script non-comment reference census behind the 27-member gate∪npm union; `grep -n "process.exit(1)"` over the 29 unwired scripts (six hits, contexts read for each); commit `826f150` body ("17 are gate-invoked … 28 are unwired … a one-off AMONG GUARDS"; "out of scope here, recorded not fixed"; the `.crush/stdin/p1-b.report.md` R22-3 diagnosis it applies to others); for sub-card (d): `grep -c "check-all" .github/workflows/ci.yml` at `git show 826f150~1:...` → 6, at `git show 826f150:...` → 9, at `git show db9444f:...` → 9 (never 10); `grep -n "check-all" .github/workflows/ci.yml` at each revision (line-per-match, confirming lines == occurrences, ruling out the task-#1125 lines-vs-matches class); `db9444f:docs/CORRECTNESS_OPEN_ITEMS.md:2282` (item 80's own Status card, same commit family, stating "six"); commit `0464369` body ("all ten `check-all` mentions were comments"); for the `:1224` correction: `scripts/verify-gate-report.mjs:1220-1228` read directly — `:1224` is inside a `detail:` template-literal string assembled by executable code (a diagnostic message), not a `//` comment, while `:104` and `:1169` genuinely are `//` comments (verified by reading each in context).

83. **[T, LOW (F3) + INFO (F4) — record correction, closed on filing] Commit `db9444f`'s body (task #1133) claims the re-wrapped CHANGELOG entry has "zero flagged lines (was four)" — true only under a carve-out the body never names — and justifies the one short line it introduced with a wrong figure, in a commit whose entire subject was that a previous commit's figures were wrong.** (Filed 2026-08-19, task #1136 F3+F4; both parts re-measured mechanically at `64aa491` and `db9444f`, greedy fill to 78 with backtick code spans atomic, first-atom = the next line's first whitespace-delimited token with spans kept whole including gluing punctuation.)

    - **Update (task #1138/F5+F6, 2026-08-19): this card's OWN two figures needed correction — the entry's line range was off by one (`:111`-`:132` stated, `:111`-`:133` true, matching this card's own "23-line entry" count), and the F4 replacement figure for the code span (48) was itself wrong (46 — see the rewritten F4 sub-point below for the three-generation history: 52 → 48 → 46).** A round-start reader still needs to take NO ACTION — both corrections are prose/count-only, the published CHANGELOG text is unaffected, and this card stays CLOSED on filing per its original scope; the corrections are folded into the sub-points below rather than left as a separate stale layer, since this card is itself a record of exactly this defect class and a reader should not have to cross-reference a later item to get the current numbers.
    - **A round-start reader needs to take NO ACTION on this card.** The published CHANGELOG text itself is correct and render-byte-identical across the rewrap (independently re-verified below); only the commit body's self-description is wrong. `db9444f` stays unamended per CLAUDE.md R30-12's non-retroactive posture — this card is the record.
    - **Status:** CLOSED on filing (record only).
    - **Current-number-or-verdict — two corrections to one commit body:**
      1. **F3: "zero flagged lines (was four)" rests on an unnamed carve-out.** Pure greedy at `64aa491` (PRE) flags FIVE lines, not four: `:113` len 73 (next atom `is`, 2, would fit at 76), `:114` len 70 (`"Fixed"`, 7, fits at 78), `:115` len 71 (`(task`, 5, fits at 77), `:116` len 38 (`independent`, 11, fits at 50), and `:123` len 53 (`**Migration:**`, 14, fits at 68). The four lines the body names — `:113`-`:116`, with exactly those would-fit widths — are enumerated correctly; what is missing is the fifth. `:123` is flagged in BOTH states (PRE and HEAD carry the identical line `  (` + the `error[E0596]` code span + `).` followed by `  **Migration:** bind …`), and it is the only line between HEAD's true count of one and the claimed zero. Both of the body's figures — "was four" (PRE) and "zero" (HEAD) — are reachable only by silently treating `:123` as a terminal for its sentence group, an exception the body never states. Applied consistently to both sides, the carve-out makes the DELTA honest (4 → 0); left unstated, it makes the ABSOLUTE claim ("zero flagged lines") false, and `:123` remains 15 columns short of greedy at HEAD (53 + 1 + 14 = 68 ≤ 78). The carve-out is not a rendering boundary: the entire 23-line entry (`:111`-`:133`) is ONE Markdown paragraph — no blank line anywhere inside it, including between `:123` and `:124` — which `db9444f`'s own byte-identity check demonstrates by joining all 23 lines. A sentence-group terminal is a defensible wrap style; a metric with an unnamed style exception is not the metric the prose describes.
      2. **F4: the "52-char code span" figure — corrected once to 48 by task #1136, and that correction was ALSO wrong; the true figure is 46. Three generations of one defect are recorded here.** The original `db9444f` body says `:122=31` "IS maximal: the next atomic token is a 52-char code span that fits in no 78-window behind that much prose" (generation 1: **52**). Task #1136 corrected this card to say the code span is 48 chars (generation 2: **48**) — but 48 is the length of the span WITH its backtick delimiters included (`` `error[E0596]: cannot borrow *shared as mutable` ``, backtick-to-backtick inclusive, measured 48 chars), not the code text *between* the backticks, which is what "the code span" denotes in ordinary usage and what this card's own words ("the code SPAN itself, ... between its backticks") say it is measuring. Measured directly (generation 3, this correction): `:123` raw length 53; the full glued ATOM (`(` + backtick-delimited span + backtick + `).`) is 51 chars; the span WITH its two backticks is 48 chars; the span text WITHOUT the backticks — `error[E0596]: cannot borrow *shared as mutable`, the actual code quoted — is **46** chars (`echo -n 'error[E0596]: cannot borrow *shared as mutable' | wc -c` → 46). So three figures coexist for three different substrings of the same line, and none of them is 52: the atom is 51, the delimited span (backticks included) is 48, the code text alone (backticks excluded, what "code span... between its backticks" describes) is 46. The CONCLUSION is unaffected by any of these three generations: `:122` len 31, and even under the smallest of the three candidate widths (46 + 2 backticks + 1 leading space + 1 trailing `).` needs the full 51-char atom to actually wrap), 31 + 1 + 51 = 83 > 78, so `:122` is genuinely maximal and splitting the code span was rightly refused. The three-generation history is the more instructive fact than any single figure: `db9444f` (task #1133) introduced 52 while correcting a PRIOR commit's wrong counts; task #1136 corrected 52 → 48 while filing THIS card, whose own subject is "figures wrong in a commit about figures being wrong" — and got its own replacement figure wrong, by conflating "the span between the backticks" (what its prose said) with "the span including the backticks" (what its arithmetic actually measured); this card's third pass corrects 48 → 46 and keeps the figure and its gloss consistent with each other. A defect that survives being corrected twice, each correction landing in a commit whose subject is correcting the same class of error, is not a fluke of one commit — it is evidence that measuring a quoted code span's length by eye, without a `wc -c`/`len()` check committed alongside the prose, reproduces the mistake it is trying to fix.
    - **Next trigger:** none — closed on filing. If the 78-column wrap rule is ever mechanized into a guard script, the sentence-group-terminal exception must be an explicitly named rule (not an implicit judgement), and `:123` of this entry is its first regression fixture; `:122` is the counter-fixture proving genuine maximality is still detectable.
    - **Evidence:** line-length/first-atom re-derivation at both revisions (`git show 64aa491:crates/aligned-vmem/CHANGELOG.md` and HEAD, entry at `:111`-`:133` in both — NOT `:111`-`:132`; `:133` is `  free \`pub unsafe fn\` API, which is unaffected.` and `:134` opens the next bullet with `- `, confirmed by direct line read at both revisions; this off-by-one, in a card about off-by-one errors, is corrected here by task #1137/F5) — PRE flagged `[113, 114, 115, 116, 123]`, HEAD flagged `[123]`, HEAD with `:123` excluded `[ ]`; `git diff 64aa491..db9444f -- crates/aligned-vmem/CHANGELOG.md` (the 10-line reflow hunk); whitespace-normalised sha256 of the 23-line entry (the correct `:111`-`:133` range — the cited hash reproduces ONLY over this range, not over `:111`-`:132`) = `9bedbba57ff1179917e97930d711b6db2c668dd3500f324dde933c567731279e` at BOTH revisions, matching the prefix `db9444f`'s own body quotes (byte-identity independently reproduced); HEAD `:121`-`:124` read directly (no blank line between `:123` and `:124`); commit `db9444f` body ("Post-fix the entry has zero flagged lines (was four)"; "the next atomic token is a 52-char code span").

84. **[T, accepted coverage reduction — not a defect] `aligned-vmem`'s miri job loses BOTH of its source-text guards: `granted_huge_reader_enumeration_is_pinned` and `no_borrowed_reservation_escapes_lazy_reservation` are now `#[cfg_attr(miri, ignore)]`, because miri's default filesystem isolation forbids the `std::fs::read_dir` walk of `src/` that both guards are built on.** (Filed 2026-08-19, task #1147. Two waves collided, neither aware of the other: the `aligned-vmem-miri` job was added by task #1057 to close items 41/61, and the two guards were added independently by tasks #1103 and #1104/#1113.)

    - **A round-start reader needs to take NO ACTION on this card.** Both guards still run, and still fail when they should, under every non-miri `cargo test` invocation. What is lost is only their miri-INTERPRETED execution — and miri interpretation was never their purpose: they are pure text scans over `.rs` files, with no memory-model or UB content whatsoever.
    - **Status:** OPEN as an accepted, recorded coverage reduction. Recorded here rather than left in a commit body precisely because that is R22-3's class — a follow-up that reaches no index is lost.
    - **Current-number-or-verdict:** `cargo miri test -p aligned-vmem` and `... --all-features` both complete with both guards reported `... ignored`. **The second guard was MASKED, not absent:** miri aborts on the first failure, so landing SHA `1ed79e96`'s CI log named only `granted_huge_reader_enumeration`; `lazy_reservation_no_borrowed_reservation.rs` fails identically and was found only by isolating the later-ordered binaries. Independently confirmed at filing that these two are the complete set: `grep -rln "std::fs\|fs::read_dir\|std::process\|Command::new\|std::net" crates/aligned-vmem/tests/ crates/aligned-vmem/src/` returns exactly those two files and nothing else.
    - **Counterfactual, re-verified at filing (a guard that cannot fail is not a guard):** perturbing `granted_huge_reader_enumeration.rs`'s `"src/os/unix.rs" => (0, 0, 0, 7)` expectation to `999` produces a red `cargo test` with the real mismatch printed; injecting a bogus name into `lazy_reservation_no_borrowed_reservation.rs`'s pinned method list does the same. Both restored to green.
    - **Host-dependence of the error text (know this before reproducing):** on Linux CI miri reports ``unsupported operation: `opendir` not available when isolation is enabled``; on a Windows host the same root cause surfaces as ``can't call foreign function `FindFirstFileExW` ``. Same sandbox, OS-specific foreign-function name — a reproduction that greps for the Linux string on Windows will wrongly conclude the defect is gone.
    - **Next trigger — option (c), moving both guards to `scripts/`, is the correct permanent home and is NOT blocked by any technical loss.** Explicitly evaluated at filing and found lossless: both guards use exactly one Rust-specific construct, `env!("CARGO_MANIFEST_DIR")`, and are otherwise byte/string scanning; neither uses `cfg!`, macros, or type information, and `scripts/vmem-doc-drift-guard.mjs` already does the identical class of work. It was NOT attempted here because `lazy_reservation_no_borrowed_reservation.rs`'s scanner is a six-class hand-rolled analyser guarding the H1 borrow-leak property, and a subtly wrong port would silently weaken a safety guard — worse than an honestly-recorded `#[ignore]`. That migration needs its own zero-trust review plus a `scripts/check-all.mjs` step and a `ci.yml` row.
    - **Option (b) — `-Zmiri-disable-isolation` on the job — is rejected, and would silently reverse a documented decision.** Task #1057 chose `MIRIFLAGS: -Zmiri-ignore-leaks` over `-Zmiri-disable-isolation` deliberately and recorded why in the job's own comment (`.github/workflows/ci.yml`): the only intentional-leak case is `tests/smoke.rs`'s `leak_zeroed_pages_is_zeroed_and_static`, and that narrower flag sufficed. Widening to disable isolation would weaken the whole job's sandbox for the sake of two text tests.
    - **Evidence:** `crates/aligned-vmem/tests/granted_huge_reader_enumeration.rs` and `crates/aligned-vmem/tests/lazy_reservation_no_borrowed_reservation.rs`, both `#[cfg_attr(miri, ignore)]` with inline comments cross-referencing each other and this card; the `aligned-vmem-miri` job in `.github/workflows/ci.yml` (`dtolnay/rust-toolchain@nightly` + `components: miri`, `MIRIFLAGS: "-Zmiri-ignore-leaks"`, `ubuntu-latest`); CI landing SHA `1ed79e96`'s `aligned-vmem under miri` job log — the single-failure report that triggered this task and that, by aborting, concealed the second guard.

86. **[T, process/indexing-hygiene — decision recorded, deliberately deferred] Both `docs/CORRECTNESS_OPEN_ITEMS.md` (2,423 lines) and `docs/perf/OPEN_ITEMS.md` (2,393 lines) are again past the ~1,000-line threshold CLAUDE.md's R34-24 rule flags for another archive split — this task explicitly decides NOT to perform that split now, and records why.** (Filed 2026-08-19, task #1143.)

    - **Status:** OPEN — decision recorded (defer), not a code/doc defect requiring immediate action; a future round should perform the split as its own dedicated, careful task.
    - **Current-number-or-verdict:** `docs/CORRECTNESS_OPEN_ITEMS.md` = 2,423 lines (already has a sibling archive, `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`, 1,512 lines, from the task #1109 split); `docs/perf/OPEN_ITEMS.md` = 2,393 lines (already has a sibling archive, `docs/perf/OPEN_ITEMS_ARCHIVE.md`, from the R29-6 split). Both mains are roughly 2.4× the ~1,000-line threshold. Re-splitting either main file further (moving more current-tier card bodies into their archives, the same mechanism R34-24 describes) would reduce both back toward the threshold.
    - **Why this task defers rather than performs the split:** three independent reasons, each sufficient on its own. (1) **Scope:** this task's declared edit scope is exactly three files (`docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`, `docs/perf/OPEN_ITEMS.md`) — a correct split of `docs/perf/OPEN_ITEMS.md` requires writing INTO `docs/perf/OPEN_ITEMS_ARCHIVE.md`, which is outside that scope (this task already reverted one such edit made in the course of fixing item 32 above, once the scope constraint was reconsidered — see that item's history for the reverted `§ D32` archive section). (2) **Demonstrated risk:** this campaign's own history shows a mechanical index split is NOT a safe, routine operation — task #1109's R34-24 split of the correctness index truncated 9 "Recently resolved" pointers mid-heading and lost the verdict on 19 of 32 cards, discovered only later by task #1116 and fully corrected by task #1123's stronger self-checking test (`tests/no_stale_doc_references.rs::correctness_index_recently_resolved_pointers_carry_verdicts`, which this task re-ran clean — see the Verification section of this task's own report). A rushed re-split, folded into a 9-point remediation task alongside eight unrelated corrections, is exactly the condition (time pressure, divided attention, no dedicated review pass) that produced the original defect. (3) **No structural urgency:** unlike a broken pointer or a duplicate item number (both fixed elsewhere in this same task), file length alone does not silently mislead a reader the way those defects do — a 2,400-line file is slower to read end-to-end (the cost R34-24's own rule is written against) but does not misstate any item's status. The cost of deferring one more round is "round-start reads stay slower than ideal," not "a reader is told something false."
    - **Next trigger:** a future round performs the split as ITS OWN dedicated task (not folded into a multi-point remediation), following the established R29-6/R34-24 mechanism (move full card bodies to the sibling archive, leave a one-line current-state pointer in the main file) — and, per the lesson task #1109 → #1116 → #1123 already taught this campaign, must re-run (or write, for `docs/perf/OPEN_ITEMS.md`, which currently has no equivalent) a pointer-resolution self-check test IMMEDIATELY after the split, in the same commit, rather than trusting the mechanical edit was correct. `docs/perf/OPEN_ITEMS.md` has no `no_stale_doc_references.rs`-style self-check today (confirmed: that test only opens `docs/CORRECTNESS_OPEN_ITEMS.md`/`docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md`) — a perf-index split without first adding an equivalent check would repeat task #1109's mistake with no safety net at all, a strictly worse position than the correctness index was in.
    - **Evidence:** `wc -l docs/CORRECTNESS_OPEN_ITEMS.md docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md docs/perf/OPEN_ITEMS.md` (this task, 2026-08-19); CLAUDE.md's R34-24 "Phased delivery" bullet (the split rule and its ~1,000-line trigger); task #1109's split commit and task #1123's correction (referenced throughout this index's other cards, e.g. items 81-83); `tests/no_stale_doc_references.rs::correctness_index_recently_resolved_pointers_carry_verdicts` (the self-check that exists for the correctness index only).

---

## Recently resolved (closure trail — do not re-list as open)

**Full closure narratives moved to the archive (R34-24, task #1109, 2026-08-18).**
Each pointer line below is the moved entry's original header line (verbatim,
item number unchanged), followed by the relocation note; the full byte-identical
narrative lives in `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved —
full closure trail".

- 77. **[M1, record correction — closed on filing] Commit bodies `d58bd67` (task #1086) and `a988e51` (task #1085) both claim a below-real-page skip treatment that only TWO of the THREE forced-page test files actually received: the record, not the code, was wrong.** (Filed and corrected 2026-08-18, task #1096/finding M1.) — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 51. **`aligned-vmem`'s `#[cfg(unix)]` code was never compiled by the standard LOCAL verification matrix on the campaign's Windows host** (filed round 8, task #904, finding UC5 of `docs/reviews/2026-08-13-aligned-vmem-round8-closing-review.md`) — **CLOSED** by task #1059, option (a): a permanent cross-target gate in `npm run check`. **Reopened and re-closed by task #1071 — see the CORRECTION block below.** — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 54. **[T, INFO] Tautological tests and small untested corners** (Filed 2026-08-14, task #934/C-9, combining findings V-29 and V-31 of `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`) — **CLOSED** by task #1058. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 41+61. **`aligned-vmem` had NO `cargo miri test -p aligned-vmem` step anywhere in CI** (item 41: the missing step; item 61: the same gap phrased as runtime-semantics concern — one fix, closed together) — **CLOSED** by task #1057. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 69. **Flaky test: `safe_decommit_over_never_committed_tail_succeeds` intermittently read `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS` as 0 instead of 1 under full-suite parallel load** — **CLOSED** by task #1063. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 68. **Name asymmetry in `Reservation` decommit capability API** (`Reservation::decommit_reclaims_and_zeroes()`, associated `const fn`, compile-time capability query, vs. `Reservation::can_decommit_reclaim_and_zero()`, instance method combining compile-time capability with runtime `is_huge()`) — **CLOSED** by task #1052, no code change (option (c): asymmetry accepted as-is). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 46. **`numa-shim`'s public `reserve_on_node` signature returns `aligned_vmem::Reservation`, coupling the crate's own semver to `aligned-vmem 0.2`** — **CLOSED** by task #1053 (option (a): coupling accepted and documented; `pub use aligned_vmem::Reservation;` added to `numa-shim`, gated `#[cfg(feature = "vmem-integration")]`). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 66. **`Reservation` carried no committed-length state (R6-1 / R7-2, the second of R7's two conditional-NO-GO conditions).** — **CLOSED** by task #1051, commit `0c1e6c4`; the surface it shipped initially leaked the watermark back out through `as_reservation()` and was re-sealed by task #1104 (publication-audit finding H1 — see the correction bullet below). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 50-U10. **`aligned-vmem` — the U10 half of item 50 ("Windows `bench-internals` reserve-path counters have zero test coverage") rested on a FALSE premise and is closed.** (Filed round 8, task #903, finding U10 of `docs/reviews/2026-08-13-aligned-vmem-round8-review.md`; re-flagged as stale by R7-9 and closed by task #1045.) — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 1. **Flaky test — `canary_survives_promotion_and_free_leaves_no_leak`** (`tests/r14_4_promotion_free_correctness.rs`) — **RESOLVED** by an urgent CI-fix task (2026-07-26), responding to `origin/main` CI run `30217256247` / job `89833506941` failing on the `test (--features "hardened medium-classes")` step with `error: 1 target failed: --test r14_4_promotion_free_correctness`. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 3. **Flaky test — `shadow_path_activation_oracle_fast_and_slow_both_reachable` scheduler-sensitive percentage thresholds (BOTH regimes).** **RESOLVED** in TWO steps, both 2026-08-16 — do not read this card as landing in a single commit: (a) the root-cause fix, commit `8d68715` (task #1030): the `SERIAL` guard plus exact-equality assertions; (b) a portability follow-up, the commit carrying this entry (task #1033, finding F5 of `docs/reviews/2026-08-16-aligned-vmem-r6-wave-review.md`): (a)'s exact equalities were only valid on strong-CAS targets, and were replaced by two-sided bounds. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 2. **Clippy dead-code — `--features "hardened medium-classes"` was not clippy-clean (11 errors)** — **RESOLVED** by R23-5 (task #374). All 11 were genuine `#[cfg(...)]` predicate mismatches (an item gated one way, its only consumer gated a DIFFERENT way, so under the specific intersection `hardened medium-classes` the consumer compiled out but the item did not) — confirmed exhaustively per item via `grep` across `src/`, `tests/`, `benches/`, `crates/` before touching anything; NONE were genuine orphans, so nothing was deleted. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 3. **Deferred decision — `aligned-vmem`'s `mock` Cargo-feature-unification hazard was resolved with a doc-only fix, explicitly deferring a stronger `--cfg`-flag conversion; the SAME finding recurs in `numa-shim` and the deferral is load-bearing for that crate's own upcoming round.** — **CLOSED** (updated 2026-08-09, task #778/F5 — round-closing review of the numa-shim round). Filed 2026-08-09, task #776/F13, round-closing review of the aligned-vmem round. **RE-OPENED** 2026-08-14 (task #934/C-9) — see the `[A]` tier's item 42; the deadline this deferral was conditioned on has fired. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 4. **Two flaky coarse-wall-clock tests surfaced by `npm run check`'s `--all-features` step** — **RESOLVED** by R23-6 (task #375). One independent read-only review first corrected the originally-proposed fix (a `TEST_LOCK`-style mutex): a mutex only serializes test FUNCTIONS within ONE test binary/process, but the actual flakiness source is CPU contention from MULTIPLE test binaries (separate OS processes) running concurrently under `npm run check`'s `--all-features` step, plus the CI runner's own background load — a mutex inside one binary cannot serialize against a different process. That correction was confirmed independently before this task began and is reflected in the fix below (no `TEST_LOCK` was added to either file). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 5. **`dealloc_batch_small` doc comment claimed the LAST `TCACHE_CAP` freed blocks stay magazine-warm; the implementation keeps the FIRST.** — **RESOLVED** by R24-7 (task #385), a doc-only policy decision (no `src/` behavior change, no numbers measured). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 30. **`canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound assertion proved no double-release, not no leak.** — **RESOLVED** by R28-2 (task #431), a test-only strengthening (no `src/` behavior change). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 31. **CI clippy `--all-targets` red on all five rows — pre-existing example/test lint+compile errors** — **RESOLVED** by R33-1 (task #506, commit `e526517befbf5a0cd0ca1a7ee62f9d84ffe509ee`). Five distinct failures, all pre-existing on `main` (four inherited from Round-31 example files, one from Round-32 task #502). The brief enumerated only two and prescribed "one line of doc-indent + adding the missing `fn main`"; re-running ALL five ci.yml clippy rows (as the brief instructed) revealed three further latent failures masked by cargo's fail-fast target scheduling — all five were necessary for the DONE-WHEN criterion (all five clippy rows green): — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 32. **F10 shadow-head ordering gap — finding F-1** (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-1 [medium]) — **RESOLVED** by R34-6 (task #525). The F10 shadow-head fast path in `RemoteFreeRing::full_check` (`src/alloc_core/remote_free_ring.rs`) replaced every push's pre-F10 `head.load(Acquire)` with a `cached_head.load(Relaxed)` on the producer's own cache line. The module doc's value-domain proof (`cached_head <= head` always, so the fast path can only under-estimate occupancy) was correct, but the ordering role the removed load played was never addressed: under the abstract memory model, a producer P that takes only the fast path carries no happens-before chain to the consumer's `slot.store(EMPTY)`, so the consumer's clear and P's `slot.store(offset)` into a recycled slot are unordered. NOT a data race (both atomic on the same `AtomicU32`) — a potential lost-update/liveness defect, confirmed NOT realizable on any hardware Rust targets (x86-TSO, ARMv8, RISC-V RVWMO, POWER cumulativity). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 33. **F-5 release-surviving panic sites vs. "NEVER panics" doc claim** (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-5 [low]) — **RESOLVED** by R34-16 (task #535). The module doc in `src/global/sefer_alloc.rs` claimed "Every entry point here returns null on failure and NEVER panics," but five release-surviving (not `debug_assert!`) invariant checks are reachable from the `GlobalAlloc` impl under `production`: (1) `alloc_core/alloc_core.rs:2158` `assert!` in `realloc_inplace_fast_path_known_base`; (2) `alloc_core/alloc_core_large_cache.rs:147` `.expect` in `large_cache_slot_take` (base); (3) `:160` `.expect` (extension); (4) `:166` `unreachable!` (take, extension disabled); (5) `:321` `unreachable!` (set, extension disabled). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 34. **F-6 `HeapCore` by-value construction stack-pressure pin** (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-6 [low]) — **RESOLVED** by R34-18 (task #537). `HeapCore` is constructed BY VALUE on the frame that triggers a thread's FIRST allocation (`HeapRegistry::claim`'s `HeapCore::new(idx) → write(hc)` in both `claim` and `claim_with_config`, and the process-global fallback's `MaybeUninit<HeapCore>` path in `global/fallback.rs`). Rust does not guarantee return-value/move elision, so a debug build (or any backend that materialises the temporary) can place one ~7 KiB copy on a small-stack thread's first-allocation frame — a realistic stack-overflow risk for embedded-class 16–64 KiB stacks. The audit's ~7 KiB figure was INFERRED from in-tree `-Zprint-type-sizes` field-offset notes, never measured (`size_of::<HeapCore>()` existed nowhere in `src/` or `tests/`). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 15. **G2 — no loom model exercises the F10 fast path over a recycled slot** (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding G2 [medium]) — **RESOLVED** by R34-19 (task #538). The two existing shadow loom models in `tests/loom_remote_ring.rs` neither reached the F-1 interleaving: `RingModelShadow` (CAP=4) joined producers before draining (no wrap → no slot reuse); `RingModelShadow1` (CAP=1) forced the slow path exclusively. The one thing F10 actually changed — a producer proving room from the shadow alone and reserving a slot the consumer just cleared — was modelled by nothing. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 35. **F-2 provenance-asymmetry hypothesis — RESOLVED-NEGATIVE** (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-2 [low]; open item 15) — **RESOLVED** by R34-5 (task #524), following the item's own decision rule. The item's blocking question was: does the concurrent multi-producer SMALL-block `RemoteFreeRing` push/drain path (`Node::atomic_u32_at`, backing `head`/`tail`/`cached_head`/`slots`) flag under Stacked Borrows the way `Node::atomic_ptr_ref` was fixed for in task #142 — the one piece of evidence the repo's tooling could not supply until a concurrent small-ring miri test existed (audit G1). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 36. **H8 — `dbb4016`'s `fix(perf):` prefix considered for a reword to `feat(api):`, DECIDED against a rebase, prefix left as-is** (task #578, `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H8) — **RESOLVED, no code change.** Sol-F1's commit (`9296adb`, post-G1- rebase SHA `dbb4016`, "AllocCore::dbg_* inherent methods now genuinely require `internals`") used `fix(perf):`. The review flagged this as inapt for a pure visibility/cfg-gating change (no algorithm changed, only which callers can reach existing code) and pointed to the identical-class predecessor `27879af` (R34-3, gating the module PATHS behind `internals`), which used `feat(api):` — arguably the closer match, since CLAUDE.md's R30-12 taxonomy has no dedicated slot for "API-surface visibility change." - **Decision:** left `dbb4016` as-is — an accepted historical imprecision, not reworded. Two considered options were (a) a small rebase to reword just `dbb4016`, or (b) accept the existing prefix and use correct judgment for any NEW commits in the same class. (b) was chosen per the task's own explicit default guidance ("default to (b) unless a rebase is already happening anyway for some other reason in this batch") — no other rebase was in flight this round, and this is the exact non-retroactive posture CLAUDE.md's own R30-12 section already states for this rule ("no historical commit message is retagged or amended by this rule; it governs new commits going forward only" — the same posture the raw-log-truncation and immutable-source-identity rules elsewhere in CLAUDE.md also take). H2 (task #572), the directly-analogous follow-up commit extending this exact same gating work to 6 more files, independently used `fix(perf):` as well (`25d6ac4d23b4859b726724424e5912dc54fe0bf0`) and passed `verify-commit-prefixes.mjs` — establishing `fix(perf):` as the now-repeated, lint-accepted precedent for "narrow an existing diagnostic hook's reachability without changing its behavior," rather than treating `dbb4016` as an isolated one-off mistake to correct. A rebase deep enough to reword `dbb4016` would also need to touch every commit stacked on top of it since (including H2, H3, H4, H5, H7 above) — disproportionate risk for a P4 wording nit, per the same cost/benefit reasoning G1's rebase (task #555) already weighed once this session for a higher-severity (P2) case. - **Files changed:** none (this index entry only) — a documented decision, not a rebase or a reword. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 37. **Flaky test — `repeated_same_segment_frees_are_observed_as_tier1_hits`** (`tests/segment_table_contains_base_tier1_counters.rs`) — **RESOLVED** by wave 3's own `npm run check --all-features` gate run (2026-08-05, same session as H1-H8, tasks #571-578). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 38. **Flaky test — `ac1_trim_empties_pool_and_evicts_large_cache`** (`tests/r31_10_trim_current_thread_api.rs`) — **RESOLVED** by wave 4's own post-landing `npm run check --all-features` gate run (2026-08-05, same session as I1-I10, tasks #579-588; found in a background rerun launched after `782b92e` landed, task #589). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 39. **Flaky test — `oom_injection_flag_is_clean_after_test`** (`tests/regression_free_path_chunk_oom_graceful.rs`) — **RESOLVED** by the first full remote CI run over the pushed backlog (2026-08-05, CI run `31045983765` on landing SHA `42d4206`, task #621, found during the map-verification pass of this session's release-readiness work). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 40. **CI-coverage gap — `cargo test -p racy-ptr-cell` ran in ZERO CI configurations** (`.github/workflows/ci.yml`'s `test-workspace` job) — **FLAGGED AND RESOLVED IN THE SAME ROUND** (2026-08-09, task #774 filing this entry per this file's own "file in the same commit that flags it" rule; found by the racy-ptr-cell round-closing review, `docs/reviews/2026-08-09-racy-ptr-cell-round-closing-review.md` §F1; closed by task #773 immediately prior in the same round, commit `a5e8e42`). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 41. **SyncRegion one-shot convenience methods missing reentrancy cross-references** (`crates/sefer-region/src/sync_region.rs`, methods `clear` and `get_cloned`) — **RESOLVED** by the release-prep review's finding F5 closure (2026-08-09). The round-2 closing review (`docs/reviews/2026-08-08-sefer-region-round2-closing-review.md`, finding F) flagged that of the seven one-shot convenience methods (`insert`, `remove`, `contains`, `len`, `is_empty`, `clear`, `get_cloned`), only `remove` explicitly cross-references the type-level `## Reentrancy` section. This is a documentation gap: `clear` runs every `T::Drop` under the write lock, and `get_cloned` runs `T::clone` under the read lock — the two methods that actually execute user code under the lock — yet neither points to the deadlock hazard section. — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 44. **`aligned-vmem`'s hand-written `mmap` FFI declaration has an ABI shape mismatch risk on 32-bit Unix targets — the `offset` parameter was hardcoded as `i64`, assuming a 64-bit POSIX `off_t`, which is not guaranteed on 32-bit Unix platforms (e.g. glibc i686 and traditional 32-bit ARM default to a 32-bit off_t without `_FILE_OFFSET_BITS=64`).** — **CLOSED** (task #914, correcting H2C1 docs half of `docs/reviews/2026-08-13-aligned-vmem-round10-closing-review.md`). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 45. **`aligned-vmem` Linux HugeTLB path leaks entire pinned huge-page mapping when system's default huge-page size is not 2 MiB.** — **CLOSED** (task #909, finding H1 of `docs/reviews/2026-08-13-aligned-vmem-independent-review.md`). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 56. **[T, LOW] `scripts/vmem-doc-drift-guard.mjs` false-positives on `from_raw_parts`'s "insufficient whenever the reservation was over-reserved" sentence** — **CLOSED** (2026-08-16, personal follow-up during round-3 close-out, per this item's own "Next trigger"). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 42a. **`aligned-vmem`'s `mock` Cargo-feature-unification hazard (item 42's aligned-vmem half)** — **CLOSED** (2026-08-16, task #962, per the maintainer decision recorded in this session: "делаем 2" — convert, do not just document the risk). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 57. **`scripts/bench-table.mjs` has been unable to build `benches/global_alloc.rs` since task #583, ~11 days before discovery.** — **CLOSED** (2026-08-16, found and fixed by a user report of `npm run bench:table` failing). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 49. **`aligned-vmem` has ten FFI call sites relying on the edition-2021 implicit `unsafe fn` body instead of an explicit `unsafe {}` block with its own `// SAFETY:` comment — none unsound today, but edition 2024 makes `unsafe_op_in_unsafe_fn` a hard error at all ten.** — **CLOSED** (task #997, P3-8 pass 2 of the 0.2.0 pre-release audit/closing-review campaign). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 52. **[T, INFO] `decommit_lazy` leaves free BSD reclaim on the table** — **CLOSED** (filed 2026-08-14, task #934/C-9, from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` finding V-4). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 53. **[T, INFO] `Reservation::from_raw_parts` hard-codes `granted_huge: false`, creating a fail-open hazard when callers follow documented decommit advice** — **CLOSED** (filed 2026-08-14, task #934/C-9, sub-observation about item 48 from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 64. **Follow-up from commit 66b8508 (task #1030): `npm run check` lacks a `cargo test -p aligned-vmem` row with DEFAULT features, though ci.yml has a separate job that tests workspace members with default features.** — **CLOSED** (filed 2026-08-16, R7-8 finding class third occurrence; this is the same gap task #1024 closed with commit 66b8508, resurfaced as items 64/65 in task #1034). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 65. **CI-coverage gap: `aligned-vmem-gates` job added three steps (cargo doc with RUSTDOCFLAGS="-D warnings", cargo publish --dry-run, cargo semver-checks check-release) that are NOT covered by `npm run check`.** — **CLOSED** (filed 2026-08-16, task #1039 coverage gap; same class as task #1024's `aligned-vmem package gates` gap). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 71. **`scripts/vmem-doc-drift-guard.mjs` went false-green when commit `a4b8e50` (task #1055) split `crates/aligned-vmem/src/lib.rs` into modules-per-file — its scan list was frozen at the pre-split three files (`src/lib.rs`, `Cargo.toml`, `README.md`), so every rustdoc that moved into `src/api/*.rs`, `src/os/*.rs`, `src/reservation*.rs`, `bench_internals/*.rs` left the guard's jurisdiction, and a live violation of its own rule shipped under the green (task #1069's `reserve_aligned_huge.rs` pool-cost note: an unqualified "deliberately not trimmed away / mapping kept whole" sentence).** — **CLOSED** (2026-08-18, task #1078; third guard of the campaign to lose contact with its subject, after task #1071's cargo cache replay and task #1073's foreign-worktree test binary). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).

- 75. **`scripts/verify-vmem-page-constant-call-sites.mjs`'s tree scan was host-dependent: a readdir walk with a hand-maintained SKIP_DIRS that consulted no `.gitignore`, so gitignored scratch copies (on the reporting host: `tmp/asm_check/main.rs`, `tmp/heap_core_size_probe.rs`, `tmp/sefer_backup.rs` — a 1058-line stale copy of a source file) were scanned alongside the real sources, making both the verdict (a stale copy of an old `alloc_core.rs` could flip the guard RED with long-fixed call sites, or dilute it GREEN) and the summary's "scanned N file(s)" count host-dependent and not reproducible from a clean clone.** — **CLOSED** (2026-08-18, task #1088, finding L7; fixed in the same task that filed it). — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" (R34-24 split, task #1109, 2026-08-18).
