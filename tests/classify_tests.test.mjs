// Test suite for scripts/classify-tests.mjs + scripts/classify-tests/{toml,
// discover,features,pins,groups,report}.mjs.
//
// Strategy: ground truth by construction. Most tests build synthetic trees in
// mkdtempSync(tmpdir)/classify-tests-* dirs (removed in after/finally) and
// assert against the modules' own documented outputs. The final block is a
// CLI smoke test against THIS repo via spawnSync — those assertions are
// DATA-PINNED: they pin today's measured values; if tests/ changes, the
// numbers legitimately change. Internal-consistency assertions (coverage,
// CSV row count) hold regardless.
//
// Node >= 24, ESM, zero npm dependencies. No cargo, no network, no writes
// outside os.tmpdir().

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { parseToml, packageValue } from '../scripts/classify-tests/toml.mjs';
import { discoverTargets, explicitTestTargets } from '../scripts/classify-tests/discover.mjs';
import {
  classifySource, cfgFacts, deriveFirstConfigWinMarkers, fileLevelInnerAttrs,
  FIRST_CONFIG_WIN_CANDIDATES, listSrcRustFiles, markerRegex,
  rawMatchZone, stripComments, stripCommentsKeepStrings,
} from '../scripts/classify-tests/features.mjs';
import {
  collectPinSurfaces, scanAllPins, scanJ13HashPins, scanPins, scanPublishedCounts,
} from '../scripts/classify-tests/pins.mjs';
import {
  assertGroupInvariants, buildGroups, groupKey, intersections, mergeability,
  pilotCandidate, relaxationLadder, themePrefix,
} from '../scripts/classify-tests/groups.mjs';
import { PLAN_HYPOTHESES, renderReport } from '../scripts/classify-tests/report.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const HEX64 = 'a'.repeat(64);

/** Create a fresh temp workspace dir with optional relative-path files. */
function makeTree(files = {}) {
  const dir = mkdtempSync(join(tmpdir(), 'classify-tests-'));
  for (const [rel, content] of Object.entries(files)) {
    const abs = join(dir, rel);
    mkdirSync(dirname(abs), { recursive: true });
    writeFileSync(abs, content);
  }
  return dir;
}

/** Minimal synthetic manifest doc for discoverTargets. */
function manifestOf(tomlText) {
  return parseToml(tomlText || '[package]\nname = "synth"\n');
}

// ---------------------------------------------------------------- 1. toml.mjs

test('parseToml: plain tables, booleans, quoted keys', () => {
  const doc = parseToml([
    '[package]',
    'name = "x"',
    'autotests = true',
    '',
    "[target.'cfg(loom)'.dev-dependencies]",
    'loom = "0.3"',
  ].join('\n'));
  const pkg = doc.sections.find((s) => s.kind === 'table' && s.path[0] === 'package');
  assert.equal(pkg.values.autotests, true);
  assert.equal(pkg.values.name, 'x');
  const tgt = doc.sections.find((s) => s.path.join('.') === 'target.cfg(loom).dev-dependencies');
  assert.equal(tgt.values.loom, '0.3');
});

test('parseToml: [[test]] array tables accumulate', () => {
  const doc = parseToml([
    '[[test]]',
    'name = "a"',
    'harness = false',
    '[[test]]',
    'name = "b"',
    'test = false',
  ].join('\n'));
  const tests = doc.sections.filter((s) => s.kind === 'array' && s.path[0] === 'test');
  assert.equal(tests.length, 2);
  assert.deepEqual(tests.map((t) => t.values.name), ['a', 'b']);
  assert.equal(tests[0].values.harness, false);
});

test('parseToml: multi-line arrays and comments', () => {
  const doc = parseToml([
    '# full-line comment',
    '[package]',
    'autotests = true # trailing comment',
    'features = [',
    '  "alloc-core", # inside array',
    '  "std",',
    ']',
  ].join('\n'));
  const pkg = doc.sections.find((s) => s.path[0] === 'package');
  assert.equal(pkg.values.autotests, true);
  assert.deepEqual(pkg.values.features, ['alloc-core', 'std']);
});

test('parseToml: throws on unbalanced input', () => {
  assert.throws(() => parseToml('[package\nname = "x"'), /unparsable/);
  assert.throws(() => parseToml('key = [1, 2'), /unbalanced|unparsable/);
  assert.throws(() => parseToml('garbage line without equals'), /unparsable/);
});

test("packageValue reads [package] autotests (present and absent)", () => {
  const present = parseToml('[package]\nautotests = false\n');
  assert.equal(packageValue(present, 'autotests', true), false);
  const absent = parseToml('[package]\nname = "x"\n');
  assert.equal(packageValue(absent, 'autotests', true), true);
  assert.equal(packageValue(absent, 'autotests', false), false);
});

// ------------------------------------------------------------ 2. discover.mjs

test('discover: depth-1 tests/<dir>/main.rs is a target; deeper main.rs is not', () => {
  const dir = makeTree({
    'Cargo.toml': '[package]\nname = "synth"\n',
    'tests/mydir/main.rs': 'fn main() {}\n',
    'tests/compile_fail/case1/src/main.rs': 'fn main() {}\n',
    'tests/compile_fail/case1/src/lib.rs': 'pub fn f() {}\n',
  });
  const { targets, nonTargets } = discoverTargets(dir, manifestOf());
  const names = targets.map((t) => t.name);
  assert.deepEqual(names, ['mydir']);
  assert.equal(targets[0].path, 'tests/mydir/main.rs');
  assert.equal(targets[0].declared_explicit, false);
  const kinds = Object.fromEntries(nonTargets.map((n) => [n.path, n.kind]));
  assert.equal(kinds['tests/compile_fail'], 'directory-fixture');
  assert.equal(kinds['tests/compile_fail/case1'], 'directory-fixture');
  assert.equal(kinds['tests/compile_fail/case1/src/main.rs'], 'support-module');
  assert.equal(kinds['tests/compile_fail/case1/src/lib.rs'], 'support-module');
  rmSync(dir, { recursive: true, force: true });
});

