// Crate-graph narrowing counterfactuals for the CI planner (chunk P1.5b).
// All fixture graphs are hand-built: no cargo metadata is invoked here, so the
// test stays cargo-free and deterministic (real-graph wiring is covered by the
// script selftests and `node scripts/ci/crate-graph.mjs --dump`).
import assert from 'node:assert/strict';
import { plan } from '../scripts/ci-plan.mjs';
import { graphFromMetadata, mergeRefEdges, scanReferenceEdges, affectedCrates, crateForPath, rootAffected } from '../scripts/ci/crate-graph.mjs';

const seam = new Set();
const CRATE_GROUPS_FIX = { mid: 'crate_mid', leaf: 'crate_leaf', standalone: 'crate_standalone', devdep: 'crate_devdep' };
// root -> mid -> leaf chain, plus a standalone leaf and a dev-dep provider.
const fixture = () => ({
  packages: new Map([
    ['sefer-alloc', { dir: '.', deps: new Set(['mid']), dependents: new Set() }],
    ['mid', { dir: 'crates/x/mid', deps: new Set(['leaf', 'devdep']), dependents: new Set(['sefer-alloc']) }],
    ['leaf', { dir: 'crates/x/leaf', deps: new Set(), dependents: new Set(['mid']) }],
    ['standalone', { dir: 'crates/x/standalone', deps: new Set(), dependents: new Set() }],
    ['devdep', { dir: 'crates/x/devdep', deps: new Set(), dependents: new Set(['mid']) }],
  ]),
  root: 'sefer-alloc',
});
const on = r => Object.keys(r).filter(k => r[k]).sort().join(',');
const pg = (files, graph, event = 'pull_request') =>
  on(plan({ files, event, seamTests: seam, crateGraph: graph, crateGroups: CRATE_GROUPS_FIX }));

const g = fixture();
// single changed crate selects itself only
assert.deepEqual([...affectedCrates(['standalone'], g)].sort(), ['standalone']);
// dependent chain: change leaf => leaf, mid, root affected
assert.deepEqual([...affectedCrates(['leaf'], g)].sort(), ['leaf', 'mid', 'sefer-alloc']);
assert.equal(rootAffected(affectedCrates(['leaf'], g), g), true, 'root affected via 2-step chain');
// root unaffected for a standalone change
assert.equal(rootAffected(affectedCrates(['standalone'], g), g), false);
// crateForPath: src file, manifest, negatives
assert.equal(crateForPath('crates/x/leaf/src/lib.rs', g), 'leaf');
assert.equal(crateForPath('crates/x/leaf/Cargo.toml', g), 'leaf');
assert.equal(crateForPath('crates/x/other/src/lib.rs', g), null);
assert.equal(crateForPath('src/lib.rs', g), null);

