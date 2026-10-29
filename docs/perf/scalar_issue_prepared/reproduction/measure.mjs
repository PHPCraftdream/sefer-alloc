import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';
import crypto from 'node:crypto';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';

const root = process.cwd();
const repos = Object.fromEntries(['baseline', 'prepared'].map(x => [x, path.join(root, 'worktrees', `scalar-issue-${x}`)]));
const scratch = path.join(root, '.rush/scalar-issue');
const evidence = path.join(repos.prepared, 'docs/perf/scalar_issue_prepared');
const raw = path.join(evidence, 'raw');
fs.mkdirSync(raw, { recursive: true });
const features = 'production internals bench-internals';
const base = 'f80c681b799ef4ef7fa08d79c06896fdf1104fe9';
const files = ['Cargo.toml', 'Cargo.lock', 'examples/scalar_issue_gate.rs', 'benches/global_alloc.rs', 'src/alloc_core/segment/segment_table/route_slots.rs', 'src/registry/segment_route/small_sidecar.rs'];
const hash = b => crypto.createHash('sha256').update(b).digest('hex');
function command(cmd, args, cwd, options = {}) {
  const p = spawnSync(cmd, args, { cwd, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, ...options });
  if (p.error || p.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${p.error ?? p.status}\n${p.stdout}\n${p.stderr}`);
  return p.stdout;
}
function hashes(repo) { return Object.fromEntries(files.map(f => [f, hash(fs.readFileSync(path.join(repo, f)))])); }
const initial = Object.fromEntries(Object.entries(repos).map(([arm, repo]) => [arm, hashes(repo)]));
assert.equal(initial.baseline['examples/scalar_issue_gate.rs'], initial.prepared['examples/scalar_issue_gate.rs']);
assert.equal(initial.baseline['Cargo.lock'], initial.prepared['Cargo.lock']);
assert.equal(initial.baseline['benches/global_alloc.rs'], initial.prepared['benches/global_alloc.rs']);
const identities = {};
for (const [arm, repo] of Object.entries(repos)) {
  assert.equal(command('git', ['rev-parse', 'HEAD'], repo).trim(), base);
  const patch = command('git', ['diff', '--binary', '--no-ext-diff', base, '--', 'src', 'Cargo.toml', 'Cargo.lock', 'benches'], repo);
  fs.writeFileSync(path.join(evidence, `${arm}.patch`), patch);
  identities[arm] = { base, patch_sha256: hash(patch), new_example_sha256: initial[arm]['examples/scalar_issue_gate.rs'], source_hashes: initial[arm] };
}
const meta = { captured_before_build: new Date().toISOString(), features, platform: os.platform(), release: os.release(), arch: os.arch(), cpu: os.cpus()[0].model, rustc: command('rustc', ['-Vv'], root), cargo: command('cargo', ['-V'], root).trim(), identities };
fs.writeFileSync(path.join(evidence, 'source_identity.json'), JSON.stringify(meta, null, 2));
fs.copyFileSync(path.join(repos.prepared, 'examples/scalar_issue_gate.rs'), path.join(evidence, 'scalar_issue_gate.rs'));
const targets = Object.fromEntries(Object.keys(repos).map(arm => [arm, path.join(scratch, `target-${arm}`)]));
for (const [arm, repo] of Object.entries(repos)) {
  console.log(`Build benchmark probe: ${arm}`);
  const env = { ...process.env, CARGO_TARGET_DIR: targets[arm] };
  const p = spawnSync('cargo', ['build', '--release', '--locked', '--example', 'scalar_issue_gate', '--features', features], { cwd: repo, env, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  fs.writeFileSync(path.join(scratch, `build-${arm}.log`), `${p.stdout ?? ''}\n${p.stderr ?? ''}`);
  if (p.error || p.status !== 0) throw new Error(`probe build ${arm}: ${p.error ?? p.status}; see scratch build log`);
}
const observations = [];
function probe(arm, n, block, position, control) {
  const exe = path.join(targets[arm], 'release/examples/scalar_issue_gate.exe');
  const t = process.hrtime.bigint();
  const out = command(exe, [String(n)], repos[arm]);
  const process_ns = Number(process.hrtime.bigint() - t);
  const row = JSON.parse(out.trim());
  assert.equal(row.oracle_ok, true);
  assert.equal(row.scalar_calls, n);
  assert.equal(row.n, n);
  const record = { arm, n, block, position, control, process_ns, ...row };
  observations.push(record);
  fs.appendFileSync(path.join(raw, 'scalar.ndjson'), JSON.stringify(record) + '\n');
}
assert(!fs.existsSync(path.join(raw, 'scalar.ndjson')), 'refuse overwrite of measurements');
for (const n of [32768, 65536]) {
  for (let b = 0; b < 20; b++) {
    for (const [i, arm] of ['baseline', 'prepared', 'prepared', 'baseline'].entries()) probe(arm, n, b, i, false);
  }
  console.log(`Scalar ABBA complete N=${n}: 80 fresh processes`);
}
for (let b = 0; b < 10; b++) for (let i = 0; i < 4; i++) probe('baseline', 65536, b, i, true);
console.log('Scalar A/A complete: 40 fresh processes');
// Каждая canonical sweep — отдельный процесс; snapshots снимаются до следующего прогона arm.
for (const [i, arm] of ['baseline', 'prepared', 'prepared', 'baseline'].entries()) {
  console.log(`Canonical ${i + 1}/4: ${arm}`);
  const env = { ...process.env, CARGO_TARGET_DIR: targets[arm] };
  const p = spawnSync(process.execPath, ['scripts/bench-table.mjs'], { cwd: repos[arm], env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  fs.writeFileSync(path.join(scratch, `canonical-${i}-${arm}.log`), `${p.stdout ?? ''}\n${p.stderr ?? ''}`);
  if (p.error || p.status !== 0) throw new Error(`canonical ${arm}: ${p.error ?? p.status}; see scratch log`);
  const text = `${p.stdout}\n${p.stderr}`;
  // Не публикуем строки Cargo с частными путями; числовой вывод не меняется.
  const clean = text.split(/\r?\n/).map(l => /[A-Za-z]:[\\/]/.test(l) ? '# TRUNCATED — local build path; command: npm run bench:table' : l).join('\n');
  fs.writeFileSync(path.join(raw, `canonical-${i}-${arm}.log`), clean);
  const source = path.join(targets[arm], 'criterion');
  const dest = path.join(raw, `criterion-${i}-${arm}`);
  function copyNew(dir, rel = '') {
    for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
      const src = path.join(dir, e.name), r = path.join(rel, e.name);
      if (e.isDirectory()) {
        if (e.name === 'report' || e.name === 'base' || e.name === 'change') continue;
        copyNew(src, r);
      } else if (path.basename(path.dirname(src)) === 'new' && ['sample.json', 'estimates.json', 'benchmark.json'].includes(e.name)) {
        const to = path.join(dest, r);
        fs.mkdirSync(path.dirname(to), { recursive: true });
        fs.copyFileSync(src, to);
      }
    }
  }
  copyNew(source);
}
for (const [arm, repo] of Object.entries(repos)) assert.deepEqual(hashes(repo), initial[arm], `${arm} changed while measuring`);
fs.writeFileSync(path.join(evidence, 'completed.json'), JSON.stringify({ completed: new Date().toISOString(), scalar_processes: observations.length, canonical_sweeps: 4 }, null, 2));
console.log('All requested measurements complete; source inputs unchanged.');
