# Checkpoint — 2026-08-14 [aligned-vmem-round2-fix-pass-inflight]

## Session summary

This session continues a long-running `aligned-vmem` pre-release review campaign (this is the crate's ~13th review round overall across this and prior sessions). Picking up from `docs/checkpoints/2026-08-13-2100.md`: since that checkpoint, this session completed **round 11** (a deliberately blind, from-scratch pre-release review — the reviewer was given no history, no round count, no prior findings — found 24 actionable findings V-1 through V-39, fixed via 6 parallel `/crush` worktree tasks, merged as commits `ba77a43`..`3f496b0`), then a **closing review of round 11** (`docs/reviews/2026-08-14-aligned-vmem-round11-closing-review.md`, 10 findings C-1..C-10 including one HIGH/BLOCKING test that would have panicked on Linux CI, fixed via 7 parallel tasks merged `44ba1a4`..`498bd65`), then a **closing review of THAT closing review** (`docs/reviews/2026-08-14-aligned-vmem-round11cr-closing-review.md`, 12 findings R-1..R-12, all LOW/INFO — the pattern had converged to doc/index/comment-only issues by this third nesting level, no more code bugs — fixed via 5 parallel tasks merged into `9fdd098`, final commit `6f94f89`). Both closing-review fix passes caught real defects during MY OWN zero-trust review before merging — most notably in the round11cr pass: a delegate's own edit corrupted a Windows path literal (embedded a literal CR byte via backslash-escape mangling) and structurally broke item 1's multi-line header by inserting a relocated item (55) into the wrong physical location — both caught by direct byte-level file inspection (`xxd`, `cat -A`) and fixed with `head`/`tail`-based file reconstruction (`sed -i`/`perl -pe` inexplicably failed to apply on this Windows/Git-Bash environment for reasons not fully diagnosed — `printf '%s\n' <content>` as a separate arg, avoiding printf's own format-string escape processing, was what finally worked).

After `6f94f89` landed (repo clean, matching `origin/main`... **not yet pushed this session**, verify before assuming), the user asked for a SECOND independent blind review (`@oxx`, again with zero campaign-history context — this time explicitly forbidding the reviewer from reading `docs/CORRECTNESS_OPEN_ITEMS.md`/`docs/perf/OPEN_ITEMS.md`/`docs/reviews/`/git history at all, stricter than round 11's isolation). It produced `docs/reviews/2026-08-14-aligned-vmem-pre-release-review-round2.md` — and this time found a GENUINE new HIGH-severity bug that survived all three prior rounds: **W-1** — on Windows, `Reservation::is_huge()` returns `true` after a `MEM_LARGE_PAGES` request fails and the ordinary-page retry succeeds (`win_reserve_commit`'s single-call fast path derives `granted_huge` from the ORIGINAL requested flags, not from which VirtualAlloc call actually produced the returned pointer — the Unix sibling function gets this right, Windows didn't). The reviewer empirically reproduced it on real Windows hardware via a scratch crate (since deleted). I personally re-verified the bug by reading the code myself before accepting it. The review also found ~25 more findings (P-1 perf: the Unix exact-size fast path is a proven net syscall LOSS on 64-bit by the crate's own already-written-down numbers; M-1: `mock::record` holds a `RefMut` across an allocating `Vec::push`, a reentrancy panic hazard for `GlobalAlloc` consumers; F-1: `fault-injection` feature is silently inert without `lazy-commit`; U-1: BSD decommit is an undocumented no-op; A-1/A-2/A-3/G-1/G-2/T-1..T-5/D-3/D-4/P-4 and more — see the review doc for full detail).

