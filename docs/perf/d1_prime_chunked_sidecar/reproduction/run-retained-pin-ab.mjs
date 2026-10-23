import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const outRoot = path.join(root, 'raw/d1-registration/run6');
const identity = JSON.parse(fs.readFileSync(path.join(outRoot, 'source_identity.json'), 'utf8'));
const runs = [
  { id: 'before', repo: path.join(root, 'before-repo'), target: path.join(root, 'target-retained-before'), commit: identity.measurement_source_snapshot.baseline_commit, baseTree: identity.measurement_source_snapshot.baseline_base_tree, measurementTree: identity.measurement_source_snapshot.baseline_measurement_tree, features: 'production internals' },
  { id: 'after', repo: path.resolve(root, '../..'), target: path.join(root, 'target-retained-after'), commit: identity.measurement_source_snapshot.candidate_commit, baseTree: identity.measurement_source_snapshot.candidate_base_tree, measurementTree: identity.measurement_source_snapshot.candidate_measurement_tree, features: 'production internals bench-internals' },
];
const pairsN = 20;
const rawPath = path.join(outRoot, 'retained_pin_samples.ndjson');
assert.ok(!fs.existsSync(rawPath), `refusing to overwrite ${rawPath}`);

function run(command, args, cwd, env) {
  return spawnSync(command, args, { cwd, env, encoding: 'utf8', maxBuffer: 16 * 1024 * 1024, timeout: 600_000 });
}
function measurementTree(runSpec) {
  const indexPath = path.join(root, `source-verify-retained-${runSpec.id}.index`);
  assert.ok(!fs.existsSync(indexPath), `refusing to reuse ${indexPath}`);
  const env = { ...process.env, GIT_INDEX_FILE: indexPath, GIT_OPTIONAL_LOCKS: '0' };
  try {
    for (const args of [['read-tree', 'HEAD'], ['add', '-A']]) {
      const result = spawnSync('git', args, { cwd: runSpec.repo, env, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
      assert.equal(result.status, 0, `${runSpec.id}: git ${args[0]} failed: ${result.stderr}`);
    }
    const tree = spawnSync('git', ['write-tree'], { cwd: runSpec.repo, env, encoding: 'utf8' });
    assert.equal(tree.status, 0, `${runSpec.id}: git write-tree failed`);
    return tree.stdout.trim();
  } finally {
    if (fs.existsSync(indexPath)) fs.rmSync(indexPath);
  }
}
function verifySource(runSpec) {
  assert.equal(run('git', ['rev-parse', 'HEAD'], runSpec.repo).stdout.trim(), runSpec.commit, `${runSpec.id}: HEAD changed`);
  assert.equal(run('git', ['rev-parse', 'HEAD^{tree}'], runSpec.repo).stdout.trim(), runSpec.baseTree, `${runSpec.id}: commit tree changed`);
  assert.equal(measurementTree(runSpec), runSpec.measurementTree, `${runSpec.id}: measurement tree changed`);
  const source = fs.readFileSync(path.join(runSpec.repo, 'examples/d1_retained_pin_memory_oracle.rs'));
  assert.equal(createHash('sha256').update(source).digest('hex'), identity.measurement_source_snapshot.retained_pin_harness_sha256, `${runSpec.id}: pin probe source drift`);
}
function parse(stdout, runSpec) {
  const values = {};
  for (const line of stdout.split(/\r?\n/)) {
    const match = /^RESULT\s+([a-z0-9_]+)=(\d+)$/.exec(line.trim());
    if (match) values[match[1]] = Number(match[2]);
  }
  const required = [
    'sidecar_size_bytes', 'sidecar_alloc_bytes', 'sidecar_free_bytes_after_owner_unlink',
    'sidecar_free_bytes_after_last_pin', 'sidecar_alloc_calls_after_last_pin', 'sidecar_free_calls_after_last_pin',
    'rss_before_kib', 'rss_after_root_kib', 'rss_after_registration_kib', 'rss_after_owner_unlink_pin_held_kib', 'rss_after_last_pin_drop_kib', 'rss_after_root_free_kib',
    'commit_before_kib', 'commit_after_root_kib', 'commit_after_registration_kib', 'commit_after_owner_unlink_pin_held_kib', 'commit_after_last_pin_drop_kib', 'commit_after_root_free_kib',
  ];
  for (const key of required) assert.ok(Number.isFinite(values[key]), `${runSpec.id}: missing RESULT ${key}`);
  assert.equal(values.sidecar_alloc_bytes, values.sidecar_size_bytes);
  assert.equal(values.sidecar_free_bytes_after_owner_unlink, 0, 'owner unlink freed a still-pinned sidecar');
  assert.equal(values.sidecar_free_bytes_after_last_pin, values.sidecar_alloc_bytes, 'last pin did not free exact sidecar bytes');
  assert.equal(values.sidecar_alloc_calls_after_last_pin, 1);
  assert.equal(values.sidecar_free_calls_after_last_pin, 1);
  if (runSpec.id === 'after') {
    assert.equal(values.constructed_chunks_while_pinned, 1);
    assert.equal(values.pin_refs_before_unlink, 2);
    assert.equal(values.pin_refs_after_unlink, 1);
  }
  return values;
}
function mean(values) { return values.reduce((sum, value) => sum + value, 0) / values.length; }
function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
function tSummary(deltas) {
  const n = deltas.length;
  const avg = mean(deltas);
  const variance = deltas.reduce((sum, value) => sum + (value - avg) ** 2, 0) / (n - 1);
  const sd = Math.sqrt(variance);
  const se = sd / Math.sqrt(n);
  const crit = n >= 20 ? 2.093 : 2.262;
  return { n, mean_delta: avg, sd_delta: sd, se_delta: se, ci95_low: avg - crit * se, ci95_high: avg + crit * se, t: se === 0 ? 0 : avg / se, critical_t_005: crit, significant_p_lt_005_approx: se > 0 && Math.abs(avg / se) > crit };
}
function ratio(numerator, denominator, label) {
  assert.ok(Number.isFinite(numerator) && numerator > 0 && Number.isFinite(denominator) && denominator > 0, `${label}: invalid ratio input`);
  const result = numerator / denominator;
  assert.ok(Math.abs(result * denominator - numerator) <= Math.max(1, numerator) * 1e-12, `${label}: ratio arithmetic mismatch`);
  return { numerator, denominator, ratio: result };
}

for (const runSpec of runs) verifySource(runSpec);
fs.writeFileSync(rawPath, '');
const buildLogs = [];
for (const runSpec of runs) {
  const build = run('cargo', ['build', '--release', '--example', 'd1_retained_pin_memory_oracle', '--features', runSpec.features], runSpec.repo, { ...process.env, CARGO_TARGET_DIR: runSpec.target });
  const buildLog = path.join(outRoot, `${runSpec.id}-retained-pin-build.log`);
  fs.writeFileSync(buildLog, `${build.stdout ?? ''}${build.stderr ?? ''}\nexit=${build.status}\n`);
  assert.equal(build.status, 0, `${runSpec.id}: retained-pin oracle build failed`);
  buildLogs.push(path.basename(buildLog));
  runSpec.exe = path.join(runSpec.target, 'release/examples/d1_retained_pin_memory_oracle.exe');
}

const rawSamples = [];
const paired = [];
for (let block = 0; block < pairsN; block++) {
  const byArm = { before: [], after: [] };
  for (const arm of ['after', 'before', 'before', 'after']) {
    const runSpec = runs.find((item) => item.id === arm);
    const start = process.hrtime.bigint();
    const child = spawnSync(runSpec.exe, [], { encoding: 'utf8', maxBuffer: 2 * 1024 * 1024, timeout: 120_000 });
    const processWall = Number(process.hrtime.bigint() - start);
    assert.equal(child.status, 0, `${arm}/block${block}: oracle process failed`);
    const values = parse(child.stdout, runSpec);
    const row = { arm, commit: runSpec.commit, tree: runSpec.measurementTree, features: runSpec.features, block, slot: arm === 'after' ? 'A' : 'B', order: 'A/B/B/A', process_wall_ns: processWall, ...values, stdout: child.stdout };
    rawSamples.push(row);
    fs.appendFileSync(rawPath, JSON.stringify(row) + '\n');
    byArm[arm].push(row);
  }
  const keys = ['rss_before_kib', 'rss_after_root_kib', 'rss_after_registration_kib', 'rss_after_owner_unlink_pin_held_kib', 'rss_after_last_pin_drop_kib', 'rss_after_root_free_kib', 'commit_before_kib', 'commit_after_root_kib', 'commit_after_registration_kib', 'commit_after_owner_unlink_pin_held_kib', 'commit_after_last_pin_drop_kib', 'commit_after_root_free_kib', 'sidecar_size_bytes', 'sidecar_alloc_bytes'];
  const pair = { block };
  for (const key of keys) {
    pair[key] = {
      before: mean(byArm.before.map((row) => row[key])),
      after: mean(byArm.after.map((row) => row[key])),
    };
  }
  paired.push(pair);
}

const comparison = [];
for (const key of ['rss_before_kib', 'rss_after_root_kib', 'rss_after_registration_kib', 'rss_after_owner_unlink_pin_held_kib', 'rss_after_last_pin_drop_kib', 'rss_after_root_free_kib', 'commit_before_kib', 'commit_after_root_kib', 'commit_after_registration_kib', 'commit_after_owner_unlink_pin_held_kib', 'commit_after_last_pin_drop_kib', 'commit_after_root_free_kib', 'sidecar_size_bytes', 'sidecar_alloc_bytes']) {
  const before = rawSamples.filter((row) => row.arm === 'before').map((row) => row[key]);
  const after = rawSamples.filter((row) => row.arm === 'after').map((row) => row[key]);
  const deltas = paired.map((row) => row[key].after - row[key].before);
  const beforeMedian = median(before);
  const afterMedian = median(after);
  comparison.push({ key, before_median: beforeMedian, after_median: afterMedian, after_over_before_median: ratio(afterMedian, beforeMedian, key), paired_delta_after_minus_before: tSummary(deltas) });
}
const summary = {
  source_identity: identity.measurement_source_snapshot,
  pairs: pairsN,
  order: 'after/before/before/after',
  builds: buildLogs,
  oracle_contract: 'owner unlink removes lookup visibility; retained pin keeps sidecar system allocation alive; last-pin drop frees exact sidecar allocation',
  comparison,
};
fs.writeFileSync(path.join(outRoot, 'retained_pin_summary.json'), JSON.stringify(summary, null, 2) + '\n');
const csv = (value) => {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
};
const header = ['metric', 'before_median', 'after_median', 'after_over_before_numerator', 'after_over_before_denominator', 'after_over_before_ratio', 'paired_mean_delta_after_minus_before', 'ci95_low', 'ci95_high', 't', 'critical_t_005', 'significant_p_lt_005_approx'];
const rows = comparison.map((item) => [item.key, item.before_median, item.after_median, item.after_over_before_median.numerator, item.after_over_before_median.denominator, item.after_over_before_median.ratio, item.paired_delta_after_minus_before.mean_delta, item.paired_delta_after_minus_before.ci95_low, item.paired_delta_after_minus_before.ci95_high, item.paired_delta_after_minus_before.t, item.paired_delta_after_minus_before.critical_t_005, item.paired_delta_after_minus_before.significant_p_lt_005_approx].map(csv).join(','));
fs.writeFileSync(path.join(outRoot, 'retained_pin_summary.csv'), [header.join(','), ...rows].join('\n') + '\n');
console.log(`PASS: ${rawSamples.length} fresh-process retained-pin samples; sidecar lifetime oracles all passed.`);
