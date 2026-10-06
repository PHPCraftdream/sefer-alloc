// Regression self-test for the evidence-registry judge
// (scripts/verify-evidence-registry.mjs), plan step 7 (#2098/Ph7).
//
// The judge guards docs/evidence/registry.csv against a catalogue of failure
// modes. This script keeps THE JUDGE honest by running it, as a child
// process, against known-bad mutations of known-good fixtures and asserting
// the exact exit code plus the exact output marker for each rule.
//
// Case groups:
//   B1 — mutations over a synthetic fixture registry (tmp dir, always
//        removed). The fixture receipt is scripts/verify-evidence-registry.mjs
//        itself: a stable, always-present repo file (sha256 computed at
//        runtime over LF-normalized content). The green PASS control pins
//        source_sha to current HEAD.
//   B2 — mutations over a COPY of the real docs/evidence/registry.csv.
//   B2b — baseline-ratchet cases in a THROWAWAY git repository created in
//        os.tmpdir() (git init + two commits with -c user.name/-c user.email,
//        removed in finally). This is the only git writing this script does.
//
// Identity rules assume a FULL clone (source_sha ancestry vs HEAD via
// merge-base). The selftest is only run where that holds: the local
// check-all pipeline and the CI job evidence-registry, which uses
// fetch-depth: 0. The `identity-shallow` rule itself cannot be exercised
// here (it needs a shallow clone) — it is covered only by the CI
// configuration inspector (fetch-depth: 0 on that job).
//
// Usage: node scripts/evidence-registry-selftest.mjs
// Prints a `case | expected | observed | ok` table and, on success,
//   EVIDENCE-REGISTRY-SELFTEST: ALL PASS (N cases)   (exit 0)
// or lists the failed cases and exits 1.

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { tmpdir } from 'node:os';

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, '..');
const judge = join(scriptDir, 'verify-evidence-registry.mjs');

function die(msg) { console.error(`selftest setup error: ${msg}`); process.exit(2); }
function gitOut(args, cwd = repoRoot) {
  const r = spawnSync('git', args, { encoding: 'utf8', cwd });
  if (r.status !== 0) die(`git ${args.join(' ')} failed: ${r.stderr}`);
  return r.stdout.trim();
}

// ------------------------------------------------------------ fixtures ---

const HEADER = [
  'id', 'transition', 'entry', 'features', 'os', 'toolchain', 'profile',
  'source_sha', 'source_tree', 'binary_id', 'activation', 'positive_marker',
  'mutant', 'mutant_rejected', 'receipt', 'receipt_sha256', 'scope', 'status',
  'status_reason', 'owner_phase', 'source_cell',
];
const REQUIRED_HEADER = ['cell_id', 'class', 'gate', 'expects', 'requires_mutant', 'note'];

// Stable, always-present receipt fixture: the judge script itself. NOT README.
const JUDGE_REL = 'scripts/verify-evidence-registry.mjs';
if (!existsSync(join(repoRoot, JUDGE_REL))) die(`${JUDGE_REL} not found`);
const lf = (s) => s.replace(/\r\n?/g, '\n');
const RECEIPT_SHA = createHash('sha256')
  .update(lf(readFileSync(join(repoRoot, JUDGE_REL), 'utf8'))).digest('hex');

const HEAD_SHA = gitOut(['rev-parse', 'HEAD']);
const UNPUBLISHED_SHA = 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef';
const SPIKE_SHA = '8a0286166a07c3ff35258836400d94f16c7994cd'; // known non-ancestor

// Ancestor commit where tests/global_alloc_installed.rs DIFFERS from HEAD
// (the SeferMalloc->SeferAlloc rename), and the commit BEFORE the test file
// was added at all. Resolved at runtime; never hardcoded.
const testLog = gitOut(['log', '--format=%H', '--', 'tests/global_alloc_installed.rs'])
  .split('\n').filter(Boolean);
if (testLog.length < 2) die('need >=2 commits touching tests/global_alloc_installed.rs');
// evidence-stale pin: the OLDEST commit that still contains the file but with
// content differing from HEAD (here: the pre-rename version).
function blobAt(sha, rel) {
  const r = spawnSync('git', ['show', `${sha}:${rel}`],
    { encoding: 'utf8', cwd: repoRoot, stdio: ['ignore', 'pipe', 'pipe'] });
  return r.status === 0 ? r.stdout : null;
}
const STALE_SHA = [...testLog].reverse()
  .find((s) => { const b = blobAt(s, 'tests/global_alloc_installed.rs');
    return b !== null && lf(b) !== lf(readFileSync(join(repoRoot,
      'tests/global_alloc_installed.rs'), 'utf8')); });
if (!STALE_SHA) die('no ancestor commit with a differing tests/global_alloc_installed.rs');
const addCommit = gitOut(['log', '--format=%H', '--diff-filter=A', '--',
  'tests/global_alloc_installed.rs']);
const BASE_SHA = gitOut(['rev-parse', `${addCommit}^`]);             // file absent

