import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifySamples } from './d1_prime_verify_samples.mjs';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const evidence = path.join(root, 'docs/perf/d1_prime_chunked_sidecar');
const summaries = path.join(evidence, 'summaries');
const reportPath = path.join(root, 'docs/perf/D1_PRIME_CHUNKED_SIDECAR_GATE.md');
const outputCsvPath = path.join(root, 'docs/perf/D1_PRIME_CHUNKED_SIDECAR_GATE_summary.csv');

function parseCsv(text) {
  const records = [];
  let record = [], field = '', quoted = false;
  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (quoted) {
      if (char === '"' && text[i + 1] === '"') { field += '"'; i++; }
      else if (char === '"') quoted = false;
      else field += char;
    } else if (char === '"') quoted = true;
    else if (char === ',') { record.push(field); field = ''; }
    else if (char === '\n') { record.push(field.replace(/\r$/, '')); records.push(record); record = []; field = ''; }
    else field += char;
  }
  if (field || record.length) { record.push(field.replace(/\r$/, '')); records.push(record); }
  assert.ok(records.length >= 2, 'CSV must include a header and rows');
  const [header, ...rows] = records;
  return rows.filter((row) => row.length).map((row) => Object.fromEntries(header.map((key, i) => [key, row[i] ?? ''])));
}
function csvFile(name) { return parseCsv(fs.readFileSync(path.join(summaries, name), 'utf8')); }
function jsonFile(name) { return JSON.parse(fs.readFileSync(path.join(summaries, name), 'utf8')); }
function finite(value, label) { assert.ok(value !== '' && value != null, `${label} is missing`); const number = Number(value); assert.ok(Number.isFinite(number), `${label} must be finite`); return number; }
function close(actual, expected, label) { assert.ok(Math.abs(actual - expected) <= Math.max(1, Math.abs(expected)) * 1e-10, `${label}: arithmetic mismatch`); }
function fmt(value, digits = 2) { return value === '' || value == null ? '—' : Number(value).toFixed(digits); }
function median(values) { const sorted = [...values].sort((a, b) => a - b); const middle = Math.floor(sorted.length / 2); return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2; }
function ratio(numerator, denominator, label) {
  const n = finite(numerator, `${label} numerator`), d = finite(denominator, `${label} denominator`);
  assert.notEqual(d, 0, `${label}: zero denominator`);
  const value = n / d;
  close(value * d, n, label);
  return value;
}
function esc(value) { return String(value ?? '').replaceAll('|', '\\|').replaceAll('\n', ' '); }
function table(headers, rows) {
  for (const row of rows) assert.equal(row.length, headers.length, 'Число ячеек таблицы не совпадает с заголовком');
  return [
    `| ${headers.join(' | ')} |`,
    `| ${headers.map(() => '---').join(' | ')} |`,
    ...rows.map((row) => `| ${row.map(esc).join(' | ')} |`),
  ].join('\n');
}
function by(rows, predicate, label) {
  const row = rows.find(predicate);
  assert.ok(row, `missing ${label}`);
  return row;
}
function makeSummaryRow(axis, workload, metric, unit, before, candidate, extra = {}) {
  const b = before === '' ? '' : finite(before, `${workload}/${metric} before`);
  const c = candidate === '' ? '' : finite(candidate, `${workload}/${metric} candidate`);
  const d = b === '' || c === '' ? '' : c - b;
  const r = b === '' || c === '' || b === 0 ? '' : ratio(c, b, `${workload}/${metric} candidate/before`);
  return { axis, workload, metric, unit, before: b, candidate: c, delta_candidate_minus_before: d, ratio_numerator: c, ratio_denominator: b, candidate_over_before: r, ci95_low: extra.ci95_low ?? '', ci95_high: extra.ci95_high ?? '', sample_count: extra.sample_count ?? '', before_over_mimalloc: extra.before_over_mimalloc ?? '', candidate_over_mimalloc: extra.candidate_over_mimalloc ?? '', source: extra.source ?? '' };
}

verifySamples(evidence, csvFile);
const wall = csvFile('wallclock_comparison.csv');
const wallPerRun = csvFile('wallclock_per_run.csv');
const iai = csvFile('iai_comparison.csv');
const marginal = csvFile('iai_marginal_comparison.csv');
const installed = csvFile('installed_paired_ab_summary.csv');
const mt = csvFile('mt_larson_run2_summary.csv');
const foreign = csvFile('foreign_free_ab_summary.csv');
const registration = csvFile('summary.csv');
const oracle = csvFile('oracle_summary.csv');
const windowsMemory = csvFile('retained_pin_summary.csv');
const linuxMemory = csvFile('linux_retained_pin_summary.csv');
const firstBefore = jsonFile('first_alloc_before_summary.json');
const firstAfter = jsonFile('first_alloc_after_summary.json');
const passportText = fs.readFileSync(path.join(evidence, 'reproduction/source_passport.txt'), 'utf8');
const passport = Object.fromEntries(passportText.split(/\r?\n/).filter((line) => line.includes('=')).map((line) => { const i = line.indexOf('='); return [line.slice(0, i), line.slice(i + 1)]; }));
const linuxIdentity = jsonFile('retained_pin_linux_wsl_source_identity.json');
const registrationIdentity = jsonFile('registration_source_identity.json').measurement_source_snapshot;
const windowsIdentity = jsonFile('retained_pin_windows_source_identity.json').measurement_source_snapshot;
assert.equal(passport.baseline_commit, 'f80c681b799ef4ef7fa08d79c06896fdf1104fe9');
assert.equal(passport.candidate_commit, '734395b7bb12bdd21f605c07de5426b2e4e717ef');
assert.equal(linuxIdentity.process_boundary, 'fresh WSL process per sample; pair order after/before/before/after');

