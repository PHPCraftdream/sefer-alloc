# CI tiers: which jobs run for which change

Running every job in `.github/workflows/ci.yml` for every push to a pull request made
fix series queue for an hour or more (measured on 2026-10-13: PR runs averaged ~85 minutes, up to 175
minutes with queueing) and re-ran Miri/Loom/macOS/Windows for pull requests that only touched documents.
Runs are therefore **tiered**: a job runs when the files of the change need it, and the complete set runs
on a schedule and on demand.

## File layout and decomposition contract

`ci.yml` is the orchestration graph: triggers, concurrency, permissions, job names/IDs,
conditions, dependencies, runners, matrices and job environment stay here. Cohesive
execution sequences live in repository-local composite actions under
`.github/actions/ci-<job>/action.yml`. Checkout stays before each local action call.
The original extraction moved steps into 51 local composite actions and reduced the
workflow from 5,437 to 954 lines without changing its 60 job definitions. It was a
maintainability change, not a faster allocator or authorization to run additional campaigns.

The workflow keeps cross-job outputs and matrices inline. `licenses` and
`docs-consistency` delegate their checks to local composite actions after checkout.
`aligned-vmem-hugetlb-real` and `globalalloc-model-miri` keep their checkout
and test runs inline while their setup moved to composite actions; similar commands are not automatically interchangeable.

`node scripts/verify-ci-decomposition.mjs` expands only actions actually called by
this workflow, verifies the pre-extraction digest receipt in
`docs/verification/ci-decomposition-baseline.json`, and runs cargo-free positive and
negative reader/selection/license fixtures. It runs in the existing always-on
consistency job. The receipt includes pending Task88 native-facts gates; it does not
claim that those tests have executed successfully. Two explicitly checked additions
are excluded from the historical comparison: this integrity step and the relocated
J13b source-path selector. Intentional future command/graph changes require a reviewed
update of the affected receipt digests, not removal of the gate or regeneration in CI.
`--record` is deliberately limited to the original pre-extraction workflow shape.

The `kani` job stays in the `seam` tier. P2.4 runs 12 `sefer-segment` and 8
`sefer-registry` harnesses with `-p <package> --all-features`, retains the
`sefer-concurrent` row, and removes the former root proof and temporary
`sefer-alloc-core` package-check rows. Historical timings in
[CI_LONG_JOBS.md](perf/CI_LONG_JOBS.md) are not timings for this split.

`scripts/ci/effective-workflow.mjs` is a restricted static reader, not a YAML execution
engine. Local action calls must be unconditional single-key calls after checkout.
Action inputs/outputs, nested local actions, unsupported contexts, missing files and
path escapes fail closed. Context-sensitive sequences remain inline. Source locations
are retained for sentinel diagnostics. Unused action files cannot satisfy wiring gates.

Composite steps require explicit shells. The `ci-default-shell` marker records a
validated equivalent of the original implicit shell: `bash -e {0}` on Unix, `pwsh`
on Windows, with a runner expression for the existing multi-OS matrix. Existing
explicit shells are unchanged; choosing explicit `bash` instead would add `pipefail`
and would not be a pure move.

