// Cargo-free intra-crate restructuring counterfactuals shared by node:test and
// the runner self-test. The evidence integration uses a fake read-only Git adapter.
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import {
  deriveLayerRestructuringEvidence,
  hasLayerRestructuringAttempt,
  layerRestructuringPolicy,
  zeroCostVerdict,
} from '../scripts/ghost_diff.mjs';

const DECLARED = ['refactor: reorganize layers\n\nRestructuring: layers'];
const BASE = 'kind,path,count,reason\nbackedge,a->b,5,one\nbackedge,c->d,2,two\n';
const LOWER = 'kind,path,count,reason\nbackedge,a->b,4,one\nbackedge,c->d,2,two\n';
const LOWER_EDGES = 'kind,path,count,reason\nbackedge,a->b,5,one\n';
const EMPTY = 'kind,path,count,reason\n';
const RENAME = { status: 'R096', from: 'src/old.rs', to: 'src/new.rs', sourceAbsent: true, destinationPresent: true };
const changed = () => new Map([
  ['docs/layering_baseline.csv', ['backedge,a->b,5,one', 'backedge,a->b,4,one']],
  ['src/old.rs', ['pub fn moved() {}']],
  ['src/new.rs', ['pub fn moved() {}']],
]);
const evidence = (baseBaselineText = BASE, headBaselineText = LOWER, renames = [RENAME], renameHunks = new Map([['src/new.rs', ['pub fn moved() {}']]])) => ({ renames, renameHunks, baseBaselineText, headBaselineText });

