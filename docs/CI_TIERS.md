# CI tiers: which jobs run for which change

`.github/workflows/ci.yml` has ~50 jobs. Running all of them for every push to a pull request made
fix series queue for an hour or more (measured on 2026-10-13: PR runs averaged ~85 minutes, up to 175
minutes with queueing) and re-ran Miri/Loom/macOS/Windows for pull requests that only touched documents.
Runs are therefore **tiered**: a job runs when the files of the change need it, and the complete set runs
on a schedule and on demand.

## Mechanics

- Job `changes` runs `scripts/ci-plan.mjs` over the diff (`pull_request`: PR base..merge commit; `push`:
  `before`..`sha`) and exposes one boolean output per **group**.
- Every gated job has `needs: changes` and one job-level `if: needs.changes.outputs.<group> == 'true'`.
  Job-level conditions (not workflow-level `paths:`) are deliberate: a skipped job is reported as
  "skipped", which branch protection treats as passing; a workflow skipped by `paths:` would leave required
  checks pending forever.
- `docs-consistency` is always on and cheap: the doc/inventory tests (`no_stale_doc_references`,
  `r1_03_pool_docs_contract`, `unsafe_allow_placement`) and the repository guard scripts that read README,
  ARCHITECTURE and `ci.yml`. A documents-only change still cannot publish a stale count.
- Concurrency: a newer push to the same PR cancels the older run (`cancel-in-progress` only for
  `pull_request`; main, schedule and dispatch runs are never cancelled).
- `licenses` (license compatibility, plus cargo-deny `bans` and `sources`) is always on, but its check
  runs only when the dependency set changed since a run that passed. `scripts/license-check-fingerprint.mjs`
  hashes every input the verdict depends on: `Cargo.lock`, `deny.toml`, the workspace members'
  name/version/license/license-file/publish, the cargo-deny version, the `licenses` job text and the script
  itself. A pass stores an Actions-cache marker keyed `license-check-<fingerprint>`; a run that finds the
  marker skips the check and says so in its step summary. A miss (a new dependency set, or an evicted
  marker) runs the check. Pull-request caches are visible only to that pull request, so `main` keeps its own
  markers. Advisories stay in `deny` on every run: the advisory database moves without the graph changing.

## Groups

| Group | Selected by (changed path) |
|---|---|
| `always` | every change: fmt, commit-prefix-lint, evidence-registry, deny, licenses, docs-consistency |
| `fast` | `src/**`, `crates/**`, `tests/**`, `benches/**`, `examples/**`, `fuzz/**`, `Cargo.*`, build scripts (`check-matrix`, `run-check-matrix`, `lib`, `check-all`), and any unrecognised path |
| `seam` | `src/**`, `crates/**`, `Cargo.*`, tests named `*loom*`/`*miri*`/`*kani*` or listed by `scripts/loom.mjs`/`miri*.mjs`, those manifests |
| `os` | `Cargo.lock`/toolchain/`.cargo`, a root `Cargo.toml` edit that is not only bench/example/test targets, `src/alloc_core/platform/**`, tests named `*windows*`/`*macos*`/`*numa*`/`*platform*`, the platform crates (`aligned-vmem`, `numa-shim`, `proc-*`) |
| `crate_*` | `crates/<name>/**`: the gates of that one workspace crate (its dependents also get `fast`+`seam`) |
| everything | `schedule`, `workflow_dispatch`, PR label `full-ci`, and any change to `.github/workflows/ci.yml` or `scripts/ci-plan.mjs` |

Documents (`docs/**`, `*.md`, `*.csv`, `*.sha256`, `.rush/**`), other workflows and non-matrix scripts
select `always` only. An unrecognised path selects `fast` (conservative), never nothing. A merge to `main`
(`push`) runs the diff-selected groups without `seam` and `os`: the heavy groups already ran for its pull
request and the nightly run re-runs them on `main`.

## When the full set runs

- **Nightly** (`0 3 * * *`) on `main`: everything except the three weekly-only jobs.
- **Weekly** (`0 6 * * 1`): everything including `numa-real-kernel`, `fuzz-run` and `feature-powerset`.
- **On demand**: `gh workflow run ci.yml --ref <branch>` (full), or label a PR `full-ci`.
- **Sprint close**: the most recent nightly (or an on-demand full run on the closing SHA) must be green.
- Recommended, owner to confirm: a PR that changes an unsafe seam (`src/alloc_core/**`, `src/registry/**`,
  `src/global/**`) carries `full-ci` before merge when it is about to be merged on its own.

