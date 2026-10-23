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
const runSpecs = [
  { id: 'before', dir: 'raw/before-clean-2', commit: passport.baseline_commit, tree: passport.baseline_tree },
  { id: 'after', dir: 'raw/after-clean-2', commit: passport.candidate_commit, tree: passport.candidate_tree },
];

function csv(value) {
  const text = String(value ?? '');
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

function parseRows(log, label) {
  assert.match(log, /\[iai\] PASS — 106 bench\(es\) produced Ir/, `${label}: iai run failed or was incomplete`);
  const lines = log.split(/\r?\n/);
  const header = lines.findIndex((line) => /^\s*bench\s+Ir\s+L1\s+L2\s+RAM\s+EstCycles\s+Ir\/op\*/.test(line));
  assert.ok(header >= 0, `${label}: final summary table missing`);
  const rows = new Map();
  const rowPattern = /^\s{2}([A-Za-z0-9_]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+([\d,]+)\s+(-|[\d,.]+)\s*$/;
  for (const line of lines.slice(header + 2)) {
    const match = rowPattern.exec(line);
    if (!match) continue;
    const [, name, ir, l1, l2, ram, cycles, marginal] = match;
    assert.ok(!rows.has(name), `${label}: duplicate benchmark ${name}`);
    rows.set(name, {
      ir: Number(ir.replaceAll(',', '')),
      l1: Number(l1.replaceAll(',', '')),
      l2: Number(l2.replaceAll(',', '')),
      ram: Number(ram.replaceAll(',', '')),
      cycles: Number(cycles.replaceAll(',', '')),
      marginal: marginal === '-' ? null : Number(marginal.replaceAll(',', '')),
    });
  }
  assert.equal(rows.size, 106, `${label}: expected 106 summary rows`);
  return rows;
}

function parseRatios(log, label) {
  const lines = log.split(/\r?\n/);
  const marker = lines.findIndex((line) => line.includes('Sefer/mimalloc marginal Ir/op ratio'));
  assert.ok(marker >= 0, `${label}: marginal ratio table missing`);
  const rows = new Map();
  const rowPattern = /^\s{2}([A-Za-z0-9_]+)\s+([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*$/;
  for (const line of lines.slice(marker + 3)) {
    const match = rowPattern.exec(line);
    if (match) rows.set(match[1], { sefer: Number(match[2]), mimalloc: Number(match[3]), ratio: Number(match[4]) });
  }
  assert.equal(rows.size, 6, `${label}: expected six workload-matched marginal ratios`);
  return rows;
}

const data = new Map();
for (const run of runSpecs) {
  const log = fs.readFileSync(path.join(root, run.dir, 'iai.log'), 'utf8');
  data.set(run.id, {
    run,
    rows: parseRows(log, run.id),
    ratios: parseRatios(log, run.id),
    log,
  });
}
const before = data.get('before');
const after = data.get('after');
assert.deepEqual([...before.rows.keys()].sort(), [...after.rows.keys()].sort(), 'before/after bench sets differ');

const result = [];
for (const [name, b] of before.rows) {
  const a = after.rows.get(name);
  const deltaPct = (a.ir - b.ir) / b.ir * 100;
  const cycleDeltaPct = (a.cycles - b.cycles) / b.cycles * 100;
  assert.ok(Math.abs((a.ir / b.ir) * b.ir - a.ir) <= Math.max(1, a.ir) * 1e-12);
  result.push({
    name,
    beforeIr: b.ir,
    afterIr: a.ir,
    deltaPct,
    beforeCycles: b.cycles,
    afterCycles: a.cycles,
    cycleDeltaPct,
    beforeMarginal: b.marginal,
    afterMarginal: a.marginal,
    marginalDeltaPct: b.marginal != null && a.marginal != null && b.marginal !== 0
      ? (a.marginal - b.marginal) / b.marginal * 100
      : null,
  });
}

const head = ['bench', 'before_commit', 'after_commit', 'before_tree', 'after_tree', 'features', 'before_Ir_total', 'after_Ir_total', 'delta_pct_(after-before)/before', 'before_EstCycles', 'after_EstCycles', 'EstCycles_delta_pct_(after-before)/before', 'before_marginal_Ir_per_op', 'after_marginal_Ir_per_op', 'marginal_delta_pct_(after-before)/before'];
fs.writeFileSync(path.join(root, 'iai_comparison.csv'), [head.join(','), ...result.map((row) => [row.name, before.run.commit, after.run.commit, before.run.tree, after.run.tree, 'production bench-internals internals', row.beforeIr, row.afterIr, row.deltaPct, row.beforeCycles, row.afterCycles, row.cycleDeltaPct, row.beforeMarginal, row.afterMarginal, row.marginalDeltaPct].map(csv).join(','))].join('\n') + '\n');

const workloads = ['small_churn_16b', 'churn_256b', 'cold_alloc_free_256x16b', 'cold_alloc_free_256x64b', 'recycle_alloc_free_256x16b', 'recycle_alloc_free_256x64b'];
const marginalRows = workloads.map((workload) => {
  const beforeRatio = before.ratios.get(workload);
  const afterRatio = after.ratios.get(workload);
  assert.ok(beforeRatio && afterRatio, `${workload}: missing workload-matched ratio`);
  for (const [run, parsed] of [['before', before], ['after', after]]) {
    const seferName = workload;
    const miName = `mimalloc_${workload}`;
    const sefer = parsed.rows.get(seferName);
    const mimalloc = parsed.rows.get(miName);
    assert.ok(sefer && mimalloc && sefer.marginal != null && mimalloc.marginal != null, `${run}/${workload}: missing marginal data`);
    const ratio = sefer.marginal / mimalloc.marginal;
    assert.ok(Math.abs(ratio - parsed.ratios.get(workload).ratio) < 0.002, `${run}/${workload}: emitted marginal ratio disagrees with source counts`);
  }
  const bSefer = before.rows.get(workload).marginal;
  const bMi = before.rows.get(`mimalloc_${workload}`).marginal;
  const aSefer = after.rows.get(workload).marginal;
  const aMi = after.rows.get(`mimalloc_${workload}`).marginal;
  return { workload, bSefer, bMi, beforeRatio: beforeRatio.ratio, aSefer, aMi, afterRatio: afterRatio.ratio };
});
const marginalHead = ['workload', 'before_Sefer_marginal_Ir_op', 'before_mimalloc_marginal_Ir_op', 'before_ratio_Sefer_over_mimalloc', 'after_Sefer_marginal_Ir_op', 'after_mimalloc_marginal_Ir_op', 'after_ratio_Sefer_over_mimalloc'];
fs.writeFileSync(path.join(root, 'iai_marginal_comparison.csv'), [marginalHead.join(','), ...marginalRows.map((row) => [row.workload, row.bSefer, row.bMi, row.beforeRatio, row.aSefer, row.aMi, row.afterRatio].map(csv).join(','))].join('\n') + '\n');

console.log(`PASS: ${result.length} total-Ir/cycle rows and ${marginalRows.length} verified workload-matched marginal ratios; wrote CSV outputs.`);
