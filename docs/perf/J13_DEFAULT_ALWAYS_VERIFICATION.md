# J13 default Always — implementation verification record

Date: 2026-11-11
Updated: 2026-11-12
Status: NOT DELIVERED — candidate correctness CI and IAI verification are incomplete

## Scope and immutable identities

- Verified base: `484f6944923843662c6814c7d0f78d081bc42b33`; actual-main CI `43103553837` completed SUCCESS.
- Initial implementation: `3fd1fb96a711c77509d5a5c534b33221d9fa493f`.
- Candidate branch: `j13-default-always-20261111`.
- Delivery PR: https://github.com/PHPCraftdream/sefer-alloc/pull/231.
- Initial exact-source correctness CI: https://github.com/PHPCraftdream/sefer-alloc/actions/runs/43112665508.

The runtime change is confined to the shared default tuple: Always on Linux with `alloc-decommit` and without `numa-aware`, Off elsewhere. Requested minimum 8 MiB, advice budget 0/unlimited, explicit overrides, first-config-wins, publication protocol, cache/release accounting, header layout and advice geometry are unchanged. This is not an Auto change or a new Large measurement campaign. Production feature composition is unchanged.

## Actual ghost-zero-cost result: SKIP, not proof

Job `130832701457` on the initial candidate completed successfully as a job but printed:

```text
[ghost-zero-cost] SKIP: the range declares a runtime change (perf(runtime) / perf(opt-in) / fix(perf)), has no verification-scoped commit and changes no ghost code or build-graph input
```

**ghost-zero-cost returned SKIP and is not proof of machine identity or Small-path instruction-count neutrality.** The truthful `perf(runtime)` prefix is retained. A green job containing this SKIP is not a measured zero Ir delta.

Authoritative log: https://github.com/PHPCraftdream/sefer-alloc/actions/runs/43112665508/job/130832701457. A session-local copy is `.rush/stdin/j13-default-3fd1fb96-zero-cost.log`; that scratch copy is not claimed to be a committed artifact.

## IAI protocol and current execution blocker

The owner selected the existing `perf-gate.yml` manual base/candidate protocol without workflow edits, new benchmarks or automatic triggers:

1. Run the existing workflow on verified main `484f6944` and verify a successful baseline cache save under `iai-baseline-main-484f6944923843662c6814c7d0f78d081bc42b33`.
2. After the candidate is pushed, run it once on `j13-default-always-20261111`.
3. Read the actual restore log: the baseline must be the exact base-SHA cache, not an earlier candidate-branch cache.
4. Require equal recorded rustc and Valgrind versions, and derive every benchmark's Ir delta from the actual paired outputs. Expected delta is zero. Disclose every nonzero value; a difference exceeding the owner's 10 percent bound stops delivery for owner review.
5. If the base moves or a candidate run must be repeated, inspect and delete only the candidate-branch IAI cache first; do not overwrite or delete the main reference silently.

**The `perf_gate_iai` suite does not contain a dedicated “1024 allocations / 1024 frees” cell.** Its actual benchmark set must be named in the eventual results, not replaced by an invented standalone result.

The requested first dispatch was attempted:

```text
gh workflow run perf-gate.yml --ref main
```

It created no run and returned:

```text
HTTP 422: Cannot trigger a 'workflow_dispatch' on a disabled workflow
```

The workflow API reported ID `306084949`, state `disabled_manually`. The attempt to enable this existing manual-only workflow was refused by the session's worktree confinement verifier. No alternate API or wrapper was used to bypass that refusal. The owner can execute `gh workflow enable perf-gate.yml` in the session; this does not require editing its YAML. The only active trigger in the reviewed file is manual dispatch.

Current evidence availability:

| Required fact | Current evidence |
|---|---|
| Successful base run ID | Unavailable: dispatch rejected before run creation |
| Exact saved base cache key | Unverified: no baseline run executed |
| Candidate IAI run ID | Not launched without a valid base |
| Actual restored cache key | Unavailable: no candidate IAI run |
| Base/candidate rustc equality | Unverified |
| Base/candidate Valgrind equality | Unverified |
| Per-benchmark base and candidate Ir | Unavailable; not zero |
| Per-benchmark Ir delta | Unavailable; no neutrality or performance GO claimed |

