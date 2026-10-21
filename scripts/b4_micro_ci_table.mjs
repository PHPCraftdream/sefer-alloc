#!/usr/bin/env node
// B4-M micro-pack — CI receipts (iai A/B + mt-bench ratios) for
// docs/perf/B4_MICRO_PACK.md §12. Reads only committed raw files; no cargo.
// Usage: node scripts/b4_micro_ci_table.mjs [--csv]
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const IAI = 'docs/perf/_raw_b4_micro_iai_ab.csv'; // final head, counters trimmed
const IAI_V1 = 'docs/perf/_raw_b4_micro_iai_ab_v1.csv'; // first head, hot-path counters present
const MT = {
  pr87: 'docs/perf/_raw_b4_state_byte_mt_pr87.log',
  v1a: 'docs/perf/_raw_b4_micro_mt_v1a.log',
  v1b: 'docs/perf/_raw_b4_micro_mt_v1b.log',
  v2: 'docs/perf/_raw_b4_micro_mt_v2.log',
};

function readIai(path) {
  const lines = readFileSync(path, 'utf8').trim().split(/\r?\n/);
  const head = lines[0].split(',');
  const ix = (n) => head.indexOf(n);
  return lines.slice(1).map((l) => l.split(',')).map((r) => ({
    cell: r[ix('cell')], a: Number(r[ix('A_Ir')]), b: Number(r[ix('B_Ir')]),
    shaA: r[ix('sha_a')], shaB: r[ix('sha_b')], features: r[ix('features')],
  }));
}
const cells = readIai(IAI);
const v1 = new Map(readIai(IAI_V1).map((c) => [c.cell, c]));
assert.equal(cells.length, v1.size, 'v1/v2 cell sets differ');
for (const c of cells) {
  assert.ok(v1.has(c.cell), `cell ${c.cell} missing in v1`);
  c.pct = c.a === 0 ? 0 : ((c.b - c.a) / c.a) * 100;
}
const mimalloc = cells.filter((c) => c.cell.startsWith('mimalloc_'));
assert.ok(mimalloc.length > 0, 'mimalloc control cells missing');
assert.ok(mimalloc.every((c) => c.a === c.b), 'mimalloc control moved: run invalid');
const sefer = cells.filter((c) => !c.cell.startsWith('mimalloc_') && c.a > 100);
const better = sefer.filter((c) => c.b < c.a);
const worse = sefer.filter((c) => c.b > c.a);
const same = sefer.length - better.length - worse.length;
assert.equal(better.length + worse.length + same, sefer.length);
const sorted = [...sefer].sort((x, y) => x.pct - y.pct);
const maxWorse = worse.reduce((m, c) => Math.max(m, c.pct), 0);
const fmt = (c) => `| \`${c.cell}\` | ${c.a} | ${c.b} | ${c.b - c.a} | ${c.pct.toFixed(2)} % |`;
// v1 -> v2 on the B side: only the bench-internals observer counters changed.
const v1v2 = sefer.map((c) => ({ cell: c.cell, b1: v1.get(c.cell).b, b2: c.b }))
  .filter((r) => r.b1 !== r.b2).sort((x, y) => (y.b1 - y.b2) - (x.b1 - x.b2));
// The two A sides (1e20e9aa, 4a2071ed) differ only in docs: every A cell must match.
for (const c of cells) assert.equal(v1.get(c.cell).a, c.a, `A side moved for ${c.cell}`);