// (a) standalone leaf: fast stays on (root test jobs inside it are narrowed
// by select-tests), seam/os are suppressed without a root dependency.
assert.equal(pg(['crates/x/standalone/src/lib.rs'], g), 'always,crate_standalone,fast');
// counterfactual: the SAME file with a null graph keeps the blanket fast+seam,
// so removing the narrowing rule turns this pair of assertions red.
assert.equal(pg(['crates/x/standalone/src/lib.rs'], null), 'always,fast,seam');
// (b) crate the root depends on: fast/seam stay on.
assert.equal(pg(['crates/x/mid/src/lib.rs'], g), 'always,crate_mid,fast,seam');
// (c) leaf changed: both dependent groups on, root reached through mid.
assert.equal(pg(['crates/x/leaf/src/lib.rs'], g), 'always,crate_leaf,crate_mid,fast,seam');
// (d) dev-dependency edge pulls the dependent's group.
assert.equal(pg(['crates/x/devdep/src/lib.rs'], g), 'always,crate_devdep,crate_mid,fast,seam');
// (e) unknown crate dir with a graph: old conservative fast+seam.
assert.equal(pg(['crates/brand-new/src/lib.rs'], g), 'always,fast,seam');
// (f) null-graph regression guard: old behavior exactly (blanket fast+seam+own group).
assert.equal(pg(['crates/containers/sefer-region/src/lib.rs'], null), 'always,crate_sefer_region,fast,seam');
// (g) mixed change: crate + src => fast/seam regardless of the root.
assert.equal(pg(['crates/x/standalone/src/lib.rs', 'src/lib.rs'], g), 'always,crate_standalone,fast,seam');
// (h) root Cargo.lock keeps the full surface with a graph present.
assert.equal(pg(['Cargo.lock'], g), 'always,fast,os,seam');
// (i) push events still drop seam/os after narrowing.
assert.equal(pg(['crates/x/mid/src/lib.rs'], g, 'push'), 'always,crate_mid,fast');
// (j) HOLE 2 at the plan level: a reference edge alone (#[path]/include!) keeps
// seam/os on. The fixture root mirrors the post-mergeRefEdges state: the ref
// edge is merged into deps and remembered in refDeps.
const gRef = fixture();
gRef.packages.set('sefer-alloc', { dir: '.', deps: new Set(['mid', 'standalone']), dependents: new Set(), refDeps: new Set(['standalone']) });
gRef.packages.get('standalone').dependents.add('sefer-alloc');
assert.equal(pg(['crates/x/standalone/src/lib.rs'], gRef), 'always,crate_standalone,fast,seam');

// Counterfactual pair 1 (HOLE 1): declared optional dep must produce the edge
// even though a resolve-based reading (which the fixture fakes) lacks it.
const metaFix = () => ({
  workspace_root: '/repo',
  workspace_members: ['p-root', 'p-policy'],
  packages: [
    { id: 'p-root', name: 'sefer-alloc', manifest_path: '/repo/Cargo.toml',
      dependencies: [{ name: 'policy', optional: true }] },
    { id: 'p-policy', name: 'policy', manifest_path: '/repo/crates/t/policy/Cargo.toml', dependencies: [] },
  ],
  resolve: { nodes: [{ id: 'p-root', deps: [] }, { id: 'p-policy', deps: [] }] },
});
const gOpt = graphFromMetadata(metaFix());
assert.equal(rootAffected(affectedCrates(['policy'], gOpt), gOpt), true, 'optional declared dep reaches root');
const metaNoDeps = metaFix();
delete metaNoDeps.packages[0].dependencies; // resolve-only view: no edges at all
const gNoDeps = graphFromMetadata(metaNoDeps);
assert.deepEqual([...gNoDeps.packages.get('sefer-alloc').deps], []);
assert.equal(rootAffected(affectedCrates(['policy'], gNoDeps), gNoDeps), false, 'resolve-only counterfactual');

// Counterfactual pair 2 (HOLE 2): a root-test #[path] include of a member file.
const refG = fixture(); // root dir '.' like the real root crate
// standalone: no Cargo path to root, so only the ref edge can reach it
const withRefs = mergeRefEdges(structuredClone(refG),
  scanReferenceEdges([{ path: 'tests/loom_x.rs', text: '# [ path = "../crates/x/standalone/src/f.rs" ] mod f;' }], refG));
assert.equal(withRefs.packages.get('sefer-alloc').deps.has('standalone'), true);
assert.equal(rootAffected(affectedCrates(['standalone'], withRefs), withRefs), true, 'ref edge reaches root');
const bare = mergeRefEdges(structuredClone(refG), scanReferenceEdges([], refG));
assert.equal(rootAffected(affectedCrates(['standalone'], bare), bare), false, 'no-ref-edges counterfactual');
// include_str! variant and spaced include_bytes!
const strs = scanReferenceEdges([
  { path: 'tests/a.rs', text: 'const S: &str = include_str!("../crates/x/leaf/src/s.txt");' },
  { path: 'tests/b.rs', text: 'let b = include_bytes ! ("../crates/x/mid/src/b.bin");' },
], refG);
assert.deepEqual([...strs.get('sefer-alloc')].sort(), ['leaf', 'mid']);
console.log('crate_graph.test.mjs: PASS');
