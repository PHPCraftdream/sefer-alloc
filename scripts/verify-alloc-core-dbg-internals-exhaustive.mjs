// H2 (task #572, Sol-remediation review finding H2) — EXHAUSTIVE structural
// check that every `AllocCore::dbg_*` inherent method requires `internals`.
//
// ## Background
//
// Sol-F1 (task #563) gated the `dbg_*`-only `impl AllocCore` blocks behind
// `#[cfg(feature = "internals")]` in exactly 3 files
// (`alloc_core_core_diag.rs`/`alloc_core_small_diag.rs`/
// `alloc_core_small_reclaim.rs`), and proved the fix with a compile-fail
// oracle (`scripts/verify-internals-negative-boundary.mjs`) that tests
// exactly ONE representative method (`dbg_carve_batch`). That single-method
// oracle passed — but 31 OTHER `dbg_*` methods across 3 DIFFERENT files
// (`alloc_core_large_cache.rs`, `alloc_core_small_pool.rs`,
// `alloc_core.rs`'s numa-only methods) remained fully reachable without
// `internals`, undetected, because nothing checked the REST of the surface.
// This is finding H2 (P1) of
// `docs/reviews/2026-08-05-sol-remediation-readonly-review.md`.
//
// This script closes that class of gap structurally: it does not test one
// method, it enumerates EVERY `pub fn dbg_*` / `pub unsafe fn dbg_*` method
// across every `src/alloc_core/*.rs` file with an `impl AllocCore` block,
// and asserts each one is gated behind `internals` OR is explicitly
// allowlisted below with a documented reason. Run it whenever a new `dbg_*`
// method is added to `AllocCore` — a method that is neither gated nor
// allowlisted fails the build.
//
// Usage:
//   node scripts/verify-alloc-core-dbg-internals-exhaustive.mjs
//   npm run check   (wired in as a step, alongside the existing
//                     verify-internals-negative-boundary.mjs)

import { readFileSync, readdirSync } from 'node:fs';
import { join, relative } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const ALLOC_CORE_DIR = join(REPO_ROOT, 'src', 'alloc_core');
const TESTS_DIR = join(REPO_ROOT, 'tests');

/** Recursively list every `.rs` file under `dir`, returning paths relative
 * to `dir` (POSIX-separated, so the ALLOWLIST's `file.rs` keys keep working
 * for the historically-flat `src/alloc_core/` layout while also covering any
 * future subdirectory, e.g. `src/alloc_core/deferred_large/`). H2-followup
 * (finding F10): the original version used a non-recursive `readdirSync`,
 * silently invisible to a future `impl AllocCore` block placed in a
 * subdirectory — verified currently harmless (no `pub fn dbg_*` exists under
 * any `src/alloc_core/` subdirectory today), fixed pre-emptively. */
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

// Methods deliberately NOT gated behind `internals`, each with a recorded
// reason. Every entry here must still be justified the same way task #563
// justified its own three exceptions (a real, non-test/bench/example
// caller). Do not add an entry here to silence this script without first
// confirming the reason via `grep -rn "\.<method>(" src/` yourself.
const ALLOWLIST = new Map([
  // Sol-F1 (task #563): back `AllocStats::stats()`, a stable always-on
  // public API method under plain `production` (src/global/sefer_alloc.rs).
  ['alloc_core_core_diag.rs::dbg_foreign_or_unroutable_frees', 'backs AllocStats::stats() (task #563)'],
  ['alloc_core_core_diag.rs::dbg_segments_reserved_total', 'backs AllocStats::stats() (task #563)'],
  ['alloc_core_core_diag.rs::dbg_segments_released_total', 'backs AllocStats::stats() (task #563)'],
  // H2 (task #572): a 4th sibling of the same class, found by this script's
  // own first exhaustive run against `alloc_core_small_pool.rs` —
  // `SeferAlloc::stats()`'s `decommit_calls` field
  // (src/global/sefer_alloc.rs) reads this directly.
  ['alloc_core_small_pool.rs::dbg_decommit_count', 'backs AllocStats::stats() (task #572)'],
]);

