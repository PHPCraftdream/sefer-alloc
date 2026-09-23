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

148. **`HeapOverflow`'s bounded second-chance overflow ring still loses cross-thread frees under sustained owner-starvation — a genuinely lossless protocol was scoped OUT of R2-09's fix as too large/risky for a single task cycle; the interim mitigation (exact drop counter + honest capacity docs) landed instead.** (Filed round 2 of the independent src review, R2-09, task #2011, `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md`.) The finding: `HeapCore::push_with_overflow_retry`'s terminal branch (`src/registry/heap_core_xthread/overflow.rs:923`) bumps `DBG_RING_PUSH_RETRY_EXHAUSTED` and returns once BOTH the segment's `RemoteFreeRing` (256 slots) AND the heap-level `HeapOverflow` ring (2048 native / 64 under miri) are saturated with a genuinely non-draining owner (paused or exited) — the freed block is a bounded, documented, non-UB leak (`heap_overflow.rs`'s own "Capacity — an honest bound, not an unbounded proof" module-doc section already states this explicitly). The review's suggested real fix is either a growable OS-backed spill (extending `HeapOverflow`'s existing lazily-materialised-sidecar mechanism from ONE fixed-size chunk to a linked chain of chunks) or a proven producer-side helping protocol with a separate ownership/lifetime model — both explicitly flagged by the review itself as needing "a genuinely new protocol," not a small patch.

    **Why this was scoped out of R2-09 rather than attempted:** a growable-modulus concurrent ring (the natural shape for "extend capacity while staying a REUSABLE ring, not an ever-growing unbounded queue") is a materially harder concurrent-correctness problem than anything else in this review round — reusing the crate's existing EBR machinery (`src/concurrent/epoch/`) to safely reclaim drained overflow chunks is a promising de-risking angle (sidesteps inventing a new unsafe reclamation protocol from scratch) but is still a multi-file, hot-cross-thread-free-path redesign needing its own dedicated loom model (the existing `crates/once-ptr-cell/tests/loom_once_ptr_cell.rs` only models the CURRENT single-fixed-sidecar CAS, not a growable chain) and a real cross-thread pause/resume ledger proof — exactly the review's own P1 findings this same round (R2-06, R2-10) warn are easy to get subtly wrong on this exact class of intrusive atomic structure. R2-09 is P2, not P1; rushing an under-verified structural change to the allocator's hottest cross-thread free path to close a P2 finding within one task cycle was judged the wrong tradeoff (CLAUDE.md's "don't design for hypothetical future requirements... state material tradeoffs" discipline, applied to scope rather than a fabricated design).

    **What R2-09 DID land instead (the review's own explicitly-sanctioned interim fallback — "До исправления явно ограничить поддерживаемый workload и экспортировать точный drop counter, не обещать общий bound"):** `AllocStats` gained a new field exposing `DBG_RING_PUSH_RETRY_EXHAUSTED` (the exact, final, "this block is now permanently unreachable" counter — distinct from the already-exposed `ring_overflows`/`DBG_RING_OVERFLOW`, which measures only first-TIER ring-full events, most of which DO get recovered by the second-chance overflow ring) — see `src/global/alloc_stats.rs` and `src/global/sefer_alloc/diag.rs`. A new deterministic paused-owner regression test (`tests/remote_fanin.rs::remote_fanin_owner_starved_residual_is_exactly_accounted`) drives a burst deliberately sized past the combined ring+overflow capacity, proving the counter fires (non-vacuous `exhausted_delta > 0`, unlike the pre-existing harness's `== 0` case) and that `SeferAlloc::stats().cross_thread_frees_lost` exactly reflects that delta on the PUBLIC surface, not just a crate-internal test hook. (An earlier draft of this test additionally asserted `reclaimed + exhausted_delta == N` as a full ledger reconciliation — that assertion was WRONG and removed: this allocator is capacity-elastic, so the owner's post-burst `alloc()` loop keeps succeeding regardless of how many of the original N blocks were permanently dropped, making `reclaimed` and `exhausted_delta` non-complementary quantities. See that test's own doc comment for the full counterfactual.) This proves the diagnostic is trustworthy even though it cannot prove zero loss.

    - **Status:** OPEN — the lossless redesign itself is NOT implemented; only the interim exact-counter + honest-docs mitigation landed.
    - **Current number:** 0 real-fix attempts so far; 1 interim-mitigation commit (this round).
    - **Next trigger:** A dedicated future task explicitly scoped to the growable-spill (or producer-side-helping) redesign, budgeted for its own loom model and cross-thread ledger proof — not bundled into an unrelated round's remediation queue.
    - **Evidence:** `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md` R2-09 section; `src/registry/heap_core_xthread/overflow.rs:918-923` (the terminal exhausted branch); `src/registry/heap_overflow.rs`'s module doc "Capacity — an honest bound, not an unbounded proof" and "The wedge hazard — why a naive lazy sidecar would be UNSOUND" sections (the existing sidecar mechanism this redesign would need to extend, and the exact class of hazard a chunk-chain extension would need to re-prove); this round's commit adding `AllocStats`'s new exact-counter field and the paused-owner ledger regression test.

