import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const passport = Object.fromEntries(
  fs.readFileSync(path.join(root, 'passport.txt'), 'utf8')
    .split(/\r?\n/)
    .filter((line) => line.includes('='))
    .map((line) => {
      const at = line.indexOf('=');
      return [line.slice(0, at), line.slice(at + 1)];
    }),
);

const runs = [
  { id: 'before-1', arm: 'before', dir: 'raw/before-clean-1', commit: passport.baseline_commit, tree: passport.baseline_tree },
  { id: 'after-1', arm: 'after', dir: 'raw/after-clean-1', commit: passport.candidate_commit, tree: passport.candidate_tree },
  { id: 'after-2', arm: 'after', dir: 'raw/after-clean-2', commit: passport.candidate_commit, tree: passport.candidate_tree },
  { id: 'before-2', arm: 'before', dir: 'raw/before-clean-2', commit: passport.baseline_commit, tree: passport.baseline_tree },
];
const arms = ['SeferAlloc', 'mimalloc', 'System'];
const sizes = ['16B', '64B', '256B', '1024B'];
const specs = [
  ...['global_alloc', 'global_alloc_churn', 'global_alloc_churn_write', 'global_alloc_churn_with_teardown']
    .map((group) => ({ group, ids: sizes.flatMap((size) => arms.map((arm) => `${group}/${arm}/${size}`)), scale: 1024, unit: 'ns/pair' })),
  { group: 'manual_realloc_sim', ids: arms.map((arm) => `global_alloc/manual_realloc_sim/${arm}`), scale: 1, unit: 'ns/closure' },
  { group: 'segment_decommit_cycle', ids: arms.map((arm) => `segment_decommit_cycle/${arm}/253KiB`), scale: 1, unit: 'ns/batch' },
  { group: 'working_set_cycle', ids: sizes.map((size) => `working_set_cycle/SeferAlloc/${size}`), scale: 1, unit: 'ns/batch' },
];
const expected = specs.flatMap((spec) => spec.ids);
assert.equal(new Set(expected).size, expected.length, 'benchmark IDs must be unique');
assert.ok(passport.baseline_commit && passport.baseline_tree && passport.candidate_commit && passport.candidate_tree);

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const mid = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? sorted[mid] : (sorted[mid - 1] + sorted[mid]) / 2;
}

function linearSlope(xs, ys) {
  const meanX = xs.reduce((sum, value) => sum + value, 0) / xs.length;
  const meanY = ys.reduce((sum, value) => sum + value, 0) / ys.length;
  let covariance = 0;
  let variance = 0;
  for (let i = 0; i < xs.length; i++) {
    covariance += (xs[i] - meanX) * (ys[i] - meanY);
    variance += (xs[i] - meanX) ** 2;
  }
  assert.ok(variance > 0, 'linear sample requires varying iteration counts');
  return covariance / variance;
}

function csv(value) {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function collect(directory) {
  const result = new Map();
  const visit = (dir) => {
    for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
      const file = path.join(dir, entry.name);
      if (entry.isDirectory()) visit(file);
      else if (entry.name === 'benchmark.json' && path.basename(path.dirname(file)) === 'new') {
        const meta = JSON.parse(fs.readFileSync(file, 'utf8'));
        const samplePath = path.join(path.dirname(file), 'sample.json');
        const estimatesPath = path.join(path.dirname(file), 'estimates.json');
        if (fs.existsSync(samplePath) && fs.existsSync(estimatesPath)) {
          assert.ok(!result.has(meta.full_id), `duplicate sample set for ${meta.full_id}`);
          result.set(meta.full_id, {
            sample: JSON.parse(fs.readFileSync(samplePath, 'utf8')),
            estimates: JSON.parse(fs.readFileSync(estimatesPath, 'utf8')),
          });
        }
      }
    }
  };
  visit(directory);
  return result;
}

