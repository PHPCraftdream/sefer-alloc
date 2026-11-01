#!/usr/bin/env node
// TH-1 report tables (measurement only), derived from the committed raw data
// in docs/perf/th1/impl: run1 (all 7 wild-judge scenarios, 4 runs per sefer
// arm, 2 mimalloc), run2 (grow64/grow1k/large/phase) and run3
// (growmix/regrow/wild), 8 runs per sefer arm, 4 mimalloc,
// plus the g1_mt_one T=1 wall times (ms). Statistic: min and median ns/op per
// arm; ratios name both arms. Usage: node scripts/th1_report.mjs [--csv DIR]
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const dir = join(REPO_ROOT, 'docs', 'perf', 'th1', 'impl');
const jsonl = (f) => readFileSync(join(dir, f), 'utf8').trim().split(/\r?\n/).map((l) => JSON.parse(l));
const med = (a) => { const s = [...a].sort((x, y) => x - y); return s[s.length >> 1]; };
const MIB = 1 << 20;
const csvDir = (() => { const i = process.argv.indexOf('--csv'); return i >= 0 ? process.argv[i + 1] : null; })();

const rows = [];
for (const run of ['run1', 'run2', 'run3']) {
  const recs = jsonl(`${run}_raw.jsonl`);
  for (const s of [...new Set(recs.map((x) => x.r.scenario))]) {
    const of = (t) => recs.filter((x) => x.r.scenario === s && x.tag === t);
    const sums = new Set(recs.filter((x) => x.r.scenario === s).map((x) => x.r.checksum));
    assert.equal(sums.size, 1, `${run}/${s}: every run executes the same op sequence`);
    const ns = (t) => of(t).map((x) => x.r.ns_per_op);
    const r = {
      run, s, n: of('x').length,
      baseMin: Math.min(...ns('base')), thMin: Math.min(...ns('x')), miMin: Math.min(...ns('mi')),
      baseMed: med(ns('base')), thMed: med(ns('x')),
      pfBase: med(of('base').map((x) => x.page_faults)), pfTh: med(of('x').map((x) => x.page_faults)), pfMi: med(of('mi').map((x) => x.page_faults)),
      hugeBase: Math.max(...of('base').map((x) => x.r.anon_huge_peak)), hugeTh: Math.max(...of('x').map((x) => x.r.anon_huge_peak)),
      peakBase: Math.max(...of('base').map((x) => x.r.peak_rss)), peakTh: Math.max(...of('x').map((x) => x.r.peak_rss)), peakMi: Math.max(...of('mi').map((x) => x.r.peak_rss)),
    };
    r.minRatio = r.thMin / r.baseMin;
    r.medRatio = r.thMed / r.baseMed;
    assert.ok(Math.abs(r.minRatio * r.baseMin - r.thMin) < 1e-9);
    rows.push(r);
  }
}
console.log('| Run | Scenario | runs/arm | base min / med ns | TH-1 min / med ns | mimalloc min ns | TH-1 / base (min) | TH-1 / base (median) | base / mi (min) | TH-1 / mi (min) | page faults base / TH-1 / mi | THP peak MiB base / TH-1 | peak RSS MiB base / TH-1 / mi |');
console.log('|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---|---|---|');
for (const r of rows) {
  console.log(`| ${r.run} | ${r.s} | ${r.n} | ${r.baseMin.toFixed(1)} / ${r.baseMed.toFixed(1)} | ${r.thMin.toFixed(1)} / ${r.thMed.toFixed(1)} | ${r.miMin.toFixed(1)} | ${r.minRatio.toFixed(3)} | ${r.medRatio.toFixed(3)} | ${(r.baseMin / r.miMin).toFixed(3)} | ${(r.thMin / r.miMin).toFixed(3)} | ${r.pfBase} / ${r.pfTh} / ${r.pfMi} | ${Math.round(r.hugeBase / MIB)} / ${Math.round(r.hugeTh / MIB)} | ${Math.round(r.peakBase / MIB)} / ${Math.round(r.peakTh / MIB)} / ${Math.round(r.peakMi / MIB)} |`);
}
const mt = [];
for (const run of ['run1', 'run2', 'run3']) {
  const L = readFileSync(join(dir, `${run}_mt.txt`), 'utf8').trim().split(/\r?\n/).map((l) => l.split(' '));
  for (const w of ['mstress', 'larson']) {
    const f = (t) => L.filter((m) => m[0] === t && m[1] === w).map((m) => +m[2]);
    mt.push({ run, w, n: f('x').length, base: med(f('base')), th: med(f('x')) });
  }
}
console.log('\n| Run | g1_mt_one T=1 | runs/arm | base median ms | TH-1 median ms | TH-1 / base |');
console.log('|---|---|---:|---:|---:|---:|');
for (const m of mt) console.log(`| ${m.run} | ${m.w} | ${m.n} | ${m.base} | ${m.th} | ${(m.th / m.base).toFixed(3)} |`);

// Probe stages (docs/perf/th1/probe, scratch builds, TH1_OFF switched the hint
// off in the same binary): th1 = every reservation, th1b = small segments,
// th1c = from 16 live segments of any kind, th1d = from the 16th small segment.
const probeDir = join(REPO_ROOT, 'docs', 'perf', 'th1', 'probe');
console.log('\n| Probe stage | Scenario | off min ns | on min ns | on / off (min) | peak RSS MiB off / on |');
console.log('|---|---|---:|---:|---:|---|');
for (const st of ['th1', 'th1b', 'th1c', 'th1d']) {
  const P = readFileSync(join(probeDir, `${st}.txt`), 'utf8').trim().split(/\r?\n/).map((l) => {
    const [tag, sc, pf, ...rest] = l.split(' ');
    return { tag, sc, pf: +pf, r: JSON.parse(rest.join(' ')) };
  });
  for (const s of [...new Set(P.map((x) => x.sc))]) {
    const of = (t) => P.filter((x) => x.sc === s && x.tag === t);
    const off = Math.min(...of('off').map((x) => x.r.ns_per_op));
    const on = Math.min(...of('on').map((x) => x.r.ns_per_op));
    const pk = (t) => Math.round(Math.max(...of(t).map((x) => x.r.peak_rss)) / MIB);
    console.log(`| ${st} | ${s} | ${off.toFixed(1)} | ${on.toFixed(1)} | ${(on / off).toFixed(3)} | ${pk('off')} / ${pk('on')} |`);
  }
}

if (csvDir) {
  writeFileSync(join(csvDir, 'G1_TH1_LINUX_THP_summary.csv'),
    ['run,scenario,runs_per_arm,base_ns_min,th1_ns_min,mi_ns_min,base_ns_median,th1_ns_median,th1_over_base_min,th1_over_base_median,page_faults_base,page_faults_th1,page_faults_mi,thp_peak_base,thp_peak_th1,peak_rss_base,peak_rss_th1,peak_rss_mi',
      ...rows.map((r) => [r.run, r.s, r.n, r.baseMin, r.thMin, r.miMin, r.baseMed, r.thMed, r.minRatio.toFixed(4), r.medRatio.toFixed(4), r.pfBase, r.pfTh, r.pfMi, r.hugeBase, r.hugeTh, r.peakBase, r.peakTh, r.peakMi].join(',')),
      ...mt.map((m) => [m.run, `mt_${m.w}`, m.n, '', '', '', m.base, m.th, '', (m.th / m.base).toFixed(4), '', '', '', '', '', '', '', ''].join(','))].join('\n') + '\n');
}
