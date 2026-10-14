// R20 PRF-04 heap reuse gate RUNNER. Executes the preregistered pipeline; the judge
// (r20_prf04_heap_reuse_gate.mjs) and the prereg are frozen inputs and are
// never modified here. The frozen file list lives in the judge (--hashes /
// --check-hashes); this runner is one of those files.
//
// Modeled on scripts/r20_prf04_claim_path_run.mjs (stage pipeline, identity
// manifest, sanitised logs) and inheriting the producer-boundary machinery
// (EntryPoint::Custom profiles, nm/disassembly receipts, Amendment B coverage)
// from scripts/r19_prf01_sidecar_summary_run.mjs in a reduced form.
//
// Commands:
//   --prepare   worktrees <primary>/worktrees/r20p4/{A,B,C} (equal length,
//               one-letter labels); B = A + cherry-pick edf90675 on a temp
//               branch; identity manifest BEFORE any measurement.
//   --build     per-arm builds (native on Linux, wsl.exe on Windows):
//               r20_prf04_heap_reuse_iai (iai), r20_prf04_heap_reuse_l2_iai (controls),
//               r20_prf04_heap_reuse_oracle example under cfg prf04_heap_reuse_activation,
//               r20_prf04_heap_reuse_iai control under cfg prf04_heap_reuse_oracle.
//               C is built with -C debuginfo=line-tables-only (registered).
//   --oracle    r20_prf04_heap_reuse_oracle on A and B for every needed mode + probes.
//   --measure   iai runs A1,A2,B1,B2,C0 (both bench targets) + one producer
//               control run per label; parses Ir and child profiles; builds
//               the judge input; invokes the judge; derives summary.csv from
//               the SAME JSON.
//   --rehearsal full pipeline WITHOUT a real B in the iai stage (arm B = A'
//               under the registered distinguishing flags); the oracle stage
//               still uses the real B (path activation only).
//   --slop      inject +11 Ir into one small_churn_16b_2n B1 cell (negative
//               control: expected NO-GO/INVALID).
//   --cleanup   remove arm worktrees, temp branch, WSL target dirs.
//
// KNOWN LIMITATIONS (disclosed in the preregistration): no two-phase
// capture/identity-review protocol. Identity is instead re-validated at EVERY
// stage: --build, --oracle and --measure each recompute every arm's
// archIdentity (HEAD/tree/identitySha256, clean-tree) and refuse to run on a
// mismatch with manifest.json; builds.json records manifestSha256 and
// oracle.json records manifestSha256 + buildsSha256, both checked by
// --measure. The constructionGainReceipt is NOT produced automatically (the
// judge requires it only when Ir_B - Ir_A < -T_s); the runner RETAINS the
// construction cell's Callgrind profiles plus a sha256 inventory before
// deleting the observation home, so a human can complete the receipt without
// re-measuring and re-run the judge alone. The --slop flag is a rehearsal-only
// negative control: an official measure with it is refused, and a sloped
// rehearsal writes a durable counterfactual marker into input.json and
// summary.csv, which the judge rejects (INVALID) outside rehearsal.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve, sep, basename } from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CELLS,
  RELEASE_LAYER,
  FROZEN_FILES as CANONICAL_FROZEN_FILES,
  validateActivation,
  isL2AllocCaller,
  hasL2FirstBindPath,
  syntheticCallgrindProfile,
  fakeBenchProgram,
  fakeOracleProgram,
} from './r20_prf04_heap_reuse_support.mjs';
import { runRunnerSelfTest } from './r20_prf04_heap_reuse_support.mjs';
const CANDIDATE = 'TBD';

const RUNNER_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(RUNNER_DIR, '..');
const REPO_ROOT = REPO;
const JUDGE = 'scripts/r20_prf04_heap_reuse_gate.mjs';
const PREREG = 'docs/perf/R20_PRF04_HEAP_REUSE_PREREG.md';
const FROZEN_FILES = [
  'benches/r20_prf04_heap_reuse_iai.rs', 'benches/r20_prf04_heap_reuse_l2_iai.rs',
  'benches/support/r20_prf04_heap_reuse.rs', 'examples/r20_prf04_heap_reuse_oracle.rs',
  'examples/r20_prf04_heap_reuse_l2_oracle.rs',
];
const IAI_TARGET = 'r20_prf04_heap_reuse_iai';
const CONTROLS_TARGET = 'r20_prf04_heap_reuse_l2_iai';
const ORACLE_TARGET = 'r20_prf04_heap_reuse_oracle';
const L2_ORACLE_TARGET = 'r20_prf04_heap_reuse_l2_oracle';
const FEATURES = 'production bench-internals internals';
const ORACLE_CFG = 'prf04_heap_reuse_activation';
const CONTROL_CFG = 'prf04_heap_reuse_oracle';
const DISTINGUISH_RUSTFLAGS = '-C debuginfo=line-tables-only';
const DEFAULT_OUT = 'docs/perf/R20_PRF04_HEAP_REUSE_RUN01';
const REHEARSAL_OUT = 'target/r20p4-rehearsal';
const RUNS = ['A1', 'A2', 'B1', 'B2', 'C0'];
const RUN_ARM = { A1: 'A', A2: 'A', B1: 'B', B2: 'B', C0: 'C' };
const RAW_LOG_LIMIT = 2 * 1024 * 1024;


