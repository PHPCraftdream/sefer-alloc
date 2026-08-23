# Correctness / CI-debt open items — [A] Active tier

**Part of the split index.** This file holds the full text of every
**[A]** (active) card. Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for
the purpose/scope/convention header, the round-start reading order, and
the complete item-number → file lookup table; come here for the card
bodies. See `docs/correctness-open-items/TRACKED_hook_safety.md`,
`TRACKED_verification_coverage.md`, `TRACKED_platform_contracts.md`,
`TRACKED_ci_gate_coverage.md`, `TRACKED_test_flakiness.md`,
`TRACKED_correctness_residuals.md`, `TRACKED_publish_readiness.md`,
`TRACKED_process_record.md`, `TRACKED_misc.md`
for the **[T]** tier (split by THEME, task #1222, 2026-08-20 — superseding
task #1221's same-day item-number-range split) and
`docs/correctness-open-items/RESOLVED.md` for
the closure trail. (Split 2026-08-20, task #1217, reversing item 86's
2026-08-19 deferral — see `docs/CORRECTNESS_OPEN_ITEMS.md` item 86 for
the reversal record.)

---

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
    - **CORRECTION (2026-08-23, task #1263):** this card's premise that
      numa-shim "has not yet had its first crates.io publish (task #657,
      itself blocked)" is FALSE — numa-shim 0.1.0 was published to crates.io
      on 2026-06-29 17:36:48 UTC (crates.io API; also stated by
      crates/numa-shim/CHANGELOG.md). The "free to convert only until first
      publish" window has therefore ALREADY closed: removing or narrowing
      the `mock` feature is now a semver-breaking change to published API,
      not a pre-publication cleanup. The deferral rationale above is
      historical text kept per the history-is-not-rewritten convention; the
      real decision point is now the 0.2.0 boundary (or a purely additive
      cfg alongside the existing feature). See task #1263 and the corrected
      comment in crates/numa-shim/Cargo.toml.
    - **RECOMMENDATION (2026-08-23, task #1264 — F2 of
      `docs/reviews/2026-08-23-164206-numa-shim-publication-audit-Sol-codex.md`
      re-raised this card; task #1264 is docs-only research, no code was
      changed):** the audit asks the owner to settle, before the next
      publish, whether the `mock` seam becomes a build-time cfg, moves to a
      test-support crate, or stays a Cargo feature as an explicit risk
      acceptance. What follows is a recommendation with THIS crate's
      concrete numbers — the decision itself remains the owner's (breaking
      changes are an owner call per CLAUDE.md). One fact changes the
      calculus since item 42 was last weighed: **the next release is
      already 0.2.0-shaped and already breaking** —
      `crates/numa-shim/CHANGELOG.md`'s Unreleased "Changed" section bumps
      `aligned-vmem` 0.1 → 0.2 with `reserve_on_node`'s return type moving
      with it, and the audit's own F1 recommends shipping the next release
      as 0.2.0. The "free until first publish" window that #1263's
      correction said had closed is therefore open ONE more time, in
      modified form: removing `mock` rides the already-breaking 0.2.0 at
      zero MARGINAL semver cost.
      - **(a) build-time `--cfg numa_shim_mock` (mirror task #962, commit
        `18c29e4`):** the proven in-repo template — aligned-vmem's
        conversion was 13 files, +245/−212, 46 cfg sites, with a ±136-line
        ci.yml treatment. For numa-shim the concrete shape: 27
        `feature = "mock"` cfg sites across 5 files (22 lines in
        `src/lib.rs`: the `pub mod mock` gate at `:97`, four mock/real
        dispatch pairs inside `current_node_resolution` `:311`/`:323`,
        `current_node` `:363`/`:380`, `bind_range` `:459`/`:467`,
        `reserve_on_node` `:511`/`:527`, the doc-hidden Linux forwarder at
        `:691`, and 11 `allow(dead_code)` `cfg_attr` sites; plus
        `tests/mock_dispatch.rs`, `tests/node_resolution.rs`,
        `tests/node_resolution_linux.rs`, `benches/numa_bench.rs`);
        `crates/numa-shim/Cargo.toml` drops `mock = []`, adds
        `[lints.rust] unexpected_cfgs` (mirroring
        `crates/aligned-vmem/Cargo.toml:179`), and must resolve
        `[[bench]] numa_bench`'s `required-features = ["mock"]` — a bench
        cannot require a cfg flag; post-#962 `vmem_bench` carries no
        required-features, so the bench compiles under both arms or gets an
        internal cfg gate. The part #962 did NOT have: numa-shim's mock has
        a CROSS-CRATE consumer — the root crate's `numa-aware-mock` (root
        `Cargo.toml:721`) gates 8 root test files (6 of them
        `use numa_shim::mock;`). So the root diff is: `numa-aware-mock =
        ["numa-aware"]` (keep the gate feature, stop forwarding the
        soon-gone `numa-shim/mock`), each of the 8 files' single cfg site
        re-gated to also require the cfg flag (so plain `--all-features`
        builds SKIP them silently instead of failing on the missing
        `numa_shim::mock` module — the exact semantics aligned-vmem's own
        tests adopted with `#![cfg(aligned_vmem_mock)]`),
        `'cfg(numa_shim_mock)'` added to root `Cargo.toml:104`'s check-cfg
        list, and explicit `RUSTFLAGS="--cfg numa_shim_mock"` steps on the
        CI rows that want mock coverage (`ci.yml` `numa-shim-mock`
        `:2622-2623`, `numa-shim-windows` `:2652-2653`, `numa-shim-macos`
        `:2675`, plus dedicated rows replacing the mock coverage today's
        `--all-features` clippy rows at `:2632`/`:2661` reach only
        incidentally). Estimate: ~15 files, ~35 gate edits — a comparable
        one-task size to #962, against a proven pattern. Semver: breaking
        (removes 0.1.0's published `mock` feature), covered by the 0.2.0
        bump the release already needs. A staged variant (a′) exists if the
        owner wants zero break at 0.2.0: land the cfg ADDITIVELY
        (`any(feature = "mock", numa_shim_mock)` per site), document `mock`
        as deprecated, remove it at 0.3.0 — the additive path the corrected
        `crates/numa-shim/Cargo.toml` comment itself names — at the cost of
        uglier gate expressions and the hazard surviving through 0.2.x.
      - **(b) separate unpublished test-support crate:** for THIS crate not
        a file move but an architecture change: the mock's value is that it
        sits INSIDE the dispatch arms of
        `current_node`/`current_node_resolution`/`bind_range`/
        `reserve_on_node` (`src/lib.rs:311-527`), so CI asserts the REAL
        wrapping logic. A separate crate can reach that only via new
        injection surface on numa-shim (runtime backend indirection or
        `#[doc(hidden)]` hooks) — i.e. NEW published API surface, the
        opposite of what the audit asks — or by duplicating the wrapping
        logic in the test crate, which then tests a copy rather than the
        shipped code. Larger diff than (a), strictly smaller benefit; not
        recommended.
      - **(c) keep the feature, record explicit risk acceptance:** zero
        code diff — this card plus a CHANGELOG line IS the record. The risk
        accepted, concretely: any downstream graph in which one target
        enables `numa-shim/mock` silently swaps real NUMA syscalls for the
        recording stub for every other consumer in that graph — this repo
        itself demonstrates how easily that happens (every root
        `--all-features` build enables it via `numa-aware-mock`); docs.rs
        is protected only by the hand-maintained explicit feature list
        (`crates/numa-shim/Cargo.toml:27-28`), and `cargo doc
        --all-features` anywhere still renders the mock as reference API.
        The audit pre-commits to treating a warning as insufficient for a
        confident GO (F2: «простого README-предупреждения недостаточно» — a
        README warning alone is not enough), so (c) means knowingly
        shipping the next release with F2 still an open P1.
      - **Recommended: (a) at the 0.2.0 boundary.** The semver objection —
        the only reason numa-shim's conversion was deferred while
        aligned-vmem's was executed — no longer applies, because 0.2.0 is
        already breaking for `vmem-integration` users regardless; the
        pattern is proven in-repo at nearly this exact scale; and (a) is
        the only option that structurally closes F2 rather than documenting
        it. Fallback if the owner judges the diff too large for the
        pre-release window: staged (a′). (c) is acceptable only as a
        conscious decision to keep F2 open at release. **This
        recommendation does NOT close this card — the owner's recorded
        decision does; until then item 42 stays OPEN with this section as
        the standing briefing.**
    - **CORRECTION SCOPE NOTE (2026-08-23, task #1274, tenth review N1):**
      task #1263's premise correction above covered only decision (5) of
      `53b3ca2` — the `mock` Cargo FEATURE. The same commit's "before this
      crate's first crates.io publish" premise was equally false for its
      decisions (1) and (4), which — with `dbfeca3`'s 2026-07-19 enum-level
      `#[non_exhaustive]` on `MockCall` from the same already-published
      window — are THREE semver-breaking changes to published 0.1.0's
      `--features mock` surface: `CALLS`/`CURRENT_NODE_SLOT` `pub` →
      `pub(crate)` (item removal), `#[non_exhaustive]` on `MockCall`
      (breaks exhaustive match), and `#[non_exhaustive]` on the
      `BindRange`/`ReserveOnNode` variants (breaks struct-literal
      construction / exhaustive field patterns). Recorded in
      `crates/numa-shim/CHANGELOG.md`'s Unreleased `### Removed` section by
      task #1274. Consequence: the next release cannot be `0.1.1`, which
      strengthens this card's recommendation (a) — 0.2.0 is already
      breaking on three additional axes beyond the `aligned-vmem` 0.2
      return-type move. Full finding: N1 of
      `docs/reviews/2026-08-23-183220-numa-shim-publication-readiness-review-oh.md`.

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
