// R20 PRF-01 v2 gate RUNNER. Executes the preregistered pipeline; the judge
// (r20_prf01_sidecar_summary_gate.mjs) and the prereg are frozen inputs and are
// never modified here. The frozen file list lives in the judge (--hashes /
// --check-hashes); this runner is one of those files.
//
// Modeled on scripts/r20_prf04_claim_path_run.mjs (stage pipeline, identity
// manifest, sanitised logs) and inheriting the producer-boundary machinery
// (EntryPoint::Custom profiles, nm/disassembly receipts, Amendment B coverage)
// from scripts/r19_prf01_sidecar_summary_run.mjs in a reduced form.
//
// Commands:
//   --prepare   worktrees <primary>/worktrees/r20p1/{A,B,C} (equal length,
//               one-letter labels); B = A + cherry-pick edf90675 on a temp
//               branch; identity manifest BEFORE any measurement.
//   --build     per-arm builds (native on Linux, wsl.exe on Windows):
//               perf_gate_iai (iai), r20_prf01_controls_iai (controls),
//               r19_prf01_oracle example under cfg r19_sidecar_summary_oracle,
//               perf_gate_iai control under cfg r19_prf01_producer_control.
//               C is built with -C debuginfo=line-tables-only (registered).
//   --oracle    r19_prf01_oracle on A and B for every needed mode + probes.
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
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const RUNNER_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(RUNNER_DIR, '..');
const REPO_ROOT = resolve(spawnSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'],
  { cwd: REPO, encoding: 'utf8' }).stdout.trim(), '..');
const JUDGE = 'scripts/r20_prf01_sidecar_summary_gate.mjs';
const PREREG = 'docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_PREREG.md';
const IAI_TARGET = 'perf_gate_iai';
const CONTROLS_TARGET = 'r20_prf01_controls_iai';
const ORACLE_TARGET = 'r19_prf01_oracle';
const FEATURES = 'production bench-internals internals';
const ORACLE_CFG = 'r19_sidecar_summary_oracle';
const CONTROL_CFG = 'r19_prf01_producer_control';
const DISTINGUISH_RUSTFLAGS = '-C debuginfo=line-tables-only';
const PICK = 'edf90675';
const PRODUCER = 'r19_prf01_joined_producer_same_word';
const PRODUCER_SYMBOL = '<perf_gate_iai::prf01_support::R19Prf01>::producer_free_window';
const BENEFIT = ['multiseg_cold_256k', 'seg_cycle_decommit_256k', 'growth16_n', 'growth16_2n', 'growth16_4n'];
const ORACLE_MODES = [...BENEFIT, 'publication', 'producer', 'probe-lease', 'probe-global'];
const HOT_FAMILIES = ['small_churn_16b', 'churn_256b', 'churn_write_256b', 'aligned_churn_640b_a128'];
const HOT_CELLS = HOT_FAMILIES.flatMap(f => ['n', '2n', '4n'].map(s => `r20_prf01_${f}_${s}`));
const CONSTRUCTION = 'r20_prf01_start_one_alloc_free_16b';
const MI = ['mimalloc_small_churn_16b','mimalloc_small_churn_16b_2n','mimalloc_churn_256b','mimalloc_cold_alloc_free_256x16b','mimalloc_cold_alloc_free_256x16b_2n','mimalloc_cold_alloc_free_256x16b_4n','mimalloc_cold_alloc_only_256x16b','mimalloc_cold_alloc_only_256x16b_2n','mimalloc_cold_alloc_only_256x16b_4n','mimalloc_cold_alloc_free_256x64b','mimalloc_recycle_alloc_free_256x16b','mimalloc_recycle_alloc_free_256x64b','mimalloc_bootstrap_proxy'];
const IAI_CELLS = [...BENEFIT, ...MI, 'r19_prf01_publication', PRODUCER];
const CONTROLS_CELLS = [CONSTRUCTION, ...HOT_CELLS];
const HOT_DECL = { [CONSTRUCTION]: [1, 16, 8],
  ...Object.fromEntries(HOT_FAMILIES.flatMap(f => [['n', 64], ['2n', 128], ['4n', 256]].map(([s, ops]) =>
    [`r20_prf01_${f}_${s}`, [ops, f === 'small_churn_16b' ? 16 : f === 'aligned_churn_640b_a128' ? 640 : 256, f === 'aligned_churn_640b_a128' ? 128 : 8]]))) };
const RUNS = ['A1', 'A2', 'B1', 'B2', 'C0'];
const RUN_ARM = { A1: 'A', A2: 'A', B1: 'B', B2: 'B', C0: 'C' };
// CLAUDE.md artifact tiers: <200 KiB verbatim, 200 KiB-2 MiB truncated or gzipped when committed, >2 MiB never in git.
const RAW_LOG_LIMIT = 2 * 1024 * 1024;
const PRODUCER_SETTINGS = ['--tool=callgrind', '--error-exitcode=0', '--trace-children=yes', '--fair-sched=try',
  '--I1=32768,8,64', '--D1=32768,8,64', '--LL=8388608,16,64', '--cache-sim=yes', '--compress-strings=no',
  '--compress-pos=no', '--dump-line=yes', '--dump-instr=no', '--combine-dumps=no', '--separate-threads=yes'];

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
const publicText = t => String(t)
  .replaceAll(toHost(REPO_ROOT), '<repo>').replaceAll(REPO_ROOT.replace(/\\/g, '/'), '<repo>')
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
  const exe = IS_WIN ? 'wsl.exe' : 'bash';
  const args = IS_WIN ? ['bash', '-lc', script] : ['-lc', script];
  const r = spawnSync(exe, args, { encoding: 'utf8', maxBuffer: 1 << 28, timeout,
    env: { ...process.env, MSYS_NO_PATHCONV: '1' } });
  return { code: r.status ?? -1, out: r.stdout ?? '', err: r.stderr ?? '', timedOut: r.error?.code === 'ETIMEDOUT' };
}
function wslOk(script, what, opts) {
  const r = wsl(script, opts);
  assert.equal(r.code, 0, `${what} failed (exit ${r.code})\n--- stderr tail ---\n${r.err.split('\n').slice(-40).join('\n')}`);
  return r;
}

const ctx = { outDir: null, rehearsal: false, slop: false, manifest: null, builds: null, oracle: null };
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
  const check = spawnSync(process.execPath, [join(REPO, JUDGE), '--check-hashes'], { encoding: 'utf8' });
  assert.equal(check.status, 0, `frozen hash drift (a frozen file was edited?)\n${check.stdout}${check.stderr}`);
  const list = spawnSync(process.execPath, [join(REPO, JUDGE), '--hashes'], { encoding: 'utf8' }).stdout;
  return list.trim().split('\n').map(l => { const [h, path] = l.split(/\s+/); return { path, sha256: h }; });
}

// Per-stage identity binding (re-freeze 9): rebuild the prepare-stage identities from the live
// worktrees and compare with manifest.json before any stage does its work. The official out-dir
// (docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01) lives in the PRIMARY checkout and is not part of
// the worktrees A/B/C, so none of these checks read it.
function verifyIdentities(stage) {
  const m = ctx.manifest;
  const wt = label => join(WT_DIR, label);
  const expect = {
    A: archIdentity('A', wt('A')),
    B: m.rehearsal ? { ...archIdentity('B', wt('A')), rehearsalProxyFor: 'A' } : archIdentity('B', wt('B')),
    C: { ...archIdentity('A', wt('A')) },
  };
  if (m.rehearsal) expect.oracleB = archIdentity('B', wt('B'));
  for (const [label, e] of Object.entries(expect)) {
    const got = m.arches[label];
    assert.ok(got, `manifest has no arch ${label} (${stage})`);
    for (const k of ['head', 'tree', 'identitySha256'])
      assert.equal(got[k], e[k], `arm ${label} identity drift at the ${stage} stage: manifest ${got[k]} vs live ${e[k]}`);
  }
  assert.equal(m.baseHead, baseHead(), `the runner HEAD moved since prepare (${stage})`);
  console.log(`[${stage}] identity re-validated against manifest.json (A/B/C${m.rehearsal ? ' + oracleB' : ''})`);
}