/** Walk backward from line index `i` (exclusive) through the contiguous
 * attribute/doc-comment block immediately above it, returning true if any
 * REAL `#[cfg(...)]` ATTRIBUTE line (not a doc comment or `//` comment merely
 * mentioning the string) in that block names `feature = "internals"`. Stops
 * at the first line that is neither an attribute, a doc comment, a blank
 * line, nor a `//`-comment continuation line — doc/`//` lines are walked
 * through (so a `///` line between two `#[cfg(...)]` attributes doesn't
 * truncate the walk early) but never themselves count as a gate.
 *
 * H2-followup (task #572's own review-remediation round, finding F10 of
 * `docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`): the
 * original version of this function accepted ANY line containing the
 * literal `feature = "internals"` — including a `///` doc comment merely
 * DESCRIBING the gate rather than an actual `#[cfg(...)]` attribute
 * applying it. Verified harmless in practice (re-running the ORIGINAL
 * looser check vs. this tightened one produces byte-identical
 * gated/violation classifications across every method as of this fix), but
 * a latent false-pass a future doc comment could trigger. Tightened so only
 * lines literally starting with `#[` can set `gated = true`. */
function precedingBlockIsInternalsGated(lines, i) {
  let gated = false;
  let j = i - 1;
  while (j >= 0) {
    const trimmed = lines[j].trim();
    if (trimmed === '') {
      j--;
      continue;
    }
    if (trimmed.startsWith('#[')) {
      if (trimmed.includes('feature = "internals"')) gated = true;
      j--;
      continue;
    }
    if (trimmed.startsWith('///') || trimmed.startsWith('//!') || trimmed.startsWith('//')) {
      // Walk through comment lines without ever treating their CONTENT as a
      // gate — a doc comment can mention `feature = "internals"` in prose
      // without that being a real attribute.
      j--;
      continue;
    }
    break;
  }
  return gated;
}

/** Extract every `impl AllocCore { ... }` block's `pub (unsafe )?fn dbg_*`
 * methods, each annotated with whether an `internals` cfg gate covers it —
 * EITHER on the method's own immediately-preceding attribute block, OR on
 * the enclosing `impl AllocCore` block itself (task #563 gated whole impl
 * blocks in some files, individual methods in others — both are valid
 * patterns and must both be recognised). Methods inside `impl` blocks for
 * OTHER types (e.g. `RemoteFreeRing`, `ReservedSmallSegment`) are correctly
 * ignored — H2's finding is specifically about `AllocCore::dbg_*`; other
 * types' `internals` boundaries are a separate, not-yet-scoped concern. */
function scanFile(filename) {
  const path = join(ALLOC_CORE_DIR, filename);
  const lines = readFileSync(path, 'utf8').split('\n');
  const findings = [];

  // Stack of {name, gated, depthAtEntry} for nested-brace tracking — Rust
  // doesn't nest `impl` blocks, but this still correctly handles `impl`
  // blocks separated by other top-level items at brace depth 0.
  let currentImplName = null;
  let currentImplGated = false;
  let implBraceDepthAtEntry = null;
  let braceDepth = 0;

  for (let i = 0; i < lines.length; i++) {
    const implMatch = lines[i].match(/^\s*impl(?:<[^>]*>)?\s+([A-Za-z_]\w*)\b/);
    if (implMatch) {
      currentImplName = implMatch[1];
      currentImplGated = precedingBlockIsInternalsGated(lines, i);
      implBraceDepthAtEntry = braceDepth;
    }

    for (const ch of lines[i]) {
      if (ch === '{') braceDepth++;
      else if (ch === '}') braceDepth--;
    }
    // Exited the current impl block once brace depth returns to (or below)
    // what it was when the `impl` line itself was seen.
    if (implBraceDepthAtEntry !== null && braceDepth <= implBraceDepthAtEntry && lines[i].includes('}')) {
      currentImplName = null;
      currentImplGated = false;
      implBraceDepthAtEntry = null;
    }

    if (currentImplName !== 'AllocCore') continue;

    const m = lines[i].match(/^\s*pub (unsafe )?fn (dbg_\w+)/);
    if (!m) continue;
    const methodName = m[2];
    const methodOwnGated = precedingBlockIsInternalsGated(lines, i);
    const gated = methodOwnGated || currentImplGated;

    findings.push({ file: filename, method: methodName, gated, line: i + 1 });
  }

  return findings;
}

console.log(`[verify-alloc-core-dbg-internals-exhaustive] repo: ${REPO_ROOT}`);
console.log(`[verify-alloc-core-dbg-internals-exhaustive] scanning ${ALLOC_CORE_DIR}...\n`);

const files = listRsFilesRecursive(ALLOC_CORE_DIR);
let totalMethods = 0;
let totalGated = 0;
let totalAllowlisted = 0;
const violations = [];