const perRun = [];
for (const run of runs) {
  const runRoot = path.join(root, run.dir);
  assert.match(fs.readFileSync(path.join(runRoot, 'bench-table.log'), 'utf8'), /\[bench-table\] PASS — 159 bench id\(s\) parsed, all 51 expected ids present/);
  const samples = collect(path.join(runRoot, 'criterion'));
  for (const id of expected) {
    const sampleSet = samples.get(id);
    assert.ok(sampleSet, `${run.id}: missing raw sample ${id}`);
    const { sample, estimates } = sampleSet;
    assert.ok(['Linear', 'Flat'].includes(sample.sampling_mode), `${run.id}/${id}: unexpected sampling mode`);
    assert.equal(sample.times.length, sample.iters.length, `${run.id}/${id}: times/iters length mismatch`);
    assert.ok(sample.times.length >= 10, `${run.id}/${id}: fewer than 10 samples`);
    const spec = specs.find((candidate) => candidate.ids.includes(id));
    const normalized = sample.times.map((time, index) => {
      assert.ok(Number.isFinite(time) && time > 0 && Number.isFinite(sample.iters[index]) && sample.iters[index] > 0);
      return time / sample.iters[index] / spec.scale;
    });
    const criterionSummary = sample.sampling_mode === 'Linear' ? estimates.slope : estimates.mean;
    assert.ok(criterionSummary?.point_estimate > 0, `${run.id}/${id}: missing Criterion estimate`);
    const criterionEstimate = criterionSummary.point_estimate / spec.scale;
    const rawSlope = sample.sampling_mode === 'Linear'
      ? linearSlope(sample.iters, sample.times) / spec.scale
      : median(normalized);
    assert.ok(Number.isFinite(rawSlope) && rawSlope > 0 && Number.isFinite(criterionEstimate), `${run.id}/${id}: invalid slope estimate`);
    perRun.push({
      run: run.id,
      arm: run.arm,
      commit: run.commit,
      tree: run.tree,
      features: passport.features,
      host: `${passport.cpu_model}; ${passport.logical_processors} logical CPUs; ${passport.host}`,
      id,
      unit: spec.unit,
      estimator: sample.sampling_mode === 'Linear' ? 'OLS slope' : 'median flat sample',
      n: normalized.length,
      value: criterionEstimate,
      rawSlope,
      criterionEstimate,
      rawVsCriterionPct: (rawSlope / criterionEstimate - 1) * 100,
      ciLow: criterionSummary.confidence_interval.lower_bound / spec.scale,
      ciHigh: criterionSummary.confidence_interval.upper_bound / spec.scale,
      sampleMedian: median(normalized),
      sampleMin: Math.min(...normalized),
      sampleMax: Math.max(...normalized),
    });
  }
}
assert.equal(perRun.length, runs.length * expected.length);

const perRunHead = ['run', 'source_arm', 'commit', 'tree', 'features', 'host', 'benchmark_id', 'unit', 'raw_sample_estimator', 'sample_count', 'raw_ols_slope_ns_per_op', 'criterion_estimate_ns_per_op', 'raw_ols_vs_criterion_delta_pct', 'criterion_ci_low_ns_per_op', 'criterion_ci_high_ns_per_op', 'normalized_sample_median_ns_per_op', 'normalized_sample_min_ns_per_op', 'normalized_sample_max_ns_per_op'];
fs.writeFileSync(path.join(root, 'wallclock_per_run.csv'), [perRunHead.join(','), ...perRun.map((row) => [row.run, row.arm, row.commit, row.tree, row.features, row.host, row.id, row.unit, row.estimator, row.n, row.rawSlope, row.criterionEstimate, row.rawVsCriterionPct, row.ciLow, row.ciHigh, row.sampleMedian, row.sampleMin, row.sampleMax].map(csv).join(','))].join('\n') + '\n');

function workloadKey(id) {
  return id.replace(/\/(SeferAlloc|mimalloc|System)(?=\/|$)/, '/{allocator}');
}

