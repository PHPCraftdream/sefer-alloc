# Checkpoint — 2026-07-10 [rad-tasks-and-bench-refresh-uncommitted]

## Session summary

Continuation of a long `sefer-alloc` (D:\dev\rust\sefer-alloc) session. Prior arc (fully complete, pushed, CI green): a 5-pass performance implementation (PERF-PASS-1..5, tasks #49-53) landed 20 commits `76ab7ca..e6b9b3a` on `origin/main`; main CI (`29099480005`), Kani (`29099480037`), and a manually-triggered `perf-gate` (`29099704992`) all confirmed green.

After that arc closed, the user asked for an `fxx` research agent to read a newly-appeared external review doc (`docs/reviews/2026-07-10-radical-performance-optimization-audit.md`, 486 lines, no git history) and form a phased implementation plan. That agent (id `a9c113c12a3c9a897`) returned and wrote `docs/perf/PERF_PLAN_2026-07-10-radical-audit-implementation-plan.md` — 9 deduplicated workstreams (E1-E9) across 9 phases (0-8), independently verifying every claim against `e6b9b3a` and cross-referencing the G1 honest-reject. Top picks: Phase 1 (registry bootstrap ~16 MiB first-touch RSS defect), Phase 3 (extend PASS-3's own residual — pool cap silently clamped to 4 despite public API advertising 8), Phase 4 (cross-thread ring-overflow silent drop, H1-adjacent, needs loom), Phase 5 (MagazineBitmap — demoted to a bounded GO/NO-GO experiment, not a committed win).

User then asked to (1) run comparative wall-clock benchmarks (`npm run bench:table`, SeferAlloc vs mimalloc vs System) and print tables — done, full results relayed in-conversation (SeferAlloc now leads mimalloc at every churn size, 1.1x-11.7x; cold/bulk tiny remains mimalloc's edge at 1.4x-2.35x; large-segment decommit cycle 4.2x/29.6x faster than mimalloc/System). (2) `/fxx`: create grouped tasks from the new plan, grouping items that can be resolved in one pass — done: created tasks #54-#58 (RAD-1..RAD-5), each bundling a Phase-0 judge-harness with its Phase 1/3/4/5 consumer, with `blockedBy` chains (RAD-2 blocks on RAD-1 for clean bisection; RAD-3 and RAD-4 both block on RAD-2 i.e. the alloc_core.rs split; RAD-5 blocks on all three, sequenced last since it's the coin-flip experiment). Gated phases 6-8 (batch API, dirty-segment queue, feature split, DenseRegion) were deliberately NOT turned into tasks — they need human policy answers (plan §5, 6 open questions) or harness evidence first. (3) Mid-flight, while creating tasks, the user sent a new instruction: "обнови бенчмарки в доках/readme" — handled it: updated `README.md` (3 canonical comparative tables — churn+write, churn non-writing, cold-direct — plus the Performance section header, summary bullets, verdict section, and the dedicated cold-first-touch table) and `docs/ALLOC_BENCH.md` (resolved the stale 2026-07-09 methodology-change warning by adding a new top dated section "0.3.x post-PERF-PASS re-measurement (2026-07-10)" with full tables including Vec_push and segment_decommit_cycle) to the fresh 2026-07-10 numbers, with an explicit honesty note that this is the first run on the corrected churn-measurement methodology (2026-07-09 fix removed ~25% cold-phase skew from the churn loop), so roughly a quarter of the churn improvement vs the 2026-07-07 rows is methodology, not raw allocator speedup.

Nothing has been committed yet — README.md and docs/ALLOC_BENCH.md are modified but unstaged, and the two new docs (the radical-audit review + its implementation plan) are still untracked. The user has not yet asked to commit or to start implementing any RAD-1..5 task.

## Active goal

None currently armed that I'm aware of (the earlier session's `/goal` "реализовать все таски" was tied to the now-complete PERF-PASS arc and should have auto-cleared once that TaskList emptied — not re-verified this turn).

## TaskList

### pending
- #54 RAD-1: Registry bootstrap — first_alloc harness + lazy next_free + chunked registry (план Phase 0a+1, E1)  (blockedBy: none)
- #55 RAD-2: Механический split alloc_core.rs на модули, zero behavior change (план Phase 2, §12.1)  (blockedBy: #54)
- #56 RAD-3: Масштабируемый честный small-segment pool + pool_cap_sweep (план Phase 0b+3, E2)  (blockedBy: #55)
- #57 RAD-4: Не-теряющий cross-thread overflow fallback + remote_fanin harness (план Phase 0c+4, E3a)  (blockedBy: #55)
- #58 RAD-5: MagazineBitmap — bounded GO/NO-GO эксперимент (план Phase 5, E4)  (blockedBy: #55, #56, #57)

### recently completed
- #49-53 PERF-PASS-1..5 (the prior 5-pass performance arc, fully landed and pushed)
- #40 doc-writing task for the original perf-review synthesis

## Decisions

- Grouped RAD-1..5 by "judge harness + its consuming phase" rather than 1:1 with the plan's 9 phases — collapses Phase 0(a)/(b)/(c) into whichever RAD task consumes that harness's red→green counterfactual, per the user's explicit instruction to group tasks solvable in one pass.
- Did NOT create tasks for gated Phase 6 (batch API), Phase 7 (dirty-segment queue), Phase 8 bucket (feature split / hybrid index / aligned LUT / DenseRegion) — these require human policy decisions (plan §5) or harness-evidence gates not yet available; premature to queue as actionable work.
- Sequenced RAD-2 (mechanical `alloc_core.rs` split) as a blocker for RAD-3/RAD-4 (not RAD-1) — RAD-1 touches `bootstrap.rs`/`heap_registry.rs`, not `alloc_core.rs`, so it's independent; RAD-3/RAD-4/RAD-5 all edit the region RAD-2 splits, so smaller post-split diffs are worth the sequencing cost.
- For the bench-table refresh, chose to preserve older dated sections in `docs/ALLOC_BENCH.md` (historical record per the file's existing convention) rather than overwrite them, adding the 2026-07-10 run as a new top section — consistent with how 2026-07-05/07-06/07-07 sections already coexist in that file.

## Open questions

- None from the user directly this turn. Implicit: whether/when to commit the two doc updates (README/ALLOC_BENCH) together with the two new untracked docs (radical-audit review + implementation plan) — not yet requested. Whether to start RAD-1 (the top-ranked, unblocked task) — not yet requested either.

## Repo state

```
 M README.md
 M docs/ALLOC_BENCH.md
?? docs/checkpoints/2026-07-08-perf4-decommit-churn-investigation.md
?? docs/checkpoints/2026-07-10-radical-audit-plan-pending-ci-watch.md
?? docs/checkpoints/2026-07-10-review-fix-cycle-38of38-oxx-pending.md
?? docs/perf/PERF_PLAN_2026-07-10-radical-audit-implementation-plan.md
?? docs/reviews/2026-07-10-radical-performance-optimization-audit.md
?? docs/security/
```

```
e6b9b3a fix(alloc-core): apply fh review's 3 low-severity findings from the 5-pass perf work
bd7f9ee docs(perf): final re-pin of IAI_BASELINE.md, session summary (PERF-PASS-5)
1fc6dd3 docs(alloc-core): document AllocCore field-order reorder as a measured no-op
a329b35 perf(registry): bundle Tcache's per-class count with its slots
ca9e70a perf(alloc-core): reorder SegmentHeader for cache-line locality
```

HEAD (`e6b9b3a`) is already pushed to `origin/main`; everything shown above is local-only, uncommitted/untracked work from this turn.
