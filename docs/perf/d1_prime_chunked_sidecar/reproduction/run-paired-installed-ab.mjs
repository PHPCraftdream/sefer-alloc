import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const outRoot = path.join(root, 'raw/paired-installed-ab/run1');
const runs = [
  { id: 'before', exe: path.join(root, 'target-first-before/release/examples/paired_ab_sefer.exe'), commit: 'f80c681b799ef4ef7fa08d79c06896fdf1104fe9', tree: 'b5d22c5a770e503750bb549152245325d915f350', allocator: 'sefer' },
  { id: 'after', exe: path.join(root, 'target-first-after/release/examples/paired_ab_sefer.exe'), commit: '734395b7bb12bdd21f605c07de5426b2e4e717ef', tree: '89bfd1c99ff3f528b1d9d1e93c468029c3e82496', allocator: 'sefer' },
  { id: 'mimalloc', exe: path.join(root, 'target-first-after/release/examples/paired_ab_mimalloc.exe'), commit: '734395b7bb12bdd21f605c07de5426b2e4e717ef', tree: '89bfd1c99ff3f528b1d9d1e93c468029c3e82496', allocator: 'mimalloc' },
];
const byId = new Map(runs.map((run) => [run.id, run]));
const comparisons = [
  { id: 'after-before', a: 'after', b: 'before', pairs: 20 },
  { id: 'before-mimalloc', a: 'before', b: 'mimalloc', pairs: 20 },
  { id: 'after-mimalloc', a: 'after', b: 'mimalloc', pairs: 20 },
  { id: 'before-aa-control', a: 'before', b: 'before', pairs: 10 },
  { id: 'after-aa-control', a: 'after', b: 'after', pairs: 10 },
];
const required = ['elapsed_ns', 'segments_reserved_total', 'rss_after_kib', 'commit_after_kib'];
const rawFiles = [];

function parse(stdout, run) {
  const values = {};
  for (const line of stdout.split(/\r?\n/)) {
    const match = /^RESULT\s+([a-z0-9_]+)=(\d+)$/.exec(line.trim());
    if (match) values[match[1]] = Number(match[2]);
  }
  for (const key of required) assert.ok(Number.isFinite(values[key]), `${run.id}: missing RESULT ${key}`);
  assert.equal(values.segments_reserved_total > 0, run.allocator === 'sefer', `${run.id}: allocator activation oracle failed`);
  return values;
}
function mean(values) { return values.reduce((sum, value) => sum + value, 0) / values.length; }
function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}
function tTest(deltas) {
  const n = deltas.length;
  const avg = mean(deltas);
  const variance = deltas.reduce((sum, value) => sum + (value - avg) ** 2, 0) / (n - 1);
  const sd = Math.sqrt(variance);
  const se = sd / Math.sqrt(n);
  const critical = n >= 30 ? 2.045 : n >= 20 ? 2.093 : 2.262;
  return { n, mean_delta_ns: avg, sd_delta_ns: sd, se_delta_ns: se, critical_t_005_approx: critical, ci95_low_ns: avg - critical * se, ci95_high_ns: avg + critical * se, t: se === 0 ? (avg === 0 ? 0 : Infinity) : avg / se, significant_p_lt_005_approx: se > 0 && Math.abs(avg / se) > critical };
}
function ratio(numerator, denominator, label) {
  assert.ok(Number.isFinite(numerator) && numerator > 0 && Number.isFinite(denominator) && denominator > 0, `${label}: invalid ratio inputs`);
  const value = numerator / denominator;
  assert.ok(Math.abs(value * denominator - numerator) <= Math.max(1, numerator) * 1e-12, `${label}: ratio arithmetic mismatch`);
  return { numerator, denominator, ratio: value };
}

