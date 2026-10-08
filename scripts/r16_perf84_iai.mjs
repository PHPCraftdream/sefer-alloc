// R16 item84: frozen worktree-local A/C0 snapshots and original iai semantics.
// --prepare never builds/runs; --compile and --activation never benchmark.
// --run <A1|A2|C0|B1|B2> is explicit later-phase measurement authorization.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, writeFileSync, lstatSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync, spawn } from 'node:child_process';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const BASE = '265a571abc67323792334dc4dc5452fc59328d19';
const STORE = join(ROOT, 'target', 'r16-perf84');
const FEATURES = 'production bench-internals internals';
const OWNED = ['src/registry/heap_core/free/dealloc_own_base.rs', 'src/registry/heap_core/state/tcache_flush.rs'];
const BENCHES = [
  'dealloc_free_only_16b_n16', 'dealloc_free_only_16b_n17', 'dealloc_free_only_16b_n32',
  'dealloc_prealloc_only_16b', 'dealloc_free_only_16b_n8', 'dealloc_free_only_16b_n9',
  'dealloc_flush_class_only_16b_prefix', 'dealloc_flush_class_only_16b',
  'alloc_clear_magazine_only_16b_prefix', 'alloc_clear_magazine_only_16b',
  'alloc_magazine_hit_only_16b', 'small_churn_16b', 'small_churn_16b_2n',
  'dealloc_flush_all_tcache_16b_prefix', 'dealloc_flush_all_tcache_16b',
];
const hash = (data) => createHash('sha256').update(data).digest('hex');
const git = (...args) => execFileSync('git', args, { cwd: ROOT });
const files = () => git('ls-files', '-z').toString().split('\0').filter(Boolean).sort();
const wslPath = (path) => {
  const m = /^([A-Za-z]):[\\/](.*)$/.exec(path);
  assert(m, 'Windows WSL driver requires drive-qualified worktree');
  return `/mnt/${m[1].toLowerCase()}/${m[2].replaceAll('\\', '/')}`;
};
const quote = (s) => `'${s.replaceAll("'", "'\\''")}'`;

function generatedHarness(original) {
  const replace = (old, value) => {
    assert.equal(original.split(old).length, 2, `expected one harness anchor: ${old}`);
    original = original.replace(old, value);
  };
  replace("const LINUX_TARGET = '/tmp/sefer-iai';", 'const LINUX_TARGET = \'"$PWD/target"\';');
  replace("      'cargo bench',", "      'CARGO_BUILD_JOBS=3',\n      'cargo bench --locked',");
  // Keep original installed-runner probe, but never permit dependency installs.
  const start = original.indexOf('  // --locked: use the crate');
  const end = original.indexOf('\n}\n', start);
  assert(start > 0 && end > start);
  original = original.slice(0, start) + "  throw new Error('R16: matching preinstalled runner required; installs prohibited');" + original.slice(end);
  return original;
}

function manifest(dir, paths) {
  return paths.map((p) => [p, hash(readFileSync(join(dir, p)))]);
}
function verify(label) {
  const dir = join(STORE, label);
  const identity = JSON.parse(readFileSync(join(STORE, `${label}.identity.json`)));
  assert.equal(identity.base, BASE);
  assert.deepEqual(manifest(dir, identity.inputs.map(([p]) => p)), identity.inputs, `${label}: frozen inputs changed`);
  assert.equal(hash(JSON.stringify(identity.inputs)), identity.input_sha256);
  return dir;
}
function prepare() {
  assert.equal(git('rev-parse', 'HEAD').toString().trim(), BASE, 'unexpected HEAD');
  assert(!existsSync(join(STORE, 'A')), 'snapshots already exist; verify/reuse, never overwrite');
  assert(!existsSync(join(STORE, 'C0')), 'C0 already exists');
  const paths = files().filter((p) => !p.startsWith('docs/') && !p.startsWith('.github/'));
  assert(paths.includes('Cargo.lock'));
  // Include the one new oracle without inventing a Cargo manifest.
  paths.push('tests/r16_perf84_activation.rs');
  paths.sort();
  const data = new Map();
  for (const p of paths) {
    assert(!p.includes('..') && !p.startsWith('/'));
    assert(!lstatSync(join(ROOT, p)).isSymbolicLink(), `symlink input rejected: ${p}`);
    // A src is EXACT HEAD, even if another agent changes working-tree comments.
    data.set(p, p.startsWith('src/') ? git('show', `${BASE}:${p}`) : readFileSync(join(ROOT, p)));
  }
  data.set('scripts/iai.mjs', Buffer.from(generatedHarness(data.get('scripts/iai.mjs').toString())));
  mkdirSync(STORE, { recursive: true });
  const benchPatch = git('diff', '--binary', BASE, '--', 'benches/perf_gate_iai.rs');
  writeFileSync(join(STORE, 'bench.patch'), benchPatch);
  for (const label of ['A', 'C0']) {
    const dir = join(STORE, label);
    for (const [p, bytes] of data) {
      mkdirSync(dirname(join(dir, p)), { recursive: true });
      writeFileSync(join(dir, p), bytes);
    }
    const inputs = manifest(dir, paths);
    const identity = { base: BASE, tree: git('rev-parse', `${BASE}^{tree}`).toString().trim(),
      features: FEATURES, input_sha256: hash(JSON.stringify(inputs)),
      bench_patch_sha256: hash(benchPatch), bench_sha256: hash(data.get('benches/perf_gate_iai.rs')),
      harness_sha256: hash(data.get('scripts/iai.mjs')), inputs };
    writeFileSync(join(STORE, `${label}.identity.json`), JSON.stringify(identity, null, 2) + '\n');
    verify(label);
  }
  assert.equal(readFileSync(join(STORE, 'A.identity.json')).toString(), readFileSync(join(STORE, 'C0.identity.json')).toString());
  console.log('Prepared identical A/C0; A src from pinned HEAD; no builds or measurements.');
}

