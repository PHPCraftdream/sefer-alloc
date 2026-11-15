import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { test } from 'node:test';
import { fileURLToPath } from 'node:url';
// Relative imports (like tests/module_graph.test.mjs), so cwd never matters.
// All lint-logic cases run on synthetic trees in fresh mkdtemp dirs, removed
// in finally; the single CLI end-to-end case reads the real tree (cargo-free).
import { buildModuleGraph } from '../scripts/module-graph/graph.mjs';
import { loadBaseline, checkBackedges, emitBaseline } from '../scripts/module-graph/layering.mjs';

const check = (g, rows) => checkBackedges(g, rows ?? []);

const lineOf = (source, needle) => {
  const idx = source.indexOf(needle);
  if (idx < 0) throw new Error(`fixture needle not found: ${JSON.stringify(needle)}`);
  return source.slice(0, idx).split('\n').length;
};

// Writes `files` (posix rel paths under <tmp>/src/) plus <tmp>/layers.json,
// builds the graph with git:false and always removes the temp dir. Baseline
// files for the pure-function tests live in the same tmp dir.
async function withGraph(files, config, baseline) {
  const tmp = mkdtempSync(join(tmpdir(), 'layering-lint-test-'));
  try {
    const srcDir = join(tmp, 'src');
    for (const [rel, content] of Object.entries(files)) {
      const abs = join(srcDir, ...rel.split('/'));
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, content);
    }
    const configPath = join(tmp, 'layers.json');
    writeFileSync(configPath, JSON.stringify(config));
    const g = buildModuleGraph({ srcDir, configPath, git: false });
    const baselinePath = join(tmp, 'baseline.csv');
    if (baseline !== undefined) writeFileSync(baselinePath, baseline);
    // loadBaseline inside the try: the tmp dir is removed in finally, so the
    // caller must receive parsed rows rather than a dead path.
    return { g, rows: baseline !== undefined ? loadBaseline(baselinePath) : undefined };
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

const lowHighLayers = [
  { order: 1, name: 'low', modules: ['lower', 'root'] },
  { order: 2, name: 'high', modules: ['upper'] },
];

// Case 1: a back-edge missing from the baseline is a new-edge finding with
// both layer names/orders, the ref count and the real file:line site.
test('a back-edge absent from the baseline is a new-edge RED', async () => {
  const lower = 'use crate::upper::Thing;\n';
  const { g, rows } = await withGraph({
    'lib.rs': 'mod lower;\nmod upper;\n',
    'lower.rs': lower,
    'upper.rs': 'pub struct Thing;\n',
  }, { layers: lowHighLayers, boundaries: [], expected_back_edges: [] }, 'kind,path,count,reason\n');
  const findings = check(g, rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'new-edge');
  const m = findings[0].message;
  assert.ok(m.includes('lower -> upper'), m);
  assert.ok(m.includes('low@1'), m);
  assert.ok(m.includes('high@2'), m);
  assert.ok(m.includes('1 refs'), m);
  assert.ok(m.includes(`lower.rs:${lineOf(lower, 'use crate::upper')}`), m);
});

// Case 2: current count > baseline count is a growth finding.
test('a count growth beyond the baseline is a growth RED', async () => {
  const lower = 'use crate::upper::Thing;\n\npub fn go() {\n    crate::upper::helper();\n}\n';
  const { g, rows } = await withGraph({
    'lib.rs': 'mod lower;\nmod upper;\n',
    'lower.rs': lower,
    'upper.rs': 'pub struct Thing;\npub fn helper() {}\n',
  }, { layers: lowHighLayers, boundaries: [], expected_back_edges: [] }, 'kind,path,count,reason\nbackedge,lower->upper,1,seeded\n');
  const e = g.backEdges.find(x => x.source === 'lower' && x.target === 'upper');
  assert.ok(e);
  assert.equal(e.count, 2, 'use + qualified path merge into one edge of count 2');
  const findings = check(g, rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'growth');
  assert.ok(findings[0].message.includes('baseline 1 -> current 2'), findings[0].message);
});

// Case 3: a count drop is RED too — the baseline may only shrink via an
// explicit edit, so the message tells you to lower it.
test('a count drop without lowering the baseline is a ratchet RED with the lower-the-baseline hint', async () => {
  const lower = 'use crate::upper::Thing;\n';
  const { g, rows } = await withGraph({
    'lib.rs': 'mod lower;\nmod upper;\n',
    'lower.rs': lower,
    'upper.rs': 'pub struct Thing;\n',
  }, { layers: lowHighLayers, boundaries: [], expected_back_edges: [] }, 'kind,path,count,reason\nbackedge,lower->upper,3,seeded\n');
  const findings = check(g, rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'ratchet');
  const m = findings[0].message;
  assert.ok(m.includes('baseline 3 -> current 1'), m);
  assert.ok(m.includes('lower the baseline'), m);
});

// Case 4: current state exactly matching the baseline yields zero findings.
test('a clean graph within the baseline is OK (zero findings)', async () => {
  const { g, rows } = await withGraph({
    'lib.rs': 'mod lower;\nmod upper;\n',
    'lower.rs': 'use crate::upper::Thing;\n',
    'upper.rs': 'pub struct Thing;\n',
  }, { layers: lowHighLayers, boundaries: [], expected_back_edges: [] }, 'kind,path,count,reason\nbackedge,lower->upper,1,seeded\n');
  assert.deepEqual(check(g, rows), []);
});

// Case 5: a vanished edge with a stale baseline row is RED under the same
// ratchet rule, reported as current 0 with no sites section.
test('a vanished edge with a stale baseline row is a ratchet RED (current 0)', async () => {
  const { g, rows } = await withGraph({
    'lib.rs': 'mod lower;\nmod upper;\n',
    'lower.rs': 'pub struct Unused;\n',
    'upper.rs': 'pub struct Thing;\n',
  }, { layers: lowHighLayers, boundaries: [], expected_back_edges: [] }, 'kind,path,count,reason\nbackedge,lower->upper,3,seeded\n');
  const findings = check(g, rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'ratchet');
  const m = findings[0].message;
  assert.ok(m.includes('current 0'), m);
  assert.ok(!m.includes('sites:'), 'vanished edge has no sites section');
});

// Case 6: a declared conduit's own re-export edge is exempt from back-edge
// candidacy, and refs THROUGH it resolve to the defining module (same layer
// as the consumer here), so nothing is flagged.
test("a conduit module's own re-export edge is not a back-edge (declared conduit)", async () => {
  const files = {
    'lib.rs': 'mod hub;\nmod low;\nmod high;\n',
    'hub/mod.rs': 'pub use crate::high::Widget;\n',
    'low.rs': 'use crate::hub::Widget;\n',
    'high.rs': 'pub struct Widget;\n',
  };
  const config = {
    layers: [
      { order: 1, name: 'base', modules: ['hub'] },
      { order: 2, name: 'top', modules: ['low', 'high', 'root'] },
    ],
    boundaries: [],
    expected_back_edges: [],
    conduits: [{ modules: ['hub'], status: 'conduit' }],
  };
  const { g, rows } = await withGraph(files, config, 'kind,path,count,reason\n');
  assert.ok(g.edges.some(e => e.source === 'hub' && e.target === 'high'), 'the hub->high edge itself exists');
  assert.ok(!g.backEdges.some(e => e.source === 'hub' && e.target === 'high'),
    'hub->high is exempt from back-edge candidacy');
  assert.deepEqual(check(g, rows), []);
});

// Case 7 (counterfactual of 6): without the conduits declaration hub->high
// becomes a back-edge — the declaration is load-bearing.
test('counterfactual: without the conduit declaration hub->high is a new-edge', async () => {
  const { g, rows } = await withGraph({
    'lib.rs': 'mod hub;\nmod low;\nmod high;\n',
    'hub/mod.rs': 'pub use crate::high::Widget;\n',
    'low.rs': 'use crate::hub::Widget;\n',
    'high.rs': 'pub struct Widget;\n',
  }, {
    layers: [
      { order: 1, name: 'base', modules: ['hub'] },
      { order: 2, name: 'top', modules: ['low', 'high', 'root'] },
    ],
    boundaries: [],
    expected_back_edges: [],
  }, 'kind,path,count,reason\n');
  const findings = check(g, rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'new-edge');
  assert.ok(findings[0].message.includes('hub -> high'), findings[0].message);
});

// Case 8: refs THROUGH a conduit count against the DEFINING module's layer —
// low (order 1) referencing high (order 2) via the hub conduit is still a
// back-edge, attributed to low -> high.
test("a reference THROUGH a conduit is counted against the defining module's layer", async () => {
  const { g, rows } = await withGraph({
    'lib.rs': 'mod hub;\nmod low;\nmod high;\n',
    'hub/mod.rs': 'pub use crate::high::Widget;\n',
    'low.rs': 'use crate::hub::Widget;\n',
    'high.rs': 'pub struct Widget;\n',
  }, {
    layers: [
      { order: 1, name: 'base', modules: ['hub', 'low'] },
      { order: 2, name: 'top', modules: ['high', 'root'] },
    ],
    boundaries: [],
    expected_back_edges: [],
    conduits: [{ modules: ['hub'], status: 'conduit' }],
  }, 'kind,path,count,reason\n');
  const findings = check(g, rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'new-edge');
  const m = findings[0].message;
  assert.ok(m.includes('low -> high'), m);
  assert.ok(!m.includes('hub -> high'), 'the conduit itself is never the finding edge');
});

// Case 9: segment and route live in ONE merged layer until G1 — mutual refs
// inside it are fine; splitting them into two layers reds the upward edge.
test('segment_route: mutual references inside ONE layer produce no back-edge; splitting the layer reds them', async () => {
  const files = {
    'lib.rs': 'mod seg;\nmod rte;\n',
    'seg.rs': 'use crate::rte::Route;\n',
    'rte.rs': 'use crate::seg::Segment;\npub struct Route;\n',
  };
  const merged = await withGraph(files, {
    layers: [
      { order: 1, name: 'low', modules: ['root'] },
      { order: 4, name: 'segment_route', modules: ['seg', 'rte'] },
    ],
    boundaries: [],
    expected_back_edges: [],
  }, 'kind,path,count,reason\n');
  assert.deepEqual(check(merged.g, merged.rows), []);
  const split = await withGraph(files, {
    layers: [
      { order: 1, name: 'low', modules: ['root'] },
      { order: 4, name: 'segment', modules: ['seg'] },
      { order: 5, name: 'route', modules: ['rte'] },
    ],
    boundaries: [],
    expected_back_edges: [],
  }, 'kind,path,count,reason\n');
  const findings = check(split.g, split.rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'new-edge');
  assert.ok(findings[0].message.includes('seg -> rte'), findings[0].message);
});

// Case 10: a module in no layer and not a conduit is an unmapped finding;
// declaring it a conduit clears it.
test('a module in no layer and not a declared conduit is an unmapped RED', async () => {
  const files = { 'lib.rs': 'mod rogue;\n', 'rogue.rs': 'pub struct R;\n' };
  const base = { layers: [{ order: 1, name: 'low', modules: ['root'] }], boundaries: [], expected_back_edges: [] };
  const un = await withGraph(files, base, 'kind,path,count,reason\n');
  const findings = check(un.g, un.rows);
  assert.equal(findings.length, 1);
  assert.equal(findings[0].kind, 'unmapped');
  const m = findings[0].message;
  assert.ok(m.includes('rogue'), m);
  assert.ok(m.includes('declare it a conduit'), m);
  const con = await withGraph(files, { ...base, conduits: [{ modules: ['rogue'], status: 'conduit' }] }, 'kind,path,count,reason\n');
  assert.deepEqual(check(con.g, con.rows), []);
});

// Case 11: the CSV parser tolerates CRLF, `#` comments and quoted fields
// with commas and `""` escapes, and rejects malformed rows with 1-based
// line numbers.
test('loadBaseline parses CRLF, comments and quoted fields; throws on malformed rows', async () => {
  const tmp = mkdtempSync(join(tmpdir(), 'layering-lint-test-'));
  try {
    const path = join(tmp, 'baseline.csv');
    const good = '# committed ratchet baseline\r\n'
      + 'kind,path,count,reason\r\n'
      + 'backedge,lower->upper,1,plain reason\r\n'
      + 'backedge,a->b,2,"has, comma and ""escaped"" quotes"\r\n';
    writeFileSync(path, good);
    const rows = loadBaseline(path);
    assert.equal(rows.length, 2);
    assert.deepEqual(rows[1], { kind: 'backedge', edge: 'a->b', count: 2, reason: 'has, comma and "escaped" quotes' });

    const bad = (text) => {
      const p = join(tmp, 'bad.csv');
      writeFileSync(p, text);
      try { loadBaseline(p); return null; } catch (e) { return e; }
    };
    assert.match(bad('nope,not,a,header\nbackedge,a->b,1,\n').message, /line 1/);
    assert.match(bad('kind,path,count,reason\nedge,a->b,1,\n').message, /line 2/);
    assert.match(bad('kind,path,count,reason\nbackedge,a->b,x,\n').message, /line 2/);
    assert.match(bad('kind,path,count,reason\nbackedge,ab,1,\n').message, /line 2/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

// Case 12: the lint core is deterministic — two independent builds of the
// same tree give identical findings and identical emitBaseline output.
test('checkBackedges and emitBaseline are deterministic', async () => {
  const files = {
    'lib.rs': 'mod lower;\nmod upper;\n',
    'lower.rs': 'use crate::upper::Thing;\n',
    'upper.rs': 'pub struct Thing;\n',
  };
  const config = { layers: lowHighLayers, boundaries: [], expected_back_edges: [] };
  const a = await withGraph(files, config);
  const b = await withGraph(files, config);
  const fa = checkBackedges(a.g, []);
  const fb = checkBackedges(b.g, []);
  assert.equal(JSON.stringify(fa), JSON.stringify(fb));
  const ba = emitBaseline(a.g);
  assert.equal(ba, emitBaseline(b.g));
  assert.match(ba, /^kind,path,count,reason\n/);
  assert.match(ba, /backedge,lower->upper,1,\n/);
  assert.equal(ba.split('\n').length - 1, 2, 'header + one row');
  assert.ok(ba.endsWith('\n') && !ba.endsWith('\n\n'), 'LF endings, single trailing newline');
});

// Case 13 (the only real-tree test): the CLI ratchet is green against the
// committed baseline on the real src/ tree.
test('CLI --check-backedges is green on the real tree against the committed baseline', () => {
  const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
  const r = spawnSync(process.execPath, ['scripts/module-graph.mjs', '--check-backedges'], {
    cwd: repoRoot, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024,
  });
  assert.equal(r.status, 0, `exit ${r.status}, stderr: ${r.stderr}`);
  assert.ok(r.stdout.includes('layering lint: OK'), r.stdout);
});
