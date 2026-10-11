import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync, unlinkSync, utimesSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { execFileSync, spawnSync } from 'node:child_process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BASE = '042e88fb67ab48dd6bef7cf8bc416d8b0103ea54';
const TARGET = join(ROOT, 'target', 'r19-prf02');
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const git = (...args) => execFileSync('git', args, { cwd: ROOT });
const q = value => `'${value.replaceAll("'", "'\\''")}'`;
const linuxRoot = () => {
  const match = /^([A-Za-z]):[\\/](.*)$/.exec(ROOT);
  return match ? `/mnt/${match[1].toLowerCase()}/${match[2].replaceAll('\\', '/')}` : ROOT;
};
const FEATURES = { instrumented: 'production bench-internals internals', pure: 'production internals' };
const BENCH = { instrumented: 'r19_prf02_instrumented_iai', pure: 'r19_prf02_production_iai' };
const FILTERS = ['dealloc_prealloc_only_16b', 'dealloc_free_only_16b_n8', 'dealloc_free_only_16b_n9', 'dealloc_free_only_16b_n16', 'small_churn_16b', 'small_churn_16b_2n', 'mimalloc_small_churn_16b', 'mimalloc_small_churn_16b_2n', 'large_alloc_free_cycle', 'alloc_magazine_hit_only_16b', 'alloc_magazine_prefill_only_16b', 'dealloc_own_thread_body_only_16b', 'dealloc_segment_base_of_ptr_probe_only_16b'];

