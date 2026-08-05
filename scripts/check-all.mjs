// One command, every pre-push gate. Runs the exact checks CI runs (fmt,
// clippy across the full feature matrix, tests across the main feature
// combos) plus the deterministic iai judge, so a red CI run is caught here
// FIRST — not after a push.
//
// Why this exists: this session pushed 17 commits and only discovered CI
// was broken (rustfmt drift from the PERF-3 phases, two workflow jobs
// pointing at test files deleted in task #204) by watching the Actions run
// AFTER pushing. `npm run check` is the single command that should have
// caught all of that beforehand — run it before every push.
//
// Usage (from repo root):
//   node scripts/check-all.mjs
//   npm run check
//
// What it runs, in order (fails fast — stops at the first red step). Step
// numbers below are kept in sync with the actual runtime step list by hand;
// a numbering drift here (a doubled "7" between the last clippy row and the
// first `cargo test` row) was finding F5 of
// `docs/reviews/2026-08-05-hs-new-waves-release-readonly-review.md`, fixed
// in the same pass that made the runtime banner derive its row count from
// `clippyRows.length` (task #587/F9, commit `650b818`):
//   0. node scripts/argv-roundtrip-test.mjs   (shell:false argv regression; R27-9)
//   1. cargo fmt --all -- --check           (rustfmt gate)
//   2-7. the 6 `clippy` rows from scripts/check-matrix.mjs's PER_PR_ROWS
//      (R30-5: GENERATED, not hand-written — default / experimental /
//      --all-features / hardened medium-classes internals / production /
//      production internals)
//   8. cargo test --features production                          (default prod suite)
//   9-11. cargo test x3 more feature combos (alloc-stats+bench-internals,
//      pinning, --all-features)
//   12-13. the 2 remaining (non-clippy) PER_PR_ROWS rows — `cargo check --bench
//      perf_gate_iai --features "production bench-internals"` (R30-5:
//      scripts/iai.mjs's own DEFAULT_FEATURES and npm run check's own final
//      step — the exact command R29-16's 4x E0433 broke, now an
//      independent standalone check of its own), plus the internals-boundary
//      test (R34 review F1: runs r34_3_internals_boundary_api.rs WITHOUT
//      `internals` so the guard is non-vacuous)
//   14. node scripts/verify-internals-negative-boundary.mjs   (Sol-F1,
//      task #563, release-readiness review finding F1: the REAL compile-fail
//      oracle for the negative half of the `internals` boundary —
//      `AllocCore::dbg_carve_batch` must NOT compile without `internals` and
//      MUST compile with it; see that script's own header)
//   15. node scripts/verify-alloc-core-dbg-internals-exhaustive.mjs   (H2,
//      task #572, Sol-remediation review finding H2: the EXHAUSTIVE
//      structural complement to 14 — 14 only proves ONE method is gated;
//      this enumerates and checks EVERY `AllocCore::dbg_*` method across
//      `src/alloc_core/*.rs`; see that script's own header)
//   16. node scripts/verify-perf-gate-stubs.mjs   (R30-5: generated "feature
//      ABSENT" stub check for benches/perf_gate_iai.rs's library_benchmark_group!)
//   17. node scripts/verify-gate-report.mjs   (R31-5a: structural checks over
//      every docs/perf/R*_*.md gate report — companion CSV exists, valid
//      40-hex SHA/no placeholder, cited raw logs exist)
//   18. node scripts/verify-commit-prefixes.mjs   (R31-5c: commit-prefix
//      lint for CLAUDE.md's R30-12 perf(runtime)/perf(opt-in)/bench/
//      docs(config) taxonomy, local default range — see that script's own
//      header; the precise PR-scoped complement runs as ci.yml's
//      `commit-prefix-lint` job)
//   19. npm run iai                                                (deterministic judge,
//      requires WSL + valgrind — see scripts/iai.mjs; skipped with a warning if
//      WSL is unavailable, since this is the one step that can't run on a bare
//      Windows/Linux CI runner without the WSL layer this repo's dev scripts use)
//
// This does NOT replace CI (CI additionally runs miri, loom, TSan, multi-arch,
// no_std, MSRV — see .github/workflows/ci.yml) — it is the FAST subset that
// catches the most common drift (fmt, clippy, the two main test combos, and
// an instruction-count regression) in a few minutes on the dev host, so most
// pushes never need a red CI run to discover a problem.

import { REPO_ROOT, run } from './lib.mjs';
import { PER_PR_ROWS, rowToCargoArgs, rowLabel } from './check-matrix.mjs';