// Green fixture PASS row: entry cites the real test file (literal tests/*.rs
// path — the judge extracts test files by that pattern) that installs
// `#[global_allocator] static SeferAlloc` (cfg feature = "alloc-global").
const GREEN_ENTRY = 'tests/global_alloc_installed.rs; cargo test --features alloc-global';
function passRow(overrides = {}) {
  return {
    id: 'E-pass',
    transition: 'model A -> model B (fixture)',
    entry: GREEN_ENTRY,
    features: 'alloc-global',
    os: 'linux',
    toolchain: 'rustc stable',
    profile: 'dev',
    source_sha: HEAD_SHA,
    source_tree: 'tree fixture',
    binary_id: 'fixture-bin',
    activation: GREEN_ENTRY,
    positive_marker: 'test result: ok. 3 passed; 0 failed',
    mutant: 'W-1 write-dye mutant KILLED',
    mutant_rejected: 'true',
    receipt: JUDGE_REL,
    receipt_sha256: RECEIPT_SHA,
    scope: 'shipping-installed',
    status: 'PASS',
    status_reason: 'full run, marker observed',
    owner_phase: 'Ph7',
    source_cell: 'fixture:E-pass',
    ...overrides,
  };
}
const KD_REASON = 'src/lib.rs:1 Node::write_next ← caller';
function kdRow(overrides = {}) {
  return passRow({
    id: 'E-kd',
    status: 'KNOWN-DEFECT',
    activation: '',
    positive_marker: '',
    mutant: '',
    mutant_rejected: '',
    status_reason: KD_REASON,
    source_cell: 'fixture:E-kd',
    ...overrides,
  });
}

function reqRow(cell, overrides = {}) {
  return { cell_id: cell, class: 'fixture', gate: 'C1',
    expects: cell === 'E-kd' ? 'KNOWN-DEFECT' : 'PASS',
    requires_mutant: cell === 'E-pass' ? 'true' : 'false', note: '', ...overrides };
}
const baseRegistry = (extra = []) => [passRow(), kdRow(), ...extra];
const baseRequired = (extra = []) => [reqRow('E-pass'), reqRow('E-kd'), ...extra];

function csvField(v) {
  if (v && typeof v === 'object' && 'raw' in v) return v.raw; // verbatim injection
  const s = String(v);
  return /[",\n\r]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
}
function toCsv(header, rows) {
  return [header, ...rows].map((r) => r.map(csvField).join(',')).join('\n') + '\n';
}
const objCells = (obj) => HEADER.map((h) => obj[h] ?? '');
function writeFixture(dir, registryRows, requiredRows, { crlf = false, postReg = null,
  requiredHeader = REQUIRED_HEADER } = {}) {
  let reg = toCsv(HEADER, registryRows.map((r) => HEADER.map((h) => r[h])));
  if (postReg) reg = postReg(reg);
  let req = toCsv(requiredHeader, requiredRows.map((r) => requiredHeader.map((h, i) =>
    i >= REQUIRED_HEADER.length ? 'extra' : r[h])));
  if (crlf) { reg = reg.replace(/\n/g, '\r\n'); req = req.replace(/\n/g, '\r\n'); }
  writeFileSync(join(dir, 'registry.csv'), reg);
  writeFileSync(join(dir, 'required_cells.csv'), req);
}

// Minimal CSV parser mirroring the judge's (for mutating the real registry).
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
  return rows.filter((r) => !(r.length === 1 && r[0].trim() === ''));
}

// ------------------------------------------------- real-registry cases ---

const realRegistryText = readFileSync(join(repoRoot, 'docs', 'evidence', 'registry.csv'), 'utf8');
const realRequiredText = readFileSync(join(repoRoot, 'docs', 'evidence', 'required_cells.csv'), 'utf8');
const realRows = parseCsv(realRegistryText);
const HDR = realRows[0];
const col = (name) => HDR.indexOf(name);

// Mutate a copy of the real registry: `mut` maps id -> cell patches (by
// column name); `replace` maps id -> full row (array of 21 cells).
function realFixture(dir, { mut = {}, replace = {}, required = null } = {}) {
  const rows = realRows.map((cells) => {
    const patches = mut[cells[0]];
    if (!patches) return cells;
    const out = [...cells];
    for (const [k, v] of Object.entries(patches)) out[col(k)] = v;
    return out;
  }).map((cells) => (replace[cells[0]] ? replace[cells[0]] : cells));
  writeFileSync(join(dir, 'registry.csv'), toCsv(HDR, rows.slice(1)));
  writeFileSync(join(dir, 'required_cells.csv'), required ?? realRequiredText);
}

// Rewrites R6a-C4 (KNOWN-DEFECT) into a fully valid published PASS row:
// every rule stays green except required-cell-status (expects=KNOWN-DEFECT).
function validPassCells(id) {
  return toRow(HEADER, passRow({
    id, owner_phase: 'Ph6a', source_cell: `draft:${id}`,
    positive_marker: 'test result: ok. 1 passed; 0 failed',
  }));
}
function toRow(header, obj) { return header.map((h) => obj[h] ?? ''); }

// ------------------------------------------------------------ run -------

function runJudge(args) {
  const r = spawnSync(process.execPath, [judge, ...args], { encoding: 'utf8' });
  return { code: r.status, stdout: r.stdout ?? '', stderr: r.stderr ?? '' };
}
const markerHit = (out, prefix) => out.split('\n').some((l) => l.startsWith(prefix));
const redLines = (out) => out.split('\n').filter((l) => l.startsWith('RED:'));

function judgeArgs(dir, { root = repoRoot, extra = [], reg = join(dir, 'registry.csv'),
  req = join(dir, 'required_cells.csv') } = {}) {
  return ['--registry', reg, '--required', req, '--repo-root', root, ...extra];
}

// ------------------------------------------------------- ratchet repo ---

function runGit(args, cwd) {
  const r = spawnSync('git', args, { encoding: 'utf8', cwd });
  if (r.status !== 0) throw new Error(`git ${args.join(' ')}: ${r.stderr}`);
  return r.stdout.trim();
}
const GIT_ID = ['-c', 'user.name=x', '-c', 'user.email=x@x'];

