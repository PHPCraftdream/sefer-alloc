// R20 PRF-01 C3 runner, minimally adapted from the validated v2 staged runner.
// The preregistration/support module is authoritative for inventory and freezes.
//
// Modeled on scripts/r20_prf04_claim_path_run.mjs (stage pipeline, identity
// manifest, sanitised logs) and inheriting the producer-boundary machinery
// (EntryPoint::Custom profiles, nm/disassembly receipts, Amendment B coverage)
// from scripts/r19_prf01_sidecar_summary_run.mjs in a reduced form.
//
// Commands:
//   --frozen-manifest <json> (operator-captured gate --hashes output as [{path,sha256}])
//               no git worktree, branch, cherry-pick, or commit is created.
//   --build     per-arm builds (native on Linux, wsl.exe on Windows):
//               perf_gate_iai (iai), r20_prf01_controls_iai (controls),
//               r19_prf01_oracle example under cfg r19_sidecar_summary_oracle,
//               perf_gate_iai control under cfg r19_prf01_producer_control.
//               C is built with -C debuginfo=line-tables-only (registered).
//   --oracle    C3 candidate oracle --c3-json <cell> <processId> for required
//               per-cell/per-process counter observations.
//   --measure   iai runs A1,A2,B1,B2,C0 (both bench targets) + one producer
//               control run per label; parses Ir and child profiles; builds
//               the judge input; invokes the judge; derives summary.csv from
//               the SAME JSON.
//   --rehearsal full pipeline WITHOUT a real B in the iai stage (arm B = A'
//               under the registered distinguishing flags); the oracle stage
//               still uses the real B (path activation only).
//   --slop      inject +11 Ir into one small_churn_16b_2n B1 cell (negative
//               control: expected NO-GO/INVALID).
//   --cleanup   not supported: arm dirs are caller-owned and never removed.
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
import { dirname, join, relative, resolve, sep } from 'node:path';
import * as C3 from './r20_prf01_c3_support.mjs';
import { fileURLToPath } from 'node:url';
import { applyHarnessOverlay, validateC3OracleCounters, selfTestC3OracleCounters, selfTestMergeHarnessCargo, mergeHarnessCargo, measureToInput as buildMeasureInput, parseIr } from './r20_prf01_c3_runner_support.mjs';

const RUNNER_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(RUNNER_DIR, '..');
const REPO_ROOT = resolve(spawnSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'],
  { cwd: REPO, encoding: 'utf8' }).stdout.trim(), '..');
const parseArg = name => { const i=process.argv.indexOf(name); return i<0?null:process.argv[i+1]; };
const RAW_LOG_LIMIT = 2 * 1024 * 1024;
const PRODUCER_SETTINGS = ['--tool=callgrind', '--error-exitcode=0', '--trace-children=yes', '--fair-sched=try',
  '--I1=32768,8,64', '--D1=32768,8,64', '--LL=8388608,16,64', '--cache-sim=yes', '--compress-strings=no',
  '--compress-pos=no', '--dump-line=yes', '--dump-instr=no', '--combine-dumps=no', '--separate-threads=yes'];
const sha = b => createHash('sha256').update(b).digest('hex');
const shaFile = p => sha(fs.readFileSync(p));
const now = () => new Date().toISOString();
const IS_WIN = process.platform === 'win32';
const winToWsl = path => {
  const match = /^([A-Za-z]):(.*)$/.exec(path);
  assert.ok(match, `not an absolute Windows path: ${path}`);
  return `/mnt/${match[1].toLowerCase()}${match[2].replace(/\\/g, '/')}`;
};
const toHost = p => IS_WIN ? winToWsl(p) : p;
const RUN_ROOT = resolve(parseArg('--arm-root') ?? join(REPO, 'worktrees', 'r20p1-c3'));
const armDir = arm => join(RUN_ROOT, arm);
const JUDGE = 'scripts/r20_prf01_c3_gate.mjs';
const PREREG = 'docs/perf/R20_PRF01_C3_DEFAULT_PREREG.md';
const IAI_TARGET = 'perf_gate_iai';
const CONTROLS_TARGET = 'r20_prf01_controls_iai';
const ORACLE_TARGET = 'r20_prf01_c3_oracle';
const TRIM_TARGET = C3.TRIM_TARGET;
const FEATURES = C3.FEATURES.join(' ');
const ORACLE_CFG = 'r19_sidecar_summary_oracle';
const SUMMARY_ORACLE_CFG = 'r20_prf01_c3_oracle';
const CONTROL_CFG = 'r19_prf01_producer_control';
const C3_CONTROL_CFG = 'r20_prf01_c3_producer_control';
const DISTINGUISH_RUSTFLAGS = '-C debuginfo=line-tables-only';
const BENEFIT = [...C3.V2_BENEFIT];
const HOT_FAMILIES = [...C3.HOT_FAMILIES];
const HOT_CELLS = [...C3.HOT_CELLS];
const CONSTRUCTION = C3.CONSTRUCTION;
const MI = [...C3.MIMALLOC];
const PRODUCER = C3.SAME_WORD_PRODUCER;
const IAI_CELLS = C3.INVENTORY.filter(n => C3.targetFor(n) === 'iai');
const CONTROLS_CELLS = C3.INVENTORY.filter(n => C3.targetFor(n) === 'controls');
const C3_CELLS = C3.INVENTORY.filter(n => C3.targetFor(n) === 'c3');
const PRODUCER_SPECS = C3.PRODUCERS;
const TRIM_CELLS = C3.INVENTORY.filter(n => C3.targetFor(n) === 'trim');
const HOT_DECL = {
  [CONSTRUCTION]: [1, 16, 8],
  ...Object.fromEntries(HOT_FAMILIES.flatMap(f =>
    [['n', 64], ['2n', 128], ['4n', 256]].map(([suffix, ops]) => [
      `r20_prf01_${f}_${suffix}`,
      [ops,
        f === 'small_churn_16b' ? 16 : f === 'aligned_churn_640b_a128' ? 640 : 256,
        f === 'aligned_churn_640b_a128' ? 128 : 8],
    ]),
  )),
};
const REQUIRED_STAGES = ['--prepare','--build','--oracle','--measure','--selftest'];
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
  assert.equal(
    r.code,
    0,
    `${what} failed (exit ${r.code})\n--- stderr tail ---\n${r.err.split('\n').slice(-40).join('\n')}`,
  );
  return r;
}