test('discover: flat tests/*.rs auto-discovery and explicit path claim (no dup)', () => {
  const dir = makeTree({
    'Cargo.toml': [
      '[package]',
      'name = "synth"',
      '',
      '[[test]]',
      'name = "flat_a"',
      'path = "tests/flat_a.rs"',
      '',
      '[[test]]',
      'name = "implied"', // no path -> tests/implied.rs
    ].join('\n'),
    'tests/flat_a.rs': 'fn main() {}\n',
    'tests/flat_b.rs': 'fn main() {}\n',
    'tests/implied.rs': 'fn main() {}\n',
  });
  const { targets, explicitCount } = discoverTargets(dir, manifestOf(
    '[[test]]\nname = "flat_a"\npath = "tests/flat_a.rs"\n\n[[test]]\nname = "implied"\n'));
  assert.equal(explicitCount, 2);
  assert.deepEqual(targets.map((t) => t.name).sort(), ['flat_a', 'flat_b', 'implied']);
  const fa = targets.find((t) => t.name === 'flat_a');
  assert.equal(fa.declared_explicit, true);
  const fb = targets.find((t) => t.name === 'flat_b');
  assert.equal(fb.declared_explicit, false);
  const implied = targets.find((t) => t.name === 'implied');
  assert.equal(implied.path, 'tests/implied.rs');
  // every file exactly once
  const paths = targets.map((t) => t.path);
  assert.equal(new Set(paths).size, paths.length, 'no duplicate target paths');
  rmSync(dir, { recursive: true, force: true });
});

test('discover: duplicate target name throws', () => {
  const dir = makeTree({
    'Cargo.toml': ['[package]', 'name = "synth"'].join('\n'),
    'tests/dup/main.rs': 'fn main() {}\n', // depth-1 auto target collides with [[test]] name
  });
  assert.throws(() => discoverTargets(dir, manifestOf('[[test]]\nname = "dup"\n')),
    /duplicate target name/);
  rmSync(dir, { recursive: true, force: true });
});

test('discover: autotests=false keeps only explicit targets, reclassifies files', () => {
  const dir = makeTree({
    'Cargo.toml': ['[package]', 'name = "synth"', 'autotests = false', '',
      '[[test]]', 'name = "only"'].join('\n'),
    'tests/only.rs': 'fn main() {}\n',
    'tests/auto.rs': 'fn main() {}\n',
  });
  const { targets, nonTargets } = discoverTargets(dir,
    manifestOf('[package]\nautotests = false\n\n[[test]]\nname = "only"\n'));
  assert.deepEqual(targets.map((t) => t.name), ['only']);
  const auto = nonTargets.find((n) => n.path === 'tests/auto.rs');
  assert.ok(auto, 'auto-discovered file reclassified as non-target');
  rmSync(dir, { recursive: true, force: true });
});

test('discover: test=false / harness=false / required-features flow into fields', () => {
  const toml = [
    '[package]',
    'name = "synth"',
    '',
    '[[test]]',
    'name = "a"',
    'test = false',
    'harness = false',
    'required-features = ["alloc-core", "std"]',
  ].join('\n');
  const [t] = explicitTestTargets(manifestOf(toml));
  assert.equal(t.test_enabled, false);
  assert.equal(t.harness, false);
  assert.deepEqual(t.required_features, ['alloc-core', 'std']);
  // defaults when keys absent
  const [d] = explicitTestTargets(manifestOf('[[test]]\nname = "b"\n'));
  assert.equal(d.test_enabled, true);
  assert.equal(d.harness, true);
  assert.deepEqual(d.required_features, []);
});

test('discover: non-target kinds and exact single coverage of every file', () => {
  const dir = makeTree({
    'Cargo.toml': '[package]\nname = "synth"\n',
    'tests/real.rs': 'fn main() {}\n',
    'tests/sub/main.rs': 'fn main() {}\n',
    'tests/run.mjs': 'console.log(1);\n',
    'tests/reg.proptest-regressions': 'box 1\n',
    'tests/data.txt': 'data\n',
    'tests/empty_dir/.keep': '\n',
    'tests/fixtures/helper.rs': 'pub fn h() {}\n',
  });
  const { targets, nonTargets } = discoverTargets(dir, manifestOf());
  const kindOf = {};
  for (const n of nonTargets) kindOf[n.path] = n.kind;
  assert.equal(kindOf['tests/fixtures'], 'directory-fixture');
  assert.equal(kindOf['tests/fixtures/helper.rs'], 'support-module',
    '.rs inside a non-target subdir is a support-module');
  assert.equal(kindOf['tests/run.mjs'], 'mjs-script');
  assert.equal(kindOf['tests/reg.proptest-regressions'], 'proptest-regressions');
  assert.equal(kindOf['tests/data.txt'], 'other-file');
  assert.equal(kindOf['tests/empty_dir'], 'directory-fixture');
  // sub/main.rs at depth 1 IS a target named sub
  assert.ok(targets.some((t) => t.name === 'sub' && t.path === 'tests/sub/main.rs'));
  // coverage: every file on disk appears exactly once across targets+nonTargets
  const all = [];
  const walk = (d, rel) => {
    for (const e of readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
      const child = rel === '' ? e.name : `${rel}/${e.name}`;
      if (e.isDirectory()) walk(join(d, e.name), child);
      else all.push(`tests/${child}`);
    }
  };
  walk(join(dir, 'tests'), '');
  const listed = new Set([...targets.map((t) => t.path), ...nonTargets.map((n) => n.path)]);
  assert.equal(listed.size, targets.length + nonTargets.length, 'no double-count');
  for (const f of all) assert.ok(listed.has(f), `missing from inventory: ${f}`);
  rmSync(dir, { recursive: true, force: true });
});