const summaryRows = [];
const wallIds = [
  'global_alloc/{allocator}/16B',
  'global_alloc_churn/{allocator}/16B',
  'global_alloc_churn_write/{allocator}/16B',
  'global_alloc_churn_with_teardown/{allocator}/64B',
  'segment_decommit_cycle/{allocator}/253KiB',
];
const wallRunSpecs = [
  { id: 'before-clean-1', run: 'before-1', source: 'before' },
  { id: 'after-clean-1', run: 'after-1', source: 'after' },
  { id: 'after-clean-2', run: 'after-2', source: 'after' },
  { id: 'before-clean-2', run: 'before-2', source: 'before' },
];
function linearSlope(xs, ys) {
  const meanX = xs.reduce((sum, value) => sum + value, 0) / xs.length;
  const meanY = ys.reduce((sum, value) => sum + value, 0) / ys.length;
  let covariance = 0, variance = 0;
  for (let i = 0; i < xs.length; i++) { covariance += (xs[i] - meanX) * (ys[i] - meanY); variance += (xs[i] - meanX) ** 2; }
  assert.ok(variance > 0, 'Linear Criterion sample must have varying iteration counts');
  return covariance / variance;
}
function readWallSample(run, fullId) {
  const [group, allocator, size] = fullId.split('/');
  const dir = path.join(evidence, 'raw/criterion', run.id, group, `${allocator.toLowerCase()}_${size.toLowerCase()}`, 'new');
  const meta = JSON.parse(fs.readFileSync(path.join(dir, 'benchmark.json'), 'utf8'));
  const sample = JSON.parse(fs.readFileSync(path.join(dir, 'sample.json'), 'utf8'));
  const estimates = JSON.parse(fs.readFileSync(path.join(dir, 'estimates.json'), 'utf8'));
  assert.equal(meta.full_id, fullId, `${run.id}/${fullId}: benchmark ID mismatch`);
  assert.equal(sample.times.length, sample.iters.length, `${run.id}/${fullId}: sample lengths differ`);
  assert.ok(sample.times.length >= 10, `${run.id}/${fullId}: insufficient samples`);
  const scale = group === 'segment_decommit_cycle' ? 1 : 1024;
  const normalized = sample.times.map((time, index) => finite(time, `${run.id}/${fullId} time`) / finite(sample.iters[index], `${run.id}/${fullId} iterations`) / scale);
  const statistic = sample.sampling_mode === 'Linear' ? estimates.slope : sample.sampling_mode === 'Flat' ? estimates.mean : null;
  assert.ok(statistic?.point_estimate > 0, `${run.id}/${fullId}: unsupported Criterion sampling mode`);
  const rawSlope = (sample.sampling_mode === 'Linear' ? linearSlope(sample.iters, sample.times) : median(normalized)) / (sample.sampling_mode === 'Linear' ? scale : 1);
  return { rawSlope, criterion: statistic.point_estimate / scale };
}
const wallRows = wallIds.map((pattern) => {
  const [group, , size] = pattern.split('/');
  const idFor = (allocator) => pattern.replace('{allocator}', allocator);
  const perRun = wallRunSpecs.map((run) => ({ ...run, sefer: readWallSample(run, idFor('SeferAlloc')), mimalloc: readWallSample(run, idFor('mimalloc')), system: readWallSample(run, idFor('System')) }));
  const aggregate = (allocator, metric, source) => median(perRun.filter((run) => run.source === source).map((run) => run[allocator][metric]));
  const beforeRaw = aggregate('sefer', 'rawSlope', 'before'), afterRaw = aggregate('sefer', 'rawSlope', 'after');
  const beforeCriterion = aggregate('sefer', 'criterion', 'before'), afterCriterion = aggregate('sefer', 'criterion', 'after');
  const beforeMimallocRaw = aggregate('mimalloc', 'rawSlope', 'before'), afterMimallocRaw = aggregate('mimalloc', 'rawSlope', 'after');
  const beforeSystemRaw = aggregate('system', 'rawSlope', 'before'), afterSystemRaw = aggregate('system', 'rawSlope', 'after');
  const beforeVsMimalloc = median(perRun.filter((run) => run.source === 'before').map((run) => ratio(run.sefer.criterion, run.mimalloc.criterion, `${pattern}/${run.run} baseline Sefer/mimalloc Criterion ratio`)));
  const afterVsMimalloc = median(perRun.filter((run) => run.source === 'after').map((run) => ratio(run.sefer.criterion, run.mimalloc.criterion, `${pattern}/${run.run} candidate Sefer/mimalloc Criterion ratio`)));
  const row = by(wall, (item) => item.workload_id === pattern, `wallclock CSV ${pattern}`);
  const sourceRows = perRun.map((run) => run);
  for (const run of sourceRows) for (const [allocator, name] of [['sefer', 'SeferAlloc'], ['mimalloc', 'mimalloc'], ['system', 'System']]) {
    const stored = by(wallPerRun, (item) => item.run === run.run && item.benchmark_id === idFor(name), `per-run cross-check ${run.run}/${idFor(name)}`);
    close(run[allocator].rawSlope, stored.raw_ols_slope_ns_per_op, `${run.run}/${name} raw Criterion sample cross-check`);
    close(run[allocator].criterion, stored.criterion_estimate_ns_per_op, `${run.run}/${name} Criterion estimate cross-check`);
  }
  for (const [key, value] of [
    ['before_SeferAlloc_raw_slope_ns_per_op', beforeRaw], ['after_SeferAlloc_raw_slope_ns_per_op', afterRaw],
    ['before_SeferAlloc_Criterion_estimate_ns_per_op', beforeCriterion], ['after_SeferAlloc_Criterion_estimate_ns_per_op', afterCriterion],
    ['before_mimalloc_ns_per_op', beforeMimallocRaw], ['after_mimalloc_ns_per_op', afterMimallocRaw],
    ['before_System_ns_per_op', beforeSystemRaw], ['after_System_ns_per_op', afterSystemRaw],
    ['before_Sefer/mimalloc_ratio', beforeVsMimalloc], ['after_Sefer/mimalloc_ratio', afterVsMimalloc],
  ]) close(value, row[key], `${pattern} checked summary ${key}`);
  const numerator = afterCriterion, denominator = beforeCriterion;
  const candidateRatio = ratio(numerator, denominator, `${pattern} candidate/baseline Criterion ratio`);
  summaryRows.push(makeSummaryRow('canonical wall-clock', pattern, 'SeferAlloc raw OLS slope', row.unit, beforeRaw, afterRaw, { before_over_mimalloc: beforeVsMimalloc, candidate_over_mimalloc: afterVsMimalloc, source: 'raw Criterion sample.json + estimates.json' }));
  summaryRows.push(makeSummaryRow('canonical wall-clock', pattern, 'SeferAlloc Criterion estimate', row.unit, denominator, numerator, { before_over_mimalloc: beforeVsMimalloc, candidate_over_mimalloc: afterVsMimalloc, source: 'raw Criterion sample.json + estimates.json' }));
  return [pattern, row.unit, fmt(beforeRaw), fmt(afterRaw), fmt(afterRaw - beforeRaw), `${fmt(numerator)} / ${fmt(denominator)} = ${fmt(candidateRatio, 3)}`, `${fmt(beforeCriterion)} / ${fmt(afterCriterion)}`, `${fmt(beforeMimallocRaw)} / ${fmt(afterMimallocRaw)}`, `${fmt(beforeVsMimalloc, 3)} / ${fmt(afterVsMimalloc, 3)}`];
});

