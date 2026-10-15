// R6 A/B iai runner: deterministic instruction-count (Ir) A/B between the
// merge-base of HEAD and origin/main (arm A) and HEAD (arm B) on one
// iai-callgrind bench target. Runs ONLY in GitHub CI
// (.github/workflows/r20-gates.yml, PR-check mode via `.github/gate-run.json`
// + the `gate-run` label): stages are invoked as
// `node scripts/r20_r6_ab_iai_run.mjs --<stage> [--out-dir DIR]`.
//
// Stages:
//   --prepare  resolve A (config baseSha, else `git merge-base HEAD
//              origin/main`) and B (HEAD); create detached worktrees
//              target/r6ab/{A,B} and write the immutable identity manifest
//              (manifest.json) BEFORE any measurement; refuses to overwrite.
//   --build    `cargo bench --no-run` per arm into target/r6ab/tg-{A,B}
//              (RUSTC_WRAPPER scrubbed, --locked); records the binary SHA256.
//   --oracle   extension point: a path-activation oracle (bench-internals
//              counters bound to the manifest identity) is intentionally NOT
//              wired yet; the stage is a documented no-op so the default
//              stage list `prepare,build,oracle,measure` works and a future
//              oracle slots in without a workflow change (see
//              docs/CI_TIERS.md, "A/B iai runs on a PR").
//   --measure  `cargo bench` per arm, parse `Instructions:` per cell, store
//              sanitised raw logs, derive summary.csv and verdict.txt from
//              the SAME parsed numbers. With `thresholds` in the config the
//              verdict is PASS/FAIL per cell (delta_pct > threshold) and the
//              stage exits 1 on any FAIL.
//   --cleanup  remove the arm worktrees and target/r6ab.
//
// Config fields (read from .github/gate-run.json; also validated by
// scripts/r20-gate-config.mjs, whose validated values reach this runner via
// the workflow's GITHUB_ENV): benchTarget (default perf_gate_iai), features
// (default "production bench-internals", as scripts/iai.mjs), cells
// (optional; default = every cell both arms print, sets must match), baseSha
// (optional explicit A), thresholds (optional map cell -> max tolerated
// delta_pct). A dirty worktree, a manifest overwrite, a cell-set mismatch
// between arms, a non-positive A count or a binary drift between build and
// measure are all hard errors.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const RUNNER_DIR = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(RUNNER_DIR, '..');
const AB_DIR = join(REPO, 'target', 'r6ab');
const DEFAULT_OUT = 'docs/perf/R20_R6_AB_IAI_RUN01';
const CONFIG_FILE = join(REPO, '.github', 'gate-run.json');
const DEFAULT_BENCH = 'perf_gate_iai';
const DEFAULT_FEATURES = 'production bench-internals';
const RAW_LOG_LIMIT = 2 * 1024 * 1024;

const BENCH_TARGET_RE = /^[a-z][a-z0-9_]{0,63}$/;
const FEATURES_RE = /^[a-z0-9][a-z0-9 _-]{0,199}$/;
const CELL_RE = /^[A-Za-z0-9_]+$/;
const SHA_RE = /^[0-9a-f]{40}$/;

const sha256 = (buf) => createHash('sha256').update(buf).digest('hex');
const nowIso = () => new Date().toISOString();
const hasOwn = (o, k) => Object.prototype.hasOwnProperty.call(o, k);

// --- pure helpers (covered by --selftest) ------------------------------------

function validBenchTarget(v) {
  return typeof v === 'string' && BENCH_TARGET_RE.test(v);
}

function validFeatures(v) {
  return typeof v === 'string' && FEATURES_RE.test(v);
}

// Non-empty, unique, comma-separated cell names, safe characters only.
function validCellsSpec(v) {
  if (typeof v !== 'string') return false;
  const parts = v.split(',').map(s => s.trim()).filter(s => s.length > 0);
  return parts.length > 0 && parts.every(p => CELL_RE.test(p))
    && new Set(parts).size === parts.length;
}

function validBaseSha(v) {
  return typeof v === 'string' && SHA_RE.test(v);
}

