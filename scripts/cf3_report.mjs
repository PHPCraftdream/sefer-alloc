#!/usr/bin/env node
// CF-3 report generator (measurement only): tables and summary CSV of
// docs/perf/CF3_HUGE_PAGE_THRESHOLD.md, derived from the raw per-run records of
// examples/g1_wild_judge.rs (arms t<N> = sefer_pool built with WJ_THP=N and the
// default pool; sefer = SeferAlloc::new(); mi = mimalloc; sefer and t16 run the
// SAME binary and the same configuration, so their difference is the noise floor).
// Asserts: same op sequence in every arm, resolved threshold == requested one,
// no config conflicts, hinted-segment counter ordered by threshold.
//
// Usage: node scripts/cf3_report.mjs RAW.jsonl EVIDENCE.jsonl MECH.jsonl OUT_DIR
//        node scripts/cf3_report.mjs --assemble OUT_DIR   (head + tables + tail -> report)
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const NL = String.fromCharCode(10);
if (process.argv[2] === '--assemble') {
  const d = process.argv[3];
  const t = (f) => readFileSync(join(d, 'cf3', f), 'utf8');
  writeFileSync(join(d, 'CF3_HUGE_PAGE_THRESHOLD.md'), [t('doc_head.md'), '## 2. Tables', '', t('cf3_tables.md'), t('doc_tail.md')].join(NL));
  process.exit(0);
}
const [rawPath, evPath, mechPath, outDir] = process.argv.slice(2);
if (!outDir) throw new Error('usage: cf3_report.mjs RAW EVIDENCE MECH OUT_DIR');
const SHA = readFileSync(join(outDir, 'cf3', 'tree.txt'), 'utf8').trim();
if (!/^[0-9a-f]{40}$/.test(SHA)) throw new Error('tree.txt is not a 40-hex commit SHA');
const lines = (p) => readFileSync(p, 'utf8').split(NL).filter(Boolean).map((l) => JSON.parse(l));
const raw = lines(rawPath);
const ev = lines(evPath);
const mech = lines(mechPath);