const iaiSelected = ['small_churn_16b', 'cold_alloc_free_256x16b', 'cold_alloc_free_256x64b', 'recycle_alloc_free_256x16b', 'large_alloc_free_cycle'];
const iaiRows = iaiSelected.map((name) => {
  const row = by(iai, (item) => item.bench === name, `IAI ${name}`);
  summaryRows.push(makeSummaryRow('iai', name, 'raw total Ir', 'Ir', row.before_Ir_total, row.after_Ir_total, { source: 'iai_comparison.csv' }));
  summaryRows.push(makeSummaryRow('iai', name, 'Estimated Cycles total', 'cycles', row.before_EstCycles, row.after_EstCycles, { source: 'iai_comparison.csv' }));
  return [name, fmt(row.before_Ir_total, 0), fmt(row.after_Ir_total, 0), `${fmt(row.after_Ir_total, 0)} / ${fmt(row.before_Ir_total, 0)} = ${fmt(ratio(row.after_Ir_total, row.before_Ir_total, `${name} total Ir ratio`), 3)}`, fmt(row.before_EstCycles, 0), fmt(row.after_EstCycles, 0)];
});
const marginalSelected = ['small_churn_16b', 'cold_alloc_free_256x16b', 'cold_alloc_free_256x64b', 'recycle_alloc_free_256x16b'];
const marginalRows = marginalSelected.map((name) => {
  const row = by(marginal, (item) => item.workload === name, `marginal ${name}`);
  summaryRows.push(makeSummaryRow('iai marginal', name, 'SeferAlloc marginal Ir/op', 'Ir/op', row.before_Sefer_marginal_Ir_op, row.after_Sefer_marginal_Ir_op, { before_over_mimalloc: row.before_ratio_Sefer_over_mimalloc, candidate_over_mimalloc: row.after_ratio_Sefer_over_mimalloc, source: 'iai_marginal_comparison.csv' }));
  return [name, fmt(row.before_Sefer_marginal_Ir_op, 1), fmt(row.after_Sefer_marginal_Ir_op, 1), fmt(row.before_mimalloc_marginal_Ir_op, 1), fmt(row.after_mimalloc_marginal_Ir_op, 1), fmt(row.before_ratio_Sefer_over_mimalloc, 3), fmt(row.after_ratio_Sefer_over_mimalloc, 3)];
});

