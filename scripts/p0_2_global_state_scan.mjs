#!/usr/bin/env node
// P0.2 global-state interference scan for tests/regression_merged/*.
//
// The P0.2 pilot merged 55 flat test files into one process (75 root regression_* pool minus 20 P0.1 hard blockers; 3 of the original 78 were moved to crates/containers/sefer-concurrent by P1.2); modules whose
// assertions depend on PROCESS-GLOBAL state (dbg_* static counters, panic
// hooks, env vars, RSS/thread queries, global registry lifecycle) can now
// interfere with each other under the parallel in-process harness.
//
// Usage:
//   node scripts/p0_2_global_state_scan.mjs --dir tests/regression_merged [--csv <out>]
//   node scripts/p0_2_global_state_scan.mjs --exclusions docs/perf/p0_2_exclusions.json [--md]
//     (derives the exclusion table from the restored flat tests/<module>.rs files)
//
// Zero deps. Read-only: never writes anything except the optional --csv file
// explicitly requested on the command line. Comment/string stripping mirrors
// scripts/classify-tests (stripComments zone-b semantics; newlines preserved
// so line numbers match the original files).
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { extractInventory } from './test-inventory.mjs';

// ---------------------------------------------------------------- stripping
function stripComments(src) {
  let out = '';
  let i = 0;
  let state = 'code'; // code | line | block | str | chr
  let blockDepth = 0;
  while (i < src.length) {
    const ch = src[i];
    const next = src[i + 1];
    if (state === 'code') {
      if (ch === '/' && next === '/') { state = 'line'; i += 2; continue; }
      if (ch === '/' && next === '*') { state = 'block'; blockDepth = 1; i += 2; continue; }
      if (ch === '"') { state = 'str'; out += '""'; i++; continue; }
      // Distinguish char literals from lifetimes: `'a'` (char) has its
      // closing quote at i+2; `'abc` (lifetime) does not.
      if (ch === "'") {
        const isChar = src[i + 2] === "'" || (src[i + 1] === '\\' && src[i + 3] === "'");
        if (isChar) { state = 'chr'; out += "''"; i++; continue; }
        out += ch; i++; continue;
      }
      out += ch; i++; continue;
    }
    if (state === 'line') {
      if (ch === '\n') { state = 'code'; out += ch; }
      i++; continue;
    }
    if (state === 'block') {
      if (ch === '/' && next === '*') { blockDepth++; i += 2; continue; }
      if (ch === '*' && next === '/') { blockDepth--; i += 2; if (blockDepth === 0) state = 'code'; continue; }
      if (ch === '\n') out += ch;
      i++; continue;
    }
    if (state === 'str') {
      if (ch === '\n') { out += ch; i++; continue; }
      if (ch === '\\') {
        // preserve escaped line-continuations (`"... \\` + newline)
        if (next === '\n') { out += '\n'; i += 2; continue; }
        i += 2; continue;
      }
      if (ch === '"') { state = 'code'; i++; continue; }
      i++; continue;
    }
    // char literal / lifetime (`'static` etc.): preserve newlines so line
    // numbers never drift even when a lifetime is misdetected as a char.
    if (ch === '\n') { out += ch; i++; continue; }
    if (ch === '\\') {
      if (next === '\n') { out += '\n'; i += 2; continue; }
      i += 2; continue;
    }
    if (ch === "'") { state = 'code'; i++; continue; }
    i++;
  }
  return out;
}

