# Checkpoint — 2026-08-11 [region-no-consumer-decision-mid-835]

## Session summary

This session's earlier portion (already checkpointed separately) closed the entire `sefer-region` F1-F13+perf remediation campaign (tasks #813-#832), including a closing `@oh` review that found and fixed 9 real findings. After that closed, the user asked a series of clarifying questions in the current window: why `main`'s CI was red (found and fixed a real, 7-commit-old dead-code bug in `--no-default-features` clippy — task done, commit `d434b64`), what `sefer-region` is for and how 0.2.0 differs from 0.1.0, who would actually use it, and — critically — **why sefer-alloc's own runtime doesn't use it**. That last question led to a real architectural finding: `docs/ALLOC_PLAN.md`'s original design was "one substrate, two faces" (the handle-store and the `GlobalAlloc` face were meant to share the same governed memory), but this was never built — `Region<T>`/`Handle<T>` shipped as an independent wrapper over third-party `slotmap`, sharing no memory/mechanism with the real segment/magazine allocator. This was already honestly flagged by a prior audit (F5, corrected 2026-08-09) but the user's question surfaced it fresh.

The user then asked whether there's other similarly-unused ("dead") code elsewhere in the workspace worth worrying about — I empirically checked all 10 workspace members (`vmem`, `numa`, `malloc-bench`, `region`, `racy-ptr-cell`, `size-classes`, `globalalloc-model`, `tagged-index-stack`, `proc-memstat`, `proc-probe`) via grep for real call sites (not just `Cargo.toml` declarations), and confirmed `sefer-region` is the ONLY one with zero internal consumers — every other crate is genuinely wired into either the production allocator hot path (behind its feature flag) or legitimately used dev/test/bench/fuzz infrastructure.

Based on this, the user made an explicit decision: **stop investing further API-polish effort into `sefer-region`** beyond what's already committed, since it has no internal consumer and no known external one. This reversed an earlier plan (from before this checkpoint's window) to decide 4 "pre-freeze" API questions (Q3 handle-yielding iteration/retain, Q4 error-type shape/`#[non_exhaustive]`, Q5 `SyncRegion` fallible constructors, Q13 `try_insert`) from a code-quality review (`docs/reviews/2026-08-11-sefer-region-code-quality-review.md`, 23 findings total: 1 HIGH already fixed as Q1/d434b64, 6 MEDIUM, 10 LOW, 6 INFO). Tasks #833 and #834 (which had been created to make those 4 decisions) were deleted per this reversal. Two new tasks were created instead: **#835** (mark "no internal consumer, not under active investment" in README/lib.rs/ALLOC_PLAN.md/an open-items index/the code-quality-review itself/CHANGELOG — docs-only, no code/API change) and **#836** (fix the 18 non-blocking findings — Q2, Q6-Q12, Q14-Q23 — in one batch, since those are docs-only/dedup/hygiene and don't touch the frozen-API question).

The user then invoked `/babygoal` with "давай решим оставшиеся таски по нему" (let's solve the remaining tasks for it). Domain was already fully understood from this session; strategy chosen was sequential `/crush` delegation (the established pattern for this entire campaign). `/babysit` was already armed from earlier in the session (cron `a2620bce`, 15-min interval, session-only) — confirmed still active via `CronList`. Task #835 was marked `in_progress` and a `/crush` session launched (session id `region-mark-no-consumer`, background task `bj17a8ufe`) with a detailed prompt covering all 6 doc-marking locations. This checkpoint is being written WHILE that `/crush` session is still running — at the time of writing, `git status --short` shows `README.md`, `src/lib.rs`, and `docs/ALLOC_PLAN.md` already modified (matching the prompt's items 1-3); items 4 (open-items index), 5 (code-quality-review closing note), and 6 (CHANGELOG entry) may or may not be done yet — NOT personally verified or committed at checkpoint time. The delegated diff has NOT yet been zero-trust reviewed by me.

Task #836 (the 18-findings batch) has not been started yet — it is next after #835 lands and is personally verified/committed.

## Active goal