// ----------------------------------------------------------- 3. features.mjs

test('stripComments (zone b) removes comments AND string contents', () => {
  const src = [
    '// line comment with loom::model mention',
    '/* block comment #[global_allocator] */',
    '//! doc comment mention of with_config',
    'let s = "string with #[global_allocator] inside";',
    'let c = \'x\';',
    'fn main() {}',
  ].join('\n');
  const b = stripComments(src);
  assert.ok(!/global_allocator|loom|with_config|string with/.test(b),
    'zone b erases comments and string contents');
  assert.ok(/fn main/.test(b));
  const a = stripCommentsKeepStrings(src);
  assert.ok(!/doc comment mention/.test(a), 'doc/line comments still stripped in zone a');
  assert.ok(/string with #\[global_allocator\] inside/.test(a),
    'zone a preserves string literal contents');
});

test('zone a keeps feature names in inner cfg attrs; doc-comment markers never fire', () => {
  const src = [
    '//! mentions #[global_allocator] in a doc comment only',
    '#![cfg(feature = "alloc-core")]',
    'fn main() {}',
  ].join('\n');
  const stripped = stripCommentsKeepStrings(src);
  assert.ok(stripped.includes('"alloc-core"'), 'cfg feature name survives zone a');
  const facts = classifySource(src, []);
  assert.equal(facts.global_allocator, false,
    'doc-comment mention of #[global_allocator] must not fire');
  assert.deepEqual(facts.cfg_features, ['alloc-core']);
});

test('fileLevelInnerAttrs: only pre-item #![cfg(/#![cfg_attr( are file-level', () => {
  const multi = [
    '#![cfg(',
    '    feature = "alloc-core",',
    '    not(feature = "std")',
    ')]',
    'use std::vec::Vec;',
    '#![cfg(feature = "late")]', // after an item: ignored
  ].join('\n');
  const attrs = fileLevelInnerAttrs(stripCommentsKeepStrings(multi));
  assert.equal(attrs.length, 1);
  assert.equal(attrs[0].kind, 'cfg');
  assert.ok(attrs[0].body.includes('alloc-core'));
  assert.ok(!attrs[0].body.includes('late'));

  const itemLevel = ['use std::vec::Vec;', '#[cfg(miri)]', 'fn f() {}'].join('\n');
  const facts = classifySource(itemLevel, []);
  assert.equal(facts.file_level_cfg, null);
  assert.equal(facts.miri_only, false,
    'miri in an item-level #[cfg(miri)] is not file-level');

  const cfgAttr = ['#![cfg_attr(miri, feature = "miri-core")]', 'fn main() {}'].join('\n');
  const f2 = classifySource(cfgAttr, []);
  assert.ok(f2.file_level_cfg_attr.includes('miri'));
  assert.equal(f2.miri_only, true,
    'miri inside #![cfg_attr(miri, ...)] IS in the file-level attr text');
  assert.ok(!fileLevelInnerAttrs(stripCommentsKeepStrings(
    '#[cfg(miri)]\nfn f() {}')).some((a) => a.kind === 'cfg_attr'),
  'item-level #[cfg] is not a file-level attr');
});

test('classifySource: global_allocator real vs comment-only vs string-only', () => {
  const real = classifySource('#[global_allocator]\nstatic A: X = X;\n', []);
  assert.equal(real.global_allocator, true);
  const commented = classifySource('// #[global_allocator] in a comment\nfn main() {}\n', []);
  assert.equal(commented.global_allocator, false);
  const strOnly = classifySource('const S: &str = "#[global_allocator]";\nfn main() {}\n', []);
  assert.equal(strOnly.global_allocator, false);
});

test('classifySource: first_config_wins marker matching with word boundaries', () => {
  const markers = [{ marker: 'with_config' }, { marker: 'without_config' }];
  const hit = classifySource('let x = with_config(a, b);\n', markers);
  assert.deepEqual(hit.first_config_wins, ['with_config']);
  const neg = classifySource('let x = without_config(a);\n', markers);
  assert.deepEqual(neg.first_config_wins, ['without_config'],
    "'without_config' must not match marker 'with_config'");
  const commented = classifySource('// with_config in comment\n', markers);
  assert.deepEqual(commented.first_config_wins, []);
});

test('classifySource: process_spawn signals', () => {
  const yes = [
    'use std::process::Command;',
    'fn main() { let c = Command::new("x"); }',
  ].join('\n');
  assert.equal(classifySource(yes, []).process_spawn, true);
  const importBraces = [
    'use std::process::{Command, Output};',
    'fn main() { Command::new("x"); }',
  ].join('\n');
  assert.equal(classifySource(importBraces, []).process_spawn, true,
    'braced import + bare Command::new counts');
  assert.equal(classifySource('let e = std::env::current_exe()?;', []).process_spawn, true);
  assert.equal(classifySource('let k = CARGO_BIN_EXE_tool;', []).process_spawn, true,
    'unquoted CARGO_BIN_EXE_ token fires; inside string literals the token is ' +
    'erased by zone b (documented module behavior)');
  assert.equal(classifySource('thread::spawn(|| {});', []).process_spawn, false,
    'thread::spawn is NOT process spawn');
  assert.equal(classifySource('tokio::spawn(f());', []).process_spawn, false,
    'tokio::spawn is NOT process spawn');
});

