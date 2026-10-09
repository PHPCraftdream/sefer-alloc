// Exhaustive hidden-hook internals boundary. Uses the comment/string-proof
// inventory parser and inherited module gates, not textual cfg mentions.
// Parent-only acceptance: --self-test retains structural cases plus literal fixtures.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';
import { scanFile, listRust, idFor, rustMask, attributesRequire, effectiveFileGates, REVIEWED_SURFACE } from './verify-dbg-hook-safety.mjs';

// Arrays deliberately retain duplicate rows for validation before Map creation.
const ALLOWLIST = [
  ['src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_foreign_or_unroutable_frees', 'backs AllocStats::stats()'],
  ['src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_segments_reserved_total', 'backs AllocStats::stats()'],
  ['src/alloc_core/alloc_core/alloc_core_core_diag/totals.rs::dbg_segments_released_total', 'backs AllocStats::stats()'],
  ['src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs::dbg_decommit_count', 'backs AllocStats::stats()'],
];
function verify(files, allowlist = ALLOWLIST, include = hook => hook.file.startsWith('src/alloc_core/') && hook.owner === 'AllocCore') {
  const errors = [], exceptions = new Map(), found = new Set(), gatedNames = new Set();
  for (const [id, reason] of allowlist) {
    if (exceptions.has(id)) errors.push(`duplicate internals allowlist row: ${id}`);
    if (!reason.trim()) errors.push(`missing internals exception reason: ${id}`);
    exceptions.set(id, reason);
  }
  const fileRequires = effectiveFileGates(files);
  let total = 0;
  for (const file of files.values()) for (const hook of file.hooks) {
    if (!include(hook)) continue;
    total++; found.add(hook.id);
    if (attributesRequire(hook.attrs, 'internals') || fileRequires(hook.file, 'internals')) gatedNames.add(hook.name);
    else if (!exceptions.has(hook.id)) errors.push(`hidden API missing internals gate: ${hook.id}`);
  }
  for (const id of exceptions.keys()) if (!found.has(id)) errors.push(`stale internals allowlist row: ${id}`);
  return { errors, total, gatedNames };
}
function testCallers(files, gatedNames, roots) {
  const requiresFile = effectiveFileGates(files, roots), errors = [];
  for (const [id, file] of files) {
    if (requiresFile(id, 'internals')) continue;
    const calls = [...rustMask(file.text).matchAll(/[.:](\w+)\s*\(/g)].map(match => match[1]).filter(name => gatedNames.has(name));
    if (calls.length) errors.push(`test caller missing internals ${roots.has(id) ? 'crate' : 'inherited module'} gate: ${id}: ${[...new Set(calls)].sort().join(', ')}`);
  }
  return errors;
}
function testFiles(sources, manifest) {
  const files = new Map([...sources].map(([id, text]) => [id, { ...scanFile(id, text), text }]));
  // Cargo auto-discovers tests/*.rs and tests/*/main.rs. Nested helpers
  // are not roots, even when their directory is not named support.
  const roots = new Set([...files.keys()].filter(id => /^tests\/[^/]+\.rs$|^tests\/[^/]+\/main\.rs$/.test(id)));
  // Explicit targets can gate a crate via required-features instead of #![cfg].
  // Strip TOML comments without treating a # inside a string as a comment.
  const toml = manifest.replace(/"(?:\\.|[^"\\])*"|#[^\r\n]*/g, token => token.startsWith('#') ? '' : token);
  for (const target of toml.split(/^\s*\[\[test\]\]\s*$/m).slice(1)) {
    const section = target.split(/^\s*\[/m)[0];
    const name = /^\s*name\s*=\s*"([^"]+)"/m.exec(section)?.[1];
    const path = /^\s*path\s*=\s*"([^"]+)"/m.exec(section)?.[1] ?? (name ? `tests/${name}.rs` : null);
    if (!files.has(path)) continue;
    roots.add(path);
    const required = /^\s*required-features\s*=\s*\[([^\]]*)\]/m.exec(section)?.[1] ?? '';
    for (const feature of required.matchAll(/"([^"]+)"/g))
      files.get(path).globals.push(`#![cfg(feature = "${feature[1]}")]`);
  }
  return { files, roots };
}
function selfCheck() {
  const fixture = text => new Map([['fixture.rs', scanFile('fixture.rs', text)]]);
  const select = () => true;
  for (const declaration of ['pub const fn dbg_const()', 'pub fn value_for_test()', 'pub fn value_for_tests()', 'pub fn inject_fault()']) {
    const bad = verify(fixture(`impl AllocCore { ${declaration} {} }`), [], select);
    assert.equal(bad.total, 1);
    assert.equal(bad.errors.length, 1);
    for (const gate of ['#[cfg(feature = "internals")]', '#[cfg(all(feature = "other", feature = "internals"))]'])
      assert.equal(verify(fixture(`${gate}\nimpl AllocCore { ${declaration} {} }`), [], select).errors.length, 0);
  }
  for (const gate of ['// #[cfg(feature = "internals")]', '#[cfg(not(feature = "internals"))]', '#[cfg(any(feature = "internals", feature = "production"))]', '#[cfg_attr(feature = "internals", allow(dead_code))]'])
    assert(verify(fixture(`${gate}\nimpl AllocCore { pub fn dbg_gate() {} }`), [], select).errors.includes('hidden API missing internals gate: fixture.rs::dbg_gate'));
  const inherited = new Map([
    ['fixture/mod.rs', scanFile('fixture/mod.rs', '#[cfg(feature = "internals")]\nmod child;')],
    ['fixture/child.rs', scanFile('fixture/child.rs', 'impl AllocCore { pub const fn dbg_const() {} }')],
  ]);
  assert.equal(verify(inherited, [], select).errors.length, 0);
  inherited.set('fixture/mod.rs', scanFile('fixture/mod.rs', '#[cfg(any(feature = "internals", feature = "production"))]\nmod child;'));
  assert(verify(inherited, [], select).errors.includes('hidden API missing internals gate: fixture/child.rs::dbg_const'));
  assert(verify(fixture('impl AllocCore { pub fn dbg_gate() {} }'), [['fixture.rs::dbg_gate', 'stats'], ['fixture.rs::dbg_gate', 'stats']], select).errors.includes('duplicate internals allowlist row: fixture.rs::dbg_gate'));
  assert(verify(fixture(''), [['fixture.rs::dbg_retired', 'stats']], select).errors.includes('stale internals allowlist row: fixture.rs::dbg_retired'));
  assert.equal(verify(fixture('const X: &str = "pub fn dbg_fake() {}"; /* impl AllocCore { pub fn dbg_fake() {} } */'), [], select).total, 0);
  // Structural row coverage, not an independent oracle: names/gates come
  // from the same reviewed policy. Literal fixtures below do not use rows.
  for (const row of REVIEWED_SURFACE.filter(row => row.gates.includes('internals'))) {
    const split = row.id.lastIndexOf('::'), file = row.id.slice(0, split), name = row.id.slice(split + 2);
    const declaration = `${row.kind === 'unsafe' ? '/// # Safety\n/// Caller owns the allocation.\n' : ''}pub ${row.kind === 'unsafe' ? 'unsafe ' : ''}fn ${name}() {}`;
    const files = new Map([[file, scanFile(file, `impl AllocCore {\n${declaration}\n}`)]]);
    assert(verify(files, [], select).errors.includes(`hidden API missing internals gate: ${row.id}`));
    files.set(file, scanFile(file, `#[cfg(feature = "internals")]\nimpl AllocCore {\n${declaration}\n}`));
    assert.equal(verify(files, [], select).errors.length, 0);
  }
  // Hand-authored actual AllocCore forwarders: default owner selection must
  // inventory these non-dbg names, including unsafe flush, without row loops.
  const magazine = 'src/alloc_core/small/alloc_core_small_magazine.rs';
  const source = `impl AllocCore {
    pub fn refill_class(&mut self, class_idx: usize, want: usize, out: &mut [*mut u8]) -> usize { 0 }
    pub fn refill_class_bump(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize { 0 }
    pub fn refill_class_bump_virgin(&mut self, class_idx: usize, out: &mut [*mut u8], virgin_out: &mut u16) -> usize { 0 }
    /// # Safety
    /// Caller owns the live blocks exactly once.
    pub unsafe fn flush_class(&mut self, class_idx: usize, blocks: &[*mut u8]) {}
  }`;
  const actual = new Map([[magazine, scanFile(magazine, source)]]);
  assert.equal(verify(actual, []).total, 4);
  assert.deepEqual(verify(actual, []).errors, [
    `hidden API missing internals gate: ${magazine}::refill_class`,
    `hidden API missing internals gate: ${magazine}::refill_class_bump`,
    `hidden API missing internals gate: ${magazine}::refill_class_bump_virgin`,
    `hidden API missing internals gate: ${magazine}::flush_class`,
  ]);
  actual.set(magazine, scanFile(magazine, `#[cfg(feature = "internals")]\n${source}`));
  assert.deepEqual(verify(actual, []).errors, []);
  actual.set(magazine, scanFile(magazine, source));
  const parent = 'src/alloc_core/small/mod.rs';
  actual.set(parent, scanFile(parent, '#[cfg(feature = "internals")] mod alloc_core_small_magazine;'));
  assert.deepEqual(verify(actual, []).errors, []);
  actual.set(parent, scanFile(parent, '#[cfg(any(feature = "internals", feature = "production"))] mod alloc_core_small_magazine;'));
  assert.equal(verify(actual, []).errors.length, 4);

  // The AllocCore-only boundary deliberately excludes SegmentLayout; the
  // safety scanner separately enforces its internals policy.
  const layout = 'src/alloc_core/segment/segment_layout.rs';
  const geometry = new Map([[layout, scanFile(layout, 'impl SegmentLayout { pub fn small_decommit_start() -> usize { 0 } }')]]);
  assert.equal(verify(geometry, []).total, 0);
  assert.deepEqual(verify(geometry, [], select).errors, [`hidden API missing internals gate: ${layout}::small_decommit_start`]);
  for (const declaration of ['pub const unsafe fn', 'pub unsafe const fn']) {
    const parsed = fixture(`impl AllocCore { ${declaration} dbg_const_unsafe() {} }`);
    assert.equal(verify(parsed, [], select).total, 1);
    assert.deepEqual(verify(parsed, [], select).errors, ['hidden API missing internals gate: fixture.rs::dbg_const_unsafe']);
  }
  // Independent caller oracle: every incoming declaration is significant,
  // including a second declaration in the same parent and an ungated root.
  const callerSources = new Map([
    ['tests/first.rs', '#![cfg(feature = "internals")]\n#[path = "nested/helper.rs"] mod helper;'],
    ['tests/second.rs', '#![cfg(feature = "internals")]\n#[path = "nested/helper.rs"] mod renamed;'],
    ['tests/nested/helper.rs', 'fn check() { core.dbg_probe(); }'],
  ]);
  const callerErrors = (sources = callerSources, manifest = '') => {
    const { files, roots } = testFiles(sources, manifest);
    return testCallers(files, new Set(['dbg_probe']), roots);
  };
  assert.deepEqual(callerErrors(), []);
  callerSources.set('tests/second.rs', '#[path = "nested/helper.rs"] mod renamed;');
  assert.deepEqual(callerErrors(), ['test caller missing internals inherited module gate: tests/nested/helper.rs: dbg_probe']);
  assert.deepEqual(callerErrors(callerSources, '[[test]]\nname = "second"\nrequired-features = ["internals"]'), []);
  callerSources.set('tests/second.rs', '#[cfg(feature = "internals")]\n#[path = "nested/helper.rs"] mod renamed;\n#[path = "nested/helper.rs"] mod ungated;');
  assert.equal(callerErrors().length, 1);
  callerSources.set('tests/second.rs', '#[cfg(feature = "internals")]\n#[path = "nested/helper.rs"] mod renamed;');
  assert.deepEqual(callerErrors(), []);
  callerSources.set('tests/nested/helper.rs', 'mod leaf;');
  callerSources.set('tests/nested/helper/leaf.rs', 'fn check() { core.dbg_probe(); }');
  assert.deepEqual(callerErrors(), []);
  callerSources.set('tests/root.rs', 'fn check() { core.dbg_probe(); }');
  callerSources.set('tests/first.rs', '#![cfg(feature = "internals")]\n#[path = "root.rs"] mod root;');
  assert.deepEqual(callerErrors(), ['test caller missing internals crate gate: tests/root.rs: dbg_probe']);
  assert.deepEqual(callerErrors(new Map([
    ['tests/root.rs', '#[cfg(feature = "internals")] mod outer { #[path = "helper.rs"] mod child; }'],
    ['tests/outer/helper.rs', 'fn check() { core.dbg_probe(); }'],
  ])), []);
  assert.equal(callerErrors(new Map([
    ['tests/root.rs', 'mod outer { #[path = "helper.rs"] mod child; }'],
    ['tests/outer/helper.rs', 'fn check() { core.dbg_probe(); }'],
  ])).length, 1);
  // Ordinary root mod resolution is relative to tests/, not tests/<root>/.
  assert.deepEqual(callerErrors(new Map([
    ['tests/root.rs', '#![cfg(feature = "internals")] mod nested;'],
    ['tests/nested/mod.rs', 'mod helper;'],
    ['tests/nested/helper.rs', 'fn check() { core.dbg_probe(); }'],
  ])), []);
}
if (process.argv.includes('--self-test')) {
  selfCheck();
  console.log('[verify-alloc-core-dbg-internals-exhaustive] parser and API boundary self-checks PASS');
} else {
  const files = new Map(listRust(join(REPO_ROOT, 'src')).map(path => { const id = idFor(path); return [id, scanFile(id, readFileSync(path, 'utf8'))]; }));
  const result = verify(files);
  // Resolve all declarations across the test tree; only actual Cargo roots
  // require their own gate. Helpers inherit only if every incoming route gates.
  const tests = testFiles(new Map(listRust(join(REPO_ROOT, 'tests')).map(path => [idFor(path), readFileSync(path, 'utf8')])), readFileSync(join(REPO_ROOT, 'Cargo.toml'), 'utf8'));
  result.errors.push(...testCallers(tests.files, result.gatedNames, tests.roots));
  if (result.errors.length) { console.error(`[verify-alloc-core-dbg-internals-exhaustive] FAIL\n${result.errors.join('\n')}`); process.exitCode = 1; }
  else console.log(`[verify-alloc-core-dbg-internals-exhaustive] ALL GREEN: ${result.total} hidden AllocCore APIs`);
}