const installedRows = ['after-before', 'before-mimalloc', 'after-mimalloc'].map((comparison) => {
  const row = by(installed, (item) => item.comparison === comparison, `installed ${comparison}`);
  const isD1 = comparison === 'after-before';
  const before = finite(row.internal_A_mean_ns, `${comparison} internal A`);
  const after = finite(row.internal_B_mean_ns, `${comparison} internal B`);
  if (isD1) summaryRows.push(makeSummaryRow('installed global allocator', comparison, 'internal window mean', 'ns', after, before, { ci95_low: row.internal_CI95_low_ns, ci95_high: row.internal_CI95_high_ns, sample_count: row.pairs, source: 'installed_paired_ab_summary.csv' }));
  return [comparison, fmt(before, 0), fmt(after, 0), `${fmt(row.A_over_B_internal_ratio, 3)}`, `${fmt(row.internal_CI95_low_ns, 0)}…${fmt(row.internal_CI95_high_ns, 0)}`, fmt(row.process_A_mean_ns, 0), fmt(row.process_B_mean_ns, 0), fmt(row.A_over_B_process_ratio, 3), `${fmt(row.process_CI95_low_ns, 0)}…${fmt(row.process_CI95_high_ns, 0)}`];
});

const mtRows = mt.map((row) => {
  const threads = row.threads;
  summaryRows.push(makeSummaryRow(`MT ${row.workload}`, `T=${threads}`, 'median of block-mean internal cell latency; CI of paired mean differences', 'ns', row.before_median_ns, row.after_median_ns, { ci95_low: row.latency_ci95_low_ns, ci95_high: row.latency_ci95_high_ns, sample_count: 10, source: 'mt_larson_run2_summary.csv' }));
  return [`${row.workload} / T=${threads}`, fmt(row.before_median_ns, 0), fmt(row.after_median_ns, 0), fmt(row.after_over_before_latency_ratio, 3), `${fmt(row.latency_ci95_low_ns, 0)}…${fmt(row.latency_ci95_high_ns, 0)}`, fmt(row.before_median_mops, 1), fmt(row.after_median_mops, 1), fmt(row.after_over_before_mops_ratio, 3), `${row.latency_significant_p_lt_005_approx === 'true' ? 'yes' : 'no'}`];
});

const foreignRows = ['30', '60', '120'].map((batch) => {
  const row = by(foreign, (item) => item.B === batch && item.T === '8' && item.mode === 'treatment' && item.metric === 'first_dealloc_latency_ns', `foreign B=${batch} latency`);
  const rss = by(foreign, (item) => item.B === batch && item.T === '8' && item.mode === 'treatment' && item.metric === 'rss_after_join_kib', `foreign B=${batch} RSS`);
  const commit = by(foreign, (item) => item.B === batch && item.T === '8' && item.mode === 'treatment' && item.metric === 'commit_after_join_kib', `foreign B=${batch} commit`);
  summaryRows.push(makeSummaryRow('foreign free', `B=${batch}, T=8`, 'first foreign dealloc latency', 'ns', row.before_median, row.after_median, { ci95_low: row.CI95_low, ci95_high: row.CI95_high, sample_count: 10, source: 'foreign_free_ab_summary.csv' }));
  return [batch, fmt(row.before_median, 0), fmt(row.after_median, 0), `${fmt(row.CI95_low, 0)}…${fmt(row.CI95_high, 0)}`, `${fmt(rss.before_median, 0)} / ${fmt(rss.after_median, 0)}`, `${fmt(commit.before_median, 0)} / ${fmt(commit.after_median, 0)}`];
});

const registrationRows = ['30', '60', '120'].map((cycles) => {
  const row = by(registration, (item) => item.scenario === 'registration' && item.cycles === cycles && item.metric === 'work_per_cycle_mean_ns', `registration ${cycles} cycles`);
  const beforeOracle = by(oracle, (item) => item.arm === 'before' && item.scenario === 'registration' && item.cycles === cycles, `baseline registration oracle ${cycles}`);
  const afterOracle = by(oracle, (item) => item.arm === 'after' && item.scenario === 'registration' && item.cycles === cycles, `candidate registration oracle ${cycles}`);
  assert.equal(beforeOracle.path_activation_pass, 'true');
  assert.equal(afterOracle.path_activation_pass, 'true');
  summaryRows.push(makeSummaryRow('registration churn', `${cycles} cycles`, 'timed work per cycle mean', 'ns/cycle', row.before_mean, row.after_mean, { ci95_low: row.ci95_low, ci95_high: row.ci95_high, sample_count: 20, source: 'summary.csv + oracle_summary.csv' }));
  return [cycles, beforeOracle.actual_registrations, afterOracle.actual_registrations, beforeOracle.actual_unlinks, afterOracle.actual_unlinks, fmt(row.before_median, 0), fmt(row.after_median, 0), `${fmt(row.after_over_before_median_ratio, 3)}`, `${fmt(row.ci95_low, 0)}…${fmt(row.ci95_high, 0)}`, afterOracle.constructed_chunks_sum, afterOracle.sidecar_backing_bytes_sum];
});