test('classifySource: abort / should_panic / catch_unwind / main_fn / line count', () => {
  assert.equal(classifySource('std::process::abort();', []).abort_marker, true);
  assert.equal(classifySource('libc::abort();', []).abort_marker, true);
  assert.equal(classifySource('panic!("x");', []).abort_marker, false);
  assert.equal(classifySource('#[should_panic]\n#[test]\nfn t() {}', []).should_panic, true);
  assert.equal(classifySource('std::panic::catch_unwind(f);', []).catch_unwind, true);
  assert.equal(classifySource('fn main() {}', []).main_fn, true);
  assert.equal(classifySource('fn not_main() {}', []).main_fn, false);
  const lines = classifySource('a\nb\nc', []).lines;
  assert.equal(lines, 3);
});

test('classifySource: loom/miri/kani gating (file-level cfg vs usage vs prose)', () => {
  const cfgLoom = classifySource('#![cfg(loom)]\nfn main() {}', []);
  assert.equal(cfgLoom.loom_only, true);
  const usageLoom = classifySource('loom::model(f);', []);
  assert.equal(usageLoom.loom_only, true);
  const proseLoom = classifySource('// the loom model here\nfn main() {}', []);
  assert.equal(proseLoom.loom_only, false, 'bare loom word in a comment is not loom_only');
  const miri = classifySource('#![cfg_attr(miri, doctest)]\nfn main() {}', []);
  assert.equal(miri.miri_only, true);
  const miriCode = classifySource('fn f() { /* miri */ }', []);
  assert.equal(miriCode.miri_only, false);
  const kani = classifySource('#![cfg(kani)]\nfn main() {}', []);
  assert.equal(kani.kani_gated, true);
});

test('classifySource: cfgFacts extracts features and known cfg keys', () => {
  const { features, keys } = cfgFacts(['feature = "alloc-core", miri, target_os = "windows"']);
  assert.deepEqual(features, ['alloc-core']);
  assert.ok(keys.includes('miri'));
  assert.ok(keys.includes('target_os'));
});

test('deriveFirstConfigWinMarkers keeps only candidates present in src-like input', () => {
  const srcDir = makeTree({ 'src/a.rs': 'pub fn with_config() {}\n' });
  const files = listSrcRustFiles(join(srcDir, 'src'));
  assert.deepEqual(files, ['a.rs']);
  const hit = (dir, marker) => (marker === 'with_config' ? { file: 'src/a.rs', line: 1 } : null);
  const markers = deriveFirstConfigWinMarkers(join(srcDir, 'src'),
    ['with_config', 'config_conflicts'], hit);
  assert.deepEqual(markers.map((m) => m.marker), ['with_config']);
  // markerRegex escapes and word-bounds
  assert.ok(markerRegex('with_config').test('x.with_config(y)'));
  assert.ok(!markerRegex('with_config').test('x.without_config(y)'));
  rmSync(srcDir, { recursive: true, force: true });
  void FIRST_CONFIG_WIN_CANDIDATES; // exported list exists; exercised via repo smoke test
});

test('rawMatchZone: classifies first-match zone as code/comment/string-literal', () => {
  assert.equal(rawMatchZone('let x = loom::model();', /\bloom\b/), 'code');
  assert.equal(rawMatchZone('// loom here\nfn f() {}', /\bloom\b/), 'comment');
  assert.equal(rawMatchZone('let s = "loom";', /\bloom\b/), 'string-literal');
  assert.equal(rawMatchZone('fn f() {}', /\bloom\b/), null);
});

// --------------------------------------------------------------- 4. pins.mjs

test('scanPins: word-boundary counting and kinds', () => {
  const surfaces = [
    { file: '.github/workflows/ci.yml', kind: 'ci_workflow', text: 'run --test alpha --test alpha-x' },
    { file: '.github/actions/a/action.yml', kind: 'ci_action', text: 'runs beta twice beta' },
    { file: 'scripts/x.mjs', kind: 'script', text: 'spawn("alpha")' },
    { file: 'docs/guide.md', kind: 'doc', text: 'see beta in docs' },
    { file: 'docs/verification/rules.json', kind: 'doc_json', text: 'gamma is covered' },
  ];
  const per = scanPins(surfaces, ['alpha', 'beta', 'gamma']);
  assert.equal(per.get('alpha').count, 3,
    'alpha + alpha-x both hit in ci.yml (hyphen is a word boundary) + script');
  assert.equal(per.get('alpha').hits[0].kind, 'ci_workflow');
  assert.equal(per.get('beta').count, 3);
  assert.equal(per.get('beta').hits.map((h) => h.kind).sort().join(','),
    'ci_action,doc');
  assert.equal(per.get('gamma').hits[0].kind, 'doc_json');
  assert.ok(!per.has('alph'), 'word boundary: alph must not hit alpha');
  assert.equal(scanPins(surfaces, ['alph']).size, 0);
});

test('scanPublishedCounts: doc-only count patterns', () => {
  const surfaces = [
    { file: 'README.md', kind: 'doc', text: '562 integration test files exist\nand 12 of 34 tests pass\n(99 files) total' },
    { file: 'scripts/j13x.mjs', kind: 'script', text: '562 integration test files' },
  ];
  const counts = scanPublishedCounts(surfaces);
  assert.equal(counts.length, 3, 'one record per matching README line; one per line');
  assert.ok(counts.every((c) => c.file === 'README.md'), 'scripts are not doc surfaces');
  const nums = counts.flatMap((c) => c.numbers);
  assert.ok(nums.includes(562) && nums.includes(12) && nums.includes(34) && nums.includes(99));
});