function sha256FileLf(path) {
  return createHash('sha256').update(lf(readFileSync(path, 'utf8'))).digest('hex');
}

// Two-commit tmp git repo: commit1 = full fixture, commit2 = registry
// without row B. Returns { root, c1, reg, req }.
function makeRatchetRepo(dir, { commit2 = true, bExpects = 'NOT_RUN|PASS' } = {}) {
  runGit(['init', '-b', 'main'], dir);
  const docsEv = join(dir, 'docs', 'evidence');
  const reviews = join(dir, 'docs', 'reviews');
  mkdirSync(docsEv, { recursive: true });
  mkdirSync(reviews, { recursive: true });
  writeFileSync(join(reviews, 'fixture.md'), 'fixture receipt\n');
  const pin = sha256FileLf(join(reviews, 'fixture.md'));
  const rowA = passRow({
    id: 'A', entry: 'fixture_entry', features: 'fixture', scope: 'shipping-installed',
    transition: 'fixture transition A',
    activation: 'SEFER_FIXTURE=1 fixture run A witness',
    positive_marker: 'fixture log: ok. 1 passed',
    mutant: 'W-A mutant killed', mutant_rejected: 'true',
    receipt: 'docs/reviews/fixture.md', receipt_sha256: pin,
    source_sha: '', owner_phase: 'Ph0', source_cell: 'fixture:A',
  });
  const rowB = passRow({
    id: 'B', status: 'NOT_RUN', entry: 'fixture_entry', features: 'fixture',
    transition: 'fixture transition B', scope: 'shipping-installed',
    activation: 'SEFER_FIXTURE=1 fixture run B witness',
    positive_marker: 'fixture log: ok for B cell',
    mutant: '', mutant_rejected: '', source_sha: '',
    receipt: 'docs/reviews/fixture.md', receipt_sha256: pin,
    owner_phase: 'Ph0', source_cell: 'fixture:B',
  });
  const req = [reqRow('A'), reqRow('B', { expects: bExpects })];
  const reg = toCsv(HEADER, [rowA, rowB].map(objCells));
  writeFileSync(join(docsEv, 'registry.csv'), reg);
  writeFileSync(join(docsEv, 'required_cells.csv'),
    toCsv(REQUIRED_HEADER, req.map((r) => REQUIRED_HEADER.map((h) => r[h]))));
  runGit(['add', '.'], dir);
  runGit([...GIT_ID, 'commit', '-m', 'fixture commit 1'], dir);
  const c1 = runGit(['rev-parse', 'HEAD'], dir);
  // Rows pin the baseline commit, known only after commit 1: rewrite + commit.
  const regPinned = toCsv(HEADER, [
    { ...rowA, source_sha: c1 }, { ...rowB, source_sha: c1 },
  ].map(objCells));
  if (regPinned !== reg) {
    writeFileSync(join(docsEv, 'registry.csv'), regPinned);
    runGit(['add', '.'], dir);
    runGit([...GIT_ID, 'commit', '-m', 'fixture commit 1 (pinned)'], dir);
  }
  const c1Final = runGit(['rev-parse', 'HEAD'], dir);
  if (commit2) {
    writeFileSync(join(docsEv, 'registry.csv'),
      toCsv(HEADER, [{ ...rowA, source_sha: c1 }].map(objCells)));
    runGit(['add', '.'], dir);
    runGit([...GIT_ID, 'commit', '-m', 'fixture commit 2: drop B'], dir);
  }
  return {
    root: dir, c1: c1Final,
    reg: join(docsEv, 'registry.csv'), req: join(docsEv, 'required_cells.csv'),
    rowA: { ...rowA, source_sha: c1 }, rowB: { ...rowB, source_sha: c1 },
  };
}

// ------------------------------------------------------------- cases ---

const cases = [];
const fx = (name, registryRows, requiredRows, opts, expect) =>
  cases.push({ name, kind: 'fixture', registryRows, requiredRows, opts: opts ?? {}, ...expect });
const rl = (name, mutation, expect) =>
  cases.push({ name, kind: 'real', mutation, ...expect });

// --- B1: green controls ---
fx('green-control', baseRegistry(), baseRequired(), {},
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });
fx('green-control-strict', baseRegistry(), baseRequired(), { extra: ['--strict'] },
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });
fx('green-control-crlf', baseRegistry(), baseRequired(), { crlf: true },
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });

// --- B1: csv structure ---
fx('csv-unterminated-quote',
  baseRegistry().map((r) => r.id === 'E-pass'
    ? { ...r, status_reason: { raw: '"full run, marker observed (unclosed' } } : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: csv-unterminated-quote' });
fx('csv-column-count', baseRegistry(), baseRequired(),
  { postReg: (t) => t.replace(/^E-pass,.*$/m, (m) => m + ',extra22') },
  { expectExit: 1, expectMarker: 'RED: csv-column-count row E-pass' });

// --- B1: id / required schema ---
fx('id-format-trailing-space',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ id: 'E-pass ' }) : r),
  baseRequired(), {}, { expectExit: 1, expectMarker: 'RED: id-format row E-pass' });
fx('id-format-inner-space',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ id: 'E pass' }) : r),
  baseRequired(), {}, { expectExit: 1, expectMarker: 'RED: id-format row E pass' });
