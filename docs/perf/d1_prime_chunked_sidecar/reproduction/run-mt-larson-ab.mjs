import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const outRoot = path.join(root, 'raw/mt-larson-ab/run2');
const runs = [
  { id: 'before', repo: path.join(root, 'before-repo'), target: path.join(root, 'target-first-before'), commit: 'f80c681b799ef4ef7fa08d79c06896fdf1104fe9', tree: 'b5d22c5a770e503750bb549152245325d915f350' },
  { id: 'after', repo: path.resolve(root, '../..'), target: path.join(root, 'target-first-after'), commit: '734395b7bb12bdd21f605c07de5426b2e4e717ef', tree: '89bfd1c99ff3f528b1d9d1e93c468029c3e82496' },
];
const expectedHarnessSha256 = '439e56cdf961e7090d64b7d57ef7516d8ec34701c68dd3ddb54001ab57fcad4c';
const expectedPatchSha256 = '743fef67ba3f7fb50e39710197d9d66ef5a7b8322d4542a3cb5c069b896b87f1';
for (const run of runs) {
  const harness = fs.readFileSync(path.join(run.repo, 'examples/ph6b_mt_ab.rs'));
  const diff = spawnSync('git', ['diff', '--binary', 'HEAD', '--', 'examples/ph6b_mt_ab.rs'], { cwd: run.repo, encoding: 'utf8' });
  assert.equal(diff.status, 0, `${run.id}: could not capture harness patch`);
  run.harness_sha256 = createHash('sha256').update(harness).digest('hex');
  run.harness_patch_sha256 = createHash('sha256').update(diff.stdout).digest('hex');
  assert.equal(run.harness_sha256, expectedHarnessSha256, `${run.id}: MT harness bytes changed`);
  assert.equal(run.harness_patch_sha256, expectedPatchSha256, `${run.id}: MT harness patch identity changed`);
}
const pairsN = 10;
const threads = [1, 2, 4, 8];
const workloads = ['larson', 'mstress'];
const rawPath = path.join(outRoot, 'samples.ndjson');
const exe = (run) => path.join(run.target, 'release/examples/ph6b_mt_ab.exe');

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
  const value = numerator / denominator;
  assert.ok(Math.abs(value * denominator - numerator) <= Math.max(1, numerator) * 1e-12, `${label}: arithmetic mismatch`);
  return { numerator, denominator, ratio: value };
}
function parse(stdout, run) {
  const cells = new Map();
  let segments = null;
  let conflicts = null;
  for (const line of stdout.split(/\r?\n/)) {
    let match = /^RESULT mt_ns workload=(larson|mstress) T=(\d+) ns=(\d+)$/.exec(line.trim());
    if (match) {
      const key = `${match[1]}/T${match[2]}`;
      cells.set(key, { workload: match[1], threads: Number(match[2]), ns: Number(match[3]) });
      continue;
    }
    match = /^RESULT mt_ops workload=(larson|mstress) T=(\d+) ops=(\d+)$/.exec(line.trim());
    if (match) {
      const cell = cells.get(`${match[1]}/T${match[2]}`);
      assert.ok(cell, `${run.id}: ops before latency row`);
      cell.ops = Number(match[3]);
      continue;
    }
    match = /^RESULT (segments_reserved_total|config_conflicts)=(\d+)$/.exec(line.trim());
    if (match) {
      if (match[1] === 'segments_reserved_total') segments = Number(match[2]);
      else conflicts = Number(match[2]);
    }
  }
  assert.equal(cells.size, 8, `${run.id}: expected 8 workload/thread cells`);
  assert.ok(segments > 0, `${run.id}: global allocator not activated`);
  assert.equal(conflicts, 0, `${run.id}: config conflict`);
  for (const cell of cells.values()) {
    const expected = (cell.workload === 'larson' ? 801_536 : 399_360) * cell.threads;
    assert.equal(cell.ops, expected, `${run.id}/${cell.workload}/T${cell.threads}: operation count mismatch`);
    cell.mops = cell.ops * 1000 / cell.ns;
  }
  return { cells: [...cells.values()], segments, conflicts };
}

fs.mkdirSync(outRoot, { recursive: true });
assert.ok(!fs.existsSync(rawPath), `refusing to overwrite ${rawPath}`);
fs.writeFileSync(rawPath, '');
for (const run of runs) {
  assert.equal(spawnSync('git', ['rev-parse', 'HEAD'], { cwd: run.repo, encoding: 'utf8' }).stdout.trim(), run.commit);
  assert.equal(spawnSync('git', ['rev-parse', 'HEAD^{tree}'], { cwd: run.repo, encoding: 'utf8' }).stdout.trim(), run.tree);
  const build = spawnSync('cargo', ['build', '--release', '--example', 'ph6b_mt_ab', '--features', 'production internals'], { cwd: run.repo, env: { ...process.env, CARGO_TARGET_DIR: run.target }, encoding: 'utf8', maxBuffer: 32 * 1024 * 1024 });
  fs.writeFileSync(path.join(outRoot, `${run.id}-build.log`), `${build.stdout ?? ''}${build.stderr ?? ''}\nexit=${build.status}\n`);
  assert.equal(build.status, 0, `${run.id}: MT example build failed`);
}