const HEX_B = 'b'.repeat(64);

test('scanJ13HashPins: table shape pins, const binding resolves, prose does not', () => {
  const direct = {
    file: 'scripts/j13a_pins.mjs', kind: 'script',
    text: `import { createHash } from 'node:crypto';\nconst h = createHash('sha256');\nconst T = {\n  'src/a.rs': '${HEX_B}',\n  "tests/x.rs": "${HEX64}",\n};\n// prose: we also hash tests/prose.rs later\n`,
  };
  const r1 = scanJ13HashPins([direct]);
  assert.deepEqual(r1.j13Scripts, ['scripts/j13a_pins.mjs']);
  assert.deepEqual(r1.j13HashPins, ['scripts/j13a_pins.mjs'], 'createHash detected');
  const paths = r1.hashPinnedFiles.map((e) => e.path).sort();
  assert.deepEqual(paths, ['src/a.rs', 'tests/x.rs']);
  assert.equal(r1.hashPinnedFiles.find((e) => e.path === 'tests/x.rs').hex, HEX64);
  assert.ok(!r1.hashPinnedFiles.some((e) => e.path.includes('prose')),
    'a 64-hex string NOT adjacent to a path / prose path mention does not pin');
  assert.equal(r1.testsFilesHashPinned.length, 1);
  assert.equal(r1.srcOrOtherFilesHashPinned.length, 1);

  const bound = {
    file: 'scripts/j13b_bound.mjs', kind: 'script',
    text: `const h = createHash('sha256');\nconst FRESH = 'tests/fresh.rs';\nconst T = { [FRESH]: '${HEX_B}' };\n`,
  };
  const r2 = scanJ13HashPins([bound]);
  assert.deepEqual(r2.hashPinnedFiles.map((e) => e.path), ['tests/fresh.rs'],
    'const NAME=path binding resolves in [NAME]: hex entries');

  const noHash = {
    file: 'scripts/j13c_nohash.mjs', kind: 'script',
    text: `const T = { 'src/a.rs': '${HEX_B}' }; // no hashing helper used here\n`,
  };
  const r3 = scanJ13HashPins([noHash]);
  assert.deepEqual(r3.j13HashPins, [], 'no hashing marker -> not a hash-pin script');
  assert.equal(r3.hashPinnedFiles.length, 0);

  const nonJ13 = {
    file: 'scripts/other.mjs', kind: 'script',
    text: `createHash('sha256'); const T = { 'src/a.rs': '${HEX_B}' };`,
  };
  assert.equal(scanJ13HashPins([nonJ13]).j13Scripts.length, 0,
    'non-j13 script names are ignored');

  const hashingScriptProse = {
    file: 'scripts/j13d_prose.mjs', kind: 'script',
    text: `createHash('sha256');\n// GO/NO-GO: verify tests/mentioned.rs matches first\n`,
  };
  const r4 = scanJ13HashPins([hashingScriptProse]);
  assert.equal(r4.hashPinnedFiles.length, 0,
    'prose mention of a tests/ path in a hashing script does NOT pin');
});

test('collectPinSurfaces: kinds and sorted surface inventory on a synthetic tree', () => {
  const dir = makeTree({
    '.github/workflows/ci.yml': 'on: push\n',
    '.github/actions/prep/action.yml': 'name: prep\n',
    'scripts/tool.mjs': 'export {};\n',
    'README.md': 'readme\n',
    'docs/sub/g.md': 'doc\n',
    'docs/verification/rules.json': '{}\n',
    'docs/perf/raw.log': 'noise\n',
  });
  const surfaces = collectPinSurfaces(dir);
  const kinds = surfaces.map((s) => s.kind);
  assert.ok(kinds.includes('ci_workflow') && kinds.includes('ci_action'));
  assert.ok(kinds.includes('script') && kinds.includes('doc') && kinds.includes('doc_json'));
  const files = surfaces.map((s) => s.file);
  const cmp = (a, b) => a.localeCompare(b);
  assert.deepEqual(files.map((f) => `${f}`), [...files].sort(cmp), 'surfaces sorted by file');
  assert.equal(surfaces.find((s) => s.file === 'README.md').kind, 'doc');
  assert.ok(!surfaces.some((s) => s.file.endsWith('.log')), 'non-md/json noise skipped');
  rmSync(dir, { recursive: true, force: true });
});

// -------------------------------------------------------------- 5. groups.mjs

const cleanFacts = () => ({
  file_level_cfg: null, file_level_cfg_attr: null, cfg_features: [], cfg_keys: [],
  global_allocator: false, first_config_wins: [], process_spawn: false,
  abort_marker: false, should_panic: false, catch_unwind: false,
  loom_only: false, loom_mentions: false, miri_only: false, kani_gated: false,
  main_fn: false, lines: 1,
});
const cleanTarget = (name, over = {}) => ({
  name, path: `tests/${name}.rs`, declared_explicit: false,
  harness: true, test_enabled: true, required_features: [], ...over,
});

test('themePrefix extraction', () => {
  assert.equal(themePrefix('regression_r4_3_x'), 'regression');
  assert.equal(themePrefix('b4_r1_pin_cache'), 'b4');
  assert.equal(themePrefix('alloc_core_batch'), 'alloc');
  assert.equal(themePrefix('plainname'), 'plainname');
});

