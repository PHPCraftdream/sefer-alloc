# J13b judge — preparatory infrastructure manifest

## Source and classification

Base: `65b74fa4`. The judge-only commit list is derived, not hand-copied:

```sh
git log --reverse --format='%H %s' 65b74fa4..HEAD
```

The first delivery is classified `bench`: preparation only, no runtime algorithm or production/default feature change. There are no `perf(runtime)` or `perf(opt-in)` changes, new dependencies, version changes, or measurements in this delivery. CI evidence for the new files is pending. Earlier foundation run `42176174292` on `87397c66` is not evidence for this judge.

## Scope and review

The delivery contains the fail-closed child, bounded workload plans and owned A→B test substrate, per-window synthetic schema and statistics, subprocess driver, separate Rust/Node tests, narrowly selected CI steps, and documentation. README/ARCHITECTURE counts were mechanically recounted: 488 root Rust integration-test files and 107 example binaries.

Two internal reviews returned FAIL and were repaired before transfer: raw fixture pipeline, training windows/status propagation, churn-small threshold, stdout preservation, test oracles/CI wiring, and the generated CI table. A later internal review returned PASS_WITH_NOTES; that is not test execution. Personal review additionally widened the 8 GiB scenario limit and aggregate traffic arithmetic to u64 to avoid 32-bit usize overflow. The original prereg remains a byte-identical suffix after a dated pre-measurement amendment.

Local checks: targeted `cargo check` and `cargo clippy -- -D warnings` for example and Rust test with `production`; individual `rustfmt --check`; `node --check`; `git diff --check`; structure limits; `ci-plan --check-doc`. No local test, example, report CLI, workload, benchmark, Miri, Loom or doc execution.

## Explicitly incomplete

Real SeferAlloc/GlobalAlloc adapter, tuple readback/conflict evidence, budget/control/per-bucket diagnostics, VM lifecycle wiring, host/artifact identity capture and actual window execution remain integration obligations. Runtime report admission is disabled; the fixture CLI only emits SYNTHETIC-labelled output. Do not close the full mixed-load judge acceptance on this preparatory scaffold alone.

PR #224 was OPEN at preparation; no merged static-J13 baseline is claimed. No merge into main or default change is authorized by this delivery.

## Artifacts and verdict

New measurement raw logs: **0 files, 0 bytes**. No wall-clock, Ir, RSS delta, performance report, immutable measurement hashes or GO/NO-GO measurement verdict exists. Current delivery status: **INFRASTRUCTURE / CI PENDING**. CI tests are wired but not yet executed for this source.