// ------------------------------------------------------------------ markers
const MARKERS = [
  { id: 'DBG_COUNTER', re: /\bdbg_[a-z0-9_]+\b/g },
  { id: 'CONFIG_CONFLICTS', re: /\bconfig_conflicts/g },
  { id: 'ALLOC_STATS', re: /\bAllocStats\b|\balloc_stats\b/g },
  { id: 'PANIC_HOOK', re: /\b(set_hook|take_hook)\b/g },
  { id: 'ENV_MUTATION', re: /\b(set_var|remove_var)\b/g },
  { id: 'PROC_QUERY', re: /\b(available_parallelism|sysinfo|memory_footprint|\brss\b|Rss|\/proc\/|isatty|current_exe|CARGO_BIN_EXE|std::process)/g },
  { id: 'REGISTRY_LIFECYCLE', re: /\bbootstrap\s*\(|\bHeapCore::(bootstrap|teardown)|\bteardown_global|install_global|global_registry/gi },
  { id: 'SERIAL_FLAG', re: /\bSERIAL\b/g, info: true },
  { id: 'EXACT_COUNTER_ASSERT', re: null, derived: 'exact_counter_assert' },
  { id: 'MODULE_STATIC', re: /\bAtomicUsize|static\s+[A-Z]/g, info: true, assertionOnly: true },
];

// Exact-counter-assert: an assert_eq!/assert!( ... == ... ) line that also
// mentions `total(` or `dbg_`.
const ASSERT_RE = /assert_eq!\s*\(|assert!\s*\(\s*[^;]*==/;
function isExactCounterAssert(line) {
  if (!ASSERT_RE.test(line)) return false;
  return /\btotal\s*\(/.test(line) || /\bdbg_/.test(line);
}

// Module-local static read across an assertion (marker 10 assertion-only rule).
function staticReadInAssertion(line) {
  if (!/\bAtomicUsize|static\s+[A-Z]/.test(line)) return false;
  return /assert_eq!\s*\(|assert!\s*\(|==/.test(line);
}

// --------------------------------------------------------------------- main
function parseArgs(argv) {
  const args = { dir: 'tests/regression_merged', csv: null, exclusions: null, md: false };
  for (let i = 2; i < argv.length; i++) {
    if (argv[i] === '--dir') args.dir = argv[++i];
    else if (argv[i] === '--csv') args.csv = argv[++i];
    else if (argv[i] === '--exclusions') args.exclusions = argv[++i];
    else if (argv[i] === '--md') args.md = true;
    else if (argv[i] === '--filters') args.filters = true;
  }
  return args;
}


// ------------------------------------------------------------- exclusions
const LEGEND = [
  ['G1', 'exact/ordering assertion on a process-global static counter'],
  ['G2', 'mutates a process-global knob or depends on global registry/fallback lifecycle state'],
  ['G3', 'exact heap-state or pointer-identity assertion on a leased registry heap that presumes a fresh (never recycled) slot, or a test whose non-vacuity depends on minting fresh slots first'],
  ['G4', 'wall-clock / timeout threshold'],
  ['E', 'cited by an evidence-registry row (flat file path appears in `--test <stem>` of a STRONG row; strict mode reports evidence-test-missing if the flat file is absent)'],
];

function runExclusions(jsonPath, md) {
  const fail = (msg) => { console.error('FAIL: ' + msg); process.exit(1); };
  const entries = JSON.parse(readFileSync(jsonPath, 'utf8'));
  const cats = new Set(LEGEND.map((l) => l[0]));
  const rows = [];
  for (const e of entries) {
    if (!cats.has(e.category)) fail(`unknown category ${e.category} for ${e.module}`);
    const file = `tests/${e.module}.rs`;
    let lines;
    try { lines = readFileSync(file, 'utf8').split(/\r?\n/); } catch (err) { fail(`${e.module}: cannot read ${file}: ${err.message}`); }
    const re = new RegExp(e.regex);
    const idx = lines.findIndex((l) => re.test(l));
    if (idx < 0) fail(`module ${e.module}: regex /${e.regex}/ matches no line of ${file}`);
    rows.push({ module: e.module, category: e.category, regex: e.regex, loc: `${file}:${idx + 1}`, code: lines[idx].trim().slice(0, 100) });
  }
  if (entries.some((e) => e.category === 'E')) {
    const reg = readFileSync('docs/evidence/registry.csv', 'utf8');
    for (const e of entries.filter((x) => x.category === 'E')) {
      if (!reg.includes(`--test ${e.module}`)) fail(`module ${e.module}: docs/evidence/registry.csv lacks literal '--test ${e.module}'`);
    }
  }
  rows.sort((a, b) => (a.module < b.module ? -1 : a.module > b.module ? 1 : 0));
  const esc = (t) => String(t).replace(/\|/g, '\\|');
  const out = [];
  out.push('| module | category | marker (regex) | file:line | code (trimmed to 100 chars) |');
  out.push('|---|---|---|---|---|');
  for (const r of rows) out.push(`| ${r.module} | ${r.category} | \`${esc(r.regex)}\` | ${r.loc} | \`${esc(r.code)}\` |`);
  const N = new Set(entries.map((e) => e.module)).size;
  const merged = readFileSync('tests/regression_merged.rs', 'utf8').split(/\r?\n/).filter((l) => /^mod /.test(l)).length;
  const T = N + merged;
  out.push('');
  const K = merged;
  const saved = K - 1;
  if (saved !== K - 1) fail(`saved ${saved} != K-1`);
  out.push(`excluded: ${N} modules, ${rows.length} entries; kept: ${merged} modules (merged); total: ${T}`);
  out.push(`binaries per full-target row: ${K} flat targets -> 1, saved ${saved}`);
  if (T !== 55) fail(`total ${T} != 55 (78 root regression_* pool - 3 moved to crates/containers/sefer-concurrent by P1.2 - 20 P0.1 hard blockers = 55)`);
  if (N !== 23) fail(`excluded modules ${N} != 23`);
  out.push('');
  out.push('Legend:');
  for (const [k, v] of LEGEND) out.push(md ? `- **${k}** = ${v}` : `  ${k} = ${v}`);
  console.log(out.join('\n'));
}

// ---------------------------------------------------------------- filters
// Derived proof that every `regression_<stem>::` filter token used by the
// miri / tsan / feature-isolation recipes selects exactly the tests of its
// own module in the merged target (libtest substring semantics).
function walkFiles(dir, out = []) {
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    const p = join(dir, e.name);
    if (e.isDirectory()) walkFiles(p, out); else out.push(p);
  }
  return out;
}

function runFilters() {
  const sources = [
    ...walkFiles('.github/actions').filter((p) => /\.ya?ml$/.test(p)).sort(),
    'scripts/miri-matrix.mjs',
    'scripts/tsan.mjs',
  ];
  const tokens = [];
  for (const file of sources) {
    const lines = readFileSync(file, 'utf8').split(/\r?\n/);
    lines.forEach((l, i) => {
      for (const m of l.matchAll(/regression_[A-Za-z0-9_]+::/g)) {
        tokens.push({ token: m[0], loc: `${file.replace(/\\/g, '/')}:${i + 1}` });
      }
    });
  }
  const dir = 'tests/regression_merged';
  const all = [];
  const byModule = new Map();
  for (const f of readdirSync(dir).filter((x) => x.endsWith('.rs')).sort()) {
    const stem = f.slice(0, -3);
    const inv = extractInventory(readFileSync(join(dir, f), 'utf8'));
    const paths = inv.tests.map((t) => `${stem}::${t.path}`);
    byModule.set(stem, paths.length);
    all.push(...paths.map((p) => ({ stem, path: p })));
  }
  const rows = [];
  let violations = 0;
  for (const t of tokens) {
    const mod = t.token.slice(0, -2);
    const matched = all.filter((x) => x.path.includes(t.token));
    const inMod = byModule.get(mod);
    const foreign = matched.filter((x) => x.stem !== mod).length;
    const ok = inMod !== undefined && matched.length > 0 && foreign === 0 && matched.length === inMod;
    if (!ok) violations++;
    rows.push(`| ${t.token.slice(0, -2)}:: | ${t.loc} | ${mod} | ${matched.length} | ${inMod === undefined ? 'n/a' : inMod} | ${ok ? 'OK' : 'VIOLATION'} |`);
  }
  console.log('| filter | source file:line | module | tests matched | tests in module | OK |');
  console.log('|---|---|---|---|---|---|');
  for (const r of rows) console.log(r);
  console.log('');
  console.log(`filters: ${tokens.length} tokens, ${new Set(tokens.map((t) => t.token)).size} distinct, ${all.length} tests in ${byModule.size} merged modules, ${violations} violations`);
  process.exit(violations ? 1 : 0);
}

const args = parseArgs(process.argv);
if (args.filters) runFilters();
if (args.exclusions) { runExclusions(args.exclusions, args.md); process.exit(0); }
let files;
try {
  files = readdirSync(args.dir).filter((f) => f.endsWith('.rs')).sort();
} catch (e) {
  console.error(`cannot read dir ${args.dir}: ${e.message}`);
  process.exit(1);
}

const perModule = [];
const csvRows = [['module', 'marker', 'file:line', 'code line']];

for (const file of files) {
  const module = file.replace(/\.rs$/, '');
  const path = join(args.dir, file);
  const stripped = stripComments(readFileSync(path, 'utf8'));
  const lines = stripped.split('\n');
  const rawLines = readFileSync(path, 'utf8').split('\n');
  const testCount = lines.filter((l) => /#\s*\[\s*test\]/.test(l)).length;
  const hits = [];

  lines.forEach((line, idx) => {
    for (const m of MARKERS) {
      if (m.derived === 'exact_counter_assert') {
        if (isExactCounterAssert(line)) hits.push({ marker: m.id, line: idx + 1, code: rawLines[idx].trim() });
        continue;
      }
      if (m.assertionOnly && !staticReadInAssertion(line)) continue;
      m.re.lastIndex = 0;
      const seen = new Set();
      let match;
      while ((match = m.re.exec(line)) !== null) {
        const key = m.id + ':' + match[0];
        if (!seen.has(key)) {
          seen.add(key);
          hits.push({ marker: m.id, matched: match[0], line: idx + 1, code: rawLines[idx].trim() });
        }
        if (m.re.lastIndex === match.index) m.re.lastIndex++;
      }
    }
  });

  perModule.push({ module, file, path, testCount, hits });
}

// ------------------------------------------------------------------- output
const L = [];
const push = (s = '') => L.push(s);

push('=== P0.2 global-state interference scan ===');
push(`dir: ${args.dir}   modules: ${files.length}`);
push('');

// (a) hit list grouped by module
push('--- (a) HIT LIST (grouped by module) ---');
for (const mod of perModule) {
  if (mod.hits.length === 0) continue;
  push('');
  push(`## ${mod.module}  (${mod.hits.length} hit${mod.hits.length === 1 ? '' : 's'})`);
  for (const h of mod.hits) {
    push(`  [${h.marker}] ${mod.file}:${h.line}`);
    push(`    ${h.code}`);
    csvRows.push([mod.module, h.marker, `${mod.file}:${h.line}`, h.code]);
  }
}
const noHit = perModule.filter((m) => m.hits.length === 0).map((m) => m.module);
push('');
push(`modules with zero hits (${noHit.length}): ${noHit.join(', ') || '(none)'}`);
push('');

// proposal
push('--- PROPOSAL TABLE (proposals only — NOT a final decision) ---');
push('module | suspected categories | verdict candidate | reason');
const EXCLUDE_GROUND_TRUTH = new Set(['regression_c3_unbounded_recycle']);
for (const mod of perModule) {
  const cats = [...new Set(mod.hits.filter((h) => !/^SERIAL_FLAG$|^MODULE_STATIC$/.test(h.marker)).map((h) => h.marker))];
  const serialOnly = mod.hits.some((h) => h.marker === 'SERIAL_FLAG');
  const infoOnly = mod.hits.length > 0 && cats.length === 0;
  let verdict = 'KEEP';
  let reason = 'no process-global-state markers on code lines';
  if (EXCLUDE_GROUND_TRUTH.has(mod.module)) {
    verdict = 'EXCLUDE';
    const h = mod.hits.find((x) => x.marker === 'EXACT_COUNTER_ASSERT' || x.marker === 'DBG_COUNTER');
    reason = `parent-confirmed: exact deltas of dbg_segments_* totals (see ${mod.file}:${h ? h.line : '?'})`;
  } else if (cats.length > 0) {
    verdict = 'REVIEW';
    const h = mod.hits.find((x) => !/^SERIAL_FLAG$|^MODULE_STATIC$/.test(x.marker));
    reason = `hits ${cats.join(',')} — e.g. ${mod.file}:${h.line} \`${h.code}\``;
  } else if (infoOnly) {
    verdict = 'KEEP';
    const h = mod.hits[0];
    reason = `informational only (${mod.hits.map((x) => x.marker).join(',')}) — e.g. ${mod.file}:${h.line}`;
  }
  if (serialOnly && verdict === 'KEEP') {
    reason += '; has SERIAL flag but per-file serial does not protect against OTHER modules';
  }
  push(`${mod.module} | ${cats.length ? cats.join(',') : (infoOnly ? mod.hits.map((h) => h.marker).join(',') : '(none)')} | ${verdict} | ${reason}`);
  csvRows.push([mod.module, cats.join(',') || '(none)', verdict, reason]);
}
push('');

// (c) per-module test counts
push('--- (c) per-module #[test] counts (comment-stripped) ---');
push('module | tests');
for (const mod of perModule) push(`${mod.module} | ${mod.testCount}`);
const totalTests = perModule.reduce((a, m) => a + m.testCount, 0);
push(`TOTAL | ${totalTests}`);
push('');
const verdictCounts = {};
for (const mod of perModule) {
  const v = mod.hits.length === 0 ? 'KEEP' : EXCLUDE_GROUND_TRUTH.has(mod.module) ? 'EXCLUDE' : 'REVIEW';
  verdictCounts[v] = (verdictCounts[v] || 0) + 1;
}
push(`summary: EXCLUDE-candidate ${verdictCounts.EXCLUDE || 0}, REVIEW-candidate ${verdictCounts.REVIEW || 0}, KEEP ${verdictCounts.KEEP || 0} (proposals only)`);

const stdout = L.join('\n');
console.log(stdout);

if (args.csv) {
  writeFileSync(args.csv, csvRows.map((r) => r.map((c) => `"${String(c).replace(/"/g, '""')}"`).join(',')).join('\n') + '\n');
  console.log(`\ncsv written: ${args.csv}`);
}