fs.mkdirSync(outRoot, { recursive: true });
const rawPath = path.join(outRoot, 'samples.ndjson');
assert.ok(!fs.existsSync(rawPath), `refusing to overwrite ${rawPath}`);
fs.writeFileSync(rawPath, '');
const allSummary = [];
for (const comparison of comparisons) {
  const aRun = byId.get(comparison.a);
  const bRun = byId.get(comparison.b);
  const raw = [];
  const pairs = [];
  for (let block = 0; block < comparison.pairs; block++) {
    const blockRows = { A: [], B: [] };
    for (const slot of ['A', 'B', 'B', 'A']) {
      const run = slot === 'A' ? aRun : bRun;
      const started = process.hrtime.bigint();
      const child = spawnSync(run.exe, [], { encoding: 'utf8', maxBuffer: 2 * 1024 * 1024, timeout: 120_000 });
      const processNs = Number(process.hrtime.bigint() - started);
      assert.equal(child.status, 0, `${comparison.id}/${block}/${slot}: child failed`);
      const result = parse(child.stdout, run);
      const sample = { comparison: comparison.id, block, slot, arm: run.id, commit: run.commit, tree: run.tree, features: 'production internals', process_wall_ns: processNs, ...result, stdout: child.stdout };
      raw.push(sample);
      fs.appendFileSync(rawPath, JSON.stringify(sample) + '\n');
      blockRows[slot].push(sample);
    }
    const a = blockRows.A.reduce((s, row) => s + row.elapsed_ns, 0) / blockRows.A.length;
    const b = blockRows.B.reduce((s, row) => s + row.elapsed_ns, 0) / blockRows.B.length;
    const aProcess = mean(blockRows.A.map((row) => row.process_wall_ns));
    const bProcess = mean(blockRows.B.map((row) => row.process_wall_ns));
    pairs.push({ block, a_internal_ns: a, b_internal_ns: b, delta_internal_ns: a - b, a_process_ns: aProcess, b_process_ns: bProcess, delta_process_ns: aProcess - bProcess });
  }
  const internalStats = tTest(pairs.map((pair) => pair.delta_internal_ns));
  const processStats = tTest(pairs.map((pair) => pair.delta_process_ns));
  const aInternal = mean(pairs.map((pair) => pair.a_internal_ns));
  const bInternal = mean(pairs.map((pair) => pair.b_internal_ns));
  const aProcess = mean(pairs.map((pair) => pair.a_process_ns));
  const bProcess = mean(pairs.map((pair) => pair.b_process_ns));
  const summary = {
    comparison: comparison.id,
    a: { id: aRun.id, commit: aRun.commit, tree: aRun.tree },
    b: { id: bRun.id, commit: bRun.commit, tree: bRun.tree },
    order: 'A/B/B/A',
    pairs: comparison.pairs,
    internal_elapsed_ns: { a_mean: aInternal, b_mean: bInternal, a_over_b: ratio(aInternal, bInternal, `${comparison.id}/internal`), paired_t: internalStats },
    full_process_wall_ns: { a_mean: aProcess, b_mean: bProcess, a_over_b: ratio(aProcess, bProcess, `${comparison.id}/process`), paired_t: processStats },
    paired_rows: pairs,
    sample_count: raw.length,
  };
  const file = path.join(outRoot, `${comparison.id}.json`);
  fs.writeFileSync(file, JSON.stringify({ summary, raw }, null, 2) + '\n');
  rawFiles.push(file);
  allSummary.push(summary);
  console.log(`${comparison.id}: ${comparison.pairs} A/B/B/A blocks; saved ${raw.length} process samples`);
}
fs.writeFileSync(path.join(outRoot, 'index.json'), JSON.stringify({ features: 'production internals', raw_files: ['samples.ndjson', ...rawFiles.map((file) => path.basename(file))], summary: allSummary }, null, 2) + '\n');
const csv = (value) => {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
};
const columns = [
  'comparison', 'arm_A', 'arm_B', 'A_commit', 'A_tree', 'B_commit', 'B_tree', 'features', 'order', 'pairs', 'sample_count',
  'internal_A_mean_ns', 'internal_B_mean_ns', 'A_over_B_internal_ratio', 'internal_delta_mean_A_minus_B_ns', 'internal_CI95_low_ns', 'internal_CI95_high_ns', 'internal_t', 'internal_critical_t', 'internal_significant_p_lt_005_approx',
  'process_A_mean_ns', 'process_B_mean_ns', 'A_over_B_process_ratio', 'process_delta_mean_A_minus_B_ns', 'process_CI95_low_ns', 'process_CI95_high_ns', 'process_t', 'process_critical_t', 'process_significant_p_lt_005_approx',
];
const csvRows = allSummary.map((row) => [
  row.comparison, row.a.id, row.b.id, row.a.commit, row.a.tree, row.b.commit, row.b.tree, 'production internals', row.order, row.pairs, row.sample_count,
  row.internal_elapsed_ns.a_mean, row.internal_elapsed_ns.b_mean, row.internal_elapsed_ns.a_over_b.ratio, row.internal_elapsed_ns.paired_t.mean_delta_ns, row.internal_elapsed_ns.paired_t.ci95_low_ns, row.internal_elapsed_ns.paired_t.ci95_high_ns, row.internal_elapsed_ns.paired_t.t, row.internal_elapsed_ns.paired_t.critical_t_005_approx, row.internal_elapsed_ns.paired_t.significant_p_lt_005_approx,
  row.full_process_wall_ns.a_mean, row.full_process_wall_ns.b_mean, row.full_process_wall_ns.a_over_b.ratio, row.full_process_wall_ns.paired_t.mean_delta_ns, row.full_process_wall_ns.paired_t.ci95_low_ns, row.full_process_wall_ns.paired_t.ci95_high_ns, row.full_process_wall_ns.paired_t.t, row.full_process_wall_ns.paired_t.critical_t_005_approx, row.full_process_wall_ns.paired_t.significant_p_lt_005_approx,
]);
fs.writeFileSync(path.join(outRoot, 'summary.csv'), [columns.join(','), ...csvRows.map((row) => row.map(csv).join(','))].join('\n') + '\n');
console.log('PASS: all arms passed allocator-activation sanity; raw samples and paired summaries saved.');
