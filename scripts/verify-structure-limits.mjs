// Structure limits (CLAUDE.md "File and module structure"):
//   - a source directory holds at most 7 entries (files plus subdirectories);
//   - a code file has at most 1000 lines.
// Scope: directories are `src/**` and `crates/*/src/**`; files are code (.rs .mjs .js .py .sh)
// anywhere outside `docs/`. tests/, examples/, benches/, scripts/ and docs/ are exempt from the
// directory rule: Cargo discovers tests/*.rs and benches/*.rs only when they are flat, and docs/
// holds hundreds of generated reports.
// Debt is a ratchet: docs/structure_limits_baseline.csv lists every violation that existed when
// the rule landed with its count then. A new violation, a baseline row that grew, a row that no
// longer violates and a row whose count dropped without lowering the baseline are all red, so the
// debt can only shrink. `--write-baseline` regenerates the file (existing reasons are kept).
// The existing tests/src_file_size_cap.rs stays as the strict zero-tolerance check for the root src/.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const MAX_ENTRIES = 7;
const MAX_LINES = 1000;
const BASELINE_REL = 'docs/structure_limits_baseline.csv';
const BASELINE_HEADER = ['kind', 'path', 'count', 'reason'];
const DEFAULT_REASON = 'grandfathered when the structure limits landed (2026-10-12)';

const folderInScope = (dir) => dir === 'src' || dir.startsWith('src/') || /^crates\/[^/]+\/src(\/|$)/.test(dir);
const fileInScope = (file) => /\.(rs|mjs|js|py|sh)$/.test(file) && !file.startsWith('docs/')
  && !file.startsWith('target/') && !file.startsWith('worktrees/');

// Violations as [{ kind: 'folder' | 'file', path, count }], sorted by kind then path.
function measure(files, linesOf) {
  const children = new Map();
  for (const file of files) {
    const parts = file.split('/');
    for (let i = 0; i < parts.length; i++) {
      const dir = parts.slice(0, i).join('/');
      if (!children.has(dir)) children.set(dir, new Set());
      children.get(dir).add(parts[i]);
    }
  }
  const out = [];
  for (const [dir, set] of children) {
    if (dir !== '' && folderInScope(dir) && set.size > MAX_ENTRIES) out.push({ kind: 'folder', path: dir, count: set.size });
  }
  for (const file of files) {
    if (!fileInScope(file)) continue;
    const lines = linesOf(file);
    if (lines > MAX_LINES) out.push({ kind: 'file', path: file, count: lines });
  }
  return out.sort((a, b) => a.kind.localeCompare(b.kind) || a.path.localeCompare(b.path));
}

function parseBaseline(text) {
  const rows = [], errors = [];
  const lines = text.replace(/\r\n/g, '\n').split('\n').filter((l) => l !== '');
  if (lines[0] !== BASELINE_HEADER.join(',')) {
    errors.push(`baseline header must be exactly ${BASELINE_HEADER.join(',')}`);
    return { rows, errors };
  }
  const seen = new Set();
  for (const line of lines.slice(1)) {
    const cells = line.split(',');
    // The reason is the last column and may contain commas.
    const [kind, path, count, ...reason] = cells;
    const text = reason.join(',').trim();
    if (!['folder', 'file'].includes(kind) || !path || !/^\d+$/.test(count ?? '')) {
      errors.push(`malformed baseline row: ${line}`);
      continue;
    }
    if (!text) errors.push(`missing reason: ${kind} ${path}`);
    const key = `${kind} ${path}`;
    if (seen.has(key)) errors.push(`duplicate baseline row: ${key}`);
    seen.add(key);
    rows.push({ kind, path, count: Number(count), reason: text });
  }
  return { rows, errors };
}

function compare(violations, baselineRows) {
  const errors = [];
  const base = new Map(baselineRows.map((r) => [`${r.kind} ${r.path}`, r]));
  const limit = (kind) => (kind === 'folder' ? `${MAX_ENTRIES} entries` : `${MAX_LINES} lines`);
  const seen = new Set();
  for (const v of violations) {
    const key = `${v.kind} ${v.path}`;
    seen.add(key);
    const b = base.get(key);
    if (!b) errors.push(`new violation: ${v.kind} ${v.path} has ${v.count} (limit ${limit(v.kind)})`);
    else if (v.count > b.count) errors.push(`grew: ${v.kind} ${v.path} has ${v.count}, baseline ${b.count}`);
    else if (v.count < b.count) errors.push(`lower the baseline: ${v.kind} ${v.path} has ${v.count}, baseline ${b.count} (run --write-baseline)`);
  }
  for (const key of base.keys()) {
    if (!seen.has(key)) errors.push(`stale baseline row, no longer over the limit: ${key} (remove it, run --write-baseline)`);
  }
  return errors;
}

function renderBaseline(violations, previous) {
  const reasons = new Map(previous.map((r) => [`${r.kind} ${r.path}`, r.reason]));
  const rows = violations.map((v) => [v.kind, v.path, v.count, reasons.get(`${v.kind} ${v.path}`) || DEFAULT_REASON]);
  return [BASELINE_HEADER.join(','), ...rows.map((r) => r.join(','))].join('\n') + '\n';
}

