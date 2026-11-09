# J13b judge — preparatory infrastructure manifest

## Current Task54 window OS producer candidate — 2026-11-09

Status: producer and discriminating fixtures authored; exact-source CI acceptance pending. Class: `bench`. `j13b_os_probe.rs` reuses fallible `proc-memstat` memory reads, bounded procfs fault/huge-page input, checked unsigned parsing/conversion/deltas, and one process-wide epoch lease. Native Linux begins an own-child VmHWM epoch with clear_refs value 5; no physical RSS, referenced bits, THP policy, foreign PID or host tuning is changed. Missing/unsupported/inconsistent fields and overlapping or poisoned epochs are errors, never zero/clamped fallback. The probe rejects non-native/Miri execution before OS I/O. Named `OsSample` quantities explicitly identify absolute RSS versus fault deltas and their non-atomic intervals.

Four Linux-native parser tests (plus unsupported-backend checking on Windows/Miri) distinguish process fields 10/12 from child fields 11/13, whitespace/parentheses/non-UTF-8 comm, malformed/signed/overflowed counters, huge-page units/absence/duplicate fields, and counter decrease/wrap. Miri must run all five parser/backend-rejection cases and cannot present host measurements as guest values. An isolated ignored native witness is explicitly activated in CI: checked VM release of a faulted 32 MiB mapping creates a prior peak, value 5 must really lower it, and a freed 16 MiB in-window peak must survive in the final sample. This discriminates no-op/incorrect reset, cumulative-peak relabelling and end-RSS substitution without malloc-retention assumptions. Overlap rejection and next-epoch release are exercised. No timing threshold or performance result is used.

The dated prereg clarification retains its original GlobalAlloc-facade payload entrypoint with System bookkeeping, not AllocCore or a false all-process-installed timing label. It documents the native OS epoch contracts before campaign data; workload volumes, numeric gates, margins and statistics are unchanged. Full windows/training, accepted-advice lifetime/peak producers, legacy-baseline capability readback, runtime row admission and campaign remain open. No library/default/dependency/version change, local probe/reset/test/model/workload execution, branch or worktree is added.

## Accepted Task54 owned buffers — 2026-11-09

Status: accepted on `e1e98236a291be57aedb3ea838cc43fa57d88dff` by CI `42534627862` SUCCESS; final PR checks 42 SUCCESS / 23 intended SKIPPED. Personally inspected native six ownership fixtures plus one isolated installed boundary, 39 Node tests / 13 legacy Rust tests / one static native test, both production hashes, semantic selftest 214 cases / 316 mutants / zero failures and protected range 17 commits / zero red. The actual-source Miri raw log positively executed all six fixtures under strict Stacked Borrows and again six under strict Tree Borrows (zero ignored); exact MIRIFLAGS were checked. Existing manifest/RUSTC_WRAPPER warnings remain visible. These receipts verify the owner on System and selected installed behavior, not complete allocator metadata or the runtime judge. Class: `bench`. `examples/_shared/j13b_owned_block.rs` owns one nonzero requested byte allocation with its exact Layout and borrowed allocator. It retains NonNull provenance, exposes only an exclusively borrowed MaybeUninit payload view, preserves old ownership on realloc failure, and frees once through RAII. A documented conditional Send implementation transfers unique byte ownership when the allocator is Sync; no Sync, Clone, pointer-to-integer reconstruction or safe raw-metadata hook is added. The allocator reference and explicit Drop encode lifetime/drop checking; there is no generic borrowed payload T requiring an additional variance marker.

The existing shape kernel remains the single implementation. Its new host-page entrypoint rejects invalid geometry and uses checked edge-page arithmetic; legacy preparation entrypoints retain their original offsets. Six tiny System-backed fixture tests cover requested-size/layout bounds, null allocation, real write offsets, growth/shrink/prefix retention, failed realloc, transfers and frees in both thread directions, observed worker panic cleanup, and invalid page geometry. A separate feature/platform-gated test binary installs SeferAlloc: two cohorts pass through the real owner helper, with owner-return sampling admitted once and foreign frees consumed as UNKNOWN without changing learning. It does not yet cover the complete campaign or both installed worker-pool directions.

CI requires seven positive native tests, including the installed boundary, and six actual-source tests under each strict-provenance Stacked/Tree Borrows configuration. The Miri fixture uses System, not the complete SeferAlloc metadata implementation: correctness item271 remains CONDITIONAL. The exact unsafe helper path now selects the existing seam group even when changed alone; planner self-tests discriminate it from ordinary safe example paths. No full-CI dispatch or production feature change is added. A first wiring check rejected missing canonical isolation flags; only the new steps were corrected, not the verifier. No local tests/models/workloads or new branches/worktrees are used. Full workload execution, per-window/OS/lifetime accounting and runtime report admission remain open.

## Accepted Task54 static reference — 2026-11-08

Status: accepted on `ac8906cb39a0db8de0a7c0a854d7b21144e38c84` by CI `42526023632` SUCCESS; final PR checks 42 SUCCESS / 23 intended SKIPPED. Personal log review confirmed 39 Node tests, 13 Rust workload tests and one explicitly activated static-reference native test PASS (zero ignored). Both production hashes remained unchanged; the overlay digest is `f6a4237c4ebc38f35165ec4ab7076d24e35a956c4f409e1bcbf88c0eac001b77`. Semantic selftest 214 cases / 316 mutants / zero failures, linter mutants 10/10 and protected range 16 commits / zero red passed. Expected dead-code warnings in the no-learning overlay and existing dependency warnings are recorded, not suppressed. These receipts accept the static reference, not the full runtime judge. Class: `bench`. The overlay changes only a disposable CI source copy, never shipping allocator source. It enables the existing Always controls, preserves fixed-one non-control advice and existing owner validation/sampling/counters, and bypasses only estimator admission. Two complete original-file hashes and exact text anchors are required; the receipt records both resulting file hashes and the overlay digest, not a whole-tree or binary identity.

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
