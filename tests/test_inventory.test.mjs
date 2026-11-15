// node --test suite for scripts/test-inventory.mjs (plan §3 gate).
//
// Uses --before-dir/--after-dir on synthetic trees under os.tmpdir().
// NO git usage, NO cargo, nothing written inside the repo.

import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
  extractInventory, parseMergedDecls, parseTestTables, runGate, render,
} from '../scripts/test-inventory.mjs';

let tmpRoots = [];
function makeTree(beforeFiles, afterFiles, { cargo = beforeFiles['Cargo.toml'] ?? '' } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'inv-gate-'));
  tmpRoots.push(root);
  for (const side of ['before', 'after']) {
    mkdirSync(join(root, side, 'tests', 'regression_merged'), { recursive: true });
  }
  writeFileSync(join(root, 'before', 'Cargo.toml'), cargo);
  writeFileSync(join(root, 'after', 'Cargo.toml'), cargo);
  const put = (side, rel, content) => {
    const p = join(root, side, rel);
    mkdirSync(join(p, '..'), { recursive: true });
    writeFileSync(p, content);
  };
  for (const [rel, c] of Object.entries(beforeFiles)) put('before', rel, c);
  for (const [rel, c] of Object.entries(afterFiles)) put('after', rel, c);
  return {
    opts: { ref: null, beforeDir: join(root, 'before'), afterDir: join(root, 'after'), mergedTarget: 'regression_merged', csv: null },
    root,
  };
}
test.after(() => {
  for (const r of tmpRoots) rmSync(r, { recursive: true, force: true });
});

const MERGED_DECL = (stem, cfgs = []) =>
  cfgs.map((c) => `#[cfg(${c})]\n`).join('') + `mod ${stem};\n`;

function okCase() {
  // one file, one #[test], one file-level cfg — moved cleanly
  return makeTree(
    {
      'Cargo.toml': '[package]\nname = "x"\n',
      'tests/regression_alpha.rs':
        '//! doc\n#![cfg(feature = "internals")]\n\n#[test]\nfn works() { assert_eq!(1, 1); }\n',
    },
    {
      'tests/regression_merged.rs': MERGED_DECL('regression_alpha', ['feature = "internals"']),
      'tests/regression_merged/regression_alpha.rs':
        '//! doc\n#![cfg(feature = "internals")]\n\n#[test]\nfn works() { assert_eq!(1, 1); }\n',
    },
  );
}

test('a. simple move OK => PASS (exit-clean result)', () => {
  const { opts } = okCase();
  const res = runGate(opts);
  const out = render(res);
  assert.match(out, /^PASS$/m);
  assert.equal(res.moved.length, 1);
  assert.equal(res.B, 1);
  assert.equal(res.A, 1);
});

test('b. RED: deleted #[test] fn in after-module => lost name in output, FAIL', () => {
  const t = okCase();
  writeFileSync(
    join(t.opts.afterDir, 'tests', 'regression_merged', 'regression_alpha.rs'),
    '//! doc\n#![cfg(feature = "internals")]\n\nfn works() { assert_eq!(1, 1); }\n',
  );
  const res = runGate(t.opts);
  const out = render(res);
  assert.match(out, /^FAIL$/m);
  assert.equal(res.problems.lost, 1);
  assert.match(out, /regression_alpha::works/);
});

test('c. RED: added fn in after-module => FAIL', () => {
  const t = okCase();
  writeFileSync(
    join(t.opts.afterDir, 'tests', 'regression_merged', 'regression_alpha.rs'),
    '//! doc\n#![cfg(feature = "internals")]\n\n#[test]\nfn works() {}\n#[test]\nfn extra() {}\n',
  );
  const res = runGate(t.opts);
  assert.equal(res.problems.added, 1);
  assert.match(render(res), /^FAIL$/m);
});

test('d. RED: mod decl loses its #[cfg] => FAIL (cfg-mismatch)', () => {
  const t = okCase();
  writeFileSync(
    join(t.opts.afterDir, 'tests', 'regression_merged.rs'),
    'mod regression_alpha;\n',
  );
  const res = runGate(t.opts);
  assert.equal(res.problems.cfgmismatch, 1);
  assert.match(render(res), /^FAIL$/m);
});