for (const file of files) {
  const findings = scanFile(file);
  for (const f of findings) {
    totalMethods++;
    const key = `${f.file}::${f.method}`;
    if (f.gated) {
      totalGated++;
    } else if (ALLOWLIST.has(key)) {
      totalAllowlisted++;
      console.log(`  [allowlisted] ${key}:${f.line} — ${ALLOWLIST.get(key)}`);
    } else {
      violations.push(f);
    }
  }
}

console.log(
  `\n[verify-alloc-core-dbg-internals-exhaustive] scanned ${files.length} file(s), ` +
    `found ${totalMethods} AllocCore::dbg_* method(s): ${totalGated} gated behind ` +
    `internals, ${totalAllowlisted} explicitly allowlisted, ${violations.length} VIOLATION(s).`,
);

let ok = true;

if (violations.length > 0) {
  ok = false;
  console.log(
    `\n[verify-alloc-core-dbg-internals-exhaustive] FAIL (check 1/2) — the ` +
      `following AllocCore::dbg_* methods are reachable WITHOUT \`internals\` ` +
      `and are NOT in the ALLOWLIST above (Sol-F1/H2 regression class):`,
  );
  for (const v of violations) {
    console.log(`  ${v.file}:${v.line} — ${v.method}`);
  }
  console.log(
    `\nEach one must either be gated with #[cfg(feature = "internals")] ` +
      `(possibly combined with its existing cfg via a second #[cfg(...)] ` +
      `attribute or all(...)), or added to this script's own ALLOWLIST with ` +
      `a documented, verified reason (a real caller outside tests/benches/examples).`,
  );
}

// ---------------------------------------------------------------------------
// Check 2/2 (H2-followup, task #572's own review-remediation round, finding
// F4 of `docs/reviews/2026-08-05-wave3-h1h8-remediation-readonly-review.md`):
// gating an `AllocCore::dbg_*` method behind `internals` is only half the
// invariant R34-3 (`b47cc6a`) established — every `tests/*.rs` file that
// CALLS a gated method must ALSO carry `feature = "internals"` in its own
// crate-level `#![cfg(...)]`, so a no-`internals` build skips the file
// (cfg'd out) instead of hard-failing to compile (E0599). Sol-F1 and H2
// gated 124 methods across 6 files without re-running that sweep; 39 test
// files were found to violate it (compiler-confirmed via `cargo test --no-run
// --tests --features production`), including 2 newly broken by H2's own
// `alloc_core_small_pool.rs` gating. Fixed in the same commit that added
// this check — see that commit's own message for the file list.
const gatedMethodNames = new Set();
for (const file of files) {
  for (const f of scanFile(file)) {
    if (f.gated || ALLOWLIST.has(`${f.file}::${f.method}`)) gatedMethodNames.add(f.method);
  }
}

const testFiles = readdirSync(TESTS_DIR).filter((f) => f.endsWith('.rs'));
const testViolations = [];
for (const f of testFiles) {
  const text = readFileSync(join(TESTS_DIR, f), 'utf8');
  const cfgMatch = text.match(/#!\[cfg\(([\s\S]*?)\)\]/);
  const hasInternals = cfgMatch && cfgMatch[1].includes('feature = "internals"');
  if (hasInternals) continue;

  const calledGated = new Set();
  for (const line of text.split('\n')) {
    const trimmed = line.trim();
    if (trimmed.startsWith('//')) continue;
    for (const m of line.matchAll(/[.:](dbg_\w+)\s*\(/g)) {
      if (gatedMethodNames.has(m[1])) calledGated.add(m[1]);
    }
  }
  if (calledGated.size > 0) testViolations.push({ file: f, methods: [...calledGated].sort() });
}

console.log(
  `\n[verify-alloc-core-dbg-internals-exhaustive] check 2/2: scanned ` +
    `${testFiles.length} tests/*.rs file(s), ${testViolations.length} VIOLATION(s) ` +
    `(call a gated method without \`internals\` in their own #![cfg]).`,
);

if (testViolations.length > 0) {
  ok = false;
  console.log(`\n[verify-alloc-core-dbg-internals-exhaustive] FAIL (check 2/2):`);
  for (const v of testViolations) {
    console.log(`  tests/${v.file}: ${v.methods.join(', ')}`);
  }
  console.log(
    `\nEach file above must add \`feature = "internals"\` to its own crate-level ` +
      `#![cfg(...)] so a no-\`internals\` build skips it (cfg'd out) instead of ` +
      `hard-failing to compile.`,
  );
}

if (!ok) process.exit(1);

console.log(`\n[verify-alloc-core-dbg-internals-exhaustive] ALL GREEN`);
process.exit(0);