// cell -> max tolerated delta_pct (finite number, sane range, >= 1 entry).
function validThresholds(v) {
  if (typeof v !== 'object' || v === null || Array.isArray(v)) return false;
  const keys = Object.keys(v);
  return keys.length > 0 && keys.every(k => CELL_RE.test(k)
    && typeof v[k] === 'number' && Number.isFinite(v[k]) && Math.abs(v[k]) <= 100000);
}

function cellsFromSpec(spec) {
  return spec.split(',').map(s => s.trim()).filter(s => s.length > 0);
}

// iai-callgrind 0.14 prints an indented dotted-path header line per benchmark, then an
// `Instructions:` block under it (same shape r20_prf04_claim_path_run.mjs
// parses). `allowed` is a Set of expected cell names, or null for discovery
// mode (accept any cell-shaped name).
function parseIr(out, allowed) {
  const ir = {};
  let current = null;
  for (const line of String(out).split(/\r?\n/)) {
    const h = /^\s*([A-Za-z_]\w*::)+([A-Za-z_]\w*)\b/.exec(line);
    if (h) {
      current = h[2];
      if (allowed && !allowed.has(current)) current = null;
      continue;
    }
    const m = /^\s*Instructions:\s*([\d,]+)\s*$/.exec(line);
    if (m && current !== null && ir[current] === undefined) {
      ir[current] = Number(m[1].replace(/,/g, ''));
    }
  }
  return ir;
}

// The ratio is computed with asserts, never by hand: A must be a positive
// count, the percentage finite and plausible, or the run fails loudly.
function abRow(cell, aIr, bIr) {
  assert.ok(Number.isInteger(aIr) && aIr > 0, `A Ir for ${cell} must be a positive integer, got ${aIr}`);
  assert.ok(Number.isInteger(bIr) && bIr >= 0, `B Ir for ${cell} must be a non-negative integer, got ${bIr}`);
  const delta = bIr - aIr;
  const pct = (delta / aIr) * 100;
  assert.ok(Number.isFinite(pct), `non-finite delta_pct for ${cell}`);
  assert.ok(Math.abs(pct) <= 10000, `implausible delta_pct for ${cell}: ${pct}`);
  return { cell, aIr, bIr, delta, pct };
}

