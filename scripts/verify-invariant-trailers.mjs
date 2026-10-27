// Invariant-Check trailer lint (docs/GHOST_CHECKS_PLAN.md §5). For every
// non-merge commit in a range, the rules of docs/verification/ghost-rules.json
// the commit TOUCHES — its changed lines intersect a protected span
// (`ghost-lint spans`, computed on the parent for removed lines and on the commit
// for added ones), its catalog entry changed, or its fixture case changed —
// must each have an `Invariant-Check: <rule-id>` block in the message:
//
//   Invariant-Check: route-pin/last-ref-free
//     Change: fetch_sub moved into EntryHandle::release
//     Why: the sidecar is still freed only in the fetch_sub(1, AcqRel) == 1 branch ...
//     Covered-by: tests/loom_route_pin_unlink.rs, ghost:route-pin/last-ref-free,
//       fixture:tests/ghost_lint_fixtures/route_pin_last_ref/mutant_free_before_check.rs
//
// Checks: a block per touched rule (extra blocks for untouched rules warn; an
// unknown id is red); `Change` non-empty; `Why` at least WHY_MIN characters, not
// a copy of another block's Why, naming at least min(2, n) of the rule's `terms`;
// every `Covered-by` item exists in the commit's tree (`ghost:<id>` — a
// ghost_assert!/ghost_event! with that id; `fixture:<path>` / `<path>` — a file),
// at least one is a test file; when the catalog entry's parameters changed, the
// commit also changes one of the Covered-by files. The script proves the
// justification is present, bound to the right rule, phrased in the invariant's
// terms and pointing at real evidence; the evidence itself (loom/miri/kani,
// mutants) is what CI runs.
//
//   node scripts/verify-invariant-trailers.mjs [<base>..<head>]   (default origin/main..HEAD)
//   node scripts/verify-invariant-trailers.mjs --self-test
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

export const WHY_MIN = 120;
const VERBOSE = process.argv.includes('--verbose');
const CATALOG = 'docs/verification/ghost-rules.json';
const FIXTURES = 'tests/ghost_lint_fixtures/';
const PARAM_SKIP = new Set(['reason', 'terms', 'fixtures', 'protocol']);

// --- parsing --------------------------------------------------------------

/** Invariant-Check blocks of a commit message: [{ id, change, why, covered: [..] }]. */
export function parseTrailers(message) {
  const out = [];
  let cur = null, field = null;
  for (const raw of message.split(/\r?\n/)) {
    const head = /^Invariant-Check:\s*(\S+)\s*$/.exec(raw);
    if (head) { cur = { id: head[1], change: '', why: '', covered: '' }; out.push(cur); field = null; continue; }
    if (!cur) continue;
    if (!/^\s/.test(raw) || raw.trim() === '') { if (raw.trim() !== '') { cur = null; field = null; } continue; }
    const f = /^\s+(Change|Why|Covered-by):\s*(.*)$/.exec(raw);
    if (f) { field = { Change: 'change', Why: 'why', 'Covered-by': 'covered' }[f[1]]; cur[field] = f[2].trim(); continue; }
    if (field) cur[field] = `${cur[field]} ${raw.trim()}`.trim();
  }
  for (const t of out) t.covered = t.covered.split(',').map((s) => s.trim()).filter(Boolean);
  return out;
}

/** New-side and old-side changed line ranges per file from `git diff -U0` text. */
export function parseHunks(diff) {
  const files = new Map();
  let cur = null;
  for (const line of diff.split('\n')) {
    const f = /^diff --git a\/(.+) b\/(.+)$/.exec(line);
    if (f) { cur = { old: f[1], new: f[2], added: [], removed: [] }; files.set(f[2], cur); continue; }
    const h = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(line);
    if (h && cur) {
      const [os, oc, ns, nc] = [Number(h[1]), h[2] === undefined ? 1 : Number(h[2]), Number(h[3]), h[4] === undefined ? 1 : Number(h[4])];
      if (oc > 0) cur.removed.push([os, os + oc - 1]);
      // A pure deletion still touches the place it happened at on the new side.
      cur.added.push(nc > 0 ? [ns, ns + nc - 1] : [ns, ns + 1]);
    }
  }
  return files;
}

