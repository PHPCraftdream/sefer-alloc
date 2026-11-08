# J13b atomic learning cell — Task60 prerequisite for Task49

Status: source authored; exact-source GitHub CI acceptance pending. This is a state/concurrency foundation, not runtime learning and not a performance result.

## Boundary

The accepted Task48 controls remain unchanged. `ThpAdvice::auto_share` still returns zero, and no production observation feeds the new cell yet. Default Off, Always semantics, process configuration, mapping charge/release, header layout, production feature composition and dependencies are unchanged.

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

This manifest is authored before the source commit exists; no SHA is invented. The source delta is reproducible with `git diff 133f3c1a` before commit and with the eventual commit range afterward. No performance raw logs were produced. CI links and final receipts must be appended only after inspecting actual nonzero executions on the published source SHA.
