import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import { test } from 'node:test';
// Imports are relative to this file (like tests/j13b_schema.test.mjs), so the
// suite never depends on process.cwd(). Everything runs on synthetic trees in
// fresh mkdtemp dirs; no network, no cargo, no real-src scanning.
import { buildModuleGraph } from '../scripts/module-graph/graph.mjs';
import { lexRust } from '../scripts/module-graph/lexer.mjs';
import { parseRustFile } from '../scripts/module-graph/parse.mjs';

const edgeOf = (g, source, target) => g.edges.find(e => e.source === source && e.target === target);
const label = e => `${e.source}->${e.target}`;
const lineOf = (source, needle) => {
  const idx = source.indexOf(needle);
  if (idx < 0) throw new Error(`fixture needle not found: ${JSON.stringify(needle)}`);
  return source.slice(0, idx).split('\n').length;
};

// Writes `files` (posix rel paths under <tmp>/src/) plus <tmp>/layers.json,
// builds the graph with git:false (the temp dir is not a git repo; graph.mjs
// documents headSha/srcTreeSha as null in that mode) and always removes the
// temp dir, even when the build throws. Nested modules use mod.rs style:
// graph.mjs resolves a child of file F as dirname(F)/<name>.rs (or
// dirname(F)/<name>/mod.rs), so e.g. `mod b;` in a/mod.rs needs a/b.rs.
function withGraph(files, config = { layers: [], boundaries: [], expected_back_edges: [] }) {
  const tmp = mkdtempSync(join(tmpdir(), 'module-graph-test-'));
  try {
    const srcDir = join(tmp, 'src');
    for (const [rel, content] of Object.entries(files)) {
      // Cargo.toml belongs at the repo root: externCrateNames reads
      // dirname(srcRoot)/Cargo.toml to register extern crate names.
      const abs = rel === 'Cargo.toml' ? join(tmp, rel) : join(srcDir, ...rel.split('/'));
      mkdirSync(dirname(abs), { recursive: true });
      writeFileSync(abs, content);
    }
    const configPath = join(tmp, 'layers.json');
    writeFileSync(configPath, JSON.stringify(config));
    return buildModuleGraph({ srcDir, configPath, maxSites: 5, git: false });
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
}

// Case 1: `super::` resolves through the parent, for use items AND raw
// expression paths, merging into one counted edge to the sibling module.
test('super:: resolves through the parent for uses and expression paths', () => {
  const innerA = 'use super::inner_b::Widget;\n\npub fn go() {\n    super::inner_b::helper();\n}\n';
  const g = withGraph({
    'lib.rs': 'mod outer;\n',
    'outer/mod.rs': 'mod inner_a;\nmod inner_b;\n',
    'outer/inner_a.rs': innerA,
    'outer/inner_b.rs': 'pub struct Widget;\npub fn helper() {}\n',
  });
  assert.equal(g.edges.length, 1, 'only the inner_a -> inner_b dependency exists');
  const e = edgeOf(g, 'outer.inner_a', 'outer.inner_b');
  assert.ok(e, 'edge must target module outer.inner_b');
  assert.equal(e.count, 2, 'use + expression path merge into one edge of count 2');
  assert.deepEqual(e.sites.map(s => s.line), [
    lineOf(innerA, 'use super::inner_b'),
    lineOf(innerA, 'super::inner_b::helper'),
  ]);
  assert.ok(e.sites.every(s => s.file === 'outer/inner_a.rs'));
});

// Case 2: one grouped use -> two item references, identical file:line sites.
test('one grouped use yields two item references at the same file:line', () => {
  const consumer = 'use crate::a::{b, c};\n';
  const g = withGraph({
    'lib.rs': 'mod a;\nmod consumer;\n', 'consumer.rs': consumer,
    'a/mod.rs': 'pub mod b;\npub mod c;\n', 'a/b.rs': '', 'a/c.rs': '',
  });
  assert.equal(g.stats.uses, 1);
  assert.equal(g.stats.useItems, 2, 'grouped use counts both leaf items');
  assert.deepEqual(g.edges.map(label).sort(), ['consumer->a.b', 'consumer->a.c']);
  for (const e of g.edges) {
    assert.equal(e.count, 1);
    assert.deepEqual(e.sites.map(s => [s.file, s.line]), [['consumer.rs', lineOf(consumer, 'use crate::a::{b, c}')]]);
  }
});

// Case 3: nested groups flatten to fully qualified item paths.
test('nested use groups flatten to fully qualified item paths', () => {
  const consumer = 'use crate::a::{b::{c, d}, e};\n';
  const g = withGraph({
    'lib.rs': 'mod a;\nmod consumer;\n', 'consumer.rs': consumer,
    'a/mod.rs': 'pub mod b;\npub mod e;\n', 'a/e.rs': '',
    'a/b/mod.rs': 'pub mod c;\npub mod d;\n', 'a/b/c.rs': '', 'a/b/d.rs': '',
  });
  assert.equal(g.stats.useItems, 3);
  assert.deepEqual(g.edges.map(e => e.target).sort(), ['a.b.c', 'a.b.d', 'a.e']);
  assert.ok(g.edges.every(e => e.source === 'consumer' && e.count === 1
    && e.sites[0].line === lineOf(consumer, 'use crate::a')));
});

// Case 4: a `pub use` re-export chain resolves the consumer to the DEFINING
// module, not the re-exporting one; the re-export site is flagged.
test('re-export chains resolve the consumer to the defining module', () => {
  const consumer = 'use crate::outer::Thing;\n';
  const outerSrc = 'pub mod inner;\npub use inner::Thing;\n';
  const g = withGraph({
    'lib.rs': 'mod outer;\nmod consumer;\n', 'consumer.rs': consumer,
    'outer/mod.rs': outerSrc, 'outer/inner.rs': 'pub struct Thing;\n',
  });
  const use = edgeOf(g, 'consumer', 'outer.inner');
  assert.ok(use, 'consumer edge must target the defining module outer.inner');
  assert.equal(use.count, 1);
  assert.equal(use.sites[0].line, lineOf(consumer, 'use crate::outer::Thing'));
  assert.equal(edgeOf(g, 'consumer', 'outer'), undefined, 'no edge to the re-exporting module itself');
  const reexport = edgeOf(g, 'outer', 'outer.inner');
  assert.ok(reexport, 'the pub use itself is an edge outer -> outer.inner');
  assert.equal(reexport.sites[0].reexport, true);
  assert.equal(reexport.sites[0].line, lineOf(outerSrc, 'pub use inner::Thing'));
});

// Case 5: `as` renames resolve once; bare expression uses of the alias add no
// duplicate ref (unmarked bare paths are not scanned).
test('as-renamed imports resolve once and alias expressions add no duplicate edge', () => {
  const consumer = 'use crate::a::LongName as Short;\n\npub fn make() -> Short {\n    Short {}\n}\n';
  const g = withGraph({
    'lib.rs': 'mod a;\nmod consumer;\n',
    'a.rs': 'pub struct LongName;\n',
    'consumer.rs': consumer,
  });
  const parsed = parseRustFile({ raw: consumer, filePath: 'consumer.rs' });
  assert.equal(parsed.uses[0].items[0].rename, 'Short', 'parser records the rename');
  assert.equal(parsed.uses[0].items[0].name, 'LongName');
  assert.equal(g.edges.length, 1);
  const e = edgeOf(g, 'consumer', 'a');
  assert.ok(e, 'renamed import resolves to the defining module a');
  assert.equal(e.count, 1, 'bare Short in the body must not add a second ref');
  assert.equal(e.sites[0].line, lineOf(consumer, 'use crate::a::LongName'));
});

// Case 6a: a glob use is parsed but edge-less (graph.mjs skips glob items);
// the qualified `crate::a::Specific::new()` path carries the module edge.
test('glob imports make no edge themselves; the qualified path references the module', () => {
  const mSrc = 'use crate::a::*;\n\npub fn go() {\n    let _ = crate::a::Specific::new();\n}\n';
  const g = withGraph({ 'lib.rs': 'mod a;\nmod m;\n', 'a.rs': 'pub struct Specific;\n', 'm.rs': mSrc });
  assert.equal(g.stats.useItems, 1, 'the glob counts as one use item');
  assert.equal(g.edges.length, 1, 'glob use is edge-less; only the qualified path refs');
  const e = edgeOf(g, 'm', 'a');
  assert.ok(e);
  assert.equal(e.count, 1);
  assert.equal(e.sites[0].line, lineOf(mSrc, 'crate::a::Specific::new'));
  assert.equal(g.unresolvedRefs.length, 0);
});

// Case 6b (documented behavior): a bare unqualified name imported only via a
// glob is never scanned, so it yields neither an edge nor an unresolved ref.
test('a bare name imported only via a glob is not scanned and creates no edge', () => {
  const mSrc = 'use crate::a::*;\n\npub fn go() {\n    let _ = Specific::new();\n}\n';
  const g = withGraph({ 'lib.rs': 'mod a;\nmod m;\n', 'a.rs': 'pub struct Specific;\n', 'm.rs': mSrc });
  assert.equal(g.stats.uses, 1, 'the glob use itself is parsed');
  assert.equal(g.edges.length, 0, 'glob use and unmarked bare path produce no edge');
  assert.equal(g.unresolvedRefs.length, 0, 'bare paths are not path refs at all');
});

// Case 7: raw `crate::` paths in expression and type position edge without
// any use statement, with the correct line numbers.
test('crate:: paths in expressions and type positions edge without any use', () => {
  const mSrc = 'pub fn go() {\n    crate::deep::mod_x::func();\n    let v: crate::deep::mod_x::Struct = loop {};\n}\n';
  const g = withGraph({
    'lib.rs': 'mod m;\nmod deep;\n', 'm.rs': mSrc,
    'deep/mod.rs': 'pub mod mod_x;\n', 'deep/mod_x.rs': 'pub fn func() {}\npub struct Struct;\n',
  });
  assert.equal(g.stats.uses, 0);
  const e = edgeOf(g, 'm', 'deep.mod_x');
  assert.ok(e, 'both raw paths resolve to deep.mod_x');
  assert.equal(e.count, 2);
  assert.deepEqual(e.sites.map(s => s.line), [
    lineOf(mSrc, 'crate::deep::mod_x::func'),
    lineOf(mSrc, 'let v:'),
  ]);
});

// Case 8: cfg-gated uses mark the edge cfgOnly with the predicate; ungated
// uses leave cfgOnly false with an empty site cfg.
test('cfg-gated uses mark the edge cfgOnly while plain uses do not', () => {
  const mSrc = '#[cfg(feature = "fast")]\nuse crate::dep_a::Hot;\n\nuse crate::dep_b::Slow;\n';
  const g = withGraph({
    'lib.rs': 'mod m;\nmod dep_a;\nmod dep_b;\n', 'm.rs': mSrc,
    'dep_a.rs': 'pub struct Hot;\n', 'dep_b.rs': 'pub struct Slow;\n',
  });
  const hot = edgeOf(g, 'm', 'dep_a');
  assert.ok(hot);
  assert.equal(hot.cfgOnly, true);
  assert.deepEqual(hot.cfgs, ['feature = "fast"']);
  assert.equal(hot.sites[0].cfg, 'feature = "fast"');
  const slow = edgeOf(g, 'm', 'dep_b');
  assert.ok(slow);
  assert.equal(slow.cfgOnly, false);
  assert.deepEqual(slow.cfgs, ['']);
  assert.equal(slow.sites[0].cfg, '');
});

// Case 9: #[path] modules resolve to their custom file (relative to the
// declaring file's dir) and their internal refs work normally.
test('#[path] modules resolve to their custom file and keep working refs', () => {
  const location = 'use crate::util::Tool;\npub fn w() {}\n';
  const g = withGraph({
    'lib.rs': '#[path = "custom/location.rs"]\nmod weird;\nmod util;\n',
    'custom/location.rs': location,
    'util.rs': 'pub struct Tool;\n',
  });
  const weird = g.modules.find(m => m.path === 'weird');
  assert.ok(weird, 'module node weird exists');
  assert.equal(weird.file, 'custom/location.rs');
  assert.equal(weird.parent, 'root');
  const e = edgeOf(g, 'weird', 'util');
  assert.ok(e, 'refs inside the relocated file resolve normally');
  assert.equal(e.sites[0].line, lineOf(location, 'use crate::util::Tool'));
  assert.equal(g.phantom.length, 0);
});

// Case 10: back-edge = source layer order < target order (graph.mjs
// deviation 1, an upward dependency); the downward edge stays unflagged.
test('back-edges flag lower-order sources referencing higher-order targets only', () => {
  const g = withGraph({
    'lib.rs': 'mod base;\nmod upper;\n',
    'base.rs': 'use crate::upper::Thing;\n',
    'upper.rs': 'use crate::base::Helper;\n',
  }, {
    layers: [
      { order: 1, name: 'lower', modules: ['base'] },
      { order: 2, name: 'upper', modules: ['upper', 'root'] },
    ],
    boundaries: [],
    expected_back_edges: [],
  });
  assert.deepEqual(g.edges.map(label).sort(), ['base->upper', 'upper->base']);
  assert.equal(g.stats.backEdges, 1, 'exactly one back-edge');
  const back = g.backEdges[0];
  assert.equal(back.source, 'base', 'base (order 1) referencing upper (order 2) is the back-edge');
  assert.equal(back.target, 'upper');
  assert.deepEqual(back.sourceLayer, { order: 1, name: 'lower' });
  assert.deepEqual(back.targetLayer, { order: 2, name: 'upper' });
});

// Case 11: a module nobody references has no incoming edge but keeps its
// outgoing one.
test('a module nobody references has no incoming edge but keeps its outgoing one', () => {
  const g = withGraph({
    'lib.rs': 'mod lonely;\nmod other;\n',
    'lonely.rs': 'use crate::other::Util;\n',
    'other.rs': 'pub struct Util;\n',
  });
  assert.ok(edgeOf(g, 'lonely', 'other'), 'lonely keeps its outgoing edge');
  assert.ok(g.edges.every(e => e.target !== 'lonely' && !e.target.startsWith('lonely.')),
    'no edge target starts with lonely');
  assert.equal(g.stats.modules, 3);
});

// Case 12: doc and code comments never become edges but land in docRefs with
// file/line.
test('doc and code comments never become edges but land in docRefs', () => {
  const mSrc = '/// see crate::other::Thing\npub fn docned() {}\n\n// crate::other::Other\npub fn commented() {}\n';
  const g = withGraph({
    'lib.rs': 'mod m;\nmod other;\n', 'm.rs': mSrc,
    'other.rs': 'pub struct Thing;\npub struct Other;\n',
  });
  assert.equal(g.edges.length, 0, 'comment mentions must not produce edges');
  assert.equal(g.docRefs.length, 2);
  const doc = g.docRefs.find(d => d.text.includes('Thing'));
  assert.ok(doc);
  assert.equal(doc.module, 'm');
  assert.equal(doc.line, lineOf(mSrc, '/// see crate::other::Thing'));
  assert.equal(doc.target, 'other');
  const plain = g.docRefs.find(d => d.text.includes('Other'));
  assert.ok(plain);
  assert.equal(plain.line, lineOf(mSrc, '// crate::other::Other'));
  assert.equal(plain.target, 'other');
});

// Case 13: string literals and lifetimes are lexed correctly - the string
// content yields no edge, the 'static lifetime blanks nothing after it, and
// the real edge later in the file still resolves.
test('string literals and lifetimes are lexed so edges stay truthful', () => {
  const mSrc = 'pub fn go() {\n    let s = "crate::fake::Path";\n    let r: &\'static str = s;'
    + '\n    let c = \'x\';\n    let w = crate::real::Widget;\n}\n';
  const lexed = lexRust(mSrc);
  assert.equal(lexed.code.length, mSrc.length, 'lexer preserves length');
  assert.ok(lexed.code.includes("'static str = s;"), 'lifetime must not blank the following code');
  assert.equal((lexed.code.match(/'/g) ?? []).length, 1, "only the lifetime quote survives; 'x' is blanked");
  assert.ok(!lexed.code.includes('crate::fake'), 'string contents are blanked');
  const g = withGraph({ 'lib.rs': 'mod m;\nmod real;\n', 'real.rs': 'pub struct Widget;\n', 'm.rs': mSrc });
  assert.equal(g.edges.length, 1);
  const e = edgeOf(g, 'm', 'real');
  assert.ok(e, 'the real edge after the tricky literals still resolves');
  assert.equal(e.sites[0].line, lineOf(mSrc, 'crate::real::Widget'));
  assert.ok(!g.edges.some(x => x.target.includes('fake')), 'no edge to fake');
  assert.equal(g.unresolvedRefs.length, 0);
});

// Case 14: cfg-disjoint duplicate `mod` decls merge into ONE node carrying
// both predicates and both visibilities (graph.mjs mergeMods).
test('cfg-disjoint duplicate mod declarations merge into one node with both predicates', () => {
  const libSrc = '#[cfg(feature = "a")]\npub mod m;\n#[cfg(feature = "b")]\npub(crate) mod m;\nmod dep;\n';
  const g = withGraph({
    'lib.rs': libSrc, 'm.rs': 'use crate::dep::Tool;\n', 'dep.rs': 'pub struct Tool;\n',
  });
  const nodes = g.modules.filter(m => m.path === 'm');
  assert.equal(nodes.length, 1, 'one module node for m');
  assert.deepEqual(nodes[0].cfgs, ['feature = "a"', 'feature = "b"']);
  assert.deepEqual(nodes[0].vis, ['pub', 'pub(crate)']);
  assert.equal(nodes[0].file, 'm.rs');
  const e = edgeOf(g, 'm', 'dep');
  assert.ok(e);
  assert.equal(e.sites[0].cfg, 'feature = "a" && feature = "b"', 'sites carry the merged module cfg');
  assert.equal(e.cfgOnly, true);
  assert.equal(g.phantom.length, 0);
});

// Case 15: `pub(in crate::m)` visibility qualifiers are blanked before
// path-ref scanning and must not create an edge (regression).
test('pub(in ...) visibility qualifiers create no edge', () => {
  const g = withGraph({
    'lib.rs': 'mod m;\n', 'm/mod.rs': 'mod child;\n', 'm/child.rs': 'pub(in crate::m) fn f() {}\n',
  });
  assert.equal(g.edges.length, 0, 'the qualifier path must not become an edge');
  assert.equal(g.stats.pathRefs, 0);
  assert.equal(g.unresolvedRefs.length, 0);
  assert.ok(g.modules.some(m => m.path === 'm.child'), 'the child module itself still exists');
});

// Case 16: boundary visibility counts restricted decls used from outside the
// boundary crate; `pub` decls are not restricted and stay uncounted.
test('boundary visibility counts restricted decls used across, not pub ones', () => {
  const segSrc = 'mod inner;\npub use inner::Wide;\n';
  const innerSrc = 'pub(crate) struct Wide;\npub struct Public;\n';
  const appSrc = 'use crate::seg::inner::Wide;\n\nuse crate::seg::inner::Public;\n';
  const g = withGraph({
    'lib.rs': 'mod seg;\nmod app;\n',
    'seg/mod.rs': segSrc, 'seg/inner.rs': innerSrc, 'app.rs': appSrc,
  }, {
    layers: [],
    boundaries: [
      { crate: 'seg', prefixes: ['seg'] },
      { crate: 'app', prefixes: ['app', 'root'] },
    ],
    expected_back_edges: [],
  });
  const seg = g.boundaryVisibility.find(b => b.crate === 'seg');
  assert.ok(seg);
  assert.equal(seg.restrictedTotal, 1, 'only pub(crate) Wide is restricted inside seg');
  assert.equal(seg.usedAcrossCount, 1);
  assert.deepEqual(seg.items, [{
    name: 'Wide',
    declKind: 'struct',
    vis: 'pub(crate)',
    declFile: 'seg/inner.rs',
    declLine: lineOf(innerSrc, 'pub(crate) struct Wide'),
    users: [`app.rs:${lineOf(appSrc, 'use crate::seg::inner::Wide')}`],
    userCount: 1,
  }]);
  assert.ok(seg.items.every(i => i.name !== 'Public'), 'pub Public is not counted');
  assert.deepEqual(g.boundaryVisibility.find(b => b.crate === 'app'),
    { crate: 'app', restrictedTotal: 0, usedAcrossCount: 0, items: [] });
});

// Case 17: git:false identity is content-only (headSha/srcTreeSha null per
// graph.mjs) and rebuilding the same tree is fully deterministic.
test('git:false identity is content-only and rebuilds are deterministic', () => {
  const files = {
    'lib.rs': 'mod lonely;\nmod other;\n',
    'lonely.rs': 'use crate::other::Util;\n',
    'other.rs': 'pub struct Util;\n',
  };
  const g1 = withGraph(files);
  const g2 = withGraph(files);
  for (const g of [g1, g2]) {
    assert.equal(g.identity.headSha, null, 'git:false documents headSha null');
    assert.equal(g.identity.srcTreeSha, null);
    assert.match(g.identity.contentSha256, /^[0-9a-f]{64}$/, 'content hash populated');
    assert.equal(g.identity.fileCount, 3);
    assert.ok(g.identity.srcDir.endsWith('src'));
  }
  assert.equal(g1.identity.contentSha256, g2.identity.contentSha256, 'same bytes -> same content hash');
  assert.equal(JSON.stringify(g1.edges), JSON.stringify(g2.edges), 'edges arrays are identical');
  assert.deepEqual(g1.modules.map(m => m.path), ['lonely', 'other', 'root']);
});

// Case 18 (regression): a crate-rooted path resolving THROUGH a use-binding
// whose target leads with an extern crate name is an EXTERNAL reference - no
// edge (in particular no misclassified edge to the facade pseudo-module
// `root`), tallied as extern instead. A crate-internal control path still
// edges normally, proving the extern predicate did not over-fire.
test('extern-anchored bindings make pathRefs external, not root edges', () => {
  const ctrlSrc = 'pub fn go() {\n    crate::thing::helper();\n    crate::inner::f();\n}\n';
  const g = withGraph({
    'Cargo.toml': '[package]\nname = "fixture"\n\n[dependencies]\nfakeextern = "1.0"\n',
    'lib.rs': 'use fakeextern::thing;\nmod inner;\nmod ctrl;\n',
    'inner.rs': 'pub fn f() {}\n',
    'ctrl.rs': ctrlSrc,
    'thing.rs': 'pub fn helper() {}\n',
  });
  assert.equal(g.edges.length, 1, 'only the crate-internal control edge exists');
  const e = edgeOf(g, 'ctrl', 'inner');
  assert.ok(e, 'crate::inner::f() resolves to the crate-internal module inner');
  assert.equal(e.count, 1);
  assert.equal(e.sites[0].line, lineOf(ctrlSrc, 'crate::inner::f()'));
  assert.equal(edgeOf(g, 'ctrl', 'thing'), undefined,
    'crate::thing::helper() resolves through the extern-anchored binding: no internal edge');
  assert.ok(g.edges.every(x => x.source !== 'ctrl' || x.target === 'inner'),
    'no edge involving root from the extern pathRef');
  assert.equal(g.stats.externUses, 2,
    'the extern pathRef (via the extern-anchored binding) + the extern use item are tallied');
  assert.equal(g.unresolvedRefs.length, 0);
});

// Case: a glob re-export of an EXTERNAL crate module (`pub use ext::m::*;`)
// must not make the resolver recurse on itself (P1.4 sefer-platform conduit).
test('glob re-export of an extern crate module terminates and creates no edge', () => {
  const g = withGraph({
    'lib.rs': 'mod conduit;\nmod consumer;\n',
    'conduit.rs': 'pub(crate) use extern_crate::module::*;\n',
    'consumer.rs': 'use crate::conduit::Thing;\n\npub fn go() {\n    crate::conduit::helper();\n}\n',
  });
  assert.ok(edgeOf(g, 'consumer', 'conduit'), 'consumer -> conduit edge survives');
  assert.equal(g.edges.some(e => e.target === 'extern_crate' || e.source === 'extern_crate'), false);
});
