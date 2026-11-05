#!/usr/bin/env node
// Test selection from a diff and the test-impact map (docs/CI_TIERS.md, CI-T1 stage 3). The map comes from
// scripts/test-impact-map.mjs (nightly artifact). Selection is conservative: anything this script cannot
// attribute to a test selects EVERY test ("all"); a subset is returned only when every changed file is
// accounted for.
//
// Usage:
//   node scripts/select-tests.mjs select --maps m1.json [m2.json ...] --base <sha> --head <sha> [--full] --out sel.json
//   node scripts/select-tests.mjs args --selection sel.json --features "<f>"|all [--no-default-features]
//       prints the cargo test selectors for a row: "" for all, else "--lib --test a --test b"
//   node scripts/select-tests.mjs --selftest
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { testTargets } from './test-impact-map.mjs';

/** Files that change what every test compiles or links against. */
const ALWAYS_ALL = /^(Cargo\.(toml|lock)|build\.rs|rust-toolchain(\.toml)?|\.cargo\/.*|deny\.toml|clippy\.toml|rustfmt\.toml|\.rustfmt\.toml)$/;
/** Non-Rust inputs a test may read at run time: only the tests that read files are selected for them. */
const DATA_FILE = /^(docs\/|scripts\/|\.github\/|[^/]+\.(md|csv|json|txt)$)/;
/** A test (or a support file it executes) that touches the file system, the manifest dir or a child process. */
const READS_FILES = /(CARGO_MANIFEST_DIR|include_str!|include_bytes!|read_to_string|fs::read|File::open|read_dir|Command::new)/;
/** Compiled only by tools that are not tests (kani); a change selects no test. */
const TOOL_ONLY = new Set(['src/kani_proofs.rs']);
const RUST_SRC = /^(src\/|crates\/[^/]+\/src\/).*\.rs$/;

const all = reason => ({ mode: 'all', reason });

/**
 * @param files    changed repository paths
 * @param maps     [{ tests: {path: [files]}, unmapped: {path: reason} }] one per row
 * @param targets  [{ name, path, requiredFeatures }] test targets of the commit
 * @param sources  path -> text of every test target and every tests/ file a map lists (for the file-reader scan)
 * @param full     an explicit full run was requested
 */
export function select({ files, maps, targets, sources, full }) {
  if (full) return all('full run requested');
  if (maps.length === 0) return all('no test-impact map available');
  if (!files) return all('changed files unknown');
  const byPath = new Map(targets.map(t => [t.path, t]));
  const covering = new Map(); // repo file -> Set of test paths executing it, over all rows
  const mappedAnywhere = new Set();
  for (const m of maps) {
    for (const [t, fs] of Object.entries(m.tests)) {
      mappedAnywhere.add(t);
      for (const f of fs) {
        if (!covering.has(f)) covering.set(f, new Set());
        covering.get(f).add(t);
      }
    }
  }
  const picked = new Map(); // test path -> reason
  const pick = (t, why) => { if (byPath.has(t) && !picked.has(t)) picked.set(t, why); };
  // Tests no row maps are always selected: nothing is known about what they execute.
  for (const t of byPath.keys()) if (!mappedAnywhere.has(t)) pick(t, 'unmapped in every row');
  let readers = null;
  const fileReaders = () => {
    if (readers) return readers;
    readers = new Set();
    for (const t of byPath.keys()) {
      const texts = [sources[t] ?? ''];
      for (const m of maps) for (const f of m.tests[t] ?? []) if (f.startsWith('tests/')) texts.push(sources[f] ?? '');
      if (texts.some(x => READS_FILES.test(x))) readers.add(t);
    }
    return readers;
  };
  for (const raw of files) {
    const f = raw.replace(/\\/g, '/');
    if (ALWAYS_ALL.test(f)) return all(`${f} changes every build`);
    if (TOOL_ONLY.has(f)) continue;
    if (byPath.has(f)) { pick(f, 'changed itself'); continue; }
    if (covering.has(f)) { for (const t of covering.get(f)) pick(t, `executes ${f}`); continue; }
    if (DATA_FILE.test(f)) { for (const t of fileReaders()) pick(t, `reads files (${f} changed)`); continue; }
    if (RUST_SRC.test(f) || f.startsWith('tests/')) return all(`${f} is executed by no mapped test`);
    return all(`${f} is not attributable to a test`);
  }
  return { mode: 'subset', reason: 'every changed file is attributed', tests: [...picked.keys()].sort(), why: Object.fromEntries(picked) };
}