fx('dup-id', baseRegistry([passRow()]), baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: dup-id row E-pass:' });

// --- restored original mutants: every registry-forgery class must go red ---
const editPass = (patch) => baseRegistry().map((r) => r.id === 'E-pass' ? passRow(patch) : r);
fx('green-by-cfg-exclusion',
  editPass({ activation: 'run with cfg-empty feature set', positive_marker: 'test result: ok. 0 passed; 0 failed' }),
  baseRequired(), {}, { expectExit: 1, expectMarker: 'RED: pass-marker-cfg-empty row E-pass:' });
fx('green-by-cfg-exclusion-ru',
  editPass({ positive_marker: 'cfg-пустой прогон: 0 тестов запущено, тест не собран' }),
  baseRequired(), {}, { expectExit: 1, expectMarker: 'RED: pass-marker-cfg-empty row E-pass:' });
fx('no-activation', editPass({ activation: '' }), baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: pass-requires-activation row E-pass:' });
fx('no-positive-marker', editPass({ positive_marker: '' }), baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: pass-requires-marker row E-pass:' });
fx('unknown-status', editPass({ status: 'GREEN' }), baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: status-dictionary row E-pass:' });
fx('unknown-scope', editPass({ scope: 'prod' }), baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: scope-vocabulary row E-pass:' });
fx('bool-mutant-rejected', editPass({ mutant_rejected: 'yes' }), baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: bool-mutant-rejected row E-pass:' });
fx('required-cell-mutant-empty', editPass({ mutant: '' }), baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: required-cell-mutant row E-pass:' });
fx('schema-columns', baseRegistry(), baseRequired(),
  { postReg: (t) => t.replace(/^id,/, 'ident,') },
  { expectExit: 1, expectMarker: 'RED: schema-columns row -:' });
fx('evidence-test-missing',
  editPass({ entry: 'tests/removed_after_measurement_xyz.rs; cargo test' }),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: evidence-test-missing row E-pass:' });
fx('receipt-external-strong',
  editPass({ receipt: 'external:' + 'a'.repeat(64), receipt_sha256: '' }),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: receipt-external-strong row E-pass:' });
// A shallow clone cannot prove ancestry: strict must refuse instead of passing.
fx('identity-shallow-strict', baseRegistry(), baseRequired(),
  { extra: ['--strict'], rootFn: (dir) => {
    const sh = join(dir, 'shallow');
    const p = repoRoot.replace(/\\/g, '/');
    runGit(['clone', '--depth', '1', '--no-checkout', '--quiet',
      /^[A-Za-z]:/.test(p) ? `file:///${p}` : `file://${p}`, sh], dir);
    return sh;
  } },
  { expectExit: 1, expectMarker: 'RED: identity-shallow row -:' });
fx('required-dup-cell', baseRegistry(),
  baseRequired([reqRow('E-pass')]), {},
  { expectExit: 1, expectMarker: 'RED: required-dup-cell row E-pass:' });
fx('required-schema', baseRegistry(), baseRequired(),
  { requiredHeader: [...REQUIRED_HEADER, 'extra'] },
  { expectExit: 1, expectMarker: 'RED: required-schema' });
fx('required-gate', baseRegistry(),
  baseRequired().map((q) => q.cell_id === 'E-pass' ? { ...q, gate: 'X9' } : q), {},
  { expectExit: 1, expectMarker: 'RED: required-gate row E-pass:' });
fx('required-expects-green', baseRegistry(),
  baseRequired().map((q) => q.cell_id === 'E-pass' ? { ...q, expects: 'GREEN' } : q), {},
  { expectExit: 1, expectMarker: 'RED: required-expects row E-pass:' });
fx('required-expects-empty', baseRegistry(),
  baseRequired().map((q) => q.cell_id === 'E-pass' ? { ...q, expects: '' } : q), {},
  { expectExit: 1, expectMarker: 'RED: required-expects row E-pass:' });
fx('required-bool', baseRegistry(),
  baseRequired().map((q) => q.cell_id === 'E-pass' ? { ...q, requires_mutant: 'yes' } : q), {},
  { expectExit: 1, expectMarker: 'RED: required-bool row E-pass:' });
fx('required-not-superset', baseRegistry(), [reqRow('E-pass')], {},
  { expectExit: 1, expectMarker: 'RED: required-not-superset' });

// --- B1: required-missing (strict vs plain) ---
fx('required-missing-strict', baseRegistry(),
  baseRequired([reqRow('E-absent', { gate: 'C2' })]), { extra: ['--strict'] },
  { expectExit: 1, expectMarker: 'RED: required-missing row E-absent:' });
fx('required-missing-plain', baseRegistry(),
  baseRequired([reqRow('E-absent', { gate: 'C2' })]), {},
  { expectExit: 0, expectMarker: 'REQUIRED-MISSING: E-absent',
    expectVerdict: '[evidence-registry] VERDICT: INCOMPLETE' });

// --- B1: source_sha identity ---
fx('identity-unpublished-strict',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ source_sha: UNPUBLISHED_SHA }) : r),
  baseRequired(), { extra: ['--strict'] },
  { expectExit: 1, expectMarker: 'RED: identity-unpublished row E-pass:' });
fx('identity-unpublished-plain',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ source_sha: UNPUBLISHED_SHA }) : r),
  baseRequired(), {},
  { expectExit: 0, expectMarker: 'IDENTITY-UNRESOLVED: row E-pass',
    expectVerdict: '[evidence-registry] VERDICT: INCOMPLETE' });
fx('identity-spike-not-ancestor-strict',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ source_sha: SPIKE_SHA }) : r),
  baseRequired(), { extra: ['--strict'] },
  { expectExit: 1, expectMarker: 'RED: identity-unpublished row E-pass:' });
fx('identity-empty-sha-strict',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ source_sha: '—' }) : r),
  baseRequired(), { extra: ['--strict'] },
  { expectExit: 1, expectMarker: 'RED: identity-empty-sha row E-pass:' });
