# Test selection enforcement criterion

**Verdict: INSUFFICIENT_WINDOW**
Reason: 27 more informative runs needed (3/30).

Window starts at 2026-11-17T13:04:56Z (`a28be5dace7d9517f3767fa37ee15a99a06d0dde`).
Collection: PHPCraftdream/sefer-alloc; since 2026-11-17T13:04:56Z; listing complete: yes.

Scope: ordinary pull-request runs or pushes to `main`, except `CI: full` runs identified by the exact selection-artifact reason `full run requested`. Scheduled and manually dispatched runs are excluded. Repeated pushes to the same PR count as distinct runs. Reruns are correlated, so each run ID contributes at most one observation.

Latest-attempt metadata is authoritative for run status and denominator classification. Retained `priorAttempts` snapshots are evaluated for red failures; confirmed misses below identify the attempt and affected target. Evidence must be bound to its own attempt by `evidenceAttempt` (untagged legacy attempt 1 records are supported). Older evidence cannot be borrowed for the latest attempt. Captured misses from older red evidence remain in the report even after a green rerun.

The guarded collector binds evidence with `evidenceAttempt` and makes one final GET `repos/<repo>/actions/runs/<id>` per run when the pass collects or refreshes candidate jobs, artifact, or log evidence, or retries an unstable check, to verify the run ID and compare attempt, status, conclusion, and head SHA when returned. If the run differs or the check is unavailable, it discards the candidate evidence and blocks PASS. Verified cache-only records need no extra GET when the fresh listing matches the last validated metadata and no evidence is acquired. On the first guarded refresh, unverified legacy evidence is not reused as current evidence; it can still be retained as historical evidence.

The API does not offer a transaction spanning evidence collection and the disk write. A run can change after the final GET and before the write. Such a change does not mix the data already collected, but the verdict can lag the current run state until the next refresh.

Selection artifact downloads are not attempt-scoped, so the collector does not download them for attempts greater than 1. The resulting lack of trustworthy selection evidence can block PASS even when the rerun is green.

Actual shadow execution runs the full core test rows with selector setup skipped; the artifact describes what enforcement would have selected, not what actually ran. A required-features target is eligible when it actually ran in a row with the same features. Subset execution retains library tests, retains integration targets only when their observed paths are selected, and misses doctest and other target kinds.

In `.github/workflows/ci.yml`, only the three `test-core` rows consume the `Selected tests` outputs. Other jobs and workspace-only tests outside those rows are not evaluated, even for crate-only selections.

If failure attribution remains unresolved, a future shadow run should write structured failure-target evidence per core row (row/features, target kind, name and observed path, or an explicit pre-test/unresolved status) and always upload it, including on failure. Include the run ID and attempt ID in both the artifact name and its contents so the evidence can be attributed to the correct retry. No CI changes are made here.

Rare failure classes may be unsampled; no observed misses does not prove future safety. Retain the nightly full run of `main` and `CI: full` at sprint close.

## Numerical criterion

PASS applies only to the three in-scope shadow `test-core` rows: the listing must be complete, at least 30 distinct run IDs must be informative (including at least one subset), and there must be zero confirmed misses and no unresolved red or attempt evidence.
An incomplete listing or too few informative runs yields `INSUFFICIENT_WINDOW`. Once those requirements are met, a confirmed miss yields `FAIL`; otherwise unresolved red or attempt evidence yields `INSUFFICIENT_WINDOW`. With neither a miss nor unresolved evidence, no informative subset run yields `FAIL` for lack of subset observations, not for a confirmed miss.
Run IDs: 6 total, 3 informative, 0 uninformative, 3 excluded. Informative runs: 0 all, 3 subset. Shortfall: 27. Confirmed misses: 0. Run IDs with unresolved red evidence (latest or historical): 0; with unresolved attempt evidence: 0.
Latest-attempt red runs (completed with conclusion `failure`): 3 total, 0 informative, 0 uninformative, 3 excluded.
Subset share (informative subset runs / all informative runs): 3/3 (100.00%). A denominator of zero makes `0/0` undefined.

## Available inputs and remaining unknowns

The recorded inputs are the run JSON records and the collection and local-check metadata summarized in this report. Evidence present in the run records:

