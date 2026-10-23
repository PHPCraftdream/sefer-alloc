import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const outRoot = path.join(root, 'raw/d1-registration/run7');
const identity = JSON.parse(fs.readFileSync(path.join(outRoot, 'source_identity.json'), 'utf8'));
const wslRoot = process.env.D1_WSL_REPO;
assert.ok(wslRoot?.startsWith('/'), 'D1_WSL_REPO должен задавать абсолютный Linux-путь candidate worktree');
const runs = [
  { id: 'before', repo: `${wslRoot}/.rush/perf-d1/before-repo`, winRepo: path.join(root, 'before-repo'), target: `${wslRoot}/.rush/perf-d1/target-linux-before`, commit: identity.measurement_source_snapshot.baseline_commit, tree: identity.measurement_source_snapshot.baseline_measurement_tree, features: 'production internals' },
  { id: 'after', repo: wslRoot, winRepo: path.resolve(root, '../..'), target: `${wslRoot}/.rush/perf-d1/target-linux-after`, commit: identity.measurement_source_snapshot.candidate_commit, tree: identity.measurement_source_snapshot.candidate_measurement_tree, features: 'production internals bench-internals' },

];
const samplesPath = path.join(outRoot, 'linux_retained_pin_samples.ndjson');
assert.ok(!fs.existsSync(samplesPath), `refusing to overwrite ${samplesPath}`);
const pairsN = 20;

function shQuote(value) { return `'${value.replaceAll("'", "'\\''")}'`; }
function wsl(command) {
  return spawnSync('wsl.exe', ['--exec', 'bash', '-lc', command], { encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 600_000 });
}
function parse(stdout, runSpec) {
  const values = {};
  for (const line of stdout.split(/\r?\n/)) {
    const match = /^RESULT\s+([a-z0-9_]+)=(\d+)$/.exec(line.trim());
    if (match) values[match[1]] = Number(match[2]);
  }
  const stages = ['before', 'root', 'registration', 'owner_unlink_pin_held', 'last_pin_drop', 'root_free'];
  const fields = ['rss', 'vm_size', 'pss', 'private_dirty'];
  for (const field of fields) for (const stage of stages) assert.ok(Number.isFinite(values[`${field}_${stage}_kib`]), `${runSpec.id}: missing ${field}_${stage}_kib`);
  for (const key of ['sidecar_size_bytes', 'sidecar_alloc_bytes', 'sidecar_free_bytes_owner_unlink', 'sidecar_free_bytes_last_pin', 'sidecar_alloc_calls', 'sidecar_free_calls']) assert.ok(Number.isFinite(values[key]), `${runSpec.id}: missing ${key}`);
  assert.equal(values.sidecar_alloc_bytes, values.sidecar_size_bytes);
  assert.equal(values.sidecar_free_bytes_owner_unlink, 0);
  assert.equal(values.sidecar_free_bytes_last_pin, values.sidecar_alloc_bytes);
  assert.equal(values.sidecar_alloc_calls, 1);
  assert.equal(values.sidecar_free_calls, 1);
  if (runSpec.id === 'after') {
    assert.equal(values.constructed_chunks_while_pinned, 1);
    assert.equal(values.pin_refs_after_unlink, 1);
  }
  return values;
}
function mean(xs) { return xs.reduce((a, b) => a + b, 0) / xs.length; }
function median(xs) { const s = [...xs].sort((a, b) => a - b); const m = Math.floor(s.length / 2); return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; }
function tSummary(xs) {
  const avg = mean(xs), n = xs.length;
  const sd = Math.sqrt(xs.reduce((sum, x) => sum + (x - avg) ** 2, 0) / (n - 1));
  const se = sd / Math.sqrt(n), crit = 2.093;
  return { n, mean_delta: avg, ci95_low: avg - crit * se, ci95_high: avg + crit * se, t: se === 0 ? 0 : avg / se, significant_p_lt_005_approx: se > 0 && Math.abs(avg / se) > crit };
}

