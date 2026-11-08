# J13b judge — preparatory infrastructure manifest

## Current Task54 static-reference candidate — 2026-11-08

Status: source overlay and activating oracles authored; exact-source CI acceptance pending. Class: `bench`. The overlay changes only a disposable CI source copy, never shipping allocator source. It enables the existing Always controls, preserves fixed-one non-control advice and existing owner validation/sampling/counters, and bypasses only estimator admission. Two complete original-file hashes and exact text anchors are required; the receipt records both resulting file hashes and the overlay digest, not a whole-tree or binary identity.

Six Node tests check confinement including Windows cross-drive paths, whole-file source pins, exact transform boundaries, production-source preservation, source-link rejection and refusal before writes for invalid input or an existing receipt. One ignored Rust test installs SeferAlloc and is explicitly activated only in the overlaid CI copy: two fresh cohorts must yield two real sparse controls, fourteen full 8 MiB advice extents, and unchanged e/m/target/applied/dropped. RAII retains exact layouts and frees all admitted blocks even when an assertion fails. Omitting selection, sampling, fixed-one advice or the admission bypass fails this behavior oracle.

The narrow CI job archives its checked-out source into one temporary `.rush` copy, applies the overlay, executes the exact native test, requires a positive one-test PASS, verifies original production hashes and removes the copy/build directory on exit. No local test/model/workload, copy, branch or worktree was created. No library dependency, production composition, header, ordering site or lifecycle protocol changed. No whole-runtime model, full judge, runtime row admission, campaign or performance result is claimed.

## Accepted Task54 artifact-routing step — 2026-11-08

Status: accepted on `be5fd14b5a29ccfec295ede0bc34682ac16b4786` by CI `42517707507` SUCCESS; final PR checks 42 SUCCESS / 23 intended SKIPPED. Personal log review: 33 Node tests (eight new plus 25 existing) and 13 Rust tests PASS; semantic selftest 214 cases / 316 mutants / zero failures; linter mutants 10/10; protected range 15 commits / zero red (existing surplus-trailer WARNs remain). Class: `bench`; no allocator source/default/feature/dependency/version change, runtime adapter, static-reference overlay, campaign or measured result in this step. Task49's accepted runtime-learning source is `6c2ba419` (CI `42512485612`), not acceptance of this judge.

The new manifest binds three distinct artifact paths and all nine arms, keeps `base/base2/mi` on the same historical executable, and requires a separately identified control-matched static-reference artifact. It validates exact metadata fields, source/binary hashes, overlay provenance and matched candidate/reference toolchain/features. The driver hashes actual executable bytes before and after each child and checks every window's artifact identity. Raw stdout remains preserved before post-child failures. Source provenance still requires a reviewed build flow; manifest strings and distinct hashes alone are not a static-reference correctness proof.

Separate Node tests cover routing, immutable copies, rejecting metadata/overlay mutations, bounded strict-UTF-8 manifest reads, real file hashing, rejection before spawn, raw preservation after in-child executable mutation, identity mismatches and unchanged runtime-admission refusal. Existing mock transport/window tests now explicitly inject artifact-verification seams; their transport assertions are preserved. These tests execute only on GitHub CI, not locally.

Real installed adapter, accurate lifetime/peak advice accounting, actual owner/remote workload/training windows, static-reference behavior, OS telemetry and runtime report admission remain open. The following original preparation record is historical, not a current-source CI receipt.

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
