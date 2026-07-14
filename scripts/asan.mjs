// AddressSanitizer sweep via WSL (Linux). Instruments the allocator's OWN
// raw-pointer memory operations (the `os` mmap/munmap seam, the intrusive
// free-list `node` seam, the realloc copy legs) with byte-granularity shadow
// memory — the out-of-bounds / use-after-free dimension that complements
// Miri (strict-provenance UB), TSan (data races), Loom (concurrency models),
// Kani (proofs) and proptest.
//
// Usage (from repo root):
//   node scripts/asan.mjs                 # default ASan target
//   node scripts/asan.mjs asan_alloc_core # explicit test
//   npm run asan
//
// Requires: WSL with a nightly toolchain + `rust-src` component (for
// -Zbuild-std). ASan (`-Zsanitizer=address`) is Linux/macOS only in the Rust
// toolchain; the Windows MSVC target cannot run it — hence the WSL path,
// identical to scripts/tsan.mjs.
//
// The exact invocation ASan needs (researched, current as of nightly-2026):
//   RUSTFLAGS="-Zsanitizer=address"
//   RUSTDOCFLAGS="-Zsanitizer=address"
//   cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu --release
// `-Zsanitizer=address` rewrites the codegen so every load/store is prefixed
// with a shadow-memory check. `-Zbuild-std --target x86_64-unknown-linux-gnu`
// is MANDATORY: ASan only instruments code compiled WITH the flag, and the
// prebuilt sysroot std is NOT — so std must be rebuilt from source (hence
// `rust-src`) targeting the host triple explicitly (a bare `--target`less
// `-Zsanitizer` build silently leaves std uninstrumented and misses bugs in
// std's own memory ops the allocator may depend on). A dedicated
// `/tmp/sefer-asan` target dir keeps the Linux ASan objects from ever
// colliding with the Windows `target/` (different object format) or a prior
// non-ASan build.
//
// CRITICAL: a custom `#[global_allocator]` under ASan is unsound. ASan installs
// its own allocator interceptors over `malloc`/`free`/`mmap`; installing
// SeferAlloc as the process `#[global_allocator]` would route every Rust
// allocation (including ASan's own internal bookkeeping) through the allocator
// under test and clash with the interceptors. So this sweep runs
// `tests/asan_alloc_core.rs`, which drives `AllocCore` DIRECTLY (constructs an
// owned `AllocCore`, calls alloc/dealloc/realloc/alloc_zeroed on it) — never
// installed as the global allocator. The allocator's segments are its own
// mmap'd regions, which ASan still shadows; an access that escapes a RELEASED
// mapping (os::release_segment → munmap on large-segment free) is flagged. See
// the test file header for the precise class of bug ASan can vs cannot see
// inside a self-hosted mmap arena (intra-segment OOB between two allocations
// within one 4 MiB segment is addressable shadow and NOT redzoned — that
// dimension is Miri's job, not ASan's).
//
// The three traps this encapsulates (identical to tsan.mjs):
//   1. RUSTC_WRAPPER / CARGO_BUILD_RUSTC_WRAPPER are inherited from the Windows
//      environment into WSL and point at `sccache.exe` — a Windows binary that
//      cannot drive the Linux rustc. We `unset` both inside the WSL shell AND
//      set them to empty strings on the cargo process.
//   2. ASan needs an instrumented std → `-Zbuild-std --target
//      x86_64-unknown-linux-gnu` → a from-scratch std build; dedicated
//      `/tmp/sefer-asan` target dir.
//   3. Both RUSTFLAGS and RUSTDOCFLAGS need `-Zsanitizer=address`.

import { REPO_ROOT, winToWsl, run, verdict } from './lib.mjs';

// The single ASan-targeted test file (see tests/asan_alloc_core.rs). It is the
// ONLY file this runner compiles under the sanitizer; the rest of the suite is
// unaffected.
const DEFAULT_TESTS = ['asan_alloc_core'];

const tests = process.argv.slice(2).length
  ? process.argv.slice(2)
  : DEFAULT_TESTS;

const wslRoot = winToWsl(REPO_ROOT);

// Two passes mirroring scripts/tsan.mjs's shape: the plain substrate, then the
// decommit path (which adds the madvise/MADV_DONTNEED + recommit surface — a
// different mmap-touching leg worth instrumenting). Both run the same ASan
// target file; both are fast (the test is bounded per the short-scenario
// policy). Each pass is [features, testList].
const passes = [
  ['alloc-core', tests],
  ['alloc-core alloc-decommit', tests],
];