- Core job metadata: 6/6 (run IDs: 44089185362, 44158276248, 44162344209, 44163950491, 44209633592, 44231928052).
- Selection artifact data: 6/6 (run IDs: 44089185362, 44158276248, 44162344209, 44163950491, 44209633592, 44231928052).
- Reduced Cargo failure evidence: 2/6 (run IDs: 44162344209, 44231928052).

### Unresolved red and attempt evidence

The criterion fails closed: an in-scope, completed failed run that is uninformative is unresolved unless the core evidence rules out a core test failure. An unavailable core job does not establish that tests were skipped, failed before running, or succeeded; explicit, consistent skipped, pre-test, or successful core evidence can rule out a test failure. Missing evidence is neither a confirmed miss nor proof that the selector retained every failing target.

None recorded.

## Collection issues

None recorded.

## Status reasons

| Category | Reason | Runs |
| --- | --- | ---: |
| excluded | full run requested | 3 |
| informative | complete core test evidence | 3 |

## Selection counts

| Category | Selection | Artifact reason | Runs |
| --- | --- | --- | ---: |
| excluded | all | full run requested | 3 |
| informative | subset | every changed file is attributed | 3 |

## Run evidence

Status and category refer to the latest attempt recorded. Evidence fields show what was recorded, not necessarily what the model accepted for that attempt. The Collection check column shows `collectionCheck`: an `unstable` marker means the final check found a change or could not verify the run, so the candidate evidence was discarded. A dash means no check marker was recorded, not that a check passed.

| Run | Created | Event | Branch | Head SHA | Latest attempt | Evidence attempt | Collection check | Run status | Run conclusion | Core lookup | Core conclusion | Shadow | test () | test (--features experimental) | test (--features alloc-core) | Selection | Selected / total | Artifact reason | Crate-only crates | Category | Reason | Failures | Record issues |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| [44089185362](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/44089185362) | 2026-11-17T13:04:59Z | push | main | a28be5dace7d9517f3767fa37ee15a99a06d0dde | 1 | 1 | stable | completed | success | available | success | true | success | success | success | subset | 199 / 499 | every changed file is attributed | — | informative | complete core test evidence | None observed | None recorded |
| [44158276248](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/44158276248) | 2026-11-17T19:24:39Z | pull\_request | p02-verdict-20261117 | 816a28f54c8d20bd2029510b362cbbf35dc37ab0 | 1 | 1 | stable | completed | success | available | success | true | success | success | success | subset | 199 / 499 | every changed file is attributed | — | informative | complete core test evidence | None observed | None recorded |
| [44162344209](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/44162344209) | 2026-11-17T19:47:21Z | pull\_request | p21-segment-route-20261117 | 8beb48f9e80587453a12beac9c5dd862e2c5c672 | 1 | 1 | stable | completed | failure | available | failure | true | failure | failure | failure | all | 499 / 499 | full run requested | — | excluded | full run requested | latest: integration: no\_panic\_doc\_accuracy (tests/no\_panic\_doc\_accuracy.rs) (test (), test (--features experimental); selection unavailable or out of scope); latest: integration: no\_stale\_doc\_references (tests/no\_stale\_doc\_references.rs) (test (), test (--features experimental); selection unavailable or out of scope) | None recorded |
| [44163950491](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/44163950491) | 2026-11-17T19:56:24Z | push | main | 66294fa74b5bf346844cc608d3b9a66c741ee0db | 1 | 1 | stable | completed | success | available | success | true | success | success | success | subset | 199 / 499 | every changed file is attributed | — | informative | complete core test evidence | None observed | None recorded |
| [44209633592](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/44209633592) | 2026-11-18T00:39:40Z | pull\_request | p21-segment-route-20261117 | 3b76a492a9f1612220831eb0b182decb6c0b46a7 | 1 | 1 | stable | completed | failure | available | failure | true | failure | failure | failure | all | 499 / 499 | full run requested | — | excluded | full run requested | None observed | core log unavailable: gh invocation failed (exit 1); gh api repos/PHPCraftdream/sefer-alloc/actions/jobs/134485289235/logs |
| [44231928052](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/44231928052) | 2026-11-18T03:39:16Z | pull\_request | p21-sefer-segment-20261118 | aee54befce3ad9526c12947d54ec90f67ff8c0a1 | 1 | 1 | stable | completed | failure | available | failure | true | failure | failure | failure | all | 499 / 499 | full run requested | — | excluded | full run requested | latest: integration: no\_panic\_doc\_accuracy (tests/no\_panic\_doc\_accuracy.rs) (test (), test (--features experimental), test (--features alloc-core); selection unavailable or out of scope); latest: integration: no\_stale\_doc\_references (tests/no\_stale\_doc\_references.rs) (test (), test (--features experimental), test (--features alloc-core); selection unavailable or out of scope) | None recorded |