const workloadKeys = [...new Set(expected.map(workloadKey))];
const comparison = [];
for (const key of workloadKeys) {
  const row = { id: key, unit: specs.find((spec) => spec.ids.some((id) => workloadKey(id) === key)).unit };
  const available = arms.filter((allocator) => expected.includes(key.replace('{allocator}', allocator)));
  for (const allocator of available) {
    const id = key.replace('{allocator}', allocator);
    for (const sourceArm of ['before', 'after']) {
      const values = perRun.filter((item) => item.id === id && item.arm === sourceArm);
      assert.equal(values.length, 2, `${key}/${allocator}/${sourceArm}: expected two runs`);
      row[`${allocator}_${sourceArm}_raw`] = median(values.map((item) => item.rawSlope));
      row[`${allocator}_${sourceArm}_criterion`] = median(values.map((item) => item.criterionEstimate));
    }
  }
  if (row.SeferAlloc_before_raw != null && row.SeferAlloc_after_raw != null) {
    row.delta_pct = (row.SeferAlloc_after_raw - row.SeferAlloc_before_raw) / row.SeferAlloc_before_raw * 100;
    row.criterion_delta_pct = (row.SeferAlloc_after_criterion - row.SeferAlloc_before_criterion) / row.SeferAlloc_before_criterion * 100;
    assert.ok(Math.abs((row.SeferAlloc_after_raw / row.SeferAlloc_before_raw) * row.SeferAlloc_before_raw - row.SeferAlloc_after_raw) <= Math.max(1, row.SeferAlloc_after_raw) * 1e-12);
  }
  for (const sourceArm of ['before', 'after']) {
    if (row.SeferAlloc_before_raw != null && row[`mimalloc_${sourceArm}_raw`] != null) {
      const ratios = runs.filter((run) => run.arm === sourceArm).map((run) => {
        const sefer = perRun.find((item) => item.id === key.replace('{allocator}', 'SeferAlloc') && item.run === run.id);
        const mimalloc = perRun.find((item) => item.id === key.replace('{allocator}', 'mimalloc') && item.run === run.id);
        assert.ok(sefer && mimalloc, `${key}/${sourceArm}: missing paired allocator row`);
        const ratio = sefer.value / mimalloc.value;
        assert.ok(Math.abs(ratio * mimalloc.value - sefer.value) <= Math.max(1, sefer.value) * 1e-12);
        return ratio;
      });
      row[`${sourceArm}_vs_mimalloc`] = median(ratios);
    }
  }
  comparison.push(row);
}

const compHead = ['workload_id', 'unit', 'before_SeferAlloc_raw_slope_ns_per_op', 'after_SeferAlloc_raw_slope_ns_per_op', 'delta_pct_(after-before)/before', 'before_SeferAlloc_Criterion_estimate_ns_per_op', 'after_SeferAlloc_Criterion_estimate_ns_per_op', 'Criterion_delta_pct_(after-before)/before', 'before_mimalloc_ns_per_op', 'after_mimalloc_ns_per_op', 'before_System_ns_per_op', 'after_System_ns_per_op', 'before_Sefer/mimalloc_ratio', 'after_Sefer/mimalloc_ratio'];
fs.writeFileSync(path.join(root, 'wallclock_comparison.csv'), [compHead.join(','), ...comparison.map((row) => [row.id, row.unit, row.SeferAlloc_before_raw, row.SeferAlloc_after_raw, row.delta_pct, row.SeferAlloc_before_criterion, row.SeferAlloc_after_criterion, row.criterion_delta_pct, row.mimalloc_before_raw, row.mimalloc_after_raw, row.System_before_raw, row.System_after_raw, row.before_vs_mimalloc, row.after_vs_mimalloc].map(csv).join(','))].join('\n') + '\n');

const fmt = (value) => value == null ? '—' : Number(value).toFixed(2);
const md = [
  '# Wall-clock comparison from raw Criterion samples',
  '',
  'For `Linear` samples, the script derives an ordinary least-squares slope from raw `times` versus `iters` and also reports Criterion’s structured slope point estimate and 95% interval. For `Flat` samples, the raw estimator is the median of `time / iters`, alongside Criterion’s mean estimate. Any OLS/Criterion divergence stays visible; paired ratios use Criterion estimates to match the canonical `bench:table` metric. Each source arm has two clean-target runs in ABBA order. `Δ % = (after − before) / before`; ratios are paired per run as SeferAlloc / mimalloc.',
  '',
  '| Workload | Unit | Before Sefer OLS | After Sefer OLS | Δ % (after − before) / before | Before Criterion estimate | After Criterion estimate | Criterion Δ % (after − before) / before | Before mimalloc | After mimalloc | Before System | After System | Before Sefer / mimalloc | After Sefer / mimalloc |',
  '|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|',
  ...comparison.map((row) => `| ${row.id} | ${row.unit} | ${fmt(row.SeferAlloc_before_raw)} | ${fmt(row.SeferAlloc_after_raw)} | ${fmt(row.delta_pct)} | ${fmt(row.SeferAlloc_before_criterion)} | ${fmt(row.SeferAlloc_after_criterion)} | ${fmt(row.criterion_delta_pct)} | ${fmt(row.mimalloc_before_raw)} | ${fmt(row.mimalloc_after_raw)} | ${fmt(row.System_before_raw)} | ${fmt(row.System_after_raw)} | ${fmt(row.before_vs_mimalloc)} | ${fmt(row.after_vs_mimalloc)} |`),
  '',
].join('\n');
fs.writeFileSync(path.join(root, 'wallclock_comparison.md'), md);
console.log(`PASS: ${runs.length} clean runs × ${expected.length} benchmark IDs; wrote per-run CSV, comparison CSV, and Markdown.`);
