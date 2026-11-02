#!/usr/bin/env node
// Own-thread free lane model report (measurement only, counts). Reads docs/perf/segmodel/win_raw.jsonl
// (docs/perf/segmodel/run.mjs, judge `WJ_SEGMODEL=1`) and prints, per scenario and modelled cache shape,
// the share of small-block frees (numerator: frees of blocks up to 1 KiB resolved by the memo, by an
// own_cache hit, or by a hash probe; denominator: all small-block frees of the run) as the median over runs.
// Usage: node scripts/segmodel_report.mjs [--csv FILE]
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const recs = readFileSync(join(REPO_ROOT, 'docs', 'perf', 'segmodel', 'win_raw.jsonl'), 'utf8')
  .trim().split(/\r?\n/).map((l) => JSON.parse(l));
const med = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; };
const pct = (n, d) => (d ? (100 * n / d).toFixed(1) : 'n/a');
const csv = [];
const SC = ['wild', 'growmix', 'phase', 'regrow', 'grow64', 'grow1k'];
console.log('| Scenario | small frees | distinct segments | odd keys | occupied slots of 16 | shape | memo | own_cache hit | hash probe (count) |');
console.log('|---|---:|---:|---:|---:|---|---:|---:|---:|');
for (const sc of SC) {
  const rs = recs.filter((r) => r.scenario === sc);
  assert.ok(rs.length >= 1, sc);
  const m0 = rs[0].r.segmodel;
  const small = med(rs.map((r) => r.r.segmodel.small));
  for (let i = 0; i < m0.v.length; i++) {
    const memo = med(rs.map((r) => r.r.segmodel.v[i][1]));
    const cache = med(rs.map((r) => r.r.segmodel.v[i][2]));
    const hash = med(rs.map((r) => r.r.segmodel.v[i][3]));
    const name = m0.v[i][0];
    // every small free is resolved by exactly one of the three (per run); check on the run's own numbers
    for (const r of rs) {
      const v = r.r.segmodel.v[i];
      assert.equal(v[1] + v[2] + v[3], r.r.segmodel.small, `${sc}/${name}: partition`);
    }
    console.log(`| ${sc} | ${small} | ${m0.distinct} | ${m0.odd} | ${m0.slots16} | ${name} | ${pct(memo, small)} % | ${pct(cache, small)} % | ${pct(hash, small)} % (${hash}) |`);
    csv.push([sc, small, m0.distinct, m0.odd, m0.slots16, name, pct(memo, small), pct(cache, small), pct(hash, small), hash].join(','));
  }
}
const i = process.argv.indexOf('--csv');
if (i >= 0) writeFileSync(process.argv[i + 1], ['scenario,small_frees,distinct_segments,odd_keys,occupied_slots16,shape,memo_pct,cache_pct,hash_pct,hash_count', ...csv].join('\n') + '\n');
