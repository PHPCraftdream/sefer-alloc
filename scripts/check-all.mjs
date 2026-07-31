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
// What it runs, in order (fails fast — stops at the first red step):
//   0. node scripts/argv-roundtrip-test.mjs   (shell:false argv regression; R27-9)
//   1. cargo fmt --all -- --check           (rustfmt gate)
//   2-6. the 5 `clippy` rows from scripts/check-matrix.mjs's PER_PR_ROWS
//      (R30-5: GENERATED, not hand-written — default / experimental /
//      --all-features / hardened medium-classes / production; the last two
//      are NEW as of R30-5, see that manifest's header for why)
//   7. cargo test --features production                          (default prod suite)
//   8-10. cargo test x3 more feature combos (alloc-stats+bench-internals,
//      pinning, --all-features)
//   11. the 1 remaining (non-clippy) PER_PR_ROWS row — `cargo check --bench
//      perf_gate_iai --features "production bench-internals"` (R30-5:
//      scripts/iai.mjs's own DEFAULT_FEATURES and npm run check's own final
//      step — the exact command R29-16's 4x E0433 broke, now an
//      independent standalone check of its own)
//   12. node scripts/verify-perf-gate-stubs.mjs   (R30-5: generated "feature
//      ABSENT" stub check for benches/perf_gate_iai.rs's library_benchmark_group!)
//   13. node scripts/verify-gate-report.mjs   (R31-5a: structural checks over
//      every docs/perf/R*_*.md gate report — companion CSV exists, valid
//      40-hex SHA/no placeholder, cited raw logs exist)
//   14. npm run iai                                                (deterministic judge,
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
  // R30-5: the 5 PER_PR_ROWS clippy rows (default / experimental /
  // --all-features / hardened medium-classes / production), generated —
  // see the comment above `clippyRows`. Byte-identical argv to the
  // pre-R30-5 hand-written steps for the first 3; `hardened medium-classes`
  // and plain `production` are NEW here (they previously ran only in CI's
  // `clippy` job / not at all, respectively).
  ...clippyRows,
  {
    name: 'test (--features production)',
    cmd: 'cargo',
    args: ['test', '--features', 'production'],
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
    // `#![cfg]`.
    name: 'test (--features "production alloc-stats bench-internals")',
    cmd: 'cargo',
    args: ['test', '--features', 'production alloc-stats bench-internals'],
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
  // R30-5: the remaining (non-clippy) PER_PR_ROWS row — currently just
  // `check-perf-gate-iai-default` (`cargo check --bench perf_gate_iai
  // --features "production bench-internals"`, scripts/iai.mjs's own
  // DEFAULT_FEATURES and the exact command R29-16's 4x E0433 broke).
  ...otherRows,
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
];

console.log(`[check-all] repo: ${REPO_ROOT}`);
console.log(`[check-all] running ${steps.length + 1} step(s) (argv-roundtrip, fmt, clippy x5 [generated], test x4, perf-gate check [generated], verify-perf-gate-stubs, verify-gate-report, iai) — fails fast\n`);

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
