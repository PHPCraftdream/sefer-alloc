import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const outRoot = path.join(root, 'raw/dealloc-only-ab/run1');
const runs = [
  { id: 'before', repo: path.join(root, 'before-repo'), target: path.join(root, 'target-first-before'), commit: 'f80c681b799ef4ef7fa08d79c06896fdf1104fe9', tree: 'b5d22c5a770e503750bb549152245325d915f350' },
  { id: 'after', repo: path.resolve(root, '../..'), target: path.join(root, 'target-first-after'), commit: '734395b7bb12bdd21f605c07de5426b2e4e717ef', tree: '89bfd1c99ff3f528b1d9d1e93c468029c3e82496' },
];
const samplePairs = 10;
const t = 8;
const blocks = [30, 60, 120];
const modes = ['treatment', 'control'];
const needed = ['mode', 'b', 't', 'rss_before_kib', 'rss_after_kib', 'rss_after_join_kib', 'commit_before_kib', 'commit_after_kib', 'commit_after_join_kib', 'first_dealloc_latency_ns', 'steady_dealloc_latency_ns', 'heaps_claimed_high_water', 'segments_reserved_before', 'segments_reserved_after'];

function parse(stdout, run, b, mode) {
  const result = {};
  for (const line of stdout.split(/\r?\n/)) {
    const match = /^RESULT\s+([a-z0-9_]+)=(\w+)$/.exec(line.trim());
    if (!match) continue;
    result[match[1]] = /^\d+$/.test(match[2]) ? Number(match[2]) : match[2];
  }
  for (const key of needed) assert.ok(result[key] != null, `${run.id}/${b}/${mode}: missing RESULT ${key}`);
  assert.equal(result.b, b);
  assert.equal(result.t, t);
  assert.equal(result.mode, mode);
  assert.ok(result.heaps_claimed_high_water >= 1, `${run.id}: owner heap was not activated`);
  assert.ok(result.segments_reserved_after > 0, `${run.id}: no real segment reservation observed`);
  return result;
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
function pctRatio(num, den, label) {
  assert.ok(Number.isFinite(num) && num > 0 && Number.isFinite(den) && den > 0, `${label}: bad ratio inputs`);
  const value = num / den;
  assert.ok(Math.abs(value * den - num) <= Math.max(1, num) * 1e-12, `${label}: ratio mismatch`);
  return { numerator: num, denominator: den, ratio: value };
}
function summarizeMetric(rows, key) {
  const before = rows.filter((row) => row.arm === 'before').map((row) => row[key]);
  const after = rows.filter((row) => row.arm === 'after').map((row) => row[key]);
  const pairs = [];
  for (let block = 0; block < samplePairs; block++) {
    const beforeBlock = rows.filter((row) => row.block === block && row.arm === 'before');
    const afterBlock = rows.filter((row) => row.block === block && row.arm === 'after');
    assert.equal(beforeBlock.length, 2, `${key}: expected two baseline samples in block ${block}`);
    assert.equal(afterBlock.length, 2, `${key}: expected two candidate samples in block ${block}`);
    pairs.push({ before: mean(beforeBlock.map((row) => row[key])), after: mean(afterBlock.map((row) => row[key])) });
  }
  assert.equal(before.length, samplePairs * 2);
  assert.equal(after.length, samplePairs * 2);
  const deltas = pairs.map((pair) => pair.after - pair.before);
  const beforeMean = mean(before);
  const afterMean = mean(after);
  const beforeMedian = median(before);
  const afterMedian = median(after);
  return {
    before_mean: beforeMean,
    after_mean: afterMean,
    after_over_before_mean: pctRatio(afterMean, beforeMean, `${key}/after-before`),
    before_median: beforeMedian,
    after_median: afterMedian,
    after_over_before_median: pctRatio(afterMedian, beforeMedian, `${key}/median-after-before`),
    paired_delta_after_minus_before: tSummary(deltas),
  };
}

fs.mkdirSync(outRoot, { recursive: true });
const rawPath = path.join(outRoot, 'samples.ndjson');
assert.ok(!fs.existsSync(rawPath), `refusing to overwrite ${rawPath}`);
fs.writeFileSync(rawPath, '');
for (const run of runs) {
  assert.equal(spawnSync('git', ['rev-parse', 'HEAD'], { cwd: run.repo, encoding: 'utf8' }).stdout.trim(), run.commit);
  assert.equal(spawnSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: run.repo, encoding: 'utf8' }).stdout.trim(), run.tree);
}
const builds = [];
for (const run of runs) {
  const build = spawnSync('cargo', ['build', '--release', '--example', 'dealloc_only_unbound_thread', '--features', 'production internals'], { cwd: run.repo, env: { ...process.env, CARGO_TARGET_DIR: run.target }, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  const log = `${build.stdout ?? ''}${build.stderr ?? ''}\nexit=${build.status}\n`;
  const logPath = path.join(outRoot, `${run.id}-build.log`);
  fs.writeFileSync(logPath, log);
  assert.equal(build.status, 0, `${run.id}: dealloc example build failed`);
  builds.push(logPath);
}

const exe = (run) => path.join(run.target, 'release/examples/dealloc_only_unbound_thread.exe');
const results = [];
for (const b of blocks) {
  for (const mode of modes) {
    for (let block = 0; block < samplePairs; block++) {
      for (const slot of ['A', 'B', 'B', 'A']) {
        const run = slot === 'A' ? runs[0] : runs[1];
        const started = process.hrtime.bigint();
        const child = spawnSync(exe(run), [String(b), String(t), mode], { encoding: 'utf8', maxBuffer: 2 * 1024 * 1024, timeout: 120_000, env: { ...process.env, CARGO_TARGET_DIR: run.target } });
        const processWall = Number(process.hrtime.bigint() - started);
        assert.equal(child.status, 0, `${run.id}/B=${b}/T=${t}/${mode}: child failed`);
        const metrics = parse(child.stdout, run, b, mode);
        const row = { arm: run.id, commit: run.commit, tree: run.tree, features: 'production internals', b, t, mode, block, slot, process_wall_ns: processWall, ...metrics, stdout: child.stdout };
        results.push(row);
        fs.appendFileSync(rawPath, JSON.stringify(row) + '\n');
      }
    }
  }
}

const summary = [];
for (const b of blocks) {
  for (const mode of modes) {
    const rows = results.filter((row) => row.b === b && row.mode === mode);
    const metricRows = {};
    for (const key of ['first_dealloc_latency_ns', 'steady_dealloc_latency_ns', 'process_wall_ns', 'rss_after_kib', 'rss_after_join_kib', 'commit_after_kib', 'commit_after_join_kib']) {
      metricRows[key] = summarizeMetric(rows, key);
    }
    summary.push({ b, t, mode, order: 'before/after/after/before', sample_pairs_per_cell: samplePairs, activation: { observed_owner_heap: rows.every((row) => row.heaps_claimed_high_water >= 1), observed_segments: rows.every((row) => row.segments_reserved_after > 0) }, metrics: metricRows });
  }
}
fs.writeFileSync(path.join(outRoot, 'summary.json'), JSON.stringify({ features: 'production internals', before: { commit: runs[0].commit, tree: runs[0].tree }, after: { commit: runs[1].commit, tree: runs[1].tree }, builds: builds.map((file) => path.basename(file)), summary }, null, 2) + '\n');
const csv = (value) => {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
};
const columns = ['B', 'T', 'mode', 'metric', 'before_mean', 'after_mean', 'after_over_before_mean', 'before_median', 'after_median', 'after_over_before_median', 'paired_mean_delta_after_minus_before', 'CI95_low', 'CI95_high', 't', 'critical_t_005', 'significant_p_lt_005_approx'];
const csvRows = [];
for (const cell of summary) for (const [metric, row] of Object.entries(cell.metrics)) csvRows.push([
  cell.b, cell.t, cell.mode, metric, row.before_mean, row.after_mean, row.after_over_before_mean.ratio, row.before_median, row.after_median, row.after_over_before_median.ratio,
  row.paired_delta_after_minus_before.mean_delta, row.paired_delta_after_minus_before.ci95_low, row.paired_delta_after_minus_before.ci95_high,
  row.paired_delta_after_minus_before.t, row.paired_delta_after_minus_before.critical_t_005, row.paired_delta_after_minus_before.significant_p_lt_005_approx,
].map(csv).join(','));
fs.writeFileSync(path.join(outRoot, 'summary.csv'), [columns.join(','), ...csvRows].join('\n') + '\n');
console.log(`PASS: ${results.length} fresh-process samples; all treatment/control cells passed allocator-activation assertions.`);
