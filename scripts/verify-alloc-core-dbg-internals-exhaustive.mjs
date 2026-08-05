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
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const ALLOC_CORE_DIR = join(REPO_ROOT, 'src', 'alloc_core');

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
 * `#[cfg(...)]` in that block names `feature = "internals"`. Stops at the
 * first line that is neither an attribute, a doc comment, a blank line, nor
 * a `//`-comment continuation line. */
function precedingBlockIsInternalsGated(lines, i) {
  let gated = false;
  let j = i - 1;
  while (j >= 0) {
    const trimmed = lines[j].trim();
    if (trimmed === '') {
      j--;
      continue;
    }
    if (
      trimmed.startsWith('#[') ||
      trimmed.startsWith('///') ||
      trimmed.startsWith('//!') ||
      trimmed.startsWith('//')
    ) {
      if (trimmed.includes('feature = "internals"')) gated = true;
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

const files = readdirSync(ALLOC_CORE_DIR).filter((f) => f.endsWith('.rs'));
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

if (violations.length > 0) {
  console.log(
    `\n[verify-alloc-core-dbg-internals-exhaustive] FAIL — the following ` +
      `AllocCore::dbg_* methods are reachable WITHOUT \`internals\` and are ` +
      `NOT in the ALLOWLIST above (Sol-F1/H2 regression class):`,
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
  process.exit(1);
}

console.log(`\n[verify-alloc-core-dbg-internals-exhaustive] ALL GREEN`);
process.exit(0);
