// Output sniffer for STALE CROSS-CHECKOUT CARGO ARTIFACTS (task #1073).
//
// The trap this recognizes: this machine sets a user-global CARGO_TARGET_DIR
// (D:\dev\rust\.cargo-target, HKCU user env var), so EVERY git worktree of
// this repo shares ONE cargo artifact cache. Tests read repo files through
// `Path::new(env!("CARGO_MANIFEST_DIR"))`, and rustc bakes that path INTO the
// test binary at compile time — so a binary compiled in worktree A can be
// silently replayed by cargo in worktree B. When worktree A is later deleted,
// every such file-reading test panics with exactly
//   read <repo-file>: Os { code: 3, kind: NotFound, message: "..." }
// a red that looks like a repo bug but is an artifact-reuse bug. This
// literally happened: `npm run check` failed at `test (--features pinning)`
// with `thread 'ci_clippy_steps_match_manifest_clippy_rows_exactly' panicked
// at tests/ci_clippy_matrix_consistency.rs:239:10: read
// scripts/check-matrix.mjs: Os { code: 3, kind: NotFound ... }` — a FALSE RED.
//
// What this module does: recognize that panic signature in captured step
// output, so a red gate step can print the real cause next to its own
// failure. scripts/check-all.mjs imports `staleArtifactDiagnosis` for its
// failure path; the gate also runs `node scripts/stale-artifact-diagnosis.mjs
// --self-test` as a step so the detector cannot silently rot — the same
// wiring rationale as scripts/argv-roundtrip-test.mjs (a detector that has
// never fired is not proven).
//
// The detector is deliberately narrow: it requires BOTH a Rust test panic
// location line (`panicked at <...>.rs:line:col`) AND an io::Error
// `kind: NotFound`. And it is ADVISORY ONLY — its output is printed IN
// ADDITION to the step's own failure, never instead of it — so a same-shaped
// but genuinely unrelated NotFound panic costs only a wrong guess at
// explanation, never a masked failure.
//
// Run the self-test directly:
//   node scripts/stale-artifact-diagnosis.mjs --self-test
//
// Exits 0 on pass, 1 on failure. No npm deps (matches scripts/'s own
// "there is no npm dependency graph" rule — see lib.mjs's header).

/**
 * Detect the stale cross-checkout cargo artifact signature in captured step
 * output. Returns a human-readable diagnosis string when BOTH markers of the
 * signature are present (a Rust test panicking at a `.rs:line:col` location
 * AND an io::Error `kind: NotFound`), and `null` otherwise. Advisory only —
 * callers print the result next to (never instead of) the step's own failure.
 */
export function staleArtifactDiagnosis(out) {
  const text = out ?? '';
  // Both markers are required on purpose (see the header): a panic alone is
  // an ordinary test failure, a NotFound alone is an ordinary missing-file
  // error from some non-test tool; only the pair is the artifact-replay shape.
  const panickedAtTestSource = /panicked at [^\n]*\.rs:\d+:\d+/m.test(text);
  const ioNotFound = /kind: NotFound\b/.test(text);
  if (!panickedAtTestSource || !ioNotFound) return null;
  return (
    'the step output shows a Rust test panicking with an io::Error NotFound while ' +
    'reading a path — the signature of a STALE CROSS-CHECKOUT CARGO ARTIFACT replayed ' +
    'from the shared CARGO_TARGET_DIR: this machine points CARGO_TARGET_DIR at ONE ' +
    'directory for every git worktree of this repo, so a test binary compiled in a ' +
    'DIFFERENT worktree can be replayed here, and tests that read repo files through ' +
    'env!("CARGO_MANIFEST_DIR") (a path rustc bakes into the binary at compile time) ' +
    'fail with exactly this signature once that checkout is deleted — every such read ' +
    "targets the deleted worktree's tree. Remediation: rebuild from THIS checkout — " +
    '`touch` the panicking test source named in the `panicked at ...` line above (or ' +
    '`cargo clean -p sefer-alloc`) — then re-run the gate. ' +
    'tests/no_stale_cross_checkout_artifacts.rs fails with a self-explanatory message ' +
    'in this same condition. Task #1073 (see also task #1071, the aligned-vmem ' +
    'cache-replay guards this complements). This diagnosis is advisory: it is printed ' +
    'in addition to the failure above, never instead of it — if the panic above is an ' +
    'unrelated io::Error NotFound (same shape, genuinely local cause), this guess is ' +
    'wrong but costs nothing.'
  );
}