const ctx = { outDir: null, rehearsal: false, slop: false, manifest: null, builds: null, oracle: null,
  candidate: null, armDirs: null };
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
  const check=spawnSync(process.execPath,[join(REPO,JUDGE),'--check-hashes'],{encoding:'utf8'});
  // A rehearsal is harness validation only and never gate data: before the first freeze it runs
  // against the live hashes and records that it was unfrozen. Official stages always require freeze.
  const unfrozen = check.status !== 0 && /not frozen yet/.test(check.stdout);
  if (unfrozen && ctx.rehearsal) {
    if (!ctx.unfrozenRehearsal) console.log('[rehearsal] UNFROZEN: prereg .sha256 absent; live hashes used, not gate data');
    ctx.unfrozenRehearsal = true;
  }
  else assert.equal(check.status,0,`frozen hash drift or snapshot mismatch\n${check.stdout}${check.stderr}`);
  const list=spawnSync(process.execPath,[join(REPO,JUDGE),'--hashes'],{encoding:'utf8'});
  assert.equal(list.status,0,'gate --hashes failed');
  const actual = list.stdout.trim().split(/\s+/).reduce((rows, token, index, all) => {
    if (index % 2 === 0) rows.push({ sha256: token, path: all[index + 1] });
    return rows;
  }, []);
  return actual;
}

// Per-stage identity binding (re-freeze 9): rebuild the prepare-stage identities from the live
// worktrees and compare with manifest.json before any stage does its work. The official out-dir
// (docs/perf/R20_PRF01_SIDECAR_SUMMARY_V2_RUN01) lives in the PRIMARY checkout and is not part of
// the worktrees A/B/C, so none of these checks read it.
function verifyIdentities(stage) {
  const m=ctx.manifest;
  const hashes=frozenHashes();
  assert.deepEqual(m.frozen,hashes,'frozen operator snapshot drift at '+stage);
  assert.deepEqual(
    hashes,
    C3.FROZEN_FILES.map(path => ({ path, sha256: shaFile(join(REPO, path)) })),
    'live frozen file drift at ' + stage,
  );
  for(const label of ['A','B','C']){
    const live=archIdentity(label,ctx.armDirs[label]),got=m.arches[label];
    assert.ok(got,`manifest has no arm ${label} at ${stage}`);
    for (const key of ['head', 'tree', 'patchSha256', 'untrackedSha256', 'identitySha256']) {
      assert.equal(got[key], live[key], `arm ${label} identity drift at ${stage}`);
    }
  }
  assert.equal(m.candidate,ctx.candidate);
}

function baseHead() {
  return git(REPO, ['rev-parse', 'HEAD']).out;
}
function harnessOverlay(dir) {
  // Untrimmed: git() trims stdout, which would eat the first entry's leading status space.
  const raw = spawnSync('git', ['--no-optional-locks', 'status', '--porcelain', '-z'],
    { cwd: dir, encoding: 'utf8', env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } });
  assert.equal(raw.status, 0, `git status failed: ${raw.stderr}`);
  const status = raw.stdout;
  const changes = [];
  for (const entry of status.split('\0').filter(Boolean)) {
    const path = entry.slice(3);
    assert.ok(C3.FROZEN_FILES.includes(path), `non-frozen overlay path forbidden: ${path}`);
    const bytes = fs.readFileSync(join(dir, path));
    changes.push({
      path,
      sha256: sha(bytes),
      base64: bytes.toString('base64'),
      untracked: entry.startsWith('?? '),
    });
  }
  return changes.sort((a, b) => a.path.localeCompare(b.path));
}
function sourceIdentity(head,tree,overlay) {
  const canonical=overlay.map(({path,sha256})=>({path,sha256}));
  const patchSha256=sha(JSON.stringify(canonical));
  const untrackedSha256=sha(JSON.stringify(overlay.filter(x=>x.untracked).map(({path,sha256})=>({path,sha256}))));
  const identitySha256=sha(JSON.stringify([head,tree,patchSha256,untrackedSha256]));
  return {baseHead:head,tree,patchSha256,untrackedSha256,identitySha256};
}
function archIdentity(label, dir) {
  assert.ok(dir && fs.existsSync(dir), `arm directory missing: ${label}`);
  const head = git(dir, ['rev-parse', 'HEAD']).out;
  const tree = git(dir, ['rev-parse', 'HEAD^{tree}']).out;
  assert.match(head,/^[0-9a-f]{40}$/); assert.match(tree,/^[0-9a-f]{40}$/);
  const overlay = harnessOverlay(dir);
  const identity = sourceIdentity(head, tree, overlay);
  return { label, wt: `<repo>/worktrees/r20p1-c3/${label}`, head, tree, ...identity,
    overlay, cargoLockSha256: shaFile(join(dir, 'Cargo.lock')), capturedAt: now() };
}
function sourceOf(a) { return (({ baseHead, tree, patchSha256, untrackedSha256, identitySha256, capturedAt }) =>
  ({ baseHead, tree, patchSha256, untrackedSha256, identitySha256, capturedAt }))(a); }