test('mergeability: each blocker individually flips a synthetic target unmergeable', () => {
  const base = { target: cleanTarget('t'), facts: cleanFacts(), externalPins: { count: 0, hits: [] } };
  assert.deepEqual(mergeability(base.target, base.facts, base.externalPins),
    { mergeable: true, reasons: [] });

  const cases = [
    ['global_allocator', { ...base, facts: { ...cleanFacts(), global_allocator: true } }, 'global_allocator'],
    ['first_config_wins', { ...base, facts: { ...cleanFacts(), first_config_wins: ['with_config'] } }, 'first_config_wins:with_config'],
    ['process_spawn', { ...base, facts: { ...cleanFacts(), process_spawn: true } }, 'process_spawn'],
    ['abort', { ...base, facts: { ...cleanFacts(), abort_marker: true } }, 'abort'],
    ['loom_only', { ...base, facts: { ...cleanFacts(), loom_only: true } }, 'loom_only'],
    ['miri_only', { ...base, facts: { ...cleanFacts(), miri_only: true } }, 'miri_only'],
    ['kani_gated', { ...base, facts: { ...cleanFacts(), kani_gated: true } }, 'kani_gated'],
    ['harness_false', { ...base, target: cleanTarget('t', { harness: false }) }, 'harness_false'],
    ['test_disabled', { ...base, target: cleanTarget('t', { test_enabled: false }) }, 'test_disabled'],
    ['required_features', { ...base, target: cleanTarget('t', { required_features: ['std'] }) }, 'required_features:std'],
    ['file_cfg', { ...base, facts: { ...cleanFacts(), file_level_cfg: 'loom' } }, 'file_cfg'],
    ['externally_pinned', { ...base, externalPins: { count: 1, hits: [{ kind: 'ci_workflow', file: 'wf.yml', count: 1 }] } }, 'externally_pinned:ci_workflow@wf.yml'],
  ];
  for (const [label, row, slug] of cases) {
    const m = mergeability(row.target, row.facts, row.externalPins);
    assert.equal(m.mergeable, false, `${label} must block`);
    assert.ok(m.reasons.includes(slug), `${label}: expected slug ${slug} in ${m.reasons}`);
  }
  // informational flags never block
  const info = mergeability(cleanTarget('t'),
    { ...cleanFacts(), should_panic: true, catch_unwind: true, main_fn: true },
    { count: 0, hits: [] });
  assert.equal(info.mergeable, true);
});

test('groupKey: uniform required-features + uniform cfg discriminate feature-only diffs', () => {
  const mk = (name, feats, cfg) => ({
    target: cleanTarget(name, { required_features: feats }),
    facts: { ...cleanFacts(), file_level_cfg: cfg },
  });
  const a1 = mk('alpha_one', ['std'], null);
  const a2 = mk('alpha_two', ['std'], null);
  assert.equal(groupKey(a1.target, a1.facts), groupKey(a2.target, a2.facts),
    'same theme + same features + no cfg share a key');
  const b1 = mk('alpha_one', ['alloc-core'], null);
  assert.notEqual(groupKey(a1.target, a1.facts), groupKey(b1.target, b1.facts),
    'differing feature names produce a different key');
  const c1 = mk('alpha_one', ['std'], 'loom');
  assert.notEqual(groupKey(a1.target, a1.facts), groupKey(c1.target, c1.facts),
    'differing file-level cfg produces a different key');
});

test('groups: saved_links = members - 1 and invariants hold', () => {
  const rows = ['g1_a', 'g1_b', 'g1_c', 'g2_a'].map((n) => ({
    target: cleanTarget(n), facts: cleanFacts(),
  }));
  const groups = buildGroups(rows);
  assert.equal(groups.size, 2);
  assert.equal(groups.get('g1|-|-').savedLinks, 2);
  assert.equal(groups.get('g2|-|-').savedLinks, 0);
  const names = rows.map((r) => r.target.name).sort((a, b) => a.localeCompare(b));
  assert.equal(assertGroupInvariants(names, groups), true);
});

test('relaxationLadder: monotone fileCount, one pilotEligible rung, tie-break pilot', () => {
  const pinned = { count: 1, hits: [{ kind: 'doc', file: 'README.md', count: 1 }] };
  const rows = [
    { target: cleanTarget('x_a'), facts: cleanFacts(), externalPins: { count: 0, hits: [] } },
    { target: cleanTarget('x_b'), facts: cleanFacts(), externalPins: { count: 0, hits: [] } },
    { target: cleanTarget('x_c', { required_features: ['std'] }), facts: cleanFacts(),
      externalPins: { count: 0, hits: [] } },
    { target: cleanTarget('x_d'), facts: { ...cleanFacts(), file_level_cfg: 'loom' },
      externalPins: { count: 0, hits: [] } },
    { target: cleanTarget('x_e'), facts: cleanFacts(), externalPins: pinned },
  ];
  const ladder = relaxationLadder(rows);
  assert.deepEqual(ladder.map((r) => r.rung),
    ['strict', 'compat', 'looser-features', 'looser-cfg', 'allow-pinned']);
  const counts = ladder.map((r) => r.fileCount);
  for (let i = 1; i < counts.length; i++) {
    assert.ok(counts[i] >= counts[i - 1], `fileCount monotone at ${ladder[i].rung}`);
  }
  assert.equal(counts[0], 2, 'strict pool excludes features/cfg/pinned');
  assert.equal(counts[1], 4, 'compat admits uniform-key feature/cfg members (x_c, x_d)');
  assert.equal(counts[2], 4, 'looser-features same pool (features already keyed)');
  assert.equal(counts[3], 4, 'looser-cfg admits the cfg file into a theme-level key');
  assert.equal(counts[4], 5, 'allow-pinned adds the pinned file');
  assert.deepEqual(ladder.filter((r) => r.pilotEligible).map((r) => r.rung), ['compat'],
    'exactly one pilotEligible rung');
  const ap = ladder.find((r) => r.rung === 'allow-pinned');
  assert.equal(ap.pilotEligible, false);
  assert.match(ap.eligibilityReason, /NOT pilot-eligible/);
});