for (const e of ev) {
  if (e.out.thp_req !== e.want || e.out.thp_res !== e.want) throw new Error(`threshold evidence: want ${e.want}, req ${e.out.thp_req}, res ${e.out.thp_res}`);
  if (e.out.pool_req !== 4 || e.out.pool_res !== 4) throw new Error(`pool must stay at the default 4, got ${e.out.pool_req}/${e.out.pool_res}`);
  if (e.out.config_conflicts !== 0) throw new Error('config conflicts');
}
const median = (a) => { const s = [...a].sort((x, y) => x - y); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const LABELS = ['sefer', 't0', 't1', 't4', 't16', 't64', 't1024', 'mi'];
const SCEN = [...new Set(raw.map((r) => r.run.scenario))];
const mib = (b) => (b / 2 ** 20).toFixed(0);
const rows = [];
for (const sc of SCEN) {
  const of = (l) => raw.filter((r) => r.run.scenario === sc && r.label === l);
  if (new Set(raw.filter((r) => r.run.scenario === sc).map((r) => r.run.checksum)).size !== 1) throw new Error(`${sc}: arms ran different op sequences`);
  const n = of('t16').length;
  for (const l of LABELS) if (of(l).length !== n) throw new Error(`${sc} ${l}: run count differs`);
  for (const l of LABELS) {
    const v = of(l);
    const ratios = v.map((r) => r.run.ns_per_op / of('t16').find((b) => b.rep === r.rep).run.ns_per_op);
    rows.push({
      scenario: sc, arm: l, runs: v.length,
      ns_med: median(v.map((r) => r.run.ns_per_op)), ns_min: Math.min(...v.map((r) => r.run.ns_per_op)),
      ratio: median(ratios), rmin: Math.min(...ratios), rmax: Math.max(...ratios),
      peak: median(v.map((r) => r.run.peak_rss)), end: median(v.map((r) => r.run.end_rss)),
      huge: median(v.map((r) => r.run.anon_huge_peak ?? 0)), faults: median(v.map((r) => r.faults)),
    });
  }
}
const get = (sc, l) => rows.find((r) => r.scenario === sc && r.arm === l);
for (const r of rows) {
  r.ns_vs_mi = r.ns_med / get(r.scenario, 'mi').ns_med;
  r.faults_vs_t16 = r.faults / get(r.scenario, 't16').faults;
  r.peak_vs_t16 = r.peak / get(r.scenario, 't16').peak;
  if (Math.abs(r.faults_vs_t16 * get(r.scenario, 't16').faults - r.faults) > 1e-6) throw new Error('ratio mismatch');
}
writeFileSync(join(outDir, 'CF3_HUGE_PAGE_THRESHOLD_summary.csv'), [`# commit_sha=${SHA}`,
  'scenario,arm,runs,ns_min,ns_median,median_of_per_rep_ratio_vs_t16,ratio_min,ratio_max,ns_median_over_mi_median,peak_rss_median,peak_rss_over_t16,end_rss_median,huge_page_bytes_at_peak_median,minor_faults_median,minor_faults_over_t16',
  ...rows.map((r) => [r.scenario, r.arm, r.runs, r.ns_min.toFixed(3), r.ns_med.toFixed(3), r.ratio.toFixed(4), r.rmin.toFixed(4), r.rmax.toFixed(4), r.ns_vs_mi.toFixed(4), r.peak, r.peak_vs_t16.toFixed(4), r.end, r.huge, r.faults, r.faults_vs_t16.toFixed(4)].join(','))].join(NL) + NL);

const table = (title, cell) => {
  const out = [`**${title}**`, '', `| scenario | ${LABELS.join(' | ')} |`, `|---|${LABELS.map(() => '---:').join('|')}|`];
  for (const sc of SCEN) out.push(`| ${sc} | ${LABELS.map((l) => cell(get(sc, l))).join(' | ')} |`);
  return out.join(NL);
};

// mechanism: hinted segments per threshold (bench-internals build)
const NS = [0, 1, 4, 16, 64, 1024];
const mrows = [];
for (const sc of SCEN) {
  let prev = null;
  for (const n of NS) {
    const v = mech.filter((m) => m.run.scenario === sc && m.want === n);
    if (!v.length) throw new Error(`mech ${sc} ${n}: no runs`);
    for (const m of v) if (m.run.thp_req !== n || m.run.thp_res !== n || m.run.config_conflicts !== 0) throw new Error(`mech ${sc} ${n}: config evidence`);
    const hinted = median(v.map((m) => m.run.thp_hinted));
    if (n === 0 && hinted !== 0) throw new Error(`${sc}: threshold 0 hinted ${hinted}`);
    if (n >= 1 && prev !== null && hinted > prev) throw new Error(`${sc}: hinted not ordered by threshold (${n})`);
    if (n >= 1) prev = hinted;
    mrows.push({ scenario: sc, thr: n, runs: v.length, hinted, huge: median(v.map((m) => m.run.anon_huge_peak)), faults: median(v.map((m) => m.faults)), reserved: median(v.map((m) => m.run.seg_reserved)) });
  }
}
const mget = (sc, n) => mrows.find((r) => r.scenario === sc && r.thr === n);
writeFileSync(join(outDir, 'CF3_HUGE_PAGE_THRESHOLD_mech_summary.csv'), [`# commit_sha=${SHA}`, 'scenario,threshold,runs,segments_hinted_median,huge_page_bytes_at_peak_median,minor_faults_median,seg_reserved_median',
  ...mrows.map((r) => [r.scenario, r.thr, r.runs, r.hinted, r.huge, r.faults, r.reserved].join(','))].join(NL) + NL);
const mtable = (title, cell) => {
  const out = [`**${title}**`, '', `| scenario | ${NS.map((n) => `threshold ${n}`).join(' | ')} |`, `|---|${NS.map(() => '---:').join('|')}|`];
  for (const sc of SCEN) out.push(`| ${sc} | ${NS.map((n) => cell(mget(sc, n))).join(' | ')} |`);
  return out.join(NL);
};
const md = [
  table('Median ns/op, ratio to `t16` of the same repetition (the default threshold; median of per-repetition ratios; `sefer` is the same configuration in the same binary: its spread is the noise floor; < 1 = faster than the default)', (r) => `${r.ratio.toFixed(3)} (${r.rmin.toFixed(2)}–${r.rmax.toFixed(2)})`),
  '',
  table('Median ns/op over mimalloc (median ns of the arm / median ns of `mi`; < 1 = faster than mimalloc)', (r) => r.ns_vs_mi.toFixed(3)),
  '',
  table('Median minor page faults of the process (3 or 9 runs per cell)', (r) => r.faults),
  '',
  table('Median peak RSS, MiB', (r) => mib(r.peak)),
  '',
  table('Median RSS after the run, MiB', (r) => mib(r.end)),
  '',
  table('Median huge-page-backed bytes at the live peak, MiB (`AnonHugePages`)', (r) => mib(r.huge)),
  '',
  mtable('Small segments the hint was issued for (counter `THP_HINTED_SMALL_SEGMENTS`, median of 3 runs)', (r) => r.hinted),
  '',
].join(NL);
writeFileSync(join(outDir, 'cf3', 'cf3_tables.md'), md);
console.log(md);