// R30-5 (task #454): every row in `scripts/check-matrix.mjs`'s `PER_PR_ROWS`
// (the single source of truth also consumed by `scripts/run-check-matrix.mjs`,
// which `.github/workflows/ci.yml`'s `check-matrix` job runs) is generated
// into a step here — each row runs EXACTLY ONCE in this script, split into
// `clippyRows` (spliced in among the hand-written steps below, at the same
// position the 3 hardcoded clippy steps used to occupy, so the step order
// this script has always documented stays intuitive) and `otherRows` (the
// non-clippy rows — currently just the perf-gate `check` row — appended
// near the end, since there is no pre-existing hand-written step for them
// to slot in next to).
const clippyRows = PER_PR_ROWS.filter((r) => r.kind === 'clippy').map((row) => ({
  name: `[matrix] ${rowLabel(row)}`,
  cmd: 'cargo',
  args: rowToCargoArgs(row),
}));
const otherRows = PER_PR_ROWS.filter((r) => r.kind !== 'clippy').map((row) => ({
  name: `[matrix] ${rowLabel(row)}`,
  cmd: 'cargo',
  args: rowToCargoArgs(row),
}));

const steps = [
  {
    // R27-9 (task #427): tooling-correctness precondition for every later
    // step — they all spawn through the same run() in lib.mjs, so a
    // regression in run()'s shell:false argv preservation would corrupt
    // every multi-word --features value before cargo ever saw it. Runs in
    // milliseconds and fails fast. Wired in explicitly (not left as a
    // direct-invocation-only script) so it cannot silently rot — the same
    // failure mode tests/no_stale_loom_files.rs (R13-5) was created to catch.
    name: 'argv-roundtrip (shell:false regression test)',
    cmd: 'node',
    args: ['scripts/argv-roundtrip-test.mjs'],
  },
  {
    name: 'rustfmt',
    cmd: 'cargo',
    args: ['fmt', '--all', '--', '--check'],
  },
  // R30-5: the 6 PER_PR_ROWS clippy rows (default / experimental /
  // --all-features / hardened medium-classes internals / production /
  // production internals), generated — see the comment above `clippyRows`.
  // Byte-identical argv to the pre-R30-5 hand-written steps for the first
  // 3; `hardened medium-classes internals` and plain `production` were NEW
  // as of R30-5 (they previously ran only in CI's `clippy` job / not at
  // all, respectively); `production internals` was added later still
  // (R34-3's `internals` feature).
  ...clippyRows,
  {
    // R34-3 (task #522, finding B1): `internals` added — this repo's own
    // `tests/` suite reaches `alloc_core`/`global`/`registry` directly, and
    // that module path now requires the `internals` feature (additive over
    // `alloc-core`/`alloc-global`, NOT implied by `production`).
    name: 'test (--features "production internals")',
    cmd: 'cargo',
    args: ['test', '--features', 'production internals'],
  },
  // C2 (bug-hunt review 2026-07-09): kept in lockstep with the CI `test`
  // job's feature matrix (.github/workflows/ci.yml). These tiers carry
  // tests whose bodies are `#[cfg(feature = "...")]`-gated, so only a run
  // WITH the feature actually exercises them.
  {
    // R24-6 (task #384): `bench-internals` added so
    // `tests/class_aware_dirty_oom_latch.rs` (which now additionally gates on
    // that feature — see `Cargo.toml`'s `bench-internals` doc) keeps running
    // under this step instead of silently being skipped by its own
    // `#![cfg]`. `internals` added (R34-3/task #522) — same whole-suite
    // rationale as the row above.
    name: 'test (--features "production alloc-stats bench-internals internals")',
    cmd: 'cargo',
    args: ['test', '--features', 'production alloc-stats bench-internals internals'],
  },
  {
    name: 'test (--features pinning)',
    cmd: 'cargo',
    args: ['test', '--features', 'pinning'],
  },
  {
    name: 'test (--all-features)',
    cmd: 'cargo',
    args: ['test', '--all-features'],
  },
  // R30-5: the remaining (non-clippy) PER_PR_ROWS rows — currently
  // `check-perf-gate-iai-default` (`cargo check --bench perf_gate_iai
  // --features "production bench-internals"`, scripts/iai.mjs's own
  // DEFAULT_FEATURES and the exact command R29-16's 4x E0433 broke), plus
  // `test-internals-boundary-no-internals` (R34 review F1: the `internals`
  // boundary guard must run WITHOUT `internals` to be non-vacuous).
  ...otherRows,
  {
    // Sol-F1 (task #563, release-readiness review finding F1): the REAL
    // compile-fail oracle for the NEGATIVE half of the `internals` boundary
    // — `test-internals-boundary-no-internals` above only proves the
    // POSITIVE half (stable re-exports resolve without `internals`); this
    // step proves `AllocCore::dbg_carve_batch` genuinely does NOT compile
    // without `internals` (and DOES compile with it). See
    // scripts/verify-internals-negative-boundary.mjs's own header.
    name: 'verify-internals-negative-boundary (Sol-F1 compile-fail oracle)',
    cmd: 'node',
    args: ['scripts/verify-internals-negative-boundary.mjs'],
  },
  {
    // H2 (task #572, Sol-remediation review finding H2): the single-method
    // oracle immediately above (`verify-internals-negative-boundary.mjs`)
    // proved exactly ONE `AllocCore::dbg_*` method is `internals`-gated; it
    // does not prove the other 127. This step is the exhaustive structural
    // complement — enumerates EVERY `AllocCore::dbg_*` method across
    // `src/alloc_core/*.rs` and asserts each is gated or explicitly
    // allowlisted. See scripts/verify-alloc-core-dbg-internals-exhaustive.mjs's
    // own header for the full rationale (this is the exact gap that let 31
    // methods across 3 files stay reachable without `internals` for a full
    // remediation wave, undetected).
    name: 'verify-alloc-core-dbg-internals-exhaustive (H2 exhaustive gating check)',
    cmd: 'node',
    args: ['scripts/verify-alloc-core-dbg-internals-exhaustive.mjs'],
  },
  {
    // R30-5: generated "feature ABSENT" compile-check enumeration for every
    // conditionally-registered iai arm in benches/perf_gate_iai.rs — the
    // mechanical, automatic form of the stub rule R29-16 violated by
    // omission. See scripts/verify-perf-gate-stubs.mjs's header for the full
    // rationale.
    name: 'verify-perf-gate-stubs (generated feature-ABSENT check)',
    cmd: 'node',
    args: ['scripts/verify-perf-gate-stubs.mjs'],
  },
  {
    // R31-5a (task #480): structural checks over every docs/perf/R*_*.md gate
    // report — companion summary CSV cited actually exists, its commit/SHA
    // field(s) are a real 40-hex SHA (not a prose placeholder, the exact
    // defect class R30-6's placeholder and R29-13's invalid 63-char hash
    // both were), and every cited `_raw_*.log` filename actually exists on
    // disk. Zero cargo invocations, pure text/regex scan — see
    // scripts/verify-gate-report.mjs's header for the full rationale and its
    // documented, individually-verified retroactive-exemption list for
    // reports predating the relevant CLAUDE.md rule.
    name: 'verify-gate-report (structural gate-report checks)',
    cmd: 'node',
    args: ['scripts/verify-gate-report.mjs'],
  },
  {
    // R31-5c (task #482): commit-prefix lint for CLAUDE.md's R30-12
    // taxonomy (perf(runtime)/perf(opt-in)/bench/docs(config)) — see
    // scripts/verify-commit-prefixes.mjs's header for the full rule. Local
    // default range is `@{u}..HEAD` (or the last 40 commits if no upstream
    // is configured) — a reasonable local approximation of "this session's
    // unpushed work"; the precise PR-scoped complement
    // (`base.sha..head.sha`) runs as its own `commit-prefix-lint` CI job
    // (needs `fetch-depth: 0`, which this project's other jobs deliberately
    // don't pay for — see that job's own comment in
    // .github/workflows/ci.yml). Pure `git log`/`git show --stat` text
    // scanning, zero cargo invocations — runs in well under a second.
    name: 'verify-commit-prefixes (R30-12 taxonomy lint, local default range)',
    cmd: 'node',
    args: ['scripts/verify-commit-prefixes.mjs'],
  },
];