test('pilotCandidate: max saved_links, tie broken lexicographically', () => {
  const groups = new Map([
    ['b|k', { key: 'b|k', members: ['b1', 'b2'], savedLinks: 1 }],
    ['a|k', { key: 'a|k', members: ['a1', 'a2'], savedLinks: 1 }],
    ['c|k', { key: 'c|k', members: ['c1'], savedLinks: 0 }],
  ]);
  assert.equal(pilotCandidate(groups).key, 'a|k');
  groups.get('c|k').savedLinks = 5;
  assert.equal(pilotCandidate(groups).key, 'c|k');
});

test('intersections: sizes <= min(|A|,|B|), zero-count sets safe', () => {
  const rows = [
    { name: 'a', flags: { ga: true, sp: false, fw: ['m1'] } },
    { name: 'b', flags: { ga: true, sp: true, fw: [] } },
  ];
  const flags = ['ga', 'sp', 'fw'];
  const { sets, matrix } = intersections(rows, flags);
  assert.equal(sets.ga, 2);
  assert.equal(sets.sp, 1);
  assert.equal(sets.fw, 1);
  for (const a of flags) for (const b of flags) {
    assert.ok(matrix[a][b] <= Math.min(sets[a], sets[b]));
  }
  const empty = intersections([{ name: 'z', flags: { ga: false } }], ['ga']);
  assert.equal(empty.sets.ga, 0);
  assert.equal(empty.matrix.ga.ga, 0);
});

// ------------------------------------------------- 6. report renderer (pure)

function reportFixture({ manyMembers = false, emptySet = false } = {}) {
  const members = manyMembers
    ? ['m1', 'm2', 'm3', 'm4', 'm5', 'm6', 'm7', 'm8', 'm9', 'm10']
    : ['m1', 'm2'];
  const flags = ['ga', 'sp'];
  const sets = { ga: emptySet ? 0 : 1, sp: 1 };
  return {
    identity: { headSha: '0'.repeat(40), treeSha: '1'.repeat(40), trackedInputsClean: true },
    counts: { targets: 2, externally_pinned: 0, nonTargets: 0, topLevelNonTargets: 0,
      topLevelEntries: 2, groupCount: 1, totalSavedLinks: 1, mergeable: 2 },
    markerList: [{ marker: 'with_config', src_hit: 'src/a.rs:1', provenance: 'derived' }],
    pins: {
      surfaceFiles: [{ file: 'README.md', kind: 'doc' }],
      publishedCounts: [],
      j13HashPins: [],
      testsFilesHashPinned: [],
      srcOrOtherFilesHashPinned: [],
    },
    hypothesesRows: PLAN_HYPOTHESES.map((h) => ({ metric: h.metric, naive: 0, measured: 0 })),
    intersectionFlags: flags,
    intersections: {
      sets,
      matrix: { ga: { ga: sets.ga, sp: emptySet ? 0 : 1 }, sp: { ga: emptySet ? 0 : 1, sp: 1 } },
    },
    nonTargets: [],
    groups: [{ key: 'x|-|-', members, savedLinks: members.length - 1 }],
    pilot: { key: 'x|-|-', members, savedLinks: members.length - 1 },
    relaxation: [{
      rung: 'compat', description: 'd', pilotEligible: true, eligibilityReason: 'ok',
      fileCount: 2, groupCount: 1, savedLinks: 1,
      pilot: { key: 'x|-|-', members, savedLinks: members.length - 1 },
    }],
    reconciliation: [{
      flag: 'global_allocator', naivePattern: 'p', naive: 0, measured: 0,
      baselineOnly: { count: 0, files: [] }, measuredOnly: { count: 0, files: [] },
    }],
  };
}

test('renderReport: pure over data; hypotheses columns present', () => {
  const data = reportFixture();
  const out1 = renderReport(data);
  const out2 = renderReport(data);
  assert.equal(out1, out2, 'renderer is pure (same data -> same text)');
  for (const col of ['metric', 'plan (2026-11-15)', 'naive grep', 'measured', 'verdict']) {
    assert.ok(out1.includes(col), `hypotheses header column missing: ${col}`);
  }
  assert.ok(out1.includes('plan 2026-11-15'));
  assert.ok(out1.includes('confirmed') || out1.includes('corrected'));
});

test('renderReport: empty-set intersections render without NaN/undefined/Infinity', () => {
  const out = renderReport(reportFixture({ emptySet: true }));
  assert.ok(!/NaN|undefined|Infinity|\[object Object\]/.test(out));
});

test('renderReport: long member lists truncate with "+N more"', () => {
  const many = renderReport(reportFixture({ manyMembers: true }));
  assert.ok(many.includes('+2 more'), '10 members -> first 8 + "+2 more"');
  const few = renderReport(reportFixture());
  assert.ok(!few.includes(' more,'), 'short member list not truncated');
});

// ------------------------------------- 7. repo CLI smoke (data-pinned values)

// NOTE: the numeric assertions below PIN TODAY'S MEASURED VALUES of this repo
// (2026-11-15). If tests/ or Cargo.toml changes, these numbers legitimately
// change and the test must be updated. The internal-consistency assertions
// (targets+nonTargets coverage, CSV row count = totalEntries+1) hold
// regardless. No cargo is invoked; the script itself never runs Rust.