The rule in `CLAUDE.md` "confirm CI went green on the landing SHA" now reads: green **for the class of the
landing diff**, plus a green nightly for the heavy groups.

### Production behavior without `internals`

The only current behavioral CI row building exactly `production` without `internals` is
`msrv-runtime-windows`: it runs `global_alloc_installed` and `global_alloc_mt`
(`.github/workflows/ci.yml:2965`). This is a global-allocator smoke, not maintenance/fallback-hook
coverage. The explicit maintenance/fallback scenario rows at `ci.yml:1862-1885` enable `internals`;
the startup/fallback injection scenarios also enable `bench-internals` (`ci.yml:1878-1885`). In
`maintenance_service.rs`, pass counters and wait hooks are gated on `internals` (lines 48-51,
250-285), while startup/worker fault injection and pause hooks require both `internals` and
`bench-internals` (lines 52-59, 287-320). In `fallback.rs`, fallback-init panic injection and its
test accessors require `internals` (lines 705-733 and following). The lock-panic regression hook
`dbg_panic_in_with_heap_releases_lock` is gated only on `std` (line 645), but its integration test
requires `alloc-global` and `internals` (`tests/regression_fallback_panic_lock.rs:31`); the init
unwind test has the same feature gate (`tests/regression_fallback_init_unwind_guard.rs:51`).

`BitmapScan`'s R18 comparison path is separately gated by both `bench-internals` and the explicit
`r18_sidecar_scan_bench` cfg (for example, `bitmap_scan.rs:4-8, 20-28, 47-60`); its normal branch
continues to use the production `swap` scan. This makes the R18 prefilter an explicit benchmark
comparison rather than an ordinary-build path.

These hooks are integration-test observability/injection, not shipping APIs. Moving them would break
existing test callers or change which feature combinations expose test hooks; this task therefore
leaves their placement unchanged. The current production-behavior gap is explicit: no CI row runs the
maintenance/fallback hook scenarios under `production` without `internals`. `clippy-production` is
`--lib`-only and is not behavioral evidence. This paragraph records the boundary without claiming
the smoke row covers those scenarios.

## Jobs

