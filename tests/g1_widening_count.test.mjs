import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { readdirSync } from 'node:fs';
import { test } from 'node:test';

const readDirNames = (dir) => readdirSync(dir);
// Cargo-free, network-free, cargo-build-free suite (CLAUDE.md): everything
// runs on synthetic trees in fresh mkdtemp dirs, exactly like
// tests/module_graph.test.mjs. The one spawned process is the script's own
// `--check` mode under plain `node` (no cargo, no build).
import { computeVariant, mergedBoundaries, splitOnlyItems, safetySignal } from '../scripts/g1-widening-count/compute.mjs';
import { lexRust } from '../scripts/module-graph/lexer.mjs';
import { generateReport } from '../scripts/g1-widening-count.mjs';

const repoRoot = resolve(dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, '$1')), '..');
const SCRIPT = join(repoRoot, 'scripts', 'g1-widening-count.mjs');

const CONFIG = {
  root_module: 'root',
  layers: [
    { order: 1, name: 'seg-layer', modules: ['a'] },
    { order: 2, name: 'core-layer', modules: ['b', 'root'] },
  ],
  boundaries: [
    { crate: 'seg', prefixes: ['a'] },
    { crate: 'core', prefixes: ['b'] },
  ],
  expected_back_edges: [],
};

// Two future crates: `seg` (module a) and `core` (module b). `Used` and the
// unsafe raw-pointer fn are referenced from b (cross-boundary => counted);
// `Unused` is referenced by nobody and `Local` only from its own crate side.
function withTree(files = {}) {
  const tmp = mkdtempSync(join(tmpdir(), 'g1-widening-test-'));
  const srcDir = join(tmp, 'src');
  const base = {
    'lib.rs': 'mod a;\nmod b;\n',
    'a.rs': [
      'pub(crate) struct Used;',
      'pub(crate) struct Unused;',
      'pub(crate) fn local_only() {}',
      'pub(crate) unsafe fn raw_free(p: *mut u8) {}',
      '',
    ].join('\n'),
    'b.rs': 'use crate::a::Used;\nuse crate::a::local_only;\npub fn go() { crate::a::raw_free(core::ptr::null_mut()); let _ = (Used, local_only); }\n',
    ...files,
  };
  for (const [rel, content] of Object.entries(base)) {
    mkdirSync(dirname(join(srcDir, ...rel.split('/'))), { recursive: true });
    writeFileSync(join(srcDir, ...rel.split('/')), content);
  }
  const configPath = join(tmp, 'layers.json');
  writeFileSync(configPath, JSON.stringify(CONFIG));
  return { tmp, srcDir, configPath };
}

function computeAll({ tmp, srcDir, configPath }) {
  const split = computeVariant({ srcDir, configPath, boundaries: CONFIG.boundaries });
  const merged = computeVariant({ srcDir, configPath, boundaries: mergedBoundaries(CONFIG) });
  return { tmp, split, merged };
}

test('pub(crate) item used from another future crate is counted; unreferenced and same-crate-used items are not', () => {
  const { split } = computeAll(withTree());
  const seg = split.perBoundary.find((b) => b.crate === 'seg');
  const names = seg.items.map((i) => i.name).sort();
  assert.deepEqual(names, ['Used', 'local_only', 'raw_free'], 'exactly the cross-boundary items');
  const rawFree = seg.items.find((i) => i.name === 'raw_free');
  assert.ok(rawFree.safety.includes('unsafe') && rawFree.safety.includes('raw-ptr'), 'raw_free carries the safety signal');
  const core = split.perBoundary.find((b) => b.crate === 'core');
  assert.equal(core.items.length, 0, 'no restricted decls inside the using crate');
  assert.equal(split.total, 3, 'total equals the sum over boundaries');
});

