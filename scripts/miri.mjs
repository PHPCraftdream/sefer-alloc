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
  // R3 (#155): fastbin / production-path miri coverage. The Э6 M2 oracle
  // strict-provenance claim (free path never touches the block body), the Э1
  // bump-direct carve pointer math (storm capped under cfg(miri)), and the Э3
  // own-segment cache invalidation on decommit.
  [
    'alloc-global alloc-xthread alloc-decommit fastbin',
    'regression_magazine_oracles',
  ],
  [
    'alloc-global alloc-xthread alloc-decommit fastbin',
    'regression_bump_direct_refill',
  ],
  // S3 (#168): the deterministic single-thread boundary sweep (S2) under miri —
  // UB-free pointer math / provenance across the size×align seam grid + the
  // realloc matrix. The grid is drastically shrunk under `cfg(miri)` inside the
  // test (a representative size/align subset, 4 realloc pairs, windowed canary)
  // so it finishes in ~40s; the native (non-miri) grid is exhaustive & unchanged.
  ['alloc-core', 'stress_boundary_sweep'],
  // `regression_own_segment_cache_invalidation` deferred from the miri set
  // (R3, #155): ~100k interpreted allocations (18_000 blocks × 6 segments,
  // count is invariant-load-bearing so it cannot be cfg(miri)-capped) does not
  // finish in a CI-acceptable time. Its UB surface is covered by
  // `decommit_miri_cycle`.
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
