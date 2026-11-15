#!/usr/bin/env node
// p12-visibility-report.mjs — plan §3 visibility gate for the P1.2
// sefer-concurrent extraction (workspace-restructure plan, step P1.2).
//
// DERIVES (never types) the crate-boundary visibility picture of
// crates/containers/sefer-concurrent and asserts it:
//   (a) the crate-root re-exports equal exactly the seven legacy types the OLD
//       root module `src/concurrent/mod.rs` re-exported (read via
//       `git show HEAD:src/concurrent/mod.rs`);
//   (b) every other crate-boundary-visible `pub` item that carries a REAL
//       `#[cfg(feature = "internals")]` / `#[cfg(all(feature = "internals",
//       feature = "bench-internals"))]` / `#[cfg(feature = "bench-internals")]`
//       attribute is a test hook — reported with derived counts;
//   (c) the set of crate-boundary-visible `pub` items that are neither (a) nor
//       (b) — the "widened" set — is EMPTY.
//
// Rationale: every `pub` item on the crate boundary was already `pub` and
// reachable from `sefer_alloc` under the same feature combination before the
// move — the seven re-exported types, and the `_*_for_tests` hooks under the
// root's `internals`/`bench-internals` features, which the root features now
// forward to the crate.
//
// A doc-comment mention of a feature name is NOT a gate: only an attribute
// line inside the contiguous attribute block directly above the item counts
// (mirrors the walk-back logic of scripts/verify-alloc-core-dbg-internals-exhaustive.mjs).

import { execSync } from 'node:child_process';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert';

const REPO_ROOT = new URL('..', import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1');
const CRATE_SRC = join(REPO_ROOT, 'crates', 'containers', 'sefer-concurrent', 'src');
const SEVEN = ['EpochHandle', 'EpochRegion', 'LockFreeHandle', 'LockFreeRegion',
  'ShardedHandle', 'ShardedRegion', 'PinnedRunner'];

function fail(msg) { console.error(`p12-visibility-report: FAIL: ${msg}`); process.exit(1); }

// Strip line/block comments and "strings"/'chars' so matching sees code only.
// cfg-attribute feature names are protected with sentinels first — otherwise
// string masking would blank `"internals"` and hide the real gates.
function mask(source) {
  return source
    .replace(/"(internals|bench-internals)"/g, '\x01$1\x01')
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
    .replace(/\/\/[^\n]*/g, (m) => ' '.repeat(m.length))
    .replace(/"(?:[^"\\\n]|\\.)*"/g, (m) => ' '.repeat(m.length))
    .replace(/\x01(internals|bench-internals)\x01/g, '"$1"');
  // note: Rust char literals / lifetimes are NOT masked — a naive '...' regex
  // would pair a lifetime quote with a later quote and mask whole lines.
}

function listRust(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = join(dir, e.name);
    return e.isDirectory() ? listRust(p) : e.name.endsWith('.rs') ? [p] : [];
  });
}

const ITEM_RE = /^(\s*)((?:pub(?:\([^)]*\))?\s+)?)(fn|struct|enum|trait|type|const|static|use|mod)\b/;
const ATTR_RE = /^\s*#\[/;
const DOC_RE = /^\s*(\/\/\/|\/\/!)/;
const REAL_GATES = {
  internals: ['#\[cfg(feature = "internals")\]',
    '#\[cfg(all(feature = "internals", feature = "bench-internals"))\]'],
  bench: ['#\[cfg(feature = "bench-internals")\]',
    '#\[cfg(all(feature = "internals", feature = "bench-internals"))\]'],
};

/** Walk back over the contiguous attribute/doc block above `idx` (0-based,
 *  masked lines) and return the real gate found there, or null. */
function walkBackGate(lines, idx) {
  let i = idx - 1;
  while (i >= 0 && (ATTR_RE.test(lines[i]) || DOC_RE.test(lines[i]))) i--;
  const block = lines.slice(i + 1, idx);
  const norm = (s) => s.trim().replace(/\s+/g, ' ');
  for (const line of block) {
    if (!ATTR_RE.test(line)) continue;
    const t = norm(line);
    if (REAL_GATES.internals.some((g) => t === norm(g))) return 'internals';
    if (REAL_GATES.bench.some((g) => t === norm(g))) return 'bench-internals';
  }
  return null;
}

// ---- 1. collect crate items -------------------------------------------------
const files = listRust(CRATE_SRC);
const relOf = (f) => f.slice(CRATE_SRC.length + 1).replace(/\\/g, '/');

// Module-tree visibility: which top-level modules does lib.rs expose, and how?
const libLines = mask(readFileSync(join(CRATE_SRC, 'lib.rs'), 'utf8')).split('\n');
const moduleVis = new Map(); // top-level module name -> 'pub' | 'restricted' | 'private'
for (const line of libLines) {
  const m = line.match(/^\s*(pub(?:\([^)]*\))?\s+)?mod\s+(\w+)\s*;/);
  if (m) moduleVis.set(m[2], m[1] && m[1].trim() === 'pub' ? 'pub' : m[1] ? 'restricted' : 'private');
}

