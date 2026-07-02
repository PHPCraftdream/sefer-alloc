// ThreadSanitizer sweep via WSL (Linux). Real data-race detection on real
// threads — the dimension neither loom (a bounded model) nor miri (effectively
// single-threaded for our tests) covers.
//
// Usage (from repo root):
//   node scripts/tsan.mjs                 # default cross-thread test set
//   node scripts/tsan.mjs race_repro heap_cross_thread   # explicit tests
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
  'heap_cross_thread',
];

const tests = process.argv.slice(2).length
  ? process.argv.slice(2)
  : DEFAULT_TESTS;

const wslRoot = winToWsl(REPO_ROOT);
const testArgs = tests.map((t) => `--test ${t}`).join(' ');

// One bash -lc line so the env scrubbing + cargo invocation share a shell.
const bashCmd = [
  `cd ${wslRoot}`,
  'unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER',
  [
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
    "--features 'alloc-global alloc-xthread alloc-decommit'",
    testArgs,
  ].join(' '),
].join(' && ');

console.log(`[tsan] tests: ${tests.join(', ')}`);
console.log(`[tsan] wsl: ${wslRoot}\n`);

const { code, out } = await run('wsl', ['bash', '-lc', bashCmd]);

// TSan reports races as warnings that do NOT fail the process exit code by
// default, so scan for its markers explicitly.
const ok = verdict('tsan', code, out, [
  'WARNING: ThreadSanitizer',
  'data race',
]);
process.exit(ok ? 0 : 1);
