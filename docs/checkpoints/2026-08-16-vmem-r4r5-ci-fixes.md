# Checkpoint — 2026-08-16 [vmem-r4r5-ci-fixes]

## Session summary

Continuation of the long-running `aligned-vmem` pre-release review campaign. This session executed round 4 (pre-publish audit, tasks #959-961: 4 LOW/INFO findings — a 64-bit-disabled example, undocumented mock cross-thread drop semantics, a manual `Default` impl replaceable by `#[derive]`, a second doomed-syscall class added to PERF-1) and round 5 (tasks #962-963: converted `mock` from a Cargo feature to the build-time `--cfg aligned_vmem_mock` flag, closing `docs/CORRECTNESS_OPEN_ITEMS.md` item 42 — the user's explicit decision: "делаем 2, т.к 1 нельзя делать по любому", i.e. convert now rather than merely document the Cargo-feature-unification hazard). Both rounds used `/crush` sessions running directly in the main working tree (not isolated worktrees this time), each personally zero-trust reviewed diff-by-diff before commit, each verified with a full local matrix plus a fresh `npm run check` (ALL GREEN both times).

After round 5's push (`74aab3c..71cfa98`), CI's `test macos (production)` job failed — the FIRST real macOS CI run since task #947/A-1 (round 2, several rounds ago) changed `commit_range`/`recommit` boundary validation from the compile-time `PAGE` constant (4096) to the runtime `page_size()` (16384 on Apple Silicon real hardware). This exposed a latent bug class: several tests across `tests/lazy_commit.rs`, `tests/fault_injection.rs`, and `tests/smoke.rs` hardcoded `PAGE` as a decommit/recommit/commit_range boundary, which fails validation unconditionally on any 16-KiB-page host. Fixed directly (commit `106a788`) and pushed again (`71cfa98..71cfa98`... actually the push that included this fix was `74aab3c..71cfa98`, containing rounds 4+5 AND the first CI fix — see repo state below for the precise commit list). **That second push ALSO failed CI**, on the SAME job, because the first fix only addressed what the FIRST failing CI run happened to surface — a retry-loop bug in `decommit_recommit_roundtrip_on_over_reserved_span` was masking a SECOND bug in the exact same test (a hardcoded `size = PAGE` used as the decommit/recommit boundary, unreached until the retry-loop bug was fixed). A methodical full audit (grep every `decommit`/`recommit`/`commit_range` call site in `crates/vmem/tests/*.rs`, checked each PAGE-usage for whether it's a real success-path boundary vs. a deliberate-rejection test vs. pure data construction) found and fixed THREE MORE live instances, all in `tests/mock.rs` (which had never been exercised by any macOS CI run before, since the mock-vs-real backend distinction is orthogonal to when this bug class was introduced): `fail_next_commit_injects_recommit_failure`, `fail_next_commit_injects_commit_range_failure`, `simulated_fault_reports_no_os_code`, and `reentrancy_guard_silently_drops_nested_calls`. Fixed (commit `667e6f2`), verified locally (cannot runtime-verify the macOS-specific page_size()=16384 branch on this Windows dev host — this still needs real CI confirmation).

**Separately, the user reported `npm run bench:table` failing** with `error: target 'global_alloc' ... requires the features: 'alloc-global', 'internals', 'bench-internals'`. Investigated and confirmed this is a PRE-EXISTING bug, ~11 days old, entirely unrelated to this session's own work: `scripts/bench-table.mjs` has hardcoded `FEATURES = 'production'` since its creation (`73a6b2b`, 2026-07-07), but task #583 (`7a9b7c7`, 2026-08-05) added `internals`/`bench-internals` to `benches/global_alloc.rs`'s own `required-features` without updating the script. Reproduced identically against a detached worktree at `d1de3bc` (the base commit this entire multi-round campaign started from) to prove it predates everything done this session. Fixed (`FEATURES = 'production internals bench-internals'`, both diagnostic-only/zero-runtime-cost features), verified end-to-end (`npm run bench:table` completes, 159 bench ids parsed, all 51 expected present), filed and closed as `docs/CORRECTNESS_OPEN_ITEMS.md` item 57. Commit `614197f`.

**Current state at interruption:** `npm run check` was re-running (in background, task `bj7u0xjle`) to verify the two new fix commits (`667e6f2`, `614197f`) before a third push attempt, when the user interrupted with `/checkpoint`. The full gate's result is UNKNOWN — it was not observed to completion. **Nothing beyond `71cfa98` has been pushed.** The previous push's CI run (`31927050497`, landing SHA `71cfa98`) is confirmed FAILED (`test macos (production)`, the same job, same underlying bug class, now believed fixed by `667e6f2`).

## Active goal

None — no `/goal` Stop hook armed.

## TaskList

### in_progress
(none — all vmem-campaign tasks through round 5 are completed; the CI-fix work just done was NOT tracked as separate TaskList items, done ad-hoc in response to user reports)

### pending
- #662 Root sefer-alloc: design note for applying bench-scale-tool alongside criterion/iai
- #763 Root sefer-alloc: implement bench-scale-tool per the approved design (blockedBy: #662)

### recently completed (round 4/5 of the vmem campaign)
- #963 aligned-vmem R5 close: zero-trust review of mock→--cfg conversion, ci.yml verification, item 42 closure, CHANGELOG
- #962 aligned-vmem R5: convert mock from Cargo feature to --cfg flag (item 42, decision made)
- #961 aligned-vmem R4 C: merge/close — zero-trust review of both diffs, verification, CHANGELOG
- #960 aligned-vmem R4 B: audit findings 3+4 — derive(Default), PERF-1 second doomed-syscall class
- #959 aligned-vmem R4 A: audit findings 1+2 — disabled-path example, mock cross-thread semantics

(#657-661 numa-shim/racy-ptr-cell/size-classes/tagged-index-stack publish-prep tasks remain `in_progress` from before this session, untouched)

## Decisions

- **Converted `mock` from a Cargo feature to a `--cfg aligned_vmem_mock` build flag** (task #962) rather than leaving the hazard merely documented — explicit user decision this session, made because the crate has never published and the conversion window ("free only until first publish") was about to close.
- **Fixed the macOS CI regressions directly, not via re-delegation** — small, well-understood, mechanically verifiable fixes (swap `PAGE` for runtime `page_size()` at specific call sites), matching this session's established pattern for CI-breaking bugs.
- **Did a full methodical audit of every decommit/recommit/commit_range call site** (not just fixing what one CI run happened to surface) after the SECOND CI failure revealed the first fix pass had been incomplete — found 3 more live instances in `tests/mock.rs` this way, pre-emptively, before a third CI run could have surfaced them one at a time.
- **Treated `npm run bench:table`'s failure as a genuinely separate, pre-existing bug** rather than assuming it was caused by this session's work — verified via a detached worktree at the pre-session base commit before writing any fix, per this project's standing "verify, don't guess" discipline.
- **Filed the bench-table bug as a new, immediately-closed item (57) in `docs/CORRECTNESS_OPEN_ITEMS.md`** rather than a silent fix, per this repo's "flag from any source" convention.

## Open questions

- **Does `667e6f2` (the second round of PAGE-vs-page_size() fixes) actually turn `test macos (production)` green?** Not yet confirmed — this session's fixes were verified as thoroughly as possible on a Windows host (full local test matrix, cross-compile checks, careful manual derivation of the macOS-specific code paths) but the macOS-specific runtime behavior (`page_size() == 16384`) cannot be executed locally. A real CI run against the next pushed SHA is the only way to close this out. If it fails AGAIN, the methodology should probably shift from "grep and fix what's found" to writing a small standalone diagnostic (e.g., a scratch test or example forcing a simulated 16 KiB `page_size()`) to enumerate ALL affected call sites deterministically rather than relying on manual code reading.
- **Was `npm run check` (background task `bj7u0xjle`) green?** Unknown — interrupted before completion. Must be re-run (or its result checked, if it finished in the background before the interrupt — check `TaskOutput` for `bj7u0xjle` first) before the next push.
- **Should the next push happen now, or does the user want to review the diffs first?** Not asked yet this checkpoint — the session was mid-verification when interrupted.

## Repo state

```
?? docs/checkpoints/2026-08-13-2100.md
?? docs/checkpoints/2026-08-14-vmem-r2-complete.md
?? docs/checkpoints/2026-08-14-vmem-r2-inflight.md
```

```
614197f fix(scripts): bench-table.mjs missing internals/bench-internals for global_alloc
667e6f2 fix(vmem): close remaining PAGE-vs-page_size() instances found by re-verification
71cfa98 docs(vmem): round 5 CHANGELOG entry + partial closure of item 42
18c29e4 feat(vmem)!: convert mock from a Cargo feature to a --cfg build flag (item 42)
3276ad4 docs(vmem): round 4 CHANGELOG entry (audit findings + the macOS CI regression fix)
b812611 refactor(vmem): derive(Default) for SystemInfo, document PERF-1's second doomed-syscall class (task #960)
2d86fcf docs(vmem): audit findings 1+2 -- flag the 64-bit-disabled example path, document mock's cross-thread drop split (task #959)
106a788 fix(vmem): fix macOS CI failures caused by task #947/A-1's page_size() granularity change
```

**Push status:** `origin/main` is at `71cfa98` (confirmed via `gh run` lookup on that SHA — CI run `31927050497` completed with conclusion `failure`, job `test macos (production)`). Local `main` is 2 commits ahead (`667e6f2`, `614197f`), both believed to fix the failure but NOT YET PUSHED and NOT YET independently re-verified via `npm run check`'s full completion (the run was in flight when this checkpoint was taken). No worktrees or `/crush` sessions currently in flight (this round's two `/crush` sessions — `vmem-r4-a`/`vmem-r4-b` for round 4, `vmem-r5` for round 5 — all completed and were merged directly into the main working tree, no isolated worktree cleanup needed this time).