console.log(`[check-all] repo: ${REPO_ROOT}`);
console.log(`[check-all] running ${steps.length + 1} step(s) (argv-roundtrip, fmt, clippy x${clippyRows.length} [generated], test x4, perf-gate check + internals-boundary test [generated], verify-internals-negative-boundary, verify-alloc-core-dbg-internals-exhaustive, verify-perf-gate-stubs, verify-gate-report, verify-commit-prefixes, iai) — fails fast\n`);

let allOk = true;
for (const step of steps) {
  console.log(`\n============================================================`);
  console.log(`  ${step.name}`);
  console.log(`============================================================`);
  const { code } = await run(step.cmd, step.args, { cwd: REPO_ROOT });
  if (code !== 0) {
    console.log(`\n[check-all] FAIL at step: ${step.name} (exit ${code})`);
    allOk = false;
    break;
  }
  console.log(`\n[check-all] OK: ${step.name}`);
}

if (allOk) {
  console.log(`\n============================================================`);
  console.log(`  npm run iai (deterministic instruction-count judge)`);
  console.log(`============================================================`);
  const { code } = await run('node', ['scripts/iai.mjs'], { cwd: REPO_ROOT });
  if (code !== 0) {
    console.log(`\n[check-all] FAIL at step: iai (exit ${code}) — if this is "WSL not found" ` +
      `or similar environment failure (not a real regression), treat iai as a manual ` +
      `follow-up rather than blocking on it here.`);
    allOk = false;
  } else {
    console.log(`\n[check-all] OK: iai`);
  }
}

console.log(`\n============================================================`);
console.log(allOk ? '[check-all] ALL GREEN — safe to push' : '[check-all] FAILED — fix before pushing');
console.log(`============================================================`);
process.exit(allOk ? 0 : 1);
