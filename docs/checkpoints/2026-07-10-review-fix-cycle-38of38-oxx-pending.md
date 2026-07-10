# Checkpoint — 2026-07-10 15:47 [review-fix-cycle-38of38-oxx-pending]

## Session summary

Started from a 7-lens adversarial review (7 parallel `@fxx` agents, docs/reviews/2026-07-09-*.md) of sefer-alloc — a Rust memory allocator — covering unsafe-soundness, correctness, performance, cleanliness, maintainability, security, and a general bug hunt. The user then asked to implement all resulting fix-tasks via `/babygoal`: decompose into a TaskList, arm `/babysit` (20-minute recurring cron, job id `720e029f`), and drive every task through a strict `oh` (implement) → personal-diff-read → `oxx` (independent adversarial review) → commit cycle, per this repo's CLAUDE.md zero-trust methodology. The user later said "act autonomously, resolve all ambiguity in favor of quality" and, once the original 17-task chain (#12–#28) finished, asked to also push and get CI green after every task is done.

27 tasks (#12–#37) are now closed and committed (26 real commits on `main`, all unpushed — currently 26 commits ahead of `origin/main`). Headline fixes: the session's only confirmed real bug — H1, a Stacked-Borrows UB in `HeapCore::thread_free` under `alloc-xthread` (task #13) — was found via a genuine miri-plain repro (not just claimed) and fixed by hoisting the cross-thread free-stack head out of `HeapCore` into a slot-resident `'static` field (`HeapSlot::thread_free` / `FALLBACK_TFS`), mirroring the project's own prior "W3" pattern. Also fixed: a Windows `VirtualAlloc(MEM_COMMIT)` unchecked-failure crash in `crates/vmem` (#12), a real perf regression in the decommit/release path (PERF-4, #14), several vacuous/mislabeled tests now made real via counterfactual verification (#16), stale unsafe-seam docs across CLAUDE.md/PLAN.md/ARCHITECTURE.md/node.rs SAFETY comments (#17–20, #31, #33), unsafe-impl hygiene (removed a misproven `Send` on `HeapSlot`, #21), an observability counter for a known footgun (#23), bench-honesty fixes (#24), a mechanical `alloc_core.rs` split extracting `LargeCacheMode` plus recording the NO-GO verdict for `alloc-runfreelist` (#27, the riskiest/last item of the original chain), and a long tail of small clippy/script hygiene items discovered along the way (#29, #30, #32, #34–36) — every one of which is now closed too. `cargo clippy --workspace --all-targets --all-features -- -D warnings` is fully clean as of commit `06ce625`.

Along the way a genuine `npm run check` (the full pre-push gate: fmt, clippy×3, test×2, iai) was run and initially failed on rustfmt drift accumulated across ~20 commits worth of agent edits (5 files) — fixed with `cargo fmt --all` and committed as `43a4d70`; a clean rerun produced `[check-all] ALL GREEN — safe to push`, and the run also exercised the new "marginal Ir/op" iai column (task #34) end-to-end for the first time, confirming its numbers.

**Task #38** (found as a byproduct of #31's review — a call to `HeapCore::install_thread_free()` in `tls_heap.rs:445` whose return value is discarded, on the *same* sensitive H1/#13 mechanism) was investigated personally (not delegated) by tracing every path into `finish_bind` back through `HeapRegistry::claim`/`claim_with_config`, confirming `bind_thread_free` always plants the handle before `finish_bind` can observe `heap`, on both the first-claim and re-claim legs, and confirming no code path ever resets `thread_free` back to `None`. Verdict: the call and the method were provably dead. Implemented the removal directly (deleted the call in `tls_heap.rs`, deleted the `install_thread_free` method in `heap_core.rs`, updated three files' doc comments to describe the current mechanism, and strengthened the `no_stale_doc_references.rs` regression guard to ban the bare `install_thread_free` token outside one historically-framed mention).

**Mid-verification, a self-inflicted mistake occurred and was caught:** a `git checkout -- src/global/sefer_alloc.rs` used to revert a counterfactual doc-probe (testing the regression guard) also wiped out the legitimate uncommitted edit to that same file, since it had never been committed. Caught immediately via `git diff --stat`, the edit was reapplied verbatim, and the full build/test/clippy suite was rerun from scratch to be safe (not just diffed) — all green: `cargo build --workspace --all-features` clean, full `--features "production alloc-xthread"` suite **124 "test result: ok" lines, 0 failed, real exit code 0** (verified without a masking `| tail` pipe this time — an earlier check had accidentally piped through `tail -30` which silently reports `tail`'s exit code, not `cargo test`'s, and truncated the visible result count to only 2 of the ~15+ test binaries; re-run without the pipe corrected this), and both `clippy --features "production alloc-xthread"` and the full `--workspace --all-targets --all-features` clippy are clean.

**In flight when interrupted for this checkpoint:** an `oxx` adversarial-review agent was just launched (async, agent id not user-facing) to independently re-verify the #38 dead-call proof from scratch (not trust the orchestrator's own trace) before commit — this is the same elevated-scrutiny treatment every other code-level change on the H1/#13 mechanism has received this session. **This review has NOT returned yet.** The `src/global/{sefer_alloc,tls_heap}.rs`, `src/registry/heap_core.rs`, and `tests/no_stale_doc_references.rs` changes for #38 are uncommitted in the working tree right now.

Once that review returns APPROVE (or APPROVE WITH NOTES), the remaining plan is: commit #38, mark task #38 completed, re-run `npm run check` one final time (the full pre-push gate), then `git push` (26+ commits currently queued, all unpushed), then watch the GitHub Actions CI run to green per the user's "push and fix CI" instruction — investigating and fixing anything CI catches that local checks didn't (miri/loom/TSan/multi-arch/no_std/MSRV are NOT covered by `npm run check`, per CLAUDE.md's own caveat, so CI could still find something new).

## Active goal

No `/goal` Stop hook is active in this session (not invoked). The operative directive is the user's plain-text instructions: "действуй самостоятельно, все вопросы решай в сторону совершенства" (act autonomously, resolve ambiguity toward quality) and "после завершения всех тасок сделай пуш и наладь ci" (after finishing all tasks, push and get CI working) — the latter is the standing instruction driving the next steps after this checkpoint.

## TaskList

### in_progress
- #38 tls_heap.rs:445 — install_thread_free() call with discarded result, investigated & fix implemented, awaiting independent oxx re-verification before commit (no blockers)

### pending
(none)

### recently completed (last 10 of 26)
- #37 retire install_thread_free — investigated, refactor declined as too risky at first pass (later superseded: #38's deeper investigation found it genuinely dead and safe to remove)
- #36 last 2 clippy defects under --all-targets --all-features (doc-indent, os.rs dead-code) — full clippy now clean
- #35 crates/numa clippy --all-features (needless_return + dead_code)
- #34 F2 — marginal Ir/op column in the iai judge (verified end-to-end in this session's npm run check run)
- #33 docs/PLAN.md:112 — same stale unsafe-seam wording already fixed in CLAUDE.md (#20)
- #32 scripts/miri.mjs — validate positional args, hard-fail on 0 selected entries
- #31 module-doc drift in heap_core.rs/fallback.rs describing pre-#13 thread_free mechanism (this is what surfaced #38)
- #30 dead-code clippy in regression_run_stack_decommit.rs under --all-targets
- #29 scripts/loom.mjs — loom_thread_free.rs was mapped to a deleted feature name, never ran
- #28 maintainability misc (larson/mstress duplication documented, examples_tmp/ removed, docs/GLOSSARY.md added)

26 tasks total completed (#12–#37); full list and descriptions are in TaskList / prior conversation turns.

## Decisions

- **oh → personal-diff-read → oxx → commit for every task, no exceptions**, including doc-only changes — chosen over trusting single-pass implementation, per this repo's CLAUDE.md zero-trust rule; caught multiple real issues this way (a bench measuring the wrong code path in #14, a broken regression-guard test-count drift in #23, a mismatched teardown-timing doc claim in #24, an overclaimed "NEVER" in #27's Cargo.toml wording).
- **#37 declined a refactor that #38 later did anyway** — #37's `oh` agent found the *call site* it was told to look at (a discarded return value) and correctly refused to guess why, given the sensitivity of the H1 mechanism; #38 was spawned as a separate, narrower investigation task specifically to resolve that uncertainty before touching code — chosen over letting #37's agent push through under uncertainty.
- **Sequential task execution, not parallel** — chosen because most tasks touch overlapping files (`alloc_core.rs`, `node.rs`, `heap_core.rs` recur across #14/#16/#18/#25/#27/#31/#38) and CLAUDE.md's phase discipline (implement → test → zero-trust review → commit) assumes one change lands before the next starts.
- **`git checkout` banned for counterfactual reverts on uncommitted work** — learned the hard way mid-#38 (see summary); going forward, `Edit`/manual revert should be used instead of `git checkout -- <file>` whenever the file has uncommitted changes that must survive the revert.

## Open questions

- None from the user. The only open technical question was #38's dead-call hypothesis, which the orchestrator's own trace resolved as "yes, dead" — the pending `oxx` review is confirmation, not open uncertainty from the user's perspective.

## Repo state

```
 M src/global/sefer_alloc.rs
 M src/global/tls_heap.rs
 M src/registry/heap_core.rs
 M tests/no_stale_doc_references.rs
?? docs/checkpoints/2026-07-08-perf4-decommit-churn-investigation.md
?? docs/security/
```

(The two `??` entries — a checkpoint and a security-audit doc — predate this session's work, are not this session's output, and have been deliberately left untouched/uncommitted throughout, per earlier explicit scoping decisions.)

```
43a4d70 style: cargo fmt drift accumulated across this session's commits
1cc2582 perf(iai): add marginal Ir/op column to the judge report
06ce625 fix(clippy): close the last two --all-targets --all-features failures
781204b fix(miri): validate positional args, hard-fail on 0 selected entries
3dd8938 docs(plan): sync unsafe-seam wording with CLAUDE.md's self-verifying rule
```

26 commits ahead of `origin/main`, all unpushed. `npm run check` was last run clean (green) at commit `1cc2582`-adjacent state, then `43a4d70` (fmt fix) landed after — the pending #38 commit plus a final `npm run check` rerun are the last steps before push.

## Resume hint

1. Check whether the `oxx` review agent (launched just before this checkpoint) has returned — if a completion notification is sitting unread, read it first.
2. If APPROVE / APPROVE WITH NOTES: commit the four modified files (`src/global/sefer_alloc.rs`, `src/global/tls_heap.rs`, `src/registry/heap_core.rs`, `tests/no_stale_doc_references.rs`) with a message describing the dead-call investigation and removal, mark task #38 completed.
3. If REJECT: do not commit; re-investigate per the reviewer's specific objection (do not force the removal through).
4. Run `npm run check` one final time (full pre-push gate).
5. `git push`.
6. Watch CI (`gh run watch` or equivalent) and fix anything red — CI covers miri/loom/TSan/multi-arch/no_std/MSRV, which `npm run check` does not.
