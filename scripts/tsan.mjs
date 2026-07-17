// ThreadSanitizer sweep via WSL (Linux). Real data-race detection on real
// threads — the dimension neither loom (a bounded model) nor miri (effectively
// single-threaded for our tests) covers.
//
// Usage (from repo root):
//   node scripts/tsan.mjs                 # default cross-thread test set
//   node scripts/tsan.mjs race_repro global_alloc_mt      # explicit tests
//   npm run tsan
//
// Requires: WSL with a nightly toolchain + `rust-src` component (for
// -Zbuild-std). TSan is Linux-only; the Windows MSVC toolchain cannot run it.
//
// The three traps this encapsulates:
//   1. RUSTC_WRAPPER / CARGO_BUILD_RUSTC_WRAPPER are inherited from the Windows
//      environment into WSL and point at `sccache.exe` — a Windows binary that
//      cannot drive the Linux rustc. We `unset` both inside the WSL shell.
//   2. TSan needs an instrumented std, so `-Zbuild-std --target
//      x86_64-unknown-linux-gnu` — which means a from-scratch std build; we use
//      a dedicated `/tmp/sefer-tsan` target dir so it never collides with the
//      Windows `target/` (different object format) or a prior non-TSan build.
//   3. Both RUSTFLAGS and RUSTDOCFLAGS need `-Zsanitizer=thread`.

import { REPO_ROOT, winToWsl, run, verdict } from './lib.mjs';

const DEFAULT_TESTS = [
  'race_repro',
  'race_norecycle',
  'global_alloc_mt',
  // task #204: `heap_cross_thread` was removed along with the `Heap` type it
  // exercised (no faithful HeapCore-facing substitute — see the CI `tsan` job
  // comment). `regression_xthread_large_free_no_leak` is the surviving
  // cross-thread reclaim test (drives a remote free of a Large segment over
  // the current `HeapCore` face) and compiles under this pass's feature set.
  'regression_xthread_large_free_no_leak',
];

const tests = process.argv.slice(2).length
  ? process.argv.slice(2)
  : DEFAULT_TESTS;

const wslRoot = winToWsl(REPO_ROOT);

// R3 (#155): the production feature set (fastbin + decommit) covers the
// magazine layer + the Э5 non-atomic per-heap counters + the #133 cross-heap
// stats() aggregation that the `alloc-*` set alone leaves uninstrumented. These
// two MT tests drive the teardown/recycle handshake and the remote-counter
// read.
const PROD_TESTS = [
  'global_alloc_mt',
  // task #204: `heap_cross_thread` (the `Heap`-face MT test formerly listed
  // here) was removed with no faithful HeapCore-facing substitute; its
  // coverage lives on via the other MT tests in this list (see the CI `tsan`
  // job comment).
  'tls_heap_teardown_ordering_stress',
  'regression_percounter_perheap_aggregation',
  // W6: the Large cross-thread FREE path (A1 deferred-large / abandoned-seg
  // exposed-provenance stacks) had no TSan coverage — the two production steps
  // above only exercise the small-block cross-thread races and the Э5 counter
  // reads. Both of these are genuine MT tests (each spawns a non-owner thread
  // that remotely frees a Large segment, then joins): `regression_realloc_
  // xthread_stamp` drives the W4/MUST-1 realloc → cross-thread-free path, and
  // `regression_xthread_large_free_no_leak` drives the A1 Large cross-thread
  // reclaim over the `HeapCore` face (task #204 renamed it from
  // `regression_heap_xthread_large_free_no_leak` when the `Heap` type it
  // originally referenced was removed). Both compile under the `production`
  // set (production ⊇ alloc-global ⊇ alloc, + alloc-xthread).
  'regression_realloc_xthread_stamp',
  'regression_xthread_large_free_no_leak',
  // S3 (#168): the concurrent boundary-stress hammer (S1) under TSan — the
  // highest-value race surface (magazine / RemoteFreeRing / Э5 counters under
  // boundary pressure). Its per-thread op budget and thread cap are slashed for
  // the sanitizer via SEFER_STRESS_OPS / SEFER_STRESS_MAX_THREADS (see
  // STRESS_ENV) so the run stays ~sub-second; native behavior is unchanged.
  'stress_concurrent_boundaries',
];

// S3 (#168): env that bounds the S1 stress test under the sanitizer. Unset in a
// native run → the test keeps its full default budget. Harmless to the other
// production TSan tests (they don't read these vars). Applied to the production
// pass only.
const STRESS_ENV = ['SEFER_STRESS_OPS=600', 'SEFER_STRESS_MAX_THREADS=4'];

function bashCmd(features, testList, extraEnv = []) {
  const testArgs = testList.map((t) => `--test ${t}`).join(' ');
  // One bash -lc line so the env scrubbing + cargo invocation share a shell.
  return [
    `cd ${wslRoot}`,
    'unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER',
    [
      ...extraEnv,
      // `unset` alone is not enough: WSL interop re-injects the Windows
      // RUSTC_WRAPPER (sccache.exe) into child processes, and a `bash -lc`
      // login shell may re-source it. Setting both to empty STRINGS directly on
      // the cargo process is what actually disables the wrapper (cargo treats an
      // empty RUSTC_WRAPPER as "no wrapper"); sccache.exe is a Windows binary
      // that cannot drive the Linux rustc.
      'RUSTC_WRAPPER=',
      'CARGO_BUILD_RUSTC_WRAPPER=',
      "RUSTFLAGS='-Zsanitizer=thread'",
      "RUSTDOCFLAGS='-Zsanitizer=thread'",
      'CARGO_TARGET_DIR=/tmp/sefer-tsan',
      'cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu',
      '--release',
      `--features '${features}'`,
      testArgs,
    ].join(' '),
  ].join(' && ');
}

// Pass 1: the cross-thread set (explicit args override the default set).
// Pass 2 (only when running the default set): the production config over the
// MT tests, so `npm run tsan` mirrors the CI `tsan` job's two steps.
// Each pass is [features, testList, extraEnv]. The production pass carries the
// S1 stress-budget env (STRESS_ENV); the cross-thread pass needs none.
const passes = process.argv.slice(2).length
  ? [['alloc-global alloc-xthread alloc-decommit', tests, []]]
  : [
      ['alloc-global alloc-xthread alloc-decommit', tests, []],
      ['production', PROD_TESTS, STRESS_ENV],
    ];

console.log(`[tsan] wsl: ${wslRoot}\n`);

let allOk = true;
for (const [features, testList, extraEnv] of passes) {
  console.log(`[tsan] features: ${features} | tests: ${testList.join(', ')}`);
  const { code, out } = await run('wsl', [
    'bash',
    '-lc',
    bashCmd(features, testList, extraEnv),
  ]);
  // TSan reports races as warnings that do NOT fail the process exit code by
  // default, so scan for its markers explicitly.
  const ok = verdict(`tsan:${features}`, code, out, [
    'WARNING: ThreadSanitizer',
    'data race',
  ]);
  allOk = ok && allOk;
}
process.exit(allOk ? 0 : 1);
