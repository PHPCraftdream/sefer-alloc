// Exhaustive hidden-hook internals boundary. Uses the comment/string-proof
// inventory parser and inherited module gates, not textual cfg mentions.
// Parent-only acceptance: --self-test retains structural cases plus literal fixtures.
import assert from 'node:assert/strict';
import { readFileSync, existsSync, mkdtempSync, writeFileSync, mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, relative } from 'node:path';
import { REPO_ROOT } from './lib.mjs';
import { scanFile, listRust, idFor, rustMask, attributesRequire, effectiveFileGates, REVIEWED_SURFACE } from './verify-dbg-hook-safety.mjs';

// Arrays deliberately retain duplicate rows for validation before Map creation.
//
// Exact allowlist of doc(hidden) AllocCore hidden APIs that are deliberately
// NOT internals-gated: each is a forwarding/internal seam with ACTUAL
// production or diagnostics consumption across the P2.2 crate boundary (the
// root `sefer-alloc` facade crate reaches these through the
// `sefer_alloc_core::alloc_core::AllocCore` re-export), so an `internals`
// cfg here would gate real production/diagnostics API and break consumers.
// Each row names its exact consumer sites, traced on this tree.
const ALLOWLIST = [
  ['crates/core/sefer-alloc-core/src/alloc_core/alloc_core/mem/mem_impl.rs::canonical_root_for',
    'facade seam: called by root src/global/sefer_alloc/diag.rs, crates/core/sefer-registry/src/registry/heap_core/alloc/hot.rs, crates/core/sefer-registry/src/registry/heap_core/free/dealloc_own_base.rs (debug asserts), crates/core/sefer-registry/src/registry/heap_core/diag/queries.rs — production + diagnostics consumption'],
  ['crates/core/sefer-alloc-core/src/alloc_core/alloc_core/mem/mem_impl.rs::canonical_block_of',
    'facade seam: called by crates/core/sefer-registry/src/registry/heap_core/free/dealloc_batch.rs (dealloc_batch_small foreign-pointer guard) plus in-crate dealloc/realloc/sidecar test hooks — production consumption'],
  ['crates/core/sefer-alloc-core/src/alloc_core/alloc_core/mem/mem_impl.rs::owned_block_of',
    'facade seam: called by crates/core/sefer-registry/src/registry/heap_core_xthread/routing.rs, crates/core/sefer-registry/src/registry/heap_core/free/realloc.rs, crates/core/sefer-registry/src/registry/heap_core/diag/diag_probes.rs — production + diagnostics consumption'],
  ['crates/core/sefer-alloc-core/src/alloc_core/alloc_core/sidecar_drain.rs::dbg_large_sidecar_full_rescues',
    'diag counter seam: forwarded by crates/core/sefer-registry/src/registry/heap_core_xthread/sidecar_drain.rs dbg_large_sidecar_full_rescues() — diagnostics (bench-internals) consumption'],
  ['crates/core/sefer-alloc-core/src/alloc_core/alloc_core/sidecar_drain.rs::dbg_large_hot_budget',
    'diag counter seam: forwarded by crates/core/sefer-registry/src/registry/heap_core_xthread/sidecar_drain.rs dbg_large_hot_budget() — diagnostics (bench-internals) consumption'],
  ['crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_magazine.rs::refill_class_bump_internal',
    'internal forwarding seam: called cross-crate by crates/core/sefer-registry/src/registry/heap_core/alloc/batch.rs batch refill — production hot-path consumption'],
];
// Mirror the hook scanner's surface: named hooks + safe raw-pointer fns are the hidden-API surface;
// unsafe raw-pointer fns outside the gates are the documented unsafe API (gated ones like flush_class are named anyway).
function verify(files, allowlist = ALLOWLIST, include = hook => (hook.file.startsWith('src/alloc_core/') || hook.file.startsWith('crates/core/sefer-alloc-core/src/alloc_core/')) && hook.owner === 'AllocCore' && (hook.named || (hook.pointerParams && !hook.unsafe))) {
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
// P2.2: the AllocCore implementation now lives in the sefer-alloc-core crate;
// the real walk covers BOTH src roots (facade + crate) exactly like main().
const WALK_ROOTS = root => [join(root, 'src'), join(root, 'crates/core/sefer-alloc-core/src')];
const WALK_ROOT_PREFIXES = ['src/', 'crates/core/sefer-alloc-core/src/'];
function walkSources(root = REPO_ROOT) {
  return new Map(WALK_ROOTS(root).flatMap((dir, i) => existsSync(dir) ? listRust(dir).map(path => {
    // Root-relative id (NOT idFor): the temp-fixture self-test root lies
    // outside the repo, where idFor's REPO_ROOT-relative id would leak an
    // absolute path and silently drop the fixture from the boundary.
    const id = WALK_ROOT_PREFIXES[i] + relative(dir, path).split('\\').join('/');
    return [id, scanFile(id, readFileSync(path, 'utf8'))];
  }) : []));
}
function walkTests(root = REPO_ROOT) {
  const testsDir = join(root, 'tests'), manifest = join(root, 'Cargo.toml');
  return testFiles(new Map(existsSync(testsDir) ? listRust(testsDir).map(path => [`tests/${relative(testsDir, path).split('\\').join('/')}`, readFileSync(path, 'utf8')]) : []), existsSync(manifest) ? readFileSync(manifest, 'utf8') : '');
}
// Shared production path: exactly the same walk + verify + caller scan for
// the real repo and for the self-test fixture (no env override, no subprocess).
function run(root, allowlist) {
  const files = walkSources(root);
  const result = verify(files, allowlist);
  const tests = walkTests(root);
  result.errors.push(...testCallers(tests.files, result.gatedNames, tests.roots));
  return result;
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
  // from the same reviewed policy. Only this gate's own surface (alloc-core
  // files, the same set `select` accepts) participates; literal fixtures
  // below do not use rows.
  const rowScope = (id) => id.startsWith('src/alloc_core/') || id.startsWith('crates/core/sefer-alloc-core/src/alloc_core/');
  for (const row of REVIEWED_SURFACE.filter(row => row.gates.includes('internals') && rowScope(row.id))) {
    const split = row.id.lastIndexOf('::'), file = row.id.slice(0, split), name = row.id.slice(split + 2);
    const declaration = `${row.kind === 'unsafe' ? '/// # Safety\n/// Caller owns the allocation.\n' : ''}pub ${row.kind === 'unsafe' ? 'unsafe ' : ''}fn ${name}() {}`;
    const files = new Map([[file, scanFile(file, `impl AllocCore {\n${declaration}\n}`)]]);
    assert(verify(files, [], select).errors.includes(`hidden API missing internals gate: ${row.id}`));
    files.set(file, scanFile(file, `#[cfg(feature = "internals")]\nimpl AllocCore {\n${declaration}\n}`));
    assert.equal(verify(files, [], select).errors.length, 0);
  }
  // Hand-authored actual AllocCore forwarders: default owner selection must
  // inventory these non-dbg names, including unsafe flush, without row loops.
  const magazine = 'crates/core/sefer-alloc-core/src/alloc_core/small/alloc_core_small_magazine.rs';
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
  const parent = 'crates/core/sefer-alloc-core/src/alloc_core/small/mod.rs';
  actual.set(parent, scanFile(parent, '#[cfg(feature = "internals")] mod alloc_core_small_magazine;'));
  assert.deepEqual(verify(actual, []).errors, []);
  actual.set(parent, scanFile(parent, '#[cfg(any(feature = "internals", feature = "production"))] mod alloc_core_small_magazine;'));
  assert.equal(verify(actual, []).errors.length, 4);

  // The AllocCore-only boundary deliberately excludes SegmentLayout; the
  // safety scanner separately enforces its internals policy.
  const layout = 'crates/core/sefer-segment/src/segment/support/segment_directory/segment_layout.rs';
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
  // Production traversal via the SAME run() path as main(): a temp fixture
  // tree (never the shared one) with an empty allowlist. Negative mutant
  // first: an ungated AllocCore hidden API under crates/core/sefer-alloc-core
  // must be DISCOVERED by the walk and reported; then the same fixture gated
  // `internals` must go green with the hook counted. Omission counterfactual:
  // deleting the fixture file must drop the count to 0, proving the count
  // comes from the walked tree and not from the allowlist.
  const fixtureRoot = mkdtempSync(join(tmpdir(), 'alloc-core-dbg-walk-'));
  const fixtureDir = join(fixtureRoot, 'crates/core/sefer-alloc-core/src/alloc_core');
  mkdirSync(fixtureDir, { recursive: true });
  mkdirSync(join(fixtureRoot, 'tests'), { recursive: true });
  writeFileSync(join(fixtureRoot, 'Cargo.toml'), '');
  const fixturePath = join(fixtureDir, 'dbg_walk_fixture.rs');
  const fixtureId = 'crates/core/sefer-alloc-core/src/alloc_core/dbg_walk_fixture.rs';
  try {
    writeFileSync(fixturePath, 'impl AllocCore { pub fn dbg_walk_probe() {} }\n');
    const mutant = run(fixtureRoot, []);
    assert.deepEqual(mutant.errors, [`hidden API missing internals gate: ${fixtureId}::dbg_walk_probe`]);
    writeFileSync(fixturePath, '#[cfg(feature = "internals")]\nimpl AllocCore { pub fn dbg_walk_probe() {} }\n');
    const gated = run(fixtureRoot, []);
    assert.deepEqual(gated.errors, []);
    assert.equal(gated.total, 1);
    rmSync(fixturePath);
    const omitted = run(fixtureRoot, []);
    assert.deepEqual(omitted.errors, []);
    assert.equal(omitted.total, 0);
  } finally {
    rmSync(fixtureRoot, { recursive: true, force: true });
  }
  console.log('[verify-alloc-core-dbg-internals-exhaustive] production-walk mutant check PASS: ungated crate fixture killed in-process, gated fixture green, omission counterfactual counts 0');
}
if (process.argv.includes('--self-test')) {
  selfCheck();
  console.log('[verify-alloc-core-dbg-internals-exhaustive] parser and API boundary self-checks PASS');
} else {
  const result = run(REPO_ROOT, ALLOWLIST);
  if (result.errors.length) { console.error(`[verify-alloc-core-dbg-internals-exhaustive] FAIL\n${result.errors.join('\n')}`); process.exitCode = 1; }
  else console.log(`[verify-alloc-core-dbg-internals-exhaustive] ALL GREEN: ${result.total} hidden AllocCore APIs`);
}
