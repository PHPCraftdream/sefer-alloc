import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import { gunzipSync } from 'node:zlib';

// Проверка чисел отчёта без повторного запуска измерений.
export function verifySamples(evidence, csvFile) {
  const mean = (xs) => xs.reduce((sum, x) => sum + x, 0) / xs.length;
  const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
  const close = (a, b, label) => {
    assert.ok(b !== '' && b != null && Number.isFinite(Number(b)), `${label}: отсутствует число`);
    assert.ok(Math.abs(a - Number(b)) < Math.max(1, Math.abs(a)) * 1e-10, `${label}: ${a} != ${b}`);
  };
  function raw(name) {
    let bytes = fs.readFileSync(path.join(evidence, 'raw', name));
    if (name.endsWith('.gz')) bytes = gunzipSync(bytes);
    return bytes.toString('utf8').trim().split(/\r?\n/).map((line) => JSON.parse(line));
  }
  function stats(rows, value, expectedBlocks) {
    assert.equal(rows.length, expectedBlocks * 4);
    const before = rows.filter((r) => r.arm === 'before').map(value);
    const after = rows.filter((r) => r.arm === 'after').map(value);
    assert.equal(before.length, expectedBlocks * 2);
    assert.equal(after.length, expectedBlocks * 2);
    assert.ok([...before, ...after].every(Number.isFinite));
    const blockBefore = [], blockAfter = [];
    const deltas = Array.from({ length: expectedBlocks }, (_, block) => {
      const b = rows.filter((r) => r.block === block && r.arm === 'before').map(value);
      const a = rows.filter((r) => r.block === block && r.arm === 'after').map(value);
      assert.equal(a.length, 2); assert.equal(b.length, 2);
      blockBefore.push(mean(b)); blockAfter.push(mean(a));
      return mean(a) - mean(b);
    });
    const delta = mean(deltas);
    const se = Math.sqrt(deltas.reduce((sum, d) => sum + (d - delta) ** 2, 0) / (expectedBlocks - 1) / expectedBlocks);
    const critical = expectedBlocks === 10 ? 2.262 : 2.093;
    return { beforeMean: mean(before), afterMean: mean(after), beforeMedian: median(before), afterMedian: median(after), beforeBlockMedian: median(blockBefore), afterBlockMedian: median(blockAfter), delta, low: delta - critical * se, high: delta + critical * se };
  }
  const registration = raw('_raw_d1_prime_registration_timing_run5.ndjson.gz');
  for (const row of csvFile('summary.csv')) {
    const rows = registration.filter((r) => r.scenario === row.scenario && r.cycles === Number(row.cycles));
    const s = stats(rows, (r) => r.metrics[row.metric], 10);
    for (const [key, value] of Object.entries({ before_mean: s.beforeMean, after_mean: s.afterMean, before_median: s.beforeMedian, after_median: s.afterMedian, paired_mean_delta_after_minus_before: s.delta, ci95_low: s.low, ci95_high: s.high })) close(value, row[key], `registration/${row.scenario}/${row.cycles}/${row.metric}/${key}`);
    close(s.afterMedian / s.beforeMedian, row.after_over_before_median_ratio, 'registration ratio');
  }
  const mt = raw('_raw_d1_prime_mt_larson_run2.ndjson.gz');
  assert.equal(mt.length, 40);
  for (const row of mt) {
    assert.equal(row.config_conflicts, 0);
    assert.ok(row.segments_reserved_total > 0);
    assert.equal(row.cells.length, 8);
    for (const cell of row.cells) assert.equal(cell.ops, (cell.workload === 'larson' ? 801536 : 399360) * cell.threads);
  }
  for (const row of csvFile('mt_larson_run2_summary.csv')) {
    const cell = (r) => r.cells.find((c) => c.workload === row.workload && c.threads === Number(row.threads));
    for (const [metric, prefix] of [['ns', 'latency'], ['mops', 'mops']]) {
      const s = stats(mt, (r) => cell(r)[metric], 10);
      close(s.beforeBlockMedian, row[metric === 'ns' ? 'before_median_ns' : 'before_median_mops'], 'MT baseline block median');
      close(s.afterBlockMedian, row[metric === 'ns' ? 'after_median_ns' : 'after_median_mops'], 'MT candidate block median');
      close(s.low, row[`${prefix}_ci95_low${metric === 'ns' ? '_ns' : ''}`], 'MT CI low');
      close(s.high, row[`${prefix}_ci95_high${metric === 'ns' ? '_ns' : ''}`], 'MT CI high');
    }
  }
  for (const [name, csv, linux] of [
    ['_raw_d1_prime_retained_pin_windows_run6.ndjson', 'retained_pin_summary.csv', false],
    ['_raw_d1_prime_retained_pin_wsl_linux_run7.ndjson', 'linux_retained_pin_summary.csv', true],
  ]) {
    const rows = raw(name);
    for (const row of rows) {
      const v = linux ? row.metrics : row;
      assert.equal(v[linux ? 'sidecar_free_bytes_owner_unlink' : 'sidecar_free_bytes_after_owner_unlink'], 0);
      assert.equal(v[linux ? 'sidecar_free_bytes_last_pin' : 'sidecar_free_bytes_after_last_pin'], v.sidecar_alloc_bytes);
      if (row.arm === 'after') assert.equal(v.constructed_chunks_while_pinned, 1);
    }
    for (const row of csvFile(csv)) {
      const s = stats(rows, (r) => (linux ? r.metrics : r)[row.metric], 20);
      close(s.beforeMedian, row[linux ? 'before_median_kib' : 'before_median'], 'memory baseline');
      close(s.afterMedian, row[linux ? 'after_median_kib' : 'after_median'], 'memory candidate');
      close(s.low, row[linux ? 'ci95_low_kib' : 'ci95_low'], 'memory CI low');
      close(s.high, row[linux ? 'ci95_high_kib' : 'ci95_high'], 'memory CI high');
    }
  }
  const installed = raw('_raw_d1_prime_installed_paired_ab.ndjson');
  for (const row of csvFile('installed_paired_ab_summary.csv')) {
    const rows = installed.filter((r) => r.comparison === row.comparison).map((r) => ({ ...r, arm: r.slot === 'A' ? 'after' : 'before' }));
    for (const [key, metric] of [['elapsed_ns', 'internal'], ['process_wall_ns', 'process']]) {
      const s = stats(rows, (r) => r[key], Number(row.pairs));
      close(s.afterMean, row[`${metric}_A_mean_ns`], 'installed A mean');
      close(s.beforeMean, row[`${metric}_B_mean_ns`], 'installed B mean');
      close(s.low, row[`${metric}_CI95_low_ns`], 'installed CI low');
      close(s.high, row[`${metric}_CI95_high_ns`], 'installed CI high');
    }
  }
  const foreign = raw('_raw_d1_prime_foreign_free_ab.ndjson.gz');
  assert.equal(foreign.length, 240);
  for (const row of csvFile('foreign_free_ab_summary.csv')) {
    const selected = foreign.filter((r) => r.b === Number(row.B) && r.t === Number(row.T) && r.mode === row.mode);
    const s = stats(selected, (r) => r[row.metric], 10);
    for (const [key, value] of Object.entries({ before_mean: s.beforeMean, after_mean: s.afterMean, before_median: s.beforeMedian, after_median: s.afterMedian, CI95_low: s.low, CI95_high: s.high })) close(value, row[key], `foreign/${row.B}/${row.mode}/${row.metric}/${key}`);
  }
  for (const arm of ['before', 'after']) {
    const json = (name) => JSON.parse(fs.readFileSync(path.join(evidence, 'summaries', name), 'utf8'));
    const samples = json(`first_alloc_${arm}_samples.json`).samples;
    const summary = json(`first_alloc_${arm}_summary.json`);
    assert.equal(samples.length, 30);
    for (const [key, value] of Object.entries(summary.summary)) {
      const values = samples.map((r) => {
        if (key === 'first_alloc_latency_ns') return r[key];
        const match = /^(rss|commit)_delta_(\d+)_heaps?_kib$/.exec(key);
        assert.ok(match, `unknown first-allocation metric ${key}`);
        return r[`${match[1]}_after_${match[2]}_heap${match[2] === '1' ? '' : 's'}_kib`] - r[`${match[1]}_before_kib`];
      });
      assert.ok(values.every(Number.isFinite));
      close(median(values), value.median, `first-allocation/${arm}/${key}`);
    }
    const log = fs.readFileSync(path.join(evidence, 'raw', `_raw_d1_prime_iai_${arm}.log`), 'utf8');
    assert.match(log, /\[iai\] PASS — 106 bench\(es\) produced Ir/);
    const counts = new Map();
    for (const line of log.split(/\r?\n/)) {
      const match = /^\s{2}([A-Za-z0-9_]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+(-|[\d,.]+)\s*$/.exec(line);
      if (match) counts.set(match[1], { ir: Number(match[2].replaceAll(',', '')), cycles: Number(match[6].replaceAll(',', '')), marginal: match[7] === '-' ? null : Number(match[7].replaceAll(',', '')) });
    }
    assert.equal(counts.size, 106);
    for (const row of csvFile('iai_comparison.csv')) {
      const v = counts.get(row.bench); assert.ok(v, `missing IAI ${row.bench}`);
      close(v.ir, row[`${arm}_Ir_total`], `IAI ${row.bench} ${arm} Ir`);
      close(v.cycles, row[`${arm}_EstCycles`], `IAI ${row.bench} ${arm} cycles`);
      if (v.marginal != null) close(v.marginal, row[`${arm}_marginal_Ir_per_op`], 'IAI marginal');
    }
    for (const row of csvFile('iai_marginal_comparison.csv')) {
      const a = counts.get(row.workload), b = counts.get(`mimalloc_${row.workload}`);
      assert.ok(a?.marginal > 0 && b?.marginal > 0);
      close(a.marginal, row[`${arm}_Sefer_marginal_Ir_op`], 'Sefer marginal');
      close(b.marginal, row[`${arm}_mimalloc_marginal_Ir_op`], 'mimalloc marginal');
      assert.ok(Math.abs(a.marginal / b.marginal - Number(row[`${arm}_ratio_Sefer_over_mimalloc`])) < 0.002, 'Округлённый IAI ratio не соответствует marginal');
    }
  }
  console.log('PASS: registration, MT, retained-pin, installed-global, foreign-free, first-allocation и IAI summaries сверены с raw samples/logs.');
}