fx('identity-empty-sha-plain',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ source_sha: '—' }) : r),
  baseRequired(), {},
  { expectExit: 0, expectMarker: 'IDENTITY-UNRESOLVED: row E-pass',
    expectVerdict: '[evidence-registry] VERDICT: INCOMPLETE' });

// --- B1: cfg / shipping-installed scope ---
fx('pass-cfg-unsatisfied',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ features: 'std' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: pass-cfg-unsatisfied row E-pass:' });
fx('shipping-installed-needs-global-allocator',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    entry: 'tests/r11_ph5b_c2_same_va_cache.rs; cargo test --features "alloc-global alloc-decommit alloc-stats"',
    features: 'alloc-global alloc-decommit alloc-stats' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: shipping-installed-needs-global-allocator row E-pass:' });

// --- B1: miri model discipline ---
fx('miri-model-missing',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ profile: 'Miri dev' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: miri-model-mismatch row E-pass:' });
fx('miri-flag-mismatch',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    profile: 'Miri SB', activation: 'cargo miri run -Zmiri-tree-borrows witness run' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: miri-model-mismatch row E-pass:' });
fx('miri-sb-plus-tb',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ profile: 'Miri SB + TB' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: miri-model-mismatch row E-pass:' });

// --- B1: loom/shadow scope ---
fx('shadow-loom-not-shipping',
  baseRegistry().map((r) => r.id === 'E-pass'
    ? passRow({ entry: 'loom_model_x witness run' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: shadow-loom-not-shipping row E-pass:' });
fx('shadow-negation-control',
  baseRegistry().map((r) => r.id === 'E-pass'
    ? passRow({ entry: 'loom probe (not shipping) run', scope: 'shadow-model' }) : r),
  baseRequired(), {},
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });

// --- B1: PASS marker discipline ---
fx('pass-forbidden-marker',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    positive_marker: 'EXIT=101; test result: ok', mutant_rejected: 'false' }) : r),
  baseRequired().map((q) => q.cell_id === 'E-pass' ? { ...q, requires_mutant: 'false' } : q), {},
  { expectExit: 1, expectMarker: 'RED: pass-forbidden-marker row E-pass:' });

// --- B1: activation concreteness ---
fx('activation-trivial-dash',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ activation: '—' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: activation-trivial row E-pass:' });
fx('activation-trivial-short',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ activation: 'abc' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: activation-trivial row E-pass:' });
fx('activation-generic-cargo',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ activation: 'cargo test' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: activation-generic row E-pass:' });
fx('activation-generic-ru',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ activation: 'прогон теста' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: activation-generic row E-pass:' });

// --- B1: mutant discipline ---
// Finding: an empty mutant with mutant_rejected=true is caught by the
// mutant-rejected-consistency rule (not requires-mutant-rejected).
fx('mutant-empty-mr-true',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ mutant: '' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: mutant-rejected-consistency row E-pass:' });
fx('mutant-rejected-false',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ mutant_rejected: 'false' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: requires-mutant-rejected row E-pass:' });
fx('mutant-survived-token',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ mutant: 'W-1 mutant выжил' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: requires-mutant-rejected row E-pass:' });
fx('mutant-waiver-undocumented', baseRegistry(),
  baseRequired().map((q) => q.cell_id === 'E-pass'
    ? { ...q, class: 'protocol', requires_mutant: 'false', note: '' } : q), {},
  { expectExit: 1, expectMarker: 'RED: mutant-waiver-undocumented row E-pass:' });
fx('mutant-waiver-ok-control', baseRegistry(),
  baseRequired().map((q) => q.cell_id === 'E-pass'
    ? { ...q, class: 'protocol', requires_mutant: 'false',
      note: 'waiver: позитивная клетка без мутанта в источнике' } : q), {},
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });

// --- B1: staleness / identity base ---
fx('evidence-stale',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ source_sha: STALE_SHA }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: evidence-stale row E-pass:' });
fx('evidence-identity-base',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ source_sha: BASE_SHA }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: evidence-identity-base row E-pass:' });

// --- B1: receipts ---
fx('receipt-exists-nope',
  baseRegistry().map((r) => r.id === 'E-pass'
    ? passRow({ receipt: 'docs/reviews/2099-01-01-nope.md' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: receipt-exists row E-pass:' });
fx('receipt-roots-readme',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ receipt: 'README.md' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: receipt-roots row E-pass:' });
fx('receipt-roots-dotdot',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ receipt: '../outside.md' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: receipt-roots row E-pass:' });
fx('receipt-pin-required',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({ receipt_sha256: '' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: receipt-pin-required row E-pass:' });
fx('receipt-sha',
  baseRegistry().map((r) => r.id === 'E-pass'
    ? passRow({ receipt_sha256: '0'.repeat(64) }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: receipt-sha row E-pass:' });

// --- B1: known-defect signature ---
fx('known-defect-signature-none',
  baseRegistry().map((r) => r.id === 'E-kd'
    ? kdRow({ status_reason: 'defect confirmed by review, no signature' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: known-defect-signature row E-kd:' });
fx('known-defect-signature-no-arrow',
  baseRegistry().map((r) => r.id === 'E-kd'
    ? kdRow({ status_reason: 'src/lib.rs:1 defect without arrow' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: known-defect-signature row E-kd:' });
fx('known-defect-signature-missing-file',
  baseRegistry().map((r) => r.id === 'E-kd'
    ? kdRow({ status_reason: 'src/nope.rs:9 Node::write_next ← x' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: known-defect-signature row E-kd:' });

// --- B1: FAIL justification ---
fx('fail-unjustified',
  baseRegistry().map((r) => r.id === 'E-pass'
    ? passRow({ status: 'FAIL', status_reason: '', mutant_rejected: 'false' }) : r),
  baseRequired().map((q) => q.cell_id === 'E-pass' ? { ...q, expects: 'FAIL' } : q), {},
  { expectExit: 1, expectMarker: 'RED: fail-justified row E-pass:' });
fx('fail-resolved-by-decision-control',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    status: 'FAIL', mutant_rejected: 'false',
    status_reason: 'resolved-by-decision: docs/design/2026-10-05-adr-addendum-ph3c-path-b.md — follows from the decision' }) : r),
  baseRequired().map((q) => q.cell_id === 'E-pass' ? { ...q, expects: 'FAIL' } : q), {},
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });

// --- B1: counters / isolated witness ---
fx('counter-not-rss',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    positive_marker: 'System requested bytes: 100',
    status_reason: 'RSS reduced 30%' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: counter-not-rss row E-pass:' });
fx('counter-notrun-control',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    positive_marker: 'System requested bytes: 100',
    status_reason: 'RSS/latency claims — NOT_RUN' }) : r),
  baseRequired(), {},
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });
fx('isolated-witness-not-sefer',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    scope: 'isolated-witness', status_reason: 'Sefer acceptance confirmed' }) : r),
  baseRequired(), {},
  { expectExit: 1, expectMarker: 'RED: isolated-witness-not-sefer row E-pass:' });
fx('isolated-witness-negation-control',
  baseRegistry().map((r) => r.id === 'E-pass' ? passRow({
    scope: 'isolated-witness', status_reason: 'NOT a Sefer acceptance' }) : r),
  baseRequired(), {},
  { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN' });

// --- B2: mutations over the REAL registry (strict mode) ---
rl('R0-21-scope-to-shipping', { mut: { 'R0-21': { scope: 'shipping-installed' } } },
  { expectExit: 1, expectMarker: 'RED: shadow-loom-not-shipping row R0-21:' });
rl('R6a-D-entry-installed', {
  mut: { 'R6a-D': { entry: 'installed #[global_allocator] SeferAlloc',
    scope: 'shipping-installed' } } },
  { expectExit: 1, expectMarker: 'RED: shadow-loom-not-shipping row R6a-D:' });
rl('R0-04-unpublished-sha', { mut: { 'R0-04': { source_sha: UNPUBLISHED_SHA } } },
  { expectExit: 1, expectMarker: 'RED: identity-unpublished row R0-04:' });
rl('R0-04-activation-dash', { mut: { 'R0-04': { activation: '-' } } },
  { expectExit: 1, expectMarker: 'RED: activation-trivial row R0-04:' });
rl('R0-04-activation-na', { mut: { 'R0-04': { activation: 'n/a' } } },
  { expectExit: 1, expectMarker: 'RED: activation-trivial row R0-04:' });
rl('R0-04-miri-dev', { mut: { 'R0-04': { profile: 'Miri dev' } } },
  { expectExit: 1, expectMarker: 'RED: miri-model-mismatch row R0-04:' });
rl('R6a-C4-promoted-to-pass', {
  replace: { 'R6a-C4': validPassCells('R6a-C4') },
  exactlyOneRed: 'RED: required-cell-status row R6a-C4:' },
  { expectExit: 1, expectMarker: 'RED: required-cell-status row R6a-C4:' });
rl('required-header-only', { required: `${REQUIRED_HEADER.join(',')}\n` },
  { expectExit: 1, expectMarker: 'RED: required-not-superset' });
rl('R4a-03-id-trailing-space', { mut: { 'R4a-03': { id: 'R4a-03 ' } } },
  { expectExit: 1, expectMarker: 'RED: id-format row R4a-03' });
// Unterminated quote: opening quote in the LAST field of the LAST row —
// anywhere earlier, a later `"` in the file would silently close the field
// and the judge would not flag EOF-inside-quotes.
rl('registry-unterminated-quote', { postRows: (rows) => rows.map((c, i) =>
  i === rows.length - 1 ? [...c.slice(0, -1), { raw: '"unterminated' }] : c) },
  { expectExit: 1, expectMarker: 'RED: csv-unterminated-quote' });
rl('R4a-03-extra-column', { postRows: (rows) => rows.map((c) =>
  c[0] === 'R4a-03' ? [...c, 'extra22'] : c) },
  { expectExit: 1, expectMarker: 'RED: csv-column-count row R4a-03' });
rl('R5b-20-features-production', { mut: { 'R5b-20': { features: 'production' } } },
  { expectExit: 1, expectMarker: 'RED: pass-cfg-unsatisfied row R5b-20:' });
rl('R6a-A1-receipt-readme', { mut: { 'R6a-A1': { receipt: 'README.md' } } },
  { expectExit: 1, expectMarker: 'RED: receipt-roots row R6a-A1:' });
rl('R6a-M1-mr-false', { mut: { 'R6a-M1': { mutant_rejected: 'false' } } },
  { expectExit: 1, expectMarker: 'RED: requires-mutant-rejected row R6a-M1:' });

rl('miri-retag-sb-to-tb', { mut: { 'R0-04': { profile: 'Miri TB (tree-borrows) MIRIFLAGS T dev' } } },
  { expectExit: 1, expectMarker: 'RED: miri-model-mismatch row R0-04:' });
rl('miri-retag-tb-to-sb', { mut: { 'R5b-07': { profile: 'Miri SB (strict-provenance)' } } },
  { expectExit: 1, expectMarker: 'RED: miri-model-mismatch row R5b-07:' });
rl('red-row-promoted-to-pass-ub-marker',
  { mut: { 'R0-01': { status: 'PASS', status_reason: 'ok' } } },
  { expectExit: 1, expectMarker: 'RED: pass-forbidden-marker row R0-01:' });
rl('p1-box-cell-never-accepts-pass', { mut: { 'R6a-C5b': { status: 'PASS' } } },
  { expectExit: 1, expectMarker: 'RED: required-cell-status row R6a-C5b:' });
rl('spike-sha-must-be-allowlisted',
  { mut: { 'R5b-09-sb': { source_sha: 'deadbeefdeadbeefdeadbeefdeadbeefdeadbeef' } } },
  { expectExit: 1, expectMarker: 'RED: identity-unpublished row R5b-09-sb:' });
rl('strong-row-cites-missing-test',
  { mut: { 'R5b-05': { entry: 'tests/r11_ph5b_c2_v2_nonexistent.rs' } } },
  { expectExit: 1, expectMarker: 'RED: evidence-test-missing row R5b-05:' });
rl('strong-row-external-receipt',
  { mut: { 'R5b-05': { receipt: 'external:' + 'a'.repeat(64), receipt_sha256: '' } } },
  { expectExit: 1, expectMarker: 'RED: receipt-external-strong row R5b-05:' });

// --- B2b: baseline ratchet in a throwaway git repo ---
function ratchet(name, build, expect) {
  cases.push({ name, kind: 'ratchet', build, ...expect });
}
ratchet('r1-baseline-id-removed', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: true });
  return { repo, args: judgeArgs(repo.root, { root: repo.root, reg: repo.reg,
    req: repo.req, extra: ['--strict', '--baseline', repo.c1] }) };
}, { expectExit: 1, expectMarker: 'RED: baseline-id-removed row B:' });
ratchet('r2-retired-covers-removal', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: true });
  const reqNoB = realRequiredOf(repo); // header + row A only
  writeFileSync(repo.req, reqNoB);
  writeFileSync(join(repo.root, 'docs', 'evidence', 'retired_ids.csv'),
    'id,reason\nB,тест-фикстура удалена\n');
  runGit(['add', '.'], repo.root);
  runGit([...GIT_ID, 'commit', '-m', 'retire B'], repo.root);
  return { repo, args: judgeArgs(repo.root, { root: repo.root, reg: repo.reg,
    req: repo.req, extra: ['--baseline', repo.c1] }) };
}, { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN',
  notMarker: 'RED: baseline-' });
ratchet('r3-promotion-without-evidence', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: false });
  writeFileSync(join(repo.root, 'docs', 'evidence', 'registry.csv'),
    toCsv(HEADER, [repo.rowA, { ...repo.rowB, status: 'PASS' }].map(objCells)));
  runGit(['add', '.'], repo.root);
  runGit([...GIT_ID, 'commit', '-m', 'promote B to PASS'], repo.root);
  return { repo, args: judgeArgs(repo.root, { root: repo.root, reg: repo.reg,
    req: repo.req, extra: ['--baseline', repo.c1] }) };
}, { expectExit: 1, expectMarker: 'RED: baseline-promotion-without-evidence row B:' });
// Promotion needs NEW evidence: a different normalised source_sha, receipt set
// or pins. A fresh marker alone, whitespace, short-sha or token-order edits do not count.
function promoteB(repo, patch) {
  writeFileSync(join(repo.root, 'docs', 'evidence', 'registry.csv'),
    toCsv(HEADER, [repo.rowA, { ...repo.rowB, status: 'PASS', ...patch }].map(objCells)));
  runGit(['add', '.'], repo.root);
  runGit([...GIT_ID, 'commit', '-m', 'promote B'], repo.root);
  return { repo, args: judgeArgs(repo.root, { root: repo.root, reg: repo.reg,
    req: repo.req, extra: ['--baseline', repo.c1] }) };
}
ratchet('r4-promotion-with-new-receipt', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: false });
  const second = join(repo.root, 'docs', 'reviews', 'fixture2.md');
  writeFileSync(second, 'second fixture receipt\n');
  return promoteB(repo, {
    receipt: 'docs/reviews/fixture.md;docs/reviews/fixture2.md',
    receipt_sha256: `${repo.rowB.receipt_sha256};${sha256FileLf(second)}`,
    positive_marker: 'fixture log: ok. 2 passed' });
}, { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: GREEN',
  notMarker: 'baseline-promotion' });
ratchet('r7-promotion-marker-only', (dir) =>
  promoteB(makeRatchetRepo(dir, { commit2: false }),
    { positive_marker: 'fixture log: ok. 2 passed (edited marker only)' }),
  { expectExit: 1, expectMarker: 'RED: baseline-promotion-without-evidence row B:' });
ratchet('r8-promotion-short-sha-and-whitespace', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: false });
  return promoteB(repo, { source_sha: repo.rowB.source_sha.slice(0, 8),
    activation: repo.rowB.activation + ' ' });
}, { expectExit: 1, expectMarker: 'RED: baseline-promotion-without-evidence row B:' });
ratchet('r9-promotion-receipt-token-order', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: false });
  return promoteB(repo, { receipt: ' docs/reviews/fixture.md ' });
}, { expectExit: 1, expectMarker: 'RED: baseline-promotion-without-evidence row B:' });
// A required cell must not start accepting PASS while its registry row carries
// no new evidence (the other half of the ratchet).
ratchet('r10-required-widened-without-evidence', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: false, bExpects: 'NOT_RUN' });
  writeFileSync(repo.req, toCsv(REQUIRED_HEADER,
    [reqRow('A'), reqRow('B', { expects: 'NOT_RUN|PASS' })]
      .map((r) => REQUIRED_HEADER.map((h) => r[h]))));
  runGit(['add', '.'], repo.root);
  runGit([...GIT_ID, 'commit', '-m', 'widen B'], repo.root);
  return { repo, args: judgeArgs(repo.root, { root: repo.root, reg: repo.reg,
    req: repo.req, extra: ['--baseline', repo.c1] }) };
}, { expectExit: 1, expectMarker: 'RED: baseline-required-widened row B:' });
ratchet('r5-baseline-unresolved-skipped', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: true });
  return { repo, args: judgeArgs(repo.root, { root: repo.root, reg: repo.reg,
    req: repo.req, extra: ['--baseline', UNPUBLISHED_SHA] }) };
}, { expectExit: 0, expectVerdict: '[evidence-registry] VERDICT: INCOMPLETE',
  expectMarker: 'BASELINE: skipped', notMarker: 'RED: baseline-' });
ratchet('r6-baseline-unresolved-strict', (dir) => {
  const repo = makeRatchetRepo(dir, { commit2: true });
  return { repo, args: judgeArgs(repo.root, { root: repo.root, reg: repo.reg,
    req: repo.req, extra: ['--baseline', UNPUBLISHED_SHA, '--strict-baseline'] }) };
}, { expectExit: 1, expectMarker: 'RED: baseline-unresolved row -:' });

// required_cells.csv copy with row B dropped (for r2).
function realRequiredOf(repo) {
  const rows = parseCsv(readFileSync(repo.req, 'utf8'));
  return toCsv(rows[0], rows.slice(1).filter((r) => r[0] !== 'B'));
}

// -------------------------------------------------------------- main ---

const results = [];
const failed = [];
const t0 = Date.now();
for (const c of cases) {
  const dir = mkdtempSync(join(tmpdir(), 'ev-reg-selftest-'));
  let observed = 'ok';
  let ok = true;
  try {
    let res;
    if (c.kind === 'fixture') {
      writeFixture(dir, c.registryRows, c.requiredRows, c.opts);
      res = runJudge(judgeArgs(dir, { extra: c.opts.extra ?? [],
        root: c.opts.rootFn ? c.opts.rootFn(dir) : repoRoot }));
    } else if (c.kind === 'real') {
      realFixture(dir, c.mutation ?? {});
      if (c.mutation?.postRows) {
        const rows = c.mutation.postRows(parseCsv(readFileSync(join(dir, 'registry.csv'), 'utf8')));
        writeFileSync(join(dir, 'registry.csv'), toCsv(HDR, rows.slice(1)));
      }
      res = runJudge(judgeArgs(dir, { extra: ['--strict'] }));
    } else { // ratchet: registry/required live inside the tmp repo
      const { repo, args } = c.build(dir);
      res = runJudge(args);
    }
    const exitOk = res.code === c.expectExit;
    const markerOk = !c.expectMarker || markerHit(res.stdout, c.expectMarker);
    const verdictOk = !c.expectVerdict || markerHit(res.stdout, c.expectVerdict);
    const notOk = !c.notMarker || !markerHit(res.stdout, c.notMarker);
    const singleOk = !c.mutation?.exactlyOneRed ||
      (redLines(res.stdout).length === 1 &&
        redLines(res.stdout)[0].startsWith(c.mutation.exactlyOneRed));
    ok = exitOk && markerOk && verdictOk && notOk && singleOk;
    if (!ok) {
      observed = `exit=${res.code} (want ${c.expectExit})`
        + ` out=[${res.stdout.split('\n')
          .filter((l) => l.startsWith('RED:') || l.includes('VERDICT:')
            || l.startsWith('BASELINE:')).join(' ;; ').slice(0, 400)}]`
        + (c.expectMarker ? ` marker=${markerOk ? 'hit' : 'MISS'}` : '')
        + (c.expectVerdict ? ` verdict=${verdictOk ? 'hit' : 'MISS'}` : '')
        + (!notOk ? ` forbidden-marker=${c.notMarker}` : '')
        + (!singleOk ? ` redLines=[${redLines(res.stdout).join(' ;; ')}]` : '')
        + (res.stderr ? ` stderr: ${res.stderr.split('\n')[0]}` : '');
      failed.push(c.name);
    }
    results.push({ name: c.name,
      expected: `exit ${c.expectExit} + ${c.expectVerdict ?? c.expectMarker ?? 'no marker'}`,
      observed, ok });
  } catch (e) {
    failed.push(c.name);
    results.push({ name: c.name,
      expected: `exit ${c.expectExit} + ${c.expectVerdict ?? c.expectMarker ?? '?'}`,
      observed: `setup error: ${e.message}`, ok: false });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

const secs = ((Date.now() - t0) / 1000).toFixed(1);
const w = Math.max(...results.map((r) => r.name.length), 4);
console.log('case'.padEnd(w) + ' | expected | observed | ok');
for (const r of results) {
  console.log(r.name.padEnd(w) + ` | ${r.expected} | ${r.observed} | ${r.ok ? 'yes' : 'NO'}`);
}
if (failed.length > 0) {
  console.error(`EVIDENCE-REGISTRY-SELFTEST: FAILED cases: ${failed.join(', ')}`);
  process.exit(1);
}
console.log(`EVIDENCE-REGISTRY-SELFTEST: ALL PASS (${results.length} cases) in ${secs}s`);
process.exit(0);
