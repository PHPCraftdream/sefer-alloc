// task #1067 (F7) — EXHAUSTIVE completeness check that every public
// `bench-internals` counter accessor in crates/aligned-vmem is referenced by
// the accessor-surface test.
//
// ## Background
//
// `crates/aligned-vmem/tests/bench_internals_counters.rs` imports every
// counter accessor by name to prove at compile time that each name resolves.
// That import is an EXISTENCE check, not a COMPLETENESS check: when R7-3
// added `windows_large_page_plain_fallback_successes`
// (src/bench_internals/windows.rs), nothing forced the test's hand-written
// list to gain the new name, and the test's own comment claim ("every
// accessor name must appear exactly once, creating a compile-time check
// that the accessor surface matches the counter surface") silently became
// false — the guard against accessor drift had itself drifted, undetected
// across every subsequent gate run (task #1067/F7 found it by hand-count:
// 15 accessors declared, 14 referenced).
//
// This script closes that class of gap structurally, in the same shape as
// `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs` (task #572/H2):
// it does not trust the hand-written list — it ENUMERATES every accessor in
// the source and requires each one to be CALLED by the test. The
// complementary direction (a name referenced by the test but deleted from
// the source) is already caught at compile time by the test's own `use`
// (E0432), so the two checks together close the loop in both directions.
//
// Accessor pattern: every counter accessor in `src/bench_internals/*.rs` is
// a free function of exactly this shape —
//     pub fn <name>() -> u64 {
// — snapshotting one (or a sum of) private `AtomicU64` static(s). The
// line-anchored regex below matches that shape and only it: `pub(crate)
// static` declarations, `reset_bench_internals_counters()` (returns `()`),
// doc comments, and `//`-comment mentions cannot match. If the accessor
// family ever grows a non-`u64` return or an argument-taking shape, this
// regex must be updated in the same commit — a silent under-match would
// recreate exactly the drift this script exists to catch.
//
// Matching rule for the test side: the accessor name must appear as a CALL
// (`name (`) on a line that is not a `//`-comment — a prose mention in a
// comment does not count (the test's own comments legitimately name
// accessors when explaining history). An accessor imported but never called
// would also fail clippy's unused-imports under the all-features row, so
// name-in-comment was never real coverage.
//
// Usage:
//   node scripts/verify-aligned-vmem-bench-internals-exhaustive.mjs
//   npm run check   (wired in as a step, alongside the other verify-* guards)

import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const SRC_DIR = join(REPO_ROOT, 'crates', 'aligned-vmem', 'src', 'bench_internals');
const TEST_FILE = join(
  REPO_ROOT,
  'crates',
  'aligned-vmem',
  'tests',
  'bench_internals_counters.rs',
);

/** Recursively list every `.rs` file under `dir`, returning paths relative
 * to `dir` (same shape as verify-alloc-core-dbg-internals-exhaustive.mjs's
 * listRsFilesRecursive — the directory is flat today, but a future
 * subdirectory must not become silently invisible to this check). */
function listRsFilesRecursive(dir, root = dir) {
  const out = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = join(dir, entry.name);
    if (entry.isDirectory()) {
      out.push(...listRsFilesRecursive(full, root));
    } else if (entry.isFile() && entry.name.endsWith('.rs')) {
      out.push(relative(root, full).split('\\').join('/'));
    }
  }
  return out;
}

const ACCESSOR_RE = /^\s*pub fn (\w+)\(\)\s*->\s*u64\s*\{/;

const files = listRsFilesRecursive(SRC_DIR);
const accessors = [];
for (const file of files) {
  const lines = readFileSync(join(SRC_DIR, file), 'utf8').split('\n');
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(ACCESSOR_RE);
    if (m) accessors.push({ file, name: m[1], line: i + 1 });
  }
}

console.log(`[verify-aligned-vmem-bench-internals-exhaustive] repo: ${REPO_ROOT}`);
console.log(`[verify-aligned-vmem-bench-internals-exhaustive] scanning ${SRC_DIR}...\n`);

// Non-comment lines of the test file only — see "Matching rule" in the header.
const testCodeLines = readFileSync(TEST_FILE, 'utf8')
  .split('\n')
  .filter((l) => !l.trim().startsWith('//'));
const testCode = testCodeLines.join('\n');

const missing = accessors.filter((a) => !new RegExp(`\\b${a.name}\\s*\\(`).test(testCode));

for (const a of accessors) {
  const status = missing.includes(a) ? 'MISSING' : 'ok';
  console.log(`  [${status}] ${a.file}:${a.line} — ${a.name}`);
}

console.log(
  `\n[verify-aligned-vmem-bench-internals-exhaustive] scanned ${files.length} file(s), ` +
    `found ${accessors.length} accessor(s), ${missing.length} MISSING from ` +
    `${relative(REPO_ROOT, TEST_FILE)}.`,
);

if (missing.length > 0) {
  console.log(
    `\n[verify-aligned-vmem-bench-internals-exhaustive] FAIL — the following accessor(s) ` +
      `are declared in src/bench_internals/ but never CALLED by the test ` +
      `(add each name to the test's import block AND its post-reset zero asserts):`,
  );
  for (const a of missing) {
    console.log(`  ${a.file}:${a.line} — ${a.name}`);
  }
  process.exit(1);
}

console.log(`\n[verify-aligned-vmem-bench-internals-exhaustive] ALL GREEN`);
process.exit(0);