// --- self-test (CLI) -------------------------------------------------------
// Mirrors the argv-roundtrip-test.mjs pattern: run directly as its own
// invocation, exit 0/1. Fixtures 2-4 prove the detector stays narrow.
if (process.argv[2] === '--self-test') {
  // FIXTURE 1 (must DETECT) — the literal observed failure from task #1073.
  const FIXTURE_1 =
    'running 1 test\n' +
    'thread \'ci_clippy_steps_match_manifest_clippy_rows_exactly\' panicked at ' +
    'tests/ci_clippy_matrix_consistency.rs:239:10:\n' +
    'read scripts/check-matrix.mjs: Os { code: 3, kind: NotFound, message: ' +
    '"The system cannot find the path specified." }\n' +
    'note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n' +
    'test ci_clippy_steps_match_manifest_clippy_rows_exactly ... FAILED';
  // FIXTURE 2 (must NOT detect) — a healthy run.
  const FIXTURE_2 =
    'running 1 test\n' +
    'test ci_clippy_steps_match_manifest_clippy_rows_exactly ... ok\n' +
    'test result: ok. 1 passed; 0 failed;';
  // FIXTURE 3 (must NOT detect) — an ordinary assertion panic: has the
  // `panicked at ...rs:line:col` line but NO `kind: NotFound`.
  const FIXTURE_3 =
    'running 1 test\n' +
    'thread \'checks_rows\' panicked at tests/foo.rs:5:5:\n' +
    'assertion `left == right` failed\n' +
    '  left: 1\n' +
    ' right: 2\n' +
    'test checks_rows ... FAILED';
  // FIXTURE 4 (must NOT detect) — a NotFound io::Error printed by a non-test
  // tool: has `kind: NotFound` but NO `panicked at ...rs:line:col` line.
  const FIXTURE_4 =
    'Error: Os { code: 2, kind: NotFound, message: "The system cannot find the file specified." }';

  const cases = [
    { name: 'FIXTURE 1: literal task #1073 false-red panic (MUST detect)', out: FIXTURE_1, mustDetect: true },
    { name: 'FIXTURE 2: healthy run (must NOT detect)', out: FIXTURE_2, mustDetect: false },
    { name: 'FIXTURE 3: ordinary assertion panic, no NotFound (must NOT detect)', out: FIXTURE_3, mustDetect: false },
    { name: 'FIXTURE 4: non-test tool NotFound, no panic location (must NOT detect)', out: FIXTURE_4, mustDetect: false },
  ];
  let failures = 0;
  for (const c of cases) {
    const detected = staleArtifactDiagnosis(c.out) !== null;
    if (detected === c.mustDetect) {
      console.log(`PASS: ${c.name}`);
    } else {
      failures++;
      console.log(
        `FAIL: ${c.name} (wanted ${c.mustDetect ? 'detection' : 'no detection'}, ` +
          `got ${detected ? 'detection' : 'none'})`,
      );
    }
  }
  if (failures) {
    console.error(
      `\n[stale-artifact-diagnosis] self-test FAILED (${cases.length - failures}/${cases.length})`,
    );
    process.exit(1);
  }
  console.log(`\n[stale-artifact-diagnosis] self-test OK (${cases.length}/${cases.length})`);
  process.exit(0);
}