const denseBefore = by(registration, (item) => item.scenario === 'dense16' && item.cycles === '1' && item.metric === 'work_per_cycle_mean_ns', 'dense16 timing');
const denseBeforeOracle = by(oracle, (item) => item.arm === 'before' && item.scenario === 'dense16', 'dense16 baseline oracle');
const denseAfterOracle = by(oracle, (item) => item.arm === 'after' && item.scenario === 'dense16', 'dense16 candidate oracle');
assert.equal(denseBeforeOracle.path_activation_pass, 'true');
assert.equal(denseAfterOracle.path_activation_pass, 'true');
summaryRows.push(makeSummaryRow('dense 16-byte fill', '262,144 blocks', 'timed work mean', 'ns', denseBefore.before_mean, denseBefore.after_mean, { ci95_low: denseBefore.ci95_low, ci95_high: denseBefore.ci95_high, sample_count: 20, source: 'summary.csv + oracle_summary.csv' }));

const componentRows = registration.filter((r) => ['work_per_cycle_mean_ns', 'process_wall_total_ns'].includes(r.metric)).map((r) => {
  if (r.metric === 'process_wall_total_ns' || r.scenario === 'sparse_max_small') summaryRows.push(makeSummaryRow('component timing', `${r.scenario}/${r.cycles}`, r.metric, 'ns', r.before_mean, r.after_mean, { ci95_low: r.ci95_low, ci95_high: r.ci95_high, sample_count: 40, source: 'summary.csv; raw registration timing' }));
  return [r.scenario, r.cycles, r.metric, fmt(r.before_mean, 0), fmt(r.after_mean, 0), fmt(ratio(r.after_mean, r.before_mean, `${r.scenario}/${r.cycles}/${r.metric}`), 3), `${fmt(r.ci95_low, 0)}…${fmt(r.ci95_high, 0)}`];
});
const memoryRows = [];
for (const [label, records, file, keys] of [
  ['Windows', windowsMemory, 'retained_pin_summary.csv', ['sidecar_size_bytes', 'rss_after_registration_kib', 'commit_after_registration_kib', 'commit_after_last_pin_drop_kib']],
  ['WSL Linux', linuxMemory, 'linux_retained_pin_summary.csv', ['pss_registration_kib', 'pss_owner_unlink_pin_held_kib', 'private_dirty_registration_kib', 'private_dirty_owner_unlink_pin_held_kib', 'private_dirty_root_free_kib']],
]) {
  for (const key of keys) {
    const row = by(records, (item) => item.metric === key, `${label} ${key}`);
    const unit = key.endsWith('_bytes') ? 'bytes' : 'KiB';
    const before = row.before_median_kib ?? row.before_median;
    const candidate = row.after_median_kib ?? row.after_median;
    const delta = row.paired_mean_delta_after_minus_before_kib ?? row.paired_mean_delta_after_minus_before;
    const ciLow = row.ci95_low_kib ?? row.ci95_low;
    const ciHigh = row.ci95_high_kib ?? row.ci95_high;
    summaryRows.push(makeSummaryRow(`${label} retained-pin memory`, 'registration / pin lifecycle', key, unit, before, candidate, { ci95_low: ciLow, ci95_high: ciHigh, sample_count: 20, source: file }));
    memoryRows.push([label, key, fmt(before, 1), fmt(candidate, 1), `${fmt(delta, 1)}`, `${fmt(ciLow, 1)}…${fmt(ciHigh, 1)}`]);
  }
}

for (const [label, sample] of [['before', firstBefore], ['candidate', firstAfter]]) {
  assert.equal(sample.sample_count, 30);
  for (const [metric, key, unit] of [['first_alloc_latency_ns', 'first_alloc_latency_ns', 'ns'], ['rss_delta_1_heap_kib', 'rss_delta_1_heap_kib', 'KiB'], ['rss_delta_8_heaps_kib', 'rss_delta_8_heaps_kib', 'KiB'], ['rss_delta_64_heaps_kib', 'rss_delta_64_heaps_kib', 'KiB'], ['commit_delta_1_heap_kib', 'commit_delta_1_heap_kib', 'KiB'], ['commit_delta_8_heaps_kib', 'commit_delta_8_heaps_kib', 'KiB'], ['commit_delta_64_heaps_kib', 'commit_delta_64_heaps_kib', 'KiB']]) {
    const value = sample.summary[key]?.median;
    assert.ok(Number.isFinite(value), `first allocation ${label}/${metric} missing`);
    if (label === 'before') summaryRows.push(makeSummaryRow('first allocation', 'fresh processes', metric, unit, value, '', { sample_count: sample.sample_count, source: 'first_alloc_before_summary.json' }));
    else {
      const prior = summaryRows.find((row) => row.axis === 'first allocation' && row.metric === metric);
      assert.ok(prior, `first allocation baseline ${metric} missing`);
      const merged = makeSummaryRow('first allocation', 'fresh processes', metric, unit, prior.before, value, { sample_count: sample.sample_count, source: 'first_alloc_before_summary.json + first_alloc_after_summary.json' });
      Object.assign(prior, merged);
    }
  }
}
const firstAllocationRows = summaryRows.filter((row) => row.axis === 'first allocation').map((row) => [row.metric, row.unit, fmt(row.before, 0), fmt(row.candidate, 0), `${fmt(row.ratio_numerator, 0)} / ${fmt(row.ratio_denominator, 0)} = ${fmt(row.candidate_over_before, 3)}`]);