/** Features enabled by `requested` (space/comma separated) plus defaults, closed over the package feature graph. */
export function featureClosure(featureMap, requested, { all: everything = false, noDefault = false } = {}) {
  if (everything) return new Set(Object.keys(featureMap));
  const out = new Set();
  const queue = requested.split(/[\s,]+/).filter(Boolean);
  if (!noDefault && 'default' in featureMap) queue.push('default');
  while (queue.length > 0) {
    const f = queue.pop();
    if (out.has(f)) continue;
    out.add(f);
    for (const item of featureMap[f] ?? []) {
      if (item.startsWith('dep:') || item.includes('/')) continue;
      queue.push(item);
    }
  }
  return out;
}

/** cargo selectors for a row: '' = run everything; otherwise `--lib` plus the selected tests the row can build. */
export function rowArgs(selection, targets, enabled) {
  if (selection.mode !== 'subset') return '';
  const picked = new Set(selection.tests);
  const names = targets.filter(t => picked.has(t.path) && t.requiredFeatures.every(f => enabled.has(f))).map(t => t.name).sort();
  return ['--lib', ...names.flatMap(n => ['--test', n])].join(' ');
}

function run(cmd, args) {
  const r = spawnSync(cmd, args, { encoding: 'utf8', maxBuffer: 1 << 28 });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')} failed: ${r.stderr}`);
  return r.stdout;
}

function metadata() {
  return JSON.parse(run('cargo', ['metadata', '--no-deps', '--format-version', '1']));
}

function arg(args, name) {
  const i = args.indexOf(name);
  return i < 0 ? undefined : args[i + 1];
}

function listAfter(args, name) {
  const i = args.indexOf(name);
  if (i < 0) return [];
  const out = [];
  for (let k = i + 1; k < args.length && !args[k].startsWith('--'); k++) out.push(args[k]);
  return out;
}

/** Markdown for the job summary. */
export function summaryMd(sel) {
  if (sel.mode === 'all') return ['### Test selection', '', `All ${sel.total} tests: ${sel.reason}.`, ''].join('\n');
  const lines = sel.tests.map(t => '- ' + t + ' — ' + sel.why[t]);
  const head = `${sel.tests.length} of ${sel.total} tests (map ${sel.mapShas.map(x => x.slice(0, 8)).join(', ')}).`;
  return ['### Test selection', '', head, '', ...lines, ''].join('\n');
}

function selectCmd(args) {
  const root = process.cwd();
  const meta = metadata();
  const targets = testTargets(meta, root);
  const maps = listAfter(args, '--maps').map(p => JSON.parse(readFileSync(p, 'utf8')));
  const [base, head] = [arg(args, '--base'), arg(args, '--head')];
  let files = null;
  if (base && head && !/^0+$/.test(base)) {
    try { files = run('git', ['diff', '--name-only', base, head]).split('\n').filter(Boolean); } catch { files = null; }
  }
  const sources = {};
  const need = new Set(targets.map(t => t.path));
  for (const m of maps) for (const fs of Object.values(m.tests)) for (const f of fs) if (f.startsWith('tests/')) need.add(f);
  for (const p of need) { try { sources[p] = readFileSync(p, 'utf8'); } catch { sources[p] = ''; } }
  const sel = select({ files, maps, targets, sources, full: args.includes('--full') });
  sel.total = targets.length;
  sel.mapShas = maps.map(m => m.sha);
  writeFileSync(arg(args, '--out'), `${JSON.stringify(sel, null, 1)}\n`);
  if (args.includes('--summary')) console.log(summaryMd(sel));
  console.log(sel.mode === 'all' ? `select-tests: all ${targets.length} tests (${sel.reason})` : `select-tests: ${sel.tests.length} of ${targets.length} tests`);
}

function argsCmd(args) {
  const sel = JSON.parse(readFileSync(arg(args, '--selection'), 'utf8'));
  const meta = metadata();
  const root = process.cwd();
  const pkg = meta.packages.find(p => p.manifest_path.replace(/\\/g, '/').endsWith('/Cargo.toml') && !p.manifest_path.replace(/\\/g, '/').includes('/crates/'));
  const features = arg(args, '--features') ?? '';
  const enabled = featureClosure(pkg.features, features === 'all' ? '' : features, { all: features === 'all', noDefault: args.includes('--no-default-features') });
  console.log(rowArgs(sel, testTargets(meta, root), enabled));
}

