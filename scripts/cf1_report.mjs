#!/usr/bin/env node
// CF-1 report generator (measurement only): derives every table and the summary
// CSV of docs/perf/CF1_POOL_SEGMENTS.md from the raw per-run records of
// examples/g1_wild_judge.rs (arms p<N> = sefer_pool built with WJ_POOL=N,
// sefer = SeferAlloc::new(), mi = mimalloc). Asserts the evidence the report
// relies on: same op sequence in every arm, resolved cap == requested cap.
//
// The mechanism oracle (MECH.jsonl: segment reserve/release counters and minor page faults per pool size,
// bench-internals builds) is deterministic and layout-insensitive; the wall-clock table is not.
//
// Usage: node scripts/cf1_report.mjs RAW.jsonl EVIDENCE.jsonl MECH.jsonl OUT_DIR LABEL
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { gunzipSync } from 'node:zlib';

// --assemble DIR: docs/perf/CF1_POOL_SEGMENTS.md = cf1/doc_head.md + both generated table files + cf1/doc_tail.md
if (process.argv[2] === '--assemble') {
  const d = process.argv[3];
  const t = (f) => readFileSync(join(d, 'cf1', f), 'utf8');
  const NLA = String.fromCharCode(10);
  const doc = [t('doc_head.md'), '## 2. Linux (WSL2) tables', '', t('cf1_tables_linux.md'), '## 3. Windows tables', '', t('cf1_tables_windows.md'), t('doc_tail.md')].join(NLA);
  writeFileSync(join(d, 'CF1_POOL_SEGMENTS.md'), doc);
  process.exit(0);
}
const [rawPath, evPath, mechPath, outDir, label] = process.argv.slice(2);
if (!label) throw new Error('usage: cf1_report.mjs RAW EVIDENCE MECH OUT_DIR LABEL');
const read = (p) => (p.endsWith('.gz') ? gunzipSync(readFileSync(p)).toString() : readFileSync(p, 'utf8'));
const lines = (p) => read(p).split('\n').filter(Boolean).map((l) => JSON.parse(l));
const SHA = readFileSync(join(outDir, 'cf1', 'linux', 'tree.txt'), 'utf8').trim();
if (!/^[0-9a-f]{40}$/.test(SHA)) throw new Error('tree.txt is not a 40-hex commit SHA');
const raw = lines(rawPath);
const ev = lines(evPath);

for (const e of ev) {
  if (e.out.pool_req !== e.want || e.out.pool_res !== e.want) {
    throw new Error(`config evidence: want ${e.want}, req ${e.out.pool_req}, res ${e.out.pool_res}`);
  }
  if (e.out.config_conflicts !== 0) throw new Error(`config_conflicts ${e.out.config_conflicts} for pool ${e.want}`);
}