<!-- ci-tiers:begin (generated by `node scripts/ci-plan.mjs --jobs-table`) -->
| Job | Name | Runs when |
|---|---|---|
| `changes` | plan (which job groups this change needs) | every change |
| `commit-prefix-lint` | commit-prefix-lint (R30-12 taxonomy, PR range + pushed range) | every change |
| `deny` | cargo-deny advisories | every change |
| `docs-consistency` | docs consistency (always) | every change |
| `evidence-registry` | evidence-registry | every change |
| `fmt` | rustfmt | every change |
| `ghost-guard` | ghost-guard (semantic lint + ghost self-test) | every change |
| `licenses` | license compatibility (only when dependencies changed) | every change |
| `check-matrix` | check-matrix (manifest-driven, non-clippy rows) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `clippy` | clippy | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `docs` | rustdoc | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `fuzz-build` | fuzz targets build (nightly) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `ghost-zero-cost` | ghost-zero-cost (.text diff + Ir vs base) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `global-regressions` | global allocator regressions (G1/G3) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `no_std` | no_std core build | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `test-core` | test (core tiers) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `test-feature-isolation` | test (feature isolation) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `test-gated-bodies` | test (gated bodies + all-features) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `test-ghost` | test (ghost build) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `test-hardened` | test (hardened tier) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `test-workspace` | test workspace members | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `test-xthread` | test (xthread progression) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `asan` | address-sanitizer (alloc-core) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `kani` | kani proofs | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `loom-alloc-global` | loom (alloc-global) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `loom-experimental` | loom (experimental) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `loom-misc` | loom (misc) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `loom-xthread` | loom (alloc-core alloc-xthread) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-alloc-core` | miri (alloc-core) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-core` | miri (core) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-fastbin` | miri (fastbin) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-plain` | miri-plain | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `tsan` | thread-sanitizer | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `msrv` | MSRV 1.93 (cargo check + test --no-run) | `os`: Cargo/toolchain surface, platform code, platform crates |
| `msrv-runtime-windows` | MSRV 1.93 runtime (global_alloc_installed + global_alloc_mt, windows) | `os`: Cargo/toolchain surface, platform code, platform crates |
| `multi-arch` | test (${{ matrix.target }}) | `os`: Cargo/toolchain surface, platform code, platform crates |
| `test-macos` | test macos (production) | `os`: Cargo/toolchain surface, platform code, platform crates |
| `test-windows` | test windows (production) | `os`: Cargo/toolchain surface, platform code, platform crates |
| `aligned-vmem-gates` | aligned-vmem package gates | `crate_aligned_vmem`: changes under its crate |
| `aligned-vmem-hugetlb-real` | aligned-vmem (real hugetlb pool, Linux) | `crate_aligned_vmem`: changes under its crate |
| `aligned-vmem-miri` | aligned-vmem under miri (interpreter, not just --cfg miri check) | `crate_aligned_vmem`: changes under its crate |
| `globalalloc-model-miri` | globalalloc-model under miri (interpreter, strict-provenance included) | `crate_globalalloc_model`: changes under its crate |
| `numa-shim-macos` | numa-shim real macOS kernel (unsupported-platform contract) | `crate_numa_shim`: changes under its crate |
| `numa-shim-macos-miri` | numa-shim macOS under miri (mod platform cfg regression guard) | `crate_numa_shim`: changes under its crate |
| `numa-shim-mock` | numa-shim mock dispatch tests | `crate_numa_shim`: changes under its crate |
| `numa-shim-windows` | numa-shim real Windows kernel (correctness, not perf) | `crate_numa_shim`: changes under its crate |
| `once-ptr-cell-gates` | once-ptr-cell package gates | `crate_once_ptr_cell`: changes under its crate |
| `once-ptr-cell-miri` | once-ptr-cell under miri (interpreter, strict-provenance included) | `crate_once_ptr_cell`: changes under its crate |
| `proc-memstat-gates` | proc-memstat gates (${{ matrix.os }}) | `crate_proc`: changes under its crate |
| `proc-probe-gates` | proc-probe gates | `crate_proc`: changes under its crate |
| `sefer-region-gates` | sefer-region package gates | `crate_sefer_region`: changes under its crate |
| `size-classes-gates` | size-classes package gates | `crate_size_classes`: changes under its crate |
| `tagged-index-stack-gates` | tagged-index-stack package gates | `crate_tagged_index_stack`: changes under its crate |
| `feature-powerset` | cargo-hack feature-powerset (depth 2, check-only, weekly) | weekly schedule or manual dispatch only |
| `fuzz-run` | fuzz targets run (weekly, 10 min/target) | weekly schedule or manual dispatch only |
| `numa-real-kernel` | NUMA on real Linux kernel (numactl smoke + env-guarded tests) | weekly schedule or manual dispatch only |
| `tis-weak-memory-wallclock-gate` | tagged-index-stack weak-memory wall-clock A/B gate (arm64) | manual dispatch only |
<!-- ci-tiers:end -->
<!-- ci-tiers:end -->

`docs-consistency` re-generates the table and fails if it differs from `ci.yml`
(`node scripts/ci-plan.mjs --check-doc`). Regenerate with `node scripts/ci-plan.mjs --jobs-table`.

## Maintaining

- The path rules live in `scripts/ci-plan.mjs` (`classifyPath`); `node scripts/ci-plan.mjs --selftest`
  covers every rule, `node scripts/ci-plan.mjs --replay 10` classifies the last merged PRs.
- A new job must be added to the group map when it is wired (its `needs`/`if` are plain text in `ci.yml`).
- Known limits: the mapping is coarse (per top-level source directory), cross-OS and Miri coverage of a
  `src/**`-only PR is deferred to the nightly run, and the planner does not understand which test a change
  can affect. When in doubt, label the PR `full-ci`.

## Gate runs are PR checks

`.github/workflows/r20-gates.yml` is a separate workflow, not part of the job
table above. It drives the pre-registered Ir gate runners
(`scripts/r20_*_run.mjs`) and has two modes: the owner's manual
`workflow_dispatch` (inputs unchanged) and a pull-request mode.