// --- Precondition check -----------------------------------------------------
// Fail fast with a clear "requires: <X>" message rather than a confusing cargo
// error or (worse) a false green. Probes WSL presence, the nightly toolchain,
// and the `rust-src` component that -Zbuild-std needs.
async function checkPreconditions() {
  // 1. WSL itself: a `wsl -e true` that exits 0 confirms the runtime is up.
  const wslProbe = await run('wsl', ['-e', 'true']);
  if (wslProbe.code !== 0) {
    console.error(
      '[asan] FAIL: WSL is not available or could not start (exit ' +
        `${wslProbe.code}). ASan (-Zsanitizer=address) is Linux/macOS only in ` +
        `the Rust toolchain; the Windows MSVC target cannot run it.\n` +
        '  requires: WSL (e.g. `wsl --install -d Ubuntu`) with a Linux nightly.',
    );
    return false;
  }
  // 2. nightly toolchain + rust-src inside WSL. One bash -lc line.
  const probe =
    'rustc +nightly --version >/dev/null 2>&1 || { echo NO_NIGHTLY; exit 0; }; ' +
    'rustup +nightly component list --installed 2>/dev/null | grep -q "^rust-src" ' +
    '|| echo NO_RUST_SRC';
  const probeRes = await run('wsl', ['bash', '-lc', probe]);
  const probeOut = probeRes.out.trim();
  if (probeOut.includes('NO_NIGHTLY')) {
    console.error(
      '[asan] FAIL: no `nightly` toolchain in WSL.\n' +
        '  requires: `rustup toolchain install nightly` inside WSL.',
    );
    return false;
  }
  if (probeOut.includes('NO_RUST_SRC')) {
    console.error(
      '[asan] FAIL: the `rust-src` component is missing from the WSL nightly ' +
        '(required for `-Zbuild-std`).\n' +
        '  requires: `rustup +nightly component add rust-src` inside WSL.',
    );
    return false;
  }
  return true;
}

function bashCmd(features, testList) {
  const testArgs = testList.map((t) => `--test ${t}`).join(' ');
  // One bash -lc line so the env scrubbing + cargo invocation share a shell.
  return [
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
      "RUSTFLAGS='-Zsanitizer=address'",
      "RUSTDOCFLAGS='-Zsanitizer=address'",
      'CARGO_TARGET_DIR=/tmp/sefer-asan',
      // halt_on_error=0 keeps ASan reporting every distinct error in a run
      // (default aborts after the first); detect_leaks=0 suppresses LSan's
      // exit-time leak scan, which can noisily flag the allocator's own
      // still-live mmap'd segments if a test exits early — ASan's value here is
      // OOB/UAF detection, not leak detection (mmap isn't tracked by LSan
      // anyway, so this is belt-and-braces).
      "ASAN_OPTIONS=halt_on_error=0:detect_leaks=0",
      'cargo +nightly test -Zbuild-std --target x86_64-unknown-linux-gnu',
      '--release',
      `--features '${features}'`,
      testArgs,
    ].join(' '),
  ].join(' && ');
}

// ASan errors abort the process with a non-zero exit, but we ALSO scan the
// output explicitly for its markers (mirrors tsan.mjs scanning for ThreadSanitizer)
// so a non-fatal ASan warning still fails the sweep rather than looking green.
const ASAN_MARKERS = [
  'AddressSanitizer',
  'heap-buffer-overflow',
  'heap-use-after-free',
  'stack-buffer-overflow',
  'stack-use-after-return',
  'global-buffer-overflow',
  'double-free',
  'SEGV on unknown address',
];

console.log(`[asan] wsl: ${wslRoot}\n`);

const ok = await checkPreconditions();
if (!ok) process.exit(2);

let allOk = true;
for (const [features, testList] of passes) {
  console.log(`[asan] features: ${features} | tests: ${testList.join(', ')}`);
  const { code, out } = await run('wsl', [
    'bash',
    '-lc',
    bashCmd(features, testList),
  ]);
  const ok = verdict(`asan:${features}`, code, out, ASAN_MARKERS);
  allOk = ok && allOk;
}
console.log(`\n[asan] overall: ${allOk ? 'PASS' : 'FAIL'}`);
process.exit(allOk ? 0 : 1);
