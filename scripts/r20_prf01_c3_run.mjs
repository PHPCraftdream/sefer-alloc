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

const RUNNER_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(RUNNER_DIR, '..');
const parseArg = name => { const i=process.argv.indexOf(name); return i<0?null:process.argv[i+1]; };
const RAW_LOG_LIMIT = 2 * 1024 * 1024;
const PRODUCER_SETTINGS = ['--tool=callgrind', '--error-exitcode=0', '--trace-children=yes', '--fair-sched=try',
  '--I1=32768,8,64', '--D1=32768,8,64', '--LL=8388608,16,64', '--cache-sim=yes', '--compress-strings=no',
  '--compress-pos=no', '--dump-line=yes', '--dump-instr=no', '--combine-dumps=no', '--separate-threads=yes'];
const sha = b => createHash('sha256').update(b).digest('hex');
const shaFile = p => sha(fs.readFileSync(p));
const now = () => new Date().toISOString();
const IS_WIN = process.platform === 'win32';
const winToWsl = p => { const m=/^([A-Za-z]):(.*)$/.exec(p); assert.ok(m,`not an absolute Windows path: ${p}`); return `/mnt/${m[1].toLowerCase()}${m[2].replace(/\\/g,'/')}`; };
const toHost = p => IS_WIN ? winToWsl(p) : p;
if (parseArg('--candidate') === 'TBD') throw new Error('--candidate TBD is not permitted');
const ARM_ROOT = parseArg('--arm-root') ? resolve(parseArg('--arm-root')) : null;
const armDir = arm => { assert.ok(ARM_ROOT, '--arm-root is required'); return join(ARM_ROOT,arm); };
const REPO_ROOT = REPO;
const WT_DIR = ARM_ROOT ?? join(REPO_ROOT,'worktrees','r20p1');
const JUDGE = 'scripts/r20_prf01_c3_gate.mjs';
const PREREG = 'docs/perf/R20_PRF01_C3_DEFAULT_PREREG.md';
const IAI_TARGET = 'perf_gate_iai';
const CONTROLS_TARGET = 'r20_prf01_controls_iai';
const ORACLE_TARGET = 'r20_prf01_c3_oracle';
const TRIM_TARGET = C3.TRIM_TARGET;
const FEATURES = C3.FEATURES.join(' ');
const ORACLE_CFG = 'r19_sidecar_summary_oracle';
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
const TRIM_CELLS = C3.INVENTORY.filter(n => C3.targetFor(n) === 'trim');
const HOT_DECL = { [CONSTRUCTION]: [1, 16, 8],
  ...Object.fromEntries(HOT_FAMILIES.flatMap(f => [['n', 64], ['2n', 128], ['4n', 256]].map(([s, ops]) =>
    [`r20_prf01_${f}_${s}`, [ops, f === 'small_churn_16b' ? 16 : f === 'aligned_churn_640b_a128' ? 640 : 256, f === 'aligned_churn_640b_a128' ? 128 : 8]]))) };
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
  assert.equal(r.code, 0, `${what} failed (exit ${r.code})\n--- stderr tail ---\n${r.err.split('\n').slice(-40).join('\n')}`);
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
function readFrozenManifest() {
  const p=parseArg('--frozen-manifest'); assert.ok(p,'--frozen-manifest JSON is required');
  const value=JSON.parse(fs.readFileSync(resolve(REPO,p),'utf8'));
  assert.ok(Array.isArray(value)&&value.length,'frozen manifest must be a non-empty [{path,sha256}] array');
  return value;
}
function frozenHashes() {
  const expected=readFrozenManifest();
  const check=spawnSync(process.execPath,[join(REPO,JUDGE),'--check-hashes',resolve(REPO,parseArg('--frozen-manifest'))],{encoding:'utf8'});
  assert.equal(check.status,0,`frozen hash drift or snapshot mismatch\n${check.stdout}${check.stderr}`);
  const list=spawnSync(process.execPath,[join(REPO,JUDGE),'--hashes'],{encoding:'utf8'});
  assert.equal(list.status,0,'gate --hashes failed');
  const actual=list.stdout.trim().split('\n').map(l=>{const [sha256,path]=l.split(/\\s+/);return {path,sha256};});
  assert.deepEqual(actual,expected,'live frozen hashes differ from operator snapshot');
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
  assert.deepEqual(hashes,C3.FROZEN_FILES.map(path=>({path,sha256:shaFile(join(REPO,path))})),'live frozen file drift at '+stage);
  for(const label of ['A','B','C']){
    const live=archIdentity(label,ctx.armDirs[label]),got=m.arches[label];
    assert.ok(got,`manifest has no arm ${label} at ${stage}`);
    for(const key of ['head','tree','patchSha256','untrackedSha256','identitySha256'])assert.equal(got[key],live[key],`arm ${label} identity drift at ${stage}`);
  }
  assert.equal(m.candidate,ctx.candidate);
}