A session-scoped `/goal` Stop hook is active with condition: **"давай решим оставшиеся таски по нему"** (let's solve the remaining tasks for it — referring to sefer-region's remaining tasks #835/#836). This will auto-clear once #835 and #836 are both done; no manual `/goal clear` needed.

## TaskList

### in_progress
- #656 sefer-region — verify/prepare for crates.io republish (blocked in practice on #801/Stage E, itself now blocked on user's not-yet-made 0.2.0 publish decision)
- #657 numa-shim — verify/prepare for crates.io republish (blockedBy: #658)
- #658 aligned-vmem — publish 0.2.0 (local already bumped, crates.io still shows 0.1.0)
- #659 racy-ptr-cell — first publish to crates.io
- #660 size-classes — first publish to crates.io
- #661 tagged-index-stack — first publish to crates.io
- #835 sefer-region: mark "no internal consumer, not under active investment" everywhere relevant — `/crush` session running now (bj17a8ufe), NOT yet personally verified/committed

### pending
- #662 Root sefer-alloc: design note for applying bench-scale-tool alongside criterion/iai (awaiting user sign-off, unrelated to this thread)
- #763 Root sefer-alloc: implement bench-scale-tool per approved design (blockedBy: #662)
- #801 sefer-region: Stage E — final release matrix + version bump + tag (blockedBy: everything above; ALSO now gated on an explicit user decision on whether to actually cut 0.2.0 at all, given the "no internal consumer" finding — this was left as an open question, not yet answered)
- #836 sefer-region: fix 18 non-blocking code-quality findings in one batch (Q2, Q6-Q12, Q14-Q23) — next after #835 lands; NOT started

### recently completed (most recent 10)
- #832 sefer-region: run @oh final review of the F1-F13+perf round — 9 findings, all fixed
- Q1 fix (no TaskList id — found during a follow-up `@oh` code-quality review, not a numbered task): red `main` CI (`cargo clippy --no-default-features` failing 7 commits, since `1bfbb7e`/#822) — fixed, commit `d434b64`
- #831 sefer-region: commit all markdown docs from this round
- #830 sefer-region: update CHANGELOG.md with the F1-F13+perf round
- #829 sefer-region: /checkpoint after the full campaign lands
- #828 sefer-region E2: measure the three structural perf levers
- #827 sefer-region E1/P-perf-3: rebuild the Region::new() contention judge
- #826 sefer-region P2/F13: seven small smells and cleanup residuals
- #825 sefer-region P2/F11: add try_with_capacity/try_reserve
- #824 sefer-region P2/F10: reentrancy + poison policy

### deleted this window
- #833 (was: Q4+Q13 error-taxonomy pre-freeze decision) — deleted per user's "stop polishing" decision
- #834 (was: Q3 iteration-shape pre-freeze decision) — deleted per user's "stop polishing" decision
- #673 (older, unrelated placeholder) — deleted earlier this window per explicit user request

## Decisions

- **`sefer-region` gets no further API investment beyond already-committed work.** Empirically confirmed zero internal consumers (only a re-export at `src/lib.rs:384,387`) and no known external consumer. The 4 pre-freeze API questions (Q3/Q4/Q5/Q13) are explicitly deferred, not decided — documented as an intentional decision, not an oversight.
- **The Q1 red-CI fix (dead `assert_send_sync` under `--no-default-features`) was fixed properly, not silenced** — un-gated two assertions that never needed `std`, restoring no_std Send/Sync tripwire coverage rather than adding `#[allow(dead_code)]`.
- **`docs/ALLOC_PLAN.md`'s "one substrate, two faces" plan is confirmed as never having been built and not planned to be** — this session's investigation (grep-based, not narrative) is the basis for closing that question rather than leaving it open.
- **All 10 workspace member crates other than `sefer-region` are confirmed to have real consumers** (5 in the production allocator hot path behind feature flags, 4 in legitimate dev/test/bench/fuzz infrastructure) — there is no broader "dead code across the workspace" problem at the crate-member level.
- **Whether to actually cut a 0.2.0 release of `sefer-region` at all remains undecided** — the user has not confirmed either "bump and publish" or "leave 0.1.0 as-is despite its known cross-region-handle bug." This is the single biggest open fork left.

## Open questions

- **Does #835's `/crush` diff hold up under personal zero-trust review?** Not yet checked — session still running as of this checkpoint.
- **Publish decision for `sefer-region` 0.2.0 (#801)**: bump-and-publish (fixes the real 0.1.0 bug, but the crate stays speculative) vs. leave 0.1.0 live with its known defect vs. some other resolution — genuinely undecided, needs the user's explicit call once #835/#836 land.
- **The five other crates' publish/republish decisions** (#656-#661): unchanged, still awaiting go-ahead, not discussed this window.
- **`docs/CORRECTNESS_OPEN_ITEMS.md` vs `docs/perf/OPEN_ITEMS.md`**: #835's prompt asked the delegate to pick the more appropriate index per each file's own stated Scope — not yet confirmed which (or whether either) was actually touched.

## Repo state

```
 M crates/region/README.md
 M crates/region/src/lib.rs
 M docs/ALLOC_PLAN.md
```

(Mid-flight `/crush` diff for #835 — not yet reviewed, not yet committed. `docs/perf/OPEN_ITEMS.md`/`docs/CORRECTNESS_OPEN_ITEMS.md`, `docs/reviews/2026-08-11-sefer-region-code-quality-review.md`, and `CHANGELOG.md` may still be pending edits from the same `/crush` run.)

```
d434b64 fix(region), test: restore no_std clippy row, closes 7-commit-old red main (code-quality review Q1)
e4f98d3 docs: correct 3 report-prose findings from the #832 closing review + record the F-C6 decomposition (F-C2, F-C5, F-C7, F-C6)
a935e79 fix(region), bench(region): close 6 real findings from the #832 closing review (F-A2, F-C3, F-C4, F-C6, F-C9, F-C10)
483a60e docs: commit all sefer-region round markdown artifacts (task #831)
337f57e docs: update CHANGELOG.md with the sefer-region F1-F13+perf round (task #830)
```