const overlaps = (ranges, spans) => ranges.some(([a, b]) => spans.some(([c, d]) => a <= d && c <= b));

// --- the per-commit verdict (pure; the self-test drives it directly) -------

/**
 * @param c.message      commit message
 * @param c.touched      Map ruleId -> reason the rule is touched
 * @param c.rules        Map ruleId -> catalog rule at the commit (or parent, if removed)
 * @param c.exists       (path) => bool, in the commit's tree
 * @param c.ghostIds     Set of ghost ids used by ghost macros in the commit's tree
 * @param c.changed      Set of paths the commit changes
 * @param c.paramsChanged Set of ruleIds whose catalog parameters changed
 */
export function checkCommit(c) {
  const errors = [], warnings = [];
  const blocks = parseTrailers(c.message);
  const byId = new Map();
  for (const b of blocks) {
    if (byId.has(b.id)) errors.push(`duplicate Invariant-Check for ${b.id}`);
    byId.set(b.id, b);
    if (!c.rules.has(b.id)) errors.push(`Invariant-Check names unknown rule ${b.id}`);
    else if (!c.touched.has(b.id)) warnings.push(`Invariant-Check for ${b.id}, which this commit does not touch`);
  }
  const whys = blocks.map((b) => b.why.replace(/\s+/g, ' ').trim().toLowerCase());
  for (const [id, why] of c.touched) {
    const b = byId.get(id);
    if (!b) { errors.push(`missing Invariant-Check for ${id} (touched: ${why})`); continue; }
    const rule = c.rules.get(id) || {};
    if (!b.change) errors.push(`${id}: empty Change`);
    const w = b.why.replace(/\s+/g, ' ').trim();
    if (w.length < WHY_MIN) errors.push(`${id}: Why is ${w.length} characters, at least ${WHY_MIN} required`);
    if (whys.filter((x) => x === w.toLowerCase()).length > 1) errors.push(`${id}: Why is a copy of another block's Why`);
    const terms = rule.terms || [];
    if (terms.length === 0) errors.push(`${id}: catalog rule has no terms`);
    const hit = terms.filter((t) => w.toLowerCase().includes(String(t).toLowerCase()));
    const need = Math.min(2, terms.length);
    if (terms.length && hit.length < need) errors.push(`${id}: Why names ${hit.length} of the rule's terms (${terms.join(', ')}); at least ${need} required`);
    if (b.covered.length === 0) errors.push(`${id}: empty Covered-by`);
    let tests = 0;
    for (const item of b.covered) {
      if (item.startsWith('ghost:')) {
        if (!c.ghostIds.has(item.slice(6))) errors.push(`${id}: Covered-by ${item}: no ghost_assert!/ghost_event! with that id in the tree`);
        continue;
      }
      const path = item.replace(/^fixture:/, '');
      if (!c.exists(path)) { errors.push(`${id}: Covered-by ${item}: not in the commit's tree`); continue; }
      if (/^(tests|crates\/[^/]+\/tests)\//.test(path)) tests++;
    }
    if (b.covered.length && tests === 0) errors.push(`${id}: Covered-by names no test or fixture file`);
    if (c.paramsChanged.has(id)) {
      const evidenceChanged = b.covered.some((it) => !it.startsWith('ghost:') && [...c.changed].some((p) => p === it.replace(/^fixture:/, '') || p.startsWith(it.replace(/^fixture:/, '').replace(/\/?$/, '/'))));
      if (!evidenceChanged) errors.push(`${id}: rule parameters changed but no Covered-by model/harness/fixture changed in the same commit`);
    }
  }
  return { errors, warnings };
}

// --- git plumbing ----------------------------------------------------------

const git = (args, input) => execFileSync('git', args, { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 1 << 28, input });
const gitOk = (args) => spawnSync('git', args, { cwd: REPO_ROOT }).status === 0;
const show = (rev, path) => {
  const r = spawnSync('git', ['show', `${rev}:${path}`], { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 1 << 28 });
  return r.status === 0 ? r.stdout : null;
};

function lintBinary() {
  const env = process.env.GHOST_LINT;
  if (env) return env;
  for (const p of ['target/debug/ghost-lint', 'target/debug/ghost-lint.exe', 'target/release/ghost-lint']) if (existsSync(join(REPO_ROOT, p))) return join(REPO_ROOT, p);
  throw new Error('ghost-lint binary not found: cargo build -p ghost-lint (or set GHOST_LINT)');
}

function spansAt(bin, catalogFile, file, text) {
  const r = spawnSync(bin, ['spans', '--root', REPO_ROOT, '--catalog', catalogFile, '--file', file, '--stdin'], { input: text, encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`ghost-lint spans ${file}: ${r.stderr}`);
  const m = new Map();
  for (const line of r.stdout.split('\n').filter(Boolean)) {
    const [id, a, b] = line.trim().split(/\s+/);
    if (!m.has(id)) m.set(id, []);
    m.get(id).push([Number(a), Number(b)]);
  }
  return m;
}

const ruleMap = (text) => new Map(((text ? JSON.parse(text) : {}).rules || []).map((r) => [r.id, r]));
const params = (r) => JSON.stringify(Object.fromEntries(Object.entries(r || {}).filter(([k]) => !PARAM_SKIP.has(k)).sort()));

function ghostIdsAt(rev) {
  // Whole-file match: rustfmt puts the id on the line after `ghost_assert!(`.
  const ids = new Set();
  let files = '';
  try { files = git(['grep', '-l', '-E', 'ghost_(assert|event)!', rev, '--', 'src', 'crates']); } catch { /* no match */ }
  for (const f of files.split('\n').filter(Boolean)) {
    for (const id of ghostIdsIn(show(rev, f.slice(rev.length + 1)) || '')) ids.add(id);
  }
  return ids;
}

const ghostIdsIn = (text) => [...text.matchAll(/ghost_(?:assert|event)!\(\s*"([^"]+)"/g)].map((m) => m[1]);

function fixtureOwner(path, rulesNow) {
  const m = /^tests\/ghost_lint_fixtures\/([^/]+)\//.exec(path);
  if (!m) return null;
  const cfg = show('HEAD', `${FIXTURES}${m[1]}/rule.json`) || '{}';
  try { const j = JSON.parse(cfg); return j.catalog || (rulesNow.has(j.id) ? j.id : null); } catch { return null; }
}

function analyse(sha, bin, work) {
  const parent = `${sha}^`;
  const message = git(['log', '-1', '--format=%B', sha]);
  const catNow = show(sha, CATALOG), catPrev = show(parent, CATALOG);
  const rulesNow = ruleMap(catNow), rulesPrev = ruleMap(catPrev);
  const rules = new Map([...rulesPrev, ...rulesNow]);
  const touched = new Map(), paramsChanged = new Set();
  for (const id of new Set([...rulesNow.keys(), ...rulesPrev.keys()])) {
    const a = rulesPrev.get(id), b = rulesNow.get(id);
    if (JSON.stringify(a) !== JSON.stringify(b)) touched.set(id, !a ? 'rule added' : !b ? 'rule removed' : 'catalog entry changed');
    if (a && b && params(a) !== params(b)) paramsChanged.add(id);
  }
  const hunks = parseHunks(git(['diff', '-U0', '--no-color', '--no-renames', parent, sha]));
  const changed = new Set(hunks.keys());
  const catFileNow = join(work, `cat-now-${sha}.json`), catFilePrev = join(work, `cat-prev-${sha}.json`);
  writeFileSync(catFileNow, catNow || '{"rules":[]}');
  writeFileSync(catFilePrev, catPrev || '{"rules":[]}');
  for (const [file, h] of hunks) {
    const owner = fixtureOwner(file, rulesNow);
    if (owner) touched.set(owner, touched.get(owner) || `fixture ${file} changed`);
    if (!file.endsWith('.rs')) continue;
    const now = show(sha, file), prev = show(parent, file);
    if (now !== null) for (const [id, sp] of spansAt(bin, catFileNow, file, now)) if (overlaps(h.added, sp)) touched.set(id, touched.get(id) || `${file} lines in its protected span`);
    if (prev !== null) for (const [id, sp] of spansAt(bin, catFilePrev, h.old, prev)) if (overlaps(h.removed, sp)) touched.set(id, touched.get(id) || `${h.old} removed lines in its protected span`);
  }
  if (VERBOSE) for (const [id, why] of touched) console.log(`TOUCH ${sha.slice(0, 8)} ${id}: ${why}`);
  return checkCommit({
    message, touched, rules, changed, paramsChanged,
    exists: (p) => gitOk(['cat-file', '-e', `${sha}:${p}`]),
    ghostIds: ghostIdsAt(sha),
  });
}

// --- self-test: the linter rejects each of its mutants ----------------------

const OK_WHY = 'the sidecar is still freed only in the branch where fetch_sub(1, AcqRel) returns 1, i.e. by the last reference; no other path frees it and the AcqRel ordering is unchanged';
const RULE = { id: 'route-pin/last-ref-free', kind: 'R4', terms: ['fetch_sub', 'AcqRel', 'last reference'] };
const base = () => ({
  touched: new Map([[RULE.id, 'src/x.rs lines in its protected span']]),
  rules: new Map([[RULE.id, RULE]]),
  exists: (p) => ['tests/loom_route_pin_unlink.rs', 'tests/ghost_lint_fixtures/route_pin/mutant_free_before_check.rs'].includes(p),
  ghostIds: new Set([RULE.id]),
  changed: new Set(['src/x.rs']),
  paramsChanged: new Set(),
});
const msg = (why = OK_WHY, extra = '', id = RULE.id, covered = 'tests/loom_route_pin_unlink.rs, ghost:route-pin/last-ref-free,\n    fixture:tests/ghost_lint_fixtures/route_pin/mutant_free_before_check.rs') =>
  `fix(perf): move fetch_sub into release\n\nbody\n\nInvariant-Check: ${id}\n  Change: fetch_sub moved into EntryHandle::release\n  Why: ${why}\n  Covered-by: ${covered}\n${extra}`;

export const SELF_TEST = [
  ['ok', () => ({ ...base(), message: msg() }), null],
  ['no trailer', () => ({ ...base(), message: 'fix: x\n\nbody\n' }), /missing Invariant-Check/],
  ['foreign rule id', () => ({ ...base(), message: msg(OK_WHY, '', 'other/rule') }), /unknown rule other\/rule/],
  ['nonexistent test', () => ({ ...base(), message: msg(OK_WHY, '', RULE.id, 'tests/loom_does_not_exist.rs, ghost:route-pin/last-ref-free') }), /not in the commit's tree/],
  ['unknown ghost id', () => ({ ...base(), message: msg(OK_WHY, '', RULE.id, 'tests/loom_route_pin_unlink.rs, ghost:route-pin/nope') }), /no ghost_assert/],
  ['Why without terms', () => ({ ...base(), message: msg('this change is fine because it was carefully reviewed and all tests pass and nothing else was changed in this commit at all ok') }), /names 0 of the rule's terms/],
  ['short Why', () => ({ ...base(), message: msg('fetch_sub AcqRel fine') }), /at least 120 required/],
  ['copied Why', () => ({ ...base(), touched: new Map([[RULE.id, 'x'], ['b/rule', 'y']]), rules: new Map([[RULE.id, RULE], ['b/rule', { ...RULE, id: 'b/rule' }]]), message: msg() + `Invariant-Check: b/rule\n  Change: y\n  Why: ${OK_WHY}\n  Covered-by: tests/loom_route_pin_unlink.rs\n` }), /copy of another/],
  ['no test in Covered-by', () => ({ ...base(), message: msg(OK_WHY, '', RULE.id, 'ghost:route-pin/last-ref-free') }), /names no test or fixture/],
  ['params changed, evidence not', () => ({ ...base(), paramsChanged: new Set([RULE.id]), message: msg() }), /parameters changed/],
  ['params changed with model', () => ({ ...base(), paramsChanged: new Set([RULE.id]), changed: new Set(['src/x.rs', 'tests/loom_route_pin_unlink.rs']), message: msg() }), null],
  ['empty Change', () => ({ ...base(), message: msg().replace('  Change: fetch_sub moved into EntryHandle::release\n', '  Change:\n') }), /empty Change/],
];

function selfTest() {
  let ok = 0;
  for (const [name, mk, want] of SELF_TEST) {
    const { errors } = checkCommit(mk());
    if (want === null) assert.deepEqual(errors, [], `self-test "${name}": expected clean, got ${errors.join('; ')}`);
    else assert.ok(errors.some((e) => want.test(e)), `self-test "${name}": expected ${want}, got ${JSON.stringify(errors)}`);
    ok++;
  }
  // parser fixtures
  const t = parseTrailers('s\n\nInvariant-Check: a/b\n  Change: c\n  Why: w1\n    w2\n  Covered-by: x,\n    y\nSigned-off-by: z\n');
  assert.deepEqual(t, [{ id: 'a/b', change: 'c', why: 'w1 w2', covered: ['x', 'y'] }], 'trailer parser');
  const h = parseHunks('diff --git a/f.rs b/f.rs\n@@ -3,2 +3,0 @@\n@@ -10 +9,3 @@\n');
  assert.deepEqual(h.get('f.rs').removed, [[3, 4], [10, 10]], 'hunk removed ranges');
  assert.deepEqual(h.get('f.rs').added, [[3, 4], [9, 11]], 'hunk added ranges');
  assert.deepEqual(ghostIdsIn('x::ghost_assert!(\n    "a/b",\n    c);\nghost_event!("c/d", k, 1);'), ['a/b', 'c/d'], 'ghost id parser (rustfmt-wrapped call)');
  return ok + 3;
}

function main() {
  const argv = process.argv.slice(2);
  const n = selfTest();
  console.log(`[verify-invariant-trailers] self-test OK (${n} cases: ${SELF_TEST.length - 2} linter mutants rejected, 2 clean, 3 parsers)`);
  if (argv.includes('--self-test')) return;
  const range = argv.find((a) => a.includes('..')) || 'origin/main..HEAD';
  const shas = git(['rev-list', '--no-merges', '--reverse', range]).split('\n').filter(Boolean);
  const bin = lintBinary();
  const work = mkdtempSync(join(tmpdir(), 'inv-trailers-'));
  let red = 0;
  try {
    for (const sha of shas) {
      const { errors, warnings } = analyse(sha, bin, work);
      const subj = git(['log', '-1', '--format=%s', sha]).trim();
      for (const w of warnings) console.log(`WARN ${sha.slice(0, 8)} ${w}`);
      if (errors.length) { red++; for (const e of errors) console.log(`RED  ${sha.slice(0, 8)} ${subj}: ${e}`); }
    }
  } finally {
    rmSync(work, { recursive: true, force: true });
  }
  console.log(`[verify-invariant-trailers] ${range}: ${shas.length} commit(s), ${red} red`);
  if (red) process.exitCode = 1;
}

if (process.argv[1] && process.argv[1].replace(/\\/g, '/').endsWith('verify-invariant-trailers.mjs')) main();