User said "давай всё исправим" (let's fix everything). I loaded the `rust-intel` skill for reference (unsafe/FFI/reentrancy discipline, relevant given W-1 and M-1 are exactly its target failure classes), then decomposed the ~28 actionable findings into **7 parallel `/crush` worktree tasks (A-G, TaskList #943-949) + 1 merge/verify/changelog task (H, #950, blocked on A-G)**:
- **A (#943, HIGH priority)** — W-1 + W-2 (Windows `win_reserve_commit`: fix the `granted_huge` tracking bug + delete a now-dead duplicate retry branch).
- **B (#944)** — U-1 (BSD `MADV_FREE` decommit fix) + U-2 (Android `MAP_ANON` support) + U-3 (symmetry debug_assert) + **P-1 (real production-behavior change: gate the Unix exact-size fast path to `#[cfg(target_pointer_width = "32")]` only, removing it from 64-bit entirely)** — this is the one task in this round making a real default-behavior change, justified as deterministic syscall-counting math (not needing empirical A/B) rather than the usual project convention of deferring unmeasured perf changes.
- **C (#945)** — M-1 (mock reentrancy guard) + M-2 (TLS-teardown panic fix, `try_with`).
- **D (#946)** — F-1 (`fault-injection = ["lazy-commit"]` in Cargo.toml) + G-2 (checked `u32→i32` cast in error.rs).
- **E (#947)** — A-1 (fix `recommit`/`commit_range` to validate against `page_size()` not `PAGE`) + A-2 (NEW: safe `Reservation::decommit`/`recommit`/`commit_range` methods, additive API) + A-3 (delete deprecated `is_empty`) + G-1 (harden `release()`'s miri-path panic to match `from_raw_parts`'s diagnostic quality).
- **F (#948)** — D-3/D-4 (stale doc citations, 2 of 3 — the third is inside B's rewritten paragraph, explicitly excluded) + P-4 (`#[inline]` on `page_size()`) + G-3pt5 (one README sentence).
- **G (#949)** — T-1..T-5 (five new tests; T-2 specifically is written to assert the POST-W-1-fix behavior and is expected/allowed to fail in G's own isolated worktree since it doesn't have A's fix — this is intentional, matching the established pattern from round 11 where cross-task test dependencies were resolved at merge time).

**Deliberately deferred, not implemented this round** (recorded for H's CHANGELOG entry): P-2 (mmap address hint — needs its own measurement), P-3 (Windows `VirtualAlloc2` — needs a portability decision, matches the established round-3 "Task H" design-note precedent of not implementing unmeasured Windows API additions), D-5 (doc-comment consolidation — cosmetic, risk of new drift), G-3pt4 (the `mock`→`--cfg` decision — already tracked as item 42, a maintainer call, not re-litigated).

All 7 worktrees (`D:/dev/rust/sefer-alloc-vmem-r2-{a..g}`, branches `vmem-r2-{a..g}` off `main` at `6f94f89`) were created and all 7 `/crush` sessions launched in background (session IDs match branch names) **immediately before this checkpoint was requested — NONE have completed or been reviewed yet.** This checkpoint is being written mid-flight, before any of the 7 tasks' diffs have been zero-trust-reviewed or merged.

## Active goal

None — no `/goal` Stop hook is armed in this session.

## TaskList

### in_progress
- #657 numa-shim — verify/prepare for crates.io republish (blockedBy: #658)
- #658 aligned-vmem — publish 0.2.0 (blockedBy: #842, #848, #849 — note: these blocker IDs are from an OLDER phase of this campaign, likely stale; re-verify against current state before trusting)
- #659 racy-ptr-cell — first publish to crates.io
- #660 size-classes — first publish to crates.io
- #661 tagged-index-stack — first publish to crates.io
- #943 aligned-vmem R2 A: W-1(HIGH)+W-2 — fix Windows is_huge() false-positive bug + dead retry branch — **`/crush` running in background, worktree `vmem-r2-a`, NOT YET REVIEWED**
- #944 aligned-vmem R2 B: U-1+U-2+U-3+P-1 — BSD decommit fix, Android support, alignment symmetry, gate exact-size fast path to 32-bit — **`/crush` running in background, worktree `vmem-r2-b`, NOT YET REVIEWED**
- #945 aligned-vmem R2 C: M-1+M-2 — fix mock::record reentrancy panic + TLS-teardown panic — **`/crush` running in background, worktree `vmem-r2-c`, NOT YET REVIEWED**
- #946 aligned-vmem R2 D: F-1+G-2 — fault-injection manifest fix + unchecked u32-to-i32 cast fix — **`/crush` running in background, worktree `vmem-r2-d`, NOT YET REVIEWED**
- #947 aligned-vmem R2 E: A-1(code)+A-2+A-3+G-1 — fix recommit granularity, add safe Reservation methods, delete deprecated is_empty, harden release() — **`/crush` running in background, worktree `vmem-r2-e`, NOT YET REVIEWED**
- #948 aligned-vmem R2 F: D-3+D-4(2 sites)+P-4+G-3pt5 — doc citation fixes, counter-doc clarification, #[inline] on page_size, README sentence — **`/crush` running in background, worktree `vmem-r2-f`, NOT YET REVIEWED**
- #949 aligned-vmem R2 G: T-1+T-2+T-3+T-4+T-5 — five test-coverage gaps — **`/crush` running in background, worktree `vmem-r2-g`, NOT YET REVIEWED**

### pending
- #662 design note for bench-scale-tool alongside criterion/iai
- #763 implement bench-scale-tool per approved design (blockedBy: #662)
- #950 aligned-vmem R2 H: merge all A-G, full verification matrix, record deferred items, CHANGELOG entry, commit (no push) (blockedBy: #943, #944, #945, #946, #947, #948, #949)

### recently completed
- #937-942 aligned-vmem round-11-closing-review's own fix pass (R-1..R-12), merged, CHANGELOG written, closing review doc committed at `6f94f89`
- #928-936 aligned-vmem round-11 closing-review fix pass (C-1..C-10), merged into `498bd65`
- #920-927 aligned-vmem round 11 (V-1..V-39 blind review fix pass), merged into `3f496b0`

## Decisions

- **Deliberately implementing P-1 (removing the Unix exact-size fast path on 64-bit) this round**, breaking from this session's earlier pattern of deferring unmeasured production-behavior changes — justified because the math is DETERMINISTIC syscall-counting (`3-2p > 1` for every hit rate `p<1`), not something needing empirical A/B validation the way a real perf claim would; removing a provably-always-pessimizing path cannot make things worse on 64-bit.
- **P-2 and P-3 (the two OTHER perf opportunities the review found) are explicitly NOT being implemented this round** — both genuinely need either a fresh measurement (P-2, an mmap address hint) or a real portability/API-surface decision (P-3, `VirtualAlloc2`), matching the established round-3 "Task H" precedent of writing these up as deferred rather than rushing an unmeasured Windows API addition.
- **A-2 (new safe `Reservation::decommit`/`recommit`/`commit_range` methods) is a real, additive public-API surface decision**, delegated to task E with explicit design guidance (bounds-check against `self.len()`, mirror the free functions' existing behavior-on-violation contracts) rather than fully specified by me — will need extra scrutiny at merge time since this is new public API, not just a bug fix.
- **Task G's T-2 test is deliberately written to potentially FAIL in its own isolated worktree** (it asserts the post-W-1-fix behavior, but worktree G doesn't have task A's fix) — this is intentional and expected, matching the established cross-task-dependency pattern from round 11; verification of T-2 passing only happens meaningfully after A and G are BOTH merged into main.
- **Loaded the `rust-intel` skill this turn** before decomposing the fix tasks, given W-1 (Windows FFI/unsafe-adjacent logic bug) and M-1 (thread-local reentrancy hazard) both fall squarely in that skill's target failure classes (§B18/B25 FFI, §B17/reentrancy-adjacent concurrency).

## Open questions

- **Whether `aligned-vmem` 0.2.0 is finally ready to publish (task #658)** remains unresolved and gated on maintainer go-ahead — not decided this session, and now even further out since round 2's fixes (especially the new HIGH bug W-1 and the P-1 behavior change) are still in-flight and unreviewed as of this checkpoint.
- **The `mock`→`--cfg` feature-unification decision** (tracked as `docs/CORRECTNESS_OPEN_ITEMS.md` item 42, flagged URGENT in the round-11-closing-review-fix-pass since its own stated deadline "before 0.2.0 ships" has now arrived) — still unresolved, a maintainer call, independently re-surfaced by round 2's review (G-3 point 4) but not re-litigated.
- **Whether main has been pushed since `6f94f89`** — NOT verified in this checkpoint; the user has not asked for a push this session as of the last completed round. Confirm `git fetch && git rev-parse origin/main` before assuming local state matches remote.
- **Immediate next step after this checkpoint**: wait for the 7 `/crush` background tasks (#943-949) to report completion via task-notifications, zero-trust-review each diff personally (per this project's CLAUDE.md "an agent's statement is a claim, not a receipt" rule — especially critical for task A's HIGH-severity bug fix and task B's production-behavior change), merge in a sensible order, run the full verification matrix, then execute task H (#950) — write the CHANGELOG entry, commit (no push unless separately asked). A third-level closing review may or may not be warranted afterward depending on what zero-trust review finds; not pre-decided.

## Repo state

```
?? docs/checkpoints/2026-08-13-2100.md
?? docs/reviews/2026-08-14-aligned-vmem-pre-release-review-round2.md
```

```
6f94f89 docs(vmem): round-11-closing-review's own fix pass CHANGELOG entry + commit review doc
9fdd098 Merge vmem-r11cr2-e (task #941): comprehensive docs/CORRECTNESS_OPEN_ITEMS.md fixes — reopened item 42's closure trail (R-1), re-verified stale citations in items 52-54 (R-2), corrected item 53's misattributions (R-3), moved items 11/13 into [A] (R-9), relocated the mis-filed sefer-region item 42 into [T] as item 55 (R-10)
b65aabb Merge vmem-r11cr2-d (task #940): CHANGELOG.md wording corrections — profile-qualified counterfactual claim (R-7), record C-7's deliberate non-action (R-8), correct the Linux cross-compile evidence claim (R-11)
6371585 Merge vmem-r11cr2-c (task #939): fix out-of-bounds pointer arithmetic in the new C-4 negative test (R-6)
eb9a68d Merge vmem-r11cr2-b (task #938): restore two dropped reservation_len invariants in from_raw_parts's Safety section (R-5)
```

Note: `main` at `6f94f89` is a clean working tree from `git`'s perspective (only the two untracked files above), but **7 separate git worktrees exist right now** (`D:/dev/rust/sefer-alloc-vmem-r2-{a..g}`) each with an in-flight `/crush` session mutating files — none of that work is visible in `main`'s status above until merged. Do not assume `main`'s clean status means "nothing in flight."