function baseHead() {
  return git(REPO, ['rev-parse', 'HEAD']).out;
}
function harnessOverlay(dir) {
  const status=git(dir,['status','--porcelain','-z']).out;
  const changes=[];
  for(const entry of status.split('\0').filter(Boolean)) {
    const path=entry.slice(3); assert.ok(C3.FROZEN_FILES.includes(path),`non-frozen overlay path forbidden: ${path}`);
    const bytes=fs.readFileSync(join(dir,path)); changes.push({path,sha256:sha(bytes),base64:bytes.toString('base64'),untracked:entry.startsWith('?? ')});
  }
  return changes.sort((a,b)=>a.path.localeCompare(b.path));
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
  const head=git(dir,['rev-parse','HEAD']).out,tree=git(dir,['rev-parse','HEAD^{tree}']).out;
  assert.match(head,/^[0-9a-f]{40}$/); assert.match(tree,/^[0-9a-f]{40}$/);
  const overlay=harnessOverlay(dir), identity=sourceIdentity(head,tree,overlay);
  return {label,wt:`<repo>/arms/${label}`,head,tree,...identity,overlay,cargoLockSha256:shaFile(join(dir,'Cargo.lock')),capturedAt:now()};
}
const sourceOf = a => (({ baseHead, tree, patchSha256, untrackedSha256, identitySha256 }) =>
  ({ baseHead, tree, patchSha256, untrackedSha256, identitySha256 }))(a);