const reexports = [];   // (a): root-level `pub use` items
const widened = [];     // (c): boundary-visible pub items that are neither (a) nor gated (b)
const internalsGated = []; // (b) internals hooks
const benchGated = [];     // (b) bench-internals-only hooks
const gatedElsewhere = []; // pub items inside non-boundary modules WITHOUT a gate (visibility-bound, not boundary)

for (const file of files) {
  const rel = relOf(file);
  const lines = mask(readFileSync(file, 'utf8')).split('\n');
  for (let i = 0; i < lines.length; i++) {
    const m = lines[i].match(ITEM_RE);
    if (!m) continue;
    const [, , vis, kind] = m;
    const isTruePub = vis.trim() === 'pub';
    if (!isTruePub) continue; // pub(crate)/private items are never boundary-visible
    const atRoot = rel === 'lib.rs';
    const topMod = rel.includes('/') ? rel.split('/')[0].replace(/\.rs$/, '') : null;
    const chainVis = atRoot ? 'root' : (moduleVis.get(topMod) ?? 'restricted');

    if (atRoot && kind === 'use') { reexports.push({ rel, line: i + 1 }); continue; }

    const gate = walkBackGate(lines, i);
    const name = (lines[i].match(/(?:fn|struct|enum|trait|type|const|static|mod)\s+(\w+)/) || [])[1] ?? '?';

    if (chainVis === 'root' || chainVis === 'pub') {
      // crate-boundary-visible
      if (gate === 'internals') internalsGated.push(`${rel}:${i + 1} ${name}`);
      else if (gate === 'bench-internals') benchGated.push(`${rel}:${i + 1} ${name}`);
      else widened.push(`${rel}:${i + 1} ${kind} ${name}`);
    } else {
      // internal module (pub(crate)/private chain): externally reachable only
      // through the root re-exports, exactly as before the move.
      if (gate === 'internals') internalsGated.push(`${rel}:${i + 1} ${name}`);
      else if (gate === 'bench-internals') benchGated.push(`${rel}:${i + 1} ${name}`);
      else gatedElsewhere.push(`${rel}:${i + 1} ${kind} ${name}`);
    }
  }
}

// ---- 2. the seven legacy names: derived from the OLD root module ------------
let oldNames;
try {
  const old = execSync('git show HEAD:src/concurrent/mod.rs', { cwd: REPO_ROOT, encoding: 'utf8' });
  oldNames = [...mask(old).matchAll(/^\s*pub\s+use\s+[\w:]+::(\w+)\s*;/gm)].map((m) => m[1]);
} catch (e) { fail(`cannot read git show HEAD:src/concurrent/mod.rs (${e.message})`); }

const newNames = reexports.map((r) => {
  const src = mask(readFileSync(join(CRATE_SRC, r.rel), 'utf8')).split('\n')[r.line - 1];
  return (src.match(/::(\w+)\s*;/) || [])[1];
});

// ---- 3. assertions ----------------------------------------------------------
assert.deepStrictEqual([...oldNames].sort(), [...SEVEN].sort(),
  `old root module must re-export exactly the seven legacy types (found: ${oldNames})`);
assert.deepStrictEqual([...newNames].sort(), [...SEVEN].sort(),
  `crate root must re-export exactly the seven legacy types (found: ${newNames})`);
assert.deepStrictEqual(newNames.sort(), oldNames.sort(),
  'crate re-export set must equal the old root re-export set');
assert.strictEqual(widened.length, 0, `widened boundary items found: ${widened.join(', ')}`);
assert(internalsGated.length + benchGated.length > 0, 'no gated hooks found — scan is broken');

// ---- 4. self-checking report: printed counts derived, then re-parsed --------
const n = internalsGated.length;
const m = benchGated.length;
const summary = `p12-visibility-report: widened=0; legacy-reexports=${newNames.length}; `
  + `internals-gated-hooks=${n}; bench-internals-gated-hooks=${m}`;
console.log(`internals-gated hooks (${n}):`); internalsGated.forEach((s) => console.log(`  ${s}`));
console.log(`bench-internals-gated hooks (${m}):`); benchGated.forEach((s) => console.log(`  ${s}`));
console.log(`legacy re-exports (${newNames.length}): ${newNames.join(', ')}`);
console.log(summary);
// assert the printed counts equal the computed list lengths (derived, not typed)
assert(new RegExp(`internals-gated-hooks=${n}\\b`).test(summary));
assert(new RegExp(`bench-internals-gated-hooks=${m}\\b`).test(summary));
assert(new RegExp(`legacy-reexports=${newNames.length}\\b`).test(summary));
assert(/widened=0/.test(summary));
