// Sol-F1 (task #563, release-readiness review finding F1) — the compile-fail
// oracle for the NEGATIVE half of the `internals` boundary.
//
// ## Background
//
// `tests/r34_3_internals_boundary_api.rs` (R34-3/task #522) already proves
// the POSITIVE half: the stable crate-root re-exports (`SeferAlloc`,
// `AllocCore`, `SegmentLayout`, etc.) resolve WITHOUT `internals`. Its own
// module doc says explicitly that it does NOT (and cannot cheaply) prove the
// NEGATIVE half — that `AllocCore`'s `dbg_*` diagnostic hooks do NOT resolve
// without `internals` — because a normal `#[test]` fn cannot assert "this
// file fails to compile" (a compile failure fails the whole test binary, not
// one test function).
//
// Before Sol-F1, that negative half was actually FALSE: `AllocCore` is
// re-exported at the crate root unconditionally (`pub use
// alloc_core::{AllocCore, SegmentLayout}` in `src/lib.rs`, gated only on
// `alloc-core`), so R34-3's module-path gate on `alloc_core`/`global`/
// `registry` did not hide `AllocCore`'s own INHERENT methods — Rust's
// module-privacy rules only affect how a TYPE is NAMED/reached, not the
// visibility of already-`pub` inherent methods on a type reachable another
// way. `sefer_alloc::AllocCore::dbg_carve_batch` compiled and ran with ZERO
// `internals` anywhere on the command line.
//
// Sol-F1 (task #563) fixed this by gating the `dbg_*`-only `impl AllocCore`
// blocks in `alloc_core_core_diag.rs`/`alloc_core_small_diag.rs`/
// `alloc_core_small_reclaim.rs` directly with `#[cfg(feature = "internals")]`
// (see those files' own module docs). This script is the automated,
// reproducible oracle that PROVES the fix, both directions:
//
//   1. `cargo build --example sol_f1_dbg_carve_batch_negative_probe
//      --features "alloc-core alloc-global alloc-decommit"` (NO `internals`)
//      MUST FAIL — `AllocCore::dbg_carve_batch` must not resolve.
//   2. The SAME command PLUS `internals` MUST SUCCEED — proving the failure
//      above is caused specifically by the `internals` gate, not some
//      unrelated breakage in the probe file (a probe that always fails
//      would trivially "pass" step 1 for the wrong reason).
//
// `examples/sol_f1_dbg_carve_batch_negative_probe.rs` is the probe file
// itself; see its own module doc for the full design. `AllocCore::
// dbg_carve_batch` is used as the representative method: a plain safe
// `pub fn` with no OTHER feature gate beyond the file's `internals` gate, so
// a failure is unambiguously about the `internals` boundary.
//
// Usage:
//   node scripts/verify-internals-negative-boundary.mjs
//   npm run check   (wired in as a step, alongside the existing
//                     `test-internals-boundary-no-internals` positive-half
//                     check from scripts/check-matrix.mjs)

import { REPO_ROOT, run } from './lib.mjs';

const EXAMPLE = 'sol_f1_dbg_carve_batch_negative_probe';
const BASE_FEATURES = 'alloc-core alloc-global alloc-decommit';

console.log(`[verify-internals-negative-boundary] repo: ${REPO_ROOT}`);

let allOk = true;

// Step 1: WITHOUT `internals` — must FAIL to compile.
console.log(`\n============================================================`);
console.log(`  [1/2] cargo build --example ${EXAMPLE} --features "${BASE_FEATURES}"`);
console.log(`        (NO internals — MUST FAIL: dbg_carve_batch must not resolve)`);
console.log(`============================================================`);
const without = await run(
  'cargo',
  ['build', '--example', EXAMPLE, '--features', BASE_FEATURES],
  { cwd: REPO_ROOT },
);
const sawExpectedError = /dbg_carve_batch.*not found|no method named `dbg_carve_batch`/s.test(
  without.out,
);
if (without.code === 0) {
  console.log(
    `\n[verify-internals-negative-boundary] FAIL: build SUCCEEDED without ` +
      `\`internals\` — AllocCore::dbg_carve_batch is reachable without ` +
      `internals (the exact Sol-F1/F1 regression). Exit code was 0.`,
  );
  allOk = false;
} else if (!sawExpectedError) {
  console.log(
    `\n[verify-internals-negative-boundary] FAIL: build failed (exit ` +
      `${without.code}) as expected, but NOT with the expected ` +
      `"dbg_carve_batch ... not found" E0599 — some OTHER breakage is ` +
      `present. This oracle only counts a failure as a PASS if it is ` +
      `caused specifically by the internals gate.`,
  );
  allOk = false;
} else {
  console.log(
    `\n[verify-internals-negative-boundary] OK: build failed as expected ` +
      `(exit ${without.code}), with the expected dbg_carve_batch E0599.`,
  );
}

// Step 2: WITH `internals` — must SUCCEED (proves step 1's failure is
// specifically about the internals gate, not a broken probe file).
console.log(`\n============================================================`);
console.log(`  [2/2] cargo build --example ${EXAMPLE} --features "${BASE_FEATURES} internals"`);
console.log(`        (WITH internals — MUST SUCCEED)`);
console.log(`============================================================`);
const withInternals = await run(
  'cargo',
  ['build', '--example', EXAMPLE, '--features', `${BASE_FEATURES} internals`],
  { cwd: REPO_ROOT },
);
if (withInternals.code !== 0) {
  console.log(
    `\n[verify-internals-negative-boundary] FAIL: build FAILED with ` +
      `\`internals\` on (exit ${withInternals.code}) — expected success. ` +
      `Either the probe file itself is broken, or Sol-F1's fix is ` +
      `incomplete (dbg_carve_batch should be reachable with internals).`,
  );
  allOk = false;
} else {
  console.log(
    `\n[verify-internals-negative-boundary] OK: build succeeded with ` +
      `internals, as expected.`,
  );
}

console.log(`\n============================================================`);
console.log(
  allOk
    ? '[verify-internals-negative-boundary] ALL GREEN'
    : '[verify-internals-negative-boundary] FAILED',
);
console.log(`============================================================`);
process.exit(allOk ? 0 : 1);