const sha = b => createHash('sha256').update(b).digest('hex');
const shaFile = p => sha(fs.readFileSync(p));
const now = () => new Date().toISOString();
const IS_WIN = process.platform === 'win32';
const winToWsl = p => {
  const m = /^([A-Za-z]):(.*)$/.exec(p);
  assert.ok(m, `not an absolute Windows path: ${p}`);
  return `/mnt/${m[1].toLowerCase()}${m[2].replace(/\\/g, '/')}`;
};
const toHost = p => (IS_WIN ? winToWsl(p) : p);
// Private checkout paths never reach a stored artifact.
const publicPath = p => {
  const normalized = String(p).split(String.fromCharCode(92)).join('/');
  const root = REPO.split(String.fromCharCode(92)).join('/');
  if (normalized.startsWith('<repo>/')) return normalized;
  assert.ok(normalized.startsWith(root + '/'), 'path is outside repository: ' + p);
  return '<repo>' + normalized.slice(root.length);
};
const dispatchExe = p => IS_WIN ? String(p) : toHost(String(p));
const executablePath = recorded => {
  const raw = String(recorded);
  let resolved;
  if (raw.startsWith('<repo>/') || raw.startsWith('<repo>\\')) resolved = resolve(REPO, raw.slice(7));
  else if (isAbsolute(raw)) resolved = resolve(raw);
  else {
    assert.ok(IS_WIN && /^[A-Za-z]:[\\/]/.test(raw), 'unsupported recorded executable path: ' + raw);
    resolved = resolve(raw);
  }
  const root = resolve(buildRoot());
  const relativePathname = relative(root, resolved);
  assert.ok(relativePathname !== '' && relativePathname !== '..' &&
    !relativePathname.startsWith('..' + sep) && !isAbsolute(relativePathname),
    'recorded executable is outside build root: ' + recorded);
  return resolved;
};
const publicText = t => String(t)
  .replaceAll(REPO_ROOT, '<repo>').replaceAll(toHost(REPO_ROOT), '<repo>')
  .replaceAll(REPO.split(String.fromCharCode(92)).join('/'), '<repo>')
  .replaceAll(JSON.stringify(REPO_ROOT).slice(1, -1), '<repo>')
  .replace(/[A-Za-z]:[\/][^\s"'<>|)]+/g, '<private>')
  .replace(/\/mnt\/[a-z]\/[^\s"'<>|)]+/g, '<private>')
  .replace(/\/home\/[^\s/"'<>|)]+/g, '<home>');

function git(cwd, args, { allowFailure = false } = {}) {
  const r = spawnSync('git', ['--no-optional-locks', ...args], { cwd, encoding: 'utf8',
    maxBuffer: 1 << 28, env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } });
  if (!allowFailure && r.status !== 0) throw new Error(`git ${args[0]} failed: ${r.stderr}`);
  return { code: r.status, out: (r.stdout ?? '').trim(), err: r.stderr ?? '' };
}
function wsl(script, { timeout = 600_000 } = {}) {
  assert.equal(IS_WIN, false, 'heavy stages require Linux CI; WSL is forbidden');
  const exe = 'bash';
  const args = IS_WIN ? ['bash', '-lc', script] : ['-lc', script];
  const r = spawnSync(exe, args, { encoding: 'utf8', maxBuffer: 1 << 28, timeout,
    env: { ...process.env, MSYS_NO_PATHCONV: '1' } });
  return {
    code: r.status ?? -1,
    out: r.stdout ?? '',
    err: r.stderr ?? '',
    timedOut: r.error?.code === 'ETIMEDOUT',
  };
}
function wslOk(script, what, opts) {
  const r = wsl(script, opts);
  assert.equal(r.code, 0,
    `${what} failed (exit ${r.code})\n--- stderr tail ---\n${r.err.split('\n').slice(-40).join('\n')}`);
  return r;
}

const ctx = { outDir: null, rehearsal: false, manifest: null, builds: null, oracle: null, adapter: null };
const local = (...p) => join(ctx.outDir, ...p);
function putText(name, text) {
  const out = publicText(text);
  fs.mkdirSync(dirname(local(name)), { recursive: true });
  fs.writeFileSync(local(name), out);
  return { path: name, sha256: sha(out) };
}
const putJson = (name, obj) => putText(name, JSON.stringify(obj, null, 2) + '\n');
function readStage(name, fn) {
  const p = local(name);
  assert.ok(fs.existsSync(p), `stage file missing: ${p} (run the earlier subcommand first)`);
  fn(JSON.parse(fs.readFileSync(p, 'utf8')));
}
function frozenHashes() {
  if (ctx.adapter?.frozenHashes) return ctx.adapter.frozenHashes();
  const check = spawnSync(process.execPath, [join(REPO, JUDGE), '--check-hashes'], {
    encoding: 'utf8',
  });
  assert.equal(check.status, 0, `frozen hash drift (a frozen file was edited?)\n${check.stdout}${check.stderr}`);
  const list = spawnSync(process.execPath, [join(REPO, JUDGE), '--hashes'], { encoding: 'utf8' }).stdout;
  return list.trim().split('\n').map(l => { const [h, path] = l.split(/\s+/); return { path, sha256: h }; });
}

// Per-stage identity binding (re-freeze 9): rebuild the prepare-stage identities from the live
// worktrees and compare with manifest.json before any stage does its work. The official out-dir
// (docs/perf/R20_PRF04_HEAP_REUSE_RUN01) lives in the PRIMARY checkout and is not part of
// the worktrees A/B/C, so none of these checks read it.
function verifyIdentities(stage) {
  const m = ctx.manifest;
  for (const label of ['A', 'B', 'C']) {
    const saved = m.arches[label];
    const head = label === 'B' ? m.candidateHead : m.baseHead;
    if (!ctx.adapter) {
      assert.equal(git(REPO, ['rev-parse', head]).out, head, 'source ref changed at ' + stage);
      assert.equal(git(REPO, ['rev-parse', head + '^{tree}']).out, saved.tree, 'source tree drift @' + label);
    } else assert.equal(ctx.adapter.resolveRef(head), head, 'fake source ref drift @' + label);
    assert.equal(snapshotIdentity(join(snapshotRoot(), label)).identitySha256, saved.snapshotSha256,
      'snapshot drift @' + label + '/' + stage);
  }
  assert.equal(overlayIdentity().sha256, m.overlaySha256, 'harness overlay changed at ' + stage);
}
function baseHead() {
  const rel = ctx.outDir ? relative(REPO, ctx.outDir).split(sep).join('/') : '';
  const own = rel && !rel.startsWith('..') ? [':(exclude)' + rel] : [];
  assert.equal(git(REPO, ['status', '--porcelain=v1', '--untracked-files=all', '--', '.', ...own]).out, '',
    'runner source worktree must be clean outside output and ignored target');
  return git(REPO, ['rev-parse', 'HEAD']).out;
}
function overlayFiles() {
  return ['benches/r20_prf04_heap_reuse_iai.rs', 'benches/r20_prf04_heap_reuse_l2_iai.rs',
    'benches/support/r20_prf04_heap_reuse.rs', 'examples/r20_prf04_heap_reuse_oracle.rs',
    'examples/r20_prf04_heap_reuse_l2_oracle.rs'].filter(p => fs.existsSync(join(REPO, p)));
}
function overlayIdentity() {
  const files = overlayFiles().sort().map(path => ({ path, sha256: shaFile(join(REPO, path)) }));
  return { files, sha256: sha(JSON.stringify(files)) };
}
function snapshotIdentity(dir) {
  const files = [];
  const walk = (base, rel = '') => {
    for (const name of fs.readdirSync(join(base, rel)).sort()) {
      const p = join(base, rel, name), r = join(rel, name).replaceAll('\\', '/');
      if (fs.statSync(p).isDirectory()) walk(base, r);
      else files.push({ path: r, sha256: shaFile(p) });
    }
  };
  walk(dir);
  return { files, identitySha256: sha(JSON.stringify(files)) };
}
const SNAPSHOT_ROOT = join(REPO, 'target', 'r20p4-sources');
const snapshotRoot = () => ctx.adapter?.snapshotRoot ?? SNAPSHOT_ROOT;
const buildRoot = () => ctx.adapter?.buildRoot ?? join(REPO, 'target', 'r20p4-builds');
function archiveSnapshot(label, head) {
  const dest = join(snapshotRoot(), label);
  fs.mkdirSync(dest, { recursive: true });
  if (ctx.adapter) ctx.adapter.archiveSnapshot(label, head, dest);
  else {
    const ar = spawnSync('git', ['--no-optional-locks', 'archive', '--format=tar', head],
      { cwd: REPO, maxBuffer: 1 << 30, env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } });
    assert.equal(ar.status, 0, 'git archive failed: ' + ar.stderr);
    const extraction = spawnSync('tar', ['-xf', '-', '-C', dest], { input: ar.stdout, maxBuffer: 1 << 20 });
    assert.equal(extraction.status, 0, 'archive extraction failed: ' + extraction.stderr);
  }
  for (const rel of overlayFiles()) {
    const to = join(dest, rel); fs.mkdirSync(dirname(to), { recursive: true });
    fs.copyFileSync(join(REPO, rel), to);
  }
  const cargo = join(dest, 'Cargo.toml');
  const manifestText = fs.readFileSync(cargo, 'utf8');
  const additions = [
    ['bench', 'r20_prf04_heap_reuse_iai', '\n[[bench]]\nname = "r20_prf04_heap_reuse_iai"\nharness = false\n'],
    ['bench', 'r20_prf04_heap_reuse_l2_iai', '\n[[bench]]\nname = "r20_prf04_heap_reuse_l2_iai"\nharness = false\n'],
    ['example', 'r20_prf04_heap_reuse_oracle', '\n[[example]]\nname = "r20_prf04_heap_reuse_oracle"\n'],
    ['example', 'r20_prf04_heap_reuse_l2_oracle', '\n[[example]]\nname = "r20_prf04_heap_reuse_l2_oracle"\n'],
  ];
  for (const [kind, name, record] of additions)
    if (!new RegExp('\\[\\[' + kind + '\\]\\]\\s*name\\s*=\\s*[\"\']' + name).test(manifestText))
      fs.appendFileSync(cargo, record);
  return dest;
}
function archIdentity(label, head, dir) {
  const tree = ctx.adapter ? ctx.adapter.tree(head) : git(REPO, ['rev-parse', head + '^{tree}']).out;
  const snap = snapshotIdentity(dir);
  const lock = join(dir, 'Cargo.lock');
  if (!fs.existsSync(lock)) {
    const checkoutLock = ctx.adapter?.lockPath ?? join(REPO, 'Cargo.lock');
    assert.ok(fs.existsSync(checkoutLock), 'source snapshot lacks Cargo.lock and CI checkout lock is absent');
    fs.copyFileSync(checkoutLock, lock);
  }
  return { label, head, tree, snapshotSha256: snap.identitySha256, inventory: snap.files,
    patchSha256: sha(''), untrackedSha256: sha('[]'),
    identitySha256: sha(JSON.stringify([label, head, tree, snap.identitySha256])),
    cargoLock: {
      path: fs.existsSync(join(dir, 'Cargo.lock')) ? 'snapshot/Cargo.lock' : 'CI checkout Cargo.lock',
      sha256: shaFile(lock),
    },
    capturedAt: now() };
}
const sourceOf = a => (({ head, patchSha256, untrackedSha256, identitySha256, capturedAt }) =>
  ({ baseHead: head, patchSha256, untrackedSha256, identitySha256, capturedAt }))(a);
function loadFallbackReceipt(outDir, expectedBaseHead) {
  const file = join(outDir, 'fallbackReceipt.json');
  assert.ok(fs.existsSync(file), 'L1 requires pre-existing fallbackReceipt.json');
  const receipt = JSON.parse(fs.readFileSync(file, 'utf8'));
  assert.equal(receipt.reason, 'guard-drop-uncollectable');
  assert.equal(receipt.rehearsal, true);
  assert.equal(receipt.source?.baseHead, expectedBaseHead);
  for (const key of ['symbolReceipt', 'profileReceipt', 'rawLogReceipt']) {
    const rel = receipt[key];
    assert.ok(typeof rel === 'string' && !isAbsolute(rel) && !rel.split(/[\\/]/).includes('..'),
      'unsafe fallback receipt path: ' + key);
    assert.ok(fs.statSync(join(outDir, rel)).isFile(), 'fallback receipt file missing: ' + rel);
  }
  return receipt;
}
function cmdPrepare() {
  ctx.outDir ??= stagePaths();
  if (!ctx.rehearsal && CANDIDATE === 'TBD' && !ctx.adapter)
    throw new Error('official prepare blocked: CANDIDATE is TBD; bind reviewed candidate first');
  assert.ok(!fs.existsSync(local('manifest.json')), 'manifest exists; refusing overwrite');
  if (!ctx.adapter && !ctx.rehearsal) assert.equal(process.platform, 'linux', 'prepare requires Linux CI');
  const resolveRef = ref => ctx.adapter ? ctx.adapter.resolveRef(ref) : git(REPO, ['rev-parse', ref]).out;
  const candidateHead = ctx.adapter ? ctx.adapter.candidate : ctx.rehearsal
    ? resolveRef('origin/main') : resolveRef(CANDIDATE);
  const base = ctx.adapter ? ctx.adapter.base : ctx.rehearsal
    ? candidateHead : git(REPO, ['merge-base', candidateHead, 'origin/main']).out;
  const heads = { A: base, B: candidateHead, C: base };
  assert.ok(!fs.existsSync(snapshotRoot()), 'snapshot root exists; refusing to overwrite');
  fs.mkdirSync(snapshotRoot(), { recursive: true });
  for (const label of ['A', 'B', 'C']) archiveSnapshot(label, heads[label]);
  const overlay = overlayIdentity();
  const arches = Object.fromEntries(['A', 'B', 'C'].map(label =>
    [label, archIdentity(label, heads[label], join(snapshotRoot(), label))]));
  assert.equal(arches.A.snapshotSha256, arches.C.snapshotSha256, 'C0 must exactly match A');
  const manifest = { version: 1, rehearsal: ctx.rehearsal, capturedAt: now(), baseHead: base,
    candidateHead, measurementCommit: base, releaseLayer: RELEASE_LAYER,
    sourceRoot: publicText(snapshotRoot()), overlay, overlaySha256: overlay.sha256, arches,
    pathLengths: ['A', 'B', 'C'].map(label => join(snapshotRoot(), label).length) };
  if (RELEASE_LAYER === 'L1') manifest.fallbackReceipt = loadFallbackReceipt(ctx.outDir, base);
  assert.equal(new Set(manifest.pathLengths).size, 1, 'snapshot paths differ in length');
  putJson('manifest.json', manifest);
}
function envCapture() {
  assert.equal(process.platform, 'linux', 'heavy CI stages require native Linux');
  const rows = [];
  for (const [cmd, args] of [['rustc', ['-Vv']], ['cargo', ['-V']], ['valgrind', ['--version']],
    ['uname', ['-srm']], ['iai-callgrind-runner', ['--version']]]) {
    const r = spawnSync(cmd, args, { cwd: REPO, encoding: 'utf8', maxBuffer: 1 << 20 });
    if (cmd === 'iai-callgrind-runner') {
      // As in v2: the runner refuses a bare --version call (no library version handshake) but
      // names its own version in the error; record that text and require the version in it.
      const text = ((r.stdout ?? '') + (r.stderr ?? '')).replace(/\x1b\[[0-9;]*m/g, '').trim();
      assert.match(text, /iai-callgrind-runner \(?\d+[.]\d+[.]\d+/, 'iai-callgrind-runner version not reported');
      rows.push(text);
      continue;
    }
    assert.equal(r.status, 0, cmd + ' environment capture failed: ' + r.stderr);
    rows.push(r.stdout.trim());
  }
  rows.push('features: ' + FEATURES, 'releaseLayer: ' + RELEASE_LAYER);
  const text = rows.join('\n') + '\n';
  return { environmentSha256: sha(text), text };
}
function executableOf(jsonOut, target) {
  const hits = [];
  for (const line of jsonOut.split('\n').filter(Boolean)) {
    try {
      const m = JSON.parse(line);
      if (m.reason === 'compiler-artifact' && m.target?.name === target && m.executable) {
        hits.push(m.executable);
      }
    }
    catch { /* ignore cargo human output */ }
  }
  assert.ok(hits.length, 'no executable reported for ' + target);
  return hits[hits.length - 1];
}
function cargoBuild({ arm, role, target, kind, rustflags = '' }) {
  if (ctx.adapter) return ctx.adapter.cargoBuild({ arm, role, target, kind, rustflags });
  assert.equal(process.platform, 'linux', 'build stage requires Linux CI');
  const dir = join(snapshotRoot(), arm), td = join(buildRoot(), arm + '-' + role);
  fs.mkdirSync(td, { recursive: true });
  const env = { ...process.env, CARGO_TARGET_DIR: td, RUSTC_WRAPPER: '', CARGO_BUILD_RUSTC_WRAPPER: '' };
  if (rustflags) env.CARGO_BUILD_RUSTFLAGS = rustflags;
  const args = [kind, '--locked', '--message-format=json-render-diagnostics',
    ...(kind === 'bench' ? ['--bench', target, '--no-run'] : ['--example', target]), '--features', FEATURES];
  const r = spawnSync('cargo', args, { cwd: dir, env, encoding: 'utf8', maxBuffer: 1 << 28 });
  assert.equal(r.status, 0, 'cargo build ' + arm + '/' + role + ' failed\n' + r.stderr);
  const exe = executableOf(r.stdout, target);
  return { role, target, kind, exe, binarySha256: shaFile(exe), features: FEATURES.split(' '),
    rustflags, targetDir: td };
}
function cmdBuild() {
  readStage('manifest.json', m => { ctx.manifest = m; ctx.rehearsal = m.rehearsal === true; });
  verifyIdentities('build');
  const env = ctx.adapter ? ctx.adapter.envCapture() : envCapture(); putText('environment.log', env.text);
  const startedAt = now(), arms = {};
  for (const arm of ['A', 'B']) {
    const oracleCfg = '--cfg ' + ORACLE_CFG + (arm === 'B' && !ctx.manifest.rehearsal ? ' --cfg ' + CONTROL_CFG : '');
    arms[arm] = {
      iai: cargoBuild({ arm, role: 'iai', target: IAI_TARGET, kind: 'bench' }),
      controls: cargoBuild({ arm, role: 'controls', target: CONTROLS_TARGET, kind: 'bench' }),
      oracle: cargoBuild({ arm, role: 'oracle', target: ORACLE_TARGET, kind: 'build', rustflags: oracleCfg }),
      control: cargoBuild({ arm, role: 'control', target: 'r20_prf04_heap_reuse_l2_oracle',
        kind: 'build', rustflags: oracleCfg }),
    };
  }
  arms.C = {
    iai: cargoBuild({
      arm: 'C',
      role: 'iai',
      target: IAI_TARGET,
      kind: 'bench',
      rustflags: DISTINGUISH_RUSTFLAGS,
    }),
    controls: cargoBuild({
      arm: 'C',
      role: 'controls',
      target: CONTROLS_TARGET,
      kind: 'bench',
      rustflags: DISTINGUISH_RUSTFLAGS,
    }),
    control: cargoBuild({ arm: 'C', role: 'control', target: 'r20_prf04_heap_reuse_l2_oracle',
      kind: 'build', rustflags: '--cfg ' + ORACLE_CFG }),
  };
  const manifestSha256 = shaFile(local('manifest.json'));
  putJson('builds.json', {
    version: 1,
    buildStartedAt: startedAt,
    capturedAt: now(),
    manifestSha256,
    environmentSha256: env.environmentSha256,
    arms,
  });
  readStage('builds.json', value => { ctx.builds = value; });
}
function oracleRecord(arm, cell, label) {
  const role = cell.layer === 'L1' ? 'oracle' : 'control';
  const built = ctx.builds.arms[arm][role];
  const sourceArm = arm === 'C' ? 'A' : arm;
  const source = sourceOf(ctx.manifest.arches[sourceArm]);
  const exe = executablePath(built.exe);
  const beforeSha256 = shaFile(exe);
  assert.equal(beforeSha256, built.binarySha256,
    'oracle executable drift before launch @' + label + '/' + cell.name);
  const startedAt = now();
  const args = [cell.family, String(cell.n), arm];
  const run = (ctx.adapter?.oracleRecord ?? ((file, argv, options) => spawnSync(file, argv, options)))
    (exe, args, { cwd: join(snapshotRoot(), sourceArm), encoding: 'utf8',
    maxBuffer: 1 << 24,
    timeout: 120000,
    env: { ...process.env } });
  const stdout = run.stdout ?? '', stderr = run.stderr ?? '';
  const rawLogReceipt = putText('raw/oracle-' + label + '-' + cell.name + '.log',
    'argv=' + JSON.stringify([exe, ...args]) + '\nexit=' + run.status +
    '\nstdout:\n' + stdout + '\nstderr:\n' + stderr);
  assert.equal(run.status, 0, 'oracle process failed @' + label + '/' + cell.name + ': ' + stderr);
  assert.equal(shaFile(exe), beforeSha256, 'oracle executable changed during launch');
  const rec = JSON.parse(stdout.trim());
  assert.equal(rec.arm, arm);
  assert.equal(rec.family, cell.family);
  assert.equal(rec.n, cell.n);
  const observerExpected = arm === 'B' && !ctx.manifest.rehearsal;
  rec.cfg = [ORACLE_CFG, ...(observerExpected ? [CONTROL_CFG] : [])];
  if (observerExpected) assert.equal(rec.observer, true, 'B candidate observer absent');
  else assert.equal(rec.observer, false, 'A/rehearsal observer must be disabled');
  assert.ok(validateActivation({ arm: observerExpected ? 'B' : 'A', family: cell.family,
    observer: rec.observer, heap_reuse_spill_bits_delta: rec.heap_reuse_spill_bits_delta,
    heap_reuse_take_hits: rec.heap_reuse_take_hits }), 'oracle activation invalid @' + label + '/' + cell.name);
  const expectedCalls = cell.layer === 'L1' ? 1 : cell.n;
  assert.equal(rec.actualTargetCalls, expectedCalls, 'oracle target call count mismatch');
  const receipt = putJson('oracle/' + label + '-' + cell.name + '.json', rec);
  return { row: rec, binarySha256: beforeSha256, source, startedAt, receipt,
    rawLogReceipt, role, runId: label + '-' + cell.name };
}
function cmdOracle() {
  readStage('manifest.json', m => { ctx.manifest = m; ctx.rehearsal = m.rehearsal === true; });
  readStage('builds.json', b => { ctx.builds = b; });
  verifyIdentities('oracle');
  assert.equal(ctx.builds.manifestSha256, shaFile(local('manifest.json')),
    'builds.json does not belong to this manifest');
  const records = {};
  const labels = ctx.manifest.rehearsal ? ['A'] : ['A', 'B'];
  for (const arm of labels) {
    records[arm] = {};
    for (const cell of CELLS) records[arm][cell.name] = oracleRecord(arm, cell, arm);
  }
  putJson('oracle.json', { version: 1, capturedAt: now(),
    manifestSha256: shaFile(local('manifest.json')), buildsSha256: shaFile(local('builds.json')),
    environmentSha256: ctx.builds.environmentSha256, releaseLayer: RELEASE_LAYER, records });
}

// Callgrind parser and target-boundary validation.
function naturalBig(token) {
  assert.match(token, /^(?:\d+|0x[0-9a-fA-F]+)$/);
  return BigInt(token);
}
function safeNumber(value) {
  assert.ok(value >= 0n && value <= BigInt(Number.MAX_SAFE_INTEGER));
  return Number(value);
}
function positionToken(token, previous) {
  if (token === '*') { assert.notEqual(previous, null); return previous; }
  if (/^[+-]/.test(token)) {
    assert.notEqual(previous, null);
    const value = previous + (token[0] === '-' ? -1n : 1n) * naturalBig(token.slice(1));
    assert.ok(value >= 0n); return value;
  }
  return naturalBig(token);
}
export function profileFunctions(text) {
  const events = /^events: (.+)$/m.exec(text)?.[1].trim().split(/\s+/);
  assert.ok(events, 'Callgrind events absent');
  const irIndex = events.indexOf('Ir');
  assert.ok(irIndex >= 0, 'Callgrind Ir event absent');
  const positions = /^positions: (.+)$/m.exec(text)?.[1].trim().split(/\s+/) ?? ['line'];
  const ids = new Map(), functions = new Map();
  let current = null, callee = null, pending = false, previous = Array(positions.length).fill(null);
  const symbol = value => {
    const m = /^(?:\((\d+)\)\s*)?(.*)$/.exec(value);
    if (m[2]) return m[2];
    assert.ok(ids.has(m[1]), 'unknown compressed function ID'); return ids.get(m[1]);
  };
  const fn = name => {
    if (!functions.has(name)) functions.set(name, { selfIr: 0n, edges: new Map(), edgeCalls: new Map() });
    return functions.get(name);
  };
  for (const line of text.split(/\r?\n/)) {
    const m = /^(?:c?fn)=\((\d+)\)\s+(.+)$/.exec(line);
    if (m) ids.set(m[1], m[2]);
  }
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith('fn=')) {
      current = symbol(line.slice(3)); fn(current); callee = null;
      previous = Array(positions.length).fill(null);
    } else if (line.startsWith('cfn=')) { callee = symbol(line.slice(4)); fn(callee); }
    else if (line.startsWith('calls=')) {
      assert.ok(current && callee && !pending, 'invalid Callgrind calls record');
      const fields = line.slice(6).trim().split(/\s+/);
      positions.forEach((_, i) => positionToken(fields[i + 1], previous[i]));
      const f = fn(current); f.edgeCalls.set(callee, (f.edgeCalls.get(callee) ?? 0n) + naturalBig(fields[0]));
      pending = true;
    } else if (/^[*+\-\d]/.test(line)) {
      assert.ok(current, 'Callgrind cost without function');
      const fields = line.trim().split(/\s+/);
      previous = positions.map((_, i) => positionToken(fields[i], previous[i]));
      const costs = fields.slice(positions.length).map(naturalBig), cost = costs[irIndex] ?? 0n;
      const f = fn(current);
      if (pending) { f.edges.set(callee, (f.edges.get(callee) ?? 0n) + cost); pending = false; }
      else f.selfIr += cost;
    } else if (line && !/^(?:#|[A-Za-z][A-Za-z_]*(?:=|:))/.test(line)) {
      throw Error('malformed Callgrind record');
    }
  }
  assert.ok(!pending, 'missing final Callgrind call cost');
  return functions;
}
export function argvTokens(value) {
  const result = []; let token = '', quote = null, active = false;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (quote) {
      if (c === quote) quote = null;
      else if (c === '\\' && quote === '"' && ['"', '\\'].includes(value[i + 1])) token += value[++i];
      else token += c;
      active = true;
    } else if (c === '"' || c === "'") { quote = c; active = true; }
    else if (/\s/.test(c)) { if (active) { result.push(token); token = ''; active = false; } }
    else { token += c; active = true; }
  }
  assert.equal(quote, null, 'unclosed argv quote'); if (active) result.push(token);
  return result;
}
function groupMembers(file, group) {
  const source = fs.readFileSync(file, 'utf8');
  const match = new RegExp('library_benchmark_group!\\(\\s*name\\s*=\\s*' + group +
    ';\\s*benchmarks\\s*=([\\s\\S]*?)\\);').exec(source);
  assert.ok(match, 'benchmark group missing: ' + group);
  return match[1].split(',').map(x => x.trim()).filter(Boolean);
}
function groupInventory() {
  const l1 = groupMembers(join(REPO, 'benches', IAI_TARGET + '.rs'), 'heap_reuse');
  const l2 = groupMembers(join(REPO, 'benches', CONTROLS_TARGET + '.rs'), 'heap_reuse_l2');
  assert.deepEqual(l1, CELLS.filter(c => c.layer === 'L1').map(c => c.name), 'L1 Rust inventory mismatch');
  assert.deepEqual(l2, CELLS.filter(c => c.layer === 'L2').map(c => c.name), 'L2 Rust inventory mismatch');
  return { L1: l1, L2: l2 };
}
export function generatedArguments(debug, log, cell, homeName, executable) {
  const commands = [...log.matchAll(/^==\d+==\s+Command:\s*(.+)$/gm)].map(m => m[1]);
  assert.equal(new Set(commands).size, 1, 'ambiguous measured argv');
  assert.ok(commands.length);
  const measured = commands[0], dispatch = argvTokens(measured);
  assert.equal(publicPath(dispatchExe(dispatch[0])), publicPath(executable), 'measured executable mismatch');
  const at = dispatch.indexOf('--iai-run');
  assert.equal(at, 1, 'dispatcher must be at argv index one');
  assert.ok(dispatch.length === 5 || dispatch.length === 6,
    'unexpected dispatcher arguments: ' + JSON.stringify(dispatch));
  assert.deepEqual(dispatch.slice(at + 1, at + 4), [cell.group, String(cell.index), '0']);
  if (dispatch.length === 6) assert.match(dispatch[5], /^(?:--bench|--module|[A-Za-z0-9_:-]+)$/);
  const records = [...debug.matchAll(/callgrind: Arguments: (.+)$/gm)].map(m => m[1]);
  // The out-file nests package/target/group dirs between the home and the cell dir,
  // so each homeName segment must appear as its own path segment, not contiguously.
  const segments = String(homeName).split(String.fromCharCode(92)).join('/').split('/').filter(Boolean);
  const matches = records.filter(line => line.includes('--callgrind-out-file=') &&
    line.split(/\s+(?=--[A-Za-z])/).some(token => {
      const path = token.replaceAll(String.fromCharCode(92), '/').replace(/^"/, '');
      return path.startsWith('--callgrind-out-file=')
        && segments.every(segment => path.includes('/' + segment + '/'));
    }));
  assert.ok(matches.length, 'actual runner arguments absent');
  const custom = cell.expectedSymbols[0];
  const candidates = matches.filter(line => line.includes('--toggle-collect=' + custom));
  assert.equal(candidates.length, 1, 'exactly one custom target toggle required');
  const toolVector = candidates[0].trim().split(/\s+(?=--[A-Za-z])/)
    .map(token => token.replace(/^"(.*)"$/, '$1'));
  const toggleFlags = toolVector.filter(token => token.startsWith('--toggle-collect='));
  assert.ok(toggleFlags.some(flag => flag.slice('--toggle-collect='.length) === custom),
    'canonical target toggle missing');
  assert.equal(toolVector.filter(token => token.includes('::__iai_callgrind_wrapper_mod')).length,
    0, 'implicit runner target toggle forbidden');
  assert.ok(!candidates[0].includes('*::__iai_callgrind_wrapper_mod::*'), 'default toggle forbidden');
  assert.ok(toolVector.includes('--separate-threads=yes'), 'separate threads missing');
  // iai 0.14 omits --collect-atstart when a toggle is set (--toggle-collect implies
  // collection off at start); an explicit value must still be "no" (R19 precedent).
  const atStart = toolVector.filter(token => token.split('=')[0] === '--collect-atstart');
  assert.ok(atStart.every(token => token === '--collect-atstart=no') && toggleFlags.length > 0,
    'collection must start disabled');
  const settings = ['--tool=callgrind', '--error-exitcode=0', '--trace-children=yes', '--fair-sched=try',
    '--I1=32768,8,64', '--D1=32768,8,64', '--LL=8388608,16,64', '--cache-sim=yes',
    '--compress-strings=no', '--compress-pos=no', '--dump-line=yes', '--dump-instr=no',
    '--combine-dumps=no', '--separate-threads=yes'];
  const expectedFlags = argvTokens(candidates[0]);
  for (const setting of settings) {
    const key = setting.split('=')[0];
    const values = toolVector.filter(flag => flag.split('=')[0] === key);
    assert.ok(values.length > 0, 'effective setting missing ' + key);
    assert.ok(values.every(value => value === setting), 'conflicting effective setting ' + key);
  }
  return {
    measuredArguments: measured,
    toolArguments: candidates[0],
    generatedArguments: candidates[0] + ' ' + measured,
    settings: toolVector,
    entryPoint: 'Custom',
    collectAtStart: false,
    defaultToggle: false,
    separateThreads: true,
    toggleCollect: custom,
  };
}
export function parseProfiles(profiles, log, options) {
  const pids = [...new Set([...log.matchAll(/^==(\d+)==/gm)].map(m => Number(m[1])))];
  assert.equal(pids.length, 1, 'actual valgrind PID missing/ambiguous');
  const args = generatedArguments(options.debug, log, options.cell, options.homeName, options.expectedExecutable);
  const measured = argvTokens(args.measuredArguments), threads = new Set(), entries = [];
  let totalIr = 0n, inclusiveIr = 0n, executions = 0n, callerContext = '';
  for (const text of profiles) {
    const header = key => {
      const found = [...text.matchAll(new RegExp('^' + key + ': (.+)$', 'gm'))];
      assert.equal(found.length, 1, 'missing/ambiguous header ' + key); return found[0][1];
    };
    assert.equal(safeNumber(naturalBig(header('pid'))), pids[0]);
    assert.equal(safeNumber(naturalBig(header('part'))), 1, 'profile parts incomplete');
    const tid = header('thread'); assert.match(tid, /^[1-9]\d*$/); assert.ok(!threads.has(tid)); threads.add(tid);
    assert.deepEqual(argvTokens(header('cmd')), measured, 'profile/log argv mismatch');
    const events = header('events').trim().split(/\s+/), irAt = events.indexOf('Ir'); assert.ok(irAt >= 0);
    const totals = [...text.matchAll(/^totals: (.+)$/gm)]; assert.equal(totals.length, 1);
    const values = totals[0][1].trim().split(/\s+/).map(naturalBig), irTotal = values[irAt] ?? 0n;
    totalIr += irTotal;
    const functions = profileFunctions(text);
    assert.equal([...functions.values()].reduce((n, f) => n + f.selfIr, 0n), irTotal, 'profile self Ir mismatch');
    const target = options.cell.expectedSymbols.find(symbol => functions.has(symbol));
    if (!target) continue;
    const fn = functions.get(target), inclusive = fn.selfIr + [...fn.edges.values()].reduce((a, b) => a + b, 0n);
    if (!inclusive) continue;
    // The parent's own guard is destroyed at process exit (exit -> __call_tls_dtors), outside
    // every measured window; only that exact teardown path may hit the target on thread 1.
    if (options.cell.layer === 'L2' && tid === '1' && options.cell.family.includes('release')
      && functions.get('__call_tls_dtors')?.edgeCalls.get(target) === 1n
      && functions.get('exit')?.edges.has('__run_exit_handlers')) continue;
    assert.ok(inclusive > 0n, 'target inclusive Ir absent');
    const release = options.cell.family.includes('release');
    const claim = ['cold', 'start', 'recycled', 'recycled_alloc', 'wave'].includes(options.cell.family);
    const callerMatches = name => options.cell.layer === 'L1'
      ? /r20_prf04_heap_reuse_iai|heap_reuse/.test(name)
      : release ? /__call_tls_dtors|drop_slow|drop_in_place|Drop/.test(name)
        : isL2AllocCaller(name);
    const dynamicPaths = new Set();
    const visit = (name, seen = new Set()) => {
      if (seen.has(name)) return;
      seen.add(name);
      for (const [next, cost] of functions.get(name)?.edges ?? []) {
        if (cost <= 0n) continue;
        dynamicPaths.add(next);
        visit(next, seen);
      }
    };
    visit(target);
    const hasDynamicPath = [...dynamicPaths].some(name => claim
      ? /alloc|claim|HeapRegistry|HeapCore|TLSHeap|registry/i.test(name)
      : /HeapLease|AbandonGuard|drop|release|tls_heap/i.test(name));
    const bindsTls = [...dynamicPaths].some(name =>
      /tls_heap::bind_slow_tagged(?:_with_config)?/.test(name));
    const claimsRegistry = [...dynamicPaths].some(name =>
      /(?:registry::)?(?:claim|claim_impl|HeapRegistry)/i.test(name));
    if (options.cell.layer === 'L2' && claim) {
      assert.ok(bindsTls && claimsRegistry,
        'L2 first-global-allocation dynamic path must demonstrate TLS bind and registry claim');
    }
    if (options.cell.layer === 'L2' && claim && !hasDynamicPath) {
      throw new Error('L2 first-global-allocation dynamic path lacks TLS bind and registry claim');
    }
    assert.ok(hasDynamicPath, claim
      ? 'allocation/claim dynamic path absent' : 'lease release/drop dynamic path absent');
    let incoming = 0n, callers = 0;
    for (const [caller, record] of functions) {
      const count = record.edgeCalls.get(target) ?? 0n, cost = record.edges.get(target) ?? 0n;
      if (count <= 0n || cost <= 0n) continue;
      if (!callerMatches(caller)) throw new Error('target incoming caller context mismatch: ' + caller);
      if (options.cell.layer === 'L2') assert.notEqual(tid, '1', 'L2 target must be child thread');
      const paths = [...record.edges].filter(([, edgeCost]) => edgeCost > 0n).map(([name]) => name);
      assert.ok(paths.includes(target), 'incoming caller edge must target the measured function');
      incoming += count; callers++; callerContext = caller;
    }
    // Valgrind reuses a joined thread's tid, so sequential children (recycled) share one
    // per-thread profile; the per-cell total below still pins executions to n.
    assert.ok(incoming >= 1n, 'positive target profile must execute the target');
    if (options.cell.layer === 'L1') assert.equal(incoming, 1n);
    if (options.cell.layer === 'L2' && claim && !isL2AllocCaller(callerContext)) {
      throw new Error('L2 first-global-allocation caller is not the strict FirstAlloc forwarding wrapper');
    }
    assert.equal(callers, 1, 'positive target profile must have one incoming caller');
    assert.equal(inclusive, irTotal, 'positive target profile inclusive Ir must equal profile total Ir');
    executions += incoming; inclusiveIr += inclusive;
    entries.push({ thread: tid, target, inclusiveIr: safeNumber(inclusive),
      executions: safeNumber(incoming), callerContext });
  }
  const ids = [...threads].map(Number).sort((a, b) => a - b);
  assert.ok(ids.includes(1));
  assert.deepEqual(ids, Array.from({ length: ids.length }, (_, i) => i + 1), 'thread profile gaps');
  assert.equal(executions, BigInt(options.cell.layer === 'L1' ? 1 : options.cell.n));
  if (options.cell.layer === 'L1') assert.ok(entries.some(item => item.thread === '1'));
  else assert.ok(entries.every(item => item.thread !== '1'));
  return { pid: pids[0], totalIr: safeNumber(totalIr), inclusiveIr: safeNumber(inclusiveIr),
    executions: safeNumber(executions), callerContext, entries, ...args };
}
export function machineBoundary(nm, assembly, symbols) {
  const blocks = [...assembly.matchAll(/^([0-9a-f]+) <(.+)>:\n([\s\S]*?)(?=^[0-9a-f]+ <|$(?![\s\S]))/gm)];
  const evidence = {};
  for (const symbol of symbols) {
    const defs = nm.split(/\r?\n/).filter(line => line.endsWith(' ' + symbol));
    assert.equal(defs.length, 1, 'exact nm definition missing/ambiguous ' + symbol);
    const exact = blocks.filter(match => match[2] === symbol);
    assert.equal(exact.length, 1, 'exact disassembly missing/ambiguous ' + symbol);
    const callers = blocks.filter(match => match[3].split('\n').some(line =>
      // A tail call compiles to jmp; a TLS destructor is registered by address (lea).
      /(?:callq?|jmpq?|lea[lq]?)\s+[^\n]*<[^>]+>/.test(line) && line.includes('<' + symbol + '>')));
    assert.ok(callers.length, 'direct call to target absent ' + symbol);
    evidence[symbol] = { definition: defs[0], body: exact[0][3],
      callers: callers.map(match => ({ name: match[2], body: match[3] })) };
  }
  return evidence;
}
function boundaryEvidence(binary, tag, symbols) {
  const exe = executablePath(binary.exe);
  if (ctx.adapter) return ctx.adapter.boundaryEvidence({ ...binary, exe }, tag, symbols);
  const nmRun = spawnSync('nm', ['-C', '--defined-only', exe],
    { encoding: 'utf8', maxBuffer: 1 << 25 });
  const disRun = spawnSync('objdump', ['-Cd', '--no-show-raw-insn', exe],
    { encoding: 'utf8', maxBuffer: 1 << 26 });
  const nmText = publicText(nmRun.stdout ?? ''), disText = publicText(disRun.stdout ?? '');
  const nmReceipt = putText('boundary/' + tag + '-nm.log', nmText);
  const disassemblyReceipt = putText('boundary/' + tag + '-disassembly.log', disText);
  assert.equal(nmRun.status, 0, 'nm failed: ' + nmRun.stderr);
  assert.equal(disRun.status, 0, 'objdump failed: ' + disRun.stderr);
  const verification = machineBoundary(nmText, disText, symbols);
  return { nmReceipt, disassemblyReceipt, verification };
}
function parseIr(output, names) {
  const ir = {}; let current = null;
  for (const raw of output.split(/\r?\n/)) {
    const line = raw.replace(/\x1b\[[0-9;]*m/g, '');
    const h = /^(?:[A-Za-z_]\w*::)+([A-Za-z_]\w*)(?:\s.*)?$/.exec(line);
    if (h) current = h[1];
    else {
      const m = /^\s*Instructions:\s*([\d,]+)/.exec(line);
      if (m && current && names.has(current) && ir[current] === undefined) ir[current] = Number(m[1].replace(/,/g, ''));
    }
  }
  return ir;
}
function walkFiles(root) {
  const files = [];
  const walk = dir => { for (const name of fs.readdirSync(dir)) {
    const path = join(dir, name); if (fs.statSync(path).isDirectory()) walk(path); else files.push(path);
  } };
  walk(root); return files;
}
function measureCell(run, arm, role, cells) {
  const binary = ctx.builds.arms[arm][role], tag = run + '-' + role;
  const exe = executablePath(binary.exe);
  const before = shaFile(exe);
  assert.equal(before, binary.binarySha256, 'executable drift before launch ' + tag);
  const startedAt = now(), homeName = 'target/r20p4-observe-' + tag;
  const home = join(snapshotRoot(), arm, homeName);
  assert.ok(!fs.existsSync(home), 'observation home already exists');
  const env = { ...process.env, IAI_CALLGRIND_LOG: 'debug', IAI_CALLGRIND_COLOR: 'never' };
  const proc = ctx.adapter ? ctx.adapter.measure({ ...binary, exe }, arm, role, cells, home, env)
    : spawnSync(exe, ['--home', home], { cwd: join(snapshotRoot(), arm), env,
      encoding: 'utf8', maxBuffer: 1 << 28, timeout: 3_600_000 });
  const debug = publicText((proc.stdout ?? '') + (proc.stderr ?? ''));
  const debugReceipt = putText('raw/' + tag + '-debug.log', debug);
  assert.equal(proc.status, 0, 'IAI failed ' + tag + ': ' + proc.stderr);
  assert.equal(shaFile(exe), before, 'executable drift after launch ' + tag);
  const files = walkFiles(home), parsed = parseIr(proc.stdout ?? '', new Set(cells.map(c => c.name)));
  const rows = [];
  for (const cell of cells) {
    const segment = path => path.replaceAll('\\', '/').split('/').includes(cell.name);
    const owned = files.filter(segment);
    const profiles = owned.filter(path => /\.out(?:[.#-]+\d+)*$/.test(path));
    const logs = owned.filter(path => /\.log(?:[.#-]+\d+)*$/.test(path));
    assert.ok(profiles.length && logs.length, 'profile or log absent: ' + run + '/' + cell.name);
    const profileTexts = profiles.map(path => publicText(fs.readFileSync(path, 'utf8')));
    const profileReceipts = profileTexts.map((text, index) => putText(
      'profiles/' + tag + '-' + cell.name + '-' + index + '.out', text));
    const inventory = putJson('profiles/' + tag + '-' + cell.name + '-inventory.json',
      profileReceipts.map(receipt => ({ receipt: receipt.path, sha256: receipt.sha256 })));
    const callgrindLog = publicText(logs.map(path => fs.readFileSync(path, 'utf8')).join('\n'));
    const logReceipt = putText('logs/' + tag + '-' + cell.name + '.log', callgrindLog);
    const groupFile = join(REPO, 'benches', cell.layer === 'L1' ? IAI_TARGET + '.rs' : CONTROLS_TARGET + '.rs');
    const members = groupMembers(groupFile, cell.group);
    assert.equal(members[cell.index], cell.name, 'dispatcher index drift');
    const symbolEvidence = boundaryEvidence(binary, tag + '-' + cell.name, [...new Set(cell.expectedSymbols)]);
    const parsed = parseProfiles(profileTexts, callgrindLog, { cell, debug,
      homeName: '/' + homeName.split('/').slice(-1)[0] + '/' + cell.name + '/',
      expectedExecutable: dispatchExe(exe) });
    const boundary = parsed;
    const source = sourceOf(ctx.manifest.arches[arm === 'C' ? 'A' : arm]);
    rows.push({ name: cell.name, arm: run, Ir: boundary.inclusiveIr, binarySha256: before, source,
      environmentSha256: ctx.builds.environmentSha256, features: FEATURES.split(' '), startedAt,
      receipt: debugReceipt.path, runId: run + '-' + cell.name,
      oracle: oracleProjection(run, cell, before, source),
      boundary: { symbol: cell.expectedSymbols, inclusiveIr: boundary.inclusiveIr,
        executions: boundary.executions, expectedExecutions: cell.layer === 'L1' ? 1 : cell.n,
        entryPoint: 'Custom', collectAtStart: false, defaultToggle: false, separateThreads: true,
        toggleCollect: boundary.toggleCollect, dispatcherGroup: cell.group, dispatcherIndex: cell.index,
        callerContext: boundary.callerContext, settings: boundary.settings,
        symbolReceipt: symbolEvidence.nmReceipt.path,
        disassemblyReceipt: symbolEvidence.disassemblyReceipt.path, profileReceipt: inventory.path,
        logReceipt: logReceipt.path, binarySha256: before } });
  }
  fs.rmSync(home, { recursive: true, force: true });
  return rows;
}
function oracleProjection(arm, cell, primarySha, source) {
  const sourceArm = arm.startsWith('B') && !ctx.manifest.rehearsal ? 'B' : 'A';
  const role = cell.layer === 'L1' ? 'oracle' : 'control';
  const rec = ctx.oracle.records[sourceArm][cell.name], oraclePin = ctx.builds.arms[sourceArm][role];
  assert.equal(rec.binarySha256, oraclePin.binarySha256, 'oracle role binary mismatch');
  return { observer: rec.row.observer,
    heap_reuse_spill_bits_delta: rec.row.heap_reuse_spill_bits_delta,
    heap_reuse_take_hits: rec.row.heap_reuse_take_hits,
    hint: rec.row.hint, bump: rec.row.bump, capScan: rec.row.capScan,
    recovery: rec.row.recovery, highWaterDelta: rec.row.highWaterDelta,
    actualTargetCalls: rec.row.actualTargetCalls, configConflictsDelta: rec.row.configConflictsDelta,
    linkage: { binarySha256: rec.binarySha256, primaryBinarySha256: primarySha,
      source: sourceOf(ctx.manifest.arches[sourceArm]), features: FEATURES.split(' '),
      cfg: rec.row.cfg, environmentSha256: ctx.builds.environmentSha256,
      receipt: rec.receipt.path, featureExactReceipt: 'oracle.json', startedAt: rec.startedAt } };
}
function cmdMeasure() {
  readStage('manifest.json', m => { ctx.manifest = m; ctx.rehearsal = m.rehearsal === true; });
  readStage('builds.json', b => { ctx.builds = b; });
  readStage('oracle.json', o => { ctx.oracle = o; });
  verifyIdentities('measure');
  assert.equal(ctx.builds.manifestSha256, shaFile(local('manifest.json')));
  assert.equal(ctx.oracle.manifestSha256, shaFile(local('manifest.json')));
  assert.equal(ctx.oracle.buildsSha256, shaFile(local('builds.json')));
  groupInventory();
  const labels = ctx.rehearsal ? ['A1'] : RUNS;
  const iai = [];
  const binaries = arm => ({ iai: ctx.builds.arms[arm].iai.binarySha256,
    controls: ctx.builds.arms[arm].controls.binarySha256,
    oracle: ctx.builds.arms[arm === 'C' ? 'A' : arm].oracle.binarySha256,
    control: ctx.builds.arms[arm].control.binarySha256 });
  const source = arm => sourceOf(ctx.manifest.arches[arm === 'C' ? 'A' : arm]);
  for (const run of labels) {
    const arm = RUN_ARM[run];
    const selected = [...measureCell(run, arm, 'iai', CELLS.filter(c => c.layer === 'L1')),
      ...measureCell(run, arm, 'controls', CELLS.filter(c => c.layer === 'L2'))];
    iai.push(...selected);
  }
  const hashes = frozenHashes();
  const environmentSha256 = ctx.builds.environmentSha256;
  const identities = {
    A: { label: 'A', measurementCommit: source('A').baseHead, source: source('A'), environmentSha256,
      binaries: binaries('A'), buildStartedAt: ctx.builds.buildStartedAt, buildReceipt: 'builds.json',
      features: FEATURES.split(' '), primaryCfg: [], oracleCfg: ['prf04_heap_reuse_activation'],
      toolchain: { rustc: 'environment.log', target: 'environment.log', profile: 'bench', flags: 'role cfg only' } },
    B: { label: 'B', measurementCommit: source('B').baseHead, source: source('B'), environmentSha256,
      binaries: binaries('B'), buildStartedAt: ctx.builds.buildStartedAt, buildReceipt: 'builds.json',
      features: FEATURES.split(' '), primaryCfg: [], oracleCfg: ctx.manifest.rehearsal
        ? ['prf04_heap_reuse_activation'] : ['prf04_heap_reuse_activation', 'prf04_heap_reuse_oracle'],
      toolchain: { rustc: 'environment.log', target: 'environment.log', profile: 'bench', flags: 'role cfg only' } },
    C0: {
      source: source('A'),
      binarySha256: binaries('C').iai,
      controlsBinarySha256: binaries('C').controls,
      buildStartedAt: ctx.builds.buildStartedAt,
      buildReceipt: 'builds.json',
    },
  };
  const input = {
    version: 1,
    hashes,
    environmentSha256,
    ...(ctx.rehearsal ? { rehearsal: true } : {}),
    releaseLayer: RELEASE_LAYER,
    ...(RELEASE_LAYER === 'L1' ? { fallbackReceipt: ctx.manifest.fallbackReceipt } : {}),
    inventory: CELLS.map(cell => cell.name),
    inventoryReceipt: 'manifest.json',
    identityReviewReceipt: 'per-stage source, overlay, and snapshot identity verification',
    environmentReceipt: 'environment.log',
    oracleReceipt: 'oracle.json',
    entry: 'L1 HeapRegistry claim lease; L2 installed SeferAlloc GlobalAlloc allocation and release',
    nativeDeferred: { deferred: true,
      method: 'native paired A/B fixed-batch latency and RSS deferred; deterministic Callgrind only' },
    identities, ancestry: { result: 'A ancestor B',
      chain: [source('A').baseHead, source('B').baseHead], receipt: 'manifest.json' }, iai };
  const receiptHashes = {};
  const walkReceipts = value => {
    if (typeof value === 'string' &&
      /^(?:manifest\.json|builds\.json|oracle\.json|environment\.log|(?:raw|logs|boundary|profiles|oracle)\/)/
        .test(value))
      receiptHashes[value] ??= shaFile(local(value));
    else if (Array.isArray(value)) value.forEach(walkReceipts);
    else if (value && typeof value === 'object') Object.values(value).forEach(walkReceipts);
  };
  walkReceipts(input);
  const receiptRoot = ctx.outDir;
  const hashTree = dir => { for (const name of fs.readdirSync(dir)) {
    const path = join(dir, name), rel = path.slice(receiptRoot.length + 1).replaceAll('\\', '/');
    if (fs.statSync(path).isDirectory()) hashTree(path);
    else if (!['input.json', 'summary.csv', 'verdict.txt', 'verifier.txt'].includes(rel))
      receiptHashes[rel] ??= shaFile(path);
  } };
  hashTree(receiptRoot); input.receiptHashes = receiptHashes;
  putText('summary.csv', ['arm,name,Ir,binarySha256',
    ...iai.map(row => [row.arm, row.name, row.Ir, row.binarySha256].join(','))].join('\n') + '\n');
  putJson('input.json', input);
  const judge = ctx.adapter ? ctx.adapter.judge(local('input.json'))
    : spawnSync(process.execPath, [join(REPO, JUDGE), '--input', local('input.json')],
      { cwd: REPO, encoding: 'utf8', maxBuffer: 1 << 24 });
  putText('verdict.txt', ['exit=' + judge.status, judge.stdout ?? '', judge.stderr ?? ''].join('\n'));
  if (ctx.rehearsal) {
    const out = String(judge.stdout ?? '') + String(judge.stderr ?? '');
    if (judge.status !== 0 || !out.includes('REHEARSAL:'))
      console.log('REHEARSAL: unsuitable L2 harness; inspect retained receipts');
    else console.log('REHEARSAL: A-only harness validation; no gate verdict');
    return;
  }
  const verifier = ctx.adapter ? ctx.adapter.verify(ctx.outDir)
    : spawnSync(process.execPath, [join(REPO, 'scripts/r20_prf04_heap_reuse_verify_artifacts.mjs'),
      ctx.outDir], { cwd: REPO, encoding: 'utf8', maxBuffer: 1 << 24 });
  putText('verifier.txt', ['exit=' + verifier.status, verifier.stdout ?? '', verifier.stderr ?? ''].join('\n'));
  process.stdout.write(judge.stdout ?? '');
  if (judge.stderr) process.stderr.write(judge.stderr);
  if (verifier.status !== 0 || judge.status !== 0) {
    putText('verdict.txt', 'INVALID: runner or artifact verifier rejected results\n');
    process.exitCode = 1;
  }
}

// --- main --------------------------------------------------------------------

function stagePaths(value = null) {
  const rel = value ?? (ctx.rehearsal ? REHEARSAL_OUT : DEFAULT_OUT);
  assert.match(rel, /^(?:docs\/perf\/R20_[A-Z0-9_]+|target\/r20[a-z0-9_-]*)$/,
    'invalid --out-dir; expected a preregistered docs path or target/r20 path');
  const resolved = resolve(REPO, rel);
  assert.ok(resolved.startsWith(REPO + sep), '--out-dir must stay inside this worktree');
  return resolved;
}
function rehearsal() {
  ctx.outDir ??= stagePaths();
  cmdPrepare();
  cmdBuild();
  cmdOracle();
  cmdMeasure();
}
function cmdCleanup() {
  for (const root of [SNAPSHOT_ROOT, join(REPO, 'target', 'r20p4-builds')]) {
    assert.ok(root.startsWith(join(REPO, 'target') + sep), 'cleanup escaped runner target');
    if (fs.existsSync(root)) fs.rmSync(root, { recursive: true, force: true });
  }
}
function parseArgs(args) {
  let mode = null, outDir = null;
  for (let i = 0; i < args.length; i++) {
    const arg = args[i];
    if (arg === '--out-dir') {
      assert.ok(outDir === null && args[i + 1] && !args[i + 1].startsWith('--'), '--out-dir requires one path');
      outDir = args[++i];
    } else {
      assert.ok(
        ['--prepare', '--build', '--oracle', '--measure', '--rehearsal', '--cleanup', '--self-test'].includes(arg),
        'unsupported argument: ' + arg,
      );
      assert.equal(mode, null, 'exactly one stage is allowed');
      mode = arg;
    }
  }
  assert.ok(mode, 'one stage is required');
  return { mode, outDir };
}

function fakeCommand(exe, cell) {
  return dispatchExe(exe) + ' --iai-run ' + cell.group + ' ' + cell.index + ' 0';
}
function main(args = process.argv.slice(2)) {
  const parsed = parseArgs(args);
  if (parsed.mode === '--self-test') return runRunnerSelfTest({
    main, ctx, local, putText, spawnSync, sha, shaFile, REPO, FEATURES, CELLS,
    fakeBenchProgram, fakeOracleProgram, basename, resolve, join, fs, assert,
    process, CANONICAL_FROZEN_FILES, JUDGE,
  });
  ctx.rehearsal = parsed.mode === '--rehearsal';
  ctx.outDir = stagePaths(parsed.outDir);
  if (parsed.mode === '--prepare') return cmdPrepare();
  if (parsed.mode === '--build') return cmdBuild();
  if (parsed.mode === '--oracle') return cmdOracle();
  if (parsed.mode === '--measure') return cmdMeasure();
  if (parsed.mode === '--cleanup') return cmdCleanup();
  if (parsed.mode === '--rehearsal') return rehearsal();
  throw new Error('unhandled stage');
}
export { main, ctx, cmdPrepare, cmdBuild, cmdOracle, cmdMeasure, parseArgs, loadFallbackReceipt };
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();

