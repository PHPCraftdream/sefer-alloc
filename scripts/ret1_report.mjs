#!/usr/bin/env node
// RET-1 measurement A report (measurement only). Reads docs/perf/ret1/win_raw.jsonl
// (docs/perf/ret1/run.mjs) and prints, per scenario and pool size, the median over
// repetitions of the per-repetition ratio to the default pool (4) for (a) the judge
// window (ns/op) and (b) the whole process (spawn to exit), with ranges, the memory
// left after the run, and the share of the window gain that survives at process
// level. Usage: node scripts/ret1_report.mjs [--csv FILE]
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const recs = readFileSync(join(REPO_ROOT, 'docs', 'perf', 'ret1', 'win_raw.jsonl'), 'utf8')
  .trim().split(/\r?\n/).map((l) => JSON.parse(l));
const med = (a) => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const f3 = (v) => v.toFixed(3);
const scenarios = [...new Set(recs.map((r) => r.scenario))];
const pools = [...new Set(recs.map((r) => r.pool))].sort((a, b) => a - b);
assert.deepEqual(pools, [4, 16, 64, 1024]);
const rows = [];
console.log('| Scenario | pool | window ratio to pool 4 (median of per-rep ratios) | window range | process ratio to pool 4 | process range | share of window gain kept at process level | RSS left after run, MiB (median) | pool-4 process ms (median) |');
console.log('|---|---:|---:|---|---:|---|---:|---:|---:|');
for (const sc of scenarios) {
  const by = (p) => recs.filter((r) => r.scenario === sc && r.pool === p).sort((a, b) => a.rep - b.rep);
  const base = by(4);
  const reps = base.length;
  for (const p of pools.filter((x) => x !== 4)) {
    const x = by(p);
    assert.equal(x.length, reps, `${sc}/${p}: equal repetitions`);
    assert.equal(new Set([...base, ...x].map((r) => r.r.checksum)).size, 1, `${sc}: same op sequence`);
    const w = x.map((r, i) => r.r.ns_per_op / base[i].r.ns_per_op);
    const q = x.map((r, i) => r.proc_ms / base[i].proc_ms);
    const wm = med(w), qm = med(q);
    const share = wm < 1 ? (1 - qm) / (1 - wm) : null;
    const rss = med(x.map((r) => r.r.end_rss)) / 1048576;
    const bms = med(base.map((r) => r.proc_ms));
    if (share !== null) assert.ok(Math.abs(share - (1 - qm) / (1 - wm)) < 1e-12);
    rows.push({ sc, p, reps, wm, wlo: Math.min(...w), whi: Math.max(...w), qm, qlo: Math.min(...q), qhi: Math.max(...q), share, rss, bms });
    console.log(`| ${sc} | ${p} | ${f3(wm)} | ${f3(Math.min(...w))}–${f3(Math.max(...w))} | ${f3(qm)} | ${f3(Math.min(...q))}–${f3(Math.max(...q))} | ${share === null ? 'n/a (no window gain)' : (100 * share).toFixed(0) + ' %'} | ${rss.toFixed(0)} | ${bms.toFixed(0)} |`);
  }
}
const i = process.argv.indexOf('--csv');
if (i >= 0) writeFileSync(process.argv[i + 1], ['scenario,pool,reps,window_ratio,window_lo,window_hi,process_ratio,process_lo,process_hi,share_kept,rss_left_mib,pool4_process_ms',
  ...rows.map((r) => [r.sc, r.p, r.reps, f3(r.wm), f3(r.wlo), f3(r.whi), f3(r.qm), f3(r.qlo), f3(r.qhi), r.share === null ? '' : r.share.toFixed(3), r.rss.toFixed(0), r.bms.toFixed(0)].join(','))].join('\n') + '\n');
