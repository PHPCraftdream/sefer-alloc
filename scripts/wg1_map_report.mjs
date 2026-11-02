#!/usr/bin/env node
// WG-1 step 1 report (measurement only): Windows sefer / mimalloc per judge scenario on
// current `main`, next to the LM-1 reading taken before AO-1 and WILD-2.
// Reads docs/perf/wg1/win_raw.jsonl (docs/perf/wg1/run.mjs) and
// docs/perf/LM1_LOSS_MAP_summary.csv (rows `judge-win`).
// Per block and arm the minimum of ns/op over the repeats; per block d = ln(sefer / mi);
// the cell is exp(mean d) with a two-sided 95 % t-interval (t, n-1 degrees of freedom).
// Usage: node scripts/wg1_map_report.mjs [--csv FILE]
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const T975 = { 3: 3.182, 5: 2.571, 7: 2.365, 9: 2.262, 11: 2.201, 15: 2.131 };
const recs = readFileSync(join(REPO_ROOT, 'docs', 'perf', 'wg1', 'win_raw.jsonl'), 'utf8')
  .trim().split(/\r?\n/).map((l) => JSON.parse(l));
const lm1 = {};
for (const l of readFileSync(join(REPO_ROOT, 'docs', 'perf', 'LM1_LOSS_MAP_summary.csv'), 'utf8').trim().split(/\r?\n/).slice(1)) {
  const c = l.split(',');
  if (c[0] === 'judge-win') lm1[c[1]] = { r: +c[2], lo: +c[3], hi: +c[4] };
}
const blocks = [...new Set(recs.map((r) => r.block))].sort((a, b) => a - b);
const t = T975[blocks.length - 1];
assert.ok(t, `no t quantile for n=${blocks.length}`);
const scenarios = ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large'];
assert.deepEqual([...new Set(recs.map((r) => r.r.scenario))].sort(), [...scenarios].sort());
const rows = [];
console.log('| Scenario | sefer / mimalloc on `main` now (exp mean d) | 95 % CI | LM-1 reading before AO-1 and WILD-2 | LM-1 95 % CI | blocks x repeats |');
console.log('|---|---:|---|---:|---|---|');
for (const sc of scenarios) {
  const d = blocks.map((b) => {
    const mn = (arm) => {
      const v = recs.filter((r) => r.block === b && r.arm === arm && r.r.scenario === sc).map((r) => r.r.ns_per_op);
      assert.ok(v.length >= 1);
      return Math.min(...v);
    };
    return Math.log(mn('sefer') / mn('mi'));
  });
  const n = d.length, m = d.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(d.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1));
  const half = t * sd / Math.sqrt(n);
  const reps = recs.filter((r) => r.block === blocks[0] && r.arm === 'sefer' && r.r.scenario === sc).length;
  assert.ok(lm1[sc], `LM-1 row for ${sc}`);
  rows.push({ sc, r: Math.exp(m), lo: Math.exp(m - half), hi: Math.exp(m + half), n, reps });
  console.log(`| ${sc} | ${Math.exp(m).toFixed(3)} | ${Math.exp(m - half).toFixed(3)}–${Math.exp(m + half).toFixed(3)} | ${lm1[sc].r.toFixed(3)} | ${lm1[sc].lo.toFixed(3)}–${lm1[sc].hi.toFixed(3)} | ${n} x ${reps} |`);
}
const i = process.argv.indexOf('--csv');
if (i >= 0) {
  writeFileSync(process.argv[i + 1], ['scenario,sefer_over_mi,ci_lo,ci_hi,lm1_before,lm1_lo,lm1_hi,blocks,repeats',
    ...rows.map((r) => [r.sc, r.r.toFixed(3), r.lo.toFixed(3), r.hi.toFixed(3), lm1[r.sc].r.toFixed(3), lm1[r.sc].lo.toFixed(3), lm1[r.sc].hi.toFixed(3), r.n, r.reps].join(','))].join('\n') + '\n');
}
