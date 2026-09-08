# Correctness / CI-debt open items — Recently resolved (closure trail)

**Part of the split index.** This file holds the full "Recently
resolved" pointer trail — the closure trail described by
`docs/CORRECTNESS_OPEN_ITEMS.md`'s convention rule 2 ("When you
close an item: move its entry to §'Recently resolved'"). Each pointer
resolves further into `docs/correctness-open-items/ARCHIVE.md`'s full
closure narratives. Start at `docs/CORRECTNESS_OPEN_ITEMS.md` for the
purpose/scope/convention header and the complete item-number → file
lookup table; see `docs/correctness-open-items/ACTIVE.md`
and `docs/correctness-open-items/TRACKED_hook_safety.md` /
`TRACKED_verification_coverage.md` / `TRACKED_platform_contracts.md` /
`TRACKED_ci_gate_coverage.md` / `TRACKED_test_flakiness.md` /
`TRACKED_correctness_residuals.md` / `TRACKED_publish_readiness.md` /
`TRACKED_process_record.md` / `TRACKED_misc.md`
(split by THEME, task #1222, 2026-08-20 — superseding task #1221's
same-day item-number-range split) for the open tiers.
(Split 2026-08-20, task #1217, reversing item 86's 2026-08-19 deferral
— see `docs/CORRECTNESS_OPEN_ITEMS.md` item 86 for the reversal
record.)

---

## Recently resolved (closure trail — do not re-list as open)

