# J13b atomic learning cell — Task60 prerequisite for Task49

Status: accepted on exact published source `91c875f36900ae5108f251e1ff0b81990736a02e`, CI `42500341404` SUCCESS after personal source and execution-log review. This is a state/concurrency foundation, not runtime learning and not a performance result. Later Task49 integration requires its own exact-source acceptance.

## Boundary

At the accepted Task60 source, Task48 controls remain unchanged: `ThpAdvice::auto_share` returns zero and no production observation feeds the new cell. This is the prerequisite's boundary, not a claim about later Task49 commits. Default Off, Always semantics, process configuration, mapping charge/release, header layout, production feature composition and dependencies are unchanged.

The new `ThpLearningCell` stores the accepted estimator's fast `e` and slow `m` Q31 components in one `AtomicU64`. Initial state is the same optimistic pair as `LargeThpEstimator::new()`. A snapshot performs one Relaxed load; one successful strong CAS commits both components together. No external payload is published through this scalar, so these Relaxed operations do not substitute for a Release/Acquire publication protocol.

Each observation makes at most eight CAS attempts. Rejection returns false and applies no update from that caller. Task49 must account for this loss separately before admitting the cell to runtime; bounded retry does not guarantee fairness or unbiased admission. A native competing-update test counts successful updates and uses an exact one-unit-per-update gradient, not timing, to check conservation. Future per-group process cells must remain independent. No background service, allocation, thread pinning, callback or business-task identity is introduced.

## Structure-only move

The small, pure `ThpAdviceOutcome` declaration moves unchanged into `large_thp_runtime/state/`, beside the new cell. Explicit path declarations in the existing wiring-only `mod.rs` preserve the outcome's original module/type path and canonical symbol namespace. The actual-source ledger model follows its file path. This keeps the containing source directory at seven entries without moving the existing runtime implementation or touching its P26 bindings/fixtures. `thp_runtime.rs` remains byte-identical to the accepted parent (normalized SHA-256 `a6f171d5e08e952c4f471d976e50b0b3382139c61f58ad70cd8cde454dc9a25a`). No structural debt or weaker catalog expectation is introduced.

## Verification obligations

- `tests/loom_thp_learning_cell.rs` mounts the actual cell and actual pure math. Native cases cover every update against the accepted estimator, recovery, group isolation, accepted-update conservation and packed-pair endpoints/distinct components. Under Loom, two different competing observations must produce exactly one complete serial order, not a torn pair or lost update.
- `tests/j13b_learning_contention_bound.rs` substitutes only the atomic seam to force every CAS to reject. It checks exactly eight attempts, false return and unchanged state. It is a negative-control test, not a substitute for the real-atomic native and Loom cases.
- Two dedicated check-matrix rows activate the native cases. The existing Loom xthread job explicitly runs `loom_thp_learning_cell` and requires its named positive test output; the local Loom manifest lists it too. No local Loom/test run is authorized by that wiring.
- Protocol P27 inventories the three ordering sites and pins update/snapshot inventories, observed-value CAS and bounded retry with four positive/negative fixture pairs. Fixtures, Loom and behavioral tests run only in GitHub CI. No execution acceptance is claimed from authored tests or local type checks.
- The cargo-free inventory reports 728 ordering sites / 96 files / 27 protocols / 52 scopes, zero gaps, double claims, dead scopes or protocol errors. The structure and Loom-wiring verifiers pass. The first Windows all-tests type check passed; its support-import warning was then corrected rather than suppressed.

No local runtime tests/models/workloads, new unsafe operations, performance measurements, performance GO, main merge or default promotion. Correctness 271 remains CONDITIONAL; Task49, the real judge and the campaign remain unfinished.

## Commit identity and artifacts

The published source commit is `91c875f36900ae5108f251e1ff0b81990736a02e` (`feat: add bounded atomic THP estimator state prerequisite`), derived by `git log --format='%H %s' 6d08655f..91c875f3`. It cherry-picks the personally reviewed caller source commit `08c9adfe`; publication excludes the local checkpoint. No performance raw logs were produced.

## Actual exact-source receipts

- [CI 42500341404](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/42500341404) completed SUCCESS on the full published SHA above. Final PR #225 checks were 42 SUCCESS and 23 intended SKIPPED, with no pending or failed checks.
- [Check-matrix](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/42500341404/job/128791573387): the two dedicated rows positively ran four native cell tests and one forced-CAS rejection test; ALL GREEN. These are not zero-test receipts.
- [Loom](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/42500341404/job/128791573308): the cfg(loom)-only actual-source competing-observation/racing-snapshot test ran and passed (one test).
- [Ghost-guard](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/42500341404/job/128791516078): 213 fixture cases / 314 mutants / zero failures; trailer-linter mutants 10/10 rejected; `bec1e53f..91c875f3` protected range checked twelve commits with zero red.
- [aarch64](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/42500341404/job/128791573214): both new binaries positively executed four plus one tests in cross-feature rows. This is stronger than compilation alone, not a native wall-clock result.
- [Zero-cost](https://github.com/PHPCraftdream/sefer-alloc/actions/runs/42500341404/job/128791572137): declared-runtime scope SKIP with the unchanged classifier, not normalized-text equality or an Ir/cost measurement.

Only GitHub CI executed tests/models. The local full protected-span attempt could not run without the absent ghost-lint binary; it was neither built nor executed locally. The actual full protected-range receipt is the CI result above.
