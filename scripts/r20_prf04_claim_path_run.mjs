// R20 PRF-04 claim-path gate RUNNER. Executes the preregistered gate pipeline;
// the judge (`r20_prf04_claim_path_gate.mjs`) and the prereg are frozen inputs
// and are never modified here. The frozen file list lives in the judge
// (`--hashes` / `--check-hashes`); this runner is one of those files.
//
// Commands (A = the clean, committed HEAD of the worktree that holds this runner):
//   --prepare   Create worktrees <primary>/worktrees/r20p4/{A,B,C} (equal-length
//               full paths, one-letter labels). B = A + cherry-pick 6951a9d0
//               (committed on a temp branch inside the B worktree). Writes the
//               immutable identity manifest BEFORE any measurement and refuses
//               to overwrite an existing one.
//   --build     WSL builds, per-arm target dirs outside the repo: iai bench for
//               A, B, C and the path-oracle example for A, B; SHA256 of every
//               binary. C is built from the A source with
//               `-C debuginfo=line-tables-only` so its binary is distinct (the
//               judge requires it); the flag changes no code.
//   --oracle    Run the path oracle on A and B for every cell family and size
//               (start, cold, recycled, wave at N/2N/4N); every record must say
//               ok:true and is bound to its arm identity and oracle binary hash.
//   --measure   iai runs A1,A2 (one A binary), B1,B2 (one B binary), C0; parses
//               `Instructions:` per cell, stores sanitised raw logs, builds the
//               judge input exactly per the judge schema, invokes the judge,
//               saves its output and derives summary.csv from the SAME JSON.
//   --rehearsal Full pipeline WITHOUT a real B in the iai stage: the iai arm "B"
//               is a second instance of the A source (label A'). The oracle
//               stage still uses the real B build (path activation only, no
//               instruction counts) and the REHEARSAL record binds it to the A'
//               identity. Output goes ONLY to target/r20p4-rehearsal/ and is
//               marked `REHEARSAL - not gate data`. `--slop` injects +6 % into
//               one cold B1 cell (negative control, expected NO-GO).
//
// Environment: Windows host + WSL2 (valgrind, iai-callgrind-runner in
// ~/.cargo/bin). RUSTC_WRAPPER/CARGO_BUILD_RUSTC_WRAPPER are scrubbed for every
// cargo call and every stored text has private paths replaced.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const RUNNER_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(RUNNER_DIR, '..'); // the worktree holding the frozen docs, judge and this runner
// Primary checkout (parent of the shared .git dir); worktrees/r20p4/<label> live under it.
const REPO_ROOT = resolve(spawnSync('git', ['rev-parse', '--path-format=absolute', '--git-common-dir'],
  { cwd: REPO, encoding: 'utf8' }).stdout.trim(), '..');
const JUDGE = 'scripts/r20_prf04_claim_path_gate.mjs';
const JUDGE_TARGET = 'r20_prf04_claim_iai';
const ORACLE_TARGET = 'r20_prf04_claim_path_oracle';
const IAI_FEATURES = 'production internals';
const ORACLE_FEATURES = 'production bench-internals internals';
const DISTINGUISH_RUSTFLAGS = '-C debuginfo=line-tables-only'; // distinct binary for same-source arms; no code change
const DEFAULT_OUT = 'docs/perf/R20_PRF04_CLAIM_PATH_RUN01';
const REHEARSAL_OUT = 'target/r20p4-rehearsal';
const WT_DIR = join(REPO_ROOT, 'worktrees', 'r20p4'); // worktrees/r20p4/{A,B,C}: equal length by construction
const PICK = '6951a9d0';
const FAMILIES = ['cold', 'recycled', 'wave'];
const SIZES = ['n', '2n', '4n'];
const COUNT = { n: 16, '2n': 32, '4n': 64 };
// Every iai cell: the start construction cell plus three families at three sizes.
const CELLS = [{ family: 'start', size: 'n', name: 'r20_prf04_start_first_claim' },
  ...FAMILIES.flatMap(f => SIZES.map(s => ({ family: f, size: s, name: `r20_prf04_${f}_first_claim_${s}` })))];
const RUNS = ['A1', 'A2', 'B1', 'B2', 'C0'];
const RUN_ARM = { A1: 'A', A2: 'A', B1: 'B', B2: 'B', C0: 'C' };
const RAW_LOG_LIMIT = 200 * 1024;