test('e. nested mod + cfg-gated inner fn preserved; flipping inner cfg => FAIL', () => {
  const mk = (innerCfg) => makeTree(
    {
      'Cargo.toml': '',
      'tests/regression_beta.rs': [
        '#![cfg(feature = "internals")]',
        'mod inner {',
        '  #[test]',
        '  fn deep() {}',
        '  #[cfg(feature = "fastbin")]',
        '  #[test]',
        '  fn gated() {}',
        '}',
        '#[test]',
        'fn top() {}',
      ].join('\n'),
    },
    {
      'tests/regression_merged.rs': MERGED_DECL('regression_beta', ['feature = "internals"']),
      'tests/regression_merged/regression_beta.rs': [
        '#![cfg(feature = "internals")]',
        'mod inner {',
        '  #[test]',
        '  fn deep() {}',
        `  #[cfg(${innerCfg})]`,
        '  #[test]',
        '  fn gated() {}',
        '}',
        '#[test]',
        'fn top() {}',
      ].join('\n'),
    },
  );
  const t = mk('feature = "fastbin"');
  let res = runGate(t.opts);
  let out = render(res);
  assert.equal(res.moved[0], 'regression_beta');
  assert.equal(res.B, 3);
  assert.match(out, /^PASS$/m);
  assert.equal(res.beforeInv.get('regression_beta').tests[1].path, 'inner::deep');
  assert.equal(res.beforeInv.get('regression_beta').tests[2].cfg.join(';'), 'cfg(feature="fastbin")');

  const t2 = mk('feature = "medium-classes"');
  res = runGate(t2.opts);
  assert.equal(res.problems.cfgmismatch + res.problems.lost + res.problems.added, 2);
  assert.match(render(res), /^FAIL$/m);
});

test('f. proptest! block property counted and preserved', () => {
  const body = (extra = '') => [
    '#![cfg(feature = "internals")]',
    'proptest! {',
    '#[test_case(1)]',
    'fn prop_roundtrip(n in 1u32..100) {',
    '    assert!(n > 0);' + extra,
    '}',
    '}',
  ].join('\n');
  const t = makeTree(
    { 'Cargo.toml': '', 'tests/regression_gamma.rs': body() },
    {
      'tests/regression_merged.rs': MERGED_DECL('regression_gamma', ['feature = "internals"']),
      'tests/regression_merged/regression_gamma.rs': body(),
    },
  );
  const res = runGate(t.opts);
  assert.equal(res.B, 1);
  assert.equal(res.beforeInv.get('regression_gamma').tests[0].path, 'prop_roundtrip');
  assert.match(render(res), /^PASS$/m);
});

test('g. orphan module file not declared by merged target => FAIL', () => {
  const t = okCase();
  writeFileSync(
    join(t.opts.afterDir, 'tests', 'regression_merged', 'regression_orphan.rs'),
    '#[test]\nfn lonely() {}\n',
  );
  const res = runGate(t.opts);
  assert.equal(res.problems.orphans, 1);
  assert.match(render(res), /^FAIL$/m);
});

test('h. unchanged collateral stem with a changed test => FAIL', () => {
  const t = makeTree(
    {
      'Cargo.toml': '',
      'tests/regression_alpha.rs': '#[test]\nfn a1() {}\n',
      'tests/collateral.rs': '#[test]\nfn c1() {}\n#[test]\nfn c2() {}\n',
    },
    {
      'tests/regression_merged.rs': MERGED_DECL('regression_alpha'),
      'tests/regression_merged/regression_alpha.rs': '#[test]\nfn a1() {}\n',
      'tests/collateral.rs': '#[test]\nfn c1() {}\n',
    },
  );
  const res = runGate(t.opts);
  assert.equal(res.problems.lost, 1);
  assert.ok(res.rows.some((r) => r.stem === 'collateral' && r.status === 'COLLATERAL-CHANGED'));
  assert.match(render(res), /^FAIL$/m);
});

test('i. two stacked file-level cfgs preserved (multi-cfg file)', () => {
  const mk = (cfgs) => makeTree(
    {
      'Cargo.toml': '',
      'tests/regression_r2_04_like.rs': [
        '#![allow(deprecated)]',
        '#![cfg(feature = "experimental")]',
        '#![cfg(feature = "internals")]',
        '#[test]',
        'fn locked() {}',
      ].join('\n'),
    },
    {
      'tests/regression_merged.rs': MERGED_DECL('regression_r2_04_like', cfgs),
      'tests/regression_merged/regression_r2_04_like.rs': [
        '#![allow(deprecated)]',
        '#![cfg(feature = "experimental")]',
        '#![cfg(feature = "internals")]',
        '#[test]',
        'fn locked() {}',
      ].join('\n'),
    },
  );
  const t = mk(['feature = "experimental"', 'feature = "internals"']);
  let res = runGate(t.opts);
  assert.deepEqual(res.beforeInv.get('regression_r2_04_like').fileCfgs, ['cfg(feature="experimental")', 'cfg(feature="internals")']);
  assert.match(render(res), /^PASS$/m);
  // order flipped => mismatch
  const t2 = mk(['feature = "internals"', 'feature = "experimental"']);
  res = runGate(t2.opts);
  assert.equal(res.problems.cfgmismatch, 1);
  assert.match(render(res), /^FAIL$/m);
});