for (const row of summaryRows) {
  row.baseline_commit = passport.baseline_commit;
  row.candidate_commit = passport.candidate_commit;
  row.cpu = linuxIdentity.host.cpu;
  row.sample_count ||= row.axis.startsWith('canonical') ? '2 source runs per arm; at least 10 Criterion samples/run' : row.axis.startsWith('iai') ? '1 recorded suite per source; deterministic Ir' : '';
  row.sampling_definition = row.axis.startsWith('MT') ? '10 ABBA blocks; 40 processes; medians of block means; CI for mean differences' : row.axis.includes('memory') ? '20 ABBA blocks; 80 processes; medians of individual samples; CI for block mean differences' : row.axis === 'first allocation' ? '30 sequential fresh processes per source; descriptive median; no paired CI' : row.axis.includes('registration') || row.axis.includes('component') || row.axis.includes('dense') ? '10 ABBA blocks; 40 processes per cell; CI for block mean differences' : 'see per-axis report';
  row.os = row.axis.startsWith('WSL') ? linuxIdentity.host.kernel_measurement : row.axis.startsWith('iai') ? 'WSL Linux / Valgrind; see original IAI logs' : linuxIdentity.host.host_os;
  row.features = row.axis.startsWith('canonical') || row.axis.startsWith('iai') ? 'production internals bench-internals' : row.axis.includes('retained-pin') ? 'baseline: production internals; candidate: production internals bench-internals' : row.axis.includes('component') || row.axis.includes('registration') || row.axis.includes('dense') ? 'production' : 'production internals';
}
const csvHeaders = ['baseline_commit', 'candidate_commit', 'cpu', 'os', 'features', 'axis', 'workload', 'metric', 'unit', 'before', 'candidate', 'delta_candidate_minus_before', 'ratio_numerator', 'ratio_denominator', 'candidate_over_before', 'ci95_low', 'ci95_high', 'sample_count', 'sampling_definition', 'before_over_mimalloc', 'candidate_over_mimalloc', 'source'];
const csvEscape = (value) => { const text = String(value ?? ''); return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text; };
const summaryCsv = [csvHeaders.join(','), ...summaryRows.map((row) => csvHeaders.map((key) => csvEscape(row[key])).join(','))].join('\n') + '\n';