function runCli(flag) {
  const r = spawnSync(process.execPath, ['scripts/classify-tests.mjs', flag],
    { cwd: repoRoot, encoding: 'utf8', maxBuffer: 256 * 1024 * 1024 });
  return r;
}

test('CLI --json: counts match today\'s pinned values and structure holds', () => {
  const r = runCli('--json');
  assert.equal(r.status, 0, `exit 0 (stderr: ${r.stderr})`);
  const data = JSON.parse(r.stdout);
  const c = data.counts;
  // --- data-pinned (2026-11-15) ---
  assert.equal(c.targets, 562);
  assert.equal(c.targetsTestFalse, 11);
  assert.equal(c.declaredExplicit, 29);
  assert.equal(c.harnessFalse, 13);
  assert.equal(c.file_level_cfg, 514);
  assert.equal(c.global_allocator, 64);
  assert.equal(c.first_config_wins, 81);
  assert.equal(c.process_spawn, 33);
  // --- internal consistency (holds regardless of data changes) ---
  assert.equal(c.totalEntries, c.targets + c.nonTargets,
    'targets+nonTargets coverage invariant');
  assert.ok(Array.isArray(data.reconciliation) && data.reconciliation.length > 0);
  assert.ok(Array.isArray(data.relaxation) && data.relaxation.length === 5);
  assert.deepEqual(data.relaxation.filter((x) => x.pilotEligible).map((x) => x.rung),
    ['compat'], 'exactly one pilotEligible rung');
  assert.match(data.identity.headSha, /^[0-9a-f]{40}$/);
  assert.match(data.identity.treeSha, /^[0-9a-f]{40}$/);
});

test('CLI --csv: row count = targets+nonTargets+1, header has key columns', () => {
  const r = runCli('--csv');
  assert.equal(r.status, 0, `exit 0 (stderr: ${r.stderr})`);
  const lines = r.stdout.replace(/\r\n$/, '').split(/\r\n/);
  // --- data-pinned (2026-11-15): 1721 entries (562 targets + 1159
  // nonTargets) + 1 header = 1722 lines ---
  assert.equal(lines.length, 1722);
  // --- internal consistency (holds regardless of data changes) ---
  const j = JSON.parse(runCli('--json').stdout);
  assert.equal(lines.length, j.counts.totalEntries + 1,
    'CSV rows = targets + nonTargets + header');
  const header = lines[0];
  for (const col of ['path', 'name', 'kind', 'harness_false', 'mergeable', 'group']) {
    assert.ok(header.includes(col), `CSV header missing column ${col}`);
  }
});

test('CLI --report: exit 0, identity line and hypotheses header present', () => {
  const r = runCli('--report');
  assert.equal(r.status, 0, `exit 0 (stderr: ${r.stderr})`);
  assert.ok(r.stdout.includes('commit `'), 'identity line present');
  assert.ok(r.stdout.includes('| metric |'), 'hypotheses table header present');
});

// --------------------------------------------------------------- 8. determinism

test('discovery+classification pipeline is deterministic on one synthetic tree', () => {
  const dir = makeTree({
    'Cargo.toml': ['[package]', 'name = "synth"', '',
      '[[test]]', 'name = "gate"', 'required-features = ["alloc-core"]'].join('\n'),
    'tests/gate.rs': '#![cfg(feature = "alloc-core")]\nfn main() {}\n',
    'tests/plain.rs': 'fn main() {}\n',
    'tests/alloc_real.rs': '#[global_allocator]\nstatic A: A = A;\n',
    'tests/sub/main.rs': 'fn main() {}\n',
    'tests/util.mjs': 'export {};\n',
  });
  const run = () => {
    const man = manifestOf('[package]\nname = "synth"\n\n[[test]]\nname = "gate"\nrequired-features = ["alloc-core"]\n');
    const { targets, nonTargets } = discoverTargets(dir, man);
    const rows = targets.map((t) => ({
      target: t,
      facts: classifySource(readFileSync(join(dir, t.path), 'utf8'), []),
      externalPins: { count: 0, hits: [] },
    }));
    const groups = buildGroups(rows.filter((r) =>
      mergeability(r.target, r.facts, r.externalPins).mergeable));
    const pins = scanAllPins([{ file: 'README.md', kind: 'doc', text: 'nothing here' }],
      targets.map((t) => t.name));
    return JSON.stringify({ targets, nonTargets, facts: rows.map((r) => r.facts), groups: [...groups], pins });
  };
  assert.equal(run(), run(), 'two pipeline runs over the same tree are deep-equal');
  rmSync(dir, { recursive: true, force: true });
});

test('groups and pins sort order is stable', () => {
  const rows = ['z_a', 'a_b', 'a_a', 'm_c'].map((n) => ({
    target: cleanTarget(n), facts: cleanFacts(),
  }));
  const g1 = [...buildGroups(rows)];
  const g2 = [...buildGroups(rows)];
  assert.deepEqual(g1, g2);
  const surfaces = [
    { file: 'b.md', kind: 'doc', text: 'z_a' },
    { file: 'a.yml', kind: 'ci_workflow', text: 'a_b a_a m_c' },
  ];
  const p1 = scanPins(surfaces, ['z_a', 'a_b', 'a_a', 'm_c']);
  const p2 = scanPins(surfaces, ['z_a', 'a_b', 'a_a', 'm_c']);
  assert.deepEqual(JSON.stringify([...p1]), JSON.stringify([...p2]));
  const s1 = collectPinSurfaces(makeTree({ 'README.md': 'x' }));
  const s2 = collectPinSurfaces(makeTree({ 'README.md': 'x' }));
  assert.deepEqual(s1.map((x) => `${x.file}:${x.kind}`), s2.map((x) => `${x.file}:${x.kind}`));
});