test('merged target with direct #[test] => FAIL', () => {
  const t = okCase();
  writeFileSync(
    join(t.opts.afterDir, 'tests', 'regression_merged.rs'),
    MERGED_DECL('regression_alpha', ['feature = "internals"']) + '\n#[test]\nfn sneaky() {}\n',
  );
  const res = runGate(t.opts);
  assert.equal(res.mergedDirect, 1);
  assert.match(render(res), /^FAIL$/m);
});

test('test=false [[test]] table excludes a flat file from targets', () => {
  const t = makeTree(
    {
      'Cargo.toml': '[[test]]\nname = "miri_thing"\nharness = false\ntest = false\nrequired-features = ["alloc-global"]\n',
      'tests/miri_thing.rs': 'fn main() {}\n',
      'tests/regression_alpha.rs': '#[test]\nfn a1() {}\n',
    },
    {
      'tests/regression_merged.rs': MERGED_DECL('regression_alpha'),
      'tests/regression_merged/regression_alpha.rs': '#[test]\nfn a1() {}\n',
      'tests/miri_thing.rs': 'fn main() {}\n',
    },
  );
  const res = runGate(t.opts);
  assert.match(render(res), /^PASS$/m);
});

// ---------------- unit tests for the pure helpers ----------------

test('extractInventory: comments, strings, char literals and lifetimes', () => {
  const src = [
    '// #[test] fn not_real() {}',
    '/* #[test] fn also_not() {} */',
    'const S: &str = "#[test] fn in_string() {}";',
    "const C: char = '#';",
    "static L: &'static str = \"x\";",
    '#[test]',
    'fn real() {}',
    'fn helper<\'a>(x: &\'a str) {}',
  ].join('\n');
  const inv = extractInventory(src);
  assert.deepEqual(inv.tests.map((t) => t.path), ['real']);
  assert.deepEqual(inv.fileCfgs, []);
});

test('extractInventory: cfg attrs between #[test] and fn collected in order', () => {
  const src = [
    'mod m {',
    '  #[cfg(feature = "a")]',
    '  #[test]',
    '  #[cfg_attr(miri, ignore)]',
    '  #[allow(dead_code)]',
    '  fn t() {}',
    '}',
  ].join('\n');
  const inv = extractInventory(src);
  assert.equal(inv.tests.length, 1);
  assert.deepEqual(inv.tests[0].cfg, ['cfg(feature="a")', 'cfg_attr(miri,ignore)']);
});

test('extractInventory: file-level cfg_attr counted as file cfg', () => {
  const inv = extractInventory('#![cfg_attr(miri, ignore)]\n#![cfg(feature = "x")]\n#[test]\nfn t() {}\n');
  assert.deepEqual(inv.fileCfgs, ['cfg_attr(miri,ignore)', 'cfg(feature="x")']);
});

test('parseMergedDecls: stacked cfgs and bare decls', () => {
  const decls = parseMergedDecls(
    '//! doc\n#[cfg(feature = "a")]\n#[cfg(feature = "b")]\nmod two_cfg;\nmod bare;\n',
  );
  assert.deepEqual(decls.get('two_cfg'), ['cfg(feature="a")', 'cfg(feature="b")']);
  assert.deepEqual(decls.get('bare'), []);
});

test('parseTestTables: minimal [[test]] fields', () => {
  const tables = parseTestTables([
    '[package]',
    'name = "x" # comment',
    '[[test]]',
    'name = "con1"',
    'harness = false',
    '[[test]]',
    'name = "off"',
    'test = false',
    'path = "tests/off.rs"',
  ].join('\n'));
  assert.equal(tables.length, 2);
  assert.equal(tables[0].harness, false);
  assert.equal(tables[1].test, false);
  assert.equal(tables[1].path, 'tests/off.rs');
});