function selftest() {
  const targets = [
    { name: 'a', path: 'tests/a.rs', requiredFeatures: [] },
    { name: 'b', path: 'tests/b.rs', requiredFeatures: [] },
    { name: 'c', path: 'tests/c.rs', requiredFeatures: ['x'] },
    { name: 'd', path: 'tests/d.rs', requiredFeatures: [] },
    { name: 'e', path: 'tests/e.rs', requiredFeatures: [] },
  ];
  const row1 = { tests: { 'tests/a.rs': ['src/m.rs', 'src/n.rs'], 'tests/b.rs': ['src/n.rs', 'tests/support/s.rs'], 'tests/d.rs': ['src/m.rs'] }, unmapped: { 'tests/c.rs': 'requires-features', 'tests/e.rs': 'no-coverage' } };
  const row2 = { tests: { 'tests/c.rs': ['src/x.rs'], 'tests/d.rs': ['src/m.rs'] }, unmapped: { 'tests/a.rs': 'no-coverage', 'tests/b.rs': 'no-coverage', 'tests/e.rs': 'no-coverage' } };
  const sources = { 'tests/a.rs': 'fn t(){}', 'tests/b.rs': 'fn t(){}', 'tests/c.rs': 'fn t(){}', 'tests/d.rs': 'let s = std::fs::read_to_string("docs/x.md");', 'tests/e.rs': '', 'tests/support/s.rs': 'fn s(){}' };
  const sel = files => select({ files, maps: [row1, row2], targets, sources, full: false });
  const names = s => (s.mode === 'all' ? `ALL:${s.reason}` : s.tests.join(','));
  // e is mapped in no row: always selected
  assert.equal(names(sel(['src/n.rs'])), 'tests/a.rs,tests/b.rs,tests/e.rs');
  assert.equal(names(sel(['src/m.rs'])), 'tests/a.rs,tests/d.rs,tests/e.rs');
  assert.equal(names(sel(['src/x.rs'])), 'tests/c.rs,tests/e.rs'); // covered only by row 2
  assert.equal(names(sel(['tests/support/s.rs'])), 'tests/b.rs,tests/e.rs'); // support file via the map
  assert.equal(names(sel(['tests/d.rs'])), 'tests/d.rs,tests/e.rs'); // a changed test selects itself
  assert.equal(names(sel(['src/n.rs', 'src/x.rs'])), 'tests/a.rs,tests/b.rs,tests/c.rs,tests/e.rs');
  assert.equal(names(sel([])), 'tests/e.rs'); // nothing changed: only the always-run set
  // data files select the file readers (d reads files)
  assert.equal(names(sel(['docs/perf/R.md'])), 'tests/d.rs,tests/e.rs');
  assert.equal(names(sel(['CHANGELOG.md', 'src/n.rs'])), 'tests/a.rs,tests/b.rs,tests/d.rs,tests/e.rs');
  // everything else is "all"
  for (const f of ['Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo/config.toml', 'src/new_file.rs', 'tests/fixtures/x.in', 'crates/q/src/lib.rs', 'weird.bin']) {
    assert.match(names(sel(['src/n.rs', f])), /^ALL:/, f);
  }
  assert.equal(names(sel(['src/kani_proofs.rs'])), 'tests/e.rs'); // kani-only file: no test affected
  assert.match(names(select({ files: ['src/n.rs'], maps: [row1], targets, sources, full: true })), /^ALL:full/);
  assert.match(names(select({ files: ['src/n.rs'], maps: [], targets, sources, full: false })), /^ALL:no test-impact map/);
  assert.match(names(select({ files: null, maps: [row1], targets, sources, full: false })), /^ALL:changed files unknown/);
  // a support file that reads files makes every test that executes it a reader
  const src2 = { ...sources, 'tests/support/s.rs': 'let _ = std::process::Command::new("node");' };
  const s2 = select({ files: ['docs/x.md'], maps: [row1, row2], targets, sources: src2, full: false });
  assert.equal(names(s2), 'tests/b.rs,tests/d.rs,tests/e.rs');
  // feature closure and per-row arguments
  const fm = { default: ['std'], std: [], x: ['y', 'dep:z', 'z/f'], y: [], prod: ['x'] };
  assert.deepEqual([...featureClosure(fm, 'prod')].sort(), ['default', 'prod', 'std', 'x', 'y']);
  assert.deepEqual([...featureClosure(fm, 'prod', { noDefault: true })].sort(), ['prod', 'x', 'y']);
  assert.equal(featureClosure(fm, '', { all: true }).size, 5);
  const subset = sel(['src/x.rs']);
  assert.equal(rowArgs(subset, targets, new Set()), '--lib --test e');
  assert.equal(rowArgs(subset, targets, new Set(['x'])), '--lib --test c --test e');
  assert.equal(rowArgs(all('x'), targets, new Set(['x'])), '');
  assert.equal(rowArgs({ mode: 'subset', tests: [] }, targets, new Set()), '--lib');
  console.log('select-tests selftest: ok');
}

function main(args) {
  if (args[0] === '--selftest') return selftest();
  if (args[0] === 'select') return selectCmd(args);
  if (args[0] === 'args') return argsCmd(args);
  console.error('usage: select-tests.mjs select|args|--selftest');
  process.exit(2);
}

if (process.argv[1]?.replace(/\\/g, '/').endsWith('scripts/select-tests.mjs')) main(process.argv.slice(2));