function baseHead() {
  assert.equal(git(REPO, ['status', '--porcelain']).out, '', 'runner worktree must be clean and committed');
  return git(REPO, ['rev-parse', 'HEAD']).out;
}
function archIdentity(label, wt) {
  const head = git(wt, ['rev-parse', 'HEAD']).out;
  const tree = git(wt, ['write-tree']).out;
  assert.match(head, /^[0-9a-f]{40}$/); assert.match(tree, /^[0-9a-f]{40}$/);
  const patch = git(wt, ['diff', '--binary', 'HEAD', '--']).out;
  const untracked = git(wt, ['ls-files', '--others', '--exclude-standard', '-z']).out;
  assert.equal(patch, '', 'tracked drift: arch must be a clean committed party');
  assert.equal(untracked, '', 'untracked drift: arch must be a clean committed party');
  const patchSha256 = sha(patch), untrackedSha256 = sha(JSON.stringify(untracked.split('\0')));
  return { label, wt: `<repo>/worktrees/r20p1/${label}`, head, tree, patchSha256, untrackedSha256,
    identitySha256: sha(JSON.stringify([label, head, patchSha256, untrackedSha256])),
    cargoLockSha256: shaFile(join(wt, 'Cargo.lock')), capturedAt: now() };
}
const sourceOf = a => (({ head, patchSha256, untrackedSha256, identitySha256, capturedAt }) =>
  ({ baseHead: head, patchSha256, untrackedSha256, identitySha256, capturedAt }))(a);

const WT_DIR = join(REPO_ROOT, 'worktrees', 'r20p1');
function ensureWorktree(label, { newBranch = false } = {}) {
  const wt = join(WT_DIR, label);
  const base = baseHead();
  if (fs.existsSync(wt)) {
    assert.equal(git(wt, ['rev-parse', 'HEAD']).out, base, `worktree ${label} is not at ${base}; remove it manually and rerun`);
    return wt;
  }
  fs.mkdirSync(WT_DIR, { recursive: true });
  const args = ['worktree', 'add'];
  if (newBranch) args.push('-b', 'r20p1-b-tmp');
  else args.push('--detach');
  git(REPO, [...args, wt, base]);
  return wt;
}
function cmdPrepare() {
  ctx.outDir = stagePaths();
  assert.ok(!fs.existsSync(local('manifest.json')), `manifest exists, refusing to overwrite: ${local('manifest.json')}`);
  fs.mkdirSync(ctx.outDir, { recursive: true });
  const base = baseHead();
  const pick = git(REPO, ['rev-parse', PICK]).out;
  const wtA = ensureWorktree('A');
  const wtB = join(WT_DIR, 'B');
  if (!fs.existsSync(wtB)) {
    ensureWorktree('B', { newBranch: true });
    const cp = git(wtB, ['-c', 'user.name=r20p1-runner', '-c', 'user.email=r20p1-runner@localhost',
      'cherry-pick', pick], { allowFailure: true });
    if (cp.code !== 0) {
      console.error(`cherry-pick ${PICK} onto ${base} CONFLICTED/failed; worktree ${wtB} left as-is. Record and stop.`);
      console.error(cp.out, cp.err);
      process.exit(1);
    }
  }
  assert.equal(git(wtB, ['rev-parse', 'HEAD^']).out, base, 'B must be exactly A + one cherry-picked commit');
  assert.equal(git(wtB, ['diff', '--name-only', base, 'HEAD', '--', JUDGE, PREREG,
    'scripts/r20_prf01_sidecar_summary_run.mjs']).out, '', 'the pick must not touch the judge, the prereg or the runner');
  assert.equal(git(wtB, ['status', '--porcelain']).out, '', 'cherry-pick left the B worktree dirty');
  ensureWorktree('C');
  const archA = archIdentity('A', wtA);
  const arches = {
    A: archA,
    B: ctx.rehearsal ? { ...archIdentity('B', wtA), rehearsalProxyFor: 'A' } : archIdentity('B', wtB),
    C: { ...archA },
  };
  if (ctx.rehearsal) arches.oracleB = archIdentity('B', wtB);
  const manifest = { version: 1, rehearsal: ctx.rehearsal, capturedAt: now(), baseHead: base,
    pickedCommit: git(wtB, ['rev-parse', 'HEAD']).out, pickRequested: pick,
    frozen: frozenHashes(), wtDir: '<repo>/worktrees/r20p1',
    pathLengths: ['A', 'B', 'C'].map(l => join(WT_DIR, l).length), arches };
  assert.equal(new Set(manifest.pathLengths).size, 1, 'A/B/C full path lengths must be equal');
  putJson('manifest.json', manifest);
  console.log(`[prepare] base=${base.slice(0, 12)} rehearsal=${ctx.rehearsal}`);
}