To run the gates on a PR:

1. Add a `.github/gate-run.json` to the PR branch, e.g.
   `{"runner": "scripts/r20_prf01_sidecar_summary_run.mjs", "stages": "prepare,build,oracle,measure", "rehearsal": false, "slop": false, "outDir": "docs/perf/R20_PRF01_SIDECAR_SUMMARY_G0_RUN01"}`.
   An official PRF-01 gate run MUST point `outDir` at a FRESH directory:
   the default `docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01` already contains
   `manifest.json`, and the runner refuses to overwrite it. `outDir` is
   incompatible with `rehearsal: true` (the runner rejects `--out-dir` for
   rehearsal stages). Fields: `outDir` (optional, `docs/perf/R20_<NAME>` or
   `target/r20<name>`) and `note` (optional, single-line, at most 200
   characters, no control characters).
2. Add the `gate-run` label to the PR (labeling, a branch update, or a reopen
   while the label is present all trigger the run).

Gate config is validated by `scripts/r20-gate-config.mjs` (`--selftest` runs
in CI, in the `docs-consistency` job of `ci.yml`). The `runner` must be a file
under `scripts/` matching `^scripts/r20_[a-z0-9_]+_run\.mjs$` and existing on
disk; the `stages` field uses the same whitelist (comma-separated, no
duplicates). An invalid or missing `.github/gate-run.json` is a red check with
an `::error::` message in the log. All runners are GitHub-hosted
`ubuntu-24.04` machines — nothing here runs self-hosted. The `gate-run` label
can only be added by a user with triage/write access to the repository, so a
run cannot be triggered from a fork without a maintainer's participation.

What you see: a "r20 gate runner" check on the PR, a step summary with the
environment, `verdict.txt` and (for the prf01 sidecar runner) the acceptance
result, and an artifact named `r20-gates-<PR number>`. PR runs never cancel
each other (concurrency is keyed on the PR number, `cancel-in-progress:
false`).

Security note: `pull_request` events can also fire from forks. The workflow
carries no secrets and checks out with `persist-credentials: false`, so a
forked PR cannot exfiltrate tokens through it; the runner path and stages are
still validated against the same allowlists before any invocation.

### A/B iai runs on a PR

`scripts/r20_r6_ab_iai_run.mjs` is a generic Ir A/B runner for the round-6
perf units: arm A is `git merge-base HEAD origin/main` (or an explicit
`baseSha` in the config), arm B is the PR HEAD. Both arms are built and
measured in their own worktrees (`target/r6ab/{A,B}`) and target directories,
using one iai bench target (default `perf_gate_iai`) with the default feature
set `production bench-internals internals` (the same set `scripts/iai.mjs` uses).

Example `.github/gate-run.json`:

```json
{"runner": "scripts/r20_r6_ab_iai_run.mjs", "stages": "prepare,build,oracle,measure", "outDir": "target/r20r6ab", "benchTarget": "perf_gate_iai", "features": "production bench-internals internals", "cells": "small_churn_16b,cold_alloc_only_256x16b", "thresholds": {"small_churn_16b": 1.0}}
```

Config fields (all optional except `runner`, validated by
`scripts/r20-gate-config.mjs`): `benchTarget`, `features`, `cells` (explicit
cell list; without it every cell both arms print is compared and the sets
must match), `baseSha` (explicit arm A), `thresholds` (map cell -> max
tolerated regression in %; without it the verdict table is informational
only). Outputs in `outDir`: `manifest.json` (immutable source identity:
SHAs, features, rustc, OS — written before any measurement), raw iai logs
under `logs/`, `summary.csv` (cell, A_Ir, B_Ir, delta, delta_pct, SHA A/B,
features, rustc, OS) and `verdict.txt` (script-generated table; the verdict
is a pure threshold check). The `--oracle` stage is an extension point: a
path-activation oracle over the `bench-internals` counters is not wired yet
and the stage is a documented no-op. The runner is GitHub-CI-only (Linux);
`node scripts/r20_r6_ab_iai_run.mjs --selftest` (runs in the
`docs-consistency` job) covers the pure parts — iai output parsing, the
ratio math (asserts: non-positive A counts and implausible percentages are
hard errors), CSV/table generation and the threshold verdict.
