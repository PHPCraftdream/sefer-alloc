# R6-T40 / U25 — conservative disposition (2026-10-16)

Runtime change retained: OPT-04 candidate only. OPT-05 batch routing is implemented in production source: one borrowed `SmallIssueRoute` context is resolved per freelist-drain batch and reused for each per-block prepare and commit. No performance measurement is made; the expected effect is only the R17-derived estimate of roughly 20–50 Ir saved PER BLOCK (resolution cost amortized out of each block's prepare/commit), not a measurement. Verification was limited to rustfmt and structural/source checks; cargo/rustc/build/tests were not run by instruction.

## OPT-04 — `System.alloc` for SmallSidecar

`EntryHandle::new` now uses `System.alloc` rather than `System.alloc_zeroed`; `SmallSidecar::initialize_at` explicitly initializes pending atomics and class leaves before publication. New `tests/r6_t40_sidecar_system_alloc.rs` directly registers a real reserved Small route, snapshots `SmallSidecar::system_totals_for_test()` around registration, and asserts requested allocation bytes equal `size_of::<SmallSidecar>()`, zeroed-byte delta is zero, and there is one System allocation. It verifies the initialized class state starts empty, then route prepare/issue works. The previous test design was invalid (it tested an allocation payload, not the sidecar) and has been replaced. This remains unmeasured pending CI and preregistered same-layer A/B.

The dedicated test is registered as a target in `scripts/check-matrix.mjs:256-261` with `production internals bench-internals`; `.github/workflows/ci.yml:1564` runs the manifest-driven check/test rows. Its check-matrix row does not specify `alloc-global` explicitly; that feature is enabled by the `production` composition. CI execution has not been performed in this task. The metric regression oracle is source-based: prior `record_alloc(..., true)` would add the sidecar size to zeroed bytes; the retained `false` records zero. This counterfactual was not executed here. The test observes initial class state only; it does not assert every pending bitmap bit is zero.

## OPT-05 — route once per batch

Implemented (drain only in this pass): `try_drain_freelist_batch` resolves one borrowed route context after the empty-head early return; each probed candidate calls the context's `prepare(offset, class)` (`bool`; `false` = spill OOM `Err`) and each committed block calls `commit(offset, class)`. Standalone tables get `None` and keep the existing `IssueTransaction::commit_prepared` fallback with the batch's index computed once. `try_carve_batch`, scalar `issue_small`/`prepare_small_issue`, `RouteSlots::prepare_small_issue` semantics, and `pop_free` are unchanged (exact HEAD).

New witness: `tests/r6_t40_batch_issue_route.rs` (registered in `Cargo.toml` as `r6_t40_batch_issue_route` with `required-features = ["production", "internals", "bench-internals", "alloc-segment-directory"]` and as check-matrix row `test-r6-t40-batch-issue-route`, `--test r6_t40_batch_issue_route`). It builds a routed `AllocCore` via `dbg_new_routed_for_test`, carves a full batch (`dbg_carve_batch`) and frees every block so a non-empty class-0 chain exists, then — following the `tests/r11_ph3a_issue_transaction_batch.rs:208,246` hook pattern — arms `SmallSidecar::fail_prepare_after_for_test(2)` before `dbg_drain_freelist_batch`. Counterfactual: the refusal fires at the 2nd per-block prepare, so the pre-OPT05 per-block-resolution path and the new single-context path are both supposed to abort with `0` BEFORE any state change; the test asserts the drain returns 0, the output pattern survives, head/live/bump (`dbg_r11_issue_state_for_test`), the directory bit and the chain's free bits are unchanged, and the retry after the one-shot countdown commits the whole chain (equivalence/retry). A second happy-path test asserts a full drain commits exactly the chain with valid, distinct, class-aligned, bitmap-allocated pointers, live credits grow by exactly `k`, the bump never moves and the exhausted chain ends at `FREE_LIST_NULL`. HONEST LIMIT: there is NO route-resolve counter — the oracle is behavioral and does NOT measure how many times the route is resolved; a per-block-resolve implementation would pass the same asserts. Compilation and execution were NOT run in this task (no cargo/rustc/npm by instruction); only rustfmt, `scripts/verify-structure-limits.mjs` and `scripts/verify-dbg-hook-safety.mjs` were run. No benchmark was run; the 20–50 Ir PER BLOCK figure is an expected R17-based estimate only, not observed performance.

## Deferred OPT-09/10/11 — current source evidence

- 09 reverse physical `PerClass` stack: deferred; no locality/benefit proof established. No runtime change.
- 10 maintenance budget by non-empty cuts: deferred because it alters drain scheduling and needs its own correctness/performance gate. No runtime change.
- 11 repeated Large sweep in one `realloc`: `src/registry/heap_core/free/realloc.rs:165-184` shows the owned-segment realloc path conditionally performs `drain_large_sidecar_ingress()` when the new request classifies Large; line 182 explicitly keeps the full sweep as a cold path without measured justification for a bounded probe. A subsequent allocation path calls `drain_large_sidecar_ingress_hot_bounded()` at `src/registry/heap_core/alloc/hot.rs:193`; therefore the observed path is a full realloc sweep followed, if realloc moves and allocates, by a bounded hot drain—not two demonstrated full sweeps. The residual bounded probe makes this a low-priority deferral, not evidence the repeated-work concern is obsolete or resolved. No change made.

09/10/11 are partial, source-limited assessments, not closures. No code change was made for any of them.

## Future preregistered A/B cells

No cells were run. A promotion report must cover these explicit paired baseline/candidate cells at the shipping layer, report latency and instruction counts plus activation oracle, configuration, and within-regime cost/benefit:

1. `recycle_alloc_only_256x16b`.
2. `recycle_alloc_free_256x16b`.
3. `cold_alloc_only_256x16b`.
4. `carve_batch_only_16b`.
5. `small_churn_16b`.
6. `dealloc_free_only_16b`.
7. `alloc_magazine_hit_only_16b` (control).
8. OPT-04 cold SmallSidecar route registration / initialization, with requested/zeroed allocation metrics.

Report full-round criterion time in addition to any narrower timed window; retain raw logs and summary CSV. These are planned cells, not measured results. CI/format/structure verification is not performance evidence.
