// miri sweep — UB detection under strict provenance on the invariant / segment
// / align-regression tests. Native (nightly miri component). Mirrors the CI
// miri matrix in .github/workflows/ci.yml.
//
// Usage (from repo root):
//   node scripts/miri.mjs           # the full CI miri matrix
//   node scripts/miri.mjs decommit_miri_cycle   # a subset (by test name)
//   npm run miri
//
// Each entry is [features, testName]; miri is slow (segment tests run 1-8 min
// each), so keep the set to the focused invariant/UB targets per the project's
// short-scenario policy — not the whole suite.

import { REPO_ROOT, run, verdict } from './lib.mjs';

const MATRIX = [
  ['experimental', 'region_invariants'],
  ['alloc-core alloc-decommit', 'decommit_miri_cycle'],
  ['alloc-global alloc-xthread', 'reclaim_offset_unit'],
  ['alloc-core', 'regression_large_align_no_segment_exhaustion'],
  ['alloc-core', 'regression_page_aligned_no_segment_exhaustion'],
  ['alloc-core', 'regression_realloc_cross_class_shrink'],
];

const filter = process.argv.slice(2);
const entries = filter.length
  ? MATRIX.filter(([, t]) => filter.includes(t))
  : MATRIX;

const env = {
  ...process.env,
  MIRIFLAGS: '-Zmiri-strict-provenance -Zmiri-disable-isolation',
};

let allOk = true;
for (const [features, test] of entries) {
  console.log(`\n[miri] ${test} (features: ${features})`);
  const { code, out } = await run(
    'cargo',
    ['+nightly', 'miri', 'test', '--features', features, '--test', test],
    { cwd: REPO_ROOT, env, shell: true },
  );
  allOk = verdict(`miri:${test}`, code, out) && allOk;
}

console.log(`\n[miri] overall: ${allOk ? 'PASS' : 'FAIL'}`);
process.exit(allOk ? 0 : 1);