export function checkLayerRestructuringPolicyCases() {
  let count = 0;
  const check = (_name, fn) => { fn(); count++; };
  const allowed = layerRestructuringPolicy(DECLARED, changed(), [], evidence());
  assert.deepEqual(allowed, {
    mode: 'layers', irTolerance: 0.001,
    reason: 'declared intra-crate layer restructuring with a strictly non-increasing, improving A1 baseline',
  });

  check('strict declaration and same-range baseline reduction', () => {
    assert.notEqual(allowed, null);
    assert.equal(layerRestructuringPolicy(['refactor: move layers'], changed(), [], evidence()), null);
    assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, BASE)), null);
    assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, BASE),), null);
    assert.equal(layerRestructuringPolicy(DECLARED, new Map([['src/old.rs', []]]), [], evidence()), null);
  });
  check('baseline metrics cannot grow and a zero-row baseline is valid', () => {
    const edgeGrowth = 'kind,path,count,reason\nbackedge,a->b,6,one\nbackedge,c->d,2,two\n';
    const totalGrowth = 'kind,path,count,reason\nbackedge,a->b,8,one\n';
    assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, edgeGrowth)), null);
    assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, totalGrowth)), null);
    assert.notEqual(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, EMPTY)), null);
    assert.notEqual(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, LOWER_EDGES)), null);
  });
  check('malformed, duplicate, zero and unsafe-count baselines fail closed', () => {
    for (const bad of [
      'kind,path,count,reason\nbackedge,a->b,bad,x\n',
      'kind,path,count,reason\nbackedge,a->b,1,x\nbackedge,a->b,1,y\n',
      'kind,path,count,reason\nbackedge,a->b,0,x\n',
      `kind,path,count,reason\nbackedge,a->b,${Number.MAX_SAFE_INTEGER},x\nbackedge,c->d,1,y\n`,
      'wrong,header,fields,here\n',
    ]) assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, bad)), null);
  });
  check('non-renamed changed src files are allowed and ghost-checked on full changed lines', () => {
    const withNonRenamed = new Map([...changed(), ['src/global/fallback.rs', ['let registered = true;']]]);
    assert.notEqual(layerRestructuringPolicy(DECLARED, withNonRenamed, [], evidence()), null);
    for (const line of ['crate::ghost::ghost_event!("x", 0, 1);', '#[cfg(sefer_ghost)]']) {
      const ghost = new Map([...changed(), ['src/global/fallback.rs', [line]]]);
      assert.equal(layerRestructuringPolicy(DECLARED, ghost, [], evidence()), null, line);
    }
  });
  check('ordinary body prose and trailer-shaped lines do not block admission', () => {
    assert.notEqual(layerRestructuringPolicy(['refactor: reorg\n\nNote: moved layers around\n\nRestructuring: layers'], changed(), [], evidence()), null);
    assert.equal(layerRestructuringPolicy(['refactor: reorg\n\nRestructuring: layers\n\nExtraction: sefer-x'], changed(), [], evidence()), null);
  });
  check('wrapped lowercase prose `extraction:` line does not block admission', () => {
    assert.notEqual(layerRestructuringPolicy(['build: follow up\n\nWrapped sentence ends with\nextraction:\n\nMore prose.\n\nRestructuring: layers'], changed(), [], evidence()), null);
  });
  check('only the exact standalone trailer authorizes mode', () => {
    assert.notEqual(layerRestructuringPolicy(['refactor: reorg\n\nRestructuring: layers\nCo-Authored-By: Example'], changed(), [], evidence()), null);
    for (const message of [
      'refactor: reorg\n\nRestructuring: layer',
      'refactor: reorg\n\nRestructuring: layers extra',
      'Restructuring: layers',
      'refactor: reorg\n\nRestructuring: layers\n\nRestructuring: layers',
      'refactor: reorg\n\nRestructuring: layers\nRestructuring: other',
      'refactor: reorg\n\nRestructuring: layers\n\nExtraction: sefer-x',
    ]) assert.equal(layerRestructuringPolicy([message], changed(), [], evidence()), null);
  });
  check('dirty, no baseline edit, Cargo/build graph and toolchain paths are forbidden', () => {
    assert.equal(layerRestructuringPolicy(DECLARED, changed(), [' M src/old.rs'], evidence()), null);
    const noBaseEdit = changed(); noBaseEdit.delete('docs/layering_baseline.csv');
    assert.equal(layerRestructuringPolicy(DECLARED, noBaseEdit, [], evidence()), null);
    for (const path of ['Cargo.toml', 'Cargo.lock', 'crates/a/Cargo.toml', 'build.rs', 'src/a/build.rs', '.cargo/config.toml', 'rust-toolchain', 'rust-toolchain.toml']) {
      assert.equal(layerRestructuringPolicy(DECLARED, new Map([...changed(), [path, ['x']]]), [], evidence()), null, path);
    }
  });
  check('positive R similarity and actual head-path evidence are required; copies do not substitute', () => {
    for (const bad of [
      [{ ...RENAME, status: 'R0' }],
      [{ ...RENAME, status: 'C096' }],
      [{ ...RENAME, to: 'src/old.rs' }],
      [{ ...RENAME, from: 'crates/a.rs' }],
      [{ ...RENAME, to: 'src/new.txt' }],
      [{ ...RENAME, sourceAbsent: false }],
      [{ ...RENAME, destinationPresent: false }],
      [RENAME, RENAME],
    ]) assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, LOWER, bad)), null);
    const copiesOnly = evidence(BASE, LOWER, [{ ...RENAME, status: 'C096' }]);
    assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], copiesOnly), null);
  });
  check('multiple real renames are admitted and edit hunks are ghost-checked', () => {
    const second = { ...RENAME, from: 'src/other.rs', to: 'src/next.rs' };
    const multiChanges = new Map([...changed(), ['src/other.rs', ['pub fn moved() {}']], ['src/next.rs', ['pub fn moved() {}']]]);
    const hunks = new Map([['src/new.rs', ['pub fn moved() {}']], ['src/next.rs', ['pub fn moved() {}']]]);
    assert.notEqual(layerRestructuringPolicy(DECLARED, multiChanges, [], evidence(BASE, LOWER, [RENAME, second], hunks)), null);
    for (const line of ['ghost_assert!(true);', '#[cfg(sefer_ghost)]', '#[cfg(any(loom, miri, kani, sefer_ghost))]']) {
      assert.equal(layerRestructuringPolicy(DECLARED, changed(), [], evidence(BASE, LOWER, [RENAME], new Map([['src/new.rs', [line]]]))), null, line);
    }
    assert.equal(layerRestructuringPolicy(DECLARED, new Map([...changed(), ['src/ghost/new.rs', ['pub fn x() {}']]]), [], evidence()), null);
  });
  check('failed layer declaration with runtime subject must not turn into a skip', () => {
    const message = 'perf(runtime): reorganize\n\nRestructuring: layers';
    assert.equal(hasLayerRestructuringAttempt(message), true);
    assert.equal(layerRestructuringPolicy([message], changed(), [], evidence(BASE, BASE)), null);
    assert.ok(readFileSync(new URL('../scripts/ghost_zero_cost.mjs', import.meta.url), 'utf8').includes('if (reason && !proofPolicy && !layerAttempt)'));
    assert.ok(readFileSync(new URL('../scripts/ghost_zero_cost.mjs', import.meta.url), 'utf8').includes('const prefix = layerAttempt ? null : declaredRuntimePrefix'));
  });
  check('bounded layer proof permits inclusive ±0.1%, rejects beyond and new ghost tokens', () => {
    const checkIr = (irHead, ghostTokens = []) => zeroCostVerdict({ policy: allowed, textSame: false, irBase: 1_000_000, irHead, ghostTokens });
    assert.equal(checkIr(1_001_000).ok, true);
    assert.equal(checkIr(999_000).ok, true);
    assert.equal(checkIr(1_001_001).ok, false);
    assert.equal(checkIr(998_999).ok, false);
    assert.equal(checkIr(1_000_000, ['ghost_event']).ok, false);
  });
  check('runner evidence is derived from base..head by injected read-only Git adapter', () => {
    const calls = [];
    const git = (args) => {
      calls.push(args);
      if (args[0] === 'diff' && args.includes('--name-status')) return 'R096\tsrc/old.rs\tsrc/new.rs\nR088\tsrc/other.rs\tsrc/next.rs\n';
      if (args[0] === 'cat-file' && /:src\/(?:old|other)\.rs$/.test(args[2])) throw new Error('source absent in head');
      if (args[0] === 'cat-file' && /:src\/(?:new|next)\.rs$/.test(args[2])) return '';
      if (args[0] === 'diff' && args.includes('-U0')) return 'diff --git a/src/old.rs b/src/new.rs\n@@ -1 +1 @@\n-old\n+new\n';
      if (args[0] === 'show' && args[1] === 'base:docs/layering_baseline.csv') return BASE;
      if (args[0] === 'show' && args[1] === 'head:docs/layering_baseline.csv') return LOWER;
      throw new Error(`unexpected read-only git call ${args.join(' ')}`);
    };
    const rangeChanges = new Map([...changed(), ['src/other.rs', ['pub fn moved() {}']], ['src/next.rs', ['pub fn moved() {}']]]);
    const derived = deriveLayerRestructuringEvidence({ base: 'base', head: 'head', git });
    assert.equal(derived.renames.length, 2);
    assert.notEqual(layerRestructuringPolicy(DECLARED, rangeChanges, [], derived), null);
    assert.ok(calls.some((args) => args.includes('--find-renames=50%')));
    assert.ok(calls.some((args) => args.includes('base') && args.includes('head')));
    for (const endpoint of ['base:docs/layering_baseline.csv', 'head:docs/layering_baseline.csv']) {
      assert.ok(calls.some((args) => args.includes(endpoint)), endpoint);
    }
  });
  return count;
}