function selfTest() {
  const flat = (n, dir = 'src/a') => Array.from({ length: n }, (_, i) => `${dir}/f${i}.rs`);
  const lines = (map) => (file) => map[file] ?? 10;
  const names = (vs) => vs.map((v) => `${v.kind}:${v.path}:${v.count}`);
  let cases = 0;
  const check = (label, actual, expected) => { assert.deepEqual(actual, expected, label); cases++; };

  check('seven entries pass', measure(flat(7), lines({})), []);
  check('eight entries fail', names(measure(flat(8), lines({}))), ['folder:src/a:8']);
  check('subdirectories count as entries', names(measure([...flat(7), 'src/a/sub/x.rs'], lines({}))), ['folder:src/a:8']);
  check('mod.rs counts as an entry', names(measure([...flat(7), 'src/a/mod.rs'], lines({}))), ['folder:src/a:8']);
  check('crate src is in scope', names(measure(flat(8, 'crates/k/src'), lines({}))), ['folder:crates/k/src:8']);
  check('crate tests are exempt from the folder rule', measure(flat(40, 'crates/k/tests'), lines({})), []);
  check('top-level tests, scripts and docs are exempt from the folder rule',
    measure([...flat(30, 'tests'), ...flat(30, 'scripts'), ...flat(30, 'docs/perf')], lines({})), []);
  check('exactly 1000 lines pass', measure(['src/x.rs'], lines({ 'src/x.rs': 1000 })), []);
  check('1001 lines fail', names(measure(['src/x.rs'], lines({ 'src/x.rs': 1001 }))), ['file:src/x.rs:1001']);
  check('scripts and tests code files are in the file rule',
    names(measure(['tests/t.rs', 'scripts/s.mjs'], lines({ 'tests/t.rs': 1200, 'scripts/s.mjs': 1100 }))),
    ['file:scripts/s.mjs:1100', 'file:tests/t.rs:1200']);
  check('docs and config files are exempt from the file rule',
    measure(['docs/r.md', 'Cargo.toml', '.github/workflows/ci.yml', 'docs/x.rs'],
      lines({ 'docs/r.md': 5000, 'Cargo.toml': 5000, '.github/workflows/ci.yml': 5000, 'docs/x.rs': 5000 })), []);

  const baseline = (...rows) => rows.map(([kind, path, count]) => ({ kind, path, count, reason: 'r' }));
  const v = (kind, path, count) => ({ kind, path, count });
  check('equal to baseline passes', compare([v('file', 'a.rs', 1200)], baseline(['file', 'a.rs', 1200])), []);
  check('new violation is red', compare([v('file', 'b.rs', 1100)], baseline(['file', 'a.rs', 1200])).length, 2);
  check('growth is red', compare([v('folder', 'src/d', 9)], baseline(['folder', 'src/d', 8])),
    ['grew: folder src/d has 9, baseline 8']);
  check('shrink without lowering the baseline is red',
    compare([v('folder', 'src/d', 8)], baseline(['folder', 'src/d', 9])).length, 1);
  check('a fixed violation leaves a stale row',
    compare([], baseline(['file', 'a.rs', 1200])).length, 1);

  check('baseline header is validated', parseBaseline('x,y\n').errors.length, 1);
  check('baseline reason is required',
    parseBaseline('kind,path,count,reason\nfile,a.rs,1200,\n').errors, ['missing reason: file a.rs']);
  check('duplicate baseline row is red',
    parseBaseline('kind,path,count,reason\nfile,a.rs,1200,why\nfile,a.rs,1200,why\n').errors, ['duplicate baseline row: file a.rs']);
  check('malformed baseline row is red', parseBaseline('kind,path,count,reason\nfile,a.rs,many,why\n').errors.length, 1);
  const text = renderBaseline([v('file', 'a.rs', 1300)], [{ kind: 'file', path: 'a.rs', count: 1200, reason: 'keep, with comma' }]);
  check('rendering keeps existing reasons', parseBaseline(text).rows, [{ kind: 'file', path: 'a.rs', count: 1300, reason: 'keep, with comma' }]);
  return cases;
}

function repoFiles() {
  const r = spawnSync('git', ['ls-files', '--cached', '--others', '--exclude-standard'], { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 1 << 26 });
  if (r.status !== 0) throw new Error(`git ls-files failed: ${r.stderr}`);
  return r.stdout.split('\n').filter((f) => f && existsSync(join(REPO_ROOT, f)));
}
const lineCount = (rel) => {
  const text = readFileSync(join(REPO_ROOT, rel), 'utf8');
  return text === '' ? 0 : text.split('\n').length - (text.endsWith('\n') ? 1 : 0);
};

function main() {
  const args = process.argv.slice(2);
  const cases = selfTest();
  if (args.includes('--self-test')) { console.log(`[verify-structure-limits] self-test OK (${cases} cases)`); return 0; }
  const violations = measure(repoFiles(), lineCount);
  const path = join(REPO_ROOT, BASELINE_REL);
  const parsed = existsSync(path) ? parseBaseline(readFileSync(path, 'utf8')) : { rows: [], errors: [`missing ${BASELINE_REL}`] };
  if (args.includes('--write-baseline')) {
    writeFileSync(path, renderBaseline(violations, parsed.rows), 'utf8');
    console.log(`[verify-structure-limits] wrote ${BASELINE_REL}: ${violations.length} rows`);
    return 0;
  }
  const errors = [...parsed.errors, ...compare(violations, parsed.rows)];
  if (errors.length) {
    console.error(`[verify-structure-limits] FAIL (limits: ${MAX_ENTRIES} entries per src directory, ${MAX_LINES} lines per code file):`);
    for (const e of errors) console.error(`  ${e}`);
    return 1;
  }
  const folders = violations.filter((x) => x.kind === 'folder').length;
  console.log(`[verify-structure-limits] OK: self-test ${cases} cases; ${folders} directories and ${violations.length - folders} files grandfathered by ${BASELINE_REL}, none new, none grown`);
  return 0;
}
process.exit(main());