function envCapture() {
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"', 'rustc -Vv', 'cargo -V',
    'valgrind --version', 'uname -srm', `echo iai-features: ${FEATURES}`,
    'iai-callgrind-runner --version 2>&1 || true'].join('; ');
  const r = wslOk(script, 'environment capture', { timeout: 120_000 });
  return { environmentSha256: sha(r.out), text: r.out };
}
function executableOf(jsonOut, target) {
  const hits = [];
  for (const line of jsonOut.split('\n').filter(Boolean)) {
    try {
      const m = JSON.parse(line);
      if (m.reason === 'compiler-artifact' && m.target?.name === target && m.executable) hits.push(m.executable);
    } catch { /* cargo human lines interleaved; ignore */ }
  }
  assert.ok(hits.length >= 1, `no executable reported for ${target}`);
  return hits[hits.length - 1];
}
function cargoBuild({ wt, label, target, features, rustflags, kind }) {
  const td = `/tmp/r20p1/${label}-${target}-${rustflags ? 'c' : 'p'}`;
  const rf = rustflags ? `RUSTFLAGS='${rustflags}' ` : '';
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"',
    'RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER=',
    `cd ${toHost(wt)}`,
    `${rf}CARGO_TARGET_DIR=${td} cargo ${kind} --locked --message-format=json-render-diagnostics ${kind === 'bench' ? `--bench ${target} --no-run` : `--example ${target}`} --features '${features}'`,
  ].join(' && ');
  const r = wslOk(script, `build ${target} @${label}`, { timeout: 900_000 });
  const exe = executableOf(r.out, target);
  const sum = wslOk(`sha256sum ${exe}`, 'sha256sum binary').out.split(/\s+/)[0];
  assert.match(sum, /^[0-9a-f]{64}$/);
  return { target, kind, exe, binarySha256: sum, features, rustflags: rustflags ?? '', cfg: rustflags ?? '', targetDir: td };
}
function cmdBuild() {
  readStage('manifest.json', m => { ctx.manifest = m; });
  verifyIdentities('build');
  const wtOf = label => join(WT_DIR, label);
  const env = envCapture();
  putText('environment.log', env.text);
  const buildStartedAt = now();
  const arms = {};
  for (const label of ['A', 'B']) {
    const wt = label === 'B' && ctx.manifest.rehearsal ? wtOf('A') : wtOf(label);
    arms[label] = {
      iai: cargoBuild({ wt, label, target: IAI_TARGET, features: FEATURES, kind: 'bench',
        rustflags: label === 'B' && ctx.manifest.rehearsal ? DISTINGUISH_RUSTFLAGS : undefined }),
      controls: cargoBuild({ wt, label, target: CONTROLS_TARGET, features: FEATURES, kind: 'bench',
        rustflags: label === 'B' && ctx.manifest.rehearsal ? DISTINGUISH_RUSTFLAGS : undefined }),
      oracle: cargoBuild({ wt: wtOf(label), label, target: ORACLE_TARGET, features: FEATURES, kind: 'build',
        rustflags: `--cfg ${ORACLE_CFG}` }),
      control: cargoBuild({ wt, label, target: IAI_TARGET, features: FEATURES, kind: 'bench',
        rustflags: `${label === 'B' && ctx.manifest.rehearsal ? DISTINGUISH_RUSTFLAGS + ' ' : ''}--cfg ${CONTROL_CFG}` }),
    };
    assert.notEqual(arms[label].iai.binarySha256, arms[label].control.binarySha256, 'control binary must differ');
  }
  arms.C = {
    iai: cargoBuild({ wt: wtOf('C'), label: 'C', target: IAI_TARGET, features: FEATURES, kind: 'bench', rustflags: DISTINGUISH_RUSTFLAGS }),
    controls: cargoBuild({ wt: wtOf('C'), label: 'C', target: CONTROLS_TARGET, features: FEATURES, kind: 'bench', rustflags: DISTINGUISH_RUSTFLAGS }),
    control: cargoBuild({ wt: wtOf('C'), label: 'C', target: IAI_TARGET, features: FEATURES, kind: 'bench',
      rustflags: `${DISTINGUISH_RUSTFLAGS} --cfg ${CONTROL_CFG}` }),
  };
  assert.notEqual(arms.C.iai.binarySha256, arms.C.control.binarySha256, 'control binary must differ');
  assert.notEqual(arms.A.iai.binarySha256, arms.B.iai.binarySha256, 'A and B iai binaries must differ');
  assert.notEqual(arms.A.iai.binarySha256, arms.C.iai.binarySha256, 'A and C iai binaries must differ');
  putJson('builds.json', { version: 1, buildStartedAt, capturedAt: now(),
    manifestSha256: shaFile(local('manifest.json')), environmentSha256: env.environmentSha256, arms });
  for (const [label, a] of Object.entries(arms))
    console.log(`[build] ${label} iai=${a.iai.binarySha256.slice(0, 12)} controls=${a.controls.binarySha256.slice(0, 12)}${a.oracle ? ` oracle=${a.oracle.binarySha256.slice(0, 12)}` : ''}`);
}

function oracleRecord(arm, mode, label = arm) {
  const exe = ctx.builds.arms[arm].oracle.exe;
  const id = `${label}-${mode}-${Date.now()}`;
  const r = wslOk(`${exe} ${mode} ${id} 2>&1`, `oracle ${arm} ${mode}`, { timeout: 300_000 });
  const line = r.out.split('\n').filter(l => l.startsWith('{')).pop();
  assert.ok(line, `oracle ${arm} ${mode} printed no JSON`);
  const rec = JSON.parse(line);
  assert.equal(rec.mode, mode);
  assert.equal(rec.config.runId, id);
  assert.equal(binarySha(exe), ctx.builds.arms[arm].oracle.binarySha256, `oracle binary drift @${arm}`);
  assert.deepEqual([...rec.features].sort(), FEATURES.split(' ').sort());
  assert.deepEqual(rec.cfg, [ORACLE_CFG]);
  putJson(`oracle/${label}-${mode}.json`, rec);
  return rec;
}
function cmdOracle() {
  readStage('manifest.json', m => { ctx.manifest = m; });
  readStage('builds.json', b => { ctx.builds = b; });
  verifyIdentities('oracle');
  assert.equal(ctx.builds.manifestSha256, shaFile(local('manifest.json')), 'builds.json does not belong to this manifest (stage binding)');
  const m = ctx.manifest;
  const records = {};
  for (const label of RUNS) {
    const arm = label.startsWith('B') ? 'B' : 'A';
    const src = sourceOf(arm === 'B' && m.rehearsal ? m.arches.oracleB : m.arches[arm]);
    records[label] = {};
    for (const mode of ORACLE_MODES) {
      const rec = oracleRecord(arm, mode, label);
      records[label][mode] = { row: rec, binarySha256: ctx.builds.arms[arm].oracle.binarySha256, source: src };
    }
  }
  putJson('oracle.json', { version: 1, capturedAt: now(),
    manifestSha256: shaFile(local('manifest.json')), buildsSha256: shaFile(local('builds.json')), records });
  console.log('[oracle] A+B done');
}

// --- producer boundary machinery (reduced port of the R19 runner) -----------

