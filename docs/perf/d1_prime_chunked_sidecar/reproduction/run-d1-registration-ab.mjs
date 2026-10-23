import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const outRoot = path.join(root, 'raw/d1-registration/run5');
const identity = JSON.parse(fs.readFileSync(path.join(outRoot, 'source_identity.json'), 'utf8'));
const runs = [
  { id: 'before', repo: path.join(root, 'before-repo'), target: path.join(root, 'target-first-before'), commit: identity.measurement_source_snapshot.baseline_commit, baseTree: identity.measurement_source_snapshot.baseline_base_tree, measureTree: identity.measurement_source_snapshot.baseline_measurement_tree },
  { id: 'after', repo: path.resolve(root, '../..'), target: path.join(root, 'target-first-after'), commit: identity.measurement_source_snapshot.candidate_commit, baseTree: identity.measurement_source_snapshot.candidate_base_tree, measureTree: identity.measurement_source_snapshot.candidate_measurement_tree },
];
const oracleSpecs = [
  { scenario: 'registration', cycles: 30 },
  { scenario: 'registration', cycles: 60 },
  { scenario: 'registration', cycles: 120 },
  { scenario: 'dense16', cycles: 1 },
  { scenario: 'sparse_max_small', cycles: 1 },
];
const pairsN = 10;
const timingSamplesPath = path.join(outRoot, 'timing_samples.ndjson');
const oracleSamplesPath = path.join(outRoot, 'oracle_samples.ndjson');
assert.ok(!fs.existsSync(timingSamplesPath) && !fs.existsSync(oracleSamplesPath), 'refusing to overwrite prior measurement samples');

