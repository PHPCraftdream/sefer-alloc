// Judge ("судья") over the evidence registry for plan step 7 (#2098/Ph7).
//
// The evidence registry (docs/evidence/registry.csv) is a table of claimed
// evidence rows — one per model/OS/toolchain transition — each carrying a
// status from a fixed dictionary, a positive marker, a mutant result, a
// receipt file and a source_sha pinning the claim to a commit. This script is
// the judge over that table: a pure-stdlib scan (no cargo/npm) that validates
// structure and vocabularies, applies the §2 draft's forbidden-auto-elevation
// rules, cross-checks receipts (existence/roots/pins/sha256), test-file cfgs
// against the Cargo feature graph, source_sha identity vs HEAD, required
// cells, and an optional baseline ratchet. Verdict:
//
//   RED        — any violation found (or, in --strict, any strict-mode
//                shortfall: missing required cells, PASS rows pinned to an
//                unpublished source_sha, shallow repo, empty PASS source_sha)
//                                                            (exit 1)
//   INCOMPLETE — no violations, but required cells are missing / awaiting a
//                final status, or verdict rows are pinned to an unpublished
//                source_sha (plain mode)                     (exit 0)
//   GREEN      — otherwise                                  (exit 0)
//
// Identity shortfalls are deliberately NOT red in plain mode: an honest
// registry that predates the merge of its evidence commits is incomplete,
// not wrong. --strict closes that gap for PASS/KNOWN-* rows.
//
// Usage: node scripts/verify-evidence-registry.mjs
//          [--registry <path>] [--required <path>] [--repo-root <path>]
//          [--strict] [--baseline <git-ref>] [--strict-baseline] [--json]
// Paths default to <git-root>/docs/evidence/{registry,required}.csv; repo root
// defaults to `git rev-parse --show-toplevel`.

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs';
import { join, resolve } from 'node:path';

const REGISTRY_HEADER = [
  'id', 'transition', 'entry', 'features', 'os', 'toolchain', 'profile',
  'source_sha', 'source_tree', 'binary_id', 'activation', 'positive_marker',
  'mutant', 'mutant_rejected', 'receipt', 'receipt_sha256', 'scope', 'status',
  'status_reason', 'owner_phase', 'source_cell',
];
const REQUIRED_HEADER = [
  'cell_id', 'class', 'gate', 'expects', 'requires_mutant', 'note',
];
const STATUS = ['PASS', 'FAIL', 'KNOWN-DEFECT', 'KNOWN-RED', 'MODEL-LIMIT',
  'NOT_RUN', 'CFG_EXCLUDED', 'BUILD_ONLY', 'INCONCLUSIVE', 'CI_PENDING',
  'CONDITIONAL'];
const SCOPE = ['shipping-installed', 'low-level-api', 'shadow-model',
  'isolated-witness', 'crate-internal', 'bench-only', 'tool-only',
  'process-witness'];
const VERDICT_STATUS = new Set(['PASS', 'FAIL', 'KNOWN-DEFECT', 'KNOWN-RED',
  'MODEL-LIMIT', 'CONDITIONAL', 'INCONCLUSIVE']);
const STRONG_STATUS = new Set(['PASS', 'KNOWN-DEFECT', 'KNOWN-RED']);
const PENDING_STATUS = new Set(['NOT_RUN', 'CI_PENDING', 'CONDITIONAL',
  'INCONCLUSIVE']);
const BOOL = new Set(['true', 'false', '']);
const GATES = new Set(['C1', 'C2', 'C3', 'C4', 'C5', 'C6', 'C7', 'COST']);
const ID_RE = /^[A-Za-z0-9][A-Za-z0-9._-]*$/;
const RECEIPT_ROOTS = ['docs/', 'tests/', 'scripts/', 'benches/', 'examples/'];
// Baseline statuses from which promotion to PASS requires fresh evidence
// fields (ratchet, A4.ii).
const PROMOTABLE = new Set(['NOT_RUN', 'CI_PENDING', 'INCONCLUSIVE',
  'CONDITIONAL', 'CFG_EXCLUDED', 'BUILD_ONLY', 'KNOWN-RED', 'KNOWN-DEFECT',
  'MODEL-LIMIT', 'FAIL']);

// ---------------------------------------------------------------- CLI ---

function usage() {
  console.error(
`usage: node scripts/verify-evidence-registry.mjs [options]
  --registry <path>    path to registry.csv (default <repo-root>/docs/evidence/registry.csv)
  --required <path>    path to required_cells.csv (default <repo-root>/docs/evidence/required_cells.csv)
  --repo-root <path>   repository root (default: git rev-parse --show-toplevel)
  --strict             strict mode: identity/required shortfalls become RED
  --baseline <ref>     ratchet against docs/evidence/registry.csv at <git-ref>
  --strict-baseline    an unresolvable baseline is RED instead of skipped
  --json               final JSON on stdout (all human output goes to stderr)`);
}

const argv = process.argv.slice(2);
const opts = { registry: null, required: null, repoRoot: null, json: false,
  strict: false, baseline: null, strictBaseline: false };
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (a === '--json') { opts.json = true; continue; }
  if (a === '--strict') { opts.strict = true; continue; }
  if (a === '--strict-baseline') { opts.strictBaseline = true; continue; }
  const key = { '--registry': 'registry', '--required': 'required',
    '--repo-root': 'repoRoot', '--baseline': 'baseline' }[a];
  if (!key || i + 1 >= argv.length) { usage(); process.exit(2); }
  opts[key] = argv[++i];
}

