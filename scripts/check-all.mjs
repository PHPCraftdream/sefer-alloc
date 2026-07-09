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
//   1. cargo fmt --all -- --check           (rustfmt gate)
//   2. cargo clippy --all-targets -- -D warnings                (CI matrix entry 1: "")
//   3. cargo clippy --all-targets --features experimental -- -D warnings  (entry 2)
//   4. cargo clippy --all-targets --all-features -- -D warnings           (entry 3)
//   5. cargo test --features production                          (default prod suite)
//   6. cargo test --features "production alloc-runfreelist"       (+ the opt-in arc)
//   7. npm run iai                                                (deterministic judge,
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

const steps = [
  {
    name: 'rustfmt',
    cmd: 'cargo',
    args: ['fmt', '--all', '--', '--check'],
  },
  {
    name: 'clippy ()',
    cmd: 'cargo',
    args: ['clippy', '--all-targets', '--', '-D', 'warnings'],
  },
  {
    name: 'clippy (--features experimental)',
    cmd: 'cargo',
    args: ['clippy', '--all-targets', '--features', 'experimental', '--', '-D', 'warnings'],
  },
  {
    name: 'clippy (--all-features)',
    cmd: 'cargo',
    args: ['clippy', '--all-targets', '--all-features', '--', '-D', 'warnings'],
  },
  {
    name: 'test (--features production)',
    cmd: 'cargo',
    args: ['test', '--features', 'production'],
  },
  {
    name: 'test (--features "production alloc-runfreelist")',
    cmd: 'cargo',
    args: ['test', '--features', 'production alloc-runfreelist'],
  },
  // C2 (bug-hunt review 2026-07-09): kept in lockstep with the CI `test`
  // job's feature matrix (.github/workflows/ci.yml). These tiers carry
  // tests whose bodies are `#[cfg(feature = "...")]`-gated, so only a run
  // WITH the feature actually exercises them.
  {
    name: 'test (--features "production alloc-stats")',
    cmd: 'cargo',
    args: ['test', '--features', 'production alloc-stats'],
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
];

console.log(`[check-all] repo: ${REPO_ROOT}`);
console.log(`[check-all] running ${steps.length + 1} step(s) (fmt, clippy x3, test x2, iai) — fails fast\n`);

let allOk = true;
for (const step of steps) {
  console.log(`\n============================================================`);
  console.log(`  ${step.name}`);
  console.log(`============================================================`);
  const { code } = await run(step.cmd, step.args, { cwd: REPO_ROOT, shell: true });
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
  const { code } = await run('node', ['scripts/iai.mjs'], { cwd: REPO_ROOT, shell: true });
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