for (const runSpec of runs) {
  const index = path.join(root, `source-check-linux-${runSpec.id}.index`);
  assert.ok(!fs.existsSync(index), `refusing to reuse ${index}`);
  const env = { ...process.env, GIT_INDEX_FILE: index, GIT_OPTIONAL_LOCKS: '0' };
  try {
    for (const args of [['read-tree', 'HEAD'], ['add', '-A']]) {
      const result = spawnSync('git', args, { cwd: runSpec.winRepo, env, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
      assert.equal(result.status, 0, `${runSpec.id}: git ${args[0]} failed: ${result.stderr}`);
    }
    const tree = spawnSync('git', ['write-tree'], { cwd: runSpec.winRepo, env, encoding: 'utf8' });
    assert.equal(tree.status, 0, `${runSpec.id}: git write-tree failed: ${tree.stderr}`);
    assert.equal(tree.stdout.trim(), runSpec.tree, `${runSpec.id}: measurement tree changed`);
    assert.equal(spawnSync('git', ['rev-parse', 'HEAD'], { cwd: runSpec.winRepo, encoding: 'utf8' }).stdout.trim(), runSpec.commit, `${runSpec.id}: HEAD changed`);
    const baseTree = spawnSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: runSpec.winRepo, encoding: 'utf8' });
    assert.equal(baseTree.status, 0, `${runSpec.id}: base tree lookup failed: ${baseTree.stderr}`);
    assert.equal(baseTree.stdout.trim(), runSpec.id === 'before' ? identity.measurement_source_snapshot.baseline_base_tree : identity.measurement_source_snapshot.candidate_base_tree, `${runSpec.id}: base tree changed`);
    const harness = fs.readFileSync(path.join(runSpec.winRepo, 'examples/d1_retained_pin_linux_oracle.rs'));
    assert.equal(createHash('sha256').update(harness).digest('hex'), identity.measurement_source_snapshot.retained_pin_linux_oracle_sha256, `${runSpec.id}: Linux oracle source changed`);
  } finally {
    if (fs.existsSync(index)) fs.rmSync(index);
  }
}
fs.writeFileSync(samplesPath, '');
for (const runSpec of runs) {
  const build = wsl(`cd ${shQuote(runSpec.repo)} && CARGO_TARGET_DIR=${shQuote(runSpec.target)} cargo build --release --example d1_retained_pin_linux_oracle --features ${shQuote(runSpec.features)}`);
  fs.writeFileSync(path.join(outRoot, `${runSpec.id}-linux-retained-pin-build.log`), `${build.stdout ?? ''}${build.stderr ?? ''}\nexit=${build.status}\n`);
  assert.equal(build.status, 0, `${runSpec.id}: Linux oracle build failed: ${build.stderr}`);
}
const rows = [];
for (let block = 0; block < pairsN; block++) {
  for (const arm of ['after', 'before', 'before', 'after']) {
    const spec = runs.find((run) => run.id === arm);
    const start = process.hrtime.bigint();
    const result = wsl(`${shQuote(`${spec.target}/release/examples/d1_retained_pin_linux_oracle`)}`);
    const hostInvocationNs = Number(process.hrtime.bigint() - start);
    assert.equal(result.status, 0, `${arm}/block${block}: oracle failed: ${result.stderr}`);
    const values = parse(result.stdout, spec);
    const row = { arm, block, slot: arm === 'after' ? 'A' : 'B', order: 'A/B/B/A', commit: spec.commit, measurement_tree: spec.tree, features: spec.features, host_invocation_ns: hostInvocationNs, metrics: values, stdout: result.stdout };
    rows.push(row);
    fs.appendFileSync(samplesPath, JSON.stringify(row) + '\n');
  }
}
const stages = ['before', 'root', 'registration', 'owner_unlink_pin_held', 'last_pin_drop', 'root_free'];
const metrics = [];
for (const field of ['rss', 'vm_size', 'pss', 'private_dirty']) for (const stage of stages) {
  const key = `${field}_${stage}_kib`;
  const before = rows.filter((row) => row.arm === 'before').map((row) => row.metrics[key]);
  const after = rows.filter((row) => row.arm === 'after').map((row) => row.metrics[key]);
  const deltas = Array.from({ length: pairsN }, (_, block) => {
    const blockRows = rows.filter((row) => row.block === block);
    return mean(blockRows.filter((row) => row.arm === 'after').map((row) => row.metrics[key])) - mean(blockRows.filter((row) => row.arm === 'before').map((row) => row.metrics[key]));
  });
  metrics.push({ metric: key, before_median_kib: median(before), after_median_kib: median(after), paired: tSummary(deltas) });
}
const summary = { source_identity: identity.measurement_source_snapshot, host: identity.host, pairs: pairsN, process_boundary: identity.process_boundary, note: 'host_invocation_ns includes WSL startup and is not treated as allocator or oracle latency', oracle: 'sidecar allocation and last-pin exact-free assertions passed in every process', metrics };
fs.writeFileSync(path.join(outRoot, 'linux_retained_pin_summary.json'), JSON.stringify(summary, null, 2) + '\n');
const csvRows = metrics.map((row) => [row.metric, row.before_median_kib, row.after_median_kib, row.paired.mean_delta, row.paired.ci95_low, row.paired.ci95_high, row.paired.t, row.paired.significant_p_lt_005_approx].join(','));
fs.writeFileSync(path.join(outRoot, 'linux_retained_pin_summary.csv'), ['metric,before_median_kib,after_median_kib,paired_mean_delta_after_minus_before_kib,ci95_low_kib,ci95_high_kib,t,significant_p_lt_005_approx', ...csvRows].join('\n') + '\n');
console.log(`PASS: ${rows.length} fresh WSL Linux processes; retained-pin sidecar lifecycle assertions passed.`);