function parseDispatch() {
  const rawCandidate=parseArg('--candidate');
  const candidate=rawCandidate ? git(REPO,['rev-parse',`${rawCandidate}^{commit}`]).out : null;
  assert.ok(candidate&&/^[0-9a-f]{40}$/.test(candidate),'--candidate must resolve to an immutable commit SHA; TBD rejected');
  const armDirs=Object.fromEntries(['A','B','C'].map(x=>[x,armDir(x)]));
  assert.ok(ARM_ROOT&&['A','B','C'].every(x=>fs.existsSync(armDirs[x])),'--arm-root must contain caller-provisioned A/B/C directories');
  return {candidate,armDirs};
}
function cmdPrepare() {
  Object.assign(ctx,parseDispatch());
  ctx.outDir=stagePaths();
  assert.ok(!fs.existsSync(local('manifest.json')),`manifest exists, refusing to overwrite: ${local('manifest.json')}`);
  requireC3Prerequisites('prepare');
  fs.mkdirSync(ctx.outDir,{recursive:true});
  const base=git(REPO,['merge-base',ctx.candidate,'origin/main']).out;
  assert.equal(git(ctx.armDirs.A,['rev-parse','HEAD']).out,base,'arm A must use merge-base commit');
  assert.equal(git(ctx.armDirs.B,['rev-parse','HEAD']).out,ctx.candidate,'arm B must use candidate commit');
  assert.equal(git(ctx.armDirs.C,['rev-parse','HEAD']).out,base,'arm C must use merge-base commit');
  const arches=Object.fromEntries(['A','B','C'].map(x=>[x,archIdentity(x,ctx.armDirs[x])]));
  assert.deepEqual(sourceOf(arches.C),sourceOf(arches.A),'C0 source overlay must exactly match A');
  const frozen=frozenHashes();
  const manifest={version:1,candidate:ctx.candidate,capturedAt:now(),baseHead:base,frozen,arches,pathLengths:Object.values(ctx.armDirs).map(p=>p.length)};
  assert.equal(new Set(manifest.pathLengths).size,1,'A/B/C full path lengths must be equal');
  putJson('manifest.json',manifest);
  console.log(`[prepare] candidate=${ctx.candidate} base=${base}`);
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
  const td = `/tmp/r20p1/${label}-${target}-${role}-${kind}-${createHash('sha1').update(rustflags ?? 'primary').digest('hex').slice(0,10)}`;
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
function requireC3Prerequisites(stage) {
  const missing = C3.validateSources().filter(x => x.status !== 'ok');
  const expected = C3.FROZEN_FILES.filter(p => !fs.existsSync(join(REPO,p)));
  const all=[...missing.map(x=>`${x.status}: ${x.file} (${x.name})`),...expected.map(x=>`frozen source missing: ${x}`)];
  assert.equal(all.length, 0, `fail closed at ${stage}; external prerequisites missing: ${all.join('; ')}`);
}
function loadManifest() {
  readStage('manifest.json', m => { ctx.manifest=m; ctx.candidate=m.candidate; });
  ctx.armDirs=Object.fromEntries(['A','B','C'].map(x=>[x,armDir(x)]));
  assert.equal(git(ctx.armDirs.A,['rev-parse','HEAD']).out,ctx.manifest.baseHead,'manifest A dir must match base SHA');
  assert.equal(git(ctx.armDirs.B,['rev-parse','HEAD']).out,ctx.candidate,'manifest B dir must match candidate SHA');
  assert.equal(git(ctx.armDirs.C,['rev-parse','HEAD']).out,ctx.manifest.baseHead,'manifest C dir must match base SHA');
  assert.ok(ctx.candidate && /^[0-9a-f]{40}$/.test(ctx.candidate), 'manifest candidate SHA missing');
}
function cmdBuild() {
  loadManifest();
  requireC3Prerequisites('build');
  verifyIdentities('build');
  const env = envCapture();
  putText('environment.log', env.text);
  const buildStartedAt = now();
  const buildRole = (label, target, features, rustflags, role=target) => cargoBuild({wt:ctx.armDirs[label],label,target,features,rustflags,kind:role==='oracle'?'build':'bench',role});
  const buildArms = {};
  for (const label of ['A','B','C']) {
    const c3 = label === 'C' ? DISTINGUISH_RUSTFLAGS : undefined;
    const rows = {
      iai: buildRole(label, IAI_TARGET, FEATURES, c3),
      controls: buildRole(label, CONTROLS_TARGET, FEATURES, c3),
      c3: buildRole(label, 'r20_prf01_c3_cells_iai', FEATURES, c3),
      trim: buildRole(label, TRIM_TARGET, FEATURES, c3),
      oracle: buildRole(label, ORACLE_TARGET, FEATURES, `--cfg ${ORACLE_CFG}`, 'oracle'),
      control: buildRole(label, IAI_TARGET, FEATURES, `--cfg ${CONTROL_CFG}`, 'control'),
      c3Control: buildRole(label, 'r20_prf01_c3_cells_iai', FEATURES, `--cfg ${C3_CONTROL_CFG}`, 'c3Control'),
    };
    for (const [key, val] of Object.entries(rows)) if (key !== 'oracle') assert.notEqual(val.binarySha256, rows[key === 'c3Control' ? 'c3' : key].binarySha256, `${label}/${key} control binary must differ`);
    buildArms[label] = rows;
  }
  putJson('builds.json',{version:1,candidate:ctx.candidate,buildStartedAt,capturedAt:now(),manifestSha256:shaFile(local('manifest.json')),environmentSha256:env.environmentSha256,arms:buildArms});
  for(const [label,a] of Object.entries(buildArms)) console.log(`[build] ${label} targets pinned: ${Object.keys(a).join(',')}`);
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
      assert.ok(rec.oracle && typeof rec.oracle === 'object', `C3 oracle fields missing ${label}/${name}`);
      for (const key of C3.C3_ORACLE_COUNTERS) if (rec.oracle[key] !== null && rec.oracle[key] !== undefined) assert.ok(Number.isSafeInteger(rec.oracle[key]) && rec.oracle[key] >= 0, `invalid ${key}`);
      records[label][name] = {row:rec,binarySha256:ctx.builds.arms[arm].oracle.binarySha256,source:sourceOf(ctx.manifest.arches[arm])};
      putJson(`oracle/${label}-${name}.json`, rec);
    }
  }
  const rec={version:1,capturedAt:now(),manifestSha256:shaFile(local('manifest.json')),buildsSha256:shaFile(local('builds.json')),records};
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
function perfGateIndex(bytes, groupName, producerName) {
  const s=bytes.toString();
  const group = new RegExp(`library_benchmark_group!\\(\\s*name\\s*=\\s*${groupName};\\s*benchmarks\\s*=([\\s\\S]*?)\\);`).exec(s);
  assert.ok(group, `${groupName} group missing`);
  const all=group[1].split(',').map(x=>x.trim()).filter(Boolean);
  const index=all.indexOf(producerName); assert.ok(index>=0,`${producerName} absent from ${groupName}`);
  return index;
}
function generatedArguments(debug, log, homeDirName, cellIndex, producerSymbol, groupName) {
  const commands = [...log.matchAll(/^==\d+==\s+Command:\s*(.+)$/gm)].map(m => m[1]);
  assert.equal(new Set(commands).size, 1, 'ambiguous measured argv'); assert.ok(commands.length);
  const measured = commands[0];
  const records = [...debug.matchAll(/callgrind: Arguments: (.+)$/gm)].map(m => m[1]);
  const candidates = records.filter(x => x.includes('--callgrind-out-file=') && x.includes(homeDirName) && x.includes('--toggle-collect=' + producerSymbol));
  assert.equal(candidates.length, 1, 'one actual runner argument record required for the producer cell');
  const tool = candidates[0];
  assert.ok(tool.includes('--separate-threads=yes'), 'separate_threads not captured');
  const toolVector = tool.trim().split(/\s+(?=--[A-Za-z])/).map(x => x.replace(/^"(.*)"$/, '$1'));
  for (const setting of PRODUCER_SETTINGS) {
    assert.equal(toolVector.filter(x => x.split('=')[0] === setting.split('=')[0]).length, 1, 'missing/duplicate producer setting ' + setting);
    assert.ok(toolVector.includes(setting), 'wrong producer setting ' + setting);
  }
  assert.equal(toolVector.filter(x => x.startsWith('--toggle-collect=')).length, 1);
  assert.ok(toolVector.includes('--toggle-collect=' + producerSymbol));
  assert.ok(!tool.includes('*::__iai_callgrind_wrapper_mod::*'), 'default toggle forbidden');
  const dispatch = argvTokens(measured);
  assert.equal(dispatch.indexOf('--iai-run'), 1, 'measured dispatcher missing');
  assert.deepEqual(dispatch.slice(2, 5), [groupName, String(cellIndex), '0'], 'measured dispatcher index mismatch');
  return { toolArguments: tool, measuredArguments: measured, generatedArguments: 'valgrind ' + tool + ' ' + measured, toolVector };
}
function parseProfiles(profiles, log, { cellIndex, producerSymbol, groupName, debug = '', expectedExecutable } = {}) {
  const pids = [...new Set([...log.matchAll(/^==(\d+)==/gm)].map(m => Number(m[1])))];
  assert.equal(pids.length, 1, 'actual valgrind PID missing/ambiguous'); assert.ok(pids[0] > 0);
  const args = generatedArguments(debug, log, 'producer', cellIndex, producerSymbol, groupName);
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
      const exact = functions.get(producerSymbol); assert.ok(exact, 'exact producer boundary absent');
      const inclusive = exact.selfIr + [...exact.edges.values()].reduce((a, b) => a + b, 0n);
      assert.ok(inclusive > 0n, 'exact producer boundary empty');
      assert.equal(inclusive, vals[index], 'exact inclusive/child total mismatch');
      childIr += vals[index];
      let incoming = 0n;
      for (const [caller, f] of functions) {
        const count = f.edgeCalls.get(producerSymbol) ?? 0n;
        if (count > 0n) { assert.match(caller, /std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure|r20_prf01_c3_cells_iai.*closure/, 'boundary incoming caller not child context'); incoming += count; }
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
function machineBoundary(nm, assembly, control, producerSymbol, callerPattern) {
  const definitions = nm.split(/\r?\n/).filter(l => l.endsWith(' ' + producerSymbol));
  assert.equal(definitions.length, 1, 'exact boundary symbol missing/ambiguous');
  const blocks = [...assembly.matchAll(/^([0-9a-f]+) <(.+)>:\n([\s\S]*?)(?=^[0-9a-f]+ <|$(?![\s\S]))/gm)].map(m => ({ name: m[2], body: m[3].trimEnd() + '\n' }));
  const exact = blocks.filter(b => b.name === producerSymbol); assert.equal(exact.length, 1, 'exact disassembly definition absent');
  const callers = blocks.filter(b => callerPattern.test(b.name) && b.body.split('\n').some(l => /\bcallq?\s/.test(l) && l.includes('<' + producerSymbol + '>')));
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
  const verification = machineBoundary(nm.out, disasm.out, b.role === 'control', producerSymbol, callerPattern);
  return { binarySha256: b.binarySha256, role: b.role, tag, verification,
    nm: putText(`boundary/${tag}-nm.log`, nm.out), disassembly: putText(`boundary/${tag}-disassembly.log`, disasm.out) };
}
function producerPass(label, arm, role, cellIndex, name, bKey) {
  const spec=PRODUCER_SPECS[name];
  const b={...ctx.builds.arms[arm][bKey],arm,role};
  const callerPattern=spec.customSymbol?/std::thread.*(?:closure|spawn)|r20_prf01_c3_cells_iai.*closure/:/std::thread.*(?:closure|spawn)|R19Prf01.*remote_free.*closure/;
  assert.ok(boundaryEvidence(b, `${label}-${name}-${role}`, spec.symbol, callerPattern).verification, 'boundary precheck failed');
  const home = `target/prf01-${label}-${name}-${role}-obs-${Date.now()}`;
  assert.ok(!fs.existsSync(join(armDir(arm),home)),'observation home already exists');
  const script = ['set -e','export PATH="$HOME/.cargo/bin:$PATH"',`cd ${toHost(armDir(arm))}`,
    `IAI_CALLGRIND_LOG=debug IAI_CALLGRIND_COLOR=never ${b.exe} --home ${toHost(join(armDir(arm),home))} 2>&1`].join(' && ');
  const startedAt = now();
  const r = wslOk(script, `${role} run ${label}`, { timeout: 3_600_000 });
  const homeDir = join(armDir(arm),home);
  const files = [];
  const walk = dir => { for (const n of fs.readdirSync(dir)) { const p = join(dir, n); if (fs.statSync(p).isDirectory()) walk(p); else files.push(p); } };
  walk(homeDir);
  const own = files.filter(p => p.replaceAll('\\', '/').includes(name));
  const profiles = own.filter(p => /\.out(?:[.#-]+\d+)*$/.test(p) && fs.statSync(p).size > 0);
  const logs = own.filter(p => /\.log(?:[.#-]+\d+)*$/.test(p));
  assert.ok(profiles.length && logs.length, `${role} ${label}: producer profiles/logs missing`);
  const groupName=name===PRODUCER?'perf_gate':name===C3.LARGE_REMOTE_FREE||name===C3.DISTINCT_WORD_PRODUCER?'r20_prf01_c3_cells':'r20_prf01_trim';
  const parsed=parseProfiles(profiles.map(p=>publicText(fs.readFileSync(p,'utf8'))),
    publicText(logs.map(p=>fs.readFileSync(p,'utf8')).join('\n')),
    {cellIndex,producerSymbol:spec.symbol,groupName,debug:publicText(r.out),expectedExecutable:publicText(b.exe)});
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
function benchRun(run, arm, target, cellNames, key) {
  const b = { ...ctx.builds.arms[arm][key], arm, role: key };
  const home = `target/prf01-${run}-${key}-obs-${Date.now()}`;
  assert.ok(!fs.existsSync(join(armDir(arm), home)), 'observation home already exists');
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"', `cd ${toHost(armDir(arm))}`,
    `IAI_CALLGRIND_LOG=debug IAI_CALLGRIND_COLOR=never ${b.exe} --home ${toHost(join(armDir(arm), home))} 2>&1`].join(' && ');
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
  const hashes = C3.FROZEN_FILES.map(path=>({path,sha256:shaFile(join(REPO,path))}));
  const environmentSha256=ctx.builds.environmentSha256;
  const identities={};
  for(const arm of ['A','B'])identities[arm]={label:arm,measurementCommit:ctx.manifest.arches[arm].head,source:sourceOf(ctx.manifest.arches[arm]),environmentSha256,binaries:Object.fromEntries(Object.entries(ctx.builds.arms[arm]).map(([k,v])=>[k,v.binarySha256])),buildStartedAt:ctx.builds.buildStartedAt,buildReceipt:'builds.json',features:C3.FEATURES,primaryCfg:[],oracleCfg:[ORACLE_CFG],toolchain:{rustc:'environment.log',target:'environment.log',profile:'release',flags:'role cfg only'}};
  identities.C0={source:identities.A.source,binarySha256:ctx.builds.arms.C.iai.binarySha256,controlsBinarySha256:ctx.builds.arms.C.controls.binarySha256,c3BinarySha256:ctx.builds.arms.C.c3.binarySha256,trimBinarySha256:ctx.builds.arms.C.trim.binarySha256,c3ControlBinarySha256:ctx.builds.arms.C.c3Control.binarySha256,controlBinarySha256:ctx.builds.arms.C.control.binarySha256,buildStartedAt:ctx.builds.buildStartedAt,buildReceipt:'builds.json'};
  for(const [key,value] of Object.entries({iai:ctx.builds.arms.C.iai.binarySha256,controls:ctx.builds.arms.C.controls.binarySha256,c3:ctx.builds.arms.C.c3.binarySha256,trim:ctx.builds.arms.C.trim.binarySha256,c3Control:ctx.builds.arms.C.c3Control.binarySha256,control:ctx.builds.arms.C.control.binarySha256}))identities.C0[`${key}BinarySha256`]=value;
  identities.C0.source=sourceOf(ctx.manifest.arches.A);
  const runnerVersionMatch=/(\d+[.]\d+[.]\d+)/.exec(fs.readFileSync(local('environment.log'),'utf8').split('\n').find(l=>l.includes('iai-callgrind-runner'))??'');assert.ok(runnerVersionMatch,'runner version absent from environment');const runnerVersion=runnerVersionMatch[1];
  const input={version:1,hashes,environmentSha256,inventoryReceipt:'manifest.json',identityReviewReceipt:'manifest.json; per-stage identity revalidation',environmentReceipt:'environment.log',oracleReceipt:'oracle.json',entry:'SeferAlloc::GlobalAlloc->HeapCore routed',nativeDeferred:{deferred:true,method:'fixed-batch native paired A/B on a quiet bare-metal host; per-process batch times divided by batch size (deferred; not launched)'},inventory:[...C3.INVENTORY],identities,iai:[],sizeOf:{A:null,B:null,receipt:'oracle.json'},ancestry:{result:'A ancestor B',chain:[ctx.manifest.arches.A.head,ctx.manifest.arches.B.head],receipt:'manifest.json'}};
  let runId=0;
  for(const run of runs){const src=sourceOf(ctx.manifest.arches[run.arm]);
    for(const batch of run.cells)for(const name of batch.cells){
      const oracleRec=ctx.oracle.records[run.label]?.[name];
      const raw=oracleRec?.row?.oracle??Object.fromEntries(C3.ALL_COUNTERS.map(k=>[k,null]));
      const binarySha256=ctx.builds.arms[run.arm][batch.key].binarySha256;
      const config=oracleRec?.row?.config??{requested:null,resolved:null,configConflictsDelta:null,processBoundary:'isolated-process',runId:`${run.label}-${name}-${runId}`,pid:batch.pids[name],notApplicableReason:'control cell has no SeferAlloc config oracle'};
      input.iai.push({name,arm:run.label,Ir:batch.ir[name],environmentSha256,binarySha256,source:src,startedAt:batch.startedAt,features:C3.FEATURES,receipt:batch.log.path,config:{...config,pid:batch.pids[name],runId:`${run.label}-${name}-${++runId}`},oracle:{...raw,unobservedReasons:Object.fromEntries(Object.keys(raw).filter(k=>raw[k]===null).map(k=>[k,`not observed for ${run.label}/${name}`])),linkage:oracleRec?{binarySha256:oracleRec.binarySha256,primaryBinarySha256:binarySha256,source:oracleRec.source,features:C3.FEATURES,cfg:[ORACLE_CFG],environmentSha256,receipt:`oracle/${run.label}-${name}.json`,featureExactReceipt:'oracle.json',startedAt:oracleRec.row.startedAt,config:oracleRec.row.config}:null}});
    }
  }
  for(const p of producerRuns){const spec=PRODUCER_SPECS[p.name],row=input.iai.find(r=>r.name===p.name&&r.arm===p.label);assert.ok(row);const primary=p.primary,control=p.control;assert.equal(primary.totalIr,row.Ir,`producer rerun Ir mismatch @${p.label}`);const workload=primary.childIr-control.childIr;assert.ok(workload>0,`empty producer workload @${p.label}`);
    row.producerIrReceipt={binarySha256:row.binarySha256,source:row.source,environmentSha256,features:C3.FEATURES,cfg:[],receipt:primary.log.path,symbolReceipt:`boundary/${p.label}-${p.name}-primary-nm.log`,profileReceipt:`profiles/${p.label}-${p.name}-primary-inventory.json`,symbol:spec.symbol,runnerVersion,valgrindVersion:'environment.log',collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,toggleCollect:spec.symbol,generatedArguments:primary.generatedArguments,aggregation:'sum of all thread profile Ir',childFreeCount:spec.freeCount,childIr:primary.childIr,boundaryOverheadIr:control.childIr,workloadIr:workload,boundaryExecutions:1,totalIr:row.Ir,startedAt:primary.startedAt,absoluteTolerance:p.name===C3.LARGE_REMOTE_FREE?'T_s':null,control:{receipt:control.log.path,workload:'child remote frees removed',source:row.source,binarySha256:control.binarySha256,environmentSha256,childIr:control.childIr,workloadIr:0,childFreeCount:0,boundaryExecutions:1,symbolReceipt:`boundary/${p.label}-${p.name}-control-nm.log`,profileReceipt:`profiles/${p.label}-${p.name}-control-inventory.json`,symbol:spec.symbol,toggleCollect:spec.symbol,cfg:[spec.cfg],features:C3.FEATURES,generatedArguments:control.generatedArguments,runnerVersion,valgrindVersion:'environment.log',collectAtStart:false,defaultToggle:false,entryPoint:'Custom',separateThreads:true,aggregation:'sum of all thread profile Ir',totalIr:control.totalIr,deltaIr:primary.totalIr-control.totalIr,parentSetupUnchanged:true,collectionUnchanged:true}};
  }
  const observedSize=arm=>{const rec=ctx.oracle.records[arm]?.[C3.V2_BENEFIT[0]]?.row;const value=rec?.oracle?.sizeof_small_sidecar??rec?.sizeof_small_sidecar;assert.ok(Number.isSafeInteger(value)&&value>0,`oracle must observe positive sidecar size for ${arm}`);return value;};
  input.sizeOf={A:observedSize('A1'),B:observedSize('B1'),receipt:'oracle.json'};
  const receipts={};const scan=v=>{if(typeof v==='string'&&/^(?:environment[.]log|manifest[.]json|builds[.]json|oracle[.]json|(?:logs|boundary|profiles|oracle)\/)/.test(v)&&!(v in receipts))receipts[v]=shaFile(local(v));else if(Array.isArray(v))v.forEach(scan);else if(v&&typeof v==='object')Object.values(v).forEach(scan);};scan(input);input.receiptHashes=receipts;
  putJson('input.json',input);putText('summary.csv',['target,arm,name,ir,binarySha256',...input.iai.map(r=>`${C3.targetFor(r.name)},${r.arm},${r.name},${r.Ir},${r.binarySha256}`)].join('\n')+'\n');return input;
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
  const indices=Object.fromEntries(Object.keys(PRODUCER_SPECS).map(name=>{const group=name===PRODUCER?'perf_gate':name===C3.DISTINCT_WORD_PRODUCER||name===C3.LARGE_REMOTE_FREE?'r20_prf01_c3_cells':'r20_prf01_trim';const file=group==='perf_gate'?'perf_gate_iai.rs':group==='r20_prf01_c3_cells'?'r20_prf01_c3_cells_iai.rs':'r20_prf01_trim_iai.rs';return[name,perfGateIndex(fs.readFileSync(join(armDir('A'),'benches',file)),group,name)];}));
  const rows=[];
  for(const label of C3.ARMS){
    const logical=label==='C0'?'C':label[0],arm=logical;
    const cells=[];
    for(const [target,names,key] of [[IAI_TARGET,IAI_CELLS,'iai'],[CONTROLS_TARGET,CONTROLS_CELLS,'controls'],['r20_prf01_c3_cells_iai',C3_CELLS,'c3'],[TRIM_TARGET,TRIM_CELLS,'trim']]) {
      const batch=benchRun(label,arm,target,new Set(names),key);
      batch.key=key; batch.cells=names; cells.push(batch);
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
  for(const arm of ['A','B','C'])for(const key of ['iai','controls','c3','trim','oracle','control','c3Control'])assert.equal(binarySha(ctx.builds.arms[arm][key].exe),ctx.builds.arms[arm][key].binarySha256,`binary drift ${arm}/${key}`);
  const input=measureToInput(rows,producerRuns);
  const judge=spawnSync(process.execPath,[join(REPO,JUDGE),'--input',local('input.json')],{encoding:'utf8',maxBuffer:1<<24});
  putText('verdict.txt',`exit=${judge.status}\n${judge.stdout}${judge.stderr}`);process.stdout.write(judge.stdout);if(judge.stderr)process.stderr.write(judge.stderr);process.exitCode=judge.status===0?0:1;
}

// --- main --------------------------------------------------------------------

function stagePaths() {
  const m = ctx.manifest;
  const producerCellIndex = perfGateIndex(fs.readFileSync(join(armDir('A'),'benches/perf_gate_iai.rs')),'perf_gate',PRODUCER);
  const c3ProducerIndexes = Object.fromEntries(C3_CELLS.map(name=>[name,perfGateIndex(fs.readFileSync(join(armDir('A'),'benches/r20_prf01_c3_cells_iai.rs')),'r20_prf01_c3_cells',name)]));
  const trimCellIndexes = Object.fromEntries(TRIM_CELLS.map(name=>[name,perfGateIndex(fs.readFileSync(join(armDir('A'),'benches/r20_prf01_trim_iai.rs')),'r20_prf01_trim',name)]));
  const runs = RUNS.map(label => { const arm=RUN_ARM[label], base=arm==='C'?'A':arm;
    const cells=[];
    for (const [target,names,key] of [[IAI_TARGET,IAI_CELLS,'iai'],[CONTROLS_TARGET,CONTROLS_CELLS,'controls'],['r20_prf01_c3_cells_iai',C3_CELLS,'c3'],[TRIM_TARGET,TRIM_CELLS,'trim']]) {
      const result=benchRun(label,arm,target,new Set(names),key);
      cells.push(result);
    }
    return {label,arm,cells};
  });
  const producerRuns=[];
  for(const label of C3.ARMS){const logical=label==='C0'?'C':label[0],arm=logical;
    for(const name of Object.keys(PRODUCER_SPECS)){const spec=PRODUCER_SPECS[name],index=indexes[name],key=name===PRODUCER?'iai':'c3';
      producerRuns.push({label,arm,name,primary:producerPass(label,arm,'primary',index,name,key),control:producerPass(label,arm,'control',index,name,name===PRODUCER?'control':'c3Control')});
    }
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
  const runnerVersionMatch = /(\d+[.]\d+[.]\d+)/.exec(fs.readFileSync(local('environment.log'),'utf8').split('\n').find(l=>l.includes('iai-callgrind-runner')) ?? '');
  assert.ok(runnerVersionMatch,'runner version absent from captured environment');
  const runnerVersion=runnerVersionMatch[1];
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
  for (const run of C3.ARMS) addReceipt(`profiles/${run}-construction-inventory.json`);
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
function main() {
  const modes=process.argv.filter(a=>['--prepare','--build','--oracle','--measure','--selftest'].includes(a));
  assert.equal(modes.length,1,'select exactly one runner stage');
  const mode=modes[0];
  try {
    if(mode==='--selftest')return runnerSelftest();
    ctx.outDir=stagePaths();
    if(mode==='--prepare')return cmdPrepare();
    if(mode==='--build')return cmdBuild();
    if(mode==='--oracle')return cmdOracle();
    return cmdMeasure();
  } catch(e) { console.error(`[${mode}] ${e.message}`);process.exitCode=1; }
}
function runnerSelftest() {
  const sourceA=sourceIdentity('a'.repeat(40),'b'.repeat(40),[]);
  assert.deepEqual(sourceIdentity('a'.repeat(40),'b'.repeat(40),[]),sourceA,'same-source identity must be label independent');
  assert.notEqual(sourceIdentity('a'.repeat(40),'b'.repeat(40),[{path:C3.FROZEN_FILES[0],sha256:sha('x')}]).identitySha256,sourceA.identitySha256,'overlay mutant must drift');
  const frozenExpected=[{path:'sample',sha256:sha('x')}];
  assert.throws(()=>assert.deepEqual([{path:'sample',sha256:sha('y')}],frozenExpected),'frozen snapshot mutant must reject');
  const log='==123== Command: /tmp/bench --iai-run r20_prf01_c3_cells 2 0';
  const debug='callgrind: Arguments: --callgrind-out-file=producer/x --toggle-collect=exact::same_word '+PRODUCER_SETTINGS.join(' ');
  const fixture='pid: 123\npart: 1\nthread: 1\ncmd: /tmp/bench --iai-run r20_prf01_c3_cells 2 0\nevents: Ir\ntotals: 10\nfn=std::thread::closure\n1 5\ncfn=exact::same_word\ncalls=1 1\n1 5\nfn=exact::same_word\n1 5\n';
  const childFixture='pid: 123\npart: 1\nthread: 2\ncmd: /tmp/bench --iai-run r20_prf01_c3_cells 2 0\nevents: Ir\ntotals: 5\nfn=std::thread::closure\n1 0\ncfn=exact::same_word\ncalls=1 1\n1 5\nfn=exact::same_word\n1 5\n';
  const proof=parseProfiles([fixture,childFixture],log,{cellIndex:2,producerSymbol:'exact::same_word',groupName:'r20_prf01_c3_cells',debug,expectedExecutable:'/tmp/bench'});
  assert.equal(proof.childIr,5); assert.equal(proof.boundaryExecutions,1);
  assert.throws(()=>parseProfiles([fixture.replace('exact::same_word','exact::distinct_word')],log,{cellIndex:2,producerSymbol:'exact::same_word',groupName:'r20_prf01_c3_cells',debug,expectedExecutable:'/tmp/bench'}));
  assert.equal(C3.INVENTORY.length,37); assert.equal(new Set(C3.INVENTORY).size,37);
  assert.throws(()=>assert.ok(/^[0-9a-f]{40}$/.test('TBD')));
  assert.equal(C3.targetFor(C3.LARGE_REMOTE_FREE),'c3');
  assert.equal(C3.targetFor(C3.TRIM_CELLS[0]),'trim');
  for(const name of Object.keys(C3.PRODUCERS))assert.ok(C3.PRODUCERS[name].symbol&&C3.PRODUCERS[name].freeCount>0);
  console.log('C3 runner selftest PASS: synthetic parser boundary exact-symbol proof, distinct-symbol rejection; inventory 37');
}
main();
