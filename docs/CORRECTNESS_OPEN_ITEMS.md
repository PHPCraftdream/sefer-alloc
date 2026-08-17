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
   - **Status:** RESOLVED — no code/process change needed beyond this clarifying note, so a future round does not re-investigate the apparent conflict from scratch. Round 8's two review docs (`docs/reviews/2026-08-13-aligned-vmem-round8-review.md`, `...-round8-closing-review.md`) are committed in the same commit as this note, per this campaign's own established convention.
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

   - **Status:** RESOLVED (release decision applied) — MIPS targets now fail compilation with a clear diagnostic (`compile_error!`) that explains the constant mismatch and points to this index entry for the decision record. The crate already uses the same pattern for unsupported Unix targets (task #918/finding H2C7); this extends it to architecture-specific broken targets. Adding MIPS support requires adding a `#[cfg(any(target_arch = "mips", target_arch = "mips64"))]` arm with the correct MIPS-specific constant values, gated on that architecture only.
   - **Decision rationale:** Fail-fast at compile time is preferable to publishing a buildable target that fails silently at runtime on every reservation call. The `EBADF` failure is documented but opaque to downstream users; a compile-time error with a diagnostic pointing at the specific problem (wrong constants) makes it explicit what must change to add support.
   - **Evidence:** the `compile_error!` gated on `any(target_arch = "mips", target_arch = "mips64")` in `crates/aligned-vmem/src/lib.rs`; the **MIPS** bullet in `crates/aligned-vmem/README.md`'s "Reasoned-from-spec targets" section (now marked "not supported (compile_error!)"); `docs/reviews/2026-08-16-aligned-vmem-independent-prerelease-audit-r4.md` finding R4-1 (evidence root)

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
   `MEM_DECOMMIT` (`crates/aligned-vmem/src/lib.rs`'s `cfg(windows)`
   `decommit_pages_impl`) genuinely UNMAPS the payload pages, unlike Linux
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
      `try_reserve_aligned_exact` (`crates/aligned-vmem/src/lib.rs`) used to skip
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
      see the fix's own comment in `lib.rs` — so removing it is free).
    - **Evidence:** `crates/aligned-vmem/src/lib.rs`'s `_SC_PAGESIZE` constant
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

46. **[T, filed 2026-08-11, task #821] `crates/sefer-region` depends on `captrack`
    v0.1.1 (exact-pinned) as a dev-dependency for a single ignored probe test.**
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
    - **Prior knowledge (repo-wide, pre-dating this "discovery" by multiple rounds):** the exact same hazard — Darwin `MADV_DONTNEED` being advisory/lazy with no zero-fill guarantee — was already documented in at least four places before this item was filed: `.github/workflows/ci.yml` (the `test-macos` job's own comment, a few lines above the step that went red, currently around line 810: "MADV_DONTNEED on Darwin is advisory/lazy (no zero-fill guarantee)"); `src/alloc_core/alloc_core_small_pool.rs` (a production code comment, currently around lines 1002-1021, stating the same fact as the load-bearing risk area for the `virgin-zero-skip` feature); and two `virgin-zero-skip` design docs, `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` (around lines 115-116 and 358) and `docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` (around line 32), whose entire safety argument is built on this fact. The honest story is: the repo knew this when `aligned-vmem` was extracted from `src/alloc_core/os.rs`, the extraction lost that knowledge, and CI finally made the gap fail loudly rather than "discovering" something new.
    - **R9_5 mis-citation, also corrected here:** `docs/perf/R9_5_VIRGIN_ZERO_SKIP_DESIGN.md:115-116` cites "`crates/aligned-vmem/src/lib.rs` §decommit note" as its source for this fact. `git log --oneline -S "advisory" -- crates/aligned-vmem/src/lib.rs` shows that note was created BY commit `9c777bc` (dated 2026-08-13) — i.e. R9_5's citation was unverifiable/forward-referencing a note that did not exist when R9_5 was written (2026-07-20). It is now accurate as of `9c777bc`, purely by coincidence of the fix landing there. See the one-line notes added to both design docs (R9_5 near lines 115-116/358, R11_8 near line 32) in this same task.
    - **Status:** OPEN — mitigated across more surface than the original `9c777bc` fix covered. As of round 6 (tasks #880-886): the test is scoped to not assert the false guarantee on the Darwin family (macOS/iOS/tvOS/watchOS); `decommit()`'s rustdoc, `recommit()`'s rustdoc, `decommit_lazy()`'s rustdoc, the crate-root module doc, `recommit_pages_impl`'s code comment, AND `README.md`'s new "Platform caveats" section all carry a consistent Darwin-scoped caveat (task #880/S1, task #881/S5, task #885/S7); the empirical root-cause oracle (task #882/S2) has now run on real macOS CI and settled the H1-vs-H2 question (see Root cause below, updated round 7 / task #888); `decommit_lazy_roundtrip`'s own vacuousness (S4's second limb) is recorded below, not yet fixed. The underlying functional gap itself is NOT fixed. `decommit()`'s core purpose — "return page-granular physical backing to the OS" — is silently unmet on the Darwin family for ordinary (non-huge) reservations: RSS does not decrease, and re-access after `recommit` returns stale data instead of a fresh zero page.
    - **Current-number-or-verdict:** confirmed via real CI (`test macos (production)` job, run `31676133649`, landing SHA `e60e46a`) — deterministic, not flaky (byte value matches exactly what was written before decommit). Linux (`aligned-vmem package gates`, `test workspace members`) and Windows (`test windows (production)`) both passed the same assertion in the same run, confirming the guarantee genuinely holds on those two platforms and the gap is Darwin-specific.
    - **Root cause:** `recommit_pages_impl`'s Unix implementation (`crates/aligned-vmem/src/lib.rs`, `#[cfg(all(unix, not(miri)))]`) is an unconditional no-op for ALL Unix platforms, justified by a comment claiming "re-access after MADV_DONTNEED is implicit — fresh zeroed pages on demand" — true on Linux, false on the Darwin family. `decommit`'s own eager path calls `madvise(MADV_DONTNEED)` uniformly across all Unix too. **This explanation was ASSERTED, not ESTABLISHED, when first written (task #882):** it was inferred from a single failing byte, which was equally consistent with a different hypothesis — the `madvise(2)` syscall itself FAILING on that CI runner for an unrelated reason (H2), since `libc_madvise` discards `madvise`'s return value by design (task #719) and nothing in the crate could previously distinguish "syscall succeeded but Darwin's semantics didn't reclaim the pages" (H1) from "syscall itself failed" (H2). Task #882 added an empirical oracle: under the `bench-internals` feature, `libc_madvise` now also records attempt/success counts into two new `#[doc(hidden)]` statics, `UNIX_MADVISE_ATTEMPTS`/`UNIX_MADVISE_SUCCESSES` (accessors `aligned_vmem::unix_madvise_attempts()`/`unix_madvise_successes()`, reset via the existing `reset_bench_internals_counters()`), and a new macOS-gated test, `macos_decommit_madvise_syscall_actually_succeeds` (`crates/aligned-vmem/tests/smoke.rs`), that asserts the `madvise` syscall itself returns success (`0`) for BOTH the eager (`decommit`, `MADV_DONTNEED`) and lazy (`decommit_lazy`, `MADV_FREE_REUSABLE`) call sites. **Updated round 7 (task #888, finding T1):** commit `1dbd6b4` was pushed and CI run `31692217669` (job `94421845398`, `test macos (production)`, image `macos-26-arm64`) ran green — `macos_decommit_madvise_syscall_actually_succeeds` passed, with `unix_madvise_attempts() == 2 && unix_madvise_successes() == 2`, ruling out H2 (the syscall itself did not fail). **This does NOT by itself confirm H1** — the H1 argument has two halves observed in TWO DIFFERENT CI runs: the stale-byte evidence (`decommit_recommit_roundtrip`'s pre-scoping failure) comes from run `31676133649`/commit `e60e46a`, before that assertion was scoped off Darwin, while the madvise-success evidence above comes from run `31692217669`/commit `1dbd6b4`; no single run has observed both the stale byte AND the successful `madvise` syscall in the same process. **Correct wording: H2 is ruled out by run `31692217669`; combined with run `31676133649`'s stale byte, H1 (advisory-only semantics) is the only remaining explanation** — NOT "H1 confirmed by CI".
    - **Darwin lazy-path alternative fix (round-6 review S9, spec-read, not verified on hardware, not a recommendation to implement without further review):** three connected observations. (1) On Darwin, `decommit_lazy` issues `MADV_FREE_REUSABLE` but nothing in this crate ever issues the paired `MADV_FREE_REUSE` before re-touching pages (`recommit_pages_impl`'s Unix implementation is an unconditional `Ok(())`, confirmed by reading it) — Apple documents these as a required pair, so this is a physical-footprint-accounting-drift concern (not a memory-safety issue), distinct from this item's own eager-`decommit` finding. (2) `decommit_lazy`'s own rustdoc describes the general "lazy is cheaper, reclaimed only under pressure" ordering (Linux `MADV_FREE` semantics), but on macOS/iOS specifically that ordering is INVERTED on the RSS axis: `MADV_FREE_REUSABLE` drops footprint immediately there, while eager `decommit`'s `MADV_DONTNEED` drops nothing at all — the opposite of the general case (on tvOS/watchOS, `decommit_lazy` falls back to the same `MADV_DONTNEED` as the eager path, so there both are equally no-ops). (3) Because of (2), a cheaper but PARTIAL alternative to the `MAP_FIXED` re-map idea below exists and is worth recording: route macOS/iOS's eager `decommit` to `MADV_FREE_REUSABLE` and issue `MADV_FREE_REUSE` from `recommit` — this would close the "return physical backing to the OS" half of `decommit`'s promise on macOS/iOS but NOT the "reads as zero" half (since `MADV_FREE_REUSABLE` preserves contents if the pages are re-touched before reclaim). **tvOS/watchOS coverage (round 7, task #895, TC3 — synchronized with `decommit_lazy`'s rustdoc and `madv_free_advice`'s doc comment, which this bullet must keep agreeing with if either changes):** this crate's cfg currently only names macOS/iOS for `MADV_FREE_REUSABLE`, so as written this alternative would not extend to tvOS/watchOS — but `MADV_FREE_REUSABLE`'s value comes from XNU, the kernel all four Darwin targets share, so it MAY work identically there too; this is REASONED-FROM-SPEC, not verified on tvOS/watchOS hardware or a tvOS/watchOS build target (neither is available to this crate's CI), not an established "no `MADV_FREE_REUSABLE` there" fact. Only re-mapping is confirmed to close both halves on all four targets; the lazy-path alternative's tvOS/watchOS coverage is an open question, not a settled no.
    - **Next trigger:** a future round should implement a real Darwin fix — the standard technique is re-`mmap`(`MAP_FIXED | MAP_ANONYMOUS`) over the decommitted range instead of (or in addition to) `madvise`, which forces the kernel to actually replace the mapping with fresh zero pages; needs its own safety analysis (interaction with concurrent access to the same reservation, `is_huge()` state, the existing `huge_pages` feature's `MAP_HUGETLB` path) and its own review round rather than a rushed fix under a CI-green-checking task. Until then, `decommit()`'s Darwin-family behavior should be treated as "hint only, no RSS/zero-fill guarantee" — the same posture already documented for the huge-page case.
    - **S4 remainder (round-6 closing review SC1, partially fixed):** the round-6 review's S4 finding had two limbs — "macOS lost its only decommit effect-oracle" (closed by task #882's new counters/test above) and "`decommit_lazy_roundtrip` (`crates/aligned-vmem/tests/smoke.rs`) is vacuous on EVERY platform, not just macOS" (still not fixed — that test only checks a post-recommit write/read round-trips, never whether `madvise` had any effect; its rustdoc previously claimed otherwise, corrected in the closing pass). The new oracle's counters are `unix`-wide, not macOS-specific (`libc_madvise` is `#[cfg(all(unix, not(miri)))]`), so the same assertion style would close the Linux half too — but no CI row currently runs `bench-internals` against the real (non-mock) Unix backend on Linux (the Linux rows in `ci.yml` are default-features, `--all-features` which turns `mock` on, or `fault-injection lazy-commit` without `bench-internals`). Closing this fully needs either a new Linux CI row or accepting the gap stays macOS-only for now.
    - **Evidence:** CI run `31676133649` (`gh run view 31676133649 --json jobs`), job `test macos (production)`, step "Run cargo test -p aligned-vmem --features ... --no-fail-fast", failure at `crates/aligned-vmem/tests/smoke.rs:174` (pre-fix line number) — `assertion left == right failed: recommitted page must be zeroed / left: 119 / right: 0`; fixed in commit (this task's own commit, landing after `e60e46a`). Discovery-framing and mis-citation correction: `docs/reviews/2026-08-13-aligned-vmem-round6-review.md` finding S3 (task #883). Round-6 closing review: `docs/reviews/2026-08-13-aligned-vmem-round6-closing-review.md`, findings SC1-SC10.

49. **CLOSED** by task #997 (P3-8 pass 2). See 'Recently resolved' below for the full closure narrative.

50. **`aligned-vmem` — `page_size()`'s sanity-guard rejection branch has never executed anywhere.** (Filed round 8, task #903, finding U11 of `docs/reviews/2026-08-13-aligned-vmem-round8-review.md`. The U10 half — Windows `bench-internals` reserve-path counters — was closed per task #917: see "Recently resolved" §50-U10 below.)
    - **Status:** OPEN — record-only; no code fix in scope for either half (see each half's own note on why a cheap fix is not appropriate here).
    - **Current-number-or-verdict, U11 half (`page_size()` guard):** the guard at `crates/aligned-vmem/src/lib.rs:390-406` and `query_os_page_size()`'s three `#[cfg]` arms (named by symbol rather than line range, task #908/V2C3, after the `:409-445` range this bullet previously cited was truncated by an unrelated comment edit one commit later — task #907's `+2` insertion above it) are structurally untestable — the queried value comes from a private `fn` with no injection seam, and the result caches in a process-wide atomic (`PAGE_SIZE_CACHE`, `:168`) on first call, so even a hypothetical test could not re-run the guard within one process. The guard's REJECTION branch (queried `< PAGE`, or non-power-of-two) has never executed in any test or CI run in this repository. Item 43's card cites this guard as the reason a wrong `_SC_PAGESIZE` constant "silently returns garbage" rather than crashing. **Corrected round-8 closing review (task #904, UC3):** this bullet originally said round 8's U1 finding "makes the guard's untested acceptance-side load-bearing" in the present tense — stale on arrival, since task #897 (merge `491afe9`, this same round) already removed that dependency by making `try_reserve_aligned_exact`'s alignment check unconditional (it no longer consults `page_size()` at all). The guard's acceptance side is now cosmetic, not load-bearing.
    - **Why not fixed now:** the cheap remedies each have a real cost: a `#[cfg(feature = "bench-internals")] fn dbg_set_page_size_for_test` would be a safe `pub fn` mutating a value the alignment fast path trusts, precisely the shape CLAUDE.md's benchmark-hook rule warns against; splitting the guard into a pure `fn sanitize_page_size(queried: usize) -> usize` and testing that in isolation is the clean version and costs one small refactor, deliberately left for a future round rather than folded into this hygiene bundle.
    - **Next trigger:** round 8's U1 fix already landed (`491afe9`, this same round), so this guard's untested acceptance side is purely cosmetic now; the remaining trigger is the `sanitize_page_size` extraction described above, which would make the REJECTION branch testable without a benchmark-hook-shaped seam.
    - **Evidence:** `docs/reviews/2026-08-13-aligned-vmem-round8-review.md` finding U11 (filed INFO, not code defect).

51. **`aligned-vmem` review campaign's standard local verification matrix never compiles this crate's `#[cfg(unix)]` code on the Windows host every round has run on.** (Filed round 8, task #904, finding UC5 of `docs/reviews/2026-08-13-aligned-vmem-round8-closing-review.md`.) `try_reserve_aligned_exact` (`crates/aligned-vmem/src/lib.rs`, `#[cfg(all(unix, not(miri)))]`) — the function round 8's U1 finding fixed — and `decommit_contract_violation_never_reaches_madvise` (`crates/aligned-vmem/tests/smoke.rs`, `#[cfg(all(unix, ...))]`) — the test round 8's U7/UC4 work touched — were BOTH `#[cfg]`'d out entirely on the campaign's `x86_64-pc-windows-msvc` host, on every command in round 8's stated verification matrix (2 `cargo test` rows, 3 clippy rows, `cargo fmt`, the drift guard). "42 passing, 47 passing, three clippy rows clean" was true and said nothing about either. Round 8's closing review closed the gap for that round only by running `cargo check`/`cargo clippy --target x86_64-unknown-linux-gnu --all-targets` against the merged tree (both clean) — a command not previously part of any round's stated matrix.
    - **Status:** OPEN — the one-off check was run and passed for round 8's diff (task #904); the matrix itself has not been permanently amended anywhere durable other than this item.
    - **Current-number-or-verdict:** confirmed clean for round 8's diff: `cargo check -p aligned-vmem --target x86_64-unknown-linux-gnu --features "lazy-commit huge-pages fault-injection bench-internals" --all-targets` and the equivalent `cargo clippy ... -- -D warnings` both pass on this host (`x86_64-unknown-linux-gnu` is already installed via rustup — no new toolchain component needed). Cost measured: seconds once the target's std is cached, a few minutes cold.
    - **Next trigger:** any future round of this campaign that touches `crates/aligned-vmem/src/lib.rs` or `crates/aligned-vmem/tests/*.rs` should add this command to its own verification pass (in addition to, not instead of, the existing default-target matrix) before merging, not just before pushing — the campaign's existing "confirm CI green" step already catches it post-push, but that is one round later than the campaign's own zero-trust-review convention intends. `--all-targets` is load-bearing: without it, `tests/` is not built and Unix-only tests are not checked.
    - **Evidence:** `docs/reviews/2026-08-13-aligned-vmem-round8-closing-review.md` finding UC5.

52. **`decommit_lazy` leaves free BSD reclaim on the table** (Filed 2026-08-14, task #934/C-9, from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` finding V-4.) **CLOSED** — see "Recently resolved" below for the full closure narrative.

53. **`Reservation::from_raw_parts` hard-codes `granted_huge: false`, creating a fail-open hazard when callers follow documented decommit advice** (Filed 2026-08-14, task #934/C-9, sub-observation about item 48 from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`.) **CLOSED** — see "Recently resolved" below for the full closure narrative.

54. **[T, INFO] Tautological tests and small untested corners** (Filed 2026-08-14, task #934/C-9, combining `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` findings V-29 and V-31.) Low-priority housekeeping items, not correctness gaps.

    - **Status:** OPEN — tracked for completeness as hygiene, not blocking.
    - **Current-number-or-verdict:**
      - **V-29:** `crates/aligned-vmem/tests/min_page.rs:8-10` (`min_page_equals_page`) asserts `MIN_PAGE == PAGE` where `pub const MIN_PAGE: usize = PAGE;` (`crates/aligned-vmem/src/lib.rs:160`) — the compiler guarantees this equality, so the test cannot fail. Its sibling `min_page_is_4kib` is fine.
      - **V-31:** Several small untested corners listed for completeness, none blocking: `release`'s early return for null pointers (`crates/aligned-vmem/src/lib.rs:1075-1078`); `ReservationParts`'s derived `PartialEq`/`Eq`; the deprecated `Reservation::is_empty` method; `leak_zeroed_pages` with an exact-multiple size (only `3 * PAGE + 7` is tested, at `crates/aligned-vmem/tests/smoke.rs:739-740`); the `try_reserve_aligned` `size + align` overflow case (addressed elsewhere as V-11).
    - **Evidence:** `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` findings V-29 and V-31; `crates/aligned-vmem/tests/min_page.rs:8-10`; `crates/aligned-vmem/src/lib.rs:160` (`MIN_PAGE` definition); `crates/aligned-vmem/src/lib.rs:1075-1078` (`release`'s null early return); `crates/aligned-vmem/tests/smoke.rs:739-740`.

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
    - **Next trigger:** when a 32-bit Linux runtime test runner becomes available in CI (e.g., via GitHub Actions `i686-unknown-linux-gnu` self-hosted runner or QEMU-based emulation), add a `cargo test --target i686-unknown-linux-gnu` step to the `aligned-vmem-gates` job. Until then, the compile-only check is the best available coverage.
    - **Evidence:** the `cargo check --target i686-unknown-linux-{gnu,musl} --all-targets` steps in `.github/workflows/ci.yml`'s `aligned-vmem-gates` job (compile-only); `crates/aligned-vmem/src/lib.rs` `try_reserve_aligned_exact` function (the 32-bit-gated exact-size reservation path); item 44 (the `OffT` fix that this compile-only check guards against regressions).

59. **[T] CI-coverage gap: Linux MAP_HUGETLB and Windows MEM_LARGE_PAGES success paths depend on configured hugetlb pool or SeLockMemoryPrivilege, neither present in standard CI.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.)

    The Linux `MAP_HUGETLB` success branch (`libc_mmap` with `MAP_HUGETLB | MAP_HUGE_2MB`) only succeeds when the system has a configured hugetlb pool with 2 MiB huge pages available. Standard GitHub Actions `ubuntu-latest` runners do NOT pre-configure any hugetlb pool (`/proc/sys/vm/nr_hugepages` defaults to 0 on these runners), so all huge-page reservation attempts fall back to ordinary 4 KiB pages in CI. Similarly, the Windows `MEM_LARGE_PAGES` success branch requires `SeLockMemoryPrivilege` and appropriate large-page configuration, which standard `windows-latest` runners do not provide. The happy paths for both platforms are therefore never exercised in CI, only the fallback paths.

    - **Status:** OPEN — not urgent, because the fallback path is the same code exercised on non-huge-page systems, and the `is_huge()` predicate correctly reports the failure (test arm verifies `granted_huge == false`). However, the actual huge-page success path is unverified in CI.
    - **Next trigger:** when a CI runner with configured hugetlb pool (Linux) or SeLockMemoryPrivilege (Windows) becomes available, add a dedicated `test-huge-pages` job that runs `cargo test --features huge-pages` with appropriate hugetlb pool setup (e.g., `sudo sysctl vm.nr_hugepages=128` on Linux). Until then, the success paths remain unverified.
    - **Evidence:** `crates/aligned-vmem/src/lib.rs` `libc_mmap` function (Linux `MAP_HUGETLB | MAP_HUGE_2MB` handling); `crates/aligned-vmem/src/lib.rs` `winapi_virtual_reserve` function (Windows `MEM_LARGE_PAGES` handling); `crates/aligned-vmem/tests/huge_pages.rs` (the test that would exercise the success path if hugetlb were available); `/proc/sys/vm/nr_hugepages` on standard runners (observed to be 0).

60. **[T] CI-coverage gap: BSD, Android, tvOS and watchOS branches are reasoned-from-spec, not empirically executed on real hardware.** (Filed 2026-08-16, TaskList #1023, from aligned-vmem prerelease-audit-r4 "Coverage gaps" section.)

    The BSD platforms (FreeBSD, NetBSD, OpenBSD, DragonFly) are only compile-verified; no BSD CI runner exists (`.github/workflows/ci.yml` has no BSD job). The `decommit_lazy` and `_SC_PAGESIZE` handling for these platforms are based on reading headers and POSIX spec, not empirical runs. Similarly, Android, tvOS and watchOS platforms are not represented in CI; only macOS covers part of the Darwin family (iOS, tvOS, watchOS are missing). This is an honest limit of the current CI infrastructure, not a discovered bug.

    - **Status:** OPEN — not urgent, because the unconditional alignment check in `unix_reserve` prevents violation of reserve alignment even if `_SC_PAGESIZE` values were wrong, and decommit correctness is verified on Linux/macOS which are the primary deployment targets. However, the BSD/Darwin/Android branches remain empirically unverified.
    - **Next trigger:** when a FreeBSD/NetBSD/OpenBSD/DragonFly CI runner becomes available (self-hosted or via a service like Cirrus CI), add a `test-bsd` job. Similarly, if iOS/tvOS/watchOS or Android CI runners become available, add corresponding jobs. Until then, these platforms remain spec-verified only.
    - **Evidence:** item 43 (BSD `_SC_PAGESIZE` values, partially open); item 48 (Darwin `MADV_DONTNEED` behavior, partially open); `.github/workflows/ci.yml` (no BSD/iOS/tvOS/watchOS/Android jobs); `crates/aligned-vmem/src/lib.rs` platform-specific constants (FreeBSD/NetBSD/OpenBSD/DragonFly `_SC_PAGESIZE` values, Android-specific handling, Darwin-family `decommit_lazy` behavior).

61. **CLOSED** by task #1057 (same fix as item 41 — the new `aligned-vmem-miri` CI job runs the interpreter, closing this runtime-semantics phrasing of the same gap). See "Recently resolved" below for the full closure narrative.

63. **Flaky test — `shadow_path_activation_oracle_fast_and_slow_both_reachable` scheduler-sensitive percentage thresholds.** See "Recently resolved" §3 for full resolution.

64. **Follow-up from commit 66b8508 (task #1030): `npm run check` lacks a `cargo test -p aligned-vmem` row with DEFAULT features, though ci.yml has a separate job that tests workspace members with default features.** — **CLOSED**, see "Recently resolved" §#64 below.

65. **CI-coverage gap: `aligned-vmem-gates` job added three steps (cargo doc with RUSTDOCFLAGS="-D warnings", cargo publish --dry-run, cargo semver-checks check-release) that are NOT covered by `npm run check`.** — **CLOSED**, see "Recently resolved" §#65 below.

66. **`Reservation` carried no committed-length state, so a lazy handle's committed prefix was a DOCUMENTED contract rather than a CHECKABLE one (R6-1 variant 3 / R7-2).** — **CLOSED** by the new `LazyReservation` type (task #1051), see "Recently resolved" §#66 below — including why all five options this card previously listed were set aside for a sixth.

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
   Full history: this entry.

68. **CLOSED** by task #1052 (option (c): asymmetry accepted as-is, no rename).
   See "Recently resolved" below for the full closure narrative.

69. **CLOSED** by task #1063 (added the missing `serial_guard()` call to `windows_virtualfree_release_failures_accessor_exists`). See "Recently resolved" below for the full closure narrative.

---

## Recently resolved (closure trail — do not re-list as open)

41+61. **`aligned-vmem` had NO `cargo miri test -p aligned-vmem` step anywhere in CI** (item 41: the missing step; item 61: the same gap phrased as runtime-semantics concern — one fix, closed together) — **CLOSED** by task #1057.

   - **What was added:** a new dedicated job `aligned-vmem-miri` in `.github/workflows/ci.yml` (placed directly after `aligned-vmem-gates`), modeled on `numa-shim-macos-miri`'s per-PR small-crate pattern with three deliberate differences: `ubuntu-latest` runner (the miri fallback backend `src/os/miri.rs` is platform-independent std::alloc — no macOS-specific cfg-collision reason to pay for a macOS runner); `MIRIFLAGS: -Zmiri-ignore-leaks` (routes around the ONE intentional leak, `tests/smoke.rs`'s `leak_zeroed_pages_is_zeroed_and_static`, where the function under test `core::mem::forget`-leaks a process-lifetime sidecar by design — NOT `-Zmiri-disable-isolation`, which solves a different problem); per-PR cadence, not weekly (suite is small; ~45 s default / ~2 min all-features locally). Steps: `cargo miri test -p aligned-vmem` (default) + `cargo miri test -p aligned-vmem --all-features` — no narrower single-feature runs, since every feature's cfg-gated code is compiled and exercised under `--all-features`.
   - **What local verification found BEFORE committing (the important part):** the known leak was the ONLY blocker item 41's history recorded, but running the suite under miri on the committer's host (and, as the CI predictor, under `--target x86_64-unknown-linux-gnu`) found NINE more failing tests — all one mechanism: they assert real-backend observables (nonzero over-reserve head offset; bench-internals counter deltas) that `src/os/miri.rs`'s minimal std::alloc backend structurally cannot produce (it returns `(base, base, size)` — never over-reserves — and never increments any counter; the counter increment sites live only in the real unix/windows backends, whose storage re-exports in `src/bench_internals/mod.rs` are already `not(miri)`-gated). Five fail on linux/miri (smoke.rs `decommit_recommit_roundtrip_on_over_reserved_span`; bench_internals_counters.rs `bench_internals_counters_existence_and_reset`; huge_pages.rs `reserve_aligned_huge_exact_size_for_2mib_align` + both `reserve_aligned_huge_rejects_non_huge_page_aligned_{size,align}` — the hugetlb-alignment guards live in the real unix backend only); six on windows/miri (same smoke test; same counters test; huge_pages.rs both `..._still_two_call_path` tests; lazy_commit.rs `windows_lazy_reserve_saves_commit_charge` and `safe_decommit_over_never_committed_tail_succeeds`' counter block). Zero UB reports in any run. All nine post-date the crate's last miri-green run (2026-08-09, task #776) and had never been executed under miri — exactly the coverage gap this item existed to surface. With owner authorization, each gained the repo's established `not(miri)` exclusion (mirroring in-file precedent like smoke.rs's Darwin-family gates), so NO real-OS coverage was lost — Windows/Linux/macOS runs still execute every one of them.
   - **Verified after the exclusions, before committing:** all four local miri runs green — `MIRIFLAGS=-Zmiri-ignore-leaks cargo +nightly miri test -p aligned-vmem` (default and `--all-features`, on windows-msvc) plus the same two under `--target x86_64-unknown-linux-gnu` (what the ubuntu runner will execute). The intentional-leak test passes under the flag; no test was skipped or weakened beyond the miri-only cfg exclusions.
   - **Consequence:** items 41 and 61 both close; the `aligned-vmem-gates` compile-only `--cfg miri` check stays (it catches cfg/type drift cheaply); future miri-blocking test regressions in this crate now go red per-PR instead of unnoticed.

69. **Flaky test: `safe_decommit_over_never_committed_tail_succeeds` intermittently read `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS` as 0 instead of 1 under full-suite parallel load** — **CLOSED** by task #1063.

   - **Discovery:** task #1056 (2026-08-17), during zero-trust verification of an unrelated doc-only fix (item 49 formatting) — the first `npm run check` run FAILED on this test with zero other changes in flight besides a markdown edit; the test binary was not even rebuilt, ruling out the edit as a cause.
   - **Root cause:** every counter-touching test in `crates/aligned-vmem/tests/lazy_commit.rs` wraps its body in `let _guard = serial_guard();` EXCEPT `windows_virtualfree_release_failures_accessor_exists`, which called `aligned_vmem::reset_bench_internals_counters()` unguarded. Under parallel test execution, that unguarded reset could land inside `safe_decommit_over_never_committed_tail_succeeds`'s own `serial_guard()`-protected read→decommit→read window — `serial_guard()` only serializes callers who ALSO take it, so an unguarded resetter is invisible to it — zeroing `WINDOWS_VIRTUALFREE_DECOMMIT_ATTEMPTS` mid-window and making the delta read 0 instead of 1. Confirmed load/timing-dependent: the failure disappeared on an isolated re-run and passed 15/15 in a tight rerun loop.
   - **Fix:** added `let _guard = serial_guard();` as the first line of `windows_virtualfree_release_failures_accessor_exists`'s body (`crates/aligned-vmem/tests/lazy_commit.rs`), matching every other counter-touching test in the file. One-line fix.
   - **Evidence:** `npm run check` run during task #1056 (`crush` session `t54-a-item49`), failing step `test (aligned-vmem --all-features)`, assertion `VirtualFree(MEM_DECOMMIT) attempts left: 0, right: 1` at `crates/aligned-vmem/tests/lazy_commit.rs:687`.

68. **Name asymmetry in `Reservation` decommit capability API** (`Reservation::decommit_reclaims_and_zeroes()`, associated `const fn`, compile-time capability query, vs. `Reservation::can_decommit_reclaim_and_zero()`, instance method combining compile-time capability with runtime `is_huge()`) — **CLOSED** by task #1052, no code change (option (c): asymmetry accepted as-is).

   - **Decision:** owner accepted the recommendation of an independent review (`@fh`, task #1052): the two names are each grammatically correct in their own construction, not accidentally inconsistent. `decommit_reclaims_and_zeroes()` is a third-person factual predicate ("decommit *reclaims and zeroes* [memory]"); `can_decommit_reclaim_and_zero()` follows Rust's standard `can_*` + bare-infinitive modal-capability naming pattern ("can [it] *reclaim and zero*"). Both proposed renamings (dropping both `s`es from the associated fn, or adding both to the instance method) would force one of the two names into a grammatically incorrect form purely to gain visual symmetry between two unrelated grammatical constructions.
   - **Discoverability already addressed structurally, without a rename:** the two methods are declared adjacently in `reservation.rs`, cross-linked via intra-doc references in both directions, and the instance method's body literally reads `Self::decommit_reclaims_and_zeroes() && !self.is_huge()` — visibly showing the relationship in the one place a reader would look.
   - **Rejected alternatives:** (a) rename the associated fn to `decommit_reclaim_and_zero` (drop both `s`es) — rejected, produces a grammatically incoherent imperative-looking name. (b) rename the instance method to `can_decommit_reclaims_and_zeroes` (add both `s`es) — rejected, ungrammatical after `can_`. A `#[deprecated]`-alias third option (keep both names) had already been rejected earlier in this card's history as permanent clarity debt.
   - **Bonus argument surfaced during closure:** the minor spelling difference is informative, not noise — it signals the two are genuinely different questions (one compile-time-only, one compile-time + `is_huge()` at runtime), where a forced-symmetric `X()`/`can_X()` pair would have wrongly implied the second is a trivial wrapper of the first.
   Full history: see this file's git history for the pre-closure card (task #934 filed it; tasks #1041/#1045/#1052 revised/corrected it before final closure).

46. **`numa-shim`'s public `reserve_on_node` signature returns `aligned_vmem::Reservation`, coupling the crate's own semver to `aligned-vmem 0.2`** — **CLOSED** by task #1053 (option (a): coupling accepted and documented; `pub use aligned_vmem::Reservation;` added to `numa-shim`, gated `#[cfg(feature = "vmem-integration")]`).

   - **Decision:** owner accepted the recommendation of an independent review (`@fh`, task #1053): `numa-shim` and `aligned-vmem` are sibling crates in the same workspace with a shared release process, so a coordinated semver-major bump across both is a cost the release process already absorbs, not an added one. The rejected alternative (a `numa-shim`-owned newtype around `Reservation`) either needs full API-forwarding boilerplate that drifts on every `aligned-vmem` change, or an `into_inner()` escape hatch that re-exposes the same coupled type anyway — reproducing the exact problem it was meant to remove.
   - **What changed:** `crates/numa-shim/src/lib.rs`'s `reserve_on_node` doc comment gained a "Semver coupling with `aligned-vmem`" section stating the coupling is intentional and why; a new `#[cfg(feature = "vmem-integration")] pub use aligned_vmem::Reservation;` re-export makes the coupling visible in `numa-shim`'s own public API surface (callers can now name the type as `numa_shim::Reservation` without a direct `aligned-vmem` dependency of their own) instead of leaving it implicit. No behavioral/functional code changed.
   - **Consequence:** unblocks task #657 (numa-shim's first crates.io publish gate), which was waiting on this decision.
   Full history: see this file's git history for the pre-closure card (filed task #778/F4, round-closing review, audit §A3).

66. **`Reservation` carried no committed-length state (R6-1 / R7-2, the second of R7's two conditional-NO-GO conditions).** — **CLOSED** by task #1051, commit `0c1e6c4`.

   - **What this card asked for, and why none of its five options were taken.** The card listed: (a) a `committed_len` field on `Reservation`; (b) a separate `LazyReservation` type; (c) returning `(Reservation, usize)`; (d) explicitly ACCEPTING the caller-tracked contract; (e) excluding `lazy-commit` from the supported release profile. I first recommended (d) plus a capability query, on the ground that a field cannot be kept truthful. The owner rejected (d): we are publishing a library, and shipping sharp primitives with the invariants left to prose is the C++ path — consumers should get tools, not homework. That objection was correct, and my argument had been aimed at the wrong target: it was an argument against BOLTING A FIELD ONTO `Reservation`, not against moving the bookkeeping into the crate at all.
   - **Why the field genuinely could not work** (the part of the analysis that survives, and the reason (a) stays rejected): `Reservation`'s mutating methods take `&self`, not `&mut self`, so they cannot update a plain field; the free primitives take a bare `*mut u8` and never see the handle at all; the crate's own only consumer uses exactly those free primitives (`src/alloc_core/os.rs`); and `into_full_parts`/`from_raw_parts` would need a seventh field supplied by the caller, i.e. the field would be only as true as the caller — today's contract with more surface.
   - **What was built instead — option (b), sharpened.** `LazyReservation` owns the `Reservation` AND a watermark. `ensure_committed(len)` is idempotent and monotone (a call at or below the watermark issues no syscall), which is what removes the caller's "did I already commit this?" bookkeeping; `shrink_committed` rounds UP so a page holding bytes the caller asked to keep is never released; `into_reservation()` is the explicit door out. Mutators take `&mut self` — not hygiene, but the crate finally stating a requirement it always had: the watermark is racy and concurrent committers must serialise, which nothing under the raw primitives ever said.
   - **The boundary that keeps this a tool and not an allocator.** Exactly one `usize` is tracked; arbitrary committed/uncommitted HOLES are deliberately not representable. Governing rule, reusable for future proposals: the crate takes on state ONLY when the OS does not cheaply expose it AND the caller would otherwise have to invent it. "How much is committed" passes (Windows knows but charges a `VirtualQuery`; Unix has no such notion); "is THIS page committed" does not, and is not offered.
   - **`lazy_commit_is_honored()`** (const fn) completes the family `decommit_reclaims_and_zeroes()` / `is_huge()`: where a platform difference exists, expose it as something to branch on. `LazyReservation::new` derives the initial watermark FROM that query rather than from the caller's `initial_commit`, which is what makes the two incapable of disagreeing.
   - **Honest limit, documented on the type:** `as_ptr()` still hands out the raw pointer and passing it to the raw primitives makes the watermark stale. No API over raw memory can prevent that. What changed is the DEFAULT — the tracked path is what you get without asking.
   - **Verification:** new `tests/lazy_reservation.rs`, 10 tests, every platform arm asserting rather than skipping. Two counterfactuals run personally: rounding DOWN instead of up made `ensure_committed_rounds_up_to_a_page` FAIL; advancing the watermark WITHOUT issuing the commit killed the binary with `STATUS_ACCESS_VIOLATION` (0xc0000005), proving the writability test touches real OS-committed memory. Both reverted, suite re-run green.
   - **Consequence for the release:** this was the SECOND of R7's two conditional-NO-GO conditions (the first, R7-1, was closed by code in `13723d7`). Both are now closed.

50-U10. **`aligned-vmem` — the U10 half of item 50 ("Windows `bench-internals` reserve-path counters have zero test coverage") rested on a FALSE premise and is closed.** (Filed round 8, task #903, finding U10 of `docs/reviews/2026-08-13-aligned-vmem-round8-review.md`; re-flagged as stale by R7-9 and closed by task #1045.)

   - **Root cause:** item 50's U10 half asserted the counters are "exercised by NOTHING in `crates/aligned-vmem/tests/` — confirmed via `grep -rn \"windows_reserve_commit\" crates/aligned-vmem/tests/` returning no output". That grep does NOT return no output. Three test functions in two files exercise the counters, verified by re-running the grep and mapping every hit back to its enclosing `fn`:
     - `crates/aligned-vmem/tests/bench_internals_counters.rs::bench_internals_counters_existence_and_reset` — asserts `windows_reserve_commit_calls() >= 1` after a live reservation (Windows-gated), then asserts all three counters read `0` after `reset_bench_internals_counters()`.
     - `crates/aligned-vmem/tests/huge_pages.rs::reserve_aligned_huge_2mib_still_two_call_path_unprivileged` — asserts `single_calls + two_call_pairs == 1`, i.e. exactly one dispatch path was taken.
     - `crates/aligned-vmem/tests/huge_pages.rs::reserve_aligned_huge_4mib_still_two_call_path` — asserts `single_calls == 0` AND `two_call_pairs == 1`.
   - **Why this closes U10 rather than merely correcting it:** U10's stated "Next trigger" was a direct assertion that a given shape advances one counter and not the other. `reserve_aligned_huge_4mib_still_two_call_path` IS that assertion, in both directions, and it is discriminating — it fails if the dispatch condition regresses either way. The oracle U10 asked for already exists.
   - **What this closure does NOT claim:** the finer rustdoc claim that a large-page retry "issues a second syscall but is still counted as 1" remains unasserted by any test; it is a syscall-count claim, and no test counts syscalls. Only the DISPATCH claim is covered. Recorded so this closure is not later read as broader than it is.
   - **A correction inside this correction (task #1045):** the first draft of this closure entry cited four test functions, of which TWO do not exist under the names given (`reserve_aligned_huge_2mib_fast_path_or_two_call`, `reset_works`) and a third (`lazy_commit.rs::windows_lazy_reserve_saves_commit_charge`) exists but contains no reference to these counters at all — `grep -rn "windows_reserve_commit" crates/aligned-vmem/tests/lazy_commit.rs` is empty. That is the same failure mode as the claim being fixed: replacing a mis-citation with a new mis-citation (cf. task #900/U2). The inventory above was re-derived mechanically from the grep, not restated from a report.
   - **Files changed:** `docs/CORRECTNESS_OPEN_ITEMS.md` (item 50's U10 half removed from the open card, this closure entry added); `crates/aligned-vmem/src/lib.rs` (`WINDOWS_RESERVE_COMMIT_TWO_CALL_PAIRS`'s rustdoc lost its stale "third best-effort retry" clause — the two-call path never requests `MEM_LARGE_PAGES`, per the code comment's own task #921/V-7 attribution).

1. **Flaky test — `canary_survives_promotion_and_free_leaves_no_leak`**
   (`tests/r14_4_promotion_free_correctness.rs`) — **RESOLVED** by an urgent
   CI-fix task (2026-07-26), responding to `origin/main` CI run `30217256247`
   / job `89833506941` failing on the `test (--features "hardened
   medium-classes")` step with `error: 1 target failed: --test
   r14_4_promotion_free_correctness`.

   - **Root cause, confirmed:** `SEGMENTS_RESERVED_TOTAL`/
     `SEGMENTS_RELEASED_TOTAL` (`src/alloc_core/os.rs:52,57`) are
     process-wide `static AtomicU64`s. Both `#[test]` functions in this file
     (`canary_survives_promotion_and_free_leaves_no_leak` and
     `repeated_promote_and_free_does_not_leak_unboundedly`) read `a.stats()`
     — which loads these same global atomics — take a before/after
     snapshot, and assert a leak-free delta. `cargo test` runs test
     functions concurrently on multiple OS threads within one process by
     default; the two tests in this file (or any other test in the same
     binary) could reserve/release a segment on the shared counters between
     one test's own snapshots, polluting its delta with unrelated activity
     — exactly the historically observed "failed 1 of 3 runs" signature.
   - **Fix:** added a file-scoped `static TEST_LOCK: Mutex<()>` + `serial()`
     helper (the SAME established pattern already used in
     `tests/directory_authoritative_miss.rs`, `tests/alloc_zeroed_fresh_large_skip.rs`,
     `tests/r13_3_magazine_virgin_hit_skips_zero.rs`,
     `tests/r21_2_opt_h_stage1_precondition_probe.rs` for tests that read
     process-wide stats/diagnostic counters), and bound `let _guard =
     serial();` at the top of BOTH test functions in the file (both read
     the same global counters, so both needed serialization, not just the
     one named in the CI failure). No assertion logic was changed — the
     `released_delta <= reserved_delta` leak-bound check is untouched.
   - **Verification:** 4 full runs of the exact CI command (`cargo test
     --features "hardened medium-classes" --no-fail-fast`, matching R22-1's
     CI row exactly — 223 test binaries each run) — all clean, 0 failures.
     Additionally ~190 direct repeated invocations of the specific compiled
     test binary (`--test-threads=4/8/16`, mimicking CI-like concurrency)
     plus several `cargo test --test r14_4_promotion_free_correctness`
     invocations — 0 failures out of roughly 200+ total runs, against the
     historical ~1-in-3 failure rate. `cargo fmt --check` clean on the
     changed file.
   - **Files changed (test/implementation only):**
     `tests/r14_4_promotion_free_correctness.rs`; this index entry itself
     is the second file touched in the same commit (`bc4aacf`).
   - **Scope of what this fix actually proves:** this fix resolved the
     test-ISOLATION RACE only — it did not touch, and did not strengthen,
     the test's own leak-bound assertion (`released_delta <=
     reserved_delta`). That assertion is a DOUBLE-RELEASE guard (released
     count never exceeding reserved count), not a proof of no leak: if a
     grow reserved a segment and never released it, `reserved_delta=1,
     released_delta=0` satisfies `0 <= 1` trivially, so a genuine
     never-released segment would not be caught by this test. This
     semantic gap pre-dates `bc4aacf` and was correctly left untouched by
     it — fixing the test-isolation race was that commit's actual, correct
     scope. See open item 4 above ("Open items" §`[T]`) for a tracked
     follow-up on strengthening leak detection itself.

3. **Flaky test — `shadow_path_activation_oracle_fast_and_slow_both_reachable`
   scheduler-sensitive percentage thresholds (BOTH regimes).** **RESOLVED**
   in TWO steps, both 2026-08-16 — do not read this card as landing in a
   single commit:
   (a) the root-cause fix, commit `8d68715` (task #1030): the `SERIAL`
   guard plus exact-equality assertions;
   (b) a portability follow-up, the commit carrying this entry (task
   #1033, finding F5 of
   `docs/reviews/2026-08-16-aligned-vmem-r6-wave-review.md`): (a)'s
   exact equalities were only valid on strong-CAS targets, and were
   replaced by two-sided bounds.

   - **Root cause, confirmed:** process-global `DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`
     counters are shared across all tests in the binary. libtest runs this
     file's three `#[test]` functions concurrently by default. Without a
     serial guard, concurrent sibling tests can pollute the counters between
     a test's `before`/`after` snapshots, causing spurious percentage
     assertion failures. The original failure pattern (slow_delta itself
     was structurally correct at 2000 ≈ ROUNDS, but fast_delta grew to 256
     under load) proves this: the adversarial mechanism worked, but the
     DELTA measurement was corrupted by concurrent noise.
   - **Fix (a), commit `8d68715`:** introduced a `static SERIAL: Mutex<()>`
     guard (following the pattern from `crates/aligned-vmem/tests/smoke.rs`) that
     all three `#[test]` functions in this file hold for their entire
     bodies. With the serial guard, each test has exclusive access to the
     global counters, eliminating the concurrent noise source. Because the
     noise source was gone, the assertions were then TIGHTENED rather than
     relaxed: lower bounds became exact equalities (`assert_eq!(total,
     ROUNDS)`), and the percentage thresholds moved from ≥95% to ≥99% fast
     path in the favorable regime and from a percentage to a
     `slow_delta >= ROUNDS - 10` count check in the adversarial one.
     Guard effectiveness was demonstrated by removing the guard: `cargo
     test --all-features --test remote_ring_shadow_head` then reported
     2258 against an expected 2000, i.e. one test's counter delta absorbed
     roughly 258 pushes issued by a concurrently running sibling. (The
     original card named a specific direction for that cross-test
     pollution; it is not recoverable from the recorded evidence, which
     shows only the polluted total, so the direction is deliberately left
     unstated here rather than guessed.)
   - **Fix (b), the commit carrying this entry:** (a)'s exact equalities
     silently assumed `compare_exchange_weak` never fails spuriously.
     `RemoteFreeRing::push` (`src/alloc_core/remote_free_ring.rs`) is a
     `loop { load tail; full_check(t); compare_exchange_weak(..) }`, and
     `full_check` bumps exactly one shadow counter PER CALL — so on an
     LL/SC target (aarch64/ARM) a spurious CAS failure retries the loop
     and double-counts one logical push. Exact equality therefore held
     only on strong-CAS targets, where `compare_exchange_weak` lowers to
     `lock cmpxchg`. Replaced with two-sided bounds
     (`total >= ROUNDS && total <= ROUNDS + CAS_RETRY_SLACK`,
     `CAS_RETRY_SLACK = 8`), which keeps everything (a) was for — the
     original failure was a ~256-push distortion by a sibling test, not a
     handful of CAS retries — while staying valid on every architecture.
     Latent, not live: both tests require `feature = "bench-internals"`,
     which is in neither `production` nor `experimental`, and every
     aarch64/macOS CI row uses only those bundles, so the tests do not
     compile there today. The failure would surface the moment an
     aarch64 row gains `bench-internals`, or on a local
     `cargo test --all-features` on Apple Silicon.
   - **Counterfactual:** removing the adversarial loop's
     `dbg_advance_head_only` call (so the shadow never goes stale) makes
     the test fail, confirming the oracle still detects a non-reached
     adversarial condition. Re-verified after fix (b) landed; note the
     assertion message changed between (a) and (b), so a quoted failure
     string is only meaningful against the matching revision.
   - **Verification:** `cargo test --all-features --test
     remote_ring_shadow_head`, 5 consecutive clean runs, plus
     `cargo clippy --all-features --all-targets -- -D warnings` clean —
     re-run for (b), not inherited from (a).
   - **Files changed:** `tests/remote_ring_shadow_head.rs` in both
     commits, plus this index entry.

2. **Clippy dead-code — `--features "hardened medium-classes"` was not
   clippy-clean (11 errors)** — **RESOLVED** by R23-5 (task #374). All 11
   were genuine `#[cfg(...)]` predicate mismatches (an item gated one way,
   its only consumer gated a DIFFERENT way, so under the specific
   intersection `hardened medium-classes` the consumer compiled out but the
   item did not) — confirmed exhaustively per item via `grep` across
   `src/`, `tests/`, `benches/`, `crates/` before touching anything; NONE
   were genuine orphans, so nothing was deleted.

   - **Items 1, 2, 4 — independent single-item mismatches:**
     - `src/alloc_core/alloc_core.rs:54` (unused import `SMALL_CLASS_COUNT`):
       both of the import's only two usages
       (`alloc_core.rs:711`/`directory_miss_streak` field,
       `alloc_core.rs:978`/its initializer) are
       `#[cfg(feature = "alloc-segment-directory")]`-gated, but the `use`
       itself was not. Fix: split the import so `SMALL_CLASS_COUNT` gets its
       own `#[cfg(feature = "alloc-segment-directory")]` line, matching its
       usages; `AllocKind`/`SizeClasses` (used unconditionally elsewhere)
       stay ungated.
     - `src/alloc_core/alloc_core_large.rs:448` and
       `src/alloc_core/alloc_core_small.rs:1941` (`let mut seg = ...` "does
       not need to be mutable"): both `seg` bindings are reassigned ONLY
       inside a `#[cfg(feature = "alloc-decommit")]` pool-drain-and-retry
       block a few lines below; with `alloc-decommit` off (as under
       `hardened medium-classes`) the binding is genuinely never mutated.
       Fix: `#[allow(unused_mut)]` on each binding, following the identical
       established pattern already at
       `src/registry/heap_core_ownership.rs:167` for the same
       feature-conditional-mutation shape.
   - **Items 3, 5, 6 — one unified root cause (`small_cur`), as suspected in
     the task brief:** `AllocCore::small_cur()` (`alloc_core.rs`, was gated
     `#[cfg(feature = "alloc-xthread")]`) has exactly one caller in the
     entire crate — `heap_core_xthread.rs::drain_heap_overflow`, which reads
     it ONLY inside its own `#[cfg(feature = "alloc-decommit")]` block
     (feeding `dec_live_and_maybe_decommit`, which itself requires that
     feature). `alloc-xthread` without `alloc-decommit` (exactly `hardened
     medium-classes`: `hardened = ["fastbin"]` →
     `["alloc-global","alloc-xthread"]`, neither of which pulls in
     `alloc-decommit`) left the method callable-but-uncalled. The two local
     `let small_cur = self.small_cur;` bindings
     (`alloc_core_small.rs:893`, `alloc_core_small_reclaim.rs:506`) are the
     SAME pattern one level down: each is read only inside its own sibling
     `#[cfg(feature = "alloc-decommit")]` block a few lines later. Fix:
     tightened `small_cur()`'s gate to
     `#[cfg(all(feature = "alloc-xthread", feature = "alloc-decommit"))]`
     (its true minimal predicate, matching its one caller), and gated both
     local bindings `#[cfg(feature = "alloc-decommit")]` directly (matching
     their one reader each). Verified two OTHER `let small_cur = ...`
     bindings at `alloc_core_small.rs:1132` and `:2545` were NOT in the
     11-error list and left untouched — clippy did not flag them (their
     enclosing functions/blocks have their own gating that made them a
     non-issue under this combo), confirming the fix was scoped to exactly
     the 3 flagged sites, not a mechanical crate-wide rename.
   - **Items 7-9 — one unified root cause (`sidecar.rs`), as suspected in
     the task brief:** `reserve_zeroed_with` has exactly one caller,
     `os.rs::reserve_directory_sidecar`, gated
     `#[cfg(feature = "alloc-segment-directory")]`. `deref`/`deref_mut` each
     have TWO independent consumer groups — `alloc_core_small.rs`'s
     `directory`/`directory_mut`/`maybe_materialize_directory` +
     `alloc_core_core_diag.rs`'s `dbg_rebuild_directory` (all inside
     `#[cfg(feature = "alloc-segment-directory")]`), and
     `large_cache_extended.rs`'s `deref_large_cache_extension[_mut]`
     forwarders (the whole module gated
     `#[cfg(feature = "large-cache-extended")]`) — either feature alone
     keeps them used. Under `hardened medium-classes`, `alloc-segment-directory`
     is off AND `large-cache-extended = ["alloc-decommit"]` is transitively
     off too (via `alloc-decommit`), so all three functions had zero live
     callers. Fix: followed the EXISTING convention already used one
     function above in the same file (`reserve`'s
     `#[cfg_attr(not(feature = "large-cache-extended"), allow(dead_code))]`,
     predating this task) rather than a hard `#[cfg]` on the function
     itself (keeps these generic `pub(crate) fn`s type-checking under
     `cargo-hack`-style per-feature builds) —
     `#[cfg_attr(not(feature = "alloc-segment-directory"), allow(dead_code))]`
     on `reserve_zeroed_with`, and
     `#[cfg_attr(not(any(feature = "alloc-segment-directory", feature = "large-cache-extended")), allow(dead_code))]`
     on `deref`/`deref_mut` (the `any(...)` reflecting their two independent
     consumer groups, neither of which alone is necessary).
   - **Items 10-11 — two independent single-item mismatches, as suspected:**
     - `src/registry/heap_core_xthread.rs:586`
       (`const EMPTIED_BASES_CAP: usize = 64;`, itself ungated): every
       actual usage (the `emptied_bases`/`emptied_count` declarations and
       both `if emptied_count < EMPTIED_BASES_CAP` comparisons) is already
       `#[cfg(feature = "alloc-decommit")]`-gated; only the constant
       declaration itself lacked the gate. Fix: added
       `#[cfg(feature = "alloc-decommit")]` to the `const` line, matching
       its usages.
     - `src/registry/heap_registry.rs:523` (`struct ConflictRollback`, and
       its `impl Drop`): constructed exactly once, inside
       `claim_with_config`'s config-mismatch branch — and
       `claim_with_config` itself is `#[cfg(feature = "alloc-decommit")]`-gated
       ("Only present under `alloc-decommit`", per its own doc comment).
       Fix: added `#[cfg(feature = "alloc-decommit")]` to both the struct
       and its `impl Drop`.
   - **One additional latent issue found and fixed in the same task (not
     among the original 11, but the same predicate-mismatch class, and
     newly exposed by fixing the 11 above — the lib now compiles under this
     combo, so `--all-targets` reaches this test target for the first
     time):** `tests/regression_batch_flush.rs`'s `DECOMMIT_COUNTER_SERIAL`/
     `SerialGuard` (a `TEST_LOCK`-style serialization guard) and its
     `use std::sync::atomic::{AtomicBool, Ordering}` import were declared
     unconditionally, but every actual use is inside
     `#[cfg(feature = "alloc-decommit")]`-gated test functions. Fixed the
     same way: gated the static/struct/impls/import on
     `#[cfg(feature = "alloc-decommit")]`.
   - **No deletions.** Every one of the 11 (plus the 1 latent test-file
     issue) was confirmed genuinely used under some other feature
     combination already in this project's CI matrix before any fix was
     applied — verified by `grep`ing every call site across the whole repo
     (not just under `hardened medium-classes`).
   - **Verification:**
     `cargo clippy --all-targets --features "hardened medium-classes" -- -D warnings`
     — 0 errors, 0 warnings (down from the stable 11). No new warning
     surfaced as a side effect of any individual fix (re-ran the full
     command after each fix). `cargo test` green across all of: `""`
     (default), `production`, `--all-features`, `hardened medium-classes`,
     `production alloc-stats`, `pinning` (the full
     `scripts/check-all.mjs` test-step feature matrix) — 0 failures in
     every combination. `cargo fmt --all -- --check` clean.
   - **CI:** added a 4th step to the `clippy` job in `.github/workflows/ci.yml`
     (`clippy (--features "hardened medium-classes")`, alongside the
     existing `clippy ()` / `clippy (--features experimental)` /
     `clippy (--all-features)` steps in that same job) so this combination's
     `-D warnings` gate now runs per-PR, not just `cargo test` (closed
     R22-1's deliberately-left-open gap).
   - **Files changed:** `src/alloc_core/alloc_core.rs`,
     `src/alloc_core/alloc_core_large.rs`, `src/alloc_core/alloc_core_small.rs`,
     `src/alloc_core/alloc_core_small_reclaim.rs`, `src/alloc_core/sidecar.rs`,
     `src/registry/heap_core_xthread.rs`, `src/registry/heap_registry.rs`,
     `tests/regression_batch_flush.rs`, `.github/workflows/ci.yml`, and this
     index.

3. **Deferred decision — `aligned-vmem`'s `mock` Cargo-feature-unification
   hazard was resolved with a doc-only fix, explicitly deferring a
   stronger `--cfg`-flag conversion; the SAME finding recurs in
   `numa-shim` and the deferral is load-bearing for that crate's own
   upcoming round.** — **CLOSED** (updated 2026-08-09, task #778/F5 —
   round-closing review of the numa-shim round). Filed 2026-08-09,
   task #776/F13, round-closing review of the aligned-vmem round.
   **RE-OPENED** 2026-08-14 (task #934/C-9) — see the `[A]` tier's item 42; the deadline this deferral was conditioned on has fired.

   - **Closure narrative:** `numa-shim`'s round reached its own §C10 finding
     in task #726 (commit `53b3ca2`) and applied EXACTLY the policy this
     item's "Next trigger" prescribed — a doc-only fix
     (`crates/numa-shim/Cargo.toml`'s `mock = []` feature comment, the `mock`
     module's own rustdoc, and a new `README.md` section), citing task
     #715's reasoning — but the review found this card was never updated
     in that commit, the same "update the card in the SAME commit"
     violation this file's own convention exists to catch (see item 41's
     analogous correction above). Both crates now carry the SAME recorded
     policy for the identical finding shape, so this item is closed
     rather than left open with a stale forward-reference.
   - **Current-number-or-verdict:** `aligned-vmem`'s `mock` feature
     (`crates/aligned-vmem/Cargo.toml`) AND `numa-shim`'s `mock` feature
     (`crates/numa-shim/Cargo.toml`) are both Cargo features (not `--cfg`
     flags), each documented with a Cargo-feature-unification warning in
     three places (`Cargo.toml`, the `mock` module's own doc, `README.md`).
     Task #715 (commit `e5f6700`) explicitly evaluated and DEFERRED the
     stronger fix for `aligned-vmem` — converting `mock` from a Cargo
     feature to a `--cfg`-style RUSTFLAGS flag, matching this repo's own
     `cfg(loom)`/`cfg(kani)` precedent (cfg flags do not unify across a
     build the way Cargo features do) — reasoning that neither crate has
     real external consumers before its first publish (`aligned-vmem`:
     task #658; `numa-shim`: task #657), so the doc-only fix closes the
     realistic near-term risk at much lower cost than a mechanical
     rewrite of the whole test-invocation surface and CI matrix for
     EITHER crate. Task #726 (commit `53b3ca2`) applied the identical
     reasoning to `numa-shim`.
   - **Evidence:** `crates/aligned-vmem/Cargo.toml`'s `mock = []` feature comment
     (the "CARGO FEATURE-UNIFICATION HAZARD" block) and
     `crates/numa-shim/Cargo.toml`'s `mock = []` feature comment both state
     the deferral explicitly: "Revisit if/when this crate gains external
     consumers and the hazard is reported for real." (Cited by feature
     name, not line range — round-5 closing review QC6 found a line-range
     citation into this exact block go stale within the same round that
     wrote it.)
   - **Revisit condition (both crates jointly):** if EITHER crate gains a
     real external consumer that reports this hazard for real, re-open
     this item and revisit BOTH crates' resolution together — do not let
     one crate silently drift to a `--cfg`-flag conversion while the
     other stays doc-only for the same finding shape.

4. **Two flaky coarse-wall-clock tests surfaced by `npm run check`'s
   `--all-features` step** — **RESOLVED** by R23-6 (task #375). One
   independent read-only review first corrected the originally-proposed fix
   (a `TEST_LOCK`-style mutex): a mutex only serializes test FUNCTIONS
   within ONE test binary/process, but the actual flakiness source is CPU
   contention from MULTIPLE test binaries (separate OS processes) running
   concurrently under `npm run check`'s `--all-features` step, plus the CI
   runner's own background load — a mutex inside one binary cannot
   serialize against a different process. That correction was confirmed
   independently before this task began and is reflected in the fix below
   (no `TEST_LOCK` was added to either file).

   - **`tests/regression_segment_table_tombstone_rebuild.rs::backshift_no_latency_spike_at_threshold_boundary`
     — got a deterministic replacement.** The test's (b) claim ("no single
     `unregister`/`recycle` does `O(HASH_CAPACITY)` work") maps exactly onto
     `SegmentTable::hash_remove`'s backward-shift scan-step count (the
     `j = (j+1) & mask` walk across both its find-the-slot and
     shift-the-cluster phases). Added `HASH_REMOVE_MAX_SCAN_STEPS`
     (`src/alloc_core/segment_table.rs`) — a process-wide high-water-mark
     `AtomicU64`, `alloc-stats`-gated increment (same convention as
     `OPT_H_ATTEMPTS`/`HARDENED_LARGE_NOOP_COUNT`), reset hook
     `reset_hash_remove_max_scan_steps`, and `AllocCore` accessors
     `dbg_hash_remove_max_scan_steps`/`dbg_reset_hash_remove_max_scan_steps`
     (`src/alloc_core/alloc_core_core_diag.rs`) — deliberately a MAX not a
     sum, matching the original test's own "no single call is an outlier"
     framing rather than conflating many small deletes with one large one.
     New test `backshift_max_scan_steps_bounded_at_threshold_boundary`
     (`#[cfg(feature = "alloc-stats")]`, same file) drives the identical
     `W = 600`-distinct-bases wave-then-drain shape as the original and
     asserts the high-water mark stays `<= 4 * W` (`HASH_CAPACITY = 8192`
     would be ~13.6x that bound) — a deterministic per-run assertion, zero
     timing, zero retries. (R24-1/task #379 wording-precision note: the
     MEASUREMENT is exact per-run, but the `4 * W = 2400` threshold is a
     regression bound calibrated to this wave's `W = 600`, not a proven
     O(cluster) worst-case for arbitrary configurations — it reliably catches
     a full O(`HASH_CAPACITY`) regression but could miss a smaller
     pathological cluster under 2400 steps.) The original wall-clock test is
     KEPT (not deleted, for manual/`--ignored` diagnostic value) but marked
     `#[ignore = "..."]`
     with a message pointing at the deterministic replacement and
     `npm run iai`.
   - **`tests/dealloc_sublinear.rs::own_thread_free_is_subquadratic` —
     no clean deterministic replacement exists; demoted to non-blocking.**
     Investigated seriously (per this task's explicit instruction not to
     default to demotion): the guard this test protects
     (`AllocCore::dealloc_small`'s M2 double-free check,
     `src/alloc_core/alloc_core_small.rs`) is, by design, an UNCONDITIONAL
     O(1) `AllocBitmap::is_free` bit test with NO loop — Phase 13.4a already
     replaced the O(free-list-length) `free_list_contains` walk this test
     guards against with exactly that O(1) bitmap test. A call-count counter
     ("how many times was the guard tested") would read identically (= N
     after N frees) under BOTH the correct O(1) implementation and the
     regressed O(N²) walk it guards against — the walk's CALL COUNT never
     changed across that regression, only its internal LENGTH did, and there
     is no length-dependent loop left in production code to instrument. The
     only way to get a counter would be adding one to code that would first
     need to reintroduce the very walk being guarded against — not a
     diagnostic-only addition. Per this task's constraint ("if a new counter
     requires touching a genuinely hot path... stop and explain the
     tradeoff"), this test is `#[ignore]`d instead, with a message pointing
     at manual `--ignored` runs and `npm run iai` /
     `benches/perf_gate_iai.rs`'s `small_churn_16b`-family arms as the
     deterministic Ir-based judges for this same free-path cost.
   - **Mechanism confirmed:** `scripts/check-all.mjs` runs `cargo test
     --features <combo>` for each of its feature-matrix entries and fails
     the whole gate on ANY test failure (including any `#[ignore]`d-off
     test simply not running) — `#[ignore]` is exactly the mechanism
     `cargo test` (and therefore this repo's CI/`check-all.mjs`) already
     uses to exclude a test from the blocking pass/fail gate while keeping
     it runnable via `cargo test -- --ignored`, so no `check-all.mjs`/CI
     workflow change was needed.
   - **Non-vacuity — mutation counterfactual (the new deterministic test):**
     temporarily forced `hash_remove`'s phase-1 find loop to burn
     `HASH_CAPACITY - 1` extra counter increments before matching (simulating
     the pre-N3 O(HASH_CAPACITY) tombstone-scan regression class directly,
     without touching pointer/unsafe logic) —
     `backshift_max_scan_steps_bounded_at_threshold_boundary` FAILED
     immediately (`max_steps = 8191` against the `2400` bound, with a
     message correctly naming the O(HASH_CAPACITY) regression class);
     reverted, and the test passed again. Confirms the new test is
     non-vacuous — it fails without the property it's checking for holding.
     No counterfactual was performed for `own_thread_free_is_subquadratic`
     (it was demoted, not replaced) — its own pre-existing counterfactual
     documentation (module doc comment, "author-verified" restoring the old
     `free_list_contains` walk trips the assertion) is unchanged and still
     applies to manual/`--ignored` runs.
   - **Verification:** `cargo test --features production` (223 binaries),
     `cargo test --features "production alloc-stats"` (223 binaries, exit
     0 — this is the combo that actually compiles and runs the new
     deterministic test), and `cargo test --all-features` all green, 0
     failures. Both previously-flaky tests confirmed `... ignored, <reason>`
     under every combo they're compiled under; the new deterministic test
     confirmed `... ok` under `production alloc-stats` and `--all-features`,
     and confirmed ABSENT (not vacuously passing) under plain `production`
     (no `alloc-stats`). `cargo clippy --all-targets -- -D warnings` clean
     across all three CI feature-matrix entries (`""`, `experimental`,
     `--all-features`). `cargo fmt --all -- --check` clean on all touched
     files.
   - **Files changed:** `src/alloc_core/segment_table.rs`,
     `src/alloc_core/alloc_core_core_diag.rs`,
     `tests/regression_segment_table_tombstone_rebuild.rs`,
     `tests/dealloc_sublinear.rs`, and this index.

5. **`dealloc_batch_small` doc comment claimed the LAST `TCACHE_CAP` freed
   blocks stay magazine-warm; the implementation keeps the FIRST.** —
   **RESOLVED** by R24-7 (task #385), a doc-only policy decision (no `src/`
   behavior change, no numbers measured).

   - **First observed:** independent read-only review of Round 23
     (`docs/reviews/2026-07-27-r23-readonly-review.md` §5.3).
   - **The gap:** `src/registry/heap_core_dealloc_batch.rs`'s
     `dealloc_batch_small` "Trade-off" doc comment (from the original R11-4
     commit `ff9ad7a`) claimed the LAST `TCACHE_CAP` blocks stay
     magazine-warm. The implementation iterates `for &p in blocks` in slice
     order and fills the magazine until `count == TCACHE_CAP`, then routes
     every further ACCEPTED block to `flush_class` — so with an empty
     magazine the FIRST `TCACHE_CAP` accepted blocks stay warm, the opposite
     of the claim.
   - **Decision (option (a) of the R24-7 brief): correct the documentation
     to describe the actual first-warm behavior; do NOT switch to a
     rolling-buffer last-warm algorithm.** `git blame` shows the "last warm"
     text was in the original R11-4 commit, unedited since, with no recorded
     rationale — an aspirational doc error matching scalar temporal-locality
     intuition, never verified against the always-first-warm implementation;
     there is no design reason "last" was specifically chosen that "first"
     would defeat. A last-warm rolling buffer would add, per overflow block,
     a `clear_magazine` RMW on a hot L1 bitmap line plus rotation/index
     arithmetic plus an extra stage write — strictly MORE per-block work
     than the current overflow arm (which only writes to `stage`), i.e. the
     SAME cost category two adjacent Round-24 tasks measured as NO-GO
     regressions in this exact code region: R24-3 (task #381,
     +37 Ir/overflow-event) and R24-4 (task #382, +14 Ir/block). The benefit
     (locality for "free a large batch then immediately realloc same
     class") is contested by the doc's own use-case argument AND has no
     in-tree consumer (R23-7: the batch API ships experimental with no
     production caller), so even a zero-cost switch would realize no
     production benefit today. Under that prior, prototyping the rolling
     buffer would very likely reproduce the R24-3/R24-4 regression class.
   - **The corrected doc comment's secondary claim is unaffected and still
     holds:** a small batch (`N <= TCACHE_CAP`) is byte-for-byte as warm as
     the scalar loop under EITHER first- or last-warm policy (all `N`
     accepted blocks fit the magazine).
   - **No in-context Ir measurement was run,** because option (b) was not
     pursued: the brief's own recommendation frames (a) as the default and
     (b) as the higher-bar prove-it-first path, and the structural prior
     (two NO-GOs in the same cost category + no consumer + the doc's own
     argument against the benefit) made the measurement very likely to only
     confirm a regression. The mandatory-if-pursued measurement gate
     therefore did not apply.
   - **Files changed:** `src/registry/heap_core_dealloc_batch.rs` (doc
     comment only) and this index. Zero `src/` behavior change; `git diff
     HEAD -- src/` shows only the doc-comment edit. No version bumps.

30. **`canary_survives_promotion_and_free_leaves_no_leak`'s leak-bound
   assertion proved no double-release, not no leak.** — **RESOLVED** by
   R28-2 (task #431), a test-only strengthening (no `src/` behavior change).

   - **The gap (recap):** the pre-existing `released_delta <=
     reserved_delta` assertion in
     `tests/r14_4_promotion_free_correctness.rs` is satisfied trivially by
     `reserved_delta=1, released_delta=0`, so a grow that reserves a segment
     and never releases it would pass silently — the assertion is a
     double-release/corruption guard, not a leak proof.
   - **Observable used — no new hook needed.** Investigated existing
     `SegmentTable`/diagnostic surface before adding anything: `HeapCore`
     already exposes `dbg_contains_base` (`&mut self`, gated
     `alloc-global + alloc-xthread`, `src/registry/heap_core_diag.rs:482`)
     and `dbg_live_count_for` (`&self`, gated `alloc-decommit`,
     `heap_core_diag.rs:317`), both safe `pub fn`s already appropriately
     gated per the benchmark-hook rule (not new — pre-existing, ungated
     wider than needed). Both gates are satisfied by plain `production`
     (`alloc-xthread` and `alloc-decommit` are both in the `production`
     feature list), so no new hook and no `bench-internals` dependency was
     required. To reach a `*mut HeapCore` for the CURRENT thread's own
     `SeferAlloc` from a `tests/` integration test (`SeferAlloc` itself
     exposes no direct `HeapCore` accessor), reused the SAME established
     save/poison/restore pattern `tests/dealloc_only_no_bind_torn.rs`
     already uses: `sefer_alloc::global::tls_heap::dbg_mark_local_torn_for_test()`
     (snapshot + poison `LOCAL`) immediately followed by
     `dbg_restore_local_for_test(saved)` (undo the poison), yielding the
     saved pointer — binding is per-THREAD (TLS), not per-`SeferAlloc`-
     instance, so this is exactly the same `HeapCore` the test's own
     `a.alloc`/`a.dealloc` calls already routed through.
   - **The new assertion:** resolves `grown`'s segment base
     (`dbg_segment_base_of_ptr`) and calls the production teardown-trim
     primitive `SeferAlloc::dbg_trim_current_thread()` (pre-existing,
     `src/global/sefer_alloc.rs:423` — flushes every tcache class, drains
     the empty-small-segment hysteresis pool, evicts the large_cache) BOTH
     immediately before taking a `live_count` baseline AND immediately after
     freeing `grown`, so both snapshots are read in the same converged,
     magazine/pool/cache-free regime (the double trim matters: a freed
     block routinely sits in the per-thread magazine rather than being
     reconciled into `live_count` immediately — see the gap found in
     development, below). After freeing and trimming, asserts `grown_base`
     is in exactly one of two sanctioned states: (a) fully unregistered
     (`dbg_contains_base == false` — the Large-segment-free path always
     calls `table.unregister` before returning, cache-admitted or not), or
     (b) still registered but with `live_count` decreased by EXACTLY one
     relative to the pre-free baseline (covers the `!HAS_PROMOTION`
     medium-ladder case, where segments are shared across size classes via
     a single per-thread `small_cur` bump cursor and routinely host other
     live blocks). Any other outcome (segment still registered with an
     unchanged or increased live_count) fails with a message naming the
     leak.
   - **Design iteration during development (kept in the report per the
     task's non-vacuity requirement, not just the final counterfactual):**
     two earlier designs were tried and rejected by ACTUAL feature-combo
     test runs, not just review — (1) a bare `dbg_contains_base(grown_base)
     == false` assumption failed under `production medium-classes
     exact-span-large` (`HAS_PROMOTION == false`) because medium-class
     segments are shared across size classes (`AllocCore::carve_block`'s
     single per-thread `small_cur`), so the segment legitimately stays
     registered with other live blocks; (2) an absolute
     `live_count_after_free == Some(0)` assumption also failed under the
     same combo (`live_count` went `Some(2)` before and after — unrelated
     co-tenant blocks were still magazine-buffered, not yet reconciled),
     which led to discovering the magazine-residency gap (`dealloc` does
     not call `dec_live` for a block that lands in the tcache — see
     `HeapCore::dbg_is_free_for`'s doc comment) and the final double-trim,
     before/after-delta design above.
   - **Non-vacuity — mutation counterfactual, run TWICE (once for the
     Large-promoted path, once for the medium-ladder path, since they are
     structurally different code paths):**
     - **Large path** (`production medium-classes`, `HAS_PROMOTION ==
       true`): commented out the `self.table.unregister(base)` call in the
       cache-admitted leg of `AllocCore::dealloc`'s Large branch
       (`src/alloc_core/alloc_core.rs:1451`), simulating a grow that
       deposits a segment into `large_cache` but never removes it from the
       table. New assertion FAILED immediately: `LEAK: grown_base (...) is
       still registered in the segment table ... live_count went from None
       to None`. Reverted; `git diff` confirmed byte-identical to the
       original; test passed again.
     - **Medium-ladder path** (`production medium-classes exact-span-large`,
       `HAS_PROMOTION == false`): commented out the
       `dec_live_batch_and_maybe_decommit`-driven block inside `flush_run`
       (`src/alloc_core/alloc_core_small_magazine.rs:682-693`, guarded with
       `#[cfg(any())]` for a clean single-site disable), simulating a leak
       where a block returned to the magazine-flush path never reconciles
       its live_count. New assertion FAILED with `live_count went from
       Some(2) to Some(2)` — exactly the "no change at all" signature the
       assertion's own doc comment predicts for this failure mode. Reverted;
       `git diff` confirmed byte-identical to the original; test passed
       again. (An earlier, less isolated counterfactual attempt at the same
       Large-branch call site under this combo produced a
       `STATUS_ACCESS_VIOLATION` crash instead of a clean assertion failure
       — because skipping `unregister` while `large_cache_slot_set` still
       ran created a genuinely double-owned segment that
       `dbg_trim_current_thread`'s `evict_all` then double-freed; this was
       diagnostic noise from an overly-blunt counterfactual site, not a
       defect in the new assertion, so the `flush_run` site above was used
       instead for a clean, isolated result.)
   - **CI-compatibility gap found and fixed during zero-trust review (before
     commit):** the strengthened block's own two accessors need
     `alloc-decommit` (`dbg_live_count_for`) and `alloc-xthread`
     (`dbg_contains_base`), a strictly NARROWER feature set than this file's
     own top-level `#![cfg(all(feature = "alloc-global", feature =
     "medium-classes"))]` gate — and `.github/workflows/ci.yml` runs a
     dedicated `test (--features "hardened medium-classes")` step
     (`hardened = ["fastbin"]` = `alloc-global + alloc-xthread`, WITHOUT
     `alloc-decommit`) that exercises this exact file. The as-delegated diff
     compiled clean only under the two combos it was directly tested against
     (`production medium-classes[, exact-span-large]`, both of which include
     `alloc-decommit` via `production`) and failed to compile under `hardened
     medium-classes` with two `E0599: no method named dbg_live_count_for`
     errors — confirmed via `cargo test --no-run --features "hardened
     medium-classes" --test r14_4_promotion_free_correctness` BEFORE the fix.
     Fixed by narrowing the new block's own gate to `#[cfg(all(feature =
     "alloc-decommit", feature = "alloc-xthread"))]` (a `let (heap,
     grown_base, live_count_before_free) = { ... };` tuple-block before the
     unconditional `a.dealloc` call, and a second `#[cfg(...)]` block after
     it for the assertion itself — the actual `a.dealloc(grown, ..)` call the
     pre-existing `released_delta <= reserved_delta` assertion needs stays
     UNCONDITIONAL either way) rather than widening the file's own top-level
     gate, which would have silently dropped this test from the `hardened
     medium-classes` CI row's coverage entirely. Re-verified after the fix:
     `cargo test --no-run --features "hardened medium-classes" --test
     r14_4_promotion_free_correctness` compiles clean and the test still
     passes (exercising only the original double-release guard, as before
     this task); both counterfactuals above were RE-RUN against the
     restructured code (not just the pre-restructure version) and still fail
     correctly.
   - **Verification:** `cargo test --release --features "production
     medium-classes" --test r14_4_promotion_free_correctness` and `cargo
     test --release --features "production medium-classes exact-span-large"
     --test r14_4_promotion_free_correctness` both green (2 passed, 0
     failed) after the final design landed. Repeat-run flake check: 35
     `cargo test` invocations plus 120 direct repeated binary invocations
     (60 per feature-combo binary, `--test-threads=4`) — 1 anomalous failure
     out of ~155 total runs, attributed to this session's heavy concurrent
     multi-agent build contention on the shared `target` directory
     (repeatedly observed "Blocking waiting for file lock on build
     directory" messages throughout), not reproduced in any of the
     subsequent 120 direct-binary runs. Full-suite regression check:
     `cargo test --release --features production` (226 `test result: ok`
     blocks, 0 `FAILED`) and `cargo test --release --features "production
     medium-classes"` (226 `test result: ok` blocks, 0 `FAILED`) both clean.
     `cargo fmt --check` clean on the changed file.
   - **Files changed:** `tests/r14_4_promotion_free_correctness.rs` and this
     index. No `src/` changes (the two counterfactual breaks used for
     non-vacuity verification were both reverted before this commit — `git
     diff` on `src/` is empty). No version bumps.

   - **R29-1 correction (2026-07-29, task #432) — REOPENED then RE-RESOLVED
     with a real root cause.** The R28-2 entry above recorded "1 anomalous
     failure out of ~155 total runs, attributed to this session's heavy
     concurrent multi-agent build contention on the shared `target`
     directory" and marked the item RESOLVED on that attribution. An
     independent readonly review
     (`docs/reviews/2026-07-29-r28-readonly-review.md` §"P0 — the R28-2
     anomalous failure is not explained") flagged that build-lock contention
     explains DELAY, not a COMPLETED assertion failure, and that the original
     root cause was therefore unproven. R29-1 investigated and confirmed the
     review was right to flag it: the anomaly is REAL (reproduced), but its
     root cause is a **test-logic bug (classification (a) from the task's
     taxonomy), NOT an allocator correctness concern, NOT infrastructure**.
     - **Reproduction:** the test binary was built from a PRIVATE isolated
       target dir (`target-r29-1-isolated/`, since cleaned up) so shared
       build-lock contention was structurally eliminated from the loop —
       the same isolation technique R26-1 used. A 2000-run sweep of the
       `production medium-classes` combo (the Large-promotion path,
       `HAS_PROMOTION == true`) using a HYBRID binary that kept the ORIGINAL
       windowed assertion (`released_delta <= reserved_delta`) but added
       failure-path-only diagnostics reproduced **6 failures out of 2000
       (0.30%)**, all with an IDENTICAL trajectory — evidence captured in
       `docs/_raw_r29_1_repro_captured.log` (150 lines, 6 full
       stdout/stderr dumps). The `production medium-classes exact-span-large`
       combo (`HAS_PROMOTION == false`, the medium-ladder path) showed **0
       failures in 600 runs** — the bug is specific to the Large-promotion
       path.
     - **Classification (a) — proven, not inferred.** Every one of the 6
       captured failures shows: (1) the R28-2 per-base leak proof at line 284
       PASSED (it ran before the failing line-319 guard and did not fire) —
       `grown`'s own segment was correctly freed
       (`still_registered=false`, `live_count_before_free=None`,
       `live_count_after_trim_recheck=None`); (2) the GLOBAL cumulative
       invariant held at failure time (`reserved_total=4 >
       released_total=2`) — no double-release. The failing trajectory was
       always: `reserved before=3 after_promote=4 after_free=4 | released
       before=0 after_promote=1 after_free=2` — i.e. the promotion grow
       released `p`'s now-empty OLD segment (reserved during heap/TLS init
       or by the sibling test via the persistent TLS heap binding, BEFORE
       this test's `stats_before` snapshot) INSIDE the window, while only 1
       segment (grown's Large) was reserved INSIDE the window. The windowed
       `released_delta <= reserved_delta` guard's premise ("every in-window
       release has a matching in-window reserve") is INVALID for
       process-wide cumulative counters read over an arbitrary snapshot
       window — a segment reserved before the window can be released inside
       it. This is the SAME mechanism the earlier `TEST_LOCK` fix (item 1
       above) partially addressed (the concurrency race between the two
       test FUNCTIONS) but did NOT fully close: the `TEST_LOCK` serializes
       against the sibling test function's concurrent activity, but NOT
       against segments left in the persistent TLS heap by PRIOR test
       invocations on the same thread, whose later release crosses the
       window. NO `src/` allocator code is implicated — the R28-2 per-base
       proof (the real leak detector) correctly passed in all 6 failures.
     - **Fix applied:** replaced the unsound WINDOWED assertion
       (`released_delta <= reserved_delta` since `stats_before`) with the
       sound GLOBAL cumulative invariant
       (`segments_released_total <= segments_reserved_total`, no windowing)
       — which is window-independent and exactly captures the guard's stated
       intent ("a double-release would indicate corruption": only a genuine
       double-release of the same OS reservation can push global released
       past global reserved). Leak detection was never this counter's job
       (the R28-2 entry's own "Scope" note at lines 121-132 already said so)
       — it is the per-base proof's job, which is reliable and
       segment-specific. The windowed deltas are retained as diagnostic
       CONTEXT only (printed on the failure path, never asserted). Failure-
       path-only diagnostics (zero pass-path cost, so they do not perturb
       the timing of the race they diagnose) were added on every assertion
       path in the test: the trajectory across all three snapshots, plus the
       cfg-gated per-base `still_registered`/`live_count` state, so a future
       CI failure is self-diagnosing from logs alone.
     - **Fix verified:** a 2000-run sweep of the same `production
       medium-classes` combo with the global-invariant fix showed **0
       failures out of 2000** (evidence: `docs/_raw_r29_1_confirm_captured.log`,
       5 lines), against the pre-fix 6/2000. All three CI-relevant feature
       combos compile clean, including the cfg-narrowing-sensitive `hardened
       medium-classes` combo (= `fastbin` + `medium-classes` = `alloc-global
       + alloc-xthread` WITHOUT `alloc-decommit`, where the per-base
       diagnostic block's `#[cfg(all(feature = "alloc-decommit", feature =
       "alloc-xthread"))]` gate correctly compiles out — the same
       R28-2-documented cfg-narrowing gap, re-verified not reintroduced).
     - **Corrected status:** the R28-2 "1 anomalous failure attributed to
       build contention" hypothesis is **REFUTED** — the anomaly is a real
       ~0.3% false-positive rate of an unsound windowed assertion form, now
       fixed. The item is **RESOLVED** on the corrected root cause
       (classification (a), test-logic window-asymmetry bug, fixed and
       verified at 0/2000). This is NOT a still-live allocator correctness
       concern and does NOT block anything.

31. **CI clippy `--all-targets` red on all five rows — pre-existing
   example/test lint+compile errors** — **RESOLVED** by R33-1 (task #506,
   commit `e526517befbf5a0cd0ca1a7ee62f9d84ffe509ee`). Five distinct failures, all pre-existing on `main`
   (four inherited from Round-31 example files, one from Round-32 task
   #502). The brief enumerated only two and prescribed "one line of
   doc-indent + adding the missing `fn main`"; re-running ALL five ci.yml
   clippy rows (as the brief instructed) revealed three further latent
   failures masked by cargo's fail-fast target scheduling — all five were
   necessary for the DONE-WHEN criterion (all five clippy rows green):

   - **E0601** in `examples/r31_10_trim_cost_gate.rs:326` — the example was
     auto-discovered (no `[[example]]` Cargo.toml entry) but gated
     `#![cfg(all(feature = "alloc-global", feature = "alloc-decommit"))]`,
     so under any feature set lacking both, the cfg stripped the entire
     crate body including `fn main`. (The brief's "add the missing
     `fn main`" framing was a misdiagnosis — `fn main` already existed at
     line 313; the root cause is the missing registration.) **Fix:**
     registered it in `Cargo.toml` with
     `required-features = ["alloc-global", "alloc-decommit"]`, mirroring
     its sibling `r31_10_trim_rss_gate` (already correctly registered,
     never failed).
   - **`clippy::doc_lazy_continuation`** in
     `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs:257`
     — a `/// block.` continuation line under a `/// -` list item. **Fix:**
     indented the line 2 spaces (clippy's own suggestion).
   - **E0599** in `examples/r31_8_large_cache_scan_isolation_off.rs:41,43`
     — calls `dbg_large_cache_hits` (`#[cfg(feature = "alloc-decommit")]`,
     `src/alloc_core/alloc_core_large_cache.rs:751`) but its
     `required-features` listed only `["alloc-core"]`. **Fix:** added
     `"alloc-decommit"` to both `r31_8_large_cache_scan_isolation_off` and
     `..._on` (which share the workload via `include!`).
   - **E0432/E0599** in `examples/r31_3_large_cache_extended_narrow_on.rs:39,43`
     — uses `LargeCacheConfig` + `SeferAlloc::with_config` (both
     `alloc-decommit`-gated) but `required-features` listed only
     `["alloc-global"]`. **Fix:** added `"alloc-decommit"`. (The `_off`
     variant uses `SeferAlloc::new()` only and was never affected.)
   - **`clippy::int_plus_one`** in `tests/remote_ring_shadow_head.rs:165`
     (Round-32 task #502, commit `d38bf73`) — `fast_after >= fast_before + 1`
     → `fast_after > fast_before` (semantically identical, clippy's
     suggestion). NOT inherited from Round 31 — the one Round-32-origin
     failure of the five.

   - **Verification:** all five ci.yml clippy rows pass locally with
     `-D warnings` (`cargo clippy --all-targets` for default /
     `--features experimental` / `--all-features` /
     `--features "hardened medium-classes"` / `--features "production"` —
     each verified rc=0); `cargo fmt --all -- --check` clean;
     `cargo test --features production` green. No runtime behavior changed
     (four `Cargo.toml` example registrations + one doc-indent + one
     clippy-suggested test rewrite).
   - **Files changed:** `Cargo.toml` (4 example `required-features`
     registrations), `examples/_shared/r31_3_large_cache_extended_narrow_ab_workload.rs`,
     `tests/remote_ring_shadow_head.rs`, this index entry.
   - **Commit prefix:** `fix(ci)` per the R30-12 taxonomy — no shipping or
     opt-in algorithm code changed, no production default changed; all edits
     are CI-clippy-red fixes (example registrations, a doc-indent, a
     clippy-suggested test rewrite).
   - **Open follow-up kept:** item 11's `npm run check` coverage-gap half
     remains OPEN above (the bugs are fixed but the question of why the
     local gate did not catch them is not).

32. **F10 shadow-head ordering gap — finding F-1**
   (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-1
   [medium]) — **RESOLVED** by R34-6 (task #525). The F10 shadow-head fast
   path in `RemoteFreeRing::full_check`
   (`src/alloc_core/remote_free_ring.rs`) replaced every push's pre-F10
   `head.load(Acquire)` with a `cached_head.load(Relaxed)` on the
   producer's own cache line. The module doc's value-domain proof
   (`cached_head <= head` always, so the fast path can only under-estimate
   occupancy) was correct, but the ordering role the removed load played
   was never addressed: under the abstract memory model, a producer P that
   takes only the fast path carries no happens-before chain to the
   consumer's `slot.store(EMPTY)`, so the consumer's clear and P's
   `slot.store(offset)` into a recycled slot are unordered. NOT a data
   race (both atomic on the same `AtomicU32`) — a potential
   lost-update/liveness defect, confirmed NOT realizable on any hardware
   Rust targets (x86-TSO, ARMv8, RISC-V RVWMO, POWER cumulativity).

   - **Resolution (variant a — promote ordering):** the two `cached_head`
     accesses in `full_check` were promoted from `Relaxed` to
     `Acquire`/`Release`, restoring the exact happens-before edge the
     removed `head.load(Acquire)` supplied, on the same producer-owned
     cache line.
   - **Cost measurement:** byte-for-byte identical assembly on x86-64
     (verified via `objdump` — both `Acquire` load and `Release` store
     compile to the same `mov` as `Relaxed`); wall-clock A/B (5 runs each)
     showed fully overlapping ranges (Relaxed: 5.65–6.10 µs, A/R:
     5.75–6.54 µs; `benches/r34_6_remote_ring_cached_head_ordering_gate.rs`).
   - **Also:** the staleness precondition (~2³² consumer advances) was
     explicitly labeled as an ASSUMPTION (not a theorem) in the module
     doc, per the second independent review's request.
   - **Commit prefix:** `fix(perf)` per the R30-12 taxonomy — shipping
     code changed to close a latent ordering/correctness defect, no
     speedup claimed, no observable behavior change on real hardware.

33. **F-5 release-surviving panic sites vs. "NEVER panics" doc claim**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-5
    [low]) — **RESOLVED** by R34-16 (task #535). The module doc in
    `src/global/sefer_alloc.rs` claimed "Every entry point here returns null
    on failure and NEVER panics," but five release-surviving (not
    `debug_assert!`) invariant checks are reachable from the `GlobalAlloc`
    impl under `production`:
    (1) `alloc_core/alloc_core.rs:2158` `assert!` in
    `realloc_inplace_fast_path_known_base`;
    (2) `alloc_core/alloc_core_large_cache.rs:147` `.expect` in
    `large_cache_slot_take` (base);
    (3) `:160` `.expect` (extension);
    (4) `:166` `unreachable!` (take, extension disabled);
    (5) `:321` `unreachable!` (set, extension disabled).

    - **Resolution (variant b — doc accuracy, no behavior change):** the
      audit could not construct a reachable violation of any of the five
      ("cannot happen" invariant checks), and the codebase's own
      `AllocCore::reclaim_offset` already documents the tradeoff between a
      graceful no-op and a defence-in-depth abort. The five were left as
      release panics deliberately: each guards allocator metadata whose
      silent corruption would be strictly worse than an immediate abort, so
      a future bug that broke one trips loudly at the point of corruption
      instead of continuing with inconsistent state. Softening variants (a)
      were rejected per-site — sites 4 cannot be softened at all
      (`large_cache_slot_take` returns a value `CachedLarge`, no no-op
      return possible; it is `#[cfg(not(large-cache-extended))]`-unreachable
      by construction), and sites 2–3 are take-side `.expect`s whose callers
      prove occupancy via an ARRAY read (`large_cache_slot_get` /
      `oldest_occupied_slot`), NOT the R32-12 bitmask, so a bitmask/array
      desync cannot reach them — softening them would only mask the very
      desync they guard against.
    - **Doc fix:** `sefer_alloc.rs`'s "No-panic" section rewritten to (1)
      keep the accurate failure-path bullets, (2) enumerate the five
      tripwires as "abort by design" defence-in-depth, and (3) state
      explicitly that a panic escaping `GlobalAlloc` aborts via
      `#[rustc_nounwind]` (not UB), independent of any downstream
      `panic = "abort"` setting.
    - **Pinning test:** `tests/no_panic_doc_accuracy.rs` pins the five by
      their distinctive panic-message strings (exactly once each) AND pins
      the doc's qualifying language (`rustc_nounwind`, `invariant tripwire`,
      absence of the old overclaim).
    - **Commit prefix:** `docs(global)` per the R30-12 taxonomy — module-doc
      accuracy fix, no shipping code changed (the only non-doc additions are
      the regression test and this index entry).

34. **F-6 `HeapCore` by-value construction stack-pressure pin**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-6
    [low]) — **RESOLVED** by R34-18 (task #537). `HeapCore` is constructed BY
    VALUE on the frame that triggers a thread's FIRST allocation
    (`HeapRegistry::claim`'s `HeapCore::new(idx) → write(hc)` in both `claim`
    and `claim_with_config`, and the process-global fallback's
    `MaybeUninit<HeapCore>` path in `global/fallback.rs`). Rust does not
    guarantee return-value/move elision, so a debug build (or any backend that
    materialises the temporary) can place one ~7 KiB copy on a small-stack
    thread's first-allocation frame — a realistic stack-overflow risk for
    embedded-class 16–64 KiB stacks. The audit's ~7 KiB figure was INFERRED
    from in-tree `-Zprint-type-sizes` field-offset notes, never measured
    (`size_of::<HeapCore>()` existed nowhere in `src/` or `tests/`).

    - **Resolution:** `size_of::<HeapCore>()` measured directly via a
      compile-error array-length probe under `--features production` (the same
      technique as the `SegmentHeader == 144` pin) = **7576 bytes** (breakdown:
      `core: AllocCore` = 864 B, `tcache: Tcache` = 6664 B — the dominant
      per-class magazine cache — plus `id`/handles ≈ 48 B). A compile-time
      `const _: () = assert!(size_of::<HeapCore>() <= 8192)` pin added in
      `src/registry/heap_core.rs` right after the struct definition, mirroring
      the established `SegmentHeader` pin pattern. Budget = 8192 (8 KiB, half
      of a 16 KiB embedded stack) leaves ~8 % headroom (616 B): minor field
      additions don't trip it, material bloat (a new array/sub-struct, or
      `Tcache` growing another class) fails the build and forces a deliberate
      budget bump. The pin is an unconditional `<=` (not exact `==`): it must
      hold across every feature composition (the struct has `#[cfg]`-gated
      fields; `production` is the maximum at 7576 B, every smaller composition
      is strictly below). A runtime `#[test]`
      (`tests/r34_18_heap_core_stack_pressure_pin.rs`) mirrors the pin and adds
      a non-vacuous lower bound (suspicious-shrink guard). This is the ONLY
      unbounded-growth stack-pressure surface in the tree (no recursion, no
      recursive drop glue, no larger stack buffer than `emptied_bases:
      [*mut u8; 64]` = 512 B, cold path), so this single pin guards the whole
      category.
    - **Point 2 (in-place `HeapCore::new_in_place` initializer) — SKIPPED:**
      evaluated and rejected as not-cheap. The dominant 6664 B component is
      `Tcache::new()`, which itself returns by value; eliminating the 7576 B
      temporary requires cascading in-place init into `Tcache` too (a
      multi-struct refactor across the `registry` + `tcache` modules), changes
      the fallible-`Option` error-handling shape at all three call sites, and
      touches `UnsafeCell` write safety invariants. The compile-time pin is
      the auditor's primary deliverable (F-6's own "Suggested direction") and
      closes the category; the in-place rewrite is not warranted for a [low]
      finding whose risk is already bounded by the pin.
    - **Commit prefix:** `fix(perf)` per the R30-12 taxonomy — structural
      layout pin, no runtime behavior change, no speedup claimed.

15. **G2 — no loom model exercises the F10 fast path over a recycled slot**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding G2
    [medium]) — **RESOLVED** by R34-19 (task #538). The two existing shadow
    loom models in `tests/loom_remote_ring.rs` neither reached the F-1
    interleaving: `RingModelShadow` (CAP=4) joined producers before draining
    (no wrap → no slot reuse); `RingModelShadow1` (CAP=1) forced the slow
    path exclusively. The one thing F10 actually changed — a producer proving
    room from the shadow alone and reserving a slot the consumer just cleared
    — was modelled by nothing.

    - **Resolution:** added `RingModelShadow2` (CAP=2, post-R34-6
      `Acquire`/`Release` cached_head orderings) and
      `shadow_fast_path_recycled_slot_concurrent_drain_never_loses_or_duplicates`:
      one producer pushes 4 offsets (bounded retry) + one consumer drains
      concurrently, `preemption_bound = 2`. CAP=2 + 4 pushes forces slot
      reuse; the interleaving where the consumer drains between pushes 2
      and 3 makes the producer's push 3 take the slow path (refreshing
      `cached_head`) and push 4 take the FAST PATH into the just-drained
      slot — the exact F-1 shape, reached in 2 preemptions.
    - **Honest limitation (stated in the test's doc comment):** this model
      is a **regression-pin, not an ordering proof**. Loom's store history
      is append-only per atomic, so it cannot surface F-1's modification-
      order freedom; even if the ordering bug were present, loom would
      very likely not detect it. What the model pins: value-domain
      invariants (exactly-once delivery, no overflow into occupied slot,
      no deadlock/panic) hold under slot reuse + concurrent drain. The
      ordering question itself was resolved in R34-6 (item 32 above,
      renumbered from a collision by task #623/M2).
    - **Counterfactual verification (non-vacuity):** replacing
      `full_check`'s body with `Ok(())` (always admit) causes the test to
      FAIL — in the zero-preemption interleaving where all 4 pushes
      execute before any drain, pushes 3+4 overwrite pushes 1+2 in the
      same 2 slots, and offset 10 is reclaimed 0 times despite landing
      (`assertion left == right failed: offset 10 landed but was
      reclaimed 0 times`). Without this check the model could be vacuous
      (R33-3 / task #508 lesson).
    - **Commit prefix:** `test(loom)` — explicitly OUTSIDE the R30-12
      five-slot taxonomy, which governs runtime/opt-in/measurement/docs
      code changes; this is pure verification-coverage addition with zero
      shipping code changed.

35. **F-2 provenance-asymmetry hypothesis — RESOLVED-NEGATIVE**
    (`docs/reviews/2026-08-04-release-stabilization-audit.md`, finding F-2
    [low]; open item 15) — **RESOLVED** by R34-5 (task #524), following the
    item's own decision rule. The item's blocking question was: does the
    concurrent multi-producer SMALL-block `RemoteFreeRing` push/drain path
    (`Node::atomic_u32_at`, backing `head`/`tail`/`cached_head`/`slots`) flag
    under Stacked Borrows the way `Node::atomic_ptr_ref` was fixed for in
    task #142 — the one piece of evidence the repo's tooling could not
    supply until a concurrent small-ring miri test existed (audit G1).

    - **Trigger test added:** R34-5 (task #524, commit `fd54ddc`, plus
      `b47a261`/`91ff1dd` fixing two local miri/tsan wrapper scripts that had
      silently omitted the `internals` feature) added
      `tests/regression_xthread_small_ring_miri.rs`
      (`xthread_small_ring_two_producers_push_owner_drains`): 2 spawned
      producer threads concurrently free small blocks from the SAME segment
      (both CAS-reserving into the same per-segment `RemoteFreeRing`) while
      the owner concurrently allocates, then force-drains via
      `dbg_drain_all_rings` — the exact "≥2 concurrent remote small-block
      ring pushes" shape audit finding G1 said was missing.
    - **Wired into `miri-plain` under plain (Stacked Borrows) miri, not Tree
      Borrows — confirmed by reading, not assumed:** `grep -n MIRIFLAGS
      .github/workflows/ci.yml` shows the `miri-plain` job's `env` block
      (`.github/workflows/ci.yml:860`) sets
      `MIRIFLAGS: "-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5"` —
      no `-Zmiri-tree-borrows` anywhere in that value or job, so this job
      runs under miri's default provenance/aliasing model, Stacked Borrows,
      exactly the model the item's decision rule names as its trigger
      condition. The job's `run:` step (`:900-903`) lists
      `--test regression_xthread_small_ring_miri` alongside the two
      pre-existing large-block plain-miri tests, confirmed via `git show
      fd54ddc --stat` (touches `.github/workflows/ci.yml`,
      `scripts/miri.mjs`, `docs/ARCHITECTURE.md`, and the new test file).
    - **Trigger condition checked independently, not taken on the commit
      message's word:** re-ran the exact test locally —
      `MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5" cargo
      +nightly miri test --features "alloc-global alloc-xthread internals"
      --test regression_xthread_small_ring_miri` — result: `test result: ok.
      1 passed; 0 failed` (~67s locally), with only the expected/documented
      integer-to-pointer-cast warnings (the same exposed-provenance
      re-derivation warnings the other `miri-plain` tests already produce,
      not errors). This matches task #524's own commit message verification
      log ("1 passed (~49s), only the expected integer-to-pointer-cast
      warnings") and independently confirms it under a fresh local run
      rather than trusting the commit's self-report.
    - **Verdict:** the trigger did NOT fire — the concurrent small-block ring
      test does not flag under Stacked Borrows. Per the item's own decision
      rule ("only if that test flags under Stacked Borrows should the
      `atomic_ptr_ref` treatment be applied to `atomic_u32_at`"), the
      `expose_provenance`/`with_exposed_provenance_mut` treatment is **NOT
      required** for `atomic_u32_at`/`atomic_u64_at`/`atomic_u8_at`. The
      original provenance-asymmetry hypothesis (task-#142's fix applied to
      `atomic_ptr_ref` only, not the ring's other atomic accessors) is
      closed **resolved-negative**: this repo's tooling can now answer the
      question audit finding G1 said was unanswerable, and the answer is
      "no asymmetry-driven miri failure is reachable" — not "asymmetry
      confirmed harmless by inspection alone," a materially stronger claim
      than the item started with.
    - **Scope of what this resolution does and does not prove:** miri's
      Stacked Borrows model not flagging a 2-producer/1-consumer
      interleaving over ~49-67s of runtime is evidence the asymmetry is not
      an easily-triggered UB source under that model and that workload
      shape — it is not an exhaustive proof over all interleavings/thread
      counts (miri explores the interleavings its scheduler happens to
      generate, not all of them) and says nothing about Tree Borrows (which
      the item's own text already argued is structurally immune here via
      `Cell` permission on raw-pointer-derived `&AtomicU32`, a separate,
      independent argument this resolution does not depend on).
    - **Files changed (doc-only):** this index entry (the corresponding open
      item, now item 35 above after task #623/M2's collision renumbering,
      was replaced with a one-line "Recently resolved" pointer per
      CLAUDE.md's R34-24 current-state-card structural rule; this closure
      narrative added here). No source, test, or CI file changed by this
      task — #524 already landed the test and CI wiring in a prior commit.
    - **Commit prefix:** `docs` — pure documentation update (closing a
      stale open-item card to reflect an already-landed, already-verified
      resolution); no shipping or opt-in code changed, no measurement run
      newly performed that a report's verdict rests on (the miri re-run
      here reproduces #524's own already-published result, it does not
      establish a new one).

36. **H8 — `dbb4016`'s `fix(perf):` prefix considered for a reword to
    `feat(api):`, DECIDED against a rebase, prefix left as-is** (task #578,
    `docs/reviews/2026-08-05-sol-remediation-readonly-review.md` finding H8)
    — **RESOLVED, no code change.** Sol-F1's commit (`9296adb`, post-G1-
    rebase SHA `dbb4016`, "AllocCore::dbg_* inherent methods now genuinely
    require `internals`") used `fix(perf):`. The review flagged this as
    inapt for a pure visibility/cfg-gating change (no algorithm changed,
    only which callers can reach existing code) and pointed to the
    identical-class predecessor `27879af` (R34-3, gating the module PATHS
    behind `internals`), which used `feat(api):` — arguably the closer
    match, since CLAUDE.md's R30-12 taxonomy has no dedicated slot for
    "API-surface visibility change."
    - **Decision:** left `dbb4016` as-is — an accepted historical
      imprecision, not reworded. Two considered options were (a) a small
      rebase to reword just `dbb4016`, or (b) accept the existing prefix
      and use correct judgment for any NEW commits in the same class. (b)
      was chosen per the task's own explicit default guidance ("default to
      (b) unless a rebase is already happening anyway for some other
      reason in this batch") — no other rebase was in flight this round,
      and this is the exact non-retroactive posture CLAUDE.md's own R30-12
      section already states for this rule ("no historical commit message
      is retagged or amended by this rule; it governs new commits going
      forward only" — the same posture the raw-log-truncation and
      immutable-source-identity rules elsewhere in CLAUDE.md also take).
      H2 (task #572), the directly-analogous follow-up commit extending
      this exact same gating work to 6 more files, independently used
      `fix(perf):` as well (`25d6ac4d23b4859b726724424e5912dc54fe0bf0`) and
      passed `verify-commit-prefixes.mjs` — establishing `fix(perf):` as
      the now-repeated, lint-accepted precedent for "narrow an existing
      diagnostic hook's reachability without changing its behavior,"
      rather than treating `dbb4016` as an isolated one-off mistake to
      correct. A rebase deep enough to reword `dbb4016` would also need to
      touch every commit stacked on top of it since (including H2, H3, H4,
      H5, H7 above) — disproportionate risk for a P4 wording nit, per the
      same cost/benefit reasoning G1's rebase (task #555) already weighed
      once this session for a higher-severity (P2) case.
    - **Files changed:** none (this index entry only) — a documented
      decision, not a rebase or a reword.

37. **Flaky test — `repeated_same_segment_frees_are_observed_as_tier1_hits`**
    (`tests/segment_table_contains_base_tier1_counters.rs`) — **RESOLVED**
    by wave 3's own `npm run check --all-features` gate run (2026-08-05,
    same session as H1-H8, tasks #571-578).

    - **Root cause, confirmed:** `CONTAINS_BASE_TIER1_HITS`/
      `CONTAINS_BASE_TIER1_MISSES` (`src/alloc_core/alloc_core.rs`) are
      process-wide `static AtomicU64`s. Both `#[test]` functions in this
      file read them via a before/after delta; `cargo test` runs the two
      tests in this file in parallel by default, so the OTHER test's
      `contains_base`/`dbg_hash_contains_only` traffic could land inside
      one test's own delta window — exactly the SAME failure class as item
      1 above (`canary_survives_promotion_and_free_leaves_no_leak`), a
      different process-wide counter pair, same root cause. Observed
      failure: `hits_delta=31 misses_delta=2` against an expected `N=32`
      for `repeated_same_segment_frees_are_observed_as_tier1_hits`.
      Confirmed as a parallelism artifact, not a real regression: passed
      clean under `cargo test --test
      segment_table_contains_base_tier1_counters --all-features --
      --test-threads=1`; confirmed the file predates this session
      (`git log -- tests/segment_table_contains_base_tier1_counters.rs`
      last touched by Round 34's `7aeee2d`, an unrelated rustfmt-drift
      commit) — this is a pre-existing flake this wave's own full-matrix
      run happened to surface, not something wave 1/2/3's own changes
      introduced.
    - **Fix:** added the SAME established `static TEST_LOCK: Mutex<()>` +
      per-test `let _guard = TEST_LOCK.lock().unwrap();` pattern item 1
      above already used (also matching
      `tests/directory_authoritative_miss.rs`,
      `tests/alloc_zeroed_fresh_large_skip.rs`,
      `tests/r13_3_magazine_virgin_hit_skips_zero.rs`,
      `tests/r21_2_opt_h_stage1_precondition_probe.rs`). No assertion logic
      changed.
    - **Verification:** 5 full `cargo test --test
      segment_table_contains_base_tier1_counters --all-features` reruns
      (default multi-threaded scheduling) after the fix — all clean, 0
      failures. `cargo fmt --all -- --check` clean.
    - **Files changed:** `tests/segment_table_contains_base_tier1_counters.rs`
      (serialization only); this index entry.

38. **Flaky test — `ac1_trim_empties_pool_and_evicts_large_cache`**
    (`tests/r31_10_trim_current_thread_api.rs`) — **RESOLVED** by wave 4's
    own post-landing `npm run check --all-features` gate run (2026-08-05,
    same session as I1-I10, tasks #579-588; found in a background rerun
    launched after `782b92e` landed, task #589).

    - **Root cause, confirmed:** `segments_released_total`
      (`SeferAlloc::stats()`) is a process-wide counter shared across every
      `SeferAlloc`/thread in the process. `cargo test` runs the six
      `#[test]` functions in this file in parallel by default; every one of
      them calls `trim_current_thread()` at least once (which can release a
      cached span and bump this counter), while
      `ac1_trim_empties_pool_and_evicts_large_cache` and
      `ac3_trim_does_not_affect_other_thread_heap` each read a before/after
      delta on it — the SAME failure class as items 1 and 25 above, a
      different process-wide counter, same root cause. Observed failure:
      `released_before=1, released_after_cache=2` — a sibling test's
      `trim_current_thread()` call landed in the narrow window between
      `ac1`'s two counter reads. Confirmed the file predates this session
      (`git log -- tests/r31_10_trim_current_thread_api.rs` last touched by
      Round 34's `7aeee2d`, an unrelated rustfmt-drift commit; the test
      itself dates to Round 31, task #474) — a pre-existing flake this
      wave's own full-matrix rerun happened to surface, not a regression
      from I1/F1 (the `HeapCore` stack-pressure budget change) or I5/F5
      (gating `SeferAlloc::dbg_trim_current_thread`), neither of which
      touches `segments_released_total` accounting or this test's own
      logic.
    - **Fix:** added the SAME established `static TEST_LOCK: Mutex<()>` +
      per-test `let _guard = TEST_LOCK.lock().unwrap();` pattern items 1
      and 25 above already used. Applied to ALL SIX tests in the file, not
      just the two that read the delta — every other test's
      `trim_current_thread()` call is itself a source of interference for
      those two. No assertion logic changed.
    - **Verification:** 5 full `cargo test --all-features --test
      r31_10_trim_current_thread_api` reruns (default multi-threaded
      scheduling) after the fix — all clean, 0 failures. Also verified
      clean under `cargo test --features "production internals" --test
      r31_10_trim_current_thread_api`. `cargo fmt --check` clean on the
      changed file.
    - **Files changed:** `tests/r31_10_trim_current_thread_api.rs`
      (serialization only); this index entry.

39. **Flaky test — `oom_injection_flag_is_clean_after_test`**
    (`tests/regression_free_path_chunk_oom_graceful.rs`) — **RESOLVED**
    by the first full remote CI run over the pushed backlog (2026-08-05,
    CI run `31045983765` on landing SHA `42d4206`, task #621, found during
    the map-verification pass of this session's release-readiness work).

    - **Root cause, confirmed:** `DBG_INJECT_CHUNK_OOM` is a process-wide
      `internals`-gated `AtomicBool`. This file has two `#[test]` fns whose
      correctness relies on sequential execution — the module doc says so
      explicitly (`oom_injection_flag_is_clean_after_test` is designed to
      run AFTER `chunk_oom_on_free_path_returns_gracefully_not_abort`,
      verifying its `OomInjectionGuard` cleared the flag on drop) — but
      `cargo test` runs the two in parallel by default with nothing
      serializing them. A race window between
      `dbg_set_inject_chunk_oom(true)` in the main test and the guard's
      `Drop` clearing it back to `false` let the second test observe the
      flag stuck `true`. Same failure class as items 1/25/26 above
      (process-wide diagnostic flag/counter, multiple tests in one file,
      no serialization), a different flag, third recurrence this session.
    - **Fix:** added the SAME established `static TEST_LOCK: Mutex<()>` +
      per-test `let _lock_guard = TEST_LOCK.lock().unwrap();` pattern
      items 1/25/26 already used (renamed to `_lock_guard` in this file
      specifically to avoid shadowing the pre-existing `let _guard =
      OomInjectionGuard;` binding in the main test — both guards are held
      simultaneously for correctness, shadowing would only have been
      confusing, not incorrect, since Rust drops shadowed bindings at
      scope end in reverse declaration order). No assertion logic changed.
    - **Verification:** 5 full `cargo test --features "production
      alloc-stats bench-internals internals" --test
      regression_free_path_chunk_oom_graceful` reruns (default
      multi-threaded scheduling) after the fix — all clean, 0 failures.
      `cargo fmt --check` clean.
    - **Files changed:** `tests/regression_free_path_chunk_oom_graceful.rs`
      (serialization only); this index entry.

40. **CI-coverage gap — `cargo test -p racy-ptr-cell` ran in ZERO CI
    configurations** (`.github/workflows/ci.yml`'s `test-workspace` job)
    — **FLAGGED AND RESOLVED IN THE SAME ROUND** (2026-08-09, task #774
    filing this entry per this file's own "file in the same commit that
    flags it" rule; found by the racy-ptr-cell round-closing review,
    `docs/reviews/2026-08-09-racy-ptr-cell-round-closing-review.md` §F1;
    closed by task #773 immediately prior in the same round, commit
    `a5e8e42`).

    - **Root cause, confirmed:** the crate's only two prior CI
      invocations were `cargo build -p racy-ptr-cell --no-default-features
      --target thumbv7em-none-eabi` (compiles no test target at all) and
      `cargo test --release -p racy-ptr-cell --test loom_racy_ptr_cell`
      under `RUSTFLAGS="--cfg loom"` (excludes `tests/cell_unit.rs` twice
      over — by `--test` target selection and by the file's own
      `#![cfg(not(loom))]` gate). All 7 of `cell_unit.rs`'s tests,
      including 4 added by the racy-ptr-cell `rust-intel` remediation
      round (tasks #700/#706-710) — `panicking_init_rolls_back_and_subsequent_call_succeeds`,
      `init_returning_the_sentinel_address_panics`,
      `align_of_one_payload_panics_at_construction`,
      `dbg_rollback_reenterable_happy_path_and_not_applicable_arm` — had
      never executed in CI. Same gap class as the identical
      `tagged-index-stack` gap task #639/#772 (F4/F5) closed one round
      earlier.
    - **Fix:** added `cargo test -p racy-ptr-cell --no-fail-fast` and
      `cargo test -p racy-ptr-cell --release --no-fail-fast` to
      `test-workspace`, next to the existing bare-metal build. The
      `--release` step specifically matters: `init_returning_the_sentinel_address_panics`
      regression-guards an `assert!` promoted from `debug_assert!` (task
      #707) — a debug-only run would stay green even if that promotion
      were silently reverted.
    - **Verification:** counterfactual — reverted the `assert!` to
      `debug_assert!` locally, confirmed the new debug step stays green
      (expected: `debug_assert!` still fires in debug) and the new
      `--release` step fails with the expected panic-message mismatch;
      reverted cleanly (`git diff` empty on `src/lib.rs`).
    - **Related, also resolved same round (not a separate item):** the
      round-closing review's F5 also flagged `CHANGELOG.md`'s "`#709` is
      **Not miri-verified**" caveat as unfiled; task #774 closed the
      underlying gap for `tests/cell_unit.rs` directly (applied `#709`'s
      own `expose_provenance`/`with_exposed_provenance_mut` fix to an
      identical provenance-losing pattern the same round's `#706`
      introduced there — see finding F2 in the same review) rather than
      filing it as a standing open item. `#709`'s own fix, in
      `tests/loom_racy_ptr_cell.rs`, remains structurally unverifiable by
      miri (loom's green-thread scheduling simulation is incompatible with
      miri's execution model) — this is an intrinsic tooling limitation,
      not an open action item.
    - **Files changed:** `.github/workflows/ci.yml` (task #773); this
      index entry.

41. **SyncRegion one-shot convenience methods missing reentrancy cross-references**
   (`crates/sefer-region/src/sync_region.rs`, methods `clear` and `get_cloned`) — **RESOLVED**
   by the release-prep review's finding F5 closure (2026-08-09). The round-2 closing review
   (`docs/reviews/2026-08-08-sefer-region-round2-closing-review.md`, finding F) flagged that
   of the seven one-shot convenience methods (`insert`, `remove`, `contains`, `len`,
   `is_empty`, `clear`, `get_cloned`), only `remove` explicitly cross-references the type-level
   `## Reentrancy` section. This is a documentation gap: `clear` runs every `T::Drop` under
   the write lock, and `get_cloned` runs `T::clone` under the read lock — the two methods that
   actually execute user code under the lock — yet neither points to the deadlock hazard section.

   - **Root cause, confirmed:** the type-level `## Reentrancy` section at
     `crates/sefer-region/src/sync_region.rs:45-58` does document the hazard thoroughly
     (it explicitly names `clear` and `get_cloned` on lines 47-48), but the per-method
     rustdoc pages for those two methods have no inline reference to it. A reader who
     arrives at `SyncRegion::clear`'s or `SyncRegion::get_cloned`'s rustdoc via a search
     engine sees no deadlock warning.
   - **Fix:** added `see the [reentrancy section](Self#reentrancy)` links to the doc
     comments for both `SyncRegion::clear` and `SyncRegion::get_cloned`, mirroring the
     existing pattern in `SyncRegion::remove`'s own doc.
   - **Verification:** `cargo doc -p sefer-region --all-features --no-deps` generates
     without broken-intra-doc-link warnings; the new links resolve correctly on the
     rendered docs.
   - **Files changed:** `crates/sefer-region/src/sync_region.rs`, `docs/CORRECTNESS_OPEN_ITEMS.md`.
    only; the BSD half of item 43 stays OPEN in the main index above — no
    BSD CI runner exists for this crate).

    - **Confirmation:** commit `1dbd6b4` was pushed and CI run `31692217669`
      (job `94421845398`, `test macos (production)`, image
      `macos-26-arm64`) ran green, executing
      `apple_silicon_page_size_is_16_kib` (`crates/aligned-vmem/tests/smoke.rs`) —
      `ok`. `page_size()` returned exactly `16384` on real aarch64 Darwin
      hardware, confirming the `_SC_PAGESIZE = 29` cfg-table entry
      (`crates/aligned-vmem/src/lib.rs`, task #714) is the correct constant for the
      macOS family, not merely a value that compiles.
    - **Task/round:** task #888 (round 7, finding T1 of
      `docs/reviews/2026-08-13-aligned-vmem-round7-review.md`), following
      the exact action item's own "Next trigger" text (filed round-closing
      review, task #776/F13): "if `apple_silicon_page_size_is_16_kib`
      passes, move the macOS half of this item to 'Recently resolved' with
      the run's citation."
    - **Remaining scope:** FreeBSD, NetBSD/OpenBSD, and DragonFly remain
      unverified on real hardware — tracked as the BSD half of item 43 in
      the main index above, unchanged by this closure.

44. **`aligned-vmem`'s hand-written `mmap` FFI declaration has an ABI shape mismatch risk on 32-bit Unix targets — the `offset` parameter was hardcoded as `i64`, assuming a 64-bit POSIX `off_t`, which is not guaranteed on 32-bit Unix platforms (e.g. glibc i686 and traditional 32-bit ARM default to a 32-bit off_t without `_FILE_OFFSET_BITS=64`).** — **CLOSED** (task #914, correcting H2C1 docs half of `docs/reviews/2026-08-13-aligned-vmem-round10-closing-review.md`).

    The independent review (task #911, finding M2) correctly identified this as a calling-convention/shape compatibility problem, not just a value-range issue — a wrong-width trailing parameter can misalign subsequent stack/register argument-passing on some 32-bit calling conventions (e.g. ARM EABI requires 8-byte alignment for 64-bit parameters, which can insert padding a 32-bit-off_t callee doesn't expect, causing it to read padding bytes as the offset value — garbage, not the intended zero).

    Task #911's initial fix used a `compile_error!` that rejected ALL `unix + 32-bit-pointer-width` combinations. The round-10 closing review (finding H2C1) identified this as a publish-blocking regression: it broke Tier 1 `i686-unknown-linux-gnu` and Tier 2 `armv7-unknown-linux-gnueabihf`, both of which have a 32-bit off_t and were previously supported. Task #914 corrected this before 0.2.0 shipped.

    - **Final fix (task #914):** replaced the `compile_error!` with a per-target `OffT` type alias that correctly types the `mmap` FFI's `offset` parameter per-target:
      - `OffT = i32` for 32-bit Linux/Android (matching their actual default 32-bit off_t width)
      - `OffT = i64` for everything else, including 32-bit BSD/Darwin (which have a 64-bit off_t natively)

      The `mmap` extern's `offset` parameter is now typed as `OffT` instead of `i64`. This allows `i686-unknown-linux-gnu` and `armv7-unknown-linux-gnueabihf` to build correctly again with the CORRECT ABI, rather than being rejected outright. The original `compile_error!` (task #911) was removed entirely, not narrowed.
    - **Evidence:** task #914 commit, per-target `OffT` type alias definitions and the `mmap`/`munmap`/`madvise`/`sysconf` extern block in `crates/aligned-vmem/src/lib.rs`, plus verification on current targets. Historical note: the original compile_error!-based fix (task #911) was caught as a publish-blocking regression by the round-10 closing review (H2C1) and corrected in the same round (task #914) before 0.2.0 shipped.

45. **`aligned-vmem` Linux HugeTLB path leaks entire pinned huge-page mapping when system's default huge-page size is not 2 MiB.** — **CLOSED** (task #909, finding H1 of `docs/reviews/2026-08-13-aligned-vmem-independent-review.md`).

    Before this fix, `libc_mmap` (`crates/aligned-vmem/src/lib.rs`, `#[cfg(all(unix, not(miri)))]`) requested huge pages via plain `MAP_HUGETLB` with NO size-encoding flag (no `MAP_HUGE_2MB`/`MAP_HUGE_1GB` etc.). On Linux, that means the kernel uses the SYSTEM'S CONFIGURED DEFAULT huge-page size — which is set via the `default_hugepagesz=` kernel boot parameter and can be 1 GiB on HPC/database-tuned hosts, NOT universally 2 MiB. But `LINUX_HUGE_PAGE_SIZE` was hard-coded to 2 MiB, and `unix_reserve`'s guard validated callers' `size`/`align` against that 2 MiB constant before allowing a huge-page reservation attempt. `try_reserve_aligned_exact` then returned `reservation_len = size` (the caller's REQUESTED size) as if that's the real OS reservation length — but if the kernel actually used a 1 GiB huge page (because that's the configured default), the true mapping is 1 GiB, not 2 MiB. Later, `release_reservation` called `munmap(reservation, reservation_len)` using that wrong, too-small length. Linux's HugeTLB `munmap` requires the length argument to be a multiple of the ACTUAL underlying huge-page size the mapping used. A 2 MiB length is not a multiple of 1 GiB, so this munmap call failed with EINVAL — and `libc_munmap` silently discards the return value by design. Result: the entire 1 GiB virtual-memory mapping AND its pinned physical huge page leaked permanently and were never released. Repeating this exhausts the system's HugeTLB pool. This finding corrects/falsifies the prior "durable premise" established by task #714 and reconfirmed by later reviews (`docs/reviews/2026-08-07-aligned-vmem-rust-intel-audit.md` and `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md`) that "the default is always 2 MiB on mainstream x86_64/aarch64 Linux" — that premise was wrong, because `default_hugepagesz=` is independently configurable per-host regardless of architecture.

    - **Fix:** added `MAP_HUGE_2MB` constant (`21 << 26 = 0x54000000`, taken from Linux kernel `include/uapi/linux/mman.h`), OR'd it into the `mmap` flags alongside `MAP_HUGETLB`, and updated `LINUX_HUGE_PAGE_SIZE`'s doc to state that the crate NOW explicitly pins the huge-page request to 2 MiB rather than relying on — and being wrong about — the system default being "always 2 MiB." This makes the fix fail-closed: if 2 MiB huge pages specifically aren't configured/available on the host, the mmap call fails cleanly (returns null → the crate's normal OOM/error path), rather than silently succeeding with a mismatched size and leaking on munmap.
    - **Trade-off (functional narrowing):** before this fix, `reserve_aligned_huge(1 GiB, 1 GiB)` on a `default_hugepagesz=1G` host with a provisioned 1 GiB pool worked correctly — the crate got a genuine 1 GiB huge page (`is_huge() == true`), no leak. With the explicit `MAP_HUGE_2MB` request, the crate can no longer obtain huge pages of any size OTHER than 2 MiB, on any host, ever. This is a fail-closed functional narrowing (leak prevention wins, but the general "any huge-page size" case is gone unless a future round queries `/proc/meminfo`'s `Hugepagesize:` or records the kernel-rounded length instead of assuming 2 MiB).
    - **Kernel-version caveat:** the `MAP_HUGE_*` size encoding (`MAP_HUGE_SHIFT`/`MAP_HUGE_2MB` etc.) was introduced in Linux 3.8 (2013); on an older kernel the bits above `MAP_HUGE_SHIFT` are not interpreted by the `MAP_HUGETLB` path, silently reverting to "use the system default huge-page size" — i.e. H1's exact bug returns, with no diagnostic, on a pre-3.8 kernel. Practically irrelevant for a 2026 crate, but recorded for completeness.
    - **Evidence:** task #909 (this task), finding H1 of `docs/reviews/2026-08-13-aligned-vmem-independent-review.md`. Historical context: the falsified "default is always 2 MiB" premise was established by task #714 and reconfirmed by `docs/reviews/2026-08-07-aligned-vmem-rust-intel-audit.md` and `docs/reviews/2026-08-09-aligned-vmem-round-closing-review.md` — this item records the correction.

56. **[T, LOW] `scripts/vmem-doc-drift-guard.mjs` false-positives on `from_raw_parts`'s "insufficient whenever the reservation was over-reserved" sentence** — **CLOSED** (2026-08-16, personal follow-up during round-3 close-out, per this item's own "Next trigger").

    The guard's `SCOPE` regex used `\bwhen\b`, which requires a word boundary immediately after "when" — so it does not match "whenever" (no boundary exists between "n" and "e" mid-word). `from_raw_parts`'s rustdoc uses "insufficient whenever the reservation was over-reserved", a correctly-conditional sentence in plain English, but the literal regex missed it and the guard convicted it as an unconditional drift.

    - **Fix:** widened the regex from `\bwhen\b` to `\bwhen(ever)?\b` in `scripts/vmem-doc-drift-guard.mjs`'s `SCOPE` alternation — exactly the one-word scope-list addition this item's own "Next trigger" specified, no `lib.rs` prose touched.
    - **Verification:** `node scripts/vmem-doc-drift-guard.mjs` now exits 0 (`OK: no unconditional over-reserve/trim statements found`); `HARD_FAIL` (`unconditional`) is untouched, so a genuinely unconditional sentence containing "whenever" would still be convicted — the widening only affects the SCOPE alternative, not the hard-fail path. `npm run check`'s `vmem-doc-drift-guard` step, previously the sole failing step, now passes.
    - **Note for whoever re-cites this item:** the two sentences' line numbers had already drifted from this item's own citation (`lib.rs:1017,1042` at filing time, `:1032,:1057` after round 3's doc edits moved them) before this closure — recorded here as a live instance of the exact staleness class this campaign's own conventions exist to catch, caught only because the guard itself re-ran and re-reported current line numbers rather than the citation being trusted at face value.
    - **Evidence:** `scripts/vmem-doc-drift-guard.mjs`'s `SCOPE` regex (the one-line diff); `crates/aligned-vmem/src/lib.rs` (the two `from_raw_parts` sentences, unchanged); `docs/CORRECTNESS_OPEN_ITEMS.md` (this closure).

42a. **`aligned-vmem`'s `mock` Cargo-feature-unification hazard (item 42's aligned-vmem half)** — **CLOSED** (2026-08-16, task #962, per the maintainer decision recorded in this session: "делаем 2" — convert, do not just document the risk).

    Cargo unifies features across a build's WHOLE dependency graph, not per edge: `mock` REPLACES the real syscall backend with a thread-local recording stub, so if ANY crate anywhere downstream of a build (including a sibling workspace member's own `[dev-dependencies]`) enabled `aligned-vmem/mock`, every OTHER consumer in that SAME build would silently get the mock backend too — no compile error, no warning. First flagged task #715 (rust-intel audit MEDIUM §C10), deferred at the time with an explicit "free only until 0.2.0's first publish" deadline (task #658) recorded in `Cargo.toml`'s own feature comment. That deadline arrived with 0.2.0 queued for publish (this item moved to `[A]`/URGENT, task #934/C-9, for exactly that reason).

    - **Fix:** converted `mock` from a Cargo feature to the build-time `--cfg aligned_vmem_mock` flag (enabled via `RUSTFLAGS="--cfg aligned_vmem_mock"`), matching this repo's own `cfg(loom)`/`cfg(kani)` precedent — cfg flags are passed only explicitly per build invocation and do not unify across the dependency graph, closing the hazard structurally rather than by documentation/convention alone. Confirmed before converting that the hazard had zero present cost (grep across the whole workspace: `aligned-vmem/mock` was not actually enabled anywhere), so the conversion carried no breaking-change cost despite touching 13 files (`crates/aligned-vmem/{Cargo.toml,README.md,src/{lib,mock,fault_injection}.rs,tests/{mock,mock_reentrancy,fault_injection,lazy_commit,smoke}.rs}`, `.github/workflows/ci.yml`, root `Cargo.toml`, `crates/numa-shim/Cargo.toml` — the last two comment-only cross-reference updates).
    - **CI:** every step that previously relied on `--all-features` to reach mock coverage (which it can no longer do — mock is not a Cargo feature) got a dedicated `RUSTFLAGS: "--cfg aligned_vmem_mock"` step instead: `aligned-vmem-gates` (clippy + test, plus the `--cfg miri` row now also carrying `--cfg aligned_vmem_mock` to preserve its previously-incidental mock+miri type coverage), `test-windows`, `test-macos`, `test-workspace`. The feature-powerset job's comment corrected (5 features now, not 6 — mock sits entirely outside cargo-hack's feature space).
    - **Verification:** a clean `cargo build -p aligned-vmem` produces zero `unexpected_cfgs` warnings (the new `[lints.rust] unexpected_cfgs` check-cfg declaration in `crates/aligned-vmem/Cargo.toml` covers the flag); `cargo test` WITH vs WITHOUT `RUSTFLAGS="--cfg aligned_vmem_mock"` flips `mock.rs` (0↔12), `mock_reentrancy.rs` (0↔2), `fault_injection.rs` (5↔0), and `lazy_commit.rs` (12↔11) exactly as intended; clippy clean on every arm; `cargo doc --all-features --no-deps` clean; `cargo test -p numa-shim --features mock` still green (28/28, confirming numa-shim's own SEPARATE `mock` feature — deliberately left unconverted, see item 42's remaining card above — was not disturbed); `ci.yml` re-parses as valid YAML; the full `npm run check` workspace gate is ALL GREEN.
    - **Not closed by this task:** `numa-shim`'s own `mock` Cargo feature carries the identical hazard and remains unconverted, deliberately — its own first publish (task #657) has not happened yet, so its "free to convert" window has not closed. See item 42's remaining card (renumbered 42, scope narrowed) for that half.
    - **Evidence:** commit (task #962, this session); `crates/aligned-vmem/Cargo.toml`'s `[lints.rust] unexpected_cfgs` declaration and the removed `mock = []`; `.github/workflows/ci.yml`'s new `RUSTFLAGS: "--cfg aligned_vmem_mock"` steps.

57. **`scripts/bench-table.mjs` has been unable to build `benches/global_alloc.rs` since task #583, ~11 days before discovery.** — **CLOSED** (2026-08-16, found and fixed by a user report of `npm run bench:table` failing).

    `scripts/bench-table.mjs` hardcodes `FEATURES = 'production'` (set when the script was created, commit `73a6b2b`, 2026-07-07). Task #583 (commit `7a9b7c7`, 2026-08-05, "fix(perf): close F5 — gate `SeferAlloc::dbg_trim_current_thread` behind `internals`") added `internals`/`bench-internals` to `benches/global_alloc.rs`'s own `[[bench]] required-features` in the root `Cargo.toml` (the bench file calls `AllocCore::dbg_layout_class_for` and `SeferAlloc::dbg_trim_current_thread`, both gated behind those features), but never updated `bench-table.mjs`'s `FEATURES` constant to match. `production` (the workspace's default-bundle feature alias) does not and should not include `internals`/`bench-internals` — those are diagnostic-only, opt-in surfaces, correctly excluded from the shipping default. Confirmed pre-existing and unrelated to any session's own work: reproduced the identical error (`error: target 'global_alloc' ... requires the features: 'alloc-global', 'internals', 'bench-internals'`) against a detached worktree at `d1de3bc`, the base commit for an entire multi-round campaign that had no reason to touch this script.

    - **Fix:** `scripts/bench-table.mjs`'s `FEATURES` constant changed from `'production'` to `'production internals bench-internals'`. Both added features are measurement-only surface gates (`internals = []` in `Cargo.toml`, zero runtime-behavior change) — adding them does not change what the bench measures, only what diagnostic API the bench file is allowed to call, matching the already-documented idiom `cargo test --features "production internals"` / `"production bench-internals"` used elsewhere in this repo (`Cargo.toml`'s own `internals` feature comment, `benches/perf_gate_iai.rs`).
    - **Verification:** `cargo bench --features "production internals bench-internals" --bench global_alloc --no-run` compiles cleanly; `npm run bench:table` runs to completion end-to-end (`[bench-table] PASS — 159 bench id(s) parsed, all 51 expected ids present`).
    - **Why this stayed undiscovered for ~11 days:** `npm run bench:table` is not part of `npm run check` (the mandatory pre-push gate) or CI — it is a manually-invoked reporting script, run only when a human explicitly asks for comparative wall-clock numbers. Nobody happened to run it between task #583 landing and this report.
    - **Evidence:** `git log -S'const FEATURES' -- scripts/bench-table.mjs` (created `73a6b2b`, last touched for unrelated label/guard reasons at `0e29fc2`/`b23d7c5`); `git log -1 -S'"internals", "bench-internals"' -- Cargo.toml` → `7a9b7c7`; reproduction against a `d1de3bc` detached worktree.

49. **`aligned-vmem` has ten FFI call sites relying on the edition-2021 implicit `unsafe fn` body instead of an explicit `unsafe {}` block with its own `// SAFETY:` comment — none unsound today, but edition 2024 makes `unsafe_op_in_unsafe_fn` a hard error at all ten.** — **CLOSED** (task #997, P3-8 pass 2 of the 0.2.0 pre-release audit/closing-review campaign).

    Fully resolved across two passes of the same campaign. Pass 1 (P3-8): `libc_munmap` and miri's `release_reservation` wrapped in explicit `unsafe {}` with per-operation `// SAFETY:` comments; the third original site, `libc_madvise_hugepage`, was deleted entirely by a separate finding (II-5) that removed the whole function (the sole remaining `MADV_HUGEPAGE` call was ineffective) — a deleted function has no migration debt. Pass 2 (closing-review, task #997): the remaining six sites this item's own measurement command names were wrapped the same way — the Windows `release_reservation`'s `winapi_virtual_release` call (see `fn release_reservation`), `winapi_virtual_reserve`'s `VirtualAlloc` call (see `fn winapi_virtual_reserve`), `winapi_virtual_decommit`'s `VirtualFree` call (see `fn winapi_virtual_decommit`), `winapi_virtual_release`'s own `VirtualFree` call (see `fn winapi_virtual_release`), the Unix `release_reservation`'s `libc_munmap` call (see `fn release_reservation`), and `libc_madvise`'s `madvise` call (see `fn libc_madvise`). Also fixed two NEW sites introduced by a different round after this item was first filed: `benches/vmem_bench.rs`'s `fault_pages` helper (`base.add(offset)` and `ptr::write_volatile(...)`, added by task #985/II-19), which had its own `unsafe_op_in_unsafe_fn` debt from day one. Re-measured with this item's own prescribed command (`RUSTFLAGS="-W unsafe_op_in_unsafe_fn" cargo clippy -p aligned-vmem --all-targets --features "lazy-commit huge-pages fault-injection bench-internals"`, both on the default target and `--target x86_64-unknown-linux-gnu` for the Unix-only sites) — zero warnings on both, confirmed independently before closing this card.

    - **Evidence:** `docs/reviews/2026-08-16-aligned-vmem-fxx-prerelease-audit.md` findings P3-8 (pass 1 + pass 2 fixes) and II-5 (the `libc_madvise_hugepage` removal); direct fixes in `crates/aligned-vmem/src/lib.rs` at the six call sites cited above and `crates/aligned-vmem/benches/vmem_bench.rs`'s `fault_pages`.

52. **[T, INFO] `decommit_lazy` leaves free BSD reclaim on the table** — **CLOSED** (filed 2026-08-14, task #934/C-9, from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md` finding V-4).

    Resolved by the `madv_free_advice()` function now having BSD-specific arms (FreeBSD/DragonFly: `MADV_FREE_BSD_5`; NetBSD/OpenBSD: `MADV_FREE_BSD_6`) instead of falling back to `MADV_DONTNEED`. The function now dispatches to the real `MADV_FREE` constants for all four BSDs, making `decommit_lazy` genuinely lazy on those platforms (not a `MADV_DONTNEED` fallback). Item 48 (the underlying decommit/Darwin gap) remains open as a separate issue; item 53 (the `from_raw_parts` interaction) is also now closed.

    - **Evidence:** see `fn madv_free_advice` — the current implementation with BSD arms.

53. **[T, INFO] `Reservation::from_raw_parts` hard-codes `granted_huge: false`, creating a fail-open hazard when callers follow documented decommit advice** — **CLOSED** (filed 2026-08-14, task #934/C-9, sub-observation about item 48 from `docs/reviews/2026-08-14-aligned-vmem-pre-release-review.md`).

    Resolved by `from_raw_parts` now taking an explicit `granted_huge: bool` parameter (breaking change) instead of hard-coding `false`. Callers can now accurately reconstruct a `Reservation` with the correct `granted_huge` flag when adopting a foreign reservation, eliminating the fail-open hazard. This was a deliberate API change in the 0.2.0 pre-release cycle to address the contract gap identified in this item. Item 48 (the underlying decommit/Darwin gap) remains open as a separate issue.

    - **Evidence:** see `pub unsafe fn from_raw_parts` signature (`granted_huge: bool` parameter) and its use in the `Reservation` constructor (`granted_huge` field).

64. **Follow-up from commit 66b8508 (task #1030): `npm run check` lacks a `cargo test -p aligned-vmem` row with DEFAULT features, though ci.yml has a separate job that tests workspace members with default features.** — **CLOSED** (filed 2026-08-16, R7-8 finding class third occurrence; this is the same gap task #1024 closed with commit 66b8508, resurfaced as items 64/65 in task #1034).

    The local gate now matches ci.yml's runtime coverage: added `cargo test -p aligned-vmem` (plain default features, no `--all-features`) to `scripts/check-all.mjs`. The pre-existing clippy rows (`--all-targets`) validated compile-time only; this new step catches runtime failures (OOM, panic) that clippy cannot see. Placement: immediately after the `test (aligned-vmem --all-features)` step, maintaining the existing grouped aligned-vmem block.

    - **Files changed:** `scripts/check-all.mjs` (added one new step; updated header comment to reflect aligned-vmem block is now 9 steps: 5 clippy + 2 test + 1 doc + 1 optional semver).
    - **Verification:** `cargo test -p aligned-vmem` (default features) runs green locally (53 tests, 0 failures); `node --check scripts/check-all.mjs` passes (script is valid JS); `cargo test --all-features --test ci_clippy_matrix_consistency` passes (PER_PR_ROWS unchanged — aligned-vmem rows are a separate group, not part of the pinned matrix).
    - **Evidence:** this session's `scripts/check-all.mjs` diff; the added step's name `test (aligned-vmem default)`; local verification output below.

65. **CI-coverage gap: `aligned-vmem-gates` job added three steps (cargo doc with RUSTDOCFLAGS="-D warnings", cargo publish --dry-run, cargo semver-checks check-release) that are NOT covered by `npm run check`.** — **CLOSED** (filed 2026-08-16, task #1039 coverage gap; same class as task #1024's `aligned-vmem package gates` gap).

    The local gate now reproduces two of the three CI steps (the third, `cargo publish --dry-run`, was deliberately excluded per this item's own "Next trigger" instruction because it contacts crates.io on every local run). Added:
    1. `RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --all-features --no-deps` — implemented as a `cargo doc` step with `env: { RUSTDOCFLAGS: '-D warnings' }` to avoid Windows shell quoting issues.
    2. `cargo semver-checks check-release --package aligned-vmem` — implemented as an optional wrapper script (`scripts/aligned-vmem-semver-check-optional.mjs`) that (a) checks if `cargo-semver-checks` is installed via `cargo semver-checks --version`, and (b) if missing, prints a clear diagnostic ("tool not installed; skipping — this is expected and NOT a failure") and exits 0; if present, runs the actual check and propagates its exit code. This distinction makes "tool missing" (configuration absence) visible and distinct from "tool present but broke" (actual semver violation), avoiding the forbidden `|| true` silent-swallow pattern.

    - **Files changed:** `scripts/check-all.mjs` (added two new steps; made the runner loop actually PASS `step.env`; corrected the header's step numbering); `scripts/aligned-vmem-semver-check-optional.mjs` (new optional wrapper script); `scripts/lib.mjs` (no change needed).
    - **A defect this closure INTRODUCED and then had to fix — recorded here rather than only in the commit body (exactly the F7/R22-3 class this index exists to prevent):** the first version of the doc step declared `env: { RUSTDOCFLAGS: '-D warnings' }` on the step object while `check-all.mjs`'s runner loop still called `run(step.cmd, step.args, { cwd: REPO_ROOT })` — it never read `step.env`. `run()` itself was never at fault (`scripts/lib.mjs:52` forwards `opts` straight to `spawn`, so it has always supported `env`); the loop simply dropped the field. The result was a step NAMED "warnings-as-errors" that could not fail — worse than an absent step, because it reads as coverage. Fixed by merging `step.env` OVER `process.env`, not replacing it: `spawn`'s `env` option replaces the environment wholesale, which would strip `PATH` and break every subsequent `cargo` lookup. Proven by counterfactual rather than by inspection — the same broken intra-doc link yields `exit 0` without the pass-through and `exit 101` with it.
    - **A second stale-doc defect found in the same file while closing this item:** the header comment's step numbering placed the aligned-vmem group AFTER the two remaining `PER_PR_ROWS` rows ("12-13 = PER_PR_ROWS, 14 = aligned-vmem"). The runtime array has had the opposite order since commit 66b8508 introduced the group. Re-derived from the array itself and corrected to `12-20` (aligned-vmem, 9 steps) / `21-22` (PER_PR_ROWS), renumbering the seven entries after them; the sibling stale figure in `docs/perf/OPEN_ITEMS.md` item 50 ("steps 14-19") was corrected in the same pass.
    - **Verification:** `RUSTDOCFLAGS="-D warnings" cargo doc -p aligned-vmem --all-features --no-deps` runs green locally; `node scripts/aligned-vmem-semver-check-optional.mjs` runs green locally (found cargo-semver-checks 0.50.0, semver check reported `0 checks: 0 pass, 254 skip` — every lint is SKIPPED because 0.1.0→0.2.0 is a major bump for a 0.x crate, so this step cannot fail until 0.2.0 is itself the baseline); `node --check scripts/check-all.mjs` passes; full `npm run check` from the primary checkout reports `ALL GREEN`.
    - **Evidence:** this session's `scripts/check-all.mjs` diff; the new `scripts/aligned-vmem-semver-check-optional.mjs` script with explicit "tool missing vs. tool present but broke" diagnostic distinction; local verification output below.