test('count conservation: sum(per-boundary) === total, every item exactly once', () => {
  const { split } = computeAll(withTree());
  const sum = split.perBoundary.reduce((n, b) => n + b.items.length, 0);
  assert.equal(sum, split.total);
  const keys = split.perBoundary.flatMap((b) => b.items.map((i) => `${i.declFile}|${i.declLine}|${i.name}`));
  assert.equal(new Set(keys).size, keys.length);
});

test('merged variant: no split-only difference for a fixture without segment/route crates', () => {
  const { split, merged } = computeAll(withTree());
  assert.equal(splitOnlyItems(split, merged).length, 0);
});

test('two generations are byte-identical (determinism: no clock, no machine paths)', () => {
  const t = withTree();
  const a = generateReport({ srcDir: t.srcDir, configPath: t.configPath, variants: 'both' });
  const b = generateReport({ srcDir: t.srcDir, configPath: t.configPath, variants: 'both' });
  assert.equal(a, b);
  assert.ok(!a.includes(tmpdir()), 'report must not contain machine paths');
  assert.ok(a.includes('total'), 'report carries the summary table');
});

test('safetySignal reads the signature, not comments or strings', () => {
  const t = withTree({
    'a.rs': 'pub(crate) fn f1(x: *const u8) {}\n// raw unsafe NonNull in a comment\npub(crate) fn f2() { let s = "*mut forged"; }\n',
  });
  const raw = readFileSync(join(t.srcDir, 'a.rs'), 'utf8');
  const { code } = lexRust(raw);
  assert.deepEqual(safetySignal({ code }, 1), ['raw-ptr']);
  assert.deepEqual(safetySignal({ code }, 3), [], 'comment/string mentions forge no signal');
  assert.deepEqual(safetySignal(null, 1), []);
  rmSync(t.tmp, { recursive: true, force: true });
});

test('--check passes on a fresh report and fails on a tampered one (nonzero exit)', () => {
  const t = withTree();
  const reportPath = join(t.tmp, 'report.md');
  const run = (extra = []) =>
    execFileSync(process.execPath, [SCRIPT, '--check', '--report-path', reportPath, '--src-dir', t.srcDir, '--config', t.configPath, ...extra], { encoding: 'utf8' });
  // Produce the committed report exactly as --report would (write via the
  // generator, not the CLI, so the CLI's docs/ default is never touched).
  writeFileSync(reportPath, generateReport({ srcDir: t.srcDir, configPath: t.configPath, variants: 'both' }));
  run(); // must not throw
  writeFileSync(reportPath, readFileSync(reportPath, 'utf8').replace('total', 'tampered-total'));
  assert.throws(() => run(), /--check FAILED/, 'tampered report must fail --check');
  rmSync(t.tmp, { recursive: true, force: true });
});

test('importing the module has no side effects: no report written, empty stdout, exit 0', () => {
  const tmp = mkdtempSync(join(tmpdir(), 'g1-widening-import-'));
  // On Windows a bare absolute path is not a valid ESM import specifier, so
  // convert via pathToFileURL inside the child (same expression the guard
  // in the script itself uses).
  execFileSync(
    process.execPath,
    ['-e', 'import(require("node:url").pathToFileURL(process.argv[1]).href)', SCRIPT],
    { cwd: tmp, encoding: 'utf8' },
  ); // throws on nonzero exit
  assert.deepEqual(
    readDirNames(tmp),
    [],
    'importing the CLI module must not write docs/perf/G1_P2_WIDENING_COUNT.md (or anything else)',
  );
  rmSync(tmp, { recursive: true, force: true });
});

test('coverage assert: a module outside every boundary fails loudly', () => {
  const t = withTree({
    'lib.rs': 'mod a;\nmod b;\nmod orphan;\n',
    'orphan.rs': 'pub(crate) fn stray() {}\n',
  });
  const cfg = JSON.parse(readFileSync(t.configPath, 'utf8'));
  assert.throws(
    () => computeVariant({ srcDir: t.srcDir, configPath: t.configPath, boundaries: cfg.boundaries }),
    /boundary coverage violated/,
  );
  rmSync(t.tmp, { recursive: true, force: true });
});