const rawSamples = [];
const pairRows = [];
const processRows = [];
for (let block = 0; block < pairsN; block++) {
  const cellsByArm = { before: [], after: [] };
  const processByArm = { before: [], after: [] };
  for (const arm of ['before', 'after', 'after', 'before']) {
    const run = runs.find((candidate) => candidate.id === arm);
    const started = process.hrtime.bigint();
    const child = spawnSync(exe(run), [], { encoding: 'utf8', maxBuffer: 2 * 1024 * 1024, timeout: 180_000, env: { ...process.env, CARGO_TARGET_DIR: run.target } });
    const processWall = Number(process.hrtime.bigint() - started);
    assert.equal(child.status, 0, `${arm}/block${block}: MT process failed`);
    const parsed = parse(child.stdout, run);
    const record = { arm, commit: run.commit, tree: run.tree, harness_sha256: run.harness_sha256, harness_patch_sha256: run.harness_patch_sha256, block, features: 'production internals', order: 'A/B/B/A', process_wall_ns: processWall, segments_reserved_total: parsed.segments, config_conflicts: parsed.conflicts, cells: parsed.cells, stdout: child.stdout };
    rawSamples.push(record);
    fs.appendFileSync(rawPath, JSON.stringify(record) + '\n');
    cellsByArm[arm].push(new Map(parsed.cells.map((cell) => [`${cell.workload}/T${cell.threads}`, cell])));
    processByArm[arm].push(processWall);
  }
  for (const workload of workloads) for (const t of threads) {
    const key = `${workload}/T${t}`;
    const beforeValues = cellsByArm.before.map((map) => map.get(key));
    const afterValues = cellsByArm.after.map((map) => map.get(key));
    pairRows.push({ block, workload, threads: t, before_ns: mean(beforeValues.map((row) => row.ns)), after_ns: mean(afterValues.map((row) => row.ns)), before_mops: mean(beforeValues.map((row) => row.mops)), after_mops: mean(afterValues.map((row) => row.mops)) });
  }
  processRows.push({ block, before_ns: mean(processByArm.before), after_ns: mean(processByArm.after) });
}

const summary = [];
for (const workload of workloads) for (const t of threads) {
  const rows = pairRows.filter((row) => row.workload === workload && row.threads === t);
  const beforeNs = median(rows.map((row) => row.before_ns));
  const afterNs = median(rows.map((row) => row.after_ns));
  const beforeMops = median(rows.map((row) => row.before_mops));
  const afterMops = median(rows.map((row) => row.after_mops));
  const nsDeltas = rows.map((row) => row.after_ns - row.before_ns);
  const mopsDeltas = rows.map((row) => row.after_mops - row.before_mops);
  summary.push({ workload, threads: t, unit: 'critical-path ns/cell; Mops/s = ops*1000/ns', pairs: pairsN, before_median_ns: beforeNs, after_median_ns: afterNs, after_over_before_latency_ratio: ratio(afterNs, beforeNs, `${workload}/T${t}/latency`), latency_delta_after_minus_before_ci95: tSummary(nsDeltas), before_median_mops: beforeMops, after_median_mops: afterMops, after_over_before_mops_ratio: ratio(afterMops, beforeMops, `${workload}/T${t}/throughput`), mops_delta_after_minus_before_ci95: tSummary(mopsDeltas) });
}
const processBefore = median(processRows.map((row) => row.before_ns));
const processAfter = median(processRows.map((row) => row.after_ns));
const processDelta = processRows.map((row) => row.after_ns - row.before_ns);
const processSummary = { before_median_ns: processBefore, after_median_ns: processAfter, after_over_before_ratio: ratio(processAfter, processBefore, 'full-process'), paired_delta_ci95: tSummary(processDelta) };
fs.writeFileSync(path.join(outRoot, 'summary.json'), JSON.stringify({ before: { commit: runs[0].commit, tree: runs[0].tree, harness_sha256: runs[0].harness_sha256, harness_patch_sha256: runs[0].harness_patch_sha256 }, after: { commit: runs[1].commit, tree: runs[1].tree, harness_sha256: runs[1].harness_sha256, harness_patch_sha256: runs[1].harness_patch_sha256 }, features: 'production internals', order: 'A/B/B/A', samples_per_arm: pairsN * 2, process_summary: processSummary, summary }, null, 2) + '\n');
const csv = (value) => {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
};
const header = ['workload', 'threads', 'before_median_ns', 'after_median_ns', 'after_over_before_latency_ratio', 'latency_delta_mean_after_minus_before_ns', 'latency_ci95_low_ns', 'latency_ci95_high_ns', 'latency_t', 'latency_significant_p_lt_005_approx', 'before_median_mops', 'after_median_mops', 'after_over_before_mops_ratio', 'mops_delta_mean_after_minus_before', 'mops_ci95_low', 'mops_ci95_high', 'mops_t', 'mops_significant_p_lt_005_approx'];
const outRows = summary.map((row) => [row.workload, row.threads, row.before_median_ns, row.after_median_ns, row.after_over_before_latency_ratio.ratio, row.latency_delta_after_minus_before_ci95.mean_delta, row.latency_delta_after_minus_before_ci95.ci95_low, row.latency_delta_after_minus_before_ci95.ci95_high, row.latency_delta_after_minus_before_ci95.t, row.latency_delta_after_minus_before_ci95.significant_p_lt_005_approx, row.before_median_mops, row.after_median_mops, row.after_over_before_mops_ratio.ratio, row.mops_delta_after_minus_before_ci95.mean_delta, row.mops_delta_after_minus_before_ci95.ci95_low, row.mops_delta_after_minus_before_ci95.ci95_high, row.mops_delta_after_minus_before_ci95.t, row.mops_delta_after_minus_before_ci95.significant_p_lt_005_approx]);
fs.writeFileSync(path.join(outRoot, 'summary.csv'), [header.join(','), ...outRows.map((row) => row.map(csv).join(','))].join('\n') + '\n');
console.log(`PASS: ${rawSamples.length} MT process samples; 8 cells per sample; operation and activation oracles all matched.`);
