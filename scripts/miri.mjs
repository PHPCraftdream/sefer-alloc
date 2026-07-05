// miri sweep — UB detection under strict provenance on the invariant / segment
// / align-regression tests. Native (nightly miri component). Mirrors the CI
// miri matrix in .github/workflows/ci.yml.
//
// Usage (from repo root):
//   node scripts/miri.mjs           # the full CI miri matrix (strict provenance)
//   node scripts/miri.mjs decommit_miri_cycle   # a subset (by test name)
//   node scripts/miri.mjs --plain   # the PLAIN-provenance matrix (exposed-
//                                    # provenance stacks; see PLAIN_MATRIX below)
//   node scripts/miri.mjs --plain regression_heap_xthread_large_free_no_leak
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
  // W3: the stats-aggregator Stacked-Borrows counterfactual. The default
  // (non-ignored) test asserts the W3 shape — counter read off a shared
  // `&Slot`, never forming `&HeapCore` over the owner's protected `&mut` — is
  // SB-clean. The `#[ignore]`d `old_pattern_is_sb_ub` in the same file
  // reproduces the pre-W3 UB on demand (run with `-- --ignored`). Tiny and
  // fast under miri (no segment reservation — it models the aliasing shape).
  ['std', 'regression_w3_stats_aliasing_miri'],
  // `regression_own_segment_cache_invalidation` deferred from the miri set
  // (R3, #155): ~100k interpreted allocations (18_000 blocks × 6 segments,
  // count is invariant-load-bearing so it cannot be cfg(miri)-capped) does not
  // finish in a CI-acceptable time. Its UB surface is covered by
  // `decommit_miri_cycle`.
];

// W6: the PLAIN-provenance matrix. `src/registry/bootstrap.rs` (~lines 126-136)
// documents that the exposed-provenance intrusive stacks — the A1
// `deferred_large` push/drain stack and the `abandoned_segs` stack — pack real
// pointer addresses via `expose_provenance` and re-derive them via
// `with_exposed_provenance_mut` BY DESIGN. That wildcard-provenance shape is
// rejected under `-Zmiri-strict-provenance` (correctly — it is the documented
// structural limit, not a bug), so these tests get ZERO miri coverage in the
// strict MATRIX above. Run them under PLAIN miri (Stacked Borrows, non-strict
// provenance — miri's default) instead: the `push.rs` / `drain.rs` /
// `heap_registry.rs` / `node.rs` pairs ARE validatable there. Small N per test
// (Large allocs, <=100 iterations) keeps each run miri-affordable. Kept SEPARATE
// from the strict MATRIX — a strict-clean test must NOT move here and vice-versa.
// Under plain miri the `expose_provenance`/`with_exposed_provenance_mut` pairs
// surface as integer-to-pointer cast WARNINGS (validated) — strict miri would
// hard-ERROR on the same casts, which is the whole reason for a plain job.
// Verified locally: `regression_xthread_large_free_no_leak` → 3 passed (~156s).
//
// NOT here: the explicit-`Heap`-face tests
// (`regression_heap_xthread_large_free_no_leak`,
// `regression_xthread_large_free_layout_mismatch`) call `Heap::new()` on a
// SPAWNED thread; that thread's per-thread primordial 4 MiB segment goes
// unreachable at thread exit, so miri's leak checker reports it — a per-thread-
// `Heap` miri artifact, NOT the exposed-provenance path (its p2i re-derivations
// warn cleanly there too). Suppressing it needs `-Zmiri-ignore-leaks`, which
// would void the "no_leak" oracle. Their cross-thread reclaim is covered on
// REAL threads under TSan (see scripts/tsan.mjs) instead.
const PLAIN_MATRIX = [
  // A1 deferred-large stack over the `SeferAlloc`/`HeapCore` (global) face.
  ['alloc-global alloc-xthread', 'regression_xthread_large_free_no_leak'],
];

const args = process.argv.slice(2);
const plain = args.includes('--plain');
const filter = args.filter((a) => a !== '--plain');
const matrix = plain ? PLAIN_MATRIX : MATRIX;
const entries = filter.length
  ? matrix.filter(([, t]) => filter.includes(t))
  : matrix;

// The strict job pins `-Zmiri-strict-provenance`; the plain job DROPS it (the
// exposed-provenance re-derivations require the default, non-strict model). Both
// keep `-Zmiri-disable-isolation`.
const env = {
  ...process.env,
  MIRIFLAGS: plain
    ? '-Zmiri-disable-isolation'
    : '-Zmiri-strict-provenance -Zmiri-disable-isolation',
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