const sha = b => createHash('sha256').update(b).digest('hex');
const shaFile = p => sha(fs.readFileSync(p));
const now = () => new Date().toISOString();
const winToWsl = p => {
  const m = /^([A-Za-z]):(.*)$/.exec(p);
  assert.ok(m, `not an absolute Windows path: ${p}`);
  return `/mnt/${m[1].toLowerCase()}${m[2].replace(/\\/g, '/')}`;
};
// Private checkout paths never reach a stored artifact (the stored bytes are hashed).
const publicText = t => String(t)
  .replaceAll(winToWsl(REPO_ROOT), '<repo>').replaceAll(REPO_ROOT.replace(/\\/g, '/'), '<repo>')
  .replace(/[A-Za-z]:[\\/][^\s"'<>|)]+/g, '<private>')
  .replace(/\/mnt\/[a-z]\/[^\s"'<>|)]+/g, '<private>')
  .replace(/\/home\/[^\s/"'<>|)]+/g, '<home>');

// --- process helpers ---------------------------------------------------------

function git(cwd, args, { allowFailure = false } = {}) {
  const r = spawnSync('git', ['--no-optional-locks', ...args], { cwd, encoding: 'utf8',
    maxBuffer: 1 << 28, env: { ...process.env, GIT_OPTIONAL_LOCKS: '0' } });
  if (!allowFailure && r.status !== 0) throw new Error(`git ${args[0]} failed: ${r.stderr}`);
  return { code: r.status, out: (r.stdout ?? '').trim(), err: r.stderr ?? '' };
}

// One WSL shell call. spawnSync passes argv straight to CreateProcess, so the
// /mnt/... paths inside the script are never mangled.
function wsl(script, { timeout = 600_000 } = {}) {
  const exe = process.platform === 'win32' ? 'wsl.exe' : 'bash';
  const args = process.platform === 'win32' ? ['bash', '-lc', script] : ['-lc', script];
  const r = spawnSync(exe, args, { encoding: 'utf8', maxBuffer: 1 << 28, timeout,
    env: { ...process.env, MSYS_NO_PATHCONV: '1' } });
  return { code: r.status ?? -1, out: r.stdout ?? '', err: r.stderr ?? '', timedOut: r.error?.code === 'ETIMEDOUT' };
}

function wslOk(script, what, opts) {
  const r = wsl(script, opts);
  assert.equal(r.code, 0, `${what} failed (exit ${r.code})\n--- stderr tail ---\n${r.err.split('\n').slice(-40).join('\n')}`);
  return r;
}

// --- stage / receipt plumbing ------------------------------------------------

const ctx = { outDir: null, rehearsal: false, slop: false, manifest: null, builds: null, oracle: null };
const local = (...p) => join(ctx.outDir, ...p);
function putText(name, text) {
  const out = publicText(text);
  fs.mkdirSync(dirname(local(name)), { recursive: true });
  fs.writeFileSync(local(name), out);
  return { path: name, sha256: sha(out) };
}
function putJson(name, obj) { return putText(name, JSON.stringify(obj, null, 2) + '\n'); }
function readStage(name, fn) {
  const p = local(name);
  assert.ok(fs.existsSync(p), `stage file missing: ${p} (run the earlier subcommand first)`);
  fn(JSON.parse(fs.readFileSync(p, 'utf8')));
}

// The frozen list is the judge's own; the runner only reads and checks it.
function frozenHashes() {
  const check = spawnSync(process.execPath, [join(REPO, JUDGE), '--check-hashes'], { encoding: 'utf8' });
  assert.equal(check.status, 0, `frozen hash drift (a frozen file was edited?)\n${check.stdout}${check.stderr}`);
  const list = spawnSync(process.execPath, [join(REPO, JUDGE), '--hashes'], { encoding: 'utf8' }).stdout;
  return list.trim().split('\n').map(l => { const [sha256, path] = l.split(/\s+/); return { path, sha256 }; });
}

// --- identity ----------------------------------------------------------------

// A is the HEAD of the worktree that holds this runner, the judge and the prereg,
// so the gate tree is one committed, immutable tree. A dirty tree is refused.
function baseHead() {
  assert.equal(git(REPO, ['status', '--porcelain']).out, '', 'runner worktree must be clean and committed');
  return git(REPO, ['rev-parse', 'HEAD']).out;
}

// Identity of one arch BEFORE any measurement. identitySha256 folds in the
// label, so the rehearsal's A' (same worktree, label B) is a distinct identity
// exactly as the judge requires.
function archIdentity(label, wt) {
  const head = git(wt, ['rev-parse', 'HEAD']).out;
  const tree = git(wt, ['write-tree']).out; // clean committed arm required
  assert.match(head, /^[0-9a-f]{40}$/); assert.match(tree, /^[0-9a-f]{40}$/);
  const patch = git(wt, ['diff', '--binary', 'HEAD', '--']).out;
  const untracked = git(wt, ['ls-files', '--others', '--exclude-standard', '-z']).out;
  assert.equal(patch, '', 'tracked drift: arch must be a clean committed party');
  assert.equal(untracked, '', 'untracked drift: arch must be a clean committed party');
  const patchSha256 = sha(patch), untrackedSha256 = sha(JSON.stringify(untracked.split('\0')));
  return { label, wt: `<repo>/worktrees/r20p4/${label}`, head, tree, patchSha256, untrackedSha256,
    identitySha256: sha(JSON.stringify([label, head, patchSha256, untrackedSha256])),
    cargoLockSha256: shaFile(join(wt, 'Cargo.lock')), capturedAt: now() };
}
const sourceOf = a => (({ head, patchSha256, untrackedSha256, identitySha256, capturedAt }) =>
  ({ baseHead: head, patchSha256, untrackedSha256, identitySha256, capturedAt }))(a);

// --- prepare -----------------------------------------------------------------

function ensureWorktree(label, { newBranch = false } = {}) {
  const wt = join(WT_DIR, label);
  const base = baseHead();
  if (fs.existsSync(wt)) {
    assert.equal(git(wt, ['rev-parse', 'HEAD']).out, base, `worktree ${label} is not at ${base}; remove it manually and rerun`);
    return wt;
  }
  fs.mkdirSync(WT_DIR, { recursive: true });
  const args = ['worktree', 'add'];
  if (newBranch) args.push('-b', `r20p4-${label.toLowerCase()}-tmp`);
  else args.push('--detach');
  git(REPO, [...args, wt, base]);
  return wt;
}

function cmdPrepare() {
  ctx.outDir = stagePaths();
  assert.ok(!fs.existsSync(local('manifest.json')),
    `manifest exists, refusing to overwrite: ${local('manifest.json')}`);
  fs.mkdirSync(ctx.outDir, { recursive: true });
  const base = baseHead();
  const pick = git(REPO, ['rev-parse', PICK]).out;
  const wtA = ensureWorktree('A');
  // The real B exists in both modes: the rehearsal still runs the path oracle on it.
  const wtB = join(WT_DIR, 'B');
  if (!fs.existsSync(wtB)) {
    ensureWorktree('B', { newBranch: true });
    const cp = git(wtB, ['-c', 'user.name=r20p4-runner', '-c', 'user.email=r20p4-runner@localhost',
      'cherry-pick', pick], { allowFailure: true });
    if (cp.code !== 0) {
      console.error(`cherry-pick ${PICK} onto ${base} CONFLICTED/failed; worktree ${wtB} left as-is. Record and stop.`);
      console.error(cp.out, cp.err);
      process.exit(1);
    }
  }
  assert.equal(git(wtB, ['rev-parse', 'HEAD^']).out, base, 'B must be exactly A + one cherry-picked commit');
  assert.equal(git(wtB, ['diff', '--name-only', `${base}`, 'HEAD', '--', JUDGE, 'scripts/r20_prf04_claim_path_run.mjs']).out, '',
    'the pick must not touch the judge or the runner');
  assert.equal(git(wtB, ['status', '--porcelain']).out, '', 'cherry-pick left the B worktree dirty');
  ensureWorktree('C');
  const archA = archIdentity('A', wtA);
  const arches = {
    A: archA,
    // Rehearsal: the iai arm B is A' (the A worktree under label B); the real B
    // is kept as `oracleB` for the path oracle.
    B: ctx.rehearsal ? { ...archIdentity('B', wtA), rehearsalProxyFor: 'A' } : archIdentity('B', wtB),
    C: { ...archA }, // C0 has the A source identity; only the binary differs
  };
  if (ctx.rehearsal) arches.oracleB = archIdentity('B', wtB);
  const hashes = frozenHashes();
  const manifest = { version: 1, rehearsal: ctx.rehearsal, capturedAt: now(), baseHead: base,
    pickedCommit: git(wtB, ['rev-parse', 'HEAD']).out, pickRequested: pick, frozen: hashes,
    wtDir: `<repo>/worktrees/r20p4`, pathLengths: ['A', 'B', 'C'].map(l => join(WT_DIR, l).length), arches };
  assert.equal(new Set(manifest.pathLengths).size, 1, 'A/B/C full path lengths must be equal');
  putJson('manifest.json', manifest);
  console.log(`[prepare] base=${base.slice(0, 12)} arches=${Object.keys(arches)} rehearsal=${ctx.rehearsal}`);
  console.log(`[prepare] manifest written: ${local('manifest.json')}`);
}

// --- build -------------------------------------------------------------------

function envCapture() {
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"', 'rustc -Vv', 'cargo -V',
    'valgrind --version', 'uname -srm', 'lscpu | sed -n \'s/^Model name[[:space:]]*:[[:space:]]*/cpu-model: /p\'',
    'iai-callgrind-runner --version 2>&1 || true',
    `echo iai-features: ${IAI_FEATURES}`, `echo oracle-features: ${ORACLE_FEATURES}`,
  ].join('; ');
  const r = wslOk(script, 'environment capture', { timeout: 120_000 });
  return { environmentSha256: sha(r.out), text: r.out };
}

// Find the built executable of a cargo target from `--message-format=json` output.
function executableOf(jsonOut, target) {
  const hits = [];
  for (const line of jsonOut.split('\n').filter(Boolean)) {
    try {
      const m = JSON.parse(line);
      if (m.reason === 'compiler-artifact' && m.target?.name === target && m.executable) hits.push(m.executable);
    } catch { /* cargo human lines are interleaved; ignore */ }
  }
  assert.equal(hits.length >= 1, true, `no executable reported for ${target}`);
  return hits[hits.length - 1];
}

function cargoBuild({ wt, label, target, features, rustflags }) {
  const isBench = target === JUDGE_TARGET;
  const td = `/tmp/r20p4/${label}`;
  const rf = rustflags ? `CARGO_BUILD_RUSTFLAGS='${rustflags}' ` : '';
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"',
    'unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER',
    `cd ${winToWsl(wt)}`,
    `${rf}RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR=${td} cargo ${isBench ? 'bench' : 'build'} --locked --message-format=json-render-diagnostics ${isBench ? `--bench ${target}` : `--example ${target}`} --features '${features}'${isBench ? ' --no-run' : ''}`,
  ].join(' && ');
  const r = wslOk(script, `build ${target} @${label}`, { timeout: 600_000 });
  const exe = executableOf(r.out, target);
  const sum = wslOk(`sha256sum ${exe}`, 'sha256sum binary').out.split(/\s+/)[0];
  assert.match(sum, /^[0-9a-f]{64}$/);
  return { target, exe, binarySha256: sum, features, rustflags: rustflags ?? '', targetDir: td };
}

function cmdBuild() {
  readStage('manifest.json', m => { ctx.manifest = m; });
  const wtOf = label => join(WT_DIR, label);
  const rehearsal = ctx.manifest.rehearsal;
  const env = envCapture();
  putText('environment.log', env.text);
  // The rehearsal's iai arm B is the A source under label B (distinct flags/target dir); its
  // oracle binary is still built from the real B worktree.
  const arms = {
    A: { iai: cargoBuild({ wt: wtOf('A'), label: 'A', target: JUDGE_TARGET, features: IAI_FEATURES }),
      oracle: cargoBuild({ wt: wtOf('A'), label: 'A', target: ORACLE_TARGET, features: ORACLE_FEATURES }) },
    B: { iai: cargoBuild({ wt: rehearsal ? wtOf('A') : wtOf('B'), label: 'B', target: JUDGE_TARGET, features: IAI_FEATURES,
        rustflags: rehearsal ? DISTINGUISH_RUSTFLAGS : undefined }),
      oracle: cargoBuild({ wt: wtOf('B'), label: rehearsal ? 'Bo' : 'B', target: ORACLE_TARGET, features: ORACLE_FEATURES }) },
    C: { iai: cargoBuild({ wt: wtOf('C'), label: 'C', target: JUDGE_TARGET, features: IAI_FEATURES,
        rustflags: DISTINGUISH_RUSTFLAGS }) },
  };
  assert.notEqual(arms.A.iai.binarySha256, arms.B.iai.binarySha256, 'A and B iai binaries must differ');
  assert.notEqual(arms.A.iai.binarySha256, arms.C.iai.binarySha256, 'A and C iai binaries must differ');
  putJson('builds.json', { version: 1, capturedAt: now(), environmentSha256: env.environmentSha256, arms });
  for (const [label, a] of Object.entries(arms))
    console.log(`[build] ${label} iai=${a.iai.binarySha256.slice(0, 12)}${a.oracle ? ` oracle=${a.oracle.binarySha256.slice(0, 12)}` : ''}`);
}

// --- oracle ------------------------------------------------------------------

function oracleRecord(arm, family, size) {
  const exe = ctx.builds.arms[arm].oracle.exe;
  const count = family === 'start' ? 1 : COUNT[size];
  const r = wslOk(`${exe} ${family} ${count} ${arm} 2>&1`, `oracle ${arm} ${family} ${size}`, { timeout: 120_000 });
  const line = r.out.split('\n').filter(l => l.startsWith('{')).pop();
  assert.ok(line, `oracle ${arm} ${family} ${size} printed no JSON`);
  const rec = JSON.parse(line);
  assert.equal(rec.ok, true, `oracle ${arm} ${family} ${size} failed its activation assertion: ${line}`);
  putJson(`oracle/${arm}-${family}-${size}.json`, rec);
  return rec;
}

function cmdOracle() {
  readStage('manifest.json', m => { ctx.manifest = m; });
  readStage('builds.json', b => { ctx.builds = b; });
  const m = ctx.manifest;
  const oracle = { A: {}, B: {} };
  const raw = { A: {}, B: {} };
  for (const arm of ['A', 'B']) {
    // Source identity the record is bound to: the arm of the iai cells (the rehearsal binds the
    // real-B oracle to the A' identity so the judge can be exercised).
    const src = sourceOf(m.arches[arm]);
    for (const [family, sizes] of [['start', ['n']], ...FAMILIES.map(f => [f, SIZES])]) {
      oracle[arm][family] = {}; raw[arm][family] = {};
      for (const size of sizes) {
        const rec = oracleRecord(arm, family, size);
        raw[arm][family][size] = rec;
        oracle[arm][family][size] = { n: rec.n, arm, bumpDelta: rec.bump_delta, hintDelta: rec.hint_delta,
          reclaimDelta: rec.reclaim_delta, newHeaps: rec.new_heaps, freeSetDelta: rec.free_set_delta,
          claimSourcesLen: rec.claim_sources_len, missingFifthExplicitZero: rec.missing_fifth_explicit_zero,
          binarySha256: ctx.builds.arms[arm].oracle.binarySha256, source: src,
          environmentSha256: ctx.builds.environmentSha256 };
      }
    }
  }
  putJson('oracle.json', { version: 1, capturedAt: now(), records: oracle, raw });
  for (const arm of ['A', 'B'])
    console.log(`[oracle] ${arm}: ${Object.entries(oracle[arm]).map(([f, v]) => `${f}:${Object.keys(v).length}ok`).join(' ')}`);
}

// --- measure -----------------------------------------------------------------

const CELL_NAMES = new Set(CELLS.map(c => c.name));
function parseIr(out) {
  const ir = {};
  let current = null;
  for (const line of out.split(/\r?\n/)) {
    const h = /^([A-Za-z_][\w]*::)+([A-Za-z_]\w*)\b/.exec(line);
    if (h) { current = h[2]; continue; }
    const m = /^\s*Instructions:\s*([\d,]+)/.exec(line);
    if (m && current && CELL_NAMES.has(current) && ir[current] === undefined)
      ir[current] = Number(m[1].replace(/,/g, ''));
  }
  return ir;
}

function iaiRun(run, arm) {
  const b = ctx.builds.arms[arm].iai;
  const rf = b.rustflags ? `CARGO_BUILD_RUSTFLAGS='${b.rustflags}' ` : '';
  const script = ['set -e', 'export PATH="$HOME/.cargo/bin:$PATH"',
    'unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER',
    `cd ${winToWsl(join(WT_DIR, ctx.manifest.rehearsal && arm === 'B' ? 'A' : arm))}`,
    `${rf}RUSTC_WRAPPER= CARGO_BUILD_RUSTC_WRAPPER= CARGO_TARGET_DIR=${b.targetDir} cargo bench --locked --bench ${JUDGE_TARGET} --features '${IAI_FEATURES}' 2>&1`,
  ].join(' && ');
  const startedAt = now();
  const r = wslOk(script, `iai run ${run}`, { timeout: 600_000 });
  const ir = parseIr(r.out);
  assert.deepEqual(Object.keys(ir).sort(), [...CELL_NAMES].sort(), `iai run ${run}: expected exactly the ${CELL_NAMES.size} cells`);
  assert.ok(Buffer.byteLength(r.out) < RAW_LOG_LIMIT, `raw log ${run} over ${RAW_LOG_LIMIT} bytes`);
  const log = putText(`logs/${run}.log`, r.out);
  return { run, arm, startedAt, ir, log };
}

function binarySha(exe) { return wslOk(`sha256sum ${exe}`, 'sha256sum binary').out.split(/\s+/)[0]; }

function cmdMeasure() {
  readStage('manifest.json', m => { ctx.manifest = m; });
  readStage('builds.json', b => { ctx.builds = b; });
  readStage('oracle.json', o => { ctx.oracle = o; });
  const runs = RUNS.map(r => iaiRun(r, RUN_ARM[r]));
  // The binaries measured must be exactly the ones hashed at build time.
  for (const arm of ['A', 'B', 'C'])
    assert.equal(binarySha(ctx.builds.arms[arm].iai.exe), ctx.builds.arms[arm].iai.binarySha256, `binary drift @${arm}`);
  const hashes = frozenHashes();
  const cells = [];
  for (const run of runs) {
    for (const c of CELLS) {
      let ir = run.ir[c.name];
      const slopped = ctx.slop && run.run === 'B1' && c.family === 'cold' && c.size === '2n';
      if (slopped) { ir = Math.round(ir * 1.06); console.log(`[measure] COUNTERFACTUAL: cold/B1/2n Ir inflated to ${ir}`); }
      cells.push({ name: c.name, family: c.family, size: c.size, arm: run.run, ir, environmentSha256: ctx.builds.environmentSha256,
        binarySha256: ctx.builds.arms[run.arm].iai.binarySha256, source: sourceOf(ctx.manifest.arches[run.arm]),
        startedAt: run.startedAt,
        receipt: JSON.stringify({ run: run.run, log: run.log.path, logSha256: run.log.sha256, counterfactual: slopped ? '+6% injected' : null }) });
    }
  }
  const summary = { capturedAt: now(), baseHead: ctx.manifest.baseHead, rehearsal: ctx.manifest.rehearsal };
  const input = { version: 1, hashes, environmentSha256: ctx.builds.environmentSha256,
    inventoryReceipt: JSON.stringify({ manifest: 'manifest.json', sha256: shaFile(local('manifest.json')), ...summary }),
    oracleReceipt: JSON.stringify({ oracle: 'oracle.json', sha256: shaFile(local('oracle.json')) }),
    cells, oracle: ctx.oracle.records };
  putJson('input.json', input);
  // Summary CSV derived from the SAME JSON in the SAME script (never by hand).
  const csv = ['arm,family,size,cell,ir,binarySha256',
    ...cells.map(c => `${c.arm},${c.family},${c.size},${c.name},${c.ir},${c.binarySha256}`)].join('\n') + '\n';
  putText('summary.csv', csv);
  const judge = spawnSync(process.execPath, [join(REPO, JUDGE), '--input', local('input.json')],
    { encoding: 'utf8', maxBuffer: 1 << 24 });
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
    return resolve(REPO, process.argv[process.argv.indexOf('--out-dir') + 1]);
  }
  return resolve(REPO, ctx.rehearsal ? REHEARSAL_OUT : DEFAULT_OUT);
}

function rehearsal() {
  ctx.outDir = stagePaths();
  putText('REHEARSAL.txt', 'REHEARSAL - not gate data. The iai arm "B" is a second instance of the A source (label A\'),\nbuilt with -C debuginfo=line-tables-only; the oracle stage runs the real B build (path activation only).\n');
  cmdPrepare(); cmdBuild(); cmdOracle(); cmdMeasure();
}

// Remove the arm worktrees, the temp branch and the WSL target dirs (between a rehearsal and the real run).
function cmdCleanup() {
  for (const label of ['A', 'B', 'C']) {
    const wt = join(WT_DIR, label);
    if (fs.existsSync(wt)) git(REPO, ['worktree', 'remove', '--force', wt]);
  }
  git(REPO, ['branch', '-D', 'r20p4-b-tmp'], { allowFailure: true });
  try { fs.rmdirSync(WT_DIR); } catch { /* not empty or absent */ }
  wslOk('rm -rf /tmp/r20p4', 'wsl cleanup', { timeout: 120_000 });
  console.log('[cleanup] arm worktrees, temp branch and WSL target dirs removed');
}

function main() {
  const args = process.argv.slice(2);
  ctx.slop = args.includes('--slop');
  const modes = args.filter(a => a.startsWith('--') && !['--slop', '--out-dir'].includes(a));
  assert.equal(modes.length, 1, `expected exactly one mode, got: ${modes}`);
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