function rehearsalRustflags(label, rehearsal) {
  return rehearsal && label === 'B' ? DISTINGUISH_RUSTFLAGS : undefined;
}
function cleanupOwnership(root, arms) {
  return { wtDir: '<repo>/worktrees/r20p1-c3', ownedWorktrees: [...arms], root };
}
function worktreeAddArgs(path, ref) {
  return ['worktree', 'add', '--detach', path, ref];
}
function ensureWorktree(label, ref) {
  const wt = armDir(label);
  if (fs.existsSync(wt)) {
    assert.equal(git(wt, ['rev-parse', 'HEAD']).out, ref,
      `worktree ${label} is not at ${ref}; remove it manually and rerun`);
  } else {
    fs.mkdirSync(RUN_ROOT, { recursive: true });
    git(REPO, worktreeAddArgs(wt, ref));
  }
  applyHarnessOverlay({ sourceRoot: REPO, targetRoot: wt, frozenFiles: C3.FROZEN_FILES });
  return wt;
}
function parseDispatch() {
  const rawCandidate = parseArg('--candidate');
  let candidate = null;
  if (!ctx.rehearsal) {
    assert.ok(
      rawCandidate && rawCandidate !== 'TBD',
      '--candidate must be an immutable commit reference; TBD rejected',
    );
    candidate = git(REPO, ['rev-parse', `${rawCandidate}^{commit}`]).out;
    assert.match(candidate, /^[0-9a-f]{40}$/, 'candidate must resolve to a commit SHA');
  } else if (rawCandidate && rawCandidate !== 'TBD') {
    candidate = git(REPO, ['rev-parse', `${rawCandidate}^{commit}`]).out;
  }
  const base = ctx.rehearsal
    ? git(REPO, ['rev-parse', 'origin/main']).out
    : git(REPO, ['merge-base', candidate, 'origin/main']).out;
  return { candidate: ctx.rehearsal ? base : candidate, base };
}
function cmdPrepare() {
  const dispatch = parseDispatch();
  ctx.candidate = dispatch.candidate;
  ctx.armDirs = Object.fromEntries(['A','B','C'].map(label => [label, armDir(label)]));
  ctx.outDir = stagePaths();
  assert.ok(
    !fs.existsSync(local('manifest.json')),
    `manifest exists, refusing to overwrite: ${local('manifest.json')}`,
  );
  requireC3Prerequisites('prepare');
  fs.mkdirSync(ctx.outDir, { recursive: true });
  const a = ensureWorktree('A', dispatch.base);
  const b = ensureWorktree('B', ctx.rehearsal ? dispatch.base : dispatch.candidate);
  const c = ensureWorktree('C', dispatch.base);
  const capturedAt = now();
  const arches = Object.fromEntries(['A', 'B', 'C'].map(label => [
    label, { ...archIdentity(label, { A: a, B: b, C: c }[label]), capturedAt },
  ]));
  const source = sourceOf(arches.A);
  arches.C = { ...arches.C, ...source };
  if (ctx.rehearsal) assert.deepEqual(sourceOf(arches.B), sourceOf(arches.A), 'rehearsal B must use A source');
  assert.deepEqual(sourceOf(arches.C), sourceOf(arches.A), 'C0 source overlay must exactly match A');
  const frozen = frozenHashes();
  const pathLengths = Object.values(ctx.armDirs).map(p => p.length);
  assert.equal(new Set(pathLengths).size, 1, 'A/B/C full path lengths must be equal');
  putJson('manifest.json', { version: 1, rehearsal: ctx.rehearsal, candidate: ctx.candidate,
    capturedAt, baseHead: dispatch.base, frozen, arches,
    ...cleanupOwnership(RUN_ROOT, ['A','B','C']), pathLengths });
  console.log(`[prepare] candidate=${ctx.candidate} base=${dispatch.base} rehearsal=${ctx.rehearsal}`);
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
function cargoBuild({ wt, label, target, features, rustflags, kind, role }) {
  const td = `/tmp/r20p1/${label}-${target}-${role}-${kind}-`
    + `${createHash('sha1').update(rustflags ?? 'primary').digest('hex').slice(0, 10)}`;
  const rf = rustflags ? `RUSTFLAGS='${rustflags}' ` : '';
  const script = [
    'set -e',
    'export PATH="$HOME/.cargo/bin:$PATH"',
    'RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER=',
    `cd ${toHost(wt)}`,
    `${rf}CARGO_TARGET_DIR=${td} cargo ${kind} --locked --message-format=json-render-diagnostics`
      + ` ${kind === 'bench' ? `--bench ${target} --no-run` : `--example ${target}`}`
      + ` --features '${features}'`,
  ].join(' && ');
  const r = wslOk(script, `build ${target} @${label}`, { timeout: 900_000 });
  const exe = executableOf(r.out, target);
  const sum = wslOk(`sha256sum ${exe}`, 'sha256sum binary').out.split(/\s+/)[0];
  assert.match(
    sum,
    /^[0-9a-f]{64}$/,
  );
  return {
    target, kind, exe, binarySha256: sum, features,
    rustflags: rustflags ?? '', cfg: rustflags ?? '', targetDir: td,
  };
}
function requireC3Prerequisites(stage) {
  const missing = C3.validateSources().filter(x => x.status !== 'ok');
  const expected = C3.FROZEN_FILES.filter(p => !fs.existsSync(join(REPO,p)));
  const all=[...missing.map(x=>`${x.status}: ${x.file} (${x.name})`),...expected.map(x=>`frozen source missing: ${x}`)];
  assert.equal(all.length, 0, `fail closed at ${stage}; external prerequisites missing: ${all.join('; ')}`);
}
function loadManifest() {
  readStage('manifest.json', m => { ctx.manifest=m; ctx.candidate=m.candidate; });
  ctx.armDirs=Object.fromEntries(['A','B','C'].map(x=>[x,armDir(x)]));
  ctx.rehearsal = ctx.manifest.rehearsal === true;
  const base = ctx.manifest.baseHead;
  const expectedB = ctx.manifest.rehearsal ? base : ctx.manifest.candidate;
  assert.equal(git(ctx.armDirs.A,['rev-parse','HEAD']).out,base,'manifest A dir must match base SHA');
  assert.equal(git(ctx.armDirs.B,['rev-parse','HEAD']).out,expectedB,'manifest B dir must match expected source SHA');
  assert.equal(git(ctx.armDirs.C,['rev-parse','HEAD']).out,base,'manifest C dir must match base SHA');
  assert.ok(
    ctx.manifest.candidate && /^[0-9a-f]{40}$/.test(ctx.manifest.candidate),
    'manifest candidate/base SHA missing',
  );
}
function expectedOracleCfg(label, rehearsal) {
  return [ORACLE_CFG, ...(label === 'B' && !rehearsal ? [SUMMARY_ORACLE_CFG] : [])];
}
function buildRolePlan(label, rehearsal) {
  const flags = rehearsalRustflags(label, rehearsal);
  const withCfg = cfg => [flags, cfg].filter(Boolean).join(' ') || undefined;
  return {
    primary: flags,
    oracle: withCfg(expectedOracleCfg(label, rehearsal).map(cfg => `--cfg ${cfg}`).join(' ')),
    control: withCfg(`--cfg ${CONTROL_CFG}`),
    c3Control: withCfg(`--cfg ${C3_CONTROL_CFG}`),
  };
}
function cmdBuild() {
  loadManifest();
  requireC3Prerequisites('build');
  verifyIdentities('build');
  const env = envCapture();
  putText('environment.log', env.text);
  const buildStartedAt = now();
  const buildRole = (label, target, features, rustflags, role = target) => cargoBuild({
    wt: ctx.armDirs[label], label, target, features, rustflags,
    kind: role === 'oracle' ? 'build' : 'bench', role,
  });
  const buildArms = {};
  for (const label of ['A','B','C']) {
    const plan = buildRolePlan(label, ctx.rehearsal);
    const rows = {
      iai: buildRole(label, IAI_TARGET, FEATURES, plan.primary),
      controls: buildRole(label, CONTROLS_TARGET, FEATURES, plan.primary),
      c3: buildRole(label, 'r20_prf01_c3_cells_iai', FEATURES, plan.primary),
      trim: buildRole(label, TRIM_TARGET, FEATURES, plan.primary),
      oracle: buildRole(label, ORACLE_TARGET, FEATURES, plan.oracle, 'oracle'),
      control: buildRole(label, IAI_TARGET, FEATURES, plan.control, 'control'),
      c3Control: buildRole(label, 'r20_prf01_c3_cells_iai', FEATURES, plan.c3Control, 'c3Control'),
    };
    assert.notEqual(rows.control.binarySha256, rows.iai.binarySha256,
      `${label}/control binary must differ from primary`);
    assert.notEqual(rows.c3Control.binarySha256, rows.c3.binarySha256,
      `${label}/c3Control binary must differ from primary`);
    buildArms[label] = rows;
  }
  putJson('builds.json', {
    version: 1,
    candidate: ctx.candidate,
    buildStartedAt,
    capturedAt: now(),
    manifestSha256: shaFile(local('manifest.json')),
    environmentSha256: env.environmentSha256,
    arms: buildArms,
  });
  for (const [label, arms] of Object.entries(buildArms)) {
    console.log(`[build] ${label} targets pinned: ${Object.keys(arms).join(',')}`);
  }
}

function cmdOracle() {
  loadManifest();
  readStage('builds.json', b => { ctx.builds = b; });
  requireC3Prerequisites('oracle');
  verifyIdentities('oracle');
  assert.equal(ctx.builds.manifestSha256, shaFile(local('manifest.json')), 'builds.json stage binding mismatch');
  const records = {};
  for (const label of C3.ARMS) {
    const logical=label==='C0'?'C':label[0],arm=logical;
    records[label] = {};
    const names = C3.INVENTORY.filter(n => C3.targetFor(n) !== null);
    for (const name of names) {
      const id = `${label}-${name}-${Date.now()}`;
      const exe = ctx.builds.arms[arm].oracle.exe;
      const run = wslOk(`${exe} --c3-json ${name} ${id}`, `oracle ${label}/${name}`, {timeout:300_000});
      const rec = JSON.parse(run.out.split(/\r?\n/).filter(x=>x.startsWith('{')).pop() ?? 'null');
      assert.ok(rec && rec.cell === name && rec.processId === id, `C3 oracle identity mismatch ${label}/${name}`);
      const cfg = expectedOracleCfg(arm, ctx.rehearsal);
      assert.deepEqual(rec.cfg, cfg, `oracle cfg mismatch ${label}/${name}`);
      assert.deepEqual([...rec.features].sort(), FEATURES.split(' ').sort(), `oracle features mismatch ${label}/${name}`);
      assert.equal(binarySha(exe), ctx.builds.arms[arm].oracle.binarySha256, `oracle binary hash mismatch ${label}/${name}`);
      assert.ok(rec.oracle && typeof rec.oracle === 'object', `C3 oracle fields missing ${label}/${name}`);
      if (rec.startedAtUnixMs !== undefined) rec.startedAt = new Date(rec.startedAtUnixMs).toISOString();
      assert.ok(typeof rec.startedAt === 'string' && !Number.isNaN(Date.parse(rec.startedAt)), 'oracle startedAt missing');
      assert.equal(rec.config?.runId, id, `oracle config runId mismatch ${label}/${name}`);
      validateC3OracleCounters({ oracle: rec.oracle, arm, rehearsal: ctx.rehearsal, needed: BENEFIT.includes(name) || C3.PUBLICATION === name || Object.hasOwn(C3.PRODUCERS, name) || C3.TRIM_CELLS.includes(name), required: C3.C3_ORACLE_COUNTERS, candidateOnly: ['summary_words_visited', 'summary_hint_loads', 'summary_hint_rmw', 'quiescent_summary_missing_bits'] });
      records[label][name] = {
        row: rec,
        binarySha256: ctx.builds.arms[arm].oracle.binarySha256,
        source: sourceOf(ctx.manifest.arches[arm]),
      };
      putJson(`oracle/${label}-${name}.json`, rec);
    }
  }
  const rec = {
    version: 1,
    capturedAt: now(),
    manifestSha256: shaFile(local('manifest.json')),
    buildsSha256: shaFile(local('builds.json')),
    records,
  };
  putJson('oracle.json',rec);
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
  const events = /^events: (.+)$/m.exec(s)?.[1].trim().split(/\s+/);
  assert.ok(events);
  const ir = events.indexOf('Ir');
  assert.ok(ir >= 0);
  const positions = /^positions: (.+)$/m.exec(s)?.[1].trim().split(/\s+/) ?? ['line'];
  const ids = new Map();
  const functions = new Map();
  let current = null;
  let callee = null;
  let pendingCall = false;
  let previous = Array(positions.length).fill(null);
  const symbol = value => {
    const match = /^(?:\((\d+)\)\s*)?(.*)$/.exec(value);
    if (match[2]) return match[2];
    assert.ok(ids.has(match[1]), 'unknown compressed function ID');
    return ids.get(match[1]);
  };
  const fn = name => {
    if (!functions.has(name)) {
      functions.set(name, { selfIr: 0n, edges: new Map(), edgeCalls: new Map() });
    }
    return functions.get(name);
  };
  for (const line of s.split(/\r?\n/)) {
    const match = /^(?:c?fn)=\((\d+)\)\s+(.+)$/.exec(line);
    if (match) ids.set(match[1], match[2]);
  }
  for (const line of s.split(/\r?\n/)) {
    if (line.startsWith('fn=')) {
      current = symbol(line.slice(3));
      fn(current);
      callee = null;
      previous = Array(positions.length).fill(null);
    }
    else if (line.startsWith('cfn=')) { callee = symbol(line.slice(4)); fn(callee); }
    else if (line.startsWith('calls=')) {
      assert.ok(current && callee && !pendingCall, 'invalid calls record');
      const fields = line.slice(6).trim().split(/\s+/);
      positions.forEach((_, i) => positionToken(fields[i + 1], previous[i]));
      pendingCall = true;
      const calls = fn(current).edgeCalls;
      calls.set(callee, (calls.get(callee) ?? 0n) + naturalBig(fields[0]));
    } else if (/^[*+\-\d]/.test(line)) {
      assert.ok(current, 'cost without function');
      const fields = line.trim().split(/\s+/);
      previous = positions.map((_, i) => positionToken(fields[i], previous[i]));
      const costs = fields.slice(positions.length).map(naturalBig), count = costs[ir] ?? 0n, f = fn(current);
      if (pendingCall) {
        f.edges.set(callee, (f.edges.get(callee) ?? 0n) + count);
        pendingCall = false;
      } else {
        f.selfIr += count;
      }
    } else if (line && !/^(?:#|[A-Za-z][A-Za-z_]*(?:=|:))/.test(line)) {
        throw Error('malformed Callgrind record');
      }
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
function perfGateIndex(bytes, groupName, producerName) {
  const s=bytes.toString();
  const pattern = `library_benchmark_group!\\(\\s*name\\s*=\\s*${groupName};\\s*`
    + `benchmarks\\s*=([\\s\\S]*?)\\);`;
  const group = new RegExp(pattern).exec(s);
  assert.ok(group, `${groupName} group missing`);
  const all=group[1].split(',').map(x=>x.trim()).filter(Boolean);
  const index=all.indexOf(producerName); assert.ok(index>=0,`${producerName} absent from ${groupName}`);
  return index;
}
function generatedArguments(debug, log, homeDirName, cellIndex, producerSymbol, groupName) {
  const commands = [...log.matchAll(/^==\d+==\s+Command:\s*(.+)$/gm)].map(m => m[1]);
  assert.equal(new Set(commands).size, 1, 'ambiguous measured argv');
  assert.ok(commands.length);
  const measured = commands[0];
  const records = [...debug.matchAll(/callgrind: Arguments: (.+)$/gm)].map(m => m[1]);
  const candidates = records.filter(argument =>
    argument.includes('--callgrind-out-file=')
    // v2 shape: the cell's own iai path segment; the observation home name also contains it.
    && argument.includes('/' + homeDirName + '/')
    && argument.includes('--toggle-collect=' + producerSymbol));
  assert.equal(candidates.length, 1, 'one actual runner argument record required for the producer cell');
  const tool = candidates[0];
  assert.ok(tool.includes('--separate-threads=yes'), 'separate_threads not captured');
  const toolVector = tool.trim().split(/\s+(?=--[A-Za-z])/).map(x => x.replace(/^"(.*)"$/, '$1'));
  for (const setting of PRODUCER_SETTINGS) {
    assert.equal(
      toolVector.filter(argument => argument.split('=')[0] === setting.split('=')[0]).length,
      1,
      'missing/duplicate producer setting ' + setting,
    );
    assert.ok(toolVector.includes(setting), 'wrong producer setting ' + setting);
  }
  assert.equal(toolVector.filter(x => x.startsWith('--toggle-collect=')).length, 1);
  assert.ok(toolVector.includes('--toggle-collect=' + producerSymbol));
  assert.ok(!tool.includes('*::__iai_callgrind_wrapper_mod::*'), 'default toggle forbidden');
  const dispatch = argvTokens(measured);
  assert.equal(dispatch.indexOf('--iai-run'), 1, 'measured dispatcher missing');
  assert.deepEqual(dispatch.slice(2, 5), [groupName, String(cellIndex), '0'], 'measured dispatcher index mismatch');
  return {
    toolArguments: tool,
    measuredArguments: measured,
    generatedArguments: 'valgrind ' + tool + ' ' + measured,
    toolVector,
  };
}
function parseProfiles(profiles, log, { cellIndex, producerSymbol, groupName, cellName, debug = '', expectedExecutable } = {}) {
  const pids = [...new Set([...log.matchAll(/^==(\d+)==/gm)].map(m => Number(m[1])))];
  assert.equal(pids.length, 1, 'actual valgrind PID missing/ambiguous'); assert.ok(pids[0] > 0);
  const args = generatedArguments(debug, log, cellName, cellIndex, producerSymbol, groupName);
  const measured = argvTokens(args.measuredArguments);
  if (expectedExecutable) assert.equal(measured[0], expectedExecutable, 'measured binary mismatch');
  let totalIr = 0n, childIr = 0n, boundaryExecutions = 0n; const threads = new Set();
  for (const s of profiles) {
    const header = key => {
      const matches = [...s.matchAll(new RegExp('^' + key + ': (.+)$', 'gm'))];
      assert.equal(matches.length, 1, 'ambiguous/missing ' + key);
      return matches[0][1];
    };
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
      const exact = functions.get(producerSymbol); assert.ok(exact, 'exact producer boundary absent');
      const inclusive = exact.selfIr + [...exact.edges.values()].reduce((a, b) => a + b, 0n);
      assert.ok(inclusive > 0n, 'exact producer boundary empty');
      assert.equal(inclusive, vals[index], 'exact inclusive/child total mismatch');
      childIr += vals[index];
      let incoming = 0n;
      for (const [caller, functionData] of functions) {
        const count = functionData.edgeCalls.get(producerSymbol) ?? 0n;
        if (count > 0n) {
          assert.match(caller,
            /std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure|r20_prf01_c3_cells_iai.*closure/,
            'boundary incoming caller not child context');
          incoming += count;
        }
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
  return {
    pid: pids[0],
    totalIr: safeNumber(totalIr),
    childIr: safeNumber(childIr),
    boundaryExecutions: safeNumber(boundaryExecutions),
    ...args,
  };
}
function machineBoundary(nm, assembly, control, producerSymbol, callerPattern) {
  const definitions = nm.split(/\r?\n/).filter(l => l.endsWith(' ' + producerSymbol));
  assert.equal(definitions.length, 1, 'exact boundary symbol missing/ambiguous');
  const blocks = [...assembly.matchAll(/^([0-9a-f]+) <(.+)>:\n([\s\S]*?)(?=^[0-9a-f]+ <|$(?![\s\S]))/gm)].map(m => ({ name: m[2], body: m[3].trimEnd() + '\n' }));
  const exact = blocks.filter(block => block.name === producerSymbol);
  assert.equal(exact.length, 1, 'exact disassembly definition absent');
  const callers = blocks.filter(block => callerPattern.test(block.name)
    && block.body.split('\n').some(line =>
      /\b(?:callq?|jmpq?)\s/.test(line) && line.includes('<' + producerSymbol + '>')));
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
  return { symbol: producerSymbol, controlEmpty: control, childCallers: callers.length };
}
function boundaryEvidence(b, tag, producerSymbol, callerPattern) {
  const nm = wslOk(`nm -C --defined-only ${b.exe}`, `nm @${b.arm}/${tag}`, { timeout: 120_000 });
  const disasm = wslOk(`objdump -Cd --no-show-raw-insn ${b.exe}`, `objdump @${b.arm}/${tag}`, { timeout: 120_000 });
  const nmReceipt = putText(`boundary/${tag}-nm.log`, nm.out);
  const disassemblyReceipt = putText(`boundary/${tag}-disassembly.log`, disasm.out);
  const verification = machineBoundary(
    nm.out, disasm.out, b.role === 'control', producerSymbol, callerPattern,
  );
  return {
    binarySha256: b.binarySha256,
    role: b.role,
    tag,
    verification,
    nm: nmReceipt,
    disassembly: disassemblyReceipt,
  };
}
function producerPass(label, arm, role, cellIndex, name, bKey) {
  const spec=PRODUCER_SPECS[name];
  const b={...ctx.builds.arms[arm][bKey],arm,role};
  // The C3 cells' producers spawn from r20_prf01_c3_cells_iai::gate::run_remote's closure.
  const callerPattern = spec.target === 'r20_prf01_c3_cells_iai'
    ? /std::thread.*(?:closure|spawn)|r20_prf01_c3_cells_iai.*closure/
    : /std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure/;
  assert.ok(boundaryEvidence(
    b,
    `${label}-${name}-${role}`,
    spec.symbol,
    callerPattern,
  ).verification, 'boundary precheck failed');
  const home = `target/prf01-${label}-${name}-${role}-obs-${Date.now()}`;
  assert.ok(
    !fs.existsSync(join(armDir(arm), home)),
    'observation home already exists',
  );
  const script = [
    'set -e',
    'export PATH="$HOME/.cargo/bin:$PATH"',
    `cd ${toHost(armDir(arm))}`,
    `IAI_CALLGRIND_LOG=debug IAI_CALLGRIND_COLOR=never ${b.exe} --home ${toHost(join(armDir(arm), home))} 2>&1`,
  ].join(' && ');
  const startedAt = now();
  const r = wslOk(script, `${role} run ${label}`, { timeout: 3_600_000 });
  const homeDir = join(armDir(arm), home);
  const files = [];
  const walk = dir => {
    for (const name of fs.readdirSync(dir)) {
      const path = join(dir, name);
      if (fs.statSync(path).isDirectory()) walk(path);
      else files.push(path);
    }
  };
  walk(homeDir);
  // Match below the observation home: the home directory name itself contains the cell name.
  const own = files.filter(p => relative(homeDir, p).replaceAll('\\', '/').includes(name));
  const profiles = own.filter(p => /\.out(?:[.#-]+\d+)*$/.test(p) && fs.statSync(p).size > 0);
  const logs = own.filter(p => /\.log(?:[.#-]+\d+)*$/.test(p));
  assert.ok(profiles.length && logs.length, `${role} ${label}: producer profiles/logs missing`);
  const groupName = name === PRODUCER
    ? 'perf_gate'
    : name === C3.LARGE_REMOTE_FREE || name === C3.DISTINCT_WORD_PRODUCER
      ? 'r20_prf01_c3_cells'
      : 'r20_prf01_trim';
  const parsed = parseProfiles(
    profiles.map(path => publicText(fs.readFileSync(path, 'utf8'))),
    publicText(logs.map(path => fs.readFileSync(path, 'utf8')).join('\n')),
    {
      cellIndex, producerSymbol: spec.symbol, groupName, cellName: name,
      debug: publicText(r.out), expectedExecutable: publicText(b.exe),
    },
  );
  assert.equal(binarySha(b.exe), b.binarySha256, `binary drift @${label}/${role}`);
  // Receipts are keyed per producer cell: one label runs several producers.
  const tag = `${label}-${name}-${role}`;
  profiles.forEach((p, i) => putText(`profiles/${tag}-${i}.out`, fs.readFileSync(p, 'utf8')));
  putJson(`profiles/${tag}-inventory.json`, profiles.map((p, i) =>
    ({ receipt: `profiles/${tag}-${i}.out`, sha256: sha(publicText(fs.readFileSync(p, 'utf8'))) })));
  putText(`raw/${tag}.log`, r.out);
  fs.rmSync(homeDir, { recursive: true, force: true });
  return { ...parsed, startedAt, binarySha256: b.binarySha256, log: putText(`logs/${tag}.log`, r.out) };
}

// --- measure -----------------------------------------------------------------

// A parse miss must leave the raw output behind: the stage dir is uploaded as a CI artifact.
function checkCells(ir, cellNames, out, what) {
  const got = Object.keys(ir).sort(), want = [...cellNames].sort();
  if (JSON.stringify(got) === JSON.stringify(want)) return;
  putText(`logs/${what}.unparsed.log`, out.slice(0, RAW_LOG_LIMIT));
  const head = out.split(/\r?\n/).slice(0, 60).join('\n');
  assert.fail(`${what}: parsed ${got.length}/${want.length} cells; missing: ${want.filter(c => !(c in ir)).join(',')}\n--- output head ---\n${head}`);
}
function benchRun(run, arm, target, cellNames, key) {
  const b = { ...ctx.builds.arms[arm][key], arm, role: key };
  const home = `target/prf01-${run}-${key}-obs-${Date.now()}`;
  assert.ok(!fs.existsSync(join(armDir(arm), home)), 'observation home already exists');
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"', `cd ${toHost(armDir(arm))}`,
    `IAI_CALLGRIND_LOG=debug IAI_CALLGRIND_COLOR=never ${b.exe} --home `
      + `${toHost(join(armDir(arm), home))} 2>&1`,
  ].join(' && ');
  const startedAt = now();
  const r = wslOk(script, `bench ${target} run ${run}`, { timeout: 3_600_000 });
  const ir = parseIr(r.out, cellNames);
  checkCells(ir, cellNames, r.out, `${run}-${key}`);
  // Store the (redacted) log FIRST, then enforce the tier ceiling: evidence survives the abort.
  const log = putText(`logs/${run}-${key}.log`, r.out);
  assert.ok(Buffer.byteLength(r.out) < RAW_LOG_LIMIT, `raw log ${run}/${target} is ${Buffer.byteLength(r.out)} bytes, over ${RAW_LOG_LIMIT}; the full redacted text is retained at ${log.path}`);
  const pids = measuredPids(join(armDir(arm), home), cellNames, b.exe);
  fs.rmSync(join(armDir(arm), home), { recursive: true, force: true });
  return { run, arm, startedAt, ir, log, pids };
}
function controlsRun(run, arm, cellNames) {
  return benchRun(run,arm,CONTROLS_TARGET,cellNames,'controls');
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

function measureToInput(runs, producerRuns) {
  return buildMeasureInput(runs, producerRuns, {
    ctx, C3, shaFile, REPO, sourceOf, ORACLE_CFG, SUMMARY_ORACLE_CFG, PRODUCER_SPECS, local, putJson, putText,
    assert, fs, join,
  });
}
function cmdMeasure() {
  loadManifest();
  readStage('builds.json', b => { ctx.builds = b; });
  readStage('oracle.json', o => { ctx.oracle = o; });
  requireC3Prerequisites('measure');
  verifyIdentities('measure');
  assert.equal(ctx.builds.manifestSha256,shaFile(local('manifest.json')),'build stage binding mismatch');
  assert.equal(ctx.oracle.manifestSha256,shaFile(local('manifest.json')),'oracle manifest binding mismatch');
  assert.equal(ctx.oracle.buildsSha256,shaFile(local('builds.json')),'oracle build binding mismatch');
  const indices = Object.fromEntries(Object.keys(PRODUCER_SPECS).map(name => {
    const group = name === PRODUCER
      ? 'perf_gate'
      : name === C3.DISTINCT_WORD_PRODUCER || name === C3.LARGE_REMOTE_FREE
        ? 'r20_prf01_c3_cells'
        : 'r20_prf01_trim';
    const file = group === 'perf_gate'
      ? 'perf_gate_iai.rs'
      : group === 'r20_prf01_c3_cells'
        ? 'r20_prf01_c3_cells_iai.rs'
        : 'r20_prf01_trim_iai.rs';
    const sourcePath = join(armDir('A'), 'benches', file);
    return [name, perfGateIndex(fs.readFileSync(sourcePath), group, name)];
  }));
  const rows=[];
  for(const label of C3.ARMS){
    const logical=label==='C0'?'C':label[0],arm=logical;
    const cells=[];
    const batches = [
      [IAI_TARGET, IAI_CELLS, 'iai'],
      [CONTROLS_TARGET, CONTROLS_CELLS, 'controls'],
      ['r20_prf01_c3_cells_iai', C3_CELLS, 'c3'],
      [TRIM_TARGET, TRIM_CELLS, 'trim'],
    ];
    for (const [target, names, key] of batches) {
      const batch = benchRun(label, arm, target, new Set(names), key);
      batch.key = key;
      batch.cells = names;
      cells.push(batch);
    }
    rows.push({label,arm:logical,cells});
  }
  const producerRuns=[];
  for(const label of C3.ARMS){
    const logical=label==='C0'?'C':label[0],arm=logical;
    for(const name of Object.keys(PRODUCER_SPECS)){
      const key=name===PRODUCER?'iai':'c3',idx=indices[name];
      producerRuns.push({label,arm,name,
        primary:producerPass(label,arm,'primary',idx,name,key),
        control:producerPass(label,arm,'control',idx,name,name===PRODUCER?'control':'c3Control')});
    }
  }
  for (const arm of ['A', 'B', 'C']) {
    const roles = ['iai', 'controls', 'c3', 'trim', 'oracle', 'control', 'c3Control'];
    for (const key of roles) {
      const build = ctx.builds.arms[arm][key];
      assert.equal(binarySha(build.exe), build.binarySha256, `binary drift ${arm}/${key}`);
    }
  }
  const input = measureToInput(rows, producerRuns);
  const judge = spawnSync(process.execPath, [join(REPO, JUDGE), '--input', local('input.json')], {
    encoding: 'utf8',
    maxBuffer: 1 << 24,
  });
  const judgeOutput = `${judge.stdout ?? ''}${judge.stderr ?? ''}`;
  const resultText = ctx.rehearsal
    ? renderRehearsalVerdict(`judge-exit=${judge.status}`, judgeOutput)
    : `exit=${judge.status}\n${judgeOutput}`;
  putText('verdict.txt', resultText);
  process.stdout.write(ctx.rehearsal ? `${resultText}\n` : judge.stdout ?? '');
  if (!ctx.rehearsal) {
    if (judge.stderr) process.stderr.write(judge.stderr);
    process.exitCode = judge.status === 0 ? 0 : 1;
  } else process.exitCode = rehearsalExitCode();
}

// --- main --------------------------------------------------------------------

function stagePaths() {
  const out = parseArg('--out-dir');
  return resolve(REPO, out ?? 'docs/perf/R20_PRF01_C3_RUN01');
}

function rehearsalExitCode() { return 0; }
function rehearsalVerdict(status, output = '') {
  const body = String(output).replace(/^REHEARSAL:\s*/i, '').trim();
  return `REHEARSAL: ${status}${body ? `\n${body}` : ''}`;
}
function renderRehearsalVerdict(status, stdout = '', stderr = '') {
  return rehearsalVerdict(status, `${stdout}${stderr}`);
}
function rehearsal() {
  ctx.rehearsal = true;
  ctx.outDir = stagePaths();
  try {
    cmdPrepare();
    cmdBuild();
    cmdOracle();
    cmdMeasure();
  } catch (error) {
    const message = `REHEARSAL: INCOMPLETE — ${error.message}`;
    putText('REHEARSAL.txt', renderRehearsalVerdict('INCOMPLETE', message));
    console.error(message);
  } finally {
    process.exitCode = rehearsalExitCode();
  }
}
function cmdCleanup() {
  assert.ok(RUN_ROOT === resolve(REPO, 'worktrees/r20p1-c3'), 'cleanup refuses custom --arm-root');
  const manifestPath = resolve(stagePaths(), 'manifest.json');
  assert.ok(fs.existsSync(manifestPath), `cleanup requires ownership manifest: ${manifestPath}`);
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  assert.deepEqual(manifest.ownedWorktrees, ['A','B','C'], 'cleanup refuses missing or invalid ownership record');
  assert.equal(manifest.wtDir, '<repo>/worktrees/r20p1-c3', 'cleanup refuses unowned worktree root');
  const expected = ['A','B','C'].map(armDir);
  assert.deepEqual(
    manifest.ownedWorktrees,
    expected.map((_, index) => ['A', 'B', 'C'][index]),
    'cleanup ownership record mismatch',
  );
  for (const wt of expected) {
    if (fs.existsSync(wt)) git(REPO, ['worktree', 'remove', '--force', wt]);
  }
  try { fs.rmdirSync(RUN_ROOT); } catch { /* non-empty or absent */ }
  console.log('[cleanup] owned A/B/C worktrees removed');
}

function main() {
  const modes = process.argv.filter(argument => [
    '--prepare', '--build', '--oracle', '--measure', '--rehearsal', '--cleanup', '--selftest',
  ].includes(argument));
  assert.equal(modes.length, 1, 'select exactly one runner stage');
  const mode = modes[0];
  if (mode === '--selftest') return runnerSelftest();
  if (mode === '--cleanup') { ctx.outDir = stagePaths(); return cmdCleanup(); }
  if (mode === '--rehearsal') return rehearsal();
  ctx.outDir = stagePaths();
  try {
    if (mode === '--prepare') return cmdPrepare();
    if (mode === '--build') return cmdBuild();
    if (mode === '--oracle') return cmdOracle();
    return cmdMeasure();
  } catch (error) { console.error(`[${mode}] ${error.message}`); process.exitCode = 1; }
}
function runnerSelftest() {
  selfTestC3OracleCounters();
  selfTestMergeHarnessCargo();
  const actualCargo = fs.readFileSync(join(REPO, 'Cargo.toml'), 'utf8');
  assert.equal(mergeHarnessCargo(actualCargo, actualCargo), actualCargo);
  return C3.runRunnerSelftest({
    sourceIdentity, sha, parseProfiles, producerSettings: PRODUCER_SETTINGS, worktreeAddArgs,
    buildRolePlan, expectedOracleCfg,
    cleanupOwnership, rehearsalRustflags, rehearsalExitCode, rehearsalVerdict,
    distinguishRustflags: DISTINGUISH_RUSTFLAGS,
  });
}
main();