const identity = linuxIdentity.measurement_source_snapshot;
const report = [
  '# D1′ chunked sidecar — comparative performance gate',
  '',
  '**Verdict: NO-GO / NOT ACCEPTED.** Safety CI receipt applies to the earlier safety-tested source; this report evaluates the measured candidate identified below. No merge to `main` is authorized or performed.',
  '',
  '## Source identity and environment',
  '',
  `- Baseline commit/tree: \`${identity.baseline_commit}\` / \`${identity.baseline_base_tree}\`; measurement tree \`${identity.baseline_measurement_tree}\`.`,
  `- Candidate commit/tree: \`${identity.candidate_commit}\` / \`${identity.candidate_base_tree}\`; retained-pin WSL measurement tree \`${identity.candidate_measurement_tree}\`.`,
  `- Canonical wall-clock, IAI and first-allocation use the cited commit trees. MT source identity is each base commit plus the pre-measurement harness patch SHA-256 \`${registrationIdentity.mt_harness_patch_sha256}\`; the later registration measurement trees must not be attributed retroactively to the MT run. Registration trees: \`${registrationIdentity.baseline_measurement_tree}\` / \`${registrationIdentity.candidate_measurement_tree}\`. Windows retained-pin trees: \`${windowsIdentity.baseline_measurement_tree}\` / \`${windowsIdentity.candidate_measurement_tree}\`.`,
  '- Ограничения: native Linux не измерялся; WSL PSS/private-dirty сняты только для component retained-pin lifecycle, не для first-alloc и нескольких уровней live segments. Эти ADR-оси не закрыты. NO-GO исключает promotion данного кандидата, но не означает полного покрытия всей матрицы. Предыдущая фраза «Linux-ось закрыта» была слишком широкой.',
  `- Windows host: ${linuxIdentity.host.cpu}; ${linuxIdentity.host.logical_processors} logical processors; ${linuxIdentity.host.host_os}; Rust ${linuxIdentity.host.rustc_windows}, Cargo ${linuxIdentity.host.cargo_windows}.`,
  `- Linux memory oracle: ${linuxIdentity.host.kernel_measurement}; ${linuxIdentity.host.linux_kernel}; Rust ${linuxIdentity.host.rustc_linux_wsl}, Cargo ${linuxIdentity.host.cargo_linux_wsl}. This is WSL evidence, not native Linux.`,
  '- Wall-clock canonical rows use clean, isolated Criterion targets and four balanced source runs (before, after, after, before); IAI totals and marginal Ir/op are reported on separate axes.',
  '- Installed-allocator comparisons use fresh processes and A/B/B/A blocks; all claims name the measured entry point below.',
  '',
  '## Canonical wall-clock — `bench:table`',
  '',
  'Entry point: Criterion invokes each allocator through `GlobalAlloc` directly in one process; this is not an installed `#[global_allocator]` run. Each delta is candidate minus baseline; each candidate/baseline ratio prints its numerator and denominator. Raw OLS slopes are diagnostic; Criterion estimates preserve the canonical `bench:table` axis. Four clean runs reduce but do not eliminate host noise; this table is not a per-cell confidence test.',
  '',
  table(['Workload', 'Unit', 'Before Sefer OLS', 'Candidate Sefer OLS', 'Delta', 'Criterion candidate / baseline', 'Criterion before / candidate', 'mimalloc before / candidate', 'Sefer/mimalloc before / candidate'], wallRows),
  '',
  '## IAI — raw totals and marginal cost are distinct',
  '',
  'Entry point: the normal churn/alloc-free workloads use `SeferAlloc` through its `GlobalAlloc` facade directly; isolated free-only probes enter through `HeapCore` diagnostics. Neither is a process-installed `#[global_allocator]` timing result. The installed-global process comparison is reported separately below.',
  '',
  table(['Workload', 'Before total Ir', 'Candidate total Ir', 'Candidate / baseline total Ir', 'Before Est. Cycles', 'Candidate Est. Cycles'], iaiRows),
  '',
  'Marginal Ir/op is the separate normalized axis; the comparison to mimalloc is shown as SeferAlloc/mimalloc for each source.',
  '',
  table(['Workload', 'Before Sefer Ir/op', 'Candidate Sefer Ir/op', 'Before mimalloc Ir/op', 'Candidate mimalloc Ir/op', 'Before Sefer/mimalloc', 'Candidate Sefer/mimalloc'], marginalRows),
  '',
  '## Real installed global allocator and mimalloc',
  '',
  'Entry point: the process installs each allocator as `#[global_allocator]` and runs the shared workload. The internal window and full process wall are both reported; a CI including zero is INCONCLUSIVE, not PASS. A/A controls are retained in `installed_paired_ab_summary.csv`.',
  '',
  table(['Comparison (A/B)', 'Internal A ns', 'Internal B ns', 'A/B internal ratio', 'Internal paired 95% CI Δ ns', 'Process A ns', 'Process B ns', 'A/B process ratio', 'Process paired 95% CI Δ ns'], installedRows),
  '',
  '## Multithreaded Larson and foreign free',
  '',
  'The MT Larson harness installs `SeferAlloc` as the real process `#[global_allocator]`; exact operation counts and route activation passed. Throughput ratios are candidate/baseline; latency CIs are paired. The foreign-free example also exercises the installed `#[global_allocator]` deallocation path. Its latency CIs include zero at all three batch sizes; RSS and commit telemetry are separate metrics.',
  '',
  table(['Workload / threads', 'Before block-mean median latency ns', 'Candidate block-mean median latency ns', 'Candidate/baseline', 'Paired latency 95% CI Δ ns', 'Before Mops', 'Candidate Mops', 'Candidate/baseline throughput', 'Latency CI excludes zero'], mtRows),
  '',
  table(['Foreign-free batch', 'Before first-dealloc median ns', 'Candidate median ns', 'Paired 95% CI Δ ns', 'RSS after join before/candidate KiB', 'Commit after join before/candidate KiB'], foreignRows),
  '',
  '## Registration churn, dense fill, and activation oracles',
  '',
  'The registration entry point is direct `SeferAlloc::alloc/dealloc` component timing, not an installed `#[global_allocator]` result. Oracle builds separately verify route registrations/unlinks and candidate constructed chunks; timing builds omit diagnostic RMW. Every row below has a corresponding passing activation oracle.',
  '',
  table(['Registration cycles', 'Baseline registrations', 'Candidate registrations', 'Baseline unlinks', 'Candidate unlinks', 'Before median ns/cycle', 'Candidate median ns/cycle', 'Candidate/baseline median', 'Paired 95% CI Δ ns', 'Candidate constructed chunks', 'Candidate backing bytes'], registrationRows),
  '',
  table(['Dense16 workload', 'Before mean ns', 'Candidate mean ns', 'Candidate / baseline = numerator / denominator', 'Paired 95% CI Δ ns', 'Baseline activation', 'Candidate activation', 'Candidate constructed chunks', 'Candidate backing bytes'], [[ '262,144 × 16-byte blocks', fmt(denseBefore.before_mean, 0), fmt(denseBefore.after_mean, 0), `${fmt(denseBefore.after_mean, 0)} / ${fmt(denseBefore.before_mean, 0)} = ${fmt(ratio(denseBefore.after_mean, denseBefore.before_mean, 'dense16 candidate/baseline'), 3)}`, `${fmt(denseBefore.ci95_low, 0)}…${fmt(denseBefore.ci95_high, 0)}`, denseBeforeOracle.path_activation_pass, denseAfterOracle.path_activation_pass, denseAfterOracle.constructed_chunks_sum, denseAfterOracle.sidecar_backing_bytes_sum ]]),
  '',
  'Sparse max-small activation also passed; its route remains retained after trim in both versions, so the oracle does not require unlink there. Полное время процесса включает запуск, вывод, создание потоков и teardown; work — только внутреннее окно. В таблице ниже обе оси используют средние, а не медианы; CI относится к среднему paired delta десяти блоков.',
  '',
  table(['Scenario', 'Cycles', 'Metric', 'Before mean ns', 'Candidate mean ns', 'Candidate/baseline mean', 'Paired mean delta 95% CI ns'], componentRows),
  '',
  '## First allocation and retained-pin memory',
  '',
  'Windows working-set/RSS and commit are reported separately; System free counters prove logical sidecar release, not immediate OS page release. Linux PSS and `Private_Dirty` come from `/proc/self/smaps_rollup` in WSL2 fresh processes. The retained-pin oracle asserts no sidecar free at owner unlink and exact backing free on final pin drop in every sample.',
  '',
  table(['Memory axis', 'Metric/stage', 'Before median', 'Candidate median', 'Paired mean delta', 'Paired 95% CI Δ'], memoryRows),
  '',
  'The first-allocation probe calls `SeferAlloc::alloc` directly (it deliberately does not install a process `#[global_allocator]`) to control when bootstrap begins. Samples are 30 fresh processes per source.',
  '',
  table(['Metric', 'Unit', 'Before median', 'Candidate median', 'Candidate / baseline = numerator / denominator'], firstAllocationRows),
  '',
  'The Windows probe includes RSS/commit deltas at one, eight, and 64 heaps; no Linux PSS claim is made for these Windows runs. Raw samples and derived summaries are `first_alloc_*_samples.json` and `first_alloc_*_summary.json`.',
  '',
  'The retained-pin oracle is a component-level `RouteDirectory::register` / `RoutePin` owner-unlink/final-release probe, not allocator throughput. Windows uses process RSS/VM-size and System allocation counters; Linux WSL uses `smaps_rollup` PSS and `Private_Dirty`.',
  '',
  'Candidate backing allocation grows from the baseline sidecar size. Более низкие resident/PSS/private-dirty values — наблюдаемая разница этого probe, не доказательство её единственной причины и не гарантия снижения reserved memory или OS commit. Memory telemetry сама выделяет временные буферы; snapshots не атомарны. First-allocation arms запускались последовательно, без balanced paired CI, и являются описательными.',
  '',

  '## Decision',
  '',
  'The candidate is **NO-GO / NOT ACCEPTED** for the current design. The decisive evidence is not a single aggregate average: canonical wall-clock contains material regressions, the corrected MT Larson harness shows significant T=4/T=8 deterioration, dense16 and repeated registration timing regress, and the IAI marginal axis does not mirror the favorable raw-total changes. The installed-global paired workload alone is inconclusive for candidate versus baseline; beating mimalloc on that one workload cannot override the regressions. The WSL memory decrease is a distinct, non-native observation and does not change this verdict.',
  '',
  '## Ограниченная zero-trust проверка доставки',
  '',
  '- Product `src/`, Cargo.lock и production composition в этой доставке не изменены. Новых public hooks нет. Unsafe-сайты новых probes/test — `System.alloc/dealloc` и прямые `SeferAlloc.alloc/dealloc`; layouts валидны, указатели остаются live до освобождения, route/pin уничтожаются до System backing. Это ручная проверка границ unsafe/RAII, не новый Miri receipt.',
  '- MT correction сохраняет receivers до завершения producers и последнего drain; exact operation assertions отвергли первый неполный прогон. Старый MT run1 исключён из verdict. Original baseline run со shared Criterion target также исключён; source-identity failures WSL произошли до сбора samples.',
  '- Регрессионный тест проверяет zero/two constructed chunks и точный final-pin free; eager construction нарушила бы исходный zero-chunk assert, premature free — счётчик при retained pin. Тест typechecked, локально не исполнялся; delivery CI пока не подтверждён.',
  '- No-build проверки: Windows targeted cargo check и Clippy -D warnings с production, production internals, production internals bench-internals; workspace cargo fmt --check через WSL; Linux oracle Clippy. Report generator --check, JS syntax и structure guard прошли. Полный независимый аудит всех rust-intel модулей не выполнялся; подагенты не запускались.',
  '',
  '## Evidence and reproduction',
  '',
  '- Machine-readable companion: `D1_PRIME_CHUNKED_SIDECAR_GATE_summary.csv` (generated by the same script as this report).',
  '- Raw logs and per-sample structured data: `docs/perf/d1_prime_chunked_sidecar/raw/`; cited files are under 200 KiB uncompressed or gzip-compressed per repository policy.',
  '- Derived axis inputs: `docs/perf/d1_prime_chunked_sidecar/summaries/`; measurement and derivation scripts are archived in `docs/perf/d1_prime_chunked_sidecar/reproduction/`.',
  '- Regenerate this report and summary: `node scripts/d1_prime_chunked_sidecar_report.mjs`.',
  '- Wall-clock raw-to-table derivation: archived `derive-wallclock.mjs`; IAI derivation: archived `derive-iai.mjs`.',
  '',
].join('\n');

if (process.argv.includes('--check')) {
  assert.equal(fs.readFileSync(reportPath, 'utf8'), report, 'Отчёт отличается от результата генерации');
  assert.equal(fs.readFileSync(outputCsvPath, 'utf8'), summaryCsv, 'Companion CSV отличается от результата генерации');
} else {
  fs.writeFileSync(reportPath, report);
  fs.writeFileSync(outputCsvPath, summaryCsv);
}
console.log(`PASS: generated ${path.relative(root, reportPath)} and ${path.relative(root, outputCsvPath)} from ${summaryRows.length} checked summary rows.`);