function run(command, args, cwd, env) {
  return spawnSync(command, args, { cwd, env, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, timeout: 600_000 });
}
function parseResult(stdout) {
  const values = {};
  for (const line of stdout.split(/\r?\n/)) {
    const match = /^RESULT\s+([a-z0-9_]+)=(.*)$/.exec(line.trim());
    if (!match) continue;
    const value = match[2];
    values[match[1]] = /^\d+$/.test(value) ? Number(value) : value;
  }
  return values;
}
function measurementTree(runSpec) {
  const indexPath = path.join(root, `source-verify-${runSpec.id}.index`);
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

function assertSource(runSpec) {
  assert.equal(run('git', ['rev-parse', 'HEAD'], runSpec.repo).stdout.trim(), runSpec.commit, `${runSpec.id}: HEAD changed`);
  assert.equal(run('git', ['rev-parse', 'HEAD^{tree}'], runSpec.repo).stdout.trim(), runSpec.baseTree, `${runSpec.id}: committed tree changed`);
  assert.equal(measurementTree(runSpec), runSpec.measureTree, `${runSpec.id}: measurement tree changed`);
  const harnessPath = path.join(runSpec.repo, 'examples/d1_sidecar_registration_gate.rs');
  const harnessSha = createHash('sha256').update(fs.readFileSync(harnessPath)).digest('hex');
  assert.equal(harnessSha, identity.measurement_source_snapshot.registration_harness_sha256, `${runSpec.id}: harness bytes changed`);
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
  const value = numerator / denominator;
  assert.ok(Math.abs(value * denominator - numerator) <= Math.max(1, numerator) * 1e-12, `${label}: ratio arithmetic mismatch`);
  return { numerator, denominator, ratio: value };
}
function statRow(results, key) {
  const before = results.filter((row) => row.arm === 'before').map((row) => row.metrics[key]);
  const after = results.filter((row) => row.arm === 'after').map((row) => row.metrics[key]);
  const pairs = [];
  for (let block = 0; block < pairsN; block++) {
    const b = results.filter((row) => row.block === block && row.arm === 'before').map((row) => row.metrics[key]);
    const a = results.filter((row) => row.block === block && row.arm === 'after').map((row) => row.metrics[key]);
    assert.equal(b.length, 2, `${key}/block${block}: two baseline observations expected`);
    assert.equal(a.length, 2, `${key}/block${block}: two candidate observations expected`);
    pairs.push({ before: mean(b), after: mean(a) });
  }
  assert.equal(before.length, pairsN * 2);
  assert.equal(after.length, pairsN * 2);
  const beforeMedian = median(before);
  const afterMedian = median(after);
  return {
    before_mean: mean(before), after_mean: mean(after),
    before_median: beforeMedian, after_median: afterMedian,
    after_over_before_median: ratio(afterMedian, beforeMedian, key),
    paired_delta_after_minus_before: tSummary(pairs.map((pair) => pair.after - pair.before)),
  };
}

for (const runSpec of runs) assertSource(runSpec);
fs.writeFileSync(timingSamplesPath, '');
fs.writeFileSync(oracleSamplesPath, '');
const buildFiles = [];
for (const runSpec of runs) {
  for (const build of [
    { mode: 'timing', features: 'production', target: runSpec.target },
    { mode: 'oracle', features: runSpec.id === 'after' ? 'production internals bench-internals' : 'production internals', target: `${runSpec.target}-oracle` },
  ]) {
    const env = { ...process.env, CARGO_TARGET_DIR: build.target };
    const result = run('cargo', ['build', '--release', '--example', 'd1_sidecar_registration_gate', '--features', build.features], runSpec.repo, env);
    const logPath = path.join(outRoot, `${runSpec.id}-${build.mode}-build.log`);
    fs.writeFileSync(logPath, `${result.stdout ?? ''}${result.stderr ?? ''}\nexit=${result.status}\n`);
    assert.equal(result.status, 0, `${runSpec.id}/${build.mode} build failed`);
    buildFiles.push(logPath);
    const exeName = process.platform === 'win32' ? 'd1_sidecar_registration_gate.exe' : 'd1_sidecar_registration_gate';
    const binary = path.join(build.target, 'release/examples', exeName);
    const copyPath = path.join(outRoot, `${runSpec.id}-${build.mode}${process.platform === 'win32' ? '.exe' : ''}`);
    fs.copyFileSync(binary, copyPath);
    build.binary = copyPath;
  }
  runSpec.timingExe = path.join(outRoot, `${runSpec.id}-timing${process.platform === 'win32' ? '.exe' : ''}`);
  runSpec.oracleExe = path.join(outRoot, `${runSpec.id}-oracle${process.platform === 'win32' ? '.exe' : ''}`);
}

const oracleRows = [];
for (const runSpec of runs) for (const cell of oracleSpecs) {
  const result = run(runSpec.oracleExe, [cell.scenario, String(cell.cycles), 'oracle'], runSpec.repo, process.env);
  assert.equal(result.status, 0, `${runSpec.id}/${cell.scenario}/${cell.cycles}: oracle failed`);
  const values = parseResult(result.stdout);
  assert.equal(values.scenario, cell.scenario);
  assert.equal(values.cycles, cell.cycles);
  assert.ok(values.blocks_per_cycle > 0);
  assert.equal(values.total_blocks, values.blocks_per_cycle * cell.cycles);
  const rawRecord = { arm: runSpec.id, commit: runSpec.commit, measure_tree: runSpec.measureTree, features: runSpec.id === 'after' ? 'production internals bench-internals' : 'production internals', ...cell, stdout: result.stdout, results: values };
  fs.appendFileSync(oracleSamplesPath, JSON.stringify(rawRecord) + '\n');
  for (let cycle = 0; cycle < cell.cycles; cycle++) {
    assert.ok(values[`registrations_visible_${cycle}`] > 0, `${runSpec.id}/${cell.scenario}/${cycle}: no route registration activated`);
  }
  let events;
  if (runSpec.id === 'after') {
    for (let cycle = 0; cycle < cell.cycles; cycle++) {
      assert.ok(values[`constructed_chunks_${cycle}`] > 0, `${cell.scenario}: no chunk constructed`);
    }
    events = {
      registrations: Array.from({ length: cell.cycles }, (_, cycle) => values[`registrations_total_${cycle}`]).reduce((sum, value) => sum + value, 0),
      unlinks: Array.from({ length: cell.cycles }, (_, cycle) => values[`unlinks_total_${cycle}`]).reduce((sum, value) => sum + value, 0),
      measured_by: 'bench-internals counters',
    };
  } else {
    events = {
      registrations_visible: Array.from({ length: cell.cycles }, (_, cycle) => values[`registrations_visible_${cycle}`]).reduce((sum, value) => sum + value, 0),
      unlinks_visible: Array.from({ length: cell.cycles }, (_, cycle) => values[`unlinks_visible_${cycle}`]).reduce((sum, value) => sum + value, 0),
      measured_by: 'live-route census deltas',
    };
  }
  const registrationCount = events.registrations ?? events.registrations_visible;
  const unlinkCount = events.unlinks ?? events.unlinks_visible;
  events.path_activation_pass = registrationCount > 0 && (cell.scenario !== 'registration' || unlinkCount > 0);
  const row = { arm: runSpec.id, commit: runSpec.commit, measure_tree: runSpec.measureTree, features: runSpec.id === 'after' ? 'production internals bench-internals' : 'production internals', ...cell, events, stdout: result.stdout, results: values };
  oracleRows.push(row);
  assert.ok(events.path_activation_pass, `${runSpec.id}/${cell.scenario}/${cell.cycles}: required route path was not activated`);
}

const oracleSummary = oracleRows.map((row) => {
  const registrations = row.events.registrations ?? row.events.registrations_visible;
  const unlinks = row.events.unlinks ?? row.events.unlinks_visible;
  const chunks = Array.from({ length: row.cycles }, (_, cycle) => row.results[`constructed_chunks_${cycle}`]).filter(Number.isFinite);
  const logicalBytes = Array.from({ length: row.cycles }, (_, cycle) => row.results[`logical_payload_bytes_${cycle}`]).filter(Number.isFinite);
  const backingBytes = Array.from({ length: row.cycles }, (_, cycle) => row.results[`sidecar_backing_bytes_${cycle}`]).filter(Number.isFinite);
  const activeRoutes = Array.from({ length: row.cycles }, (_, cycle) => row.results[`active_sidecar_routes_${cycle}`] ?? row.results[`live_routes_during_${cycle}`] ?? 0).reduce((a, b) => a + b, 0);
  return {
    arm: row.arm,
    commit: row.commit,
    measurement_tree: row.measure_tree,
    features: row.features,
    scenario: row.scenario,
    cycles: row.cycles,
    blocks_per_cycle: row.results.blocks_per_cycle,
    block_size: row.results.block_size,
    activation_method: row.events.measured_by,
    actual_registrations: registrations,
    actual_unlinks: unlinks,
    registrations_per_cycle: registrations / row.cycles,
    unlinks_per_cycle: unlinks / row.cycles,
    active_routes_observed_sum: activeRoutes,
    constructed_chunks_sum: chunks.length ? chunks.reduce((a, b) => a + b, 0) : null,
    logical_initialized_payload_bytes_sum: logicalBytes.length ? logicalBytes.reduce((a, b) => a + b, 0) : null,
    sidecar_backing_bytes_sum: backingBytes.length ? backingBytes.reduce((a, b) => a + b, 0) : null,
    counters_path_activation_pass: row.events.path_activation_pass,
  };
});
const registrationLinearity = ['before', 'after'].map((arm) => {
  const rows = oracleSummary.filter((row) => row.arm === arm && row.scenario === 'registration').sort((a, b) => a.cycles - b.cycles);
  return {
    arm,
    cells: rows.map((row) => ({ cycles: row.cycles, registrations: row.actual_registrations, unlinks: row.actual_unlinks, registrations_per_cycle: row.registrations_per_cycle, unlinks_per_cycle: row.unlinks_per_cycle })),
    methodology: 'actual counter deltas on candidate oracle; live-route census deltas on baseline oracle',
  };
});

const metricKeys = ['work_per_cycle_mean_ns', 'work_first_cycle_ns', 'work_steady_cycle_median_ns', 'thread_lifecycle_per_cycle_mean_ns', 'process_wall_per_cycle_ns', 'process_wall_total_ns'];
const timingRows = [];
for (const cell of oracleSpecs) {
  for (let block = 0; block < pairsN; block++) {
    for (const arm of ['after', 'before', 'before', 'after']) {
      const runSpec = runs.find((item) => item.id === arm);
      const started = process.hrtime.bigint();
      const result = run(runSpec.timingExe, [cell.scenario, String(cell.cycles)], runSpec.repo, process.env);
      const processWallNs = Number(process.hrtime.bigint() - started);
      assert.equal(result.status, 0, `${arm}/${cell.scenario}/${cell.cycles}/block${block}: timing process failed`);
      const values = parseResult(result.stdout);
      assert.equal(values.scenario, cell.scenario);
      assert.equal(values.cycles, cell.cycles);
      assert.equal(values.total_blocks, values.blocks_per_cycle * cell.cycles);
      const workSamples = Array.from({ length: cell.cycles }, (_, i) => values[`work_ns_${i}`]);
      const cycleWallSamples = Array.from({ length: cell.cycles }, (_, i) => values[`cycle_wall_ns_${i}`]);
      assert.ok(workSamples.every((value) => Number.isFinite(value) && value > 0));
      assert.ok(cycleWallSamples.every((value, i) => Number.isFinite(value) && value >= workSamples[i]));
      const metrics = {
        work_per_cycle_mean_ns: values.total_work_ns / cell.cycles,
        work_first_cycle_ns: workSamples[0],
        work_steady_cycle_median_ns: cell.cycles > 1 ? median(workSamples.slice(1)) : workSamples[0],
        thread_lifecycle_per_cycle_mean_ns: values.total_cycle_wall_ns / cell.cycles,
        process_wall_per_cycle_ns: processWallNs / cell.cycles,
        process_wall_total_ns: processWallNs,
      };
      const row = { arm, commit: runSpec.commit, measure_tree: runSpec.measureTree, features: 'production', scenario: cell.scenario, cycles: cell.cycles, block, order: 'A/B/B/A', process_wall_ns: processWallNs, metrics, stdout: result.stdout };
      timingRows.push(row);
      fs.appendFileSync(timingSamplesPath, JSON.stringify(row) + '\n');
    }
  }
}

const summaries = [];
for (const cell of oracleSpecs) {
  const rows = timingRows.filter((row) => row.scenario === cell.scenario && row.cycles === cell.cycles);
  const metrics = {};
  for (const key of metricKeys) metrics[key] = statRow(rows, key);
  summaries.push({ ...cell, sample_pairs: pairsN, order: 'after/before/before/after', metrics });
}
const summary = {
  source_identity: identity.measurement_source_snapshot,
  features: { timing: 'production', oracle_before: 'production internals', oracle_after: 'production internals bench-internals' },
  host: identity.host,
  builds: buildFiles.map((file) => path.basename(file)),
  oracle: oracleRows,
  oracle_summary: oracleSummary,
  registration_linearity: registrationLinearity,
  timing: summaries,
};
fs.writeFileSync(path.join(outRoot, 'summary.json'), JSON.stringify(summary, null, 2) + '\n');
const csv = (value) => {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
};
const columns = ['scenario', 'cycles', 'metric', 'before_mean', 'after_mean', 'before_median', 'after_median', 'after_over_before_median_numerator', 'after_over_before_median_denominator', 'after_over_before_median_ratio', 'paired_mean_delta_after_minus_before', 'ci95_low', 'ci95_high', 't', 'critical_t_005', 'significant_p_lt_005_approx'];
const csvRows = [];
for (const cell of summaries) for (const [metric, values] of Object.entries(cell.metrics)) csvRows.push([
  cell.scenario, cell.cycles, metric, values.before_mean, values.after_mean, values.before_median, values.after_median,
  values.after_over_before_median.numerator, values.after_over_before_median.denominator, values.after_over_before_median.ratio,
  values.paired_delta_after_minus_before.mean_delta, values.paired_delta_after_minus_before.ci95_low, values.paired_delta_after_minus_before.ci95_high,
  values.paired_delta_after_minus_before.t, values.paired_delta_after_minus_before.critical_t_005, values.paired_delta_after_minus_before.significant_p_lt_005_approx,
].map(csv).join(','));
fs.writeFileSync(path.join(outRoot, 'summary.csv'), [columns.join(','), ...csvRows].join('\n') + '\n');
const oracleColumns = ['arm', 'commit', 'measurement_tree', 'features', 'scenario', 'cycles', 'blocks_per_cycle', 'block_size', 'activation_method', 'actual_registrations', 'actual_unlinks', 'registrations_per_cycle', 'unlinks_per_cycle', 'active_routes_observed_sum', 'constructed_chunks_sum', 'logical_initialized_payload_bytes_sum', 'sidecar_backing_bytes_sum', 'path_activation_pass'];
const oracleCsvRows = oracleSummary.map((row) => [row.arm, row.commit, row.measurement_tree, row.features, row.scenario, row.cycles, row.blocks_per_cycle, row.block_size, row.activation_method, row.actual_registrations, row.actual_unlinks, row.registrations_per_cycle, row.unlinks_per_cycle, row.active_routes_observed_sum, row.constructed_chunks_sum, row.logical_initialized_payload_bytes_sum, row.sidecar_backing_bytes_sum, row.counters_path_activation_pass].map(csv).join(','));
fs.writeFileSync(path.join(outRoot, 'oracle_summary.csv'), [oracleColumns.join(','), ...oracleCsvRows].join('\n') + '\n');
console.log(`PASS: ${oracleRows.length} oracle cells and ${timingRows.length} paired timing process runs; raw logs saved.`);