## Observed misses

| Run | Attempt | Evidence source | Kind | Target | Path | Core rows | Selected subset (paths) | Reason |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |

No confirmed misses in the available evidence.

## Reproduce from recorded inputs

To refresh the recorded CI inputs from the repository root, use the collector:

```sh
node scripts/collect-test-selection-criterion.mjs
```

The collector caps counted `gh` invocations at 45. Theoretical single-page budget with 1 run-listing call and one jobs page per new run: 14 new green runs (3 calls each: jobs, artifact, final GET), or 11 new red runs with a failing core and log fetch (4 each: jobs, artifact, core log, final GET). These limits assume first attempts that download an artifact and only one kind of new run. Verified unchanged cache-only runs cost no per-run calls; they consume only listing pages. Extra listing or jobs pages reduce capacity. These are not observed totals; see the collector’s logged `calls` count for actual invocations.

The collector’s selection integrity receipt stores the original artifact SHA-256 (`sourceSha256`), the compact content SHA-256 (`compactSha256`), and the test count (`testCount`). To verify the source digest for an eligible first-attempt download, compare it with the SHA-256 of the exact `selection.json` bytes from `gh run download <ID> --name test-selection --repo PHPCraftdream/sefer-alloc`. Offline, the recorded inputs alone can verify only the compact digest and test count, not the original artifact digest.

Without recollecting, regenerate `docs/perf/TEST_SELECTION_CRITERION.md` from `docs/perf/test_selection_criterion_inputs`, or check freshness without writing:

```sh
node scripts/test-selection-criterion.mjs --report
node scripts/test-selection-criterion.mjs --check
```

## Appendix: Local checks

| Command | Exit code |
| --- | ---: |
| node scripts/verify-structure-limits.mjs | 0 |
| node scripts/module-graph.mjs --check-backedges | 0 |
| node scripts/verify-ci-node-tests.mjs | 0 |
| node scripts/verify-ci-sentinels.mjs | 0 |
| node scripts/verify-loom-target-wiring.mjs | 0 |
| node scripts/verify-miri-target-wiring.mjs | 0 |
| node scripts/verify-miri-sharding.mjs | 0 |
| node scripts/j13b\_zero\_cost\_receipt.mjs --check | 0 |
| node scripts/verify-root-package-list.mjs | 0 |
| node scripts/verify-no-bare-abort.mjs | 0 |
| node scripts/ghost\_inventory.mjs --check --report docs/verification/GHOST\_INVENTORY.md | 0 |
| node scripts/ghost\_inventory\_mutants.mjs | 0 |
| node scripts/verify-b4-30-pin.mjs | 0 |
| node scripts/verify-doc-inventories.mjs | 0 |
| node scripts/verify-ci-decomposition.mjs | 0 |
| node scripts/ci-plan.mjs --check-doc | 0 |
| node scripts/verify-sprint-close.mjs --selftest | 0 |
| node scripts/test-impact-map.mjs --selftest | 0 |
| node scripts/select-tests.mjs --selftest | 0 |
| node scripts/r20-gate-config.mjs --selftest | 0 |
| node scripts/r20\_r6\_ab\_iai\_run.mjs --selftest | 0 |
| node scripts/test-selection-impact.mjs --check (Cargo intentionally absent from PATH; cargo metadata unavailable) | 1 |
| git diff --check | 0 |
| node scripts/classify-tests.mjs --check (Cargo intentionally absent from PATH; both generated docs stale) | 1 |
| git diff --exit-code -- docs/verification/ (intentional CI decomposition baseline difference) | 1 |
| node --test tests/test\_selection\_criterion.test.mjs (26/26 passed, including subtests) | 0 |
| docs-consistency Node suite (Cargo intentionally absent from PATH; 258/260 passed; classification and old impact failed) | 1 |
| node scripts/collect-test-selection-criterion.mjs (pass 3 live: 22 gh calls at the default cap; 6 runs, all stable at attempt 1) | 0 |
