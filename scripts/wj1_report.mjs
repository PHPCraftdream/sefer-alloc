#!/usr/bin/env node
// WJ-1 report tables (measurement only), derived from the committed raw data:
// - the wild-judge A/B (docs/perf/wj1/wild/ab_rereserve_raw.jsonl: tags base /
//   rr / mi, one JSON line per process): min ns/op per tag, ratios, peak RSS;
// - the span-return probe (docs/perf/wj1/vfree_probe.jsonl): median setup and
//   return µs per shape, size and touch.
// Usage: node scripts/wj1_report.mjs [--csv DIR]  (writes the two summary CSVs)
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const dir = join(REPO_ROOT, 'docs', 'perf', 'wj1');
const lines = (f) => readFileSync(f, 'utf8').trim().split(/\r?\n/).map((l) => JSON.parse(l));
const csvDir = (() => { const i = process.argv.indexOf('--csv'); return i >= 0 ? process.argv[i + 1] : null; })();

const ab = lines(join(dir, 'wild', 'ab_rereserve_raw.jsonl'));
const scen = [...new Set(ab.map((r) => r.scenario))];
const rows = scen.map((s) => {
  const of = (t) => ab.filter((r) => r.scenario === s && r.tag === t);
  const sums = new Set(ab.filter((r) => r.scenario === s).map((r) => r.checksum));
  assert.equal(sums.size, 1, `${s}: every run must execute the same op sequence`);
  const ns = (t) => Math.min(...of(t).map((r) => r.ns_per_op));
  const rss = (t) => of(t)[0].peak_rss;
  const r = { s, runs: of('rr').length, base: ns('base'), rr: ns('rr'), mi: ns('mi'), rssRr: rss('rr'), rssMi: rss('mi') };
  r.rrBase = r.rr / r.base;
  r.rrMi = r.rr / r.mi;
  r.baseMi = r.base / r.mi;
  assert.ok(Math.abs(r.rrBase * r.base - r.rr) < 1e-9);
  return r;
});
console.log('| Scenario | base ns/op | re-reserve ns/op | mimalloc ns/op | re-reserve / base | base / mi | re-reserve / mi | peak RSS re-reserve / mi |');
console.log('|---|---:|---:|---:|---:|---:|---:|---:|');
for (const r of rows) {
  console.log(`| ${r.s} | ${r.base.toFixed(1)} | ${r.rr.toFixed(1)} | ${r.mi.toFixed(1)} | ${r.rrBase.toFixed(3)} | ${r.baseMi.toFixed(3)} | ${r.rrMi.toFixed(3)} | ${(r.rssRr / r.rssMi).toFixed(2)} |`);
}

const vf = lines(join(dir, 'vfree_probe.jsonl'));
console.log('\n| Shape | span MiB | pages touched | setup µs (median) | return µs (median) |');
console.log('|---|---:|---|---:|---:|');
for (const v of vf) {
  console.log(`| ${v.shape} | ${v.size_mib} | ${v.touch} | ${(v.setup_ns_median / 1000).toFixed(1)} | ${(v.return_ns_median / 1000).toFixed(1)} |`);
}

if (csvDir) {
  writeFileSync(join(csvDir, 'G1_WJ1_WINDOWS_SPAN_RESERVE_summary.csv'),
    ['scenario,runs_per_arm,base_ns_min,rereserve_ns_min,mi_ns_min,rereserve_over_base,base_over_mi,rereserve_over_mi,rereserve_peak_rss,mi_peak_rss',
      ...rows.map((r) => [r.s, r.runs, r.base, r.rr, r.mi, r.rrBase.toFixed(4), r.baseMi.toFixed(4), r.rrMi.toFixed(4), r.rssRr, r.rssMi].join(','))].join('\n') + '\n');
  writeFileSync(join(csvDir, 'G1_WJ1_WINDOWS_SPAN_RESERVE_probe_summary.csv'),
    ['shape,size_mib,touch,iters,setup_ns_median,setup_ns_min,return_ns_median,return_ns_min',
      ...vf.map((v) => [v.shape, v.size_mib, v.touch, v.iters, v.setup_ns_median, v.setup_ns_min, v.return_ns_median, v.return_ns_min].join(','))].join('\n') + '\n');
}