This blocks verification only. Implementation, tests and documentation continue. A source-level argument or the earlier Large wall-clock study is not a substitute for these missing instruction-count observations.

## Correctness-test repair found by actual CI

Initial candidate CI's Clippy job `130832702639` failed in the actual command:

```text
cargo clippy --all-targets --features "hardened medium-classes internals" -- -D warnings
```

Diagnostics included:

```text
error[E0433]: cannot find `policy` in `alloc_core`
  --> tests/j13b_thp_tuple_codec.rs:4:30
error[E0432]: unresolved imports `sefer_alloc::LargeThpCurve`, `sefer_alloc::LargeThpGradient`, `sefer_alloc::LargeThpMode`
```

Cause: the codec test's gate had been broadened to `alloc-core` plus `internals`, but its THP imports are intentionally exported only with `alloc-decommit`. The repair restores the original `internals` plus `alloc-decommit` test gate. It does not widen production visibility, remove a supported test case, or alter the Linux default predicate. That Clippy execution remains a failure; the overall initial run was later cancelled by the superseding source push, not turned into a pass. No rerun without a source fix is used to hide it. The repair commit `aba7cea27b7396857092bd85311ad537506edc6b` passed actual Clippy job `130848024206` in run `43117096166`; that run subsequently exposed the separate synthetic-fixture failure below.

Initial local Clippy checks targeted library feature combinations and therefore did not establish all-targets coverage. The repaired source passed all eight actual CI Clippy commands locally, including the hardened all-targets row, under the existing CPU cap. These were type/lint checks, not native test executions; exact-source CI acceptance remains required.

## Synthetic legacy-runner fixture isolation

Run `43117096166` on `aba7cea2` failed preparation job `130847893232` in `tests/g1_big_study_runner.mjs:71`: the synthetic legacy RESULT invocation returned `status: null`, not the expected 0. Its parent `spawnSync` has an 8000 ms deadline. The original assertion did not print `spawnSync.error`, so the precise stalled subprocess was not captured; this is not evidence of a THP runtime failure.

Source inspection found an avoidable dependency: unlike the JSON path, legacy provenance invokes real `rustc --version --verbose`, and the preparation action runs these synthetic process fixtures before its Rust toolchain installation. Synthetic RESULT parsing need not initialize rustup or depend on compiler availability. The focused fixture repair supplies a temporary Linux-only `rustc` metadata stub through that one invocation's PATH, rejects wrong probe arguments, and asserts that the actual saved receipt contains the synthetic metadata string. PATH changes do not escape the fixture invocation; real build and measurement provenance are untouched.

The existing 8000 ms outer deadline, 1000 ms child deadline, all four process-failure tests and legacy/JSON output assertions are retained. An explicit `assert.ifError(legacyResult.error)` now preserves a future parent-spawn error rather than reducing it to `null !== 0`. No runner implementation, study source budget, timing policy or workflow is changed. This repair requires actual Linux CI execution; it was not tested through local process execution. A session-local copy of the original failure is `.rush/stdin/j13-default-aba7cea2-preparation-failure.log`.

## Behavioral coverage and remaining gates

The default-policy test exercises real allocations around 8 MiB, validates activation counters and expected range geometry through `AllocCore`, and checks the GlobalAlloc facade boundary. Facade allocations are simultaneously live to prevent cross-case cache reuse from disguising a fresh-path test. It then publishes explicit Off after knobless use and checks that advice does not increase. Existing explicit-Off, conflict, budget, cache/release and platform oracles remain; the budget-conservation test now inherits default mode instead of setting Always explicitly.

No new unsafe production code or atomic protocol was introduced. Unsafe test allocation/deallocation calls use valid layouts and live uniquely owned pointers with their original allocator/layout. No new raw-pointer metadata hook was added.

Local type-checks and changed-file rustfmt passed without native test execution. Repository-wide `cargo fmt --check` could not start on Windows with `The filename or extension is too long. (os error 206)`; the actual CI formatting gate remains required. Native tests, Miri/Loom and performance execution are GitHub CI only.

Delivery still requires exact-source correctness acceptance, the owner's IAI evidence, a factual updated report, green actual-main landing CI and only then closure of superseded PR #224. Correctness item 264 remains OPEN. The completed registered Large study is not repeated. Tail trim, threshold sweep and adaptive Auto/budget remain later sequential packages.
