// cargo-fuzz runner via WSL (Linux). libFuzzer requires the nightly toolchain
// and does NOT run on Windows — the fuzz crate (fuzz/) is its own workspace
// root precisely so the parent `cargo build` / `cargo test` are unaffected.
//
// This is the companion to scripts/asan.mjs in the Phase-5 / nightly hardening
// tier. It is NOT part of `npm run check` (the fast everyday gate); fuzzing is
// CPU-hour-heavy and lives on a scheduled/manual cadence, not per-PR.
//
// Usage (from repo root):
//   node scripts/fuzz.mjs                      # BUILD all targets (fast smoke
//                                              # check — what CI's fuzz-build
//                                              # job does; proves no bit-rot)
//   node scripts/fuzz.mjs global_alloc_ops     # RUN one target for a bounded
//                                              # time (default 30s, override
//                                              # with SEFER_FUZZ_SECONDS=N)
//   node scripts/fuzz.mjs global_alloc_ops 120 # RUN one target for 120s
//   npm run fuzz
//
// Requires: WSL with a nightly toolchain + `cargo-fuzz` installed
// (`cargo +nightly install cargo-fuzz`).
//
// The two WSL interop traps this encapsulates (analogous to the RUSTC_WRAPPER
// trap in scripts/tsan.mjs / asan.mjs):
//   1. RUSTC_WRAPPER / CARGO_BUILD_RUSTC_WRAPPER leak from the Windows env and
//      point at `sccache.exe`, which cannot drive the Linux rustc.
//   2. CARGO_TARGET_DIR leaks from the Windows env as a Windows path
//      (`D:\...`); cargo-fuzz joins it into the fuzz binary's LD_LIBRARY_PATH,
//      producing a corrupted path and `failed to join paths` (the same
//      Windows-env-leakage class of trap). We override it to a dedicated Linux
//      target dir (`/tmp/sefer-fuzz`) so the ASan-instrumented fuzz objects
//      never collide with the Windows target/ either.

import { REPO_ROOT, winToWsl, run, verdict } from './lib.mjs';

const FUZZ_DIR = `${winToWsl(REPO_ROOT)}/fuzz`;

// The registered fuzz targets in fuzz/Cargo.toml. Used to validate a requested
// target name so a typo doesn't silently fall through to "no such target".
const KNOWN_TARGETS = new Set([
  'region_ops',
  'global_alloc_ops',
  'heap_core_ops',
]);

const args = process.argv.slice(2);

// --- Precondition check -----------------------------------------------------
// Fail fast with a clear "requires: <X>" message rather than a confusing error
// or a false green.
async function checkPreconditions() {
  const wslProbe = await run('wsl', ['-e', 'true']);
  if (wslProbe.code !== 0) {
    console.error(
      '[fuzz] FAIL: WSL is not available or could not start (exit ' +
        `${wslProbe.code}). libFuzzer requires the nightly toolchain on Linux ` +
        `and does NOT run on Windows.\n` +
        '  requires: WSL with a Linux nightly + `cargo +nightly install ' +
        'cargo-fuzz`.',
    );
    return false;
  }
  // nightly toolchain + cargo-fuzz binary, in one probe line.
  const probe =
    'rustc +nightly --version >/dev/null 2>&1 || echo NO_NIGHTLY; ' +
    'command -v cargo-fuzz >/dev/null 2>&1 || echo NO_CARGO_FUZZ';
  const probeRes = await run('wsl', ['bash', '-lc', probe]);
  const probeOut = probeRes.out.trim();
  if (probeOut.includes('NO_NIGHTLY')) {
    console.error(
      '[fuzz] FAIL: no `nightly` toolchain in WSL.\n' +
        '  requires: `rustup toolchain install nightly` inside WSL.',
    );
    return false;
  }
  if (probeOut.includes('NO_CARGO_FUZZ')) {
    console.error(
      '[fuzz] FAIL: `cargo-fuzz` is not installed in WSL.\n' +
        '  requires: `cargo +nightly install cargo-fuzz` inside WSL.',
    );
    return false;
  }
  return true;
}

// Build the shared env-scrub prefix for a WSL bash invocation. Returns the
// leading `unset ... VAR= ...` fragment (no trailing &&).
function envScrub() {
  return [
    'unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER',
    // `unset` alone is not enough: WSL interop re-injects the Windows values.
    // Setting them to empty (RUSTC_WRAPPER) / a valid Linux path
    // (CARGO_TARGET_DIR — cargo rejects an empty string) on the cargo process
    // is what actually disables/overrides them.
    'RUSTC_WRAPPER=',
    'CARGO_BUILD_RUSTC_WRAPPER=',
    'CARGO_TARGET_DIR=/tmp/sefer-fuzz',
  ].join(' ');
}

// No positional arg → build all targets (fast, mirrors CI's fuzz-build job).
if (args.length === 0) {
  console.log(`[fuzz] wsl: ${FUZZ_DIR}\n`);
  const ok = await checkPreconditions();
  if (!ok) process.exit(2);
  console.log('[fuzz] building all targets (no run) ...');
  const cmd = `cd ${FUZZ_DIR} && ${envScrub()} cargo +nightly fuzz build --target x86_64-unknown-linux-gnu`;
  const { code, out } = await run('wsl', ['bash', '-lc', cmd]);
  const ok2 = verdict('fuzz-build', code, out);
  console.log(`\n[fuzz] overall: ${ok2 ? 'PASS' : 'FAIL'}`);
  process.exit(ok2 ? 0 : 1);
}

// A target name was given → run that one for a bounded time.
const target = args[0];
if (!KNOWN_TARGETS.has(target)) {
  console.error(
    `[fuzz] unknown target "${target}" — not in fuzz/Cargo.toml.\n` +
      `  known targets: ${[...KNOWN_TARGETS].join(', ')}`,
  );
  process.exit(2);
}
const seconds = args[1] ? Number(args[1]) : (Number(process.env.SEFER_FUZZ_SECONDS) || 30);
if (!Number.isFinite(seconds) || seconds <= 0) {
  console.error(`[fuzz] invalid time budget: "${args[1]}" (use a positive number of seconds)`);
  process.exit(2);
}

console.log(`[fuzz] wsl: ${FUZZ_DIR}\n`);
const ok = await checkPreconditions();
if (!ok) process.exit(2);

console.log(`[fuzz] running "${target}" for ${seconds}s ...`);
const cmd =
  `cd ${FUZZ_DIR} && ${envScrub()} cargo +nightly fuzz run ${target} ` +
  `--target x86_64-unknown-linux-gnu -- -max_total_time=${seconds}`;
const { code, out } = await run('wsl', ['bash', '-lc', cmd]);
// libFuzzer reports a found bug as a non-zero exit with "ERROR: libFuzzer" /
// "deadly signal" / "SUMMARY: libFuzzer". A clean run ends with "Done ... runs"
// and exit 0. The verdict helper's marker scan turns any libFuzzer error into a
// FAIL; a clean bounded run is a PASS.
const foundBug = /ERROR: libFuzzer|deadly signal|SUMMARY: libFuzzer/.test(out);
const cleanRun = /Done \d+ runs?/.test(out) && code === 0;
if (foundBug) {
  console.log('\n[fuzz-run] FAIL (libFuzzer found a bug — see crash artifact above)');
  process.exit(1);
} else if (cleanRun) {
  console.log(`\n[fuzz-run:${target}] PASS (clean bounded run)`);
  process.exit(0);
} else {
  console.log(`\n[fuzz-run:${target}] FAIL (exit ${code}, no clean-run marker)`);
  process.exit(1);
}
