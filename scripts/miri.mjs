// miri sweep — UB detection under strict provenance on the invariant / segment
// / align-regression tests. Native (nightly miri component). Mirrors the CI
// miri matrix in .github/workflows/ci.yml.
//
// Usage (from repo root):
//   node scripts/miri.mjs           # the full CI miri matrix (strict provenance)
//   node scripts/miri.mjs decommit_miri_cycle   # a subset (by test name)
//   node scripts/miri.mjs --plain   # concurrent terminal allocator matrix
//   node scripts/miri.mjs --plain regression_xthread_large_free_no_leak
//   node scripts/miri.mjs --tree-borrows r8_global_box_provenance
//   npm run miri
//
// Each entry is [features, testName, packageName?, exactTest?]. Keep Miri
// focused on bounded invariant/UB targets, not whole native workloads.
// packageName omission means the root package. exactTest selects one genuine
// invariant within its single target and requires that exact result sentinel.

import { REPO_ROOT, run, verdict } from './lib.mjs';

const MATRIX = [
  ['experimental', 'region_invariants'],
  // R2-05 (independent src review round 2, task #2007): the `dbg_*`
  // diagnostic accessors previously read allocator metadata through a
  // caller-derived pointer whose ADDRESS matched a live segment but had NO
  // provenance over it (constructed via `ptr::without_provenance_mut`) --
  // strict-provenance UB even though the membership check passed. Small,
  // feature-minimal target (`alloc-core internals` only) per this file's
  // short-scenario policy; see the test's own module doc for the full
  // mechanism and why the remaining feature-gated accessors are covered
  // functionally (not under miri) by `regression_r2_05_diag_provenance`.
  ['alloc-core internals', 'regression_r2_05_diag_provenance_miri'],
  // R34-5-followup (task #524): `internals` added — `decommit_miri_cycle`'s
  // `#![cfg(...)]` gate (added by R34-3/task #522) requires it; without it
  // this entry silently compiled to 0 tests (the "pass by absence" class
  // R13-5 fixed elsewhere in this project — this script was simply never
  // updated when R34-3 introduced the `internals` feature).
  ['alloc-core alloc-decommit internals', 'decommit_miri_cycle'],
  ['alloc-core', 'regression_large_align_no_segment_exhaustion'],
  ['alloc-core', 'regression_page_aligned_no_segment_exhaustion'],
  ['alloc-core', 'regression_realloc_cross_class_shrink'],
  // R2-1 (T2): the move-leg OOB-read guard. The bogus `realloc(16b, 8 MiB,
  // 8 MiB)` scenario would `copy_nonoverlapping` 8 MiB out of a 4 MiB
  // segment — a read that escapes the segment's OS allocation. Under the fix
  // the span-consistency check returns null before any allocation/copy, so
  // this run validates the GREEN path is UB-free (the RED path's 8 MiB alloc
  // + OOB read is too slow for the miri matrix; verified RED under native
  // `cargo test` instead). Strict-provenance-clean (own-thread substrate
  // path, real sefer pointer, `contains_base`-proven base).
  ['alloc-core', 'regression_realloc_oob_old_layout'],
  // R3 (#155): fastbin / production-path miri coverage. The Э6 M2 oracle
  // strict-provenance claim (free path never touches the block body), the Э1
  // bump-direct carve pointer math (storm capped under cfg(miri)), and the Э3
  // own-segment cache invalidation on decommit.
  // R34-5-followup: `internals` added — this test's `#![cfg(...)]` gate
  // (added by R34-3/task #522) requires it.
  [
    'alloc-global alloc-xthread alloc-decommit fastbin internals',
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
  // PERF-PASS-2 (G5/C1, task #50): the virgin-segment `AllocBitmap` init-
  // elision poison-then-assert counterfactual. Under `cfg(miri)` the skip
  // does NOT fire (miri's `std::alloc` fallback is not guaranteed zeroed, so
  // the explicit zero-init stays unconditional there — see the matching
  // comments at both call sites) — so T1/T2's "reads back zero" assertions
  // hold trivially under miri regardless of the skip. What miri DOES usefully
  // scrutinise here is the new `dbg_alloc_bitmap_bytes_for` test-only
  // accessor's raw pointer-offset read loop (new code, `Node::offset`/
  // `Node::read_u8` in a loop over a caller-provided `out` slice) and T3's
  // M2 double-free-guard exercise on a freshly-reserved segment, for
  // strict-provenance UB.
  // R34-5-followup: `internals` added — this test's `#![cfg(...)]` gate
  // (added by R34-3/task #522) requires it.
  ['alloc-core internals', 'regression_virgin_bitmap_skip'],
  // W3: the stats-aggregator Stacked-Borrows counterfactual. The default
  // (non-ignored) test asserts the W3 shape — counter read off a shared
  // `&Slot`, never forming `&HeapCore` over the owner's protected `&mut` — is
  // SB-clean. The `#[ignore]`d `old_pattern_is_sb_ub` in the same file
  // reproduces the pre-W3 UB on demand (run with `-- --ignored`). Tiny and
  // fast under miri (no segment reservation — it models the aliasing shape).
  ['std', 'regression_w3_stats_aliasing_miri'],
  // R7-A5: directory sidecar below-threshold path under strict provenance.
  // The above-threshold path (materialising 32+ segments) is impractically
  // slow under miri; the below-threshold path exercises the null-pointer
  // guard + publish helpers + try_materialise early return.
  ['alloc-segment-directory', 'segment_directory_a5_miri'],
  // The extracted tagged-index-stack crate's unchecked-storage domain oracle.
  // It has no crate features; keep the package-qualified invocation separate
  // from the root-package feature matrix entry shape.
  ['', 'narrow_domain_unchecked_storage', 'tagged-index-stack'],
  // Genuine tiny installed-global Box ownership/reborrow round trip.
  ['alloc-global internals bench-internals', 'r8_global_box_provenance', undefined, 'installed_box_drop_narrow_transfer_and_reissue'],
  // Separate exact invocation: owner retirement before the actual Box Drop
  // producer resumes after its terminal RMW. This test is cfg(miri)-only.
  ['alloc-global internals bench-internals', 'r8_global_box_provenance', undefined, 'installed_box_drop_retires_before_terminal_producer_resumes'],
  // `regression_own_segment_cache_invalidation` deferred from the miri set
  // (R3, #155): ~100k interpreted allocations (18_000 blocks × 6 segments,
  // count is invariant-load-bearing so it cannot be cfg(miri)-capped) does not
  // finish in a CI-acceptable time. Its UB surface is covered by
  // `decommit_miri_cycle`.
];

// Concurrent terminal-sidecar publication and owner drain. Use the retained
// behavioral allocator regressions, not the removed deferred/ring models.
const PLAIN_MATRIX = [
  ['alloc-global alloc-xthread alloc-decommit internals bench-internals', 'regression_xthread_large_free_no_leak'],
  ['alloc-global alloc-xthread internals bench-internals', 'regression_xthread_thread_free_alias_miri'],
  ['alloc-global alloc-xthread internals bench-internals', 'regression_xthread_small_ring_miri'],
  // One target per invocation: exact filtering must not swallow other binaries.
  ['alloc-global alloc-xthread internals bench-internals', 'r6_terminal_owner_drain', undefined, 'requested_sizes_one_through_seven_retire_once_and_reissue'],
];

const args = process.argv.slice(2);
const plain = args.includes('--plain');
const treeBorrows = args.includes('--tree-borrows');
// The positional args are TEST NAMES (the second column of each MATRIX entry).
// They are NOT feature names: an entry with several features
// (`'alloc-global alloc-xthread alloc-decommit fastbin'`) must be selected as a
// whole by its test name — never token-matched against the space-joined feature
// string. Filter strictly on the test name (column 2) to keep that distinction.
const filter = args.filter((a) => a !== '--plain' && a !== '--tree-borrows');
const matrix = plain ? PLAIN_MATRIX : MATRIX;
const knownTests = new Set(matrix.map(([, t]) => t));

// Regression-guard against the silent-0-runs class of bug (task #29 in loom.mjs;
// task #18 here): a filter token that matches NO test name — a stale/typo test
// name, or a bare feature token mistaken for a test name (e.g. `alloc-decommit`,
// one of the feature words of a multi-feature entry) — must hard-fail loudly, not
// silently drop to an empty run. Validate every requested name up front.
const unknown = filter.filter((t) => !knownTests.has(t));
if (unknown.length) {
  console.error(
    `[miri] unknown test name(s): ${unknown.join(', ')} — not a test in the ${
      plain ? 'PLAIN_MATRIX' : 'MATRIX'
    }. Pass test names (the second matrix column), not feature names.`,
  );
  console.error(`[miri] known tests: ${[...knownTests].join(', ')}`);
  process.exit(2);
}

const entries = filter.length
  ? matrix.filter(([, t]) => filter.includes(t))
  : matrix;

// Smoke-guard (mirrors loom.mjs): report the resolved entry count and hard-fail
// if it is ZERO — a matrix/filter combination should never resolve to an empty
// run, which would look green while validating nothing.
console.log(
  `[miri] ${plain ? 'PLAIN' : 'strict'}${treeBorrows ? ' TreeBorrows' : ''} matrix: ${entries.length} entr${
    entries.length === 1 ? 'y' : 'ies'
  } selected — ${
    entries
      .map(
        ([features, test, packageName]) =>
          `${test} (package: ${packageName || 'root'}, features: ${
            features.trim() || '(none)'
          })`,
      )
      .join(', ') || '(none)'
  }`,
);
if (entries.length === 0) {
  console.error(
    `[miri] FAIL: 0 entries selected — stale/empty matrix or filter matched nothing`,
  );
  process.exit(2);
}

// Keep strict and non-strict runs separate. The plain run's elevated
// preemption rate exercises terminal publication inside an owner alloc frame;
// these are retained behavioral allocator regressions, not legacy ring models.
const env = {
  ...process.env,
  MIRIFLAGS: [
    plain ? '-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5' : '-Zmiri-strict-provenance -Zmiri-disable-isolation',
    ...(treeBorrows ? ['-Zmiri-tree-borrows'] : []),
  ].join(' '),
};

let allOk = true;
for (const [features, test, packageName, exactTest] of entries) {
  const packageArg = packageName ? ['-p', packageName] : [];
  const featuresArg = features.trim();
  const featuresArgs = featuresArg ? ['--features', featuresArg] : [];
  console.log(
    `\n[miri] ${test} (package: ${packageName || 'root'}, features: ${
      featuresArg || '(none)'
    })`,
  );
  // `run()` defaults to `shell: false`, so the space-joined `features` value
  // reaches cargo as ONE argv element (no shell to re-split it on whitespace).
  // The previous COMMA-join here existed only to dodge `shell: true`'s
  // whitespace-splitting (DEP0190); cargo accepts `--features "a b c"` and
  // `--features "a,b,c"` identically either way. Package-only entries omit
  // `--features` entirely rather than passing an empty value.
  const { code, out } = await run(
    'cargo',
    [
      '+nightly',
      'miri',
      'test',
      ...packageArg,
      ...featuresArgs,
      '--test',
      test,
      ...(exactTest ? ['--', '--exact', exactTest] : []),
    ],
    { cwd: REPO_ROOT, env },
  );
  const ranSomething = /running [1-9]\d* test/.test(out);
  const exactPassed = !exactTest || out.includes(`test ${exactTest} ... ok`);
  if (!ranSomething || !exactPassed) {
    console.log(`[miri:${test}] FAIL — no selected invariant completed`);
  }
  allOk = verdict(`miri:${test}`, code, out) && ranSomething && exactPassed && allOk;
}

console.log(`\n[miri] overall: ${allOk ? 'PASS' : 'FAIL'}`);
process.exit(allOk ? 0 : 1);