function git(args, allowFail = true) {
  return gitRaw(args, allowFail)?.trim() ?? null;
}
// Raw variant for content equality (show): never trims, so trailing newlines
// survive byte-for-byte comparisons.
function gitRaw(args, allowFail = true) {
  const r = spawnSync('git', args, { cwd: repoRoot, encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'] });
  if (r.status !== 0) {
    if (!allowFail) throw new Error(`git ${args.join(' ')} failed: ${r.stderr}`);
    return null;
  }
  return r.stdout;
}

const repoRoot = opts.repoRoot ? resolve(opts.repoRoot)
  : (spawnSync('git', ['rev-parse', '--show-toplevel'],
    { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }).stdout || '').trim() || null;
if (!repoRoot) { usage(); process.exit(2); }
const registryPath = opts.registry ?? join(repoRoot, 'docs', 'evidence', 'registry.csv');
const requiredPath = opts.required ?? join(repoRoot, 'docs', 'evidence', 'required_cells.csv');

// Human-readable lines go to stdout in plain mode, stderr in --json so that
// stdout carries only the final JSON document.
const emit = (line) => (opts.json ? console.error : console.log)(line);

// -------------------------------------------------------------- CSV ----
// Minimal RFC4180 parser: quoted fields, escaped "" quotes, commas and
// newlines inside quotes. BOM stripped, \r\n / \r normalized to \n first.
// Reports an unterminated trailing quote instead of silently accepting it.
function parseCsv(text) {
  text = text.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  const rows = [];
  let row = [], field = '', inQuotes = false;
  const endField = () => { row.push(field); field = ''; };
  const endRow = () => { endField(); rows.push(row); row = []; };
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inQuotes) {
      if (c === '"') {
        if (text[i + 1] === '"') { field += '"'; i++; } else inQuotes = false;
      } else field += c;
    } else if (c === '"' && field === '') inQuotes = true;
    else if (c === ',') endField();
    else if (c === '\n') endRow();
    else field += c;
  }
  if (field !== '' || row.length > 0) endRow();
  return { rows: rows.filter((r) => !(r.length === 1 && r[0].trim() === '')),
    unterminated: inQuotes };
}

function readCsv(path) {
  if (!existsSync(path)) throw new Error(`file not found: ${path}`);
  return parseCsv(readFileSync(path, 'utf8'));
}

function toRow(header, cells) {
  const o = {};
  header.forEach((h, i) => { o[h] = cells[i] ?? ''; });
  return o;
}

const lf = (s) => s.replace(/\r\n?/g, '\n');
function sha256Lf(path) {
  return createHash('sha256').update(lf(readFileSync(path, 'utf8'))).digest('hex');
}

// ------------------------------------------------------- violations ----

const violations = [];          // {rule, id, message}
const identityUnresolved = [];  // row ids
const identityWarnings = [];
const requiredMissing = [];
const pendingRequired = [];
const violationsByRule = () =>
  violations.reduce((m, v) => (m[v.rule] = (m[v.rule] ?? 0) + 1, m), {});

function violation(rule, id, message) {
  violations.push({ rule, id, message });
  emit(`RED: ${rule} row ${id}: ${message}`);
}

// ------------------------------------------------- git identity cache ---

const shallow = git(['rev-parse', '--is-shallow-repository']) === 'true';
const shaCache = new Map(); // sha -> {resolved: sha|null, ancestor: bool|null}

function identity(sha) {
  if (shaCache.has(sha)) return shaCache.get(sha);
  const res = { resolved: null, ancestor: null };
  const full = git(['rev-parse', '--verify', '--quiet', `${sha}^{commit}`]);
  if (full) {
    res.resolved = full;
    if (!shallow) {
      res.ancestor = git(['merge-base', '--is-ancestor', full, 'HEAD']) !== null;
    }
  }
  shaCache.set(sha, res);
  return res;
}

// ----------------------------------------------------- feature graph ---