149. **`RemoteFreeRing`'s `u32` tail admits an ABA hazard between the capacity check and the CAS after a full wraparound — reproduced at reduced scale in loom; the real fix (widen cursors to `u64`) was scoped out of R2-10's own task cycle as too large/risky for a single cycle, mirroring item 148's (R2-09, same review round) disposition.** (Filed round 2 of the independent src review, R2-10, task #2012, `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md`.) The finding: `RemoteFreeRing::push`/`try_push_uncounted` (`src/alloc_core/segment/remote_free_ring/ops.rs`) read `t = tail.load(Relaxed)`, check capacity via `full_check(t)`, and only then CAS `tail: t -> t+1` — nothing binds the CAS's compare value to the specific incarnation of ring state the capacity check reasoned about. A producer preempted between its capacity check and its CAS, if enough OTHER producers + the consumer complete a full `u32` wrap (net `2^32` pushes) while it is stalled, can have its stale CAS succeed by numeric coincidence when `tail` wraps back to the same value — over-reserving into an already-full ring and overwriting a live, undrained entry in the recycled slot. The existing Kani proofs (`src/kani_proofs.rs`'s `ring_wrap_proofs` module) do NOT cover this: they are exhaustive over a SINGLE `head.wrapping_add(n)` step, a genuinely different (single-call, non-temporal) claim from "does a snapshot survive an arbitrary number of intervening operations."

    **Why this was scoped out of R2-10 rather than landed:** the minimal correct fix — widen `head`/`tail`/`cached_head` from `u32` to `u64` — is layout-PRESERVING in principle (`CURSOR_BLOCK`'s existing 128-byte padding has room for two 8-byte cursors plus the `overflow` counter on the producer cache line without changing `FOOTPRINT`, so `SegmentHeader`/`Layout` downstream offsets do not move), but its full blast radius is large: this module's `HEAD_OFF`/`TAIL_OFF`/`CACHED_HEAD_OFF` layout constants, every `dbg_*` test hook with a `u32` head/tail signature (`dbg_cursors`, `dbg_set_cursors`, `dbg_advance_head_only`, `head_relaxed`, `tail_relaxed`, `drain`'s return type), `src/kani_proofs.rs`'s `ring_wrap_proofs` module, and — most substantially — test files built specifically AROUND the `u32` wrap boundary as the ring's documented "one genuinely reachable" hazard (`tests/regression_ring_cursor_wrap.rs`, `tests/remote_ring_shadow_head.rs` — both explicitly construct scenarios crossing `u32::MAX -> 0`), whose entire premise (the wrap boundary IS `u32::MAX`) would need a conceptual, not just mechanical, rewrite once the boundary moves to `u64::MAX` (a third file, `tests/remote_free_ring_head_write_sites.rs`, calls the same `dbg_*` hooks but only structurally counts write-site occurrences in the source text — type-agnostic, needs no conceptual change). This hazard also requires ~`2^32` operations during ONE producer's stall to trigger — the same order of magnitude of rarity `src/alloc_core/segment/remote_free_ring/mod.rs`'s own pre-existing "F10 wrap argument precondition" section already accepts elsewhere for a related staleness hazard ("No code change is warranted for a hazard this remote"). Rushing a large, layout-adjacent, cross-cutting change to the allocator's cross-thread free hot path to close a P2 finding within one task cycle was judged the wrong tradeoff, the same reasoning item 148 already applied to R2-09 in this same review round.

    **What R2-10 DID land instead:** a reduced-width loom reproduction, `tests/loom_remote_ring_tail_aba.rs` — `counterfactual_narrow_tail_stale_cas_violates_capacity_invariant` and `counterfactual_narrow_tail_stale_cas_overwrites_live_undrained_entry` are `#[should_panic]` counterfactuals proving the hazard is real on the CURRENT protocol shape (cursors wrap at `MOD = 4` instead of `2^32`, so a full incarnation cycle is 4 pushes, not billions) — one proving the occupancy-invariant violation, one proving the concrete data-loss consequence. `correct_wide_tail_stale_cas_rejects_after_same_finite_script` re-runs the IDENTICAL finite reproduction script against a cursor wide enough (relative to that script) that the coincidence cannot occur — modelling why the real `u64` widening closes the hole. `narrow_fresh_producer_always_rejects_concurrently_with_racing_stale_cas` is a contrast test proving the bug is specific to the STALE snapshot, not a general fragility in the fresh-check protocol. `src/alloc_core/segment/remote_free_ring/mod.rs`'s module doc gained a new "R2-10 — the tail-CAS ABA hazard" section recording the mechanism, the loom evidence, and this scoping decision; `src/kani_proofs.rs`'s `ring_wrap_proofs` module doc gained an honesty note that its two proofs do not cover this hazard class.

    - **Status:** OPEN — the `u64`-widening fix itself is NOT implemented; only the reduced-width loom reproduction + honest docs landed.
    - **Current number:** 0 real-fix attempts so far; 1 reproduction/documentation commit (this round).
    - **Next trigger:** A dedicated future task explicitly scoped to the `u32` -> `u64` cursor widening, budgeted for updating every `u32`-typed `dbg_*` hook signature and rewriting the `u32`-wrap-boundary-specific test files' premise (not just their types) — not bundled into an unrelated round's remediation queue.
    - **Evidence:** `docs/reviews/2026-09-22-120730-src-review-xa-round-2.md` R2-10 section; `src/alloc_core/segment/remote_free_ring/ops.rs`'s `push`/`try_push_uncounted`/`full_check` (the CAS-after-stale-check shape); `src/alloc_core/segment/remote_free_ring/mod.rs`'s "R2-10 — the tail-CAS ABA hazard" module-doc section; `tests/loom_remote_ring_tail_aba.rs` (the reduced-width reproduction + closure evidence); `src/kani_proofs.rs`'s `ring_wrap_proofs` module (the honesty note on what it does not cover).