const LABELS = ['sefer', 'p0', 'p4', 'p8', 'p16', 'p32', 'p64', 'p1024', 'mi'];
const SCEN = [...new Set(raw.map((r) => r.run.scenario))];
const median = (a) => { const s = [...a].sort((x, y) => x - y); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const mib = (b) => (b / 2 ** 20).toFixed(0);
const rows = [];
for (const sc of SCEN) {
  const of = (l) => raw.filter((r) => r.run.scenario === sc && r.label === l);
  const sums = new Set(raw.filter((r) => r.run.scenario === sc).map((r) => r.run.checksum));
  if (sums.size !== 1) throw new Error(`${sc}: arms ran different op sequences (${[...sums]})`);
  const n = of('sefer').length;
  for (const l of LABELS) if (of(l).length !== n) throw new Error(`${sc} ${l}: ${of(l).length} runs, expected ${n}`);
  const base = of('sefer');
  for (const l of LABELS) {
    const v = of(l);
    const ns = v.map((r) => r.run.ns_per_op);
    // per-rep ratio against the default arm of the same rep (adjacent in time)
    const pr = v.map((r) => r.run.ns_per_op / base.find((b) => b.rep === r.rep).run.ns_per_op);
    rows.push({
      scenario: sc, arm: l, runs: v.length,
      ns_min: Math.min(...ns), ns_med: median(ns),
      ratio_vs_default: median(pr),
      ratio_vs_p4: median(v.map((r) => r.run.ns_per_op / raw.find((b) => b.run.scenario === sc && b.label === 'p4' && b.rep === r.rep).run.ns_per_op)),
      ratio_vs_p4_min: Math.min(...v.map((r) => r.run.ns_per_op / raw.find((b) => b.run.scenario === sc && b.label === 'p4' && b.rep === r.rep).run.ns_per_op)),
      ratio_vs_p4_max: Math.max(...v.map((r) => r.run.ns_per_op / raw.find((b) => b.run.scenario === sc && b.label === 'p4' && b.rep === r.rep).run.ns_per_op)),
      peak_rss: median(v.map((r) => r.run.peak_rss)),
      end_rss: median(v.map((r) => r.run.end_rss)),
    });
  }
}
const get = (sc, l) => rows.find((r) => r.scenario === sc && r.arm === l);
for (const r of rows) {
  r.rss_vs_default = r.peak_rss / get(r.scenario, 'sefer').peak_rss;
  r.ns_vs_mi = r.ns_med / get(r.scenario, 'mi').ns_med;
  r.rss_vs_mi = r.peak_rss / get(r.scenario, 'mi').peak_rss;
  r.end_vs_mi = r.end_rss / get(r.scenario, 'mi').end_rss;
  // assert the arithmetic the tables print
  if (Math.abs(r.ns_vs_mi - r.ns_med / get(r.scenario, 'mi').ns_med) > 1e-12) throw new Error('ratio mismatch');
}

const head = 'scenario,arm,runs,ns_min,ns_median,median_of_per_rep_ratio_vs_default,median_of_per_rep_ratio_vs_p4,ratio_vs_p4_min,ratio_vs_p4_max,ns_median_over_mi_median,peak_rss_median,peak_rss_over_default,peak_rss_over_mi,end_rss_median,end_rss_over_mi';
writeFileSync(join(outDir, `CF1_POOL_SEGMENTS_${label}_summary.csv`), [`# commit_sha=${SHA}`, head, ...rows.map((r) => [
  r.scenario, r.arm, r.runs, r.ns_min.toFixed(3), r.ns_med.toFixed(3), r.ratio_vs_default.toFixed(4), r.ratio_vs_p4.toFixed(4), r.ratio_vs_p4_min.toFixed(4), r.ratio_vs_p4_max.toFixed(4),
  r.ns_vs_mi.toFixed(4), r.peak_rss, r.rss_vs_default.toFixed(4), r.rss_vs_mi.toFixed(4), r.end_rss, r.end_vs_mi.toFixed(4)].join(','))].join('\n') + '\n');

const arms = LABELS;
const table = (title, cell) => {
  const out = [`**${title}**`, '', `| scenario | ${arms.join(' | ')} |`, `|---|${arms.map(() => '---:').join('|')}|`];
  for (const sc of SCEN) out.push(`| ${sc} | ${arms.map((l) => cell(get(sc, l))).join(' | ')} |`);
  return out.join('\n');
};
const md = [
  table('Median ns/op, ratio to the default arm `sefer` (median of per-rep ratios; < 1 = faster than the default pool of 4)', (r) => r.ratio_vs_default.toFixed(3)),
  '',
  table('Median ns/op, ratio to the control arm `p4` (same configuration as the default, built into its own binary; median of per-rep ratios; the spread of the `sefer`/`p4` pair is the layout noise floor)', (r) => `${r.ratio_vs_p4.toFixed(3)} (${r.ratio_vs_p4_min.toFixed(2)}–${r.ratio_vs_p4_max.toFixed(2)})`),
  '',
  table('Median ns/op, ratio to mimalloc (median ns of the arm / median ns of `mi`; < 1 = faster than mimalloc)', (r) => r.ns_vs_mi.toFixed(3)),
  '',
  table('Median peak RSS, MiB', (r) => mib(r.peak_rss)),
  '',
  table('Median end-of-run RSS, MiB (after the scenario freed everything)', (r) => mib(r.end_rss)),
  '',
  table('Median end-of-run RSS over mimalloc (< 1 = smaller than mimalloc)', (r) => r.end_vs_mi.toFixed(2)),
  '',
].join('\n');

// ---- mechanism oracle ----
const NL = String.fromCharCode(10);
const mech = lines(mechPath);
const NS = [0, 4, 8, 16, 32, 64, 1024];
const mrows = [];
for (const sc of SCEN) {
  const sums = new Set(mech.filter((m) => m.run.scenario === sc).map((m) => m.run.checksum));
  if (sums.size !== 1) throw new Error(`mech ${sc}: different op sequences`);
  for (const n of NS) {
    const v = mech.filter((m) => m.run.scenario === sc && m.want === n);
    if (!v.length) throw new Error(`mech ${sc} pool ${n}: no runs`);
    for (const m of v) {
      if (m.run.pool_req !== n || m.run.pool_res !== n || (m.run.config_conflicts ?? 0) !== 0) {
        throw new Error(`mech ${sc} pool ${n}: req ${m.run.pool_req} res ${m.run.pool_res} conflicts ${m.run.config_conflicts}`);
      }
    }
    const f = v.map((m) => m.faults);
    const rel = v.map((m) => m.run.seg_released);
    const res = v.map((m) => m.run.seg_reserved);
    // counters are exact on Linux; threads/placement can move them by a few on Windows: keep the median and the range
    mrows.push({
      scenario: sc, pool: n, runs: v.length, faults: median(f), faults_spread: Math.max(...f) / Math.min(...f),
      reserved: median(res), released: median(rel),
      counter_range: `${Math.min(...res)}-${Math.max(...res)}/${Math.min(...rel)}-${Math.max(...rel)}`,
    });
  }
}
const mget = (sc, n) => mrows.find((r) => r.scenario === sc && r.pool === n);
for (const r of mrows) {
  r.reserve_saved = mget(r.scenario, 0).reserved - r.reserved;
  r.release_saved = mget(r.scenario, 0).released - r.released;
  if (r.faults_spread > 1.01) throw new Error(`mech ${r.scenario} pool ${r.pool}: fault spread ${r.faults_spread}`);
}
writeFileSync(join(outDir, `CF1_POOL_SEGMENTS_${label}_mech_summary.csv`),
  [`# commit_sha=${SHA}`, 'scenario,pool_segments,runs,minor_faults_median,seg_reserved_median,seg_released_median,reserve_saved_vs_pool0,release_saved_vs_pool0,reserved_min-max/released_min-max',
    ...mrows.map((r) => [r.scenario, r.pool, r.runs, r.faults, r.reserved, r.released, r.reserve_saved, r.release_saved, r.counter_range].join(','))].join(NL) + NL);
const mtable = (title, cell) => {
  const out = [`**${title}**`, '', `| scenario | ${NS.map((n) => `pool ${n}`).join(' | ')} |`, `|---|${NS.map(() => '---:').join('|')}|`];
  for (const sc of SCEN) out.push(`| ${sc} | ${NS.map((n) => cell(mget(sc, n))).join(' | ')} |`);
  return out.join(NL);
};
const mmd = [
  mtable('Segments released to the OS during the run (counter `seg_released`; fewer = more kept in the pool at the end)', (r) => r.released),
  '',
  mtable('Fresh segment reservations during the run (counter `seg_reserved`; fewer than pool 0 = the pool served a reuse in-run)', (r) => r.reserved),
  '',
  mtable('Reservations saved versus pool 0 (`seg_reserved(pool 0) - seg_reserved(pool N)` = in-run pool hits)', (r) => r.reserve_saved),
  '',
  mtable('Median minor page faults of the process (3 runs per cell)', (r) => (r.faults > 0 ? r.faults : 'n/a')),
  '',
].join(NL);
writeFileSync(join(outDir, `cf1_tables_${label}.md`), md + NL + mmd);
console.log(md + NL + mmd);