function applyExact(before, section) {
  const source = before.toString().split('\n');
  if (source.at(-1) === '') source.pop();
  const output = [];
  let cursor = 0;
  const lines = section.split('\n');
  for (let i = 0; i < lines.length; i++) {
    const h = /^@@ -(\d+)(?:,(\d+))? \+(\d+)(?:,(\d+))? @@/.exec(lines[i]);
    if (!h) continue;
    const start = Number(h[1]) === 0 ? 0 : Number(h[1]) - 1;
    assert(start >= cursor);
    output.push(...source.slice(cursor, start)); cursor = start;
    let oldCount = 0, newCount = 0;
    while (++i < lines.length && !lines[i].startsWith('@@ ')) {
      const line = lines[i];
      if (line.startsWith(' ') || line.startsWith('-')) {
        assert.equal(source[cursor++], line.slice(1), 'patch context mismatch'); oldCount++;
      }
      if (line.startsWith(' ') || line.startsWith('+')) { output.push(line.slice(1)); newCount++; }
    }
    assert.equal(oldCount, Number(h[2] ?? 1));
    assert.equal(newCount, Number(h[4] ?? 1)); i--;
  }
  output.push(...source.slice(cursor));
  return Buffer.from(output.join('\n') + '\n');
}
function runtimeSections(patch) {
  return patch.split(/(?=^diff --git )/m).filter(section => section.startsWith('diff --git ')).map(section => {
    const path = /^diff --git a\/(\S+) b\/(\S+)/.exec(section);
    assert(path && path[1] === path[2]);
    assert(!path[1].includes('..'));
    return [path[1], section];
  }).filter(([path]) => path.startsWith('src/'));
}
function harness(axis) {
  let text = readFileSync(join(ROOT, 'scripts/iai.mjs'), 'utf8');
  const change = (from, to) => { assert.equal(text.split(from).length, 2); text = text.replace(from, to); };
  change("from './lib.mjs'", `from '${pathToFileURL(join(ROOT, 'scripts/lib.mjs')).href}'`);
  change("const LINUX_TARGET = '/tmp/sefer-iai';", `const LINUX_TARGET = '\"$PWD/target/r19-linux\"';`);
  change("      'cargo bench',", "      'CARGO_BUILD_JOBS=3',\n      'cargo bench --locked',");
  change("const BENCH = 'perf_gate_iai';", `const BENCH = '${BENCH[axis]}';`);
  const start = text.indexOf('  // --locked: use the crate');
  const end = text.indexOf('\n}', start);
  assert(start > 0 && end > start);
  text = text.slice(0, start) + "  throw new Error('Matching preinstalled runner required; installation forbidden');" + text.slice(end);
  return text;
}
function prepare(store, bPatch, cPatch) {
  assert.equal(git('rev-parse', 'HEAD').toString().trim(), BASE);
  assert(!store.startsWith(ROOT), 'backups must be outside repository');
  assert(!existsSync(join(TARGET, 'identity.json')), 'never overwrite manifest');
  const b = readFileSync(bPatch), c = readFileSync(cPatch);
  const sections = runtimeSections(b.toString());
  const paths = [...new Set([...sections.map(([p]) => p), ...runtimeSections(c.toString()).map(([p]) => p)])].sort();
  assert(paths.every(p => /^src\/(alloc_core|registry)\//.test(p)));
  const identity = { base: BASE, b_patch_sha256: sha(b), c_patch_sha256: sha(c), arms: {}, harness: {} };
  for (const arm of ['A', 'B', 'C']) {
    identity.arms[arm] = {};
    for (const path of paths) {
      let base;
      try { base = git('show', `${BASE}:${path}`); } catch { base = null; }
      let bytes = base;
      if (arm === 'B') {
        const section = sections.find(([p]) => p === path)?.[1];
        if (section) bytes = applyExact(base ?? Buffer.alloc(0), section);
      }
      if (arm === 'C') {
        const section = runtimeSections(c.toString()).find(([p]) => p === path)?.[1];
        if (section) bytes = applyExact(base ?? Buffer.alloc(0), section);
        assert.equal(bytes === null ? null : sha(bytes), existsSync(join(ROOT, path)) ? sha(readFileSync(join(ROOT, path))) : null, 'C patch does not reconstruct current source');
      }
      identity.arms[arm][path] = bytes === null ? null : sha(bytes);
      if (bytes !== null) { mkdirSync(dirname(join(store, arm, path)), { recursive: true }); writeFileSync(join(store, arm, path), bytes); }
    }
  }
  mkdirSync(TARGET, { recursive: true });
  for (const axis of Object.keys(FEATURES)) {
    const text = harness(axis);
    const destination = join(TARGET, `iai-${axis}.mjs`);
    writeFileSync(destination, text);
    identity.harness[axis] = sha(text.replaceAll(pathToFileURL(ROOT).href, '<repo>'));
  }
  identity.inputs = Object.fromEntries(['Cargo.toml', 'Cargo.lock', 'scripts/iai.mjs', 'scripts/lib.mjs', 'scripts/r19_prf02_iai.mjs', 'benches/perf_gate_iai.rs', 'benches/r19_prf02_production_iai.rs', 'benches/r19_prf02_instrumented_iai.rs', 'scripts/r19_prf02_extract.mjs', 'examples/r19_prf02_activation.rs', 'scripts/r19_prf02_gate_table.mjs'].map(path => [path, sha(readFileSync(join(ROOT, path)))]));
  mkdirSync(store, { recursive: true });
  const tracked = git('ls-files', '-z', 'src', 'crates').toString().split('\0').filter(Boolean);
  identity.source_inputs = Object.fromEntries(tracked.filter(path => !paths.includes(path)).map(path => [path, sha(readFileSync(join(ROOT, path)))]));
  const changed = git('diff', '--name-only', '--', 'src', 'crates').toString().trim().split('\n').filter(Boolean);
  assert(changed.every(path => paths.includes(path)), 'out-of-scope runtime changes');
  identity.environment = spawnSync('wsl', ['bash','-lc', 'rustc --version --verbose; uname -sm; LC_ALL=C lscpu | sed -n "/Model name:/p"'], {encoding:'utf8'}).stdout;
  writeFileSync(join(TARGET, 'identity.json'), JSON.stringify(identity, null, 2) + '\n');
  verifyArm(store, 'C');
  console.log(JSON.stringify(identity, null, 2));
}
function identity(store) { return JSON.parse(readFileSync(join(TARGET, 'identity.json'))); }
function verifyArm(store, arm) {
  for (const [path, expected] of Object.entries(identity(store).arms[arm])) {
    assert.equal(existsSync(join(ROOT, path)) ? sha(readFileSync(join(ROOT, path))) : null, expected, `${arm}: ${path}`);
    if (expected !== null) assert.equal(sha(readFileSync(join(store, arm, path))), expected);
  }
}
function copyArm(store, arm) {
  for (const [path, expected] of Object.entries(identity(store).arms[arm])) {
    if (expected === null) { if (existsSync(join(ROOT, path))) unlinkSync(join(ROOT, path)); }
    else {
      execFileSync('cp', [join(store, arm, path), join(ROOT, path)]);
      execFileSync('cmp', [join(store, arm, path), join(ROOT, path)]);
      const stamp = new Date(Date.now() + 2000);
      utimesSync(join(ROOT, path), stamp, stamp);
    }
  }
  verifyArm(store, arm);
}
function capture(destination, command, args) {
  assert(!existsSync(destination), 'receipt exists');
  const result = spawnSync(command, args, { cwd: ROOT, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  const text = ((result.stdout ?? '') + (result.stderr ?? ''))
    .replaceAll(ROOT, '<repo>').replaceAll(ROOT.replaceAll('\\', '/'), '<repo>')
    .replaceAll(linuxRoot(), '<repo>')
    .replace(/[A-Z]:[\\/]Users[\\/][^\s\\/]+/gi, '<home>')
    .replace(/\/home\/[^\s/]+/g, '<home>');
  if (Buffer.byteLength(text) >= 200 * 1024) {
    const full = join(TARGET, `${destination.split(/[\\/]/).at(-1)}.full`);
    writeFileSync(full, text);
    const kept = text.split(/\r?\n/).filter(line => /^(?:\w+::)+\w+|Instructions:|Estimated Cycles:|\[iai\]|error|FAIL|PASS/.test(line)).join('\n');
    writeFileSync(destination, '# TRUNCATED: compilation/cache detail omitted; full sanitized output under target/r19-prf02, reproduce with the recorded driver command.\n' + kept + '\n');
  } else writeFileSync(destination, text);
  assert.equal(result.status, 0, 'command failed; raw receipt preserved');
}
function execute(store, arm, axis, repetition) {
  assert(['A', 'B', 'C'].includes(arm)); assert(axis in FEATURES); assert(/^[12]$/.test(repetition));
  const id = identity(store);
  for (const [path, expected] of Object.entries({...id.inputs, ...id.source_inputs})) assert.equal(sha(readFileSync(join(ROOT, path))), expected, path);
  verifyArm(store, 'C');
  const receipt = join(ROOT, 'docs/perf', `_raw_r19_prf02_${arm}_${axis}_${repetition}.log`);
  assert(!existsSync(receipt), 'never overwrite measurement receipt');
  try {
    copyArm(store, arm);
    capture(receipt, process.execPath, [join(TARGET, `iai-${axis}.mjs`), '--features', FEATURES[axis], ...FILTERS.filter(name => axis === 'instrumented' || !name.startsWith('dealloc_own_thread_body') && !name.startsWith('dealloc_segment_base'))]);
  } finally { copyArm(store, 'C'); console.log(git('diff', '--stat').toString()); }
}
const [mode, ...args] = process.argv.slice(2);
if (mode === '--self-test') {
  assert.equal(applyExact(Buffer.from('a\nb\n'), '@@ -1,2 +1,2 @@\n a\n-b\n+c\n').toString(), 'a\nc\n');
  assert.throws(() => applyExact(Buffer.from('x\n'), '@@ -1 +1 @@\n-a\n+b\n'));
  for (const axis of Object.keys(FEATURES)) assert(harness(axis).includes('cargo bench --locked'));
  console.log('PASS: exact patch/context and generated-harness anchors; no source switching or measurement');
} else if (mode === '--prepare') prepare(resolve(args[0]), resolve(args[1]), resolve(args[2]));
else if (mode === '--dry-run') {
  const store = resolve(args[0]); verifyArm(store, 'C');
  console.log('A/B/C: cp snapshots in current worktree; finally cp C + cmp. No source changes in dry-run.');
  console.log('Linux target: $PWD/target/r19-linux; axes:', FEATURES);
  console.log('Historical 54fc2a21: BLOCKED; never substitute a lower-bound-only revert.');
} else if (mode === '--run') execute(resolve(args[0]), args[1], args[2], args[3]);
else if (mode === '--roundtrip') {
  const store = resolve(args[0]); verifyArm(store, 'C');
  try { for (const arm of ['A', 'B', 'C']) { copyArm(store, arm); console.log(`${arm}: cp/cmp PASS`); } }
  finally { copyArm(store, 'C'); }
  console.log('C restored; no cargo or measurements');
} else if (mode === '--activation') {
  const store = resolve(args[0]); const arm = args[1]; assert(['A', 'B', 'C'].includes(arm));
  verifyArm(store, 'C');
  try {
    copyArm(store, arm);
    capture(join(ROOT, 'docs/perf', `_raw_r19_prf02_${arm}.oracle.log`), 'wsl', ['bash', '-lc', `cd ${q(linuxRoot())} && RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target/r19-linux" cargo run --locked --example r19_prf02_activation --features 'production internals bench-internals' -- ${arm} ${arm === 'A' ? 32 : 16}`]);
  } finally { copyArm(store, 'C'); }
}
else throw new Error('Usage: --self-test | --prepare external-store armB.patch armC.patch | --dry-run external-store | --run external-store A|B|C instrumented|pure 1|2');