function prepareB() {
  const a = verify('A');
  assert(!existsSync(join(STORE, 'B')), 'B already exists; never overwrite');
  const identity = JSON.parse(readFileSync(join(STORE, 'A.identity.json')));
  assert.equal(hash(readFileSync(join(ROOT, 'benches/perf_gate_iai.rs'))), identity.bench_sha256, 'benchmark pair must be identical');
  const dir = join(STORE, 'B');
  for (const [p] of identity.inputs) {
    mkdirSync(dirname(join(dir, p)), { recursive: true });
    writeFileSync(join(dir, p), readFileSync(join(OWNED.includes(p) ? ROOT : a, p)));
  }
  const changedSrc = git('diff', '--name-only', '--', 'src').toString().trim().split('\n').filter(Boolean);
  assert(changedSrc.every((p) => OWNED.includes(p)), 'candidate src diff must contain only owned files');
  const patch = git('diff', '--ignore-cr-at-eol', '--', 'src');
  assert(patch.length > 0, 'no candidate patch');
  writeFileSync(join(STORE, 'candidate.patch'), patch);
  identity.inputs = manifest(dir, identity.inputs.map(([p]) => p));
  identity.input_sha256 = hash(JSON.stringify(identity.inputs));
  identity.candidate_patch_sha256 = hash(patch);
  identity.candidate_patch_command = 'git diff --ignore-cr-at-eol -- src';
  writeFileSync(join(STORE, 'B.identity.json'), JSON.stringify(identity, null, 2) + '\n');
  verify('B');
  console.log('Frozen B: only two owned src files differ from frozen A');
}

async function execute(cmd, args, cwd, logName) {
  const destination = join(STORE, logName);
  assert(!existsSync(destination), `refuse to overwrite receipt: ${logName}`);
  let output = '';
  const child = spawn(cmd, args, { cwd, shell: false, detached: process.platform !== 'win32' });
  // Kill only OUR tree on the prescribed ten-minute anomaly, never global WSL.
  let timedOut = false;
  const timer = setTimeout(() => {
    timedOut = true;
    if (process.platform === 'win32') {
      execFileSync('taskkill', ['/PID', String(child.pid), '/T', '/F']);
    } else {
      process.kill(-child.pid, 'SIGKILL');
    }
  }, 600_000);
  for (const stream of [child.stdout, child.stderr]) stream.on('data', (b) => {
    output += b.toString();
    process.stdout.write(b);
  });
  const code = await new Promise((resolve, reject) => {
    child.on('error', reject);
    child.on('close', resolve);
  }).finally(() => clearTimeout(timer));
  // Stored artifacts never contain this machine's absolute checkout path.
  const sanitized = output.replaceAll(ROOT, '<repo>').replaceAll(ROOT.replaceAll('\\', '/'), '<repo>')
    .replaceAll(wslPath(ROOT), '<repo>');
  writeFileSync(destination, sanitized);
  assert(!timedOut, 'own process tree terminated after ten-minute anomaly');
  assert.equal(code, 0, `command failed; see target/r16-perf84/${logName}`);
}

const [mode, name] = process.argv.slice(2);
if (mode === '--prepare') prepare();
else if (mode === '--prepare-b') prepareB();
else if (mode === '--verify') { verify('A'); verify('C0'); console.log('Frozen A/C0 identities verified'); }
else if (mode === '--compile' || mode === '--activation') {
  const label = name ?? 'A';
  assert(['A', 'C0', 'B'].includes(label));
  const dir = verify(label);
  const command = mode === '--compile'
    ? `cargo bench --locked --no-run --bench perf_gate_iai --features ${quote(FEATURES)}`
    : `cargo test --locked --features ${quote(FEATURES)} --test r16_perf84_activation -- --test-threads=1 --nocapture`;
  await execute('wsl.exe', ['bash', '-lc', `cd ${quote(wslPath(dir))} && RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_BUILD_JOBS=3 CARGO_TARGET_DIR="$PWD/target" ${command}`], dir, `${label}${mode}.log`);
  verify(label);
} else if (mode === '--run') {
  assert(['A1', 'A2', 'C0', 'B1', 'B2'].includes(name), 'explicit run label required');
  const label = name.startsWith('B') ? 'B' : name === 'C0' ? 'C0' : 'A';
  const dir = verify(label);
  // Pin compiler/tool versions and activation separately before --run.
  await execute(process.execPath, ['scripts/iai.mjs', ...BENCHES], dir, `${name}.log`);
  verify(label);
} else throw new Error('Usage: --prepare | --prepare-b (later candidate only) | --verify | --compile [A|C0|B] | --activation [A|C0|B] | --run A1|A2|C0|B1|B2');