function naturalBig(token) { assert.match(token, /^(?:\d+|0x[0-9a-fA-F]+)$/, 'malformed unsigned integer'); return BigInt(token); }
function safeNumber(value) { assert.ok(value >= 0n && value <= BigInt(Number.MAX_SAFE_INTEGER), 'Ir exceeds safe judge integer'); return Number(value); }
function positionToken(token, previous) {
  if (token === '*') { assert.notEqual(previous, null, 'relative position without base'); return previous; }
  if (/^[+-]/.test(token)) {
    assert.notEqual(previous, null, 'relative position without base');
    const value = previous + (token[0] === '-' ? -1n : 1n) * naturalBig(token.slice(1));
    assert.ok(value >= 0n, 'negative position'); return value;
  }
  return naturalBig(token);
}
function profileFunctions(s) {
  const events = /^events: (.+)$/m.exec(s)?.[1].trim().split(/\s+/); assert.ok(events);
  const ir = events.indexOf('Ir'); assert.ok(ir >= 0);
  const positions = /^positions: (.+)$/m.exec(s)?.[1].trim().split(/\s+/) ?? ['line'];
  const ids = new Map(), functions = new Map(); let current = null, callee = null, pendingCall = false, previous = Array(positions.length).fill(null);
  const symbol = value => { const m = /^(?:\((\d+)\)\s*)?(.*)$/.exec(value); if (m[2]) return m[2]; assert.ok(ids.has(m[1]), 'unknown compressed function ID'); return ids.get(m[1]); };
  const fn = name => { if (!functions.has(name)) functions.set(name, { selfIr: 0n, edges: new Map(), edgeCalls: new Map() }); return functions.get(name); };
  for (const line of s.split(/\r?\n/)) { const m = /^(?:c?fn)=\((\d+)\)\s+(.+)$/.exec(line); if (m) ids.set(m[1], m[2]); }
  for (const line of s.split(/\r?\n/)) {
    if (line.startsWith('fn=')) { current = symbol(line.slice(3)); fn(current); callee = null; previous = Array(positions.length).fill(null); }
    else if (line.startsWith('cfn=')) { callee = symbol(line.slice(4)); fn(callee); }
    else if (line.startsWith('calls=')) {
      assert.ok(current && callee && !pendingCall, 'invalid calls record');
      const fields = line.slice(6).trim().split(/\s+/);
      positions.forEach((_, i) => positionToken(fields[i + 1], previous[i]));
      pendingCall = true; fn(current).edgeCalls.set(callee, (fn(current).edgeCalls.get(callee) ?? 0n) + naturalBig(fields[0]));
    } else if (/^[*+\-\d]/.test(line)) {
      assert.ok(current, 'cost without function');
      const fields = line.trim().split(/\s+/);
      previous = positions.map((_, i) => positionToken(fields[i], previous[i]));
      const costs = fields.slice(positions.length).map(naturalBig), count = costs[ir] ?? 0n, f = fn(current);
      if (pendingCall) { f.edges.set(callee, (f.edges.get(callee) ?? 0n) + count); pendingCall = false; } else f.selfIr += count;
    } else if (line && !/^(?:#|[A-Za-z][A-Za-z_]*(?:=|:))/.test(line)) throw Error('malformed Callgrind record');
  }
  assert.ok(!pendingCall, 'missing final call cost');
  return functions;
}
function argvTokens(value) {
  const result = []; let token = '', q = null, active = false;
  for (let i = 0; i < value.length; i++) {
    const c = value[i];
    if (q) { if (c === q) q = null; else if (c === '\\' && q === '"' && ['"', '\\'].includes(value[i + 1])) token += value[++i]; else token += c; active = true; }
    else if (c === '"' || c === "'") { q = c; active = true; }
    else if (/\s/.test(c)) { if (active) { result.push(token); token = ''; active = false; } }
    else { token += c; active = true; }
  }
  assert.equal(q, null, 'unclosed argv quote'); if (active) result.push(token);
  return result;
}
// perf_gate group member list of benches/perf_gate_iai.rs, for the dispatcher index binding.
function perfGateIndex(bytes) {
  const s = bytes.toString();
  const group = /library_benchmark_group!\(\s*name\s*=\s*perf_gate;\s*benchmarks\s*=([\s\S]*?)\);/.exec(s);
  assert.ok(group, 'perf_gate group missing');
  const all = group[1].split(',').map(x => x.trim()).filter(Boolean);
  return { all, index: all.indexOf(PRODUCER) };
}
function generatedArguments(debug, log, homeDirName, cellIndex) {
  const commands = [...log.matchAll(/^==\d+==\s+Command:\s*(.+)$/gm)].map(m => m[1]);
  assert.equal(new Set(commands).size, 1, 'ambiguous measured argv'); assert.ok(commands.length);
  const measured = commands[0];
  const records = [...debug.matchAll(/callgrind: Arguments: (.+)$/gm)].map(m => m[1]);
  const candidates = records.filter(x => x.includes('--callgrind-out-file=') && x.includes('/' + homeDirName + '/') && x.includes('--toggle-collect=' + PRODUCER_SYMBOL));
  assert.equal(candidates.length, 1, 'one actual runner argument record required for the producer cell');
  const tool = candidates[0];
  assert.ok(tool.includes('--separate-threads=yes'), 'separate_threads not captured');
  const toolVector = tool.trim().split(/\s+(?=--[A-Za-z])/).map(x => x.replace(/^"(.*)"$/, '$1'));
  for (const setting of PRODUCER_SETTINGS) {
    assert.equal(toolVector.filter(x => x.split('=')[0] === setting.split('=')[0]).length, 1, 'missing/duplicate producer setting ' + setting);
    assert.ok(toolVector.includes(setting), 'wrong producer setting ' + setting);
  }
  assert.equal(toolVector.filter(x => x.startsWith('--toggle-collect=')).length, 1);
  assert.ok(toolVector.includes('--toggle-collect=' + PRODUCER_SYMBOL));
  assert.ok(!tool.includes('*::__iai_callgrind_wrapper_mod::*'), 'default toggle forbidden');
  const dispatch = argvTokens(measured);
  assert.equal(dispatch.indexOf('--iai-run'), 1, 'measured dispatcher missing');
  assert.deepEqual(dispatch.slice(2, 5), ['perf_gate', String(cellIndex), '0'], 'measured dispatcher index mismatch');
  return { toolArguments: tool, measuredArguments: measured, generatedArguments: 'valgrind ' + tool + ' ' + measured, toolVector };
}
function parseProfiles(profiles, log, { cellIndex, debug = '', expectedExecutable } = {}) {
  const pids = [...new Set([...log.matchAll(/^==(\d+)==/gm)].map(m => Number(m[1])))];
  assert.equal(pids.length, 1, 'actual valgrind PID missing/ambiguous'); assert.ok(pids[0] > 0);
  const args = generatedArguments(debug, log, 'r19_prf01_joined_producer_same_word', cellIndex);
  const measured = argvTokens(args.measuredArguments);
  if (expectedExecutable) assert.equal(measured[0], expectedExecutable, 'measured binary mismatch');
  let totalIr = 0n, childIr = 0n, boundaryExecutions = 0n; const threads = new Set();
  for (const s of profiles) {
    const header = k => { const matches = [...s.matchAll(new RegExp('^' + k + ': (.+)$', 'gm'))]; assert.equal(matches.length, 1, 'ambiguous/missing ' + k); return matches[0][1]; };
    assert.equal(safeNumber(naturalBig(header('pid'))), pids[0]);
    assert.equal(safeNumber(naturalBig(header('part'))), 1, 'ambiguous/incomplete profile parts/dumps');
    const tid = header('thread'); assert.match(tid, /^[1-9]\d*$/);
    assert.ok(!threads.has(tid), 'duplicate thread profile'); threads.add(tid);
    assert.deepEqual(argvTokens(header('cmd')), measured, 'profile/log argv mismatch');
    const events = header('events').trim().split(/\s+/), index = events.indexOf('Ir'); assert.ok(index >= 0);
    const totals = [...s.matchAll(/^totals: (.+)$/gm)]; assert.equal(totals.length, 1, 'missing/ambiguous final totals');
    const vals = totals[0][1].trim().split(/\s+/).map(naturalBig);
    totalIr += vals[index] ?? 0n;
    const functions = profileFunctions(s);
    assert.equal([...functions.values()].reduce((n, f) => n + f.selfIr, 0n), vals[index], 'self costs/final totals mismatch');
    if (tid !== '1') {
      const exact = functions.get(PRODUCER_SYMBOL); assert.ok(exact, 'exact producer boundary absent');
      const inclusive = exact.selfIr + [...exact.edges.values()].reduce((a, b) => a + b, 0n);
      assert.ok(inclusive > 0n, 'exact producer boundary empty');
      assert.equal(inclusive, vals[index], 'exact inclusive/child total mismatch');
      childIr += vals[index];
      let incoming = 0n;
      for (const [caller, f] of functions) {
        const count = f.edgeCalls.get(PRODUCER_SYMBOL) ?? 0n;
        if (count > 0n) { assert.match(caller, /std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure/, 'boundary incoming caller not child context'); incoming += count; }
      }
      assert.equal(incoming, 1n, 'boundary executions unproven or not exactly one');
      boundaryExecutions += incoming;
    }
  }
  const ids = [...threads].map(Number).sort((a, b) => a - b);
  assert.ok(ids.includes(1), 'main thread missing');
  assert.deepEqual(ids, Array.from({ length: ids.length }, (_, i) => i + 1), 'thread profile gaps');
  assert.ok(childIr <= totalIr, 'childIr exceeds totalIr');
  assert.equal(ids.length, 2, 'child profile absent or ambiguous child set');
  assert.equal(boundaryExecutions, 1n, 'boundary executions not one');
  return { pid: pids[0], totalIr: safeNumber(totalIr), childIr: safeNumber(childIr), boundaryExecutions: safeNumber(boundaryExecutions), ...args };
}
function machineBoundary(nm, assembly, control) {
  const definitions = nm.split(/\r?\n/).filter(l => l.endsWith(' ' + PRODUCER_SYMBOL));
  assert.equal(definitions.length, 1, 'exact boundary symbol missing/ambiguous');
  const blocks = [...assembly.matchAll(/^([0-9a-f]+) <(.+)>:\n([\s\S]*?)(?=^[0-9a-f]+ <|$(?![\s\S]))/gm)].map(m => ({ name: m[2], body: m[3].trimEnd() + '\n' }));
  const exact = blocks.filter(b => b.name === PRODUCER_SYMBOL); assert.equal(exact.length, 1, 'exact disassembly definition absent');
  const callers = blocks.filter(b => /std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure/.test(b.name) && b.body.split('\n').some(l => /\bcallq?\s/.test(l) && l.includes('<' + PRODUCER_SYMBOL + '>')));
  assert.ok(callers.length > 0, 'child closure disassembly caller absent');
  const decoded = exact[0].body.split('\n').map(l => /^\s*[0-9a-f]+:\s+([a-z][a-z0-9]*)\b(.*)/.exec(l)).filter(Boolean);
  assert.ok(decoded.length);
  const retAt = decoded.findIndex(m => /^retq?$/.test(m[1]));
  const instructions = retAt < 0 ? decoded : decoded.slice(0, retAt + 1);
  if (control) {
    assert.ok(retAt >= 0, 'control boundary has no ret');
    assert.ok(decoded.slice(retAt + 1).every(m => /^(?:int3|nop[lw]?)$/.test(m[1])), 'control disassembly has code after ret');
    assert.ok(instructions.every(m => /^(?:retq?|endbr64|nop[lw]?)$/.test(m[1])
      || (/^(?:pushq?|popq?)$/.test(m[1]) && /^\s*%rbp\s*$/.test(m[2]))
      || (/^movq?$/.test(m[1]) && /^\s*%rsp,\s*%rbp\s*$/.test(m[2]))
      || /^leaveq?$/.test(m[1])), 'control disassembly contains branch/call/work');
  }
  return { symbol: PRODUCER_SYMBOL, controlEmpty: control, childCallers: callers.length };
}
function boundaryEvidence(b, tag) {
  const nm = wslOk(`nm -C --defined-only ${b.exe}`, `nm @${b.arm}/${tag}`, { timeout: 120_000 });
  const disasm = wslOk(`objdump -Cd --no-show-raw-insn ${b.exe}`, `objdump @${b.arm}/${tag}`, { timeout: 120_000 });
  const verification = machineBoundary(nm.out, disasm.out, b.role === 'control');
  return { binarySha256: b.binarySha256, role: b.role, tag, verification,
    nm: putText(`boundary/${tag}-nm.log`, nm.out), disassembly: putText(`boundary/${tag}-disassembly.log`, disasm.out) };
}
function producerPass(label, arm, role, cellIndex) {
  const b = { ...ctx.builds.arms[arm][role === 'control' ? 'control' : 'iai'], arm, role };
  assert.ok(boundaryEvidence(b, `${label}-${role}`).verification, 'boundary precheck failed');
  const home = `target/prf01-${label}-${role}-obs-${Date.now()}`;
  assert.ok(!fs.existsSync(join(WT_DIR, arm, home)), 'observation home already exists');
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"',
    `cd ${toHost(join(WT_DIR, arm === 'B' && ctx.manifest.rehearsal ? 'A' : arm))}`,
    `IAI_CALLGRIND_LOG=debug IAI_CALLGRIND_COLOR=never ${b.exe} --home ${toHost(join(WT_DIR, arm, home))} 2>&1`].join(' && ');
  const startedAt = now();
  const r = wslOk(script, `${role} run ${label}`, { timeout: 3_600_000 });
  const homeDir = join(WT_DIR, arm, home);
  const files = [];
  const walk = dir => { for (const n of fs.readdirSync(dir)) { const p = join(dir, n); if (fs.statSync(p).isDirectory()) walk(p); else files.push(p); } };
  walk(homeDir);
  const own = files.filter(p => p.replaceAll('\\', '/').includes(PRODUCER));
  const profiles = own.filter(p => /\.out(?:[.#-]+\d+)*$/.test(p) && fs.statSync(p).size > 0);
  const logs = own.filter(p => /\.log(?:[.#-]+\d+)*$/.test(p));
  assert.ok(profiles.length && logs.length, `${role} ${label}: producer profiles/logs missing`);
  const parsed = parseProfiles(profiles.map(p => publicText(fs.readFileSync(p, 'utf8'))),
    publicText(logs.map(p => fs.readFileSync(p, 'utf8')).join('\n')),
    { cellIndex, debug: publicText(r.out), expectedExecutable: publicText(b.exe) });
  assert.equal(binarySha(b.exe), b.binarySha256, `binary drift @${label}/${role}`);
  profiles.forEach((p, i) => putText(`profiles/${label}-${role}-${i}.out`, fs.readFileSync(p, 'utf8')));
  putJson(`profiles/${label}-${role}-inventory.json`, profiles.map((p, i) =>
    ({ receipt: `profiles/${label}-${role}-${i}.out`, sha256: sha(publicText(fs.readFileSync(p, 'utf8'))) })));
  putText(`raw/${role}-${label}.log`, r.out);
  fs.rmSync(homeDir, { recursive: true, force: true });
  return { ...parsed, startedAt, log: putText(`logs/${label}-${role}.log`, r.out) };
}

// --- measure -----------------------------------------------------------------

function parseIr(out, cellNames) {
  const ir = {}; let current = null;
  for (const raw of out.split(/\r?\n/)) {
    const line = raw.replace(/\x1b\[[0-9;]*m/g, ''); // colour never reaches the cell regexes
    // Header lines are bare `crate::group::cell`; IAI_CALLGRIND_LOG=debug lines look like
    // `iai_callgrind_runner::runner::tool: Debug: ...` and must not overwrite the current cell.
    const h = /^(?:[A-Za-z_]\w*::)+([A-Za-z_]\w*)(?:\s.*)?$/.exec(line);
    if (h) { current = h[1]; }
    else {
      const m = /^\s*Instructions:\s*([\d,]+)/.exec(line);
      if (m && current && cellNames.has(current) && ir[current] === undefined)
        ir[current] = Number(m[1].replace(/,/g, ''));
    }
  }
  return ir;
}
// A parse miss must leave the raw output behind: the stage dir is uploaded as a CI artifact.
function checkCells(ir, cellNames, out, what) {
  const got = Object.keys(ir).sort(), want = [...cellNames].sort();
  if (JSON.stringify(got) === JSON.stringify(want)) return;
  putText(`logs/${what}.unparsed.log`, out.slice(0, RAW_LOG_LIMIT));
  const head = out.split(/\r?\n/).slice(0, 60).join('\n');
  assert.fail(`${what}: parsed ${got.length}/${want.length} cells; missing: ${want.filter(c => !(c in ir)).join(',')}\n--- output head ---\n${head}`);
}
function benchRun(run, arm, target, cellNames) {
  const b = { ...ctx.builds.arms[arm].iai, arm, role: 'iai' };
  assert.ok(boundaryEvidence(b, `${run}-iai`).verification, 'boundary precheck');
  const home = `target/prf01-${run}-iai-obs-${Date.now()}`;
  assert.ok(!fs.existsSync(join(WT_DIR, arm, home)), 'observation home already exists');
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"',
    `cd ${toHost(join(WT_DIR, arm === 'B' && ctx.manifest.rehearsal ? 'A' : arm))}`,
    `IAI_CALLGRIND_LOG=debug IAI_CALLGRIND_COLOR=never ${b.exe} --home ${toHost(join(WT_DIR, arm, home))} 2>&1`].join(' && ');
  const startedAt = now();
  const r = wslOk(script, `bench ${target} run ${run}`, { timeout: 3_600_000 });
  const ir = parseIr(r.out, cellNames);
  checkCells(ir, cellNames, r.out, `${run}-${target}`);
  // Store the (redacted) log FIRST, then enforce the tier ceiling: evidence survives the abort.
  const log = putText(`logs/${run}-${target}.log`, r.out);
  assert.ok(Buffer.byteLength(r.out) < RAW_LOG_LIMIT, `raw log ${run}/${target} is ${Buffer.byteLength(r.out)} bytes, over ${RAW_LOG_LIMIT}; the full redacted text is retained at ${log.path}`);
  const pids = measuredPids(join(WT_DIR, arm, home), cellNames, b.exe);
  fs.rmSync(join(WT_DIR, arm, home), { recursive: true, force: true });
  return { run, arm, startedAt, ir, log, pids };
}
function controlsRun(run, arm, cellNames) {
  const b = { ...ctx.builds.arms[arm].controls, arm, role: 'controls' };
  const home = `target/prf01-${run}-controls-obs-${Date.now()}`;
  assert.ok(!fs.existsSync(join(WT_DIR, arm, home)), 'observation home already exists');
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"',
    `cd ${toHost(join(WT_DIR, arm === 'B' && ctx.manifest.rehearsal ? 'A' : arm))}`,
    `IAI_CALLGRIND_COLOR=never ${b.exe} --home ${toHost(join(WT_DIR, arm, home))} 2>&1`].join(' && ');
  const startedAt = now();
  const r = wslOk(script, `controls run ${run}`, { timeout: 3_600_000 });
  const ir = parseIr(r.out, cellNames);
  checkCells(ir, cellNames, r.out, `${run}-controls`);
  // Store the (redacted) log FIRST, then enforce the tier ceiling: evidence survives the abort.
  const log = putText(`logs/${run}-controls.log`, r.out);
  assert.ok(Buffer.byteLength(r.out) < RAW_LOG_LIMIT, `raw log ${run}/controls is ${Buffer.byteLength(r.out)} bytes, over ${RAW_LOG_LIMIT}; the full redacted text is retained at ${log.path}`);
  const pids = measuredPids(join(WT_DIR, arm, home), cellNames, b.exe);
  // Retain the construction cell's Callgrind profiles BEFORE deleting the observation home, so a
  // constructionGainReceipt can be completed by hand without re-measuring (prereg, Replaced 2).
  const construction = [];
  { const keep = dir => { for (const n of fs.readdirSync(dir)) { const p = join(dir, n);
      if (fs.statSync(p).isDirectory()) keep(p);
      else if (p.replaceAll('\\', '/').includes(CONSTRUCTION) && /[.]out/.test(p) && fs.statSync(p).size > 0)
        construction.push(putText(`profiles/${run}-construction-${construction.length}.out`, fs.readFileSync(p, 'utf8'))); } };
    keep(join(WT_DIR, arm, home)); }
  putJson(`profiles/${run}-construction-inventory.json`,
    construction.map(r => ({ receipt: r.path, sha256: r.sha256 })));
  fs.rmSync(join(WT_DIR, arm, home), { recursive: true, force: true });
  return { run, arm, startedAt, ir, log, pids, construction };
}
function measuredPids(homeDir, cellNames, exe) {
  const pids = {};
  const walk = dir => {
    for (const n of fs.readdirSync(dir)) {
      const p = join(dir, n);
      if (fs.statSync(p).isDirectory()) { walk(p); continue; }
      if (!/\.out(?:[.#-]+\d+)*$/.test(p)) continue;
      const cell = [...cellNames].find(c => p.replaceAll('\\', '/').split('/').includes(c));
      if (!cell) continue;
      const content = fs.readFileSync(p, 'utf8');
      const pid = Number(/^pid: (\d+)$/m.exec(content)?.[1]);
      assert.ok(Number.isSafeInteger(pid) && pid > 0, `missing measured PID: ${p}`);
      const cmd = /^cmd: (.+)$/m.exec(content)?.[1];
      assert.equal(argvTokens(cmd ?? '')[0], exe, `measured binary mismatch: ${p}`);
      if (pids[cell] !== undefined) assert.equal(pids[cell], pid, `ambiguous measured PID: ${cell}`);
      pids[cell] = pid;
    }
  };
  walk(homeDir);
  for (const cell of cellNames) assert.ok(pids[cell], `missing measured PID: ${cell}`);
  return pids;
}
function binarySha(exe) { return wslOk(`sha256sum ${exe}`, 'sha256sum binary').out.split(/\s+/)[0]; }

function cmdMeasure() {
  readStage('manifest.json', m => { ctx.manifest = m; });
  readStage('builds.json', b => { ctx.builds = b; });
  readStage('oracle.json', o => { ctx.oracle = o; });
  verifyIdentities('measure');
  assert.equal(ctx.builds.manifestSha256, shaFile(local('manifest.json')), 'builds.json does not belong to this manifest (stage binding)');
  assert.equal(ctx.oracle.manifestSha256, shaFile(local('manifest.json')), 'oracle.json does not belong to this manifest (stage binding)');
  assert.equal(ctx.oracle.buildsSha256, shaFile(local('builds.json')), 'oracle.json does not belong to this builds.json (stage binding)');
  const m = ctx.manifest;
  const producerCellIndex = perfGateIndex(fs.readFileSync(join(REPO, 'benches/perf_gate_iai.rs'))).index;
  assert.ok(producerCellIndex >= 0, 'producer cell not in the perf_gate group');
  const runs = RUNS.map(label => ({ label, arm: RUN_ARM[label],
    iai: benchRun(label, RUN_ARM[label], IAI_TARGET, new Set(IAI_CELLS)),
    controls: controlsRun(label, RUN_ARM[label], new Set(CONTROLS_CELLS)) }));
  const controls = {}, producerProfiles = {};
  for (const label of ['A1', 'A2', 'B1', 'B2', 'C0']) {
    controls[label] = producerPass(label, RUN_ARM[label], 'control', producerCellIndex);
    producerProfiles[label] = producerPass(label, RUN_ARM[label], 'iai', producerCellIndex);
  }
  for (const arm of ['A', 'B', 'C']) {
    assert.equal(binarySha(ctx.builds.arms[arm].iai.exe), ctx.builds.arms[arm].iai.binarySha256, `binary drift @${arm}`);
    assert.equal(binarySha(ctx.builds.arms[arm].control.exe), ctx.builds.arms[arm].control.binarySha256, `binary drift @${arm}/control`);
    assert.equal(binarySha(ctx.builds.arms[arm].controls.exe), ctx.builds.arms[arm].controls.binarySha256, `binary drift @${arm}/controls`);
  }
  const hashes = frozenHashes();
  const environmentSha256 = ctx.builds.environmentSha256;
  const srcOf = arm => sourceOf(m.arches[arm]);
  const pinBinaries = arm => ({
    iai: ctx.builds.arms[arm].iai.binarySha256, controls: ctx.builds.arms[arm].controls.binarySha256,
    oracle: ctx.builds.arms[arm === 'C' ? 'A' : arm].oracle.binarySha256, control: ctx.builds.arms[arm].control.binarySha256 });
  // Record the OBSERVED tool version, not a literal (re-freeze 9); the CI workflow pins ^0.14.
  const runnerVersionOf = t => { const m = /(\d+[.]\d+[.]\d+)/.exec(t.split('\n').find(l => l.includes('iai-callgrind-runner')) ?? ''); return m ? m[1] : '0.14.2'; };
  const runnerVersion = runnerVersionOf(String((() => { try { return fs.readFileSync(local('environment.log'), 'utf8'); } catch { return ''; } })()));
  let runIdSeq = 0;
  const startedOf = (r, kind) => r[kind].startedAt;
  const logOf = (r, kind) => r[kind].log.path;
  const probeGlobal = label => ctx.oracle.records[label]['probe-global'].row;
  const hotConfig = (name, label) => {
    const arm = label.startsWith('B') ? 'B' : 'A';
    const [work, block, alignment] = HOT_DECL[name];
    const probe = probeGlobal(label);
    const requested = { maintenance: probe.config.requested.maintenance, workCount: work, segmentCount: null, blockBytes: block, cachePolicy: probe.config.requested.cachePolicy };
    return { requested, resolved: { ...requested, maintenance: probe.config.resolved.maintenance, cachePolicy: probe.config.resolved.cachePolicy }, configConflictsDelta: probe.config.configConflictsDelta, processBoundary: 'isolated-process',
      unobservedReasons: { segmentCount: 'measured body has no paired geometry observer; probe census is not workload geometry' },
      declaredProvenance: { receipt: 'manifest.json', cell: name, fields: ['workCount', 'blockBytes', 'alignment'], alignment },
      resolvedProvenance: { runId: probe.config.runId, pid: probe.config.pid, arm, binarySha256: ctx.builds.arms[arm].oracle.binarySha256,
        receipt: `oracle/${label}-probe-global.json`, claimPath: 'global',
        copiedFields: ['cachePolicy', 'configConflictsDelta', 'maintenance'] } };
  };
  const COUNTERS8 = ['pending_words_visited','pending_words_exchanged','nonempty_cuts','discovery_records_reclaimed','routed_scans','summary_words_visited','summary_rmw','summary_load'];
  const oracleRow = (label, mode, primaryBinary) => {
    const rec = ctx.oracle.records[label][mode];
    return { ...Object.fromEntries(COUNTERS8.map(k => [k, rec.row.oracle[k]])),
      strict_trim_remaining: rec.row.oracle.strict_trim_remaining,
      unobservedReasons: rec.row.oracle.unobservedReasons,
      linkage: { binarySha256: rec.binarySha256, primaryBinarySha256: primaryBinary, source: rec.source,
        features: FEATURES.split(' '), cfg: [ORACLE_CFG], environmentSha256,
        receipt: `oracle/${label}-${mode}.json`, featureExactReceipt: 'oracle.json',
        startedAt: new Date(rec.row.startedAtUnixMs).toISOString(),
        config: { ...rec.row.config } } };
  };
  const input = { version: 1, hashes, environmentSha256, ...(ctx.rehearsal ? { rehearsal: true } : {}),
    inventoryReceipt: 'manifest.json',
    identityReviewReceipt: 'per-stage identity binding (see the preregistration limitations section)',
    environmentReceipt: 'environment.log',
    oracleReceipt: 'oracle.json',
    entry: 'SeferAlloc::GlobalAlloc->HeapCore routed',
    nativeDeferred: { deferred: true, method: 'batch-timer native paired A/B on a quiet bare-metal host; the R19 per-sample timer is replaced by fixed-batch timing (deferred; not launched in v2)' },
    inventory: [...BENEFIT, ...HOT_CELLS, CONSTRUCTION, ...MI, 'r19_prf01_publication', PRODUCER],
    identities: {
      A: { label: 'A', measurementCommit: srcOf('A').baseHead, source: srcOf('A'), environmentSha256,
        binaries: pinBinaries('A'), buildStartedAt: ctx.builds.buildStartedAt, buildReceipt: 'builds.json',
        features: FEATURES.split(' '), primaryCfg: [], oracleCfg: [ORACLE_CFG],
        toolchain: { rustc: 'environment.log', target: 'environment.log', profile: 'release', flags: 'role cfg only' } },
      B: { label: 'B', measurementCommit: srcOf('B').baseHead, source: srcOf('B'), environmentSha256,
        ...(ctx.rehearsal ? { oracleSource: sourceOf(m.arches.oracleB) } : {}),
        binaries: pinBinaries('B'), buildStartedAt: ctx.builds.buildStartedAt, buildReceipt: 'builds.json',
        features: FEATURES.split(' '), primaryCfg: [], oracleCfg: [ORACLE_CFG],
        toolchain: { rustc: 'environment.log', target: 'environment.log', profile: 'release', flags: 'role cfg only' } },
      C0: { source: srcOf('A'), binarySha256: pinBinaries('C').iai, controlsBinarySha256: pinBinaries('C').controls, buildStartedAt: ctx.builds.buildStartedAt, buildReceipt: 'builds.json' },
    },
    sizeOf: { A: ctx.oracle.records.A1[BENEFIT[0]].row.smallSidecarSizeBytes,
      B: ctx.oracle.records.B1[BENEFIT[0]].row.smallSidecarSizeBytes, receipt: 'oracle.json' },
    ancestry: { result: 'A ancestor B', chain: [srcOf('A').baseHead, srcOf('B').baseHead], receipt: 'manifest.json' },
    iai: [] };
  for (const r of runs) {
    const arm = r.arm;
    for (const name of IAI_CELLS) {
      const needed = [...BENEFIT, 'r19_prf01_publication', PRODUCER].includes(name);
      const mode = name === 'r19_prf01_publication' ? 'publication' : name === PRODUCER ? 'producer' : name;
      const oracle = needed
        ? oracleRow(r.label, mode, ctx.builds.arms[arm].iai.binarySha256)
        : { ...Object.fromEntries([...COUNTERS8, 'strict_trim_remaining'].map(k => [k, null])),
            unobservedReasons: Object.fromEntries([...COUNTERS8, 'strict_trim_remaining'].map(k => [k, 'no paired oracle for this control'])),
            linkage: null };
      let config;
      if (MI.includes(name)) {
        config = { requested: null, resolved: null, configConflictsDelta: null,
          notApplicableReason: 'mimalloc does not resolve SeferAlloc configuration', processBoundary: 'isolated-process' };
      } else {
        const rec = ctx.oracle.records[r.label][mode].row;
        config = { ...rec.config, requested: { ...rec.config.requested }, resolved: { ...rec.config.resolved } };
      }
      const extra = needed ? {} : {};
      const row = { name, arm: r.label, Ir: r.iai.ir[name], environmentSha256,
        binarySha256: pinBinaries(arm).iai, source: srcOf(arm), startedAt: startedOf(r, 'iai'),
        features: FEATURES.split(' '), receipt: logOf(r, 'iai'),
        config: { ...config, pid: r.iai.pids[name], runId: `${r.label}-${name}-${++runIdSeq}` }, oracle, ...extra };
      if (name === PRODUCER) {
        const control = controls[r.label];
        const primary = producerProfiles[r.label];
        const boundaryOverheadIr = control.childIr;
        assert.equal(primary.totalIr, row.Ir, `producer rerun Ir mismatch @${r.label}`);
        assert.ok(boundaryOverheadIr >= 1 && boundaryOverheadIr <= 16, `boundary overhead ${boundaryOverheadIr} outside 1..16 @${r.label}`);
        row.scenario = 'joined remote Small frees; repeated frees in same pending word; no owner discovery during producer window';
        row.producerIrReceipt = { binarySha256: pinBinaries(arm).iai, source: srcOf(arm), environmentSha256,
          features: FEATURES.split(' '), cfg: [], receipt: logOf(r, 'iai'),
          symbolReceipt: `boundary/${r.label}-iai-nm.log`, profileReceipt: `profiles/${r.label}-iai-inventory.json`,
          symbol: PRODUCER_SYMBOL, runnerVersion, valgrindVersion: 'environment.log',
          collectAtStart: false, defaultToggle: false, entryPoint: 'Custom', separateThreads: true,
          toggleCollect: PRODUCER_SYMBOL, generatedArguments: primary.generatedArguments,
          aggregation: 'sum of all thread profile Ir', childFreeCount: 4096, childIr: primary.childIr,
          boundaryOverheadIr, workloadIr: primary.childIr - boundaryOverheadIr, boundaryExecutions: 1,
          totalIr: row.Ir, startedAt: primary.startedAt,
          control: { receipt: `logs/${r.label}-control.log`, workload: 'child remote frees removed',
            source: srcOf(arm), binarySha256: pinBinaries(arm).control, environmentSha256,
            childIr: control.childIr, workloadIr: 0, childFreeCount: 0, boundaryExecutions: 1,
            symbolReceipt: `boundary/${r.label}-control-nm.log`, profileReceipt: `profiles/${r.label}-control-inventory.json`,
            symbol: PRODUCER_SYMBOL, toggleCollect: PRODUCER_SYMBOL, cfg: [CONTROL_CFG],
            features: FEATURES.split(' '), generatedArguments: control.generatedArguments,
            runnerVersion, valgrindVersion: 'environment.log', collectAtStart: false,
            defaultToggle: false, entryPoint: 'Custom', separateThreads: true,
            aggregation: 'sum of all thread profile Ir', totalIr: control.totalIr,
            deltaIr: row.Ir - control.totalIr, parentSetupUnchanged: true, collectionUnchanged: true } };
        assert.notEqual(row.producerIrReceipt.control.totalIr, row.Ir, 'non-distinguishing control');
        assert.ok(row.producerIrReceipt.control.deltaIr > 0, 'non-distinguishing control (deltaIr)');
        assert.ok(row.producerIrReceipt.workloadIr > 0, 'empty primary workload remainder');
      }
      input.iai.push(row);
    }
    for (const name of CONTROLS_CELLS) {
      let ir = r.controls.ir[name];
      const slopped = ctx.slop && r.label === 'B1' && name === 'r20_prf01_small_churn_16b_2n';
      if (slopped) { ir += 11; console.log(`[measure] COUNTERFACTUAL: ${name}/B1 Ir inflated to ${ir}`); }
      input.iai.push({ name, arm: r.label, Ir: ir, environmentSha256,
        binarySha256: pinBinaries(arm).controls, source: srcOf(arm), startedAt: startedOf(r, 'controls'),
        features: FEATURES.split(' '), receipt: logOf(r, 'controls'),
        config: { ...hotConfig(name, r.label), pid: r.controls.pids[name], runId: `${r.label}-${name}-${++runIdSeq}` },
        oracle: { ...Object.fromEntries([...COUNTERS8, 'strict_trim_remaining'].map(k => [k, null])),
          unobservedReasons: Object.fromEntries([...COUNTERS8, 'strict_trim_remaining'].map(k => [k, 'no paired oracle for this control'])), linkage: null } });
    }
  }
  // Counterfactual durability (re-freeze 9): --slop data is marked in the input AND the judge
  // returns INVALID for any counterfactual marker outside rehearsal.
  if (ctx.slop) input.counterfactual = { slop: true, arm: 'B1', cell: 'r20_prf01_small_churn_16b_2n', deltaIr: 11,
    note: 'negative control injected by --slop (rehearsal only); not gate data' };
  // Receipt-hash anchoring: sha256 of every out-dir file referenced by any receipt field, plus the
  // retained construction inventories. The judge treats the field as optional schema and never uses
  // it in the verdict; scripts/r20_prf01_verify_artifacts.mjs is the acceptance consumer.
  const receiptHashes = {};
  // Receipt strings may repeat (e.g. identities.*.toolchain.rustc = 'environment.log'): anchor idempotently.
  const addReceipt = p => { if (!(p in receiptHashes)) receiptHashes[p] = shaFile(local(p)); };
  const scanReceipts = v => {
    if (typeof v === 'string') { if (/^(?:environment[.]log|manifest[.]json|builds[.]json|oracle[.]json|(?:logs|boundary|profiles|oracle)\/)/.test(v)) addReceipt(v); }
    else if (Array.isArray(v)) v.forEach(scanReceipts);
    else if (v && typeof v === 'object') Object.values(v).forEach(scanReceipts);
  };
  scanReceipts(input);
  for (const run of RUNS) addReceipt(`profiles/${run}-construction-inventory.json`);
  input.receiptHashes = receiptHashes;
  putJson('input.json', input);
  const csv = [...(ctx.slop ? ['# counterfactual: --slop inflated B1/r20_prf01_small_churn_16b_2n by +11 Ir; not gate data (see counterfactual in input.json)'] : []),
    'target,arm,name,ir,binarySha256',
    ...input.iai.map(r => `${CONTROLS_CELLS.includes(r.name) ? 'controls' : 'iai'},${r.arm},${r.name},${r.Ir},${r.binarySha256}`)].join('\n') + '\n';
  putText('summary.csv', csv);
  const judge = spawnSync(process.execPath, [join(REPO, JUDGE), '--input', local('input.json')], { encoding: 'utf8', maxBuffer: 1 << 24 });
  putText('verdict.txt', `exit=${judge.status}\n${judge.stdout}${judge.stderr}`);
  process.stdout.write(judge.stdout);
  if (judge.stderr) process.stderr.write(judge.stderr);
  console.log(`[measure] input.json + summary.csv + verdict.txt written to ${ctx.outDir}`);
  process.exitCode = judge.status === 0 ? 0 : 1;
}

// --- main --------------------------------------------------------------------

function stagePaths() {
  if (process.argv.includes('--out-dir')) {
    assert.ok(!ctx.rehearsal, '--out-dir is only for non-rehearsal stages');
    const value = process.argv[process.argv.indexOf('--out-dir') + 1];
    assert.ok(value && !value.startsWith('--'), '--out-dir requires a path');
    return resolve(REPO, value);
  }
  return resolve(REPO, ctx.rehearsal ? 'target/r20p1-rehearsal' : 'docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01');
}
function rehearsal() {
  ctx.outDir = stagePaths();
  putText('REHEARSAL.txt', "REHEARSAL - not gate data. The iai arm B is a second instance of the A source (label A'),\nbuilt with -C debuginfo=line-tables-only; the oracle stage runs the real B build (path activation only).\n");
  cmdPrepare(); cmdBuild(); cmdOracle(); cmdMeasure();
}
function cmdCleanup() {
  for (const label of ['A', 'B', 'C']) {
    const wt = join(WT_DIR, label);
    if (fs.existsSync(wt)) git(REPO, ['worktree', 'remove', '--force', wt]);
  }
  git(REPO, ['branch', '-D', 'r20p1-b-tmp'], { allowFailure: true });
  try { fs.rmdirSync(WT_DIR); } catch { /* not empty or absent */ }
  wslOk('rm -rf /tmp/r20p1', 'wsl cleanup', { timeout: 120_000 });
  console.log('[cleanup] arm worktrees, temp branch and target dirs removed');
}
function main() {
  const args = process.argv.slice(2);
  ctx.slop = args.includes('--slop');
  const modes = args.filter(a => a.startsWith('--') && !['--slop', '--out-dir'].includes(a));
  assert.equal(modes.length, 1, `expected exactly one mode, got: ${modes}`);
  assert.ok(!ctx.slop || modes[0] === '--rehearsal',
    '--slop is a negative control and is allowed only with --rehearsal; it must never contaminate an official measure run');
  const mode = modes[0];
  if (mode === '--cleanup') return cmdCleanup();
  if (mode === '--rehearsal') { ctx.rehearsal = true; return rehearsal(); }
  ctx.outDir = stagePaths();
  if (mode === '--prepare') return cmdPrepare();
  if (mode === '--build') return cmdBuild();
  if (mode === '--oracle') return cmdOracle();
  if (mode === '--measure') return cmdMeasure();
  throw new Error(`unknown mode ${mode}`);
}
main();