Action-source edits select the groups of every owning job; shared-reader edits select
the union of its consumers. This does not set `full` or override weekly/dispatch-only
conditions or the existing main-push heavy-group suppression. J13b's own changed-path
guard includes its local action. License coverage follows reachable third-party uses.
The `licenses` job delegates to `.github/actions/ci-licenses/action.yml`; the cache
fingerprint currently hashes the workflow job block, not the expanded action body
(see [item 287](correctness-open-items/TRACKED_ci_gate_coverage.md#287-t-licensing-cache-fingerprint-omits-the-ci-composite-action-body-p25-187-2026-11-21)).

Exact-source acceptance: `d5c61f44d6d1fa0e72884cea5ef91397c03c69bb`, CI42762365242
SUCCESS, 59 successful jobs and four intended weekly/dispatch skips. Actual positive
logs confirm the decomposition gate, both repaired documentation guards, restored
Miri passes, and scoped Task88 native/model helpers. The [generated receipt](perf/J13B_TASK90_CI_RECEIPT.md)
preserves primary job conclusions and excerpts; the separate zero-cost receipt proves
only its declared production verification suffix. Child identity integration, reviewed
builder provenance and payload/admission remain downstream. No local builds, tests,
models or workloads were executed.

## Mechanics

- Job `changes` runs `scripts/ci-plan.mjs` over the diff (`pull_request`: PR base..merge commit; `push`:
  `before`..`sha`) and exposes one boolean output per **group**.
- Every gated job has `needs: changes` and one job-level `if: needs.changes.outputs.<group> == 'true'`.
  Job-level conditions (not workflow-level `paths:`) are deliberate: a skipped job is reported as
  "skipped", which branch protection treats as passing; a workflow skipped by `paths:` would leave required
  checks pending forever.
- `docs-consistency` is always on. Its local composite runs the doc/inventory tests (`no_stale_doc_references`,
  `r1_03_pool_docs_contract`, `unsafe_allow_placement`), the repository guard scripts that read README,
  ARCHITECTURE and `ci.yml`, the A1 layering lint (`node scripts/module-graph.mjs --check-backedges`
  against `docs/layering_baseline.csv`), and the cargo-free node suites of those scripts
  (`module_graph`, `layering_lint`, `classify_tests`). A documents-only change still cannot publish a
  stale count, and a `src/**` change cannot silently add an upward module edge.
- Concurrency: a newer push to the same PR cancels the older run (`cancel-in-progress` only for
  `pull_request`; main, schedule and dispatch runs are never cancelled).
- `licenses` (license compatibility of third-party code, plus cargo-deny `bans` and `sources`) is always
  on. It covers every Cargo workspace with a tracked `Cargo.lock` (root, `fuzz/`, the compile_fail
  fixtures) and every third-party GitHub Action: `scripts/license-check-workspaces.mjs --check` runs every
  time and fails on a workspace without a tracked lock, on an action missing from its reviewed list, and on
  an action whose license `deny.toml` does not allow. The cargo-deny check runs only when the dependency set
  changed since a run that passed. `scripts/license-check-fingerprint.mjs`
  hashes every tracked `Cargo.lock`, `deny.toml`, the workspace members'
  name/version/license/license-file/publish, the cargo-deny version, the `licenses` workflow job block,
  and both scripts. A composite-action-body-only change is not itself a fingerprint input.
  A pass stores an Actions-cache marker keyed `license-check-<fingerprint>`; a run that finds the
  marker skips the check and says so in its step summary. A miss (a new dependency set, or an evicted
  marker) runs the check. Pull-request caches are visible only to that pull request, so `main` keeps its own
  markers. Advisories stay in `deny` on every run: the advisory database moves without the graph changing.

## Groups

| Group | Selected by (changed path) |
|---|---|
| `always` | every change: fmt, commit-prefix-lint, evidence-registry, deny, licenses, docs-consistency, j13b-judge-preparation (path-selected steps) |
| `fast` | `src/**`, `crates/**`, `tests/**`, `benches/**`, `examples/**`, `fuzz/**`, `Cargo.*`, build scripts (`check-matrix`, `run-check-matrix`, `lib`, `check-all`), and any unrecognised path |
| `seam` | `src/**`, `crates/**`, `Cargo.*`, tests named `*loom*`/`*miri*`/`*kani*` or listed by `scripts/loom.mjs`/`miri*.mjs`, those manifests, the actual-source owner, operation core, persistent actors and training guard (`examples/_shared/j13b_owned_block.rs`, `examples/_shared/j13b_window_kernel.rs`, `examples/_shared/j13b_payload_actors.rs`, `examples/_shared/j13b_owner_training.rs`), and the OS/model boundary producer `examples/_shared/j13b_os_probe.rs` and its adaptive window executor `examples/_shared/j13b_adaptive_windows.rs` |
| `os` | `Cargo.lock`/toolchain/`.cargo`, a root `Cargo.toml` edit that is not only bench/example/test targets, `src/alloc_core/platform/**` (kept only as the deletion trigger of the P2.2 move) and its new location `crates/core/sefer-alloc-core/src/alloc_core/platform/**` (within the sefer-alloc-core crate — not the whole crate), `crates/core/sefer-registry/src/fallback.rs` (Windows exit lock protocol; not the whole registry crate), tests named `*windows*`/`*macos*`/`*numa*`/`*platform*`, the platform crates (`aligned-vmem`, `numa-shim`, `proc-*`, `sefer-platform`), and `examples/_shared/j13b_os_probe.rs` and its adaptive window executor `examples/_shared/j13b_adaptive_windows.rs` |
| `crate_*` | `crates/<theme>/<name>/**`: the gates of that one workspace crate (its dependents also get `fast`+`seam`) |
| everything | `schedule`, `workflow_dispatch`, and a `CI: full` trailer in the message of any commit of the range (PR: base..merge commit; push: before..sha) |

`j13b-judge-preparation` is in `always` but is not full CI: the job itself starts on every change, yet its
Node and cargo steps are skipped unless a J13b path changed (`scripts/j13b_*.mjs`, `examples/_shared/**`,
`examples/g1_j13b_*.rs`, `tests/j13b_*.{mjs,rs}`, `crates/platform/aligned-vmem/src/**`, `Cargo.toml`, `.github/workflows/ci.yml`) or the run is a
schedule/manual dispatch.

A change to `.github/workflows/ci.yml` or `scripts/ci-plan.mjs` selects `fast` (the planner self-test runs in
every `changes` job); it no longer selects the full set, and the `full-ci` PR label is gone.

Documents (`docs/**`, `*.md`, `*.csv`, `*.sha256`, `.rush/**`), other workflows and non-matrix scripts
select `always` only. An unrecognised path selects `fast` (conservative), never nothing. A merge to `main`
(`push`) runs the diff-selected groups without `seam` and `os`: the heavy groups already ran for its pull
request and the nightly run re-runs them on `main`.

## Crate-graph narrowing (P1.5b, level 1)

`scripts/ci-plan.mjs` narrows the coarse path rules with a workspace dependency
graph built from `cargo metadata --no-deps` (workspace members; all declared
manifest-level edges to path dependencies — normal, dev and build, including
optional and target-specific ones; `meta.resolve` is deliberately NOT read —
plus reference edges from a text scan of `#[path]`/`include!`/`include_str!`/
`include_bytes!`, built fresh in `scripts/ci/crate-graph.mjs`). For a diff
whose changed files lie under `crates/**`, the affected set is the changed
crates plus their transitive reverse dependents (workspace consumers).

- Per-crate `crate_*` groups run only when that crate is affected (not merely
  under `crates/**`).
- `fast` stays on for any crate change, unconditionally: the root test jobs
  inside it are narrowed by the test selector, and selection is shadow/enforce-off
  right now, so `fast` runs everything anyway.
- The heavy `seam`/`os` groups run only when the root crate `sefer-alloc` is
  affected — it is affected iff any of its direct workspace dependencies is
  affected, by declared Cargo edges or by reference edges (`#[path]`/
  `include!`/`include_str!`/`include_bytes!`)
  — or when a non-crate rule (source, tests, manifests, unknown paths) selected
  them.
- Fails closed: an unresolvable dependency path, an unresolvable `#[path]`/
  `include*!` reference, or a `cargo metadata` failure
  yields exactly the previous conservative behavior (crate change selects
  dependents' `fast`+`seam`, etc.). Root `Cargo.toml`, `Cargo.lock`, `build.rs`
  and `rust-toolchain` are unchanged by this rule and still select everything
  they always did.
- Scope unchanged: push-to-`main` still drops `seam`/`os`; `schedule`,
  `workflow_dispatch` and a `CI: full` trailer still force the full set.

## When the full set runs

- **Nightly** (`0 3 * * *`) on `main`: everything except the three weekly-only jobs.
- **Weekly** (`0 6 * * 1`): everything including `numa-real-kernel`, `fuzz-run` and `feature-powerset`.
- **Explicit trigger**: put `CI: full` on a line of its own in the message of a commit (a trailer, after the body).
  Any commit of the pushed range counts, so the next push, the PR run and the merge to `main` run the full set.
  This is the only in-repo way to ask for the full set; a prose mention is not a trigger.
- **On demand**: `gh workflow run ci.yml --ref <branch>` (full), or "Run workflow" in the Actions tab. Ordinary
  runs (push, pull request) stay available and follow the diff.
- **Sprint close**: the closing commit carries `CI: full`, and `node scripts/verify-sprint-close.mjs <sha>` confirms a
  green FULL run on exactly that SHA (every tiered job ran and succeeded; only the weekly-only and dispatch-only
  jobs may be skipped). The nightly is a safety net, not the close proof.
- Recommended, owner to confirm: a PR that changes an unsafe seam (`crates/core/sefer-alloc-core/src/**`, `crates/core/sefer-registry/src/**`,
  `src/global/**`) carries a `CI: full` commit before merge when it is about to be merged on its own.

The rule in `CLAUDE.md` "confirm CI went green on the landing SHA" now reads: green **for the class of the
landing diff**, plus a green nightly for the heavy groups.

## Feature powersets and rustdoc coverage

`feature-powerset` runs only on Mondays at 06:00 UTC or on manual dispatch, not on
push or PR even with `CI: full`. It typechecks combinations without running tests:

- `sefer-alloc` and other workspace members with features use depth 2 with
  `--no-dev-deps` (lib + bins, not all targets). The workflow discovers the
  remaining members via Cargo metadata; featureless members are not enumerated.
  The root keeps three targeted test cfg checks instead of an all-targets
  sweep (see [item 95](CORRECTNESS_OPEN_ITEMS.md)).
- `aligned-vmem` uses depth 2 with `--all-targets` to catch cfg mismatches in
  tests, benches and examples. Cargo-hack does not allow that flag with `--no-dev-deps`.
- The job also retains the root depth-3 sweep for selected allocation-policy
  features, the root MSRV each-feature check, and the PowerPC check that `std`
  compiles but `experimental` rejects targets without 64-bit atomics. Powerset
  checks run offline after one locked fetch because `--no-dev-deps` can require
  temporary lockfile reconciliation.

The `fast`-tier `docs` job treats rustdoc warnings as errors. It keeps the root's
stable all-features row and its separate manifest feature-list row (`production`,
with defaults). The workspace composite checks every other member on stable with
all features, then checks the exact docs.rs configuration where already declared:
`all-features`, or the listed `features` with defaults unless `no-default-features` opts out.
The action runs nightly `cargo doc` with declared `rustdoc-args` (e.g. `--cfg docsrs`)
passed via `CARGO_ENCODED_RUSTDOCFLAGS` alongside `-D warnings`; without declared
args the row uses stable `cargo doc`. For already-declared configurations, the
docs.rs row is skipped only when the manifest declares `all-features = true`
without rustdoc arguments. The dedicated `globalalloc-model` workflow also
reads its exact docs.rs features and rustdoc args from metadata and retains its
separate internal all-features row. These rows catch warnings specific to the
published feature set that an all-features build need not expose. Members without
`[package.metadata.docs.rs]` get only the all-features row from the workspace action;
adding declarations to EV-1-scoped manifests is deferred pending EV-1 reverify
(item 288 of `docs/CORRECTNESS_OPEN_ITEMS.md`). First finding of the new rows
(PR #285): the workspace all-features rustdoc row flagged a broken
self intra-doc link, in
`crates/core/sefer-alloc-core/src/alloc_core/platform/sidecar.rs`, repaired by
a single doc-comment line. That evidence-scoped source edit makes the close-out
PR request EV-1 re-verification for the eight affected rows, with the receipts
to be recorded in its `ci(evidence)` commit; the re-verification result is not
claimed here.

## Test-impact map (CI-T1 stage 2)

`.github/workflows/test-impact-map.yml` (nightly 04:00 UTC, on dispatch, and on a pull request carrying the label `test-impact-map`) builds every integration-test target
of two rows (`production internals`, `--all-features`) under `-C instrument-coverage`, one target at a time (the binary is deleted
after its coverage is exported, so the runner disk holds one instrumented binary), runs it, and records the
repository files with at least one covered line. Four shards per row run in parallel; a merge job per row unions them and runs
`node scripts/test-impact-map.mjs check`. The result is one artifact `test-impact-map-<row>.json` per row (30 days).
A test that a row compiles to nothing (crate-level `#![cfg(...)]`) has no covered file and is listed `no-coverage`
for that row. Format:

```text
{ "version": 1, "sha": "<commit>", "row": { "features": "...", "rustc": "..." },
  "tests": { "tests/<file>.rs": ["src/...", ...] },        sorted repo-relative paths
  "unmapped": { "tests/<file>.rs": "<reason>" } }          reasons: requires-features | test-failed | timeout | no-coverage
```

`check` fails unless every test target of the commit is exactly once in `tests` or `unmapped`, mapped paths are
sorted repo-relative and exist at that commit, and every unmapped reason is one of the four above. A target that
fails to build in the row fails the shard (a broken row must not look like a gap). The script's `--selftest`
(shard partition, cargo/llvm-cov parsing, merge, every `check` rejection) runs in `docs-consistency`.

Maps describe their recorded `sha` and rows, not necessarily the current commit. Coverage records files with
covered lines from the executions performed; it is not a static proof that every possible dependency or
unexecuted code path was attributed. Selection therefore depends on map freshness and the coverage runs' existing
assumptions; an omitted or stale attribution can limit what the selector knows.

### Test selection (CI-T1 stage 3)

Job `select-tests` (group `fast`) downloads the maps of the latest successful `test-impact-map` run on `main`
and runs `node scripts/select-tests.mjs select` over the diff (`--base`/`--head` as in `changes`). It never
fails: a missing map, a failed download or selector error yields `all`. The selection is written to the step
summary and to the artifact `test-selection`. Rules are conservative for paths the selector cannot attribute:

- A full run (`CI: full`, schedule, dispatch) selects all. `Cargo.toml`, `Cargo.lock`, `build.rs`, `.cargo/**`,
  toolchain and lint configs (`ALWAYS_ALL`) also select all.
- A changed test target selects itself. Otherwise, a changed file present in any map's `tests` entries selects
  every target mapped to that file across rows. This covering attribution wins before reader-data fallback.
  A target absent from every row's map is always selected, regardless of the changed paths.
- The five Kani-only segment and registry proof modules are ignored by test selection, as is the former
  `src/kani_proofs.rs` path for deletions and historical diffs. As with that legacy path, proof-only changes
  do not select Rust tests just because those tests read source files from disk. The separate structural limit
  check (`verify-structure-limits.mjs`) runs in always-on `docs-consistency` and also in `check-matrix`.
  An uncovered Rust file under `src/**`, `crates/*/*/src/**`, or `tests/**` selects all;
  a non-Rust file under `tests/**` is reader data instead.
- Reader data includes `docs/**`, `scripts/**`, `.github/**`, root-level `*.md|csv|json|txt`, non-`.rs`
  files under `tests/**`, Rust files under root `benches/**` or `examples/**`, and Rust files under
  `crates/*/*/benches/**` or `crates/*/*/examples/**`. When no map entry covers such a path, the selector picks
  file readers: targets and map-listed `tests/` sources containing `CARGO_MANIFEST_DIR`, `include_str!`,
  `include_bytes!`, `include!(...)`, `read_to_string`, `fs::read`, `File::open`, `read_dir`, or `Command::new`.
  The `include!` check is deliberately conservative: a textual mention marks the source as a reader.
- Other uncovered paths select all. A Node/test-data-only change is not guaranteed to produce an empty selection:
  file readers and targets unmapped in every row remain selected. A recognized reader-data-only diff yields
  subset `[]` when there are no file readers and no targets unmapped in every row.
- Level-1 crate-only rule (P1.5b): when every changed file maps to workspace
  crates (via the same `cargo metadata` graph as `ci-plan.mjs`) and none of
  them reaches the root crate `sefer-alloc` as a transitive reverse dependency,
  the selector returns a subset with a `crateOnly` field in its JSON output
  instead of falling through to reader data or `all`. That subset keeps the
  file-reader tests (readers may scan `crates/**` at runtime) and is empty only
  when no reader exists. If the root crate
  is affected, or any changed path is unmapped (or the graph cannot be built),
  the previous behavior applies unchanged.

`test-core` consumes the selection only when the repository variable `TEST_SELECTION` is `enforce` (default:
shadow mode reports the selection without changing the run). Its three `cargo test` steps then run `--lib` plus
the selected `--test <name>` targets that the row can build (`required-features` against the row's feature
closure), or plain `cargo test` when the selection is `all`. Even an empty subset retains `--lib`. A subset run
does not compile examples and benches and does not run doctests; the `clippy` rows (`--all-targets`) still cover
their compilation only for the package named by each invocation. Root-scoped rows cover the root package only, not every workspace crate's benches/examples; crate owners have their
own `--all-targets` rows for `aligned-vmem`, `numa-shim`, `sefer-region`, `once-ptr-cell`, `size-classes`,
`tagged-index-stack`, `globalalloc-model`, and `proc-memstat`, plus the CI-only `malloc-bench-rs` row in
`.github/actions/ci-test-workspace/action.yml`. Core allocator files are executed by most tests, so the saving
comes from peripheral and test-only changes; the selection summary shows what each diff got.

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
| `j13b-judge-preparation` | J13b judge preparation (schema + workload contract) | every change |
| `licenses` | license compatibility (only when dependencies changed) | every change |
| `check-matrix` | check-matrix (shard ${{ matrix.shard }}/${{ strategy.job-total }}) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `clippy` | clippy | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `docs` | rustdoc | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `fuzz-build` | fuzz targets build (nightly) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `ghost-zero-cost` | ghost-zero-cost (.text diff + Ir vs base) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `global-regressions` | global allocator regressions (G1/G3) | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `no_std` | no_std core build | `fast`: source, tests, benches, examples, fuzz, build scripts |
| `select-tests` | select tests (diff + test-impact map) | `fast`: source, tests, benches, examples, fuzz, build scripts |
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
| `miri-core-pin-cache` | miri (core, pin cache) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-core-rest` | miri (core, rest) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-core-tb` | miri (core, pin cache TreeBorrows) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-fastbin-integrity-nl7` | miri (fastbin, integrity nl7) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-fastbin-integrity-rest` | miri (fastbin, integrity rest) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-fastbin-oracles` | miri (fastbin, magazine oracles) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-plain-heavy` | miri-plain (heavy regressions) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `miri-plain-rest` | miri-plain (rest) | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `tsan` | thread-sanitizer | `seam`: source/crates/Cargo, loom/miri tests and manifests |
| `msrv` | MSRV 1.93 (cargo check + test --no-run) | `os`: Cargo/toolchain surface, platform code, platform crates |
| `msrv-runtime-windows` | MSRV 1.93 runtime (global_alloc_installed + global_alloc_mt, windows) | `os`: Cargo/toolchain surface, platform code, platform crates |
| `multi-arch` | test (${{ matrix.label }}) | `os`: Cargo/toolchain surface, platform code, platform crates |
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
| `sefer-alloc-core-gates` | sefer-alloc-core package gates | `crate_sefer_alloc_core`: changes under its crate |
| `sefer-concurrent-gates` | sefer-concurrent-gates | `crate_sefer_concurrent`: changes under its crate |
| `sefer-ghost-gates` | sefer-ghost package gates | `crate_sefer_ghost`: changes under its crate |
| `sefer-platform-gates` | sefer-platform package gates | `crate_sefer_platform`: changes under its crate |
| `sefer-policy-gates` | sefer-policy package gates | `crate_sefer_policy`: changes under its crate |
| `sefer-region-gates` | sefer-region package gates | `crate_sefer_region`: changes under its crate |
| `sefer-registry-gates` | sefer-registry package gates | `crate_sefer_registry`: changes under its crate |
| `sefer-segment-gates` | sefer-segment package gates | `crate_sefer_segment`: changes under its crate |
| `size-classes-gates` | size-classes package gates | `crate_size_classes`: changes under its crate |
| `tagged-index-stack-gates` | tagged-index-stack package gates | `crate_tagged_index_stack`: changes under its crate |
| `feature-powerset` | cargo-hack feature-powerset (depth 2, check-only, weekly) | weekly schedule or manual dispatch only |
| `fuzz-run` | fuzz targets run (weekly, 10 min/target) | weekly schedule or manual dispatch only |
| `numa-real-kernel` | NUMA on real Linux kernel (numactl smoke + env-guarded tests) | weekly schedule or manual dispatch only |
| `tis-weak-memory-wallclock-gate` | tagged-index-stack weak-memory wall-clock A/B gate (arm64) | manual dispatch only |
<!-- ci-tiers:end -->

`docs-consistency` re-generates the table and fails if it differs from `ci.yml`
(`node scripts/ci-plan.mjs --check-doc`). Regenerate with `node scripts/ci-plan.mjs --jobs-table`.

## Maintaining

- The path rules live in `scripts/ci-plan.mjs` (`classifyPath`); `node scripts/ci-plan.mjs --selftest`
  covers every rule, `node scripts/ci-plan.mjs --replay 10` classifies the last merged PRs.
- A new job must be added to the group map when it is wired (its `needs`/`if` are plain text in `ci.yml`).
- Known limits: the mapping is coarse (per top-level source directory), cross-OS and Miri coverage of a
  `src/**`-only PR is deferred to the nightly run, and the planner does not understand which test a change
  can affect. When in doubt, add a commit with a `CI: full` trailer.

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