function csvField(v) {
  const s = String(v);
  return /[",\r\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s;
}

function summaryCsv(rows, meta) {
  const head = 'cell,A_Ir,B_Ir,delta,delta_pct,sha_a,sha_b,features,rustc,os';
  const lines = rows.map(r => [r.cell, r.aIr, r.bIr, r.delta, r.pct.toFixed(4),
    meta.shaA, meta.shaB, meta.features, meta.rustc, meta.os].map(csvField).join(','));
  return [head, ...lines].join('\n') + '\n';
}

// Verdict = pure threshold check. Without thresholds the table is
// informational only (no pass/fail semantics).
function verdictLines(rows, thresholds) {
  const out = ['r6 A/B iai verdict', ''];
  out.push(`${'cell'.padEnd(44)} ${'A_Ir'.padStart(14)} ${'B_Ir'.padStart(14)} ${'delta'.padStart(12)} ${'delta_pct'.padStart(10)}  verdict`);
  let fails = 0;
  for (const r of rows) {
    let label = 'info';
    let suffix = '';
    if (thresholds && hasOwn(thresholds, r.cell)) {
      const limit = thresholds[r.cell];
      label = r.pct > limit ? 'FAIL' : 'PASS';
      suffix = ` (limit ${limit})`;
      if (label === 'FAIL') fails += 1;
    }
    out.push(`${r.cell.padEnd(44)} ${String(r.aIr).padStart(14)} ${String(r.bIr).padStart(14)} ${String(r.delta).padStart(12)} ${r.pct.toFixed(2).padStart(9)}%  ${label}${suffix}`);
  }
  out.push('');
  if (!thresholds) out.push('no thresholds in config: informational only, no pass/fail');
  else out.push(fails === 0
    ? `VERDICT: PASS (${rows.length} cells)`
    : `VERDICT: FAIL (${fails} of ${rows.length} cells over threshold)`);
  return { lines: out, fails };
}

// --- config resolution -------------------------------------------------------

// Precedence: defaults < .github/gate-run.json < validated GITHUB_ENV values
// (the dispatch path has no config file, so env is the only carrier there).
function loadRunConfig() {
  const cfg = { benchTarget: DEFAULT_BENCH, features: DEFAULT_FEATURES,
    cells: undefined, baseSha: undefined, thresholds: undefined };
  if (fs.existsSync(CONFIG_FILE)) {
    const raw = JSON.parse(fs.readFileSync(CONFIG_FILE, 'utf8'));
    if (raw.benchTarget !== undefined) {
      assert.ok(validBenchTarget(raw.benchTarget), `invalid benchTarget in gate-run.json: ${JSON.stringify(raw.benchTarget)}`);
      cfg.benchTarget = raw.benchTarget;
    }
    if (raw.features !== undefined) {
      assert.ok(validFeatures(raw.features), `invalid features in gate-run.json: ${JSON.stringify(raw.features)}`);
      cfg.features = raw.features;
    }
    if (raw.cells !== undefined) {
      assert.ok(validCellsSpec(raw.cells), `invalid cells in gate-run.json: ${JSON.stringify(raw.cells)}`);
      cfg.cells = cellsFromSpec(raw.cells);
    }
    if (raw.baseSha !== undefined) {
      assert.ok(validBaseSha(raw.baseSha), `invalid baseSha in gate-run.json: ${JSON.stringify(raw.baseSha)}`);
      cfg.baseSha = raw.baseSha;
    }
    if (raw.thresholds !== undefined) {
      assert.ok(validThresholds(raw.thresholds), `invalid thresholds in gate-run.json: ${JSON.stringify(raw.thresholds)}`);
      cfg.thresholds = raw.thresholds;
    }
  }
  const envVal = (k) => {
    const v = process.env[k];
    return v === undefined || v === '' ? undefined : v;
  };
  const fromEnv = [
    ['BENCH_TARGET_INPUT', 'benchTarget', validBenchTarget],
    ['FEATURES_INPUT', 'features', validFeatures],
    ['BASE_SHA_INPUT', 'baseSha', validBaseSha],
  ];
  for (const [envKey, field, valid] of fromEnv) {
    const v = envVal(envKey);
    if (v !== undefined) {
      assert.ok(valid(v), `invalid ${envKey}: ${JSON.stringify(v)}`);
      cfg[field] = v;
    }
  }
  const cellsEnv = envVal('CELLS_INPUT');
  if (cellsEnv !== undefined) {
    assert.ok(validCellsSpec(cellsEnv), `invalid CELLS_INPUT: ${JSON.stringify(cellsEnv)}`);
    cfg.cells = cellsFromSpec(cellsEnv);
  }
  const thEnv = envVal('THRESHOLDS_INPUT');
  if (thEnv !== undefined) {
    let parsed;
    try { parsed = JSON.parse(thEnv); } catch (e) {
      assert.fail(`invalid THRESHOLDS_INPUT JSON: ${e.message}`);
    }
    assert.ok(validThresholds(parsed), `invalid THRESHOLDS_INPUT: ${JSON.stringify(thEnv)}`);
    cfg.thresholds = parsed;
  }
  return cfg;
}

// --- process helpers ---------------------------------------------------------

function git(cwd, args) {
  const r = spawnSync('git', ['--no-optional-locks', ...args], { cwd, encoding: 'utf8', maxBuffer: 1 << 26 });
  assert.equal(r.status, 0, `git ${args.join(' ')} failed: ${r.stderr}`);
  return r.stdout.trim();
}

function scrubEnv(extra) {
  return { ...process.env, ...extra,
    RUSTC_WRAPPER: '', CARGO_BUILD_RUSTC_WRAPPER: '' };
}

// Find the built executable of a cargo target from --message-format=json
// output (same approach as r20_prf04_claim_path_run.mjs).
function executableOf(jsonOut, target) {
  const hits = [];
  for (const line of jsonOut.split('\n').filter(Boolean)) {
    try {
      const m = JSON.parse(line);
      if (m.reason === 'compiler-artifact' && m.target?.name === target && m.executable) {
        hits.push(m.executable);
      }
    } catch { /* cargo human lines are interleaved; ignore */ }
  }
  assert.ok(hits.length >= 1, `no executable reported for ${target}`);
  return hits[hits.length - 1];
}

// --- stages ------------------------------------------------------------------

function stagePaths(args) {
  if (args.includes('--out-dir')) {
    return resolve(REPO, args[args.indexOf('--out-dir') + 1]);
  }
  return resolve(REPO, DEFAULT_OUT);
}

function cmdPrepare(ctx) {
  const shaB = git(REPO, ['rev-parse', 'HEAD']);
  const shaA = ctx.cfg.baseSha ?? git(REPO, ['merge-base', 'HEAD', 'origin/main']);
  assert.ok(SHA_RE.test(shaA), `arm A is not a full SHA: ${shaA}`);
  assert.ok(SHA_RE.test(shaB), `arm B is not a full SHA: ${shaB}`);
  assert.notEqual(shaA, shaB, 'arms A and B resolved to the same commit');
  fs.mkdirSync(AB_DIR, { recursive: true });
  for (const [label, sha] of [['A', shaA], ['B', shaB]]) {
    const wt = join(AB_DIR, label);
    if (!fs.existsSync(wt)) git(REPO, ['worktree', 'add', '--detach', wt, sha]);
    assert.equal(git(wt, ['rev-parse', 'HEAD']), sha, `worktree ${label} is not at ${sha}`);
    assert.equal(git(wt, ['status', '--porcelain']), '', `worktree ${label} is dirty`);
  }
  const rustc = spawnSync('rustc', ['--version'], { encoding: 'utf8' }).stdout.trim();
  const os = spawnSync('uname', ['-sr'], { encoding: 'utf8' }).stdout.trim();
  const manifest = { version: 1, createdAt: nowIso(), shaA, shaB,
    benchTarget: ctx.cfg.benchTarget, features: ctx.cfg.features,
    cells: ctx.cfg.cells ?? null, thresholds: ctx.cfg.thresholds ?? null, rustc, os };
  const manifestPath = join(ctx.outDir, 'manifest.json');
  assert.ok(!fs.existsSync(manifestPath), `manifest exists, refusing to overwrite: ${manifestPath}`);
  fs.mkdirSync(ctx.outDir, { recursive: true });
  fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
  console.log(`[prepare] A=${shaA.slice(0, 12)} B=${shaB.slice(0, 12)} bench=${ctx.cfg.benchTarget} features='${ctx.cfg.features}'`);
  console.log(`[prepare] manifest written: ${manifestPath}`);
}

function buildArm(ctx, label) {
  const wt = join(AB_DIR, label);
  const td = join(AB_DIR, `tg-${label}`);
  const r = spawnSync('cargo', ['bench', '--locked', '--bench', ctx.cfg.benchTarget,
    '--features', ctx.cfg.features, '--no-run', '--message-format=json'],
    { cwd: wt, encoding: 'utf8', maxBuffer: 1 << 28,
      env: scrubEnv({ CARGO_TARGET_DIR: td }) });
  assert.equal(r.status, 0, `cargo bench --no-run @${label} failed\n--- stderr tail ---\n${(r.stderr ?? '').split('\n').slice(-40).join('\n')}`);
  const exe = executableOf(r.stdout, ctx.cfg.benchTarget);
  const binSha = sha256(fs.readFileSync(exe));
  const receipt = { label, targetDir: td, exe, binarySha256: binSha };
  fs.writeFileSync(join(ctx.outDir, `build-${label}.json`), JSON.stringify(receipt, null, 2) + '\n');
  console.log(`[build] ${label} binary=${binSha.slice(0, 12)} targetDir=${td}`);
  return receipt;
}

function cmdBuild(ctx) {
  fs.mkdirSync(ctx.outDir, { recursive: true });
  for (const label of ['A', 'B']) buildArm(ctx, label);
}

function cmdOracle() {
  // Extension point, deliberately not wired: a path-activation oracle (the
  // bench-internals counters, bound to the manifest identity and binary
  // hashes like r20_prf04_claim_path_run.mjs --oracle) would print its
  // per-cell records here, between --build and --measure.
  console.log('[oracle] extension point: no path-activation oracle is wired for this runner yet');
  console.log('[oracle] (bench-internals counters; see docs/CI_TIERS.md, "A/B iai runs on a PR")');
}

// Run the bench for one arm and return { ir, raw } with the raw log stored.
function measureArm(ctx, label, binSha) {
  const wt = join(AB_DIR, label);
  const td = join(AB_DIR, `tg-${label}`);
  const r = spawnSync('cargo', ['bench', '--locked', '--bench', ctx.cfg.benchTarget,
    '--features', ctx.cfg.features],
    { cwd: wt, encoding: 'utf8', maxBuffer: 1 << 28, timeout: 3_600_000,
      env: scrubEnv({ CARGO_TARGET_DIR: td }) });
  assert.equal(r.status, 0, `cargo bench @${label} failed\n--- stderr tail ---\n${(r.stderr ?? '').split('\n').slice(-40).join('\n')}`);
  const raw = `${r.stdout}\n${r.stderr}`;
  assert.ok(Buffer.byteLength(raw) < RAW_LOG_LIMIT, `raw log @${label} over ${RAW_LOG_LIMIT} bytes`);
  // Immutable identity: the measured binary must be the one hashed at build time.
  const build = JSON.parse(fs.readFileSync(join(ctx.outDir, `build-${label}.json`), 'utf8'));
  assert.equal(sha256(fs.readFileSync(build.exe)), binSha, `binary drift @${label}`);
  fs.mkdirSync(join(ctx.outDir, 'logs'), { recursive: true });
  fs.writeFileSync(join(ctx.outDir, 'logs', `${label}.log`), raw);
  const allowed = ctx.cfg.cells ? new Set(ctx.cfg.cells) : null;
  const ir = parseIr(raw, allowed);
  assert.ok(Object.keys(ir).length > 0, `iai produced no cells @${label}`);
  return ir;
}

function cmdMeasure(ctx) {
  const manifestPath = join(ctx.outDir, 'manifest.json');
  assert.ok(fs.existsSync(manifestPath), `manifest missing: ${manifestPath} (run --prepare first)`);
  const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  const buildReceipts = { A: JSON.parse(fs.readFileSync(join(ctx.outDir, 'build-A.json'), 'utf8')),
    B: JSON.parse(fs.readFileSync(join(ctx.outDir, 'build-B.json'), 'utf8')) };
  const irs = {
    A: measureArm(ctx, 'A', buildReceipts.A.binarySha256),
    B: measureArm(ctx, 'B', buildReceipts.B.binarySha256),
  };
  const keys = Object.keys(irs.A).sort();
  assert.ok(keys.length > 0, 'no cells parsed from arm A');
  assert.deepEqual(Object.keys(irs.B).sort(), keys, 'arms A and B exposed different cell sets');
  if (ctx.cfg.cells) {
    assert.deepEqual(keys, [...ctx.cfg.cells].sort(), 'measured cell set differs from the configured one');
  }
  const rows = keys.map(cell => abRow(cell, irs.A[cell], irs.B[cell]));
  fs.writeFileSync(join(ctx.outDir, 'summary.csv'), summaryCsv(rows, manifest));
  const { lines, fails } = verdictLines(rows, manifest.thresholds);
  const verdict = [...lines, '', `A=${manifest.shaA}`, `B=${manifest.shaB}`,
    `features: ${manifest.features}`, `rustc: ${manifest.rustc}`, `os: ${manifest.os}`].join('\n') + '\n';
  fs.writeFileSync(join(ctx.outDir, 'verdict.txt'), verdict);
  process.stdout.write(verdict);
  if (manifest.thresholds && fails > 0) process.exitCode = 1;
}

function cmdCleanup() {
  for (const label of ['A', 'B']) {
    const wt = join(AB_DIR, label);
    if (fs.existsSync(wt)) git(REPO, ['worktree', 'remove', '--force', wt]);
  }
  fs.rmSync(AB_DIR, { recursive: true, force: true });
  console.log('[cleanup] arm worktrees and target/r6ab removed');
}

// --- selftest ----------------------------------------------------------------

function selftest() {
  let checks = 0;
  const ok = (cond, name) => {
    checks += 1;
    if (!cond) {
      console.error(`selftest FAIL: ${name}`);
      process.exit(1);
    }
  };
  const sample = [
    '  perf_gate::small_churn_16b',
    '    Instructions:                 3,281',
    '    L1 Hits:                      5,000',
    '  perf_gate::cold_alloc_only_256x16b',
    '    Instructions:               123,456',
    'bench_user::not_a_cell',
    '    Instructions:                       7',
  ].join('\n');
  const ir = parseIr(sample, new Set(['small_churn_16b', 'cold_alloc_only_256x16b']));
  ok(ir.small_churn_16b === 3281, 'parseIr keeps the allowed cell');
  ok(ir.cold_alloc_only_256x16b === 123456, 'parseIr strips thousands separators');
  ok(!('not_a_cell' in ir), 'parseIr drops cells outside the allowed set');
  const discovered = parseIr(sample, null);
  ok(Object.keys(discovered).length === 3, 'parseIr discovery mode accepts any cell-shaped name');

  const row = abRow('c', 100, 110);
  ok(row.delta === 10 && Math.abs(row.pct - 10) < 1e-9, 'abRow computes delta and pct');
  ok(abRow('c', 100, 0).pct === -100, 'abRow tolerates B=0');
  okThrows(() => abRow('c', 0, 5), 'abRow rejects non-positive A');
  okThrows(() => abRow('c', 1.5, 3), 'abRow rejects non-integer Ir');

  ok(validBenchTarget('perf_gate_iai') && !validBenchTarget('Perf') && !validBenchTarget('a b'), 'benchTarget allowlist');
  ok(validFeatures('production bench-internals') && !validFeatures('a,b') && !validFeatures(''), 'features allowlist');
  ok(validCellsSpec('a,b_2') && validCellsSpec('a,,') && !validCellsSpec('a,a') && !validCellsSpec(1), 'cells allowlist (empty items skipped, duplicates rejected)');
  ok(validBaseSha('0123456789abcdef0123456789abcdef01234567') && !validBaseSha('HEAD'), 'baseSha allowlist');
  ok(validThresholds({ c: 1.5, d: -100 }) && !validThresholds({}) && !validThresholds({ c: 'x' }) && !validThresholds([1]), 'thresholds allowlist');

  const csv = summaryCsv([row], { shaA: 'a'.repeat(40), shaB: 'b'.repeat(40),
    features: 'production bench-internals', rustc: 'rustc 1.0', os: 'Linux x' });
  ok(csv.startsWith('cell,A_Ir,B_Ir,delta,delta_pct,sha_a,sha_b,features,rustc,os\n'), 'summary.csv header');
  ok(csv.includes('c,100,110,10,10.0000'), 'summary.csv data row');
  ok(summaryCsv([{ ...row, cell: 'weird,name' }], { shaA: 'a', shaB: 'b', features: 'f', rustc: 'r', os: 'o' })
    .includes('"weird,name"'), 'summary.csv quotes commas');

  const pass = verdictLines([row], { c: 10.5 });
  ok(pass.fails === 0 && pass.lines.at(-1).includes('VERDICT: PASS'), 'verdict PASS at limit');
  const fail = verdictLines([row], { c: 5 });
  ok(fail.fails === 1 && fail.lines.at(-1).includes('VERDICT: FAIL'), 'verdict FAIL over limit');
  const info = verdictLines([row], null);
  ok(info.fails === 0 && info.lines.at(-1).includes('informational only'), 'verdict without thresholds is informational');
  console.log(JSON.stringify({ status: 'PASS', checks }));
}

function okThrows(fn, name) {
  try {
    fn();
  } catch {
    return;
  }
  throw new Error(`selftest FAIL (expected throw): ${name}`);
}

// --- main --------------------------------------------------------------------

function main() {
  const args = process.argv.slice(2);
  if (args.includes('--selftest')) return selftest();
  // CI-only runner (Linux workflow host): refuse to do anything heavy elsewhere.
  assert.equal(process.platform, 'linux', 'this runner is GitHub-CI-only (ubuntu-24.04); use --selftest elsewhere');
  assert.ok(!args.includes('--slop'), '--slop is not supported by this runner (no negative control is defined)');
  const cfg = loadRunConfig();
  const ctx = { cfg, outDir: stagePaths(args) };
  const modes = args.filter(a => a.startsWith('--') && !['--slop', '--out-dir'].includes(a));
  assert.equal(modes.length, 1, `expected exactly one stage mode, got: ${modes}`);
  if (modes[0] === '--prepare') return cmdPrepare(ctx);
  if (modes[0] === '--build') return cmdBuild(ctx);
  if (modes[0] === '--oracle') return cmdOracle(ctx);
  if (modes[0] === '--measure') return cmdMeasure(ctx);
  if (modes[0] === '--cleanup') return cmdCleanup();
  throw new Error(`unknown mode ${modes[0]}`);
}

main();