function loadCargoFeatures() {
  try {
    const cargo = lf(readFileSync(join(repoRoot, 'Cargo.toml'), 'utf8'));
    const fsec = cargo.split(/^\[features\]\s*$/m)[1].split(/^\[/m)[0];
    const feats = {};
    for (const m of fsec.matchAll(/^([A-Za-z0-9_\-]+)\s*=\s*\[([\s\S]*?)\]/gm)) {
      feats[m[1]] = [...m[2].matchAll(/"([^"]+)"/g)].map((x) => x[1]);
    }
    return feats;
  } catch { return null; } // unparsable graph: cfg checks silently skipped
}
const cargoFeatures = loadCargoFeatures();
const allFeatures = new Set(Object.keys(cargoFeatures ?? {}));

// Closure of a feature list over [features]; "a/b" and "dep:x" add only the
// name being expanded, never the dep itself.
function featureClosure(list) {
  const s = new Set(), st = [...list];
  while (st.length) {
    const f = st.pop();
    if (s.has(f)) continue;
    s.add(f);
    for (const d of cargoFeatures[f] ?? []) {
      if (!d.includes('/') && !d.startsWith('dep:')) st.push(d);
    }
  }
  return s;
}

// Tokens of a registry features field: whitespace/comma/plus separated, drop
// empty, em-dash placeholders, --flags and parenthesized annotations.
function featureTokens(features) {
  return features.split(/[\s,+]+/).map((t) => t.trim())
    .filter((t) => t !== '' && t !== '—' && !t.startsWith('--') && !/[()]/.test(t));
}

function rowFeatureSet(row) {
  if (/--all-features/.test(row.features)) return allFeatures;
  return featureClosure(featureTokens(row.features));
}

// Evaluate the inside of a cfg(...) against a feature set with three-valued
// logic: feature atoms are true/false, every other atom (target_os,
// debug_assertions, windows, miri, …) is unknown (null), so `not(windows)` is
// unknown, not false. Callers flag a test only when the result is exactly
// false. Throws on a structure it cannot parse.
function evalCfg(expr, feats) {
  const toks = [];
  const re = /\s*(all\b|any\b|not\b|feature\b|"[^"]*"|[()=,]|[A-Za-z0-9_\-]+)/gy;
  let m, last = 0;
  while ((m = re.exec(expr))) {
    if (m.index !== last) throw new Error(`bad cfg token at ${m.index}`);
    last = re.lastIndex;
    toks.push(m[1]);
  }
  if (last !== expr.length) throw new Error('bad cfg tail');
  let pos = 0;
  const peek = () => toks[pos];
  function atom() {
    const t = toks[pos++];
    if (t === '(' || t === ')' || t === ',') throw new Error(`unexpected ${t}`);
    if (t === 'feature') {
      if (peek() !== '=') throw new Error('feature without =');
      pos++;
      const v = toks[pos++];
      if (!/^"[^"]*"$/.test(v)) throw new Error('feature without string');
      return feats.has(v.slice(1, -1));
    }
    if (t === 'all' || t === 'any' || t === 'not') {
      if (peek() !== '(') throw new Error(`${t} without (`);
      pos++;
      const args = [];
      if (peek() !== ')') {
        for (;;) {
          args.push(atom());
          if (peek() === ',') { pos++; if (peek() === ')') break; continue; }
          break;
        }
      }
      if (peek() !== ')') throw new Error(`${t} unclosed`);
      pos++;
      if (t === 'all') {
        if (args.some((a) => a === false)) return false;
        return args.some((a) => a === null) ? null : true;
      }
      if (t === 'any') {
        if (args.some((a) => a === true)) return true;
        return args.some((a) => a === null) ? null : false;
      }
      if (args.length !== 1) throw new Error('not arity');
      return args[0] === null ? null : !args[0];
    }
    if (peek() === '=') { pos++; pos++; } // `name = "value"` predicate
    return null; // bare string atom / unknown predicate: unverifiable
  }
  const v = atom();
  if (pos !== toks.length) throw new Error('cfg trailing tokens');
  return v;
}

// Every `#![cfg(...)]` inner attribute of a Rust file (already
// comment-stripped), in order; a file is compiled only if ALL of them hold.
function allInnerCfgs(text) {
  const out = [];
  let from = 0;
  for (;;) {
    const at = text.indexOf('#![cfg(', from);
    if (at < 0) return out;
    const open = text.indexOf('(', at);
    let depth = 0, inStr = false, end = -1;
    for (let i = open; i < text.length; i++) {
      const c = text[i];
      if (inStr) { if (c === '"' && text[i - 1] !== '\\') inStr = false; continue; }
      if (c === '"') inStr = true;
      else if (c === '(') depth++;
      else if (c === ')') { depth--; if (depth === 0) { end = i; break; } }
    }
    if (end < 0) return out;
    out.push(text.slice(open + 1, end));
    from = end;
  }
}

// docs/evidence/spike_commits.csv lists the commits that legitimately never
// reach main; only those may back a non-strong verdict row with an unresolved
// or non-ancestor source_sha (the row must also say "spike-not-merged").
let spikeShas = null;
function isListedSpike(sha) {
  if (spikeShas === null) {
    spikeShas = [];
    try {
      for (const cells of parseCsv(readFileSync(join(repoRoot, 'docs', 'evidence', 'spike_commits.csv'), 'utf8')).rows.slice(1)) {
        if (/^[0-9a-f]{40}$/i.test((cells[0] ?? '').trim())) spikeShas.push(cells[0].trim().toLowerCase());
      }
    } catch { /* no allowlist: no spike is allowed */ }
  }
  const s = sha.toLowerCase();
  return /^[0-9a-f]{8,40}$/.test(s) && spikeShas.some((full) => full.startsWith(s));
}

// ------------------------------------------------------ tests cache ----

const TESTS_RE = /(tests\/[A-Za-z0-9_\-\/]+\.rs)/g;
let testStems = null; // top-level tests/*.rs stems, lazily listed
function testStemSet() {
  if (testStems) return testStems;
  testStems = new Set();
  try {
    for (const f of readdirSync(join(repoRoot, 'tests'))) {
      if (f.endsWith('.rs')) testStems.add(f.slice(0, -3));
    }
  } catch { /* no tests dir (shallow/no-checkout fixture root) */ }
  return testStems;
}
// Test files a row cites: literal tests/<x>.rs paths, `tests::<x>`, `--test <x>`
// and bare identifiers (>= 8 chars with an underscore) naming an existing
// tests/<x>.rs. A strong row citing a test that does not exist is stale.
function testsFrom(...fields) {
  const out = new Set();
  const stems = testStemSet();
  for (const f of fields) {
    if (!f) continue;
    for (const m of f.matchAll(TESTS_RE)) out.add(m[1]);
    // `tests::<x>` may name a test function, not a file: count it only when
    // tests/<x>.rs exists.
    for (const m of f.matchAll(/tests::([A-Za-z0-9_]+)/g)) {
      if (stems.has(m[1])) out.add(`tests/${m[1]}.rs`);
    }
    for (const m of f.matchAll(/--test\s+([A-Za-z0-9_]+)/g)) out.add(`tests/${m[1]}.rs`);
    for (const m of f.matchAll(/[A-Za-z0-9_]{8,}/g)) {
      if (m[0].includes('_') && stems.has(m[0])) out.add(`tests/${m[0]}.rs`);
    }
  }
  return [...out];
}
// Fields that name the tests a row stands on.
const rowTests = (row) => testsFrom(row.entry, row.transition, row.activation, row.mutant);

const testFileCache = new Map(); // path -> string|null
function testFile(rel) {
  if (testFileCache.has(rel)) return testFileCache.get(rel);
  let text = null;
  try { text = readFileSync(join(repoRoot, rel), 'utf8'); } catch { /* absent */ }
  testFileCache.set(rel, text);
  return text;
}

const gitBlobCache = new Map(); // `${sha}:${path}` -> string|null (cat-file)
function blobAt(sha, rel) {
  const key = `${sha}:${rel}`;
  if (gitBlobCache.has(key)) return gitBlobCache.get(key);
  let text = null;
  if (git(['cat-file', '-e', `${sha}:${rel}`]) !== null) {
    text = gitRaw(['show', `${sha}:${rel}`]);
  }
  gitBlobCache.set(key, text);
  return text;
}

// ------------------------------------------------- prose heuristics ----

// A claim token is suppressed when a negation ("not / не / no") appears in
// the 24 characters preceding the match.
function negated(hay, match) {
  return /not|не\b|no\b/i.test(hay.slice(Math.max(0, match.index - 24), match.index));
}
const notRunWindow = (hay, match) =>
  /NOT_RUN|NOT RUN|не\s+(измеря|проверя|подтвержд|покрыва)|not\s+(measured|run|covered|claimed)|вне\s|out of scope/i
    .test(hay.slice(Math.max(0, match.index - 28),
      Math.min(hay.length, match.index + match[0].length + 48)));

// ------------------------------------------------------------ rows ----

const registryParsed = readCsv(registryPath);
if (registryParsed.unterminated) {
  violation('csv-unterminated-quote', '-',
    'registry.csv ends inside an unterminated quoted field');
}
if (JSON.stringify(registryParsed.rows[0] ?? []) !== JSON.stringify(REGISTRY_HEADER)) {
  const msg = 'registry.csv header does not match the 21-column reference header (order matters)';
  violation('schema-columns', '-', msg);
  emit(`[evidence-registry] mode: ${opts.strict ? 'strict' : 'plain'}`);
  emit(`[evidence-registry] VERDICT: RED`);
  emit(`[evidence-registry] reason: violations: schema-columns(1)`);
  emit(`[evidence-registry] exit: 1`);
  if (opts.json) {
    console.log(JSON.stringify({ verdict: 'RED', exit: 1, mode: opts.strict ? 'strict' : 'plain',
      violations: [{ rule: 'schema-columns', id: '-', message: msg }],
      identityUnresolved: [], identityWarnings: [], requiredMissing: [], pendingRequired: [],
      counters: { byStatus: {}, byPhase: {}, byScope: {}, published: 0 } }));
  }
  process.exit(1);
}

const rows = registryParsed.rows.slice(1).map((cells) => toRow(REGISTRY_HEADER, cells));
registryParsed.rows.slice(1).forEach((cells, i) => {
  if (cells.length !== REGISTRY_HEADER.length) {
    violation('csv-column-count', String(cells[0] ?? `line ${i + 2}`),
      `expected ${REGISTRY_HEADER.length} columns, got ${cells.length}`);
  }
});

const requiredParsed = readCsv(requiredPath);
if (requiredParsed.unterminated) {
  violation('csv-unterminated-quote', '-',
    'required_cells.csv ends inside an unterminated quoted field');
}
if (JSON.stringify(requiredParsed.rows[0] ?? []) !== JSON.stringify(REQUIRED_HEADER)) {
  violation('required-schema', '-',
    `required_cells.csv header must be exactly ${REQUIRED_HEADER.join(',')}`);
}
const requiredRows = requiredParsed.rows.slice(1)
  .map((cells) => toRow(requiredParsed.rows[0] ?? REQUIRED_HEADER, cells));
requiredParsed.rows.slice(1).forEach((cells, i) => {
  if (cells.length !== (requiredParsed.rows[0] ?? REQUIRED_HEADER).length) {
    violation('csv-column-count', String(cells[0] ?? `line ${i + 2}`),
      `required_cells.csv: expected ${(requiredParsed.rows[0] ?? REQUIRED_HEADER).length} columns, got ${cells.length}`);
  }
});

const seenIds = new Map();
const seenCellIds = new Map();
for (const req of requiredRows) {
  const cid = req.cell_id;
  if (cid !== cid.trim() || cid === '' || !ID_RE.test(cid)) {
    violation('id-format', cid, `cell_id must match ${ID_RE} and have no surrounding whitespace`);
  }
  if (seenCellIds.has(cid)) {
    violation('required-dup-cell', cid, `duplicate cell_id (first seen in row ${seenCellIds.get(cid)})`);
  } else seenCellIds.set(cid, cid);
  if (!GATES.has(req.gate.trim())) {
    violation('required-gate', cid, `gate must be one of C1..C7/COST, got "${req.gate}"`);
  }
  const expects = req.expects.split('|').map((s) => s.trim()).filter((s) => s !== '');
  if (expects.length === 0 || expects.some((e) => !STATUS.includes(e))) {
    violation('required-expects', cid,
      `expects must be a non-empty |-list of status dictionary values, got "${req.expects}"`);
  }
  if (req.requires_mutant.trim() !== 'true' && req.requires_mutant.trim() !== 'false') {
    violation('required-bool', cid, `requires_mutant must be true/false, got "${req.requires_mutant}"`);
  }
}

for (const row of rows) {
  const id = row.id;
  if (id !== id.trim() || id === '' || !ID_RE.test(id)) {
    violation('id-format', id, `id must match ${ID_RE} and have no surrounding whitespace`);
  }
  if (seenIds.has(id)) violation('dup-id', id, `duplicate id (first seen in row ${seenIds.get(id)})`);
  else seenIds.set(id, id);
  if (!STATUS.includes(row.status)) violation('status-dictionary', id, `unknown status "${row.status}"`);
  if (!SCOPE.includes(row.scope)) violation('scope-vocabulary', id, `unknown scope "${row.scope}"`);
  if (!BOOL.has(row.mutant_rejected)) violation('bool-mutant-rejected', id, `mutant_rejected must be true/false/empty, got "${row.mutant_rejected}"`);
  const isVerdict = VERDICT_STATUS.has(row.status);
  const isStrong = STRONG_STATUS.has(row.status);

  // (a) forbidden auto-elevations
  if (row.status === 'PASS' && row.activation.trim() === '') {
    violation('pass-requires-activation', id, 'PASS without activation');
  }
  if (row.status === 'PASS' && row.positive_marker.trim() === '') {
    violation('pass-requires-marker', id, 'PASS without positive_marker');
  }
  const cfgEmpty = /cfg-empty|cfg-пуст|(?:^|[^0-9])0 tests|(?:^|[^0-9])0 passed|(?:^|[^0-9])0 тест|no tests|not compiled|не скомпилир|не собран|вырезан по cfg/i;
  if (row.status === 'PASS' && (cfgEmpty.test(row.activation) || cfgEmpty.test(row.positive_marker))) {
    violation('pass-marker-cfg-empty', id, 'green by cfg exclusion (activation/marker is an empty-run witness)');
  }

  // (d) loom/shadow results stay in the shadow-model scope; shipping tokens
  // in the entry are forbidden unless a negation window disclaims them.
  const isLoom = /loom/i.test(row.entry + row.profile + row.features + row.activation) ||
    row.scope === 'shadow-model';
  if (isLoom) {
    if (row.scope !== 'shadow-model') {
      violation('shadow-loom-not-shipping', id,
        `loom row must carry scope "shadow-model", got "${row.scope}"`);
    }
    const forb = /installed|shipping|#\[global_allocator\]/gi;
    let m;
    while ((m = forb.exec(row.entry))) {
      if (!/not|не|no/i.test(row.entry.slice(Math.max(0, m.index - 24), m.index))) {
        violation('shadow-loom-not-shipping', id,
          `shadow/loom entry claims shipping/installed ("${m[0]}") without a negation`);
        break;
      }
    }
  }

  // (c) miri rows carry exactly one memory-model token, consistent with flags
  if (isVerdict && /miri/i.test(row.profile + row.entry + row.toolchain + row.activation)) {
    const sb = /\bSB\b|stacked|strict-provenance/i, tb = /\bTB\b|tree.?borrows/i;
    const hasSb = sb.test(row.profile), hasTb = tb.test(row.profile);
    if (hasSb && hasTb) {
      violation('miri-model-mismatch', id, 'profile carries both SB and TB model tokens');
    } else if (!hasSb && !hasTb) {
      violation('miri-model-mismatch', id, 'miri verdict row without an SB/TB model token in profile');
    }
    // Model hints in any other field must not contradict the profile token.
    const hay = [row.entry, row.transition, row.activation, row.positive_marker, row.mutant,
      row.status_reason, row.features].join(' ');
    const sbHint = /\bSB\b|stacked.?borrows|-Zmiri-strict-provenance|MIRIFLAGS (?:S|PS)\b|(?:^|[-_ (])sb(?:[-_ )]|$)/i.test(hay);
    const tbHint = /\bTB\b|tree.?borrows|-Zmiri-tree-borrows|MIRIFLAGS (?:T|PT)\b|(?:^|[-_ (])tb(?:[-_ )]|$)/i.test(hay);
    if (hasSb && !hasTb && tbHint && !sbHint) {
      violation('miri-model-mismatch', id, 'profile says SB but every other field only mentions TB');
    }
    if (hasTb && !hasSb && sbHint && !tbHint) {
      violation('miri-model-mismatch', id, 'profile says TB but every other field only mentions SB');
    }
    const flags = row.activation + ' ' + row.positive_marker;
    const treeFlag = /-Zmiri-tree-borrows/i.test(flags);
    const sbFlag = /-Zmiri-strict-provenance/i.test(flags) || /MIRIFLAGS (S|PS)\b/.test(flags);
    if (treeFlag && !hasTb) {
      violation('miri-model-mismatch', id, '-Zmiri-tree-borrows flag but profile is not TB');
    }
    if (sbFlag && !treeFlag && !hasSb) {
      violation('miri-model-mismatch', id, 'strict-provenance/MIRIFLAGS S|PS flag but profile is not SB');
    }
  }

  // (л) allocation counters must not be backed by RSS/latency claims
  const counter = /requested.?bytes|step.?count|instruction.?count|ΔIr|EstCycles|\bIr\b/i;
  const claimRes = [/\bRSS\b|latency|p99|page fault/gi, /\bRSS\b[^.;]{0,24}[-−]?\d/gi];
  if (row.status === 'PASS' && counter.test(row.positive_marker)) {
    const hay = row.transition + ' ' + row.status_reason + ' ' + row.positive_marker;
    const hasClaim = claimRes.some((re) => [...hay.matchAll(re)]
      .some((m) => !negated(hay, m) && !notRunWindow(hay, m)));
    if (hasClaim) {
      violation('counter-not-rss', id, 'allocation counter marker coupled with an RSS/latency claim');
    }
  }

  // (л) isolated witness must not be sold as Sefer acceptance (both orders)
  if (row.scope === 'isolated-witness' && row.status === 'PASS') {
    const hay = row.status_reason + row.transition;
    const pats = [/sefer.{0,40}(accept|принят|acceptance)/gi,
      /приёмка\s+Sefer|Sefer\s+(принимает|accepts)/gi];
    const hit = pats.some((re) => [...hay.matchAll(re)]
      .some((m) => !negated(hay, m) && !notRunWindow(hay, m)));
    if (hit) violation('isolated-witness-not-sefer', id, 'isolated witness presented as Sefer acceptance');
  }

  // (л) KNOWN-DEFECT needs a real file:line cause signature
  if (row.status === 'KNOWN-DEFECT') {
    const sig = /((?:src|crates|tests|benches|examples|scripts|\.github)\/\S*\.(?:rs|yml|toml|mjs)|Cargo\.toml):\d+/.exec(row.status_reason);
    if (!sig || !/(←|<-)/.test(row.status_reason)) {
      violation('known-defect-signature', id, 'KNOWN-DEFECT without a cause signature (file:line + ←)');
    } else if (!existsSync(join(repoRoot, sig[1]))) {
      violation('known-defect-signature', id, `KNOWN-DEFECT signature file does not exist: ${sig[1]}`);
    }
  }

  // (л) FAIL must be justified; resolved-by-decision: needs a real repo path
  if (row.status === 'FAIL') {
    const dec = /resolved-by-decision:/i.exec(row.status_reason);
    let decOk = false;
    if (dec) {
      const target = row.status_reason.slice(dec.index + dec[0].length)
        .split(' — ')[0].trim();
      decOk = target !== '' && existsSync(join(repoRoot, target));
    }
    if (!/KNOWN-DEFECT|KNOWN-RED|MODEL-LIMIT/.test(row.status_reason) && !decOk) {
      violation('fail-justified', id,
        'FAIL without KNOWN-DEFECT/KNOWN-RED/MODEL-LIMIT/resolved-by-decision:<repo-path> justification');
    }
  }

  if (row.mutant_rejected === 'true' && (row.status !== 'PASS' || row.mutant.trim() === '')) {
    violation('mutant-rejected-consistency', id, 'mutant_rejected=true but status is not PASS or mutant is empty');
  }

  // (e) PASS markers must not read like a red run (mutant rows are exempt —
  // their red marker is the point)
  if (row.status === 'PASS' && row.mutant_rejected !== 'true' &&
      /Undefined Behavior|\bUB\b|\bFAILED\b|panicked|EXIT=[1-9]|exit(?: code)?[ =:]+[1-9]|error\[E\d+|not granting access|weakly protected|access\b[^.;]{0,40}\bforbidden|STATUS_ACCESS_VIOLATION|0xC0000|не запуск|NOT_APPLICABLE|SURVIVED|не пойман|выжил/i
        .test(row.activation + ' ' + row.positive_marker)) {
    violation('pass-forbidden-marker', id, 'PASS activation/marker contains a failure/UB token');
  }

  // (ж) PASS activation/marker must be concrete
  if (row.status === 'PASS') {
    const TRIVIAL = new Set(['-', '—', 'n/a', 'none', 'нет', 'tbd', 'todo', '']);
    const act = row.activation.trim(), pm = row.positive_marker.trim();
    if (/^(cargo (miri )?test|прогон теста( native| targeted)?)$/i.test(act)) {
      violation('activation-generic', id, `activation is a generic command placeholder: "${act}"`);
    } else if (TRIVIAL.has(act.toLowerCase()) || TRIVIAL.has(pm.toLowerCase()) ||
        act.length < 12 || pm.length < 12) {
      violation('activation-trivial', id, 'PASS activation/positive_marker is a placeholder or under 12 characters');
    }
  }

  // A strong row citing a test file that no longer exists is stale evidence:
  // the row must be retired or downgraded, never silently skipped.
  if (isStrong) {
    for (const rel of rowTests(row)) {
      if (testFile(rel) === null) {
        violation('evidence-test-missing', id,
          `${rel} cited by a ${row.status} row does not exist at HEAD`);
      }
    }
  }

  // (a) PASS tests must actually compile under the row's feature set
  if (isVerdict && !cargoFeatures && existsSync(join(repoRoot, 'Cargo.toml'))) {
    violation('cargo-features-unparsable', id,
      'Cargo.toml [features] could not be parsed: test cfg checks would be silently skipped');
  }
  if (isVerdict && cargoFeatures) {
    const feats = rowFeatureSet(row);
    for (const rel of rowTests(row)) {
      const text = testFile(rel);
      if (text === null) continue;
      const stripped = lf(text).split('\n').map((l) => l.replace(/\/\/.*$/, '')).join('\n');
      for (const inner of allInnerCfgs(stripped)) {
        let ok;
        try { ok = evalCfg(inner, feats); } catch (e) {
          violation('cfg-unparsable', id, `${rel}: cannot evaluate #![cfg(${inner.trim()})]: ${e.message}`);
          continue;
        }
        if (ok === false) {
          violation('pass-cfg-unsatisfied', id,
            `${rel} is compiled out under features "${row.features.trim()}" (cfg ${inner.trim()}) is false`);
        }
      }
    }
  }

  // (b) shipping-installed PASS rows must run against an installed allocator
  if (isVerdict && row.scope === 'shipping-installed') {
    const entryTests = rowTests(row);
    if (entryTests.length > 0) {
      if (/HeapCore::|AllocCore::|dbg_|CountSystem/.test(row.entry)) {
        violation('shipping-installed-needs-global-allocator', id,
          'shipping-installed entry references low-level/debug internals (HeapCore::/AllocCore::/dbg_/CountSystem)');
      }
      for (const rel of entryTests) {
        const text = testFile(rel);
        if (text === null) continue;
        const lines = lf(text).split('\n')
          .map((l) => l.replace(/\/\/.*$/, ''));
        const at = lines.findIndex((l) => l.includes('#[global_allocator]'));
        const near = at >= 0 ? lines.slice(at + 1, at + 4).join('\n') : '';
        if (at < 0 || !/\bstatic\b/.test(near) || !/SeferAlloc/.test(near)) {
          violation('shipping-installed-needs-global-allocator', id,
            `${rel} has no #[global_allocator] static SeferAlloc within 3 lines`);
        }
      }
    }
  }

  // receipts: existence, path roots, pins and per-token sha256
  const tokens = row.receipt.split(';').map((t) => t.trim()).filter((t) => t !== '');
  const paths = [];
  if (tokens.length === 0) {
    violation('receipt-exists', id, 'empty receipt');
  } else {
    for (const t of tokens) {
      if (/^external:([0-9a-f]{64})$/i.test(t)) {
        if (isStrong) {
          violation('receipt-external-strong', id,
            `${row.status} row cites an external receipt (${t.slice(0, 16)}…): not verifiable from the repository`);
        }
        continue;
      }
      paths.push(t);
      const abs = join(repoRoot, t);
      if (/^(\/|\\|[A-Za-z]:)/.test(t) || /(^|[\\/])\.\.([\\/]|$)/.test(t)) {
        violation('receipt-roots', id, `receipt path must be relative without "..": ${t}`);
        continue;
      }
      if (!existsSync(abs)) {
        violation('receipt-exists', id, `receipt path does not exist: ${t} (false release token)`);
        continue;
      }
      if (!statSync(abs).isFile()) {
        violation('receipt-roots', id, `receipt path is not a file: ${t}`);
        continue;
      }
      if (!RECEIPT_ROOTS.some((r) => t.replace(/\\/g, '/').startsWith(r))) {
        violation('receipt-roots', id,
          `receipt path must live under ${RECEIPT_ROOTS.join('|')}: ${t}`);
      }
    }
    const isExternal = (t) => /^external:([0-9a-f]{64})$/i.test(t.trim());
    const shaTokens = row.receipt_sha256.split(';').map((t) => t.trim())
      .filter((t) => t !== '' && !isExternal(t));
    if (isStrong && row.receipt_sha256.trim() === '') {
      violation('receipt-pin-required', id, 'strong-status row has no receipt_sha256 pins');
    }
    if (isStrong && row.receipt_sha256.trim() !== '' && shaTokens.length !== paths.length) {
      violation('receipt-pin-required', id,
        `${shaTokens.length} sha pin(s) for ${paths.length} receipt path(s)`);
    }
    if (row.receipt_sha256.trim() !== '' && shaTokens.length === paths.length &&
        paths.length > 0 && paths.every((p) => existsSync(join(repoRoot, p)) &&
          statSync(join(repoRoot, p)).isFile())) {
      shaTokens.forEach((want, i) => {
        const got = sha256Lf(join(repoRoot, paths[i]));
        if (got !== want.toLowerCase()) {
          violation('receipt-sha', id, `receipt_sha256 mismatch for ${paths[i]}`);
        }
      });
    }
  }

  // identity-source-sha (+ strict hardening)
  if (isVerdict) {
    const sha = row.source_sha.trim();
    const emptySha = sha === '' || sha === '—';
    const hexOk = /^[0-9a-f]{8,40}$/i.test(sha);
    let idn = null;
    if (!emptySha && hexOk) idn = identity(sha);
    let reason = null;
    if (emptySha) reason = 'verdict row has empty source_sha';
    else if (!hexOk) reason = `source_sha "${sha}" is not a 40-hex or >=8-hex prefix`;
    else if (!idn.resolved) reason = `source_sha ${sha} does not resolve to a commit`;
    else if (!shallow && idn.ancestor === false) {
      reason = `source_sha resolves to ${idn.resolved} which is not an ancestor of HEAD (unpublished)`;
    }
    if (reason) {
      if (row.status === 'PASS' || row.status === 'KNOWN-DEFECT') {
        identityUnresolved.push(id);
        emit(`IDENTITY-UNRESOLVED: row ${id} ${reason}`);
      } else {
        identityWarnings.push(id);
        emit(`IDENTITY-WARNING: row ${id} ${reason}`);
      }
      const spike = /spike-not-merged/i.test(row.status_reason) && isListedSpike(sha);
      if (opts.strict && (isStrong || !spike)) {
        if (row.status === 'PASS' && emptySha) {
          violation('identity-empty-sha', id,
            'PASS row has empty/— source_sha: no commit identity for the measured run');
        } else {
          violation('identity-unpublished', id,
            `${row.status} row identity not verifiable: ${reason} (strict mode; a never-merged spike must say "spike-not-merged" in status_reason)`);
        }
      }
    } else if (isStrong && idn.ancestor === true) {
      // (и) staleness: the pinned commit must still contain the very test
      // files the entry cites, byte-identical to HEAD.
      for (const rel of rowTests(row)) {
        if (!existsSync(join(repoRoot, rel))) continue; // gone from HEAD: skip
        const old = blobAt(idn.resolved, rel.replace(/\\/g, '/'));
        if (old === null) {
          violation('evidence-identity-base', id,
            `${rel} missing at source_sha ${idn.resolved} (pin predates the test, not the measured tree)`);
          continue;
        }
        if (lf(old) !== lf(testFile(rel))) {
          violation('evidence-stale', id,
            `${rel} differs between source_sha ${idn.resolved} and HEAD`);
        }
      }
    }
  } // non-verdict rows: source_sha optional
}

// (г) required cells
const byId = new Map(rows.map((r) => [r.id, r]));
let requiredOutOfExpect = 0;
for (const req of requiredRows) {
  const row = byId.get(req.cell_id);
  if (!row) {
    requiredMissing.push(req.cell_id);
    emit(`REQUIRED-MISSING: ${req.cell_id} (treated as NOT_RUN)`);
    if (opts.strict) {
      violation('required-missing', req.cell_id,
        'required cell has no registry row (strict mode: missing evidence)');
    }
    continue;
  }
  const expects = new Set(req.expects.split('|').map((s) => s.trim()).filter((s) => s !== ''));
  if (expects.size > 0 && !expects.has(row.status)) {
    requiredOutOfExpect++;
    violation('required-cell-status', req.cell_id, `status "${row.status}" not in expects "${req.expects}"`);
  }
  if (/^true$/i.test(req.requires_mutant.trim())) {
    if (row.mutant.trim() === '') {
      violation('required-cell-mutant', req.cell_id, 'requires_mutant=true but registry row has empty mutant');
    }
    // (з) a PASS behind a mutant-required cell must prove the mutant died
    if (row.status === 'PASS') {
      if (row.mutant_rejected.trim() !== 'true') {
        violation('requires-mutant-rejected', req.cell_id,
          'PASS row with requires_mutant=true but mutant_rejected is not true');
      } else if (/не пойман|выжил|survived/i.test(row.mutant)) {
        violation('requires-mutant-rejected', req.cell_id,
          'mutant result admits the mutant survived');
      }
    }
  }
  // (и) waiver classes must document why no mutant is required
  if (/^false$/i.test(req.requires_mutant.trim()) &&
      ['protocol', 'ordering', 'oom', 'free'].includes(req.class.trim())) {
    const note = req.note.trim();
    if (!/^waiver:/.test(note) || note.slice('waiver:'.length).trim().length <= 10) {
      violation('mutant-waiver-undocumented', req.cell_id,
        `class "${req.class}" with requires_mutant=false needs a "waiver: <reason>" note (>10 chars)`);
    }
  }
  if (PENDING_STATUS.has(row.status)) pendingRequired.push(req.cell_id);
}

// --------------------------------------------------- minimal required ---
// The registry itself dictates the minimal required set: every non-PASS id,
// every mutant-checked id and every Ph6a-owned id must be a required cell.
{
  const minimal = new Set();
  for (const r of rows) {
    if (r.status !== 'PASS' || r.mutant_rejected.trim() === 'true' ||
        r.owner_phase.trim() === 'Ph6a') minimal.add(r.id);
  }
  const have = new Set(requiredRows.map((r) => r.cell_id));
  const missing = [...minimal].filter((cid) => !have.has(cid));
  if (missing.length > 0) {
    violation('required-not-superset', '-',
      `required_cells.csv is not a superset of the registry-derived minimal set; missing: ${missing.join(', ')}`);
  }
}

// ---------------------------------------------------------- baseline ----

// Normalised evidence identity: full commit hash (short forms resolved), the
// sorted receipt path set and the sorted pins. Whitespace/ordering/short-sha
// edits do not count as new evidence.
function evidenceKey(r) {
  const sha = r.source_sha.trim();
  const full = /^[0-9a-f]{8,40}$/i.test(sha) ? (git(['rev-parse', '--verify', '--quiet', `${sha}^{commit}`]) ?? sha.toLowerCase()) : sha;
  const norm = (v) => v.split(';').map((x) => x.trim().toLowerCase()).filter((x) => x !== '').sort().join(';');
  return [full, norm(r.receipt), norm(r.receipt_sha256)].join('|');
}
function newEvidence(base, cur) { return evidenceKey(base) !== evidenceKey(cur); }

function loadBaseline() {
  const ref = opts.baseline;
  const content = git(['show', `${ref}:docs/evidence/registry.csv`]);
  if (content === null) return { error: `git show ${ref}:docs/evidence/registry.csv failed` };
  const parsed = parseCsv(content);
  if (JSON.stringify(parsed.rows[0] ?? []) !== JSON.stringify(REGISTRY_HEADER)) {
    return { error: `baseline registry at ${ref} has an invalid header` };
  }
  if (parsed.unterminated) return { error: `baseline registry at ${ref} ends inside an unterminated quote` };
  const seen = new Set();
  for (const cells of parsed.rows.slice(1)) {
    if (cells.length !== REGISTRY_HEADER.length) {
      return { error: `baseline registry at ${ref} has a row with ${cells.length} columns (${cells[0] ?? '?'})` };
    }
    if (seen.has(cells[0])) return { error: `baseline registry at ${ref} has a duplicate id ${cells[0]}` };
    seen.add(cells[0]);
  }
  return { map: new Map(parsed.rows.slice(1).map((cells) => {
    const r = toRow(REGISTRY_HEADER, cells);
    return [r.id, r];
  })) };
}

if (opts.baseline) {
  const b = loadBaseline();
  if (b.error) {
    emit(`BASELINE: skipped (${b.error})`);
    if (opts.strictBaseline) {
      violation('baseline-unresolved', '-', `${b.error} (--strict-baseline)`);
    }
  } else {
    let retired = new Set();
    const retiredPath = join(repoRoot, 'docs', 'evidence', 'retired_ids.csv');
    if (existsSync(retiredPath)) {
      for (const cells of parseCsv(readFileSync(retiredPath, 'utf8')).rows.slice(1)) {
        if (cells[0]) retired.add(cells[0].trim());
      }
    }
    const baseReq = new Map();
    {
      const bre = git(['show', `${opts.baseline}:docs/evidence/required_cells.csv`]);
      if (bre !== null) {
        const pr = parseCsv(bre).rows;
        for (const cells of pr.slice(1)) baseReq.set(cells[0], toRow(pr[0], cells));
      }
    }
    for (const req of requiredRows) {
      const was = baseReq.get(req.cell_id);
      if (!was) continue;
      const wasSet = new Set(was.expects.split('|').map((x) => x.trim()));
      const nowSet = req.expects.split('|').map((x) => x.trim());
      const gainsPass = !wasSet.has('PASS') && nowSet.includes('PASS');
      const cur = byId.get(req.cell_id);
      const base = b.map.get(req.cell_id);
      if (gainsPass && cur && base && !newEvidence(base, cur)) {
        violation('baseline-required-widened', req.cell_id,
          'required cell newly accepts PASS but its registry row carries no new evidence');
      }
    }
    for (const [id, base] of b.map) {
      const cur = byId.get(id);
      if (!cur) {
        if (!retired.has(id)) {
          violation('baseline-id-removed', id,
            `id present at baseline ${opts.baseline} removed from registry and not in retired_ids.csv`);
        }
        continue;
      }
      if (PROMOTABLE.has(base.status) && cur.status === 'PASS' && !newEvidence(base, cur)) {
        violation('baseline-promotion-without-evidence', id,
          `${base.status} -> PASS at ${opts.baseline}..HEAD without new evidence (normalised source_sha / receipt set / receipt pins unchanged)`);
      }
    }
  }
}

// --------------------------------------------------------- verdict ----

const byStatus = Object.fromEntries(STATUS.map((s) => [s, 0]));
const byPhase = {};
const byScope = {};
for (const r of rows) {
  byStatus[r.status] = (byStatus[r.status] ?? 0) + 1;
  byPhase[r.owner_phase] = (byPhase[r.owner_phase] ?? 0) + 1;
  byScope[r.scope] = (byScope[r.scope] ?? 0) + 1;
}
const unresolvedSet = new Set(identityUnresolved);
const warningSet = new Set(identityWarnings);
const published = rows.filter((r) => VERDICT_STATUS.has(r.status) &&
  !unresolvedSet.has(r.id) && !warningSet.has(r.id)).length;
if (opts.strict && shallow) {
  violation('identity-shallow', '-',
    'repository is shallow: source_sha ancestry vs HEAD cannot be verified, PASS identity is unprovable (use a full clone / fetch-depth 0)');
}

let verdict, exitCode, reasons;
if (violations.length > 0) {
  verdict = 'RED'; exitCode = 1;
  reasons = ['violations: ' + Object.entries(violationsByRule()).map(([r, n]) => `${r}(${n})`).join(', ')];
} else {
  const inc = [];
  if (requiredMissing.length) inc.push(`required cells missing: ${requiredMissing.join(', ')}`);
  if (identityUnresolved.length) inc.push(`unpublished verdict rows: ${identityUnresolved.join(', ')}`);
  if (identityWarnings.length) inc.push(`verdict rows pinned to unpublished spike commits: ${identityWarnings.join(', ')}`);
  if (pendingRequired.length) inc.push(`required cells awaiting final status: ${pendingRequired.join(', ')}`);
  if (inc.length) { verdict = 'INCOMPLETE'; exitCode = 0; reasons = inc; }
  else { verdict = 'GREEN'; exitCode = 0; reasons = ['all checks passed']; }
}

emit(`[evidence-registry] mode: ${opts.strict ? 'strict' : 'plain'}`);
emit(`[evidence-registry] VERDICT: ${verdict}${opts.strict ? ' (strict)' : ''}`);
for (const r of reasons) emit(`[evidence-registry] reason: ${r}`);
emit(`[evidence-registry] exit: ${exitCode}`);

if (opts.json) {
  console.log(JSON.stringify({
    verdict,
    exit: exitCode,
    mode: opts.strict ? 'strict' : 'plain',
    violations,
    identityUnresolved,
    identityWarnings,
    requiredMissing,
    pendingRequired,
    counters: { byStatus, byPhase, byScope, published,
      unpublished: identityUnresolved.length,
      requiredCells: { total: requiredRows.length, missing: requiredMissing.length,
        outOfExpect: requiredOutOfExpect } },
  }, null, 2));
} else {
  emit(`[evidence-registry] rows by status: ` +
    STATUS.map((s) => `${s}=${byStatus[s]}`).join(' '));
  emit(`[evidence-registry] rows by owner_phase: ` +
    Object.entries(byPhase).map(([k, v]) => `${k}=${v}`).join(' '));
  emit(`[evidence-registry] rows by scope: ` +
    Object.entries(byScope).map(([k, v]) => `${k}=${v}`).join(' '));
  emit(`[evidence-registry] verdict rows published=${published} unpublished=${identityUnresolved.length}`);
  emit(`[evidence-registry] required cells: total=${requiredRows.length} missing=${requiredMissing.length} out-of-expect=${requiredOutOfExpect}`);
}
process.exit(exitCode);