- 27. **[T, filed 2026-08-06, task #653/P19, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P3-4 item 3] tagged-index-stack's `compile_error!` guard for unsupported `target_has_atomic` widths doesn't suppress the cascading `E0432` unresolved-import error on the same build — a deliberate, unrecorded tradeoff.** (Headline kept verbatim per the history-is-not-rewritten convention.) — **CLOSED** (2026-09-01, tagged-index-stack round-11 @oh independent review, finding P3-4 — the review that caught the card describing an already-fixed state). The card recorded a deliberate tradeoff from commit `300b41f`'s era: the guard fires first and gives a clear named error, but the follow-on cascade was tolerated because suppressing it "would require cfg-gating every downstream item in the file — judged too intrusive for the benefit on an already-broken build." That tradeoff stopped existing when the Sol-codex run-3 review's P2-4 fix (commit `db8bb77`) moved the crate's ENTIRE implementation behind one gate — `#[cfg(all(target_has_atomic = "64", any(not(loom), feature = "loom")))]` on `mod imp`/the re-exports in `crates/tagged-index-stack/src/lib.rs` — whose predicate is the EXACT complement of the two `compile_error!` conditions, so on an invalid config rustc emits only the named error and name-resolves no implementation item; the card simply never got updated in the rounds that followed, exactly the stale-current-state defect CLAUDE.md's "OPEN_ITEMS indexes are CURRENT-STATE, not archives" rule exists for. Personally re-verified at closure on the real toolchain: `cargo check -p tagged-index-stack --target thumbv6m-none-eabi` (no native 64-bit atomics) fails with an error whose shape is exactly ONE named `error:` line (the 64-bit-atomics guard) plus cargo's own "could not compile" summary, and ZERO `error[E...]`-numbered lines — no E0432/E0433 cascade. Regression coverage — the caveat that decided HOW this closed: the `--cfg loom`-without-feature half of the guard has been pinned since round 10 by `tests/compile_fail_loom_cfg_without_feature.rs`, but the `target_has_atomic`-cascade half had NO automated regression at closure time (this repo's CI built this crate only for `x86_64-unknown-none`, which HAS 64-bit atomics, so the no-atomics branch was never exercised automatically). Closed the preferred way rather than accepting a manual-check-only closure: `ci.yml`'s `test-workspace` job now installs the `thumbv6m-none-eabi` target and runs a dedicated step asserting the error SHAPE (build must fail; exactly one non-summary `error:` line, which must be the named 64-bit-atomics guard; zero `error[E...]` lines) — verified locally against the same command that step runs. The tier-file card in `TRACKED_publish_readiness.md` was updated to CLOSED in the same commit.

- 25. **[T, filed 2026-08-06, task #653/P19, `docs/reviews/2026-08-06-publish-readiness-sweep-closing-review.md` finding P3-4 item 1] `TaggedIndex<INDEX_BITS>` rejecting `INDEX_BITS > 32` at compile time (F1, task #638) has no automated compile-fail test — CI coverage gap, honestly recorded but unfiled until now.** (Headline kept verbatim per the history-is-not-rewritten convention; `>32` was accurate at filing time.) — **CLOSED** (2026-08-31, tagged-index-stack round-4 independent review, finding P3-8, `docs/reviews/2026-08-31-025356-tagged-index-stack-review-round4-oh.md`). The item's own trybuild-or-accepted-risk closing trigger was satisfied in round 2 (task #1689/tis-r2-T11, commit `edbe05f`): the crate declined adding a `trybuild` dependency for a single-crate, single-assertion tradeoff, citing the identical precedent already set by `crates/sefer-region/tests/handle_static_asserts.rs` and `crates/aligned-vmem/tests/smoke.rs`, and wrote that rationale into `tests/stack_unit.rs:272-289` as a permanent code comment. This card's `Status: OPEN` was simply never updated to reflect that closure across two subsequent rounds, and had independently drifted stale on every one of its cited facts: the enforced range narrowed from `1..=32` to `1..=16` in round 2 (task #1679, commit `f23db29`), so the first-rejected width is now `17`, not `33`; `TaggedIndexStack<N>` has exactly ONE const generic parameter, not two (`TaggedIndexStack<33, _>` in the old "Next trigger" text was never a real type signature); and both line citations rotted (`_CHECK_BITS` moved to `src/lib.rs:280-286`; the recorded-gap comment moved to `tests/stack_unit.rs:272-289`). No code change — this is a bookkeeping-only closure correcting a stale card, per `CLAUDE.md`'s "OPEN_ITEMS indexes are CURRENT-STATE, not archives" rule.

**Full closure narratives moved to the archive (R34-24, task #1109, 2026-08-18).**
Each pointer line below is the moved entry's original header line (verbatim,
item number unchanged), followed by the relocation note; the full byte-identical
narrative lives in `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved —
full closure trail".

- 45. **`numa-shim`'s `CURRENT_NODE_SLOT: RefCell<u32>` where a `Cell<u32>` would do, and its accessor still uses a panicking `borrow_mut()`.** (Filed 2026-08-09, task #778/F4, round-closing review — audit §A2, INFO.) — **CLOSED** (2026-08-25, task #1342, finding F3 of the twentieth independent review, `docs/reviews/2026-08-25-021741-numa-shim-publication-audit-run-17-Sol-codex.md`, which re-raised the observation as pre-existing rather than new). The thread-local — a bare `Copy` `u32` only ever whole-value read by `current_node_slot()` and written by `set_current_node()`, never partially mutated, never held across a call — is now `Cell<u32>` using `.get()`/`.set()`, which cannot panic; the panicking `RefCell::borrow_mut()` in `set_current_node` is gone by construction. Sibling `CALLS` (`RefCell<Vec<MockCall>>`, defended by `record()`'s `try_borrow_mut` reentrancy guard) and `POLICY_FAILURE_SLOT` (`RefCell<Option<(u32, io::Error)>>`) deliberately stay `RefCell` — non-`Copy` types genuinely needing borrow-checked interior mutability. Full mock+`vmem-integration` suite green after the swap. Note: an older, already-closed aligned-vmem HugeTLB item further down this trail is ALSO numbered 45 (a pre-existing index numbering quirk, out of scope for the closing task); THIS entry is the numa-shim card — the one cited by `TRACKED_misc.md` and by the thin index's item-45 lookup row.

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

- 42. **`numa-shim`'s `mock` Cargo-feature-unification hazard remains a Cargo feature (deliberately deferred) — the aligned-vmem half of this item is CLOSED, see "Recently resolved" below.** (Filed 2026-08-09, task #776/F13, round-closing review of the aligned-vmem round; moved into the `[A]` tier 2026-08-14, task #934/C-9; aligned-vmem half resolved 2026-08-16, task #962 — this card now covers ONLY the numa-shim half.) — full closure narrative (byte-identical, unmodified) moved to `docs/CORRECTNESS_OPEN_ITEMS_ARCHIVE.md` § "Recently resolved — full closure trail" — this headline is the item's ORIGINAL, pre-numa-shim-closure text (kept verbatim per the history-is-not-rewritten convention); the numa-shim half's own actual closure (2026-08-23, task #1288: `mock` converted to build-time `--cfg numa_shim_mock`) is recorded in the archive entry's own trailing "CLOSED 2026-08-23, task #1288" paragraph, not restated here.

- 143. **[T, filed 2026-09-06] `tests/r14_7_max_segments_ceiling.rs` ceiling tests fail non-deterministically well short of `MAX_SEGMENTS - 1`** — **CLOSED** 2026-09-08, task #1925. The card as it stood at closure follows verbatim, then the closure narrative.

  143. **[T, filed 2026-09-06] `tests/r14_7_max_segments_ceiling.rs`'s two tests
      (`live_large_objects_ceiling_is_exactly_max_segments_minus_one`,
      `ceiling_is_not_permanent_after_freeing_everything`) fail non-deterministically
      on this local Windows dev machine, well short of the expected `MAX_SEGMENTS - 1`
      (4095) ceiling.** Observed while running `npm run check` ahead of an unrelated
      `tagged-index-stack` publish: `achieved` read 1188 on one run and 2171 on an
      immediate re-run of the identical binary with no other `cargo`/`rustc`/`node`
      process running (`tasklist` confirmed) and ample free memory both times
      (`wmic OS get FreePhysicalMemory` — 17-18 GiB free physical, ~46 GiB free
      virtual/commit headroom via `FreeVirtualMemory`/pagefile). Each Large object
      this test allocates consumes one whole `SEGMENT` (`1 << 22` = 4 MiB,
      `src/alloc_core/os.rs:65`), so reaching the ceiling requires ~16 GiB of real
      reservation/commit from a single process — evidence points at a per-process
      OS-level constraint (Windows commit-charge or address-space fragmentation,
      likely ASLR-dependent given the non-deterministic count) rather than a code
      regression: `src/alloc_core/segment_table.rs` (where `MAX_SEGMENTS = 4096` is
      defined) and `src/alloc_core/os.rs` show no commits in recent history, and the
      failure varies in magnitude between identical runs — a real slot-bookkeeping
      bug would fail at the same wrong count every time, not a different one.
      **Not fixed here** — this is orthogonal to the `tagged-index-stack` crate the
      session's actual task concerned (this test exercises only the root crate's
      `AllocCore`/`SegmentTable`, nothing `tagged-index-stack` touches or that its
      own `cargo publish -p tagged-index-stack` packaging/verification exercises),
      and confirming the exact OS-level mechanism (vs. ruling it out and finding a
      real bug instead) needs a dedicated investigation this task did not have
      scope for. **Status (2026-09-06):** OPEN — needs a dedicated session to either
      (a) reproduce
      with `RUST_BACKTRACE=1` / process-level tooling (e.g. Windows Performance
      Recorder, `VMMap`) to confirm the OS-level constraint directly, or (b) rule
      that out and find a genuine bookkeeping regression. **Next trigger:** any
      future `npm run check` or CI run that reproduces this failure — if CI (a
      different, likely less memory-constrained environment) never reproduces it,
      that is itself evidence for the local-machine-resource-constraint hypothesis.
      **Evidence:** local `cargo test --features "production internals" --test
      r14_7_max_segments_ceiling` output, two consecutive runs, 2026-09-06 (counts
      1188 then 2171); `wmic OS get FreePhysicalMemory,FreeVirtualMemory,
      TotalVirtualMemorySize` and `wmic pagefile get AllocatedBaseSize,CurrentUsage`
      output from the same session; `git log --oneline -- src/alloc_core/
      segment_table.rs src/alloc_core/os.rs` showing no recent commits.
  
      **UPDATE 2026-09-07 — mechanism CONFIRMED, hypothesis above upgraded from
      "evidence points at" to established, and one of its own premises corrected.**
      Reproduced a third and fourth time (`npm run check` ahead of the
      globalalloc-model round-5 push: `achieved` 1092; then standalone
      `--test-threads=1`: 2127 — so it is NOT contention with parallel test
      binaries). A scratch probe replicating the test's loop and reading the
      always-compiled `AllocCore::dbg_segments_reserved_total()` /
      `dbg_segments_released_total()` counters at the first null settles the
      open question (a)/(b) above in favour of (a), with a hard OS error code:
  
      ```text
      achieved      = 2125
      live segments = 2126      table full? = false   (MAX_SEGMENTS = 4096)
      last OS error = Os { code: 1455, "The paging file is too small for this
                      operation to complete." }
      ```
  
      Error 1455 is `ERROR_COMMITMENT_LIMIT`. The segment table was barely half
      full when the allocation failed, so the null came from the OS refusing on
      the system-wide commit limit — NOT from a slot lost or gained in the
      register/recycle bookkeeping. This rules out (b).
  
      **Correction to this card's own arithmetic.** The reasoning above ("~16 GiB
      of real reservation/commit", "ample free memory ... 17-18 GiB free
      physical") was wrong on both halves. (i) `Segment::reserve` goes through
      `aligned_vmem::reserve_aligned`, which on an alignment miss **over-reserves
      `size + align` and keeps the whole mapping** (`crates/aligned-vmem/src/lib.rs:28`),
      so each 4 MiB segment can cost 8 MiB — the test's true worst-case demand is
      `MAX_SEGMENTS * 2 * SEGMENT` ≈ **32 GiB**, double what this card assumed.
      (ii) Free *physical* memory is the wrong quantity entirely: error 1455 is
      the commit limit (RAM + pagefile), which fluctuates with whatever else the
      machine is running — which is exactly why the count differs run to run
      (1092/1188/2125/2127/2171) instead of being stable. The observed failure
      points correspond to 8.5-17.0 GiB of over-reserved VA.
  
      **Remaining work is now a bounded fix, not an investigation.** The test's
      real defect is that it cannot distinguish its two possible causes: it treats
      "first null" as "slot table full", when a null also arrives when the OS
      refuses. The existing counters do not separate them either (they count
      reservation *successes*; a lost-slot bug and an OS refusal both leave
      `live < MAX_SEGMENTS`). The principled fix is a diagnostic counter for
      FAILED OS segment reservations in `src/alloc_core/os.rs`, symmetric to the
      existing `segments_reserved_total` (one relaxed atomic on an already-cold
      OOM path), letting the test assert the ceiling when the allocator's own
      table refused, and report an explicit environment-limited skip when the OS
      did. **Status:** OPEN — mechanism settled, fix not yet implemented (it
      touches root-crate production source and was out of scope for the
      globalalloc-model round-5 task that reproduced it). **Evidence:** scratch
      probe output above (probe not committed — it only reads existing public
      `dbg_*` counters and can be rewritten from this card in a few lines);
      `wmic OS get FreePhysicalMemory,FreeVirtualMemory,TotalVirtualMemorySize`
      at reproduction time (17.4 GiB free physical, 41.3 GiB free commit —
      neither is what the failure is bounded by, per the correction above).

  **CLOSURE 2026-09-08 (task #1925).** Implemented exactly the fix this card's
  own "Remaining work is now a bounded fix" paragraph specified, plus one
  cause the card had not identified.

  1. **`os::SEGMENTS_RESERVE_FAILED_TOTAL`** — a third monotonic relaxed
     counter alongside the existing reserved/released pair, bumped at every
     `aligned_vmem::reserve_aligned{,_lazy}` call that returns `None` on a
     segment-reservation path: all five `os::Segment` constructors
     (`reserve`, `reserve_exact`, `reserve_capacity_exact`, `reserve_lazy`,
     `reserve_lazy_for_measurement`) and both `numa.rs` sites, which bypass
     `os::Segment::reserve` and so would otherwise under-count under
     `numa-aware` — the same bypass the existing `SEGMENTS_RESERVED_TOTAL`
     bump in that file already documents. Exposed as
     `AllocCore::dbg_segments_reserve_failed_total()` in the
     `internals`-gated diag block (a safe `pub fn` returning `u64`, taking no
     pointer — outside the R25-1 benchmark-hook hazard class).
  2. **Both tests read the counter's DELTA across their fill loop** and treat
     a short fill as environment-limited ONLY when the delta is non-zero. A
     fill that reaches the expected ceiling is asserted normally regardless of
     the counter, so the guard cannot silence its own assertion; and a genuine
     slot-bookkeeping regression — which stops short with ZERO refused
     reservations — still fails, now with the refusal count printed in the
     message so the two causes are distinguishable from the failure text
     alone.
  3. **`CEILING_LOCK` serializes the file's two full-ceiling fills.** Not in
     the card's plan: `AllocCore::table` is a per-INSTANCE field, so the two
     tests reach their ceilings independently, and libtest runs them
     concurrently by default — doubling the process's peak segment demand for
     no benefit, since neither test is about concurrency.

  **Measured at closure** (`cargo test --features "production internals"
  --test r14_7_max_segments_ceiling -- --nocapture`, this host): both tests
  stop at 2129 and 2134 of 4095 with exactly 1 refused reservation each, pass,
  and print the environment-limited notice. This also **corrects one
  expectation the fix was designed under**: serialization alone would NOT have
  fixed the flake — even a single instance cannot reach 4095 on this host
  right now, so the counter, not the lock, is what closes the item; the lock
  removes a real but insufficient contributor.

  **What the refusing constraint actually is on this host (2026-09-08).** The
  card above reasons about a system-wide commit budget fluctuating with other
  processes, which is what the original 1092/1188/2125/2127/2171 spread looked
  like. On the host where the fix was verified the operative constraint is
  narrower and deliberate: the session runs under an explicitly imposed
  per-agent memory quota, so other agents keep their own quotas. Six
  consecutive post-fix runs stopped at 2129 (×4), 2133 and 2134 — a spread of
  5 objects, versus ~1000 historically. That tightness is the signature of a
  fixed cap, not of ambient pressure. `ERROR_COMMITMENT_LIMIT` is what a job
  object's memory limit surfaces as too, so the error code alone does not
  distinguish the two. Recorded here because the distinction changes what a
  future reader should do: under a quota there is nothing to diagnose and
  nothing to free — the environment-limited branch is the correct and final
  outcome, not a symptom to chase.

  **Counterfactual run, not asserted:** forcing the observed delta to 0 (so
  the environment-limited branch cannot trigger) makes the test FAIL with
  "expected exactly MAX_SEGMENTS-1 (4095) ... got 2133 with 0 OS
  reservation(s) refused". The assertion is live, not vacuous — the refusal
  count is precisely what decides pass from fail.

  **Residual, deliberately accepted:** on a host that always refuses, these
  two tests assert nothing and say so on stderr (libtest has no stable "skip",
  so the notice is only visible under `--nocapture` or on failure). The
  ceiling guard's real coverage therefore lives in CI, where the fill
  completes. Slot-bookkeeping correctness itself remains independently covered
  by `tests/segment_table_recycle.rs`, which does not need the full ceiling.

- 12. **[T, filed 2026-08-02, task #498] `xthread_large_double_free_no_double_reclaim` reclaim-count undercount (50 expected, 42 observed)** — **CLOSED** 2026-08-06 (task #605/K10), by a fix that had already landed for a differently-described symptom (R34-14/task #533, commit `7ef5a46`). Re-verified still closed 2026-09-08 (task #1934) during the flake sweep. Full card as it stood at closure:

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

- 14. **[T, filed 2026-08-02, task #499] `xthread_large_free_tiny_size_huge_align_is_reclaimed` fails in-file, passes in isolation** — **CLOSED** 2026-09-08 (task #1933). The flake did not reproduce; the investigation instead found a worse defect in the same file and fixed it. The registry behaviour it exposed remains OPEN as item 145. Full card, including that task's own UPDATE section:

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

      **UPDATE 2026-09-08 (task #1933) — the flake did NOT reproduce, and
      looking for it found something worse.** First, the reproduction claim
      above no longer holds on this host: 20 in-file runs under `production
      internals` and 8 under `--all-features` all passed, with the test file
      functionally unchanged since this card was filed (`git log` shows only
      R34-3's `internals` cfg-gate edit and its rustfmt follow-up). Whatever
      made it fail 5/5 in August is not reproducible here, so the card's
      "reproducible on demand" property is withdrawn.

      Instead of stopping there, the `delta 0` failure shape was attacked
      directly: the message blames the mitigation for over-rejecting, but a
      delta of 0 has a second possible cause the test never excluded — the
      free not being cross-thread at all. Adding that missing
      path-activation oracle (`assert_ne!(remote_heap, owner_heap)`, the
      owner's address carried into the spawned thread as a `usize` since
      `*mut HeapCore` is not `Send`) showed the premise is violated
      SYSTEMATICALLY: `HeapRegistry::claim()` in the spawned thread returned
      the OWNER's own heap in **20 of 20 runs**. Every "cross-thread" free in
      this file was an ordinary own-thread free.

      The consequence differs per test and is worse for three of them. The
      two `is_reclaimed` tests passed for a reason other than the path they
      name. The three `is_dropped` tests — which assert `delta == 0` — were
      **vacuous**: a free that never enters the deferred path satisfies
      "delta == 0" no matter what `large_layout_consistent` decides, so they
      could not have failed even with the mitigation removed.

      **Fixed** by `claim_remote_distinct_from`, applied at all five spawn
      sites: claim until the returned heap is not the owner's. A colliding
      claim is deliberately never recycled — the registry only offered the
      owner's slot because that slot was on the free list while the owner was
      still using it, so re-claiming takes it back out of circulation;
      recycling it instead puts the owner's live heap back in the pool, which
      (measured during this task) drains the owner's deferred frees and makes
      `xthread_large_free_mismatched_layout_is_dropped` fail with
      `delta 1 != 0` for reasons unrelated to the mitigation. **Non-vacuity
      re-established by a run, not by argument:** with the remote free
      switched from `wrong_layout` to `real_layout`, that same test now FAILS
      with `delta 1 != 0` — the assertion is sensitive to the mitigation's
      decision again. All 5 tests pass 25/25 runs afterwards.

      **Status:** the flake itself is CLOSED-as-not-reproducible with the
      tests' real coverage restored; the registry behaviour it exposed is
      NOT closed and is filed separately as item 145 below.

- 96. **[T, filed 2026-08-23, task #1247] `wasted_dirty_drains_stays_low_under_class_aware_routing` waste-ratio threshold tripped by single-round sampling noise (26.7%, 26.7%, then 33.3% across three CI occurrences)** — **CLOSED** 2026-09-08 (task #1935). Fixed structurally on 2026-09-03 (`2b7cb87`, aggregate over 5 rounds); this task supplied the CI re-observation and margin measurement the card had set as its own closing trigger. Full card, including the three occurrences and the closing evidence:

  96. **[T, filed 2026-08-23, task #1247, mitigated 2026-08-30] `wasted_dirty_drains_stays_low_under_class_aware_routing`
      (`tests/class_aware_dirty_routing.rs`, around lines 680-709) failed once in CI on the F1-F4
      push (commit `7037a4c`), not reproduced on immediate rerun.** Failure:
      `assertion failed: ratio < 0.20` with the observed ratio at 26.7%
      (`ratio = wasted / drained` from `run_round(8)`) — the test's own threshold comment
      (lines ~695-701) already documents this as an accepted risk: "20% is a generous
      ceiling... tolerating real-world scheduler jitter and the rare cross-class same-segment
      carve." Confirmed transient, not a regression introduced by the F1-F4 push: the push's
      diff (tasks #1240/#1242/#1244/#1246, range `a1554aa..7037a4c`) never touches
      `tests/class_aware_dirty_routing.rs` or any file `class_aware_dirty_routing`'s own
      module doc names as production dependencies; `gh run rerun <run-id> --failed` on the
      same landing SHA came back 100% green across all 40 non-skipped jobs, including the one
      that had failed. Filed per this file's own convention (see item 12's identical shape —
      one CI-observed failure, confirmed non-reproducible, recorded as a data point) so a
      repeat occurrence has this one on record rather than being independently re-diagnosed
      from scratch. Not investigated further here — the test's own threshold already accepts
      this failure class; tightening it or replacing the ratio-threshold approach entirely is
      out of scope for a filing task.

      **Second occurrence (2026-08-30, `test (feature isolation)` job, commit `296628a`):**
      same test, same shape — `ratio < 0.20` tripped at 26.7% again, not reproduced on an
      immediate `gh run rerun --failed` of the same commit (100% green, including Kani).
      Unrelated to the landing commit's actual diff (`crates/tagged-index-stack/**` +
      two new CI steps in `.github/workflows/ci.yml`, neither touching this test or its
      production dependencies) — confirmed by reading the test's own code before accepting
      the rerun-green signal at face value, not just trusting the precedent. **Mitigated**
      (not fully eliminated — a true 30%+ CI-jitter spike remains theoretically possible)
      by owner request: the assertion now uses a **dual threshold** — `0.30` when the
      standard `CI` environment variable is set (GitHub Actions, and effectively every other
      CI provider, sets `CI=true`), `0.20` otherwise, so a local `cargo test` run keeps the
      original tighter bound while CI gets more headroom against exactly the shared-runner
      jitter both occurrences exhibited. See the commit that added this for the exact diff.

      **Third occurrence (2026-09-03, `test (feature isolation)` job, commit `0310fdb`):**
      same test, now tripping the WIDENED 30% CI ceiling itself — observed ratio 33.3%
      (`drained=15, wasted=5`). Unrelated to the landing commit's diff (a checkpoint-doc-only
      commit on top of `crates/tagged-index-stack/**` + doc-comment-only changes in
      `src/registry/heap_registry.rs`/`bootstrap.rs`/`src/lib.rs`, none touching
      `class_aware_dirty` machinery — confirmed by reading `git log`/`git diff` over the
      relevant range before accepting this as transient). Rather than widening the threshold a
      third time (masking the same single-sample noise floor, not addressing it), fixed the
      real mechanism: a single `run_round(8)` has a small denominator (~15 drains), so one
      scheduler-jitter-induced extra wasted drain moves the ratio by ~6.7 points — exactly the
      granularity both prior occurrences and this one show. The test now runs 5 independent
      rounds and asserts the threshold against the AGGREGATE ratio (same accepted 0.20/0.30
      thresholds, unchanged, now applied to a ~5x larger and materially less noisy
      denominator) — this can only reduce the false-positive flake rate, never raise it, since
      a real regression to ~95% waste would read ~95% in every round and therefore in the
      aggregate too. Verified locally: 4 consecutive runs of the fixed test all read 0.0%
      (drained_total in the 48-92 range across runs). Left as **[T]**, not closed — a
      structural fix, but CI has not yet re-observed this test post-fix across enough runs to
      call the flake class eliminated.

      **CLOSURE 2026-09-08 (task #1935) — the closing trigger this card set for
      itself is now satisfied, measured rather than assumed.** Two independent
      lines of evidence.

      (1) *CI has now re-observed it.* Of the 25 most recent `CI`-workflow runs
      (`gh run list --workflow=CI --limit 25`), **17 are descendants of the fix
      commit `2b7cb87`** (checked per-run with `git merge-base --is-ancestor
      2b7cb87 <headSha>`, not by date). Exactly one `test (feature isolation)`
      failure appears anywhere in that window — run `33799336046` on `0310fdb`
      — and `0310fdb` is an ANCESTOR of `2b7cb87`, i.e. it is the third
      occurrence that PROMPTED the fix, not a recurrence after it. Two of the
      post-fix runs that are red for other reasons (`34023419181`,
      `34016090306` — a Windows scratch-root test trio and, in
      `34049805079`, `readme_unsafe_inventory_counts_match_reality`) log this
      test explicitly as `wasted_dirty_drains_stays_low_under_class_aware_routing
      ... ok`, so it is being exercised and passing, not skipped.

      (2) *The margin is now quantified, not just the verdict.* 12 consecutive
      local runs (`--features "production internals alloc-stats"`, reading the
      test's own `eprintln!` rather than its pass/fail) give
      `drained_total` 48-73 and a waste ratio of **0.0-4.4%** (0.0% in 8 of 12;
      highest single reading 3/68). The threshold is 20% locally / 30% in CI, so
      the worst observed reading sits at roughly a fifth of the local bound —
      against a pre-fix regime where a single round's 26.7% and 33.3% readings
      tripped those same bounds outright. Reaching 20% at the observed
      denominators would take ~10 wasted drains where the highest seen is 3.

      Status: **CLOSED**. This is the aggregate fix working as designed; no
      further change. A future re-occurrence should be filed as a NEW item with
      its own numbers rather than reopening this one, since the mechanism this
      card describes (single-round granularity of ~6.7 points per wasted drain)
      no longer exists.

- **Flake-closure gate (task #1937, 2026-09-08).** Not an item — the closing
  verification for the flake sweep that resolved items 12, 14, 96, 143 and
  filed 145/146. Recorded here because CLAUDE.md's pre-push rule makes
  `npm run check`'s usability a standing concern, and this session began with
  it unusable.

  **Starting state:** `npm run check` fail-fasted on the `r14_7` flake before
  reaching roughly half its steps. That is not a cosmetic annoyance — it is
  how commit `f66b9f0`'s R30-12 prefix defect reached `origin/main`: the gate
  aborted before `verify-commit-prefixes` ever ran.

  **Result:** `npm run check` now completes end to end, **53 steps, all OK,
  exit 0** — including `verify-commit-prefixes`, the step the flake had been
  hiding. Full-suite evidence, three independent observations, all with
  `--no-fail-fast` so a single failure could not mask the rest:
  - `cargo test --features "production internals" --no-fail-fast` — 254 test
    binaries, 0 failures, cargo exit 0.
  - `cargo test --all-features --no-fail-fast` — 254 binaries, 0 failures,
    cargo exit 0. This row matters separately: it is the only one that builds
    `large-cache-extended`, and it is what caught the second oversized-ladder
    file (`large_cache_extended_narrow_working_set_after_materialization.rs`,
    commit `d47a91e`).
  - `npm run check`'s own four test rows (`production internals`;
    `production alloc-stats bench-internals internals`; `pinning`;
    `--all-features`), all OK.

  **What this does NOT claim.** "Not observed across these runs" is not "the
  flake class is eliminated". Two mechanisms found during the sweep remain
  genuinely unexplained and are open as items 145 and 146; four sibling test
  files still carry the oversized ladder unpatched (listed in item 146). The
  gate is usable again, which was the goal — the underlying questions are
  tracked, not closed by a green run.