// mt-bench: SeferAlloc / mimalloc ops/sec ratio per OS, workload, T, run.
function parseMt(path) {
  const out = [];
  let os = null; let run = null; let wl = null;
  for (const l of readFileSync(path, 'utf8').split(/\r?\n/)) {
    let m = l.match(/^=== (\S+) production (malloc_macro_run\d)\.log/);
    if (m) { [, os, run] = m; wl = null; continue; }
    m = l.match(/^--- workload: (\w+)\s+\(mode: unpinned\)/);
    if (m) { wl = m[1]; continue; }
    if (/^--- workload:/.test(l)) { wl = null; continue; }
    m = l.match(/^\s+(\d+)\s+([\d.]+) M\s+([\d.]+) M\s+([\d.]+) M/);
    if (m && wl && os) out.push({ os, run, wl, t: Number(m[1]), r: Number(m[2]) / Number(m[3]) });
  }
  return out;
}
const mt = Object.fromEntries(Object.entries(MT).map(([k, p]) => [k, parseMt(p)]));
for (const [k, rows] of Object.entries(mt)) assert.equal(rows.length, 24, `${k}: mt rows`);
const key = (r) => `${r.os}|${r.wl}|${r.t}`;
const group = (rows) => {
  const m = new Map();
  for (const r of rows) {
    if (!m.has(key(r))) m.set(key(r), []);
    m.get(key(r)).push(r.r);
  }
  return m;
};
const g = Object.fromEntries(Object.entries(mt).map(([k, rows]) => [k, group(rows)]));
const mtRows = [...g.pr87.keys()].map((k) => {
  const [os, wl, t] = k.split('|');
  const pr89 = [...g.v1a.get(k), ...g.v1b.get(k), ...g.v2.get(k)];
  assert.equal(pr89.length, 6, `${k}: pr89 runs`);
  return { os, wl, t, pr87: g.pr87.get(k), pr89, lo: Math.min(...pr89), hi: Math.max(...pr89) };
});
const f2 = (x) => x.toFixed(2);

if (process.argv.includes('--csv')) {
  console.log('kind,key,a,b,delta,delta_pct,sha_a,sha_b,features');
  for (const c of sefer) console.log(`iai,${c.cell},${c.a},${c.b},${c.b - c.a},${c.pct.toFixed(4)},${c.shaA},${c.shaB},${c.features}`);
  for (const r of mtRows) console.log(`mt_ratio,${r.os}/${r.wl}/T${r.t},${r.pr87.map(f2).join('|')},${r.pr89.map(f2).join('|')},,,,,production`);
  process.exit(0);
}

const shaA = cells[0].shaA.slice(0, 8);
const shaB = cells[0].shaB.slice(0, 8);
console.log(`iai A/B (r20 gate-run, A = \`${shaA}\`, B = \`${shaB}\`, features \`${cells[0].features}\`):`);
console.log(`- cells with A > 100 Ir: ${sefer.length}; better ${better.length}, worse ${worse.length}, equal ${same}; largest regression ${maxWorse.toFixed(2)} %`);
console.log(`- mimalloc control cells: ${mimalloc.length}, all Δ = 0: yes`);
console.log('');
console.log('| cell (largest gains) | A Ir | B Ir | Δ | Δ % |');
console.log('|---|---:|---:|---:|---:|');
for (const c of sorted.slice(0, 10)) console.log(fmt(c));
console.log('');
console.log('| cell (all regressions) | A Ir | B Ir | Δ | Δ % |');
console.log('|---|---:|---:|---:|---:|');
for (const c of [...worse].sort((x, y) => y.pct - x.pct)) console.log(fmt(c));
console.log('');
console.log(`B side, first head (hot-path observer counters) → final head: ${v1v2.length} cells changed; largest:`);
console.log('');
console.log('| cell | B v1 | B v2 | v1 − v2 |');
console.log('|---|---:|---:|---:|');
for (const r of v1v2.slice(0, 8)) console.log(`| \`${r.cell}\` | ${r.b1} | ${r.b2} | ${r.b1 - r.b2} |`);
console.log('');
console.log('mt-bench, production (identical production code in all three PR #89 runs), SeferAlloc / mimalloc ops-per-sec ratio:');
console.log('');
console.log('| OS | workload | T | PR #87 head (run1 / run2) | PR #89, 6 runs: min … max |');
console.log('|---|---|---:|---|---|');
for (const r of mtRows) console.log(`| ${r.os} | ${r.wl} | ${r.t} | ${r.pr87.map(f2).join(' / ')} | ${f2(r.lo)} … ${f2(r.hi)} |`);
