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
import { loadCrateGraph, crateForPath, affectedCrates, rootAffected } from './ci/crate-graph.mjs';

/** Files that change what every test compiles or links against. */
const ALWAYS_ALL = /^(Cargo\.(toml|lock)|build\.rs|rust-toolchain(\.toml)?|\.cargo\/.*|deny\.toml|clippy\.toml|rustfmt\.toml|\.rustfmt\.toml)$/;
export function isAlwaysAll(path) { return ALWAYS_ALL.test(path); }
/** Non-Rust inputs a test may read at run time: only the tests that read files are selected for them. */
const DATA_FILE = /^(docs\/|scripts\/|\.github\/|[^/]+\.(md|csv|json|txt)$)/;
/** Test inputs and standalone Rust examples/benchmarks are treated like other reader data files. */
export function isDataFile(path) {
  return DATA_FILE.test(path)
    || /^tests\/(?!.*\.rs$).+$/.test(path)
    || /^(?:benches|examples)\/.*\.rs$/.test(path)
    || /^crates\/[^/]+\/[^/]+\/(?:benches|examples)\/.*\.rs$/.test(path);
}
/** A test (or a support file it executes) that touches the file system, the manifest dir or a child process. */
const READS_FILES = /(CARGO_MANIFEST_DIR|include_str!|include_bytes!|include\s*!\s*\(|read_to_string|fs::read|File::open|read_dir|Command::new|#\s*\[\s*path\s*=)/;
/** Compiled only by tools that are not tests (kani); a change selects no test. */
const TOOL_ONLY = new Set([
  'src/kani_proofs.rs', // legacy path for deletions and historical diffs
  'crates/core/sefer-registry/src/proofs/depot_overflow_proofs.rs',
  'crates/core/sefer-registry/src/proofs/depot_proofs.rs',
  'crates/core/sefer-registry/src/saturation_proofs.rs',
  'crates/core/sefer-segment/src/node_proofs.rs',
  'crates/core/sefer-segment/src/segment/segment_header/large_state_proofs.rs',
]);
export function isToolOnly(path) { return TOOL_ONLY.has(path); }
const RUST_SRC = /^(src\/|crates\/[^/]+\/[^/]+\/src\/).*\.rs$/;

const all = reason => ({ mode: 'all', reason });

/**
 * @param files    changed repository paths
 * @param maps     [{ tests: {path: [files]}, unmapped: {path: reason} }] one per row
 * @param targets  [{ name, path, requiredFeatures }] test targets of the commit
 * @param sources  path -> text of every test target and every tests/ file a map lists (for the file-reader scan)
 * @param full     an explicit full run was requested
 * @param crateGraph optional workspace crate graph (scripts/ci/crate-graph.mjs shape); null keeps the
 *                   conservative legacy behavior for unmapped crates/** files
 */
export function select({ files, maps, targets, sources, full, crateGraph = null }) {
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
  const crateOnlyFiles = []; // changed files attributable to a workspace crate and nothing else
  for (const raw of files) {
    const f = raw.replace(/\\/g, '/');
    if (ALWAYS_ALL.test(f)) return all(`${f} changes every build`);
    if (isToolOnly(f)) continue;
    if (byPath.has(f)) { pick(f, 'changed itself'); continue; }
    if (covering.has(f)) { for (const t of covering.get(f)) pick(t, `executes ${f}`); continue; }
    if (isDataFile(f)) { for (const t of fileReaders()) pick(t, `reads files (${f} changed)`); continue; }
    // Crate-graph narrowing: a change confined to a workspace crate the root does not (transitively)
    // depend on cannot affect the ROOT crate's test targets — select no root tests for it. Anything
    // ambiguous (no graph, unknown crate, root crate itself, root-affected crate) stays "all".
    if (crateGraph && (RUST_SRC.test(f) || crateForPath(f, crateGraph) !== null)) {
      const crate = crateForPath(f, crateGraph);
      if (crate && crate !== crateGraph.root) { crateOnlyFiles.push(crate); continue; }
    }
    if (RUST_SRC.test(f) || f.startsWith('tests/')) return all(`${f} is executed by no mapped test`);
    return all(`${f} is not attributable to a test`);
  }
  if (crateOnlyFiles.length) {
    const changed = [...new Set(crateOnlyFiles)].sort();
    const affected = affectedCrates(changed, crateGraph);
    if (!rootAffected(affected, crateGraph)) {
      const dependents = affected.size - changed.length;
      // Readers still run: they may scan crates/** at runtime (same semantics
      // as the DATA_FILE branch). The subset is empty only when no reader exists.
      for (const t of fileReaders()) pick(t, 'reads files (crate-only change)');
      return { mode: 'subset', reason: `crate-only change: ${changed.join(', ')} (+${dependents} dependents) does not affect the root crate`, tests: [...picked.keys()].sort(), why: Object.fromEntries(picked), crateOnly: { crates: [...affected].sort() } };
    }
    // The level-2 map cannot attribute inside a crate: root may be affected, run everything.
    return all(`crate change (${changed.join(', ')}) reaches the root crate`);
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
  const sel = select({ files, maps, targets, sources, full: args.includes('--full'), crateGraph: loadCrateGraph(root) });
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
  // Non-Rust files below tests/ and Rust examples/ files are reader inputs.
  assert.equal(names(sel(['tests/x.test.mjs'])), 'tests/d.rs,tests/e.rs');
  assert.equal(names(sel(['tests/fixtures/x.json'])), 'tests/d.rs,tests/e.rs');
  assert.equal(names(sel(['tests/fixtures/x.txt'])), 'tests/d.rs,tests/e.rs');
  assert.equal(names(sel(['examples/_shared/foo.rs'])), 'tests/d.rs,tests/e.rs');
  const pathAttrReader = { ...sources, 'tests/a.rs': '#[path = "../examples/_shared/foo.rs"] mod foo;' };
  assert.equal(names(select({ files: ['examples/_shared/foo.rs'], maps: [row1, row2], targets, sources: pathAttrReader, full: false })), 'tests/a.rs,tests/d.rs,tests/e.rs');
  const spacedPathAttrReader = { ...sources, 'tests/a.rs': '# [ path = "../examples/_shared/foo.rs"] mod foo;' };
  assert.equal(names(select({ files: ['examples/_shared/foo.rs'], maps: [row1, row2], targets, sources: spacedPathAttrReader, full: false })), 'tests/a.rs,tests/d.rs,tests/e.rs');
  assert.equal(names(sel(['crates/core/pkg/benches/probe.rs'])), 'tests/d.rs,tests/e.rs');
  assert.equal(names(sel(['crates/core/pkg/examples/helper.rs'])), 'tests/d.rs,tests/e.rs');
  const noReaders = select({ files: ['tests/x.test.mjs'], maps: [{ tests: { 'tests/a.rs': ['src/a.rs'] }, unmapped: {} }], targets: [targets[0]], sources: {}, full: false });
  assert.deepEqual(noReaders, { mode: 'subset', reason: 'every changed file is attributed', tests: [], why: {} });
  assert.match(names(sel(['tests/x.rs'])), /^ALL:/);
  assert.match(names(sel(['src/foo.rs'])), /^ALL:/);
  // Existing map attribution wins over the reader fallback.
  const coveringExamples = { ...row1, tests: { ...row1.tests, 'tests/a.rs': [...row1.tests['tests/a.rs'], 'examples/_shared/foo.rs'] } };
  assert.equal(names(select({ files: ['examples/_shared/foo.rs'], maps: [coveringExamples, row2], targets, sources, full: false })), 'tests/a.rs,tests/e.rs');
  const explicitReader = { ...sources, 'tests/a.rs': 'const X: &str = include_str!("fixtures/input.json");' };
  assert.equal(names(select({ files: ['tests/fixtures/input.json'], maps: [row1, row2], targets, sources: explicitReader, full: false })), 'tests/a.rs,tests/d.rs,tests/e.rs');
  const includedReader = { ...sources, 'tests/a.rs': 'include!("../examples/_shared/b4_interleave_core.rs");' };
  assert.equal(names(select({ files: ['examples/_shared/b4_interleave_core.rs'], maps: [row1, row2], targets, sources: includedReader, full: false })), 'tests/a.rs,tests/d.rs,tests/e.rs');
  // everything else is "all"
  for (const f of ['Cargo.toml', 'Cargo.lock', 'build.rs', '.cargo/config.toml', 'src/new_file.rs', 'tests/x.rs', 'crates/q/w/src/lib.rs', 'weird.bin']) {
    assert.match(names(sel(['src/n.rs', f])), /^ALL:/, f);
  }
  // Pin the proof paths independently of TOOL_ONLY so a missing entry fails.
  for (const f of [
    'src/kani_proofs.rs',
    'crates/core/sefer-registry/src/proofs/depot_overflow_proofs.rs',
    'crates/core/sefer-registry/src/proofs/depot_proofs.rs',
    'crates/core/sefer-registry/src/saturation_proofs.rs',
    'crates/core/sefer-segment/src/node_proofs.rs',
    'crates/core/sefer-segment/src/segment/segment_header/large_state_proofs.rs',
  ]) assert.equal(names(sel([f])), 'tests/e.rs', f);
  for (const f of [
    'crates/core/sefer-segment/src/lib.rs',
    'crates/core/sefer-registry/src/registry/heap_core/state/depot.rs',
  ]) assert.equal(sel([f]).mode, 'all', f);
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
  // Crate-graph narrowing (P1.5b): fixture graph over pure functions, no cargo call.
  const mkGraph = entries => {
    const g = { packages: new Map(), root: 'rootpkg' };
    for (const [n, deps, dir] of entries) g.packages.set(n, { dir: dir ?? `crates/t/${n}`, deps: new Set(deps), dependents: new Set() });
    for (const [n, p] of g.packages) for (const d of p.deps) g.packages.get(d)?.dependents.add(n);
    return g;
  };
  const soloGraph = mkGraph([['rootpkg', ['dep']], ['dep', []], ['standalone', []]]);
  const graphSel = (files, graph) => select({ files, maps: [row1, row2], targets, sources, full: false, crateGraph: graph });
  // (a) crate-only src change, root does not depend on the crate: file readers
  // stay selected (d reads files at runtime and may scan crates/**), e is
  // unmapped in every row; the reader rule reddens if removed.
  const crateSolo = graphSel(['crates/t/standalone/src/lib.rs'], soloGraph);
  assert.deepEqual(crateSolo, { mode: 'subset', reason: 'crate-only change: standalone (+0 dependents) does not affect the root crate', tests: ['tests/d.rs', 'tests/e.rs'], why: { 'tests/d.rs': 'reads files (crate-only change)', 'tests/e.rs': 'unmapped in every row' }, crateOnly: { crates: ['standalone'] } });
  // (b) root depends on the crate directly: unchanged conservative ALL
  assert.match(names(graphSel(['crates/t/dep/src/lib.rs'], soloGraph)), /^ALL:/);
  // (c) root -> dep -> standalone: affected set includes dep, root affected transitively
  const chainGraph = mkGraph([['rootpkg', ['dep']], ['dep', ['standalone']], ['standalone', []]]);
  assert.match(names(graphSel(['crates/t/standalone/src/other.rs'], chainGraph)), /^ALL:/);
  // (d) crate-only change plus an unrelated root test file change: tests/ rule wins
  assert.match(names(graphSel(['crates/t/standalone/src/lib.rs', 'tests/x.rs'], soloGraph)), /^ALL:/);
  // (e) crate change covered by the impact map: map attribution wins over the graph
  const mappedGraph = { ...soloGraph };
  const mapCoveringCrate = { tests: { ...row1.tests, 'tests/a.rs': [...row1.tests['tests/a.rs'], 'crates/t/standalone/src/lib.rs'] }, unmapped: row1.unmapped };
  assert.equal(names(select({ files: ['crates/t/standalone/src/lib.rs'], maps: [mapCoveringCrate, row2], targets, sources, full: false, crateGraph: mappedGraph })), 'tests/a.rs,tests/e.rs');
  // (f) no graph / unknown crate: legacy ALL (counterfactual guard for the new rule)
  assert.match(names(sel(['crates/q/w/src/lib.rs'])), /^ALL:/); // sel passes no crateGraph
  assert.match(names(select({ files: ['crates/t/standalone/src/lib.rs'], maps: [row1, row2], targets, sources, full: false, crateGraph: null })), /^ALL:/);
  const unknownGraph = mkGraph([['rootpkg', ['dep']], ['dep', []]]);
  assert.match(names(graphSel(['crates/t/standalone/src/lib.rs'], unknownGraph)), /^ALL:/); // crate unknown in graph
  // (g) crate's own Cargo.toml: crate-only, still picks the file readers;
  // root Cargo.toml: ALWAYS_ALL
  assert.deepEqual(graphSel(['crates/t/standalone/Cargo.toml'], soloGraph), { mode: 'subset', reason: 'crate-only change: standalone (+0 dependents) does not affect the root crate', tests: ['tests/d.rs', 'tests/e.rs'], why: { 'tests/d.rs': 'reads files (crate-only change)', 'tests/e.rs': 'unmapped in every row' }, crateOnly: { crates: ['standalone'] } });
  assert.match(names(graphSel(['Cargo.toml'], soloGraph)), /^ALL:/);
  // (j) allowed zero: a crate-only change with NO file readers and nothing
  // unmapped yields the empty subset; removing the no-reader guarantee reddens.
  const allMapped = { tests: { 'tests/a.rs': ['src/a.rs'], 'tests/e.rs': ['src/e.rs'] }, unmapped: {} };
  const noReaderTargets = [targets[0], targets[4]];
  const noReaderSources = { 'tests/a.rs': 'fn t(){}', 'tests/e.rs': 'fn t(){}' };
  assert.deepEqual(select({ files: ['crates/t/standalone/src/lib.rs'], maps: [allMapped], targets: noReaderTargets, sources: noReaderSources, full: false, crateGraph: soloGraph }), { mode: 'subset', reason: 'crate-only change: standalone (+0 dependents) does not affect the root crate', tests: [], why: {}, crateOnly: { crates: ['standalone'] } });
  // (h) dev-dependency edge: root test-only dep on the crate => root affected => ALL
  assert.match(names(graphSel(['crates/t/dep/src/lib.rs'], soloGraph)), /^ALL:/);
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
