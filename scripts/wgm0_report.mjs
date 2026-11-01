#!/usr/bin/env node
// WG-M0 report generator: every number in docs/perf/WG_M0_RELEASE_ATTRIBUTION.md
// comes from docs/perf/wgm0/raw_*.jsonl through this script. Writes
// docs/perf/WG_M0_RELEASE_ATTRIBUTION_summary.csv and docs/perf/wgm0/tables.md.
// Statistics are named where they are computed: "min" is the minimum of the
// per-run total timed region (ns_per_op * ops) over the runs of one cell;
// "paired median" is the median over (block, order) pairs of arm/mi where both
// arms ran in the same block and order.
import fs from 'node:fs';
import assert from 'node:assert/strict';

const dir = 'docs/perf/wgm0/';
const LABELS = [['base', 'main 8462e42c'], ['ao1', 'main 8462e42c + AO-1 v2b carve']];
const SC = ['grow64', 'grow1k', 'regrow', 'growmix'], ARMS = ['sefer', 'sefer_pool', 'mi'];
const tot = (r) => r.ns_per_op * r.ops / 1e6;
const min = (a) => Math.min(...a);
const med = (a) => { const s = [...a].sort((x, y) => x - y), n = s.length; return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2; };
const mib = (b) => Math.round(b / 1048576);
const ratio = (num, den, d = 3) => { const v = num / den; const p = v.toFixed(d); assert.ok(Math.abs(Number(p) - v) <= 0.5 * 10 ** -d + 1e-12); return p; };
const csv = ['label,scenario,arm,n,min_total_ms,med_total_ms,seg_reserved_med,seg_released_min,seg_released_max,config_conflicts_max,peak_rss_med_mib,end_rss_med_mib'];
const md = [];
const oracleFails = [];

for (const [label, desc] of LABELS) {
  const rows = fs.readFileSync(`${dir}raw_${label}.jsonl`, 'utf8').trim().split('\n').map((l) => JSON.parse(l)).map((o) => ({ ...o.r, blk: o.blk, pos: o.pos }));
  md.push(`### ${label}: ${desc}`, '',
    '| scenario | sefer min ms | sefer_pool min ms | mi min ms | sefer/mi | sefer_pool/mi | (sefer − sefer_pool) / (sefer − mi) | seg_released sefer | seg_released sefer_pool | end RSS MiB sefer / pool / mi |',
    '|---|---|---|---|---|---|---|---|---|---|');
  const pairs = [];
  for (const s of SC) {
    const by = Object.fromEntries(ARMS.map((a) => [a, rows.filter((r) => r.scenario === s && r.arm === a)]));
    for (const a of ARMS) assert.equal(by[a].length, 12, `${label} ${s} ${a}: n`);
    for (const a of ARMS) {
      const v = by[a], g = (k) => v.map((r) => r[k] ?? 0);
      csv.push([label, s, a, v.length, min(v.map(tot)).toFixed(1), med(v.map(tot)).toFixed(1), med(g('seg_reserved')), min(g('seg_released')), Math.max(...g('seg_released')), Math.max(...g('config_conflicts')), mib(med(g('peak_rss'))), mib(med(g('end_rss')))].join(','));
    }
    const m = Object.fromEntries(ARMS.map((a) => [a, min(by[a].map(tot))]));
    const rel = (a) => [min(by[a].map((r) => r.seg_released)), Math.max(...by[a].map((r) => r.seg_released))];
    // path-activation oracle: the pool arm never releases; no run of any arm has a config conflict
    if (!(rel('sefer_pool')[1] === 0)) oracleFails.push(`${label} ${s}: sefer_pool released`);
    if (ARMS.some((a) => by[a].some((r) => r.config_conflicts !== 0))) oracleFails.push(`${label} ${s}: config conflict`);
    const share = m.sefer > m.mi ? ratio(m.sefer - m.sefer_pool, m.sefer - m.mi) : 'n/a (sefer <= mi)';
    const end = ARMS.map((a) => mib(med(by[a].map((r) => r.end_rss)))).join(' / ');
    md.push(`| ${s} | ${m.sefer.toFixed(0)} | ${m.sefer_pool.toFixed(0)} | ${m.mi.toFixed(0)} | ${ratio(m.sefer, m.mi)} | ${ratio(m.sefer_pool, m.mi)} | ${share} | ${rel('sefer').join('..')} | ${rel('sefer_pool').join('..')} | ${end} |`);
    const key = (r) => `${r.blk}|${r.pos}`, mi = new Map(by.mi.map((r) => [key(r), tot(r)]));
    const pm = ['sefer', 'sefer_pool'].map((a) => med(by[a].map((r) => tot(r) / mi.get(key(r)))));
    const pshare = pm[0] > 1 ? ratio(pm[0] - pm[1], pm[0] - 1) : 'n/a';
    pairs.push([s, [ratio(pm[0], 1), ratio(pm[1], 1), pshare]]);
  }
  md.push('', `Paired median of arm/mi (same block and order, n = 12 pairs per cell), ${label}:`, '', '| scenario | sefer/mi | sefer_pool/mi | (sefer/mi − sefer_pool/mi) / (sefer/mi − 1) |', '|---|---|---|---|');
  for (const [s, v] of pairs) md.push(`| ${s} | ${v[0]} | ${v[1]} | ${v[2]} |`);
  md.push('');
}
assert.deepEqual(oracleFails, [], 'path-activation oracle failed');
md.push(`Path-activation oracle: in every run of the \`sefer_pool\` arm seg_released == 0, and config_conflicts == 0 in all ${LABELS.length * SC.length * ARMS.length * 12} runs.`, '');
fs.writeFileSync('docs/perf/WG_M0_RELEASE_ATTRIBUTION_summary.csv', csv.join('\n') + '\n');
fs.writeFileSync(`${dir}tables.md`, md.join('\n'));
console.log(md.join('\n'));
