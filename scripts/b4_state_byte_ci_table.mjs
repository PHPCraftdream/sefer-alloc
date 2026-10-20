#!/usr/bin/env node
// B4 state byte — CI receipts table (iai A/B + mt-bench ratios) for
// docs/perf/B4_STATE_BYTE.md §12. Reads only committed raw files; no cargo.
// Usage: node scripts/b4_state_byte_ci_table.mjs [--csv]
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const IAI = 'docs/perf/_raw_b4_state_byte_iai_ab.csv';
const MT = { base: 'docs/perf/_raw_b4_state_byte_mt_base.log', pr87: 'docs/perf/_raw_b4_state_byte_mt_pr87.log' };

// iai A/B: A = merge-base with main, B = PR head; Ir per cell.
const lines = readFileSync(IAI, 'utf8').trim().split(/\r?\n/);
const head = lines[0].split(',');
const ix = (n) => head.indexOf(n);
const cells = lines.slice(1).map((l) => l.split(',')).map((r) => ({
  cell: r[ix('cell')], a: Number(r[ix('A_Ir')]), b: Number(r[ix('B_Ir')]),
  sha_a: r[ix('sha_a')], sha_b: r[ix('sha_b')],
}));
for (const c of cells) c.pct = c.a === 0 ? 0 : ((c.b - c.a) / c.a) * 100;
const shaA = cells[0].sha_a.slice(0, 8);
const shaB = cells[0].sha_b.slice(0, 8);
const mimalloc = cells.filter((c) => c.cell.startsWith('mimalloc_'));
assert.ok(mimalloc.length > 0, 'mimalloc control cells missing');
const controlsExact = mimalloc.every((c) => c.a === c.b);
const sefer = cells.filter((c) => !c.cell.startsWith('mimalloc_') && c.a > 100);
const better = sefer.filter((c) => c.b < c.a).length;
const worse = sefer.filter((c) => c.b > c.a).length;
const same = sefer.length - better - worse;
assert.equal(better + worse + same, sefer.length);
const sorted = [...sefer].sort((x, y) => x.pct - y.pct);
const fmt = (c) => `| \`${c.cell}\` | ${c.a} | ${c.b} | ${c.b - c.a} | ${c.pct.toFixed(2)} % |`;

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
    if (m && wl && os) out.push({ os, run, wl, t: Number(m[1]), sefer: Number(m[2]), mi: Number(m[3]) });
  }
  return out;
}
const mt = Object.fromEntries(Object.entries(MT).map(([k, p]) => [k, parseMt(p)]));
assert.equal(mt.base.length, 24, 'base mt rows');
assert.equal(mt.pr87.length, 24, 'pr87 mt rows');
const key = (r) => `${r.os}|${r.wl}|${r.t}`;
const ratios = (rows) => {
  const m = new Map();
  for (const r of rows) {
    const k = key(r);
    if (!m.has(k)) m.set(k, []);
    m.get(k).push(r.sefer / r.mi);
  }
  return m;
};
const rb = ratios(mt.base);
const rp = ratios(mt.pr87);
const mtRows = [...rb.keys()].map((k) => {
  const [os, wl, t] = k.split('|');
  const b = rb.get(k); const p = rp.get(k);
  assert.equal(b.length, 2); assert.equal(p.length, 2);
  return { os, wl, t, b, p };
});

if (process.argv.includes('--csv')) {
  console.log('kind,key,a,b,delta,delta_pct');
  for (const c of sefer) console.log(`iai,${c.cell},${c.a},${c.b},${c.b - c.a},${c.pct.toFixed(4)}`);
  for (const r of mtRows) console.log(`mt_ratio,${r.os}/${r.wl}/T${r.t},${r.b.map((x) => x.toFixed(3)).join('|')},${r.p.map((x) => x.toFixed(3)).join('|')},,`);
  process.exit(0);
}

console.log(`iai A/B (r20 gate-run, A = \`${shaA}\`, B = \`${shaB}\`, features \`production bench-internals internals\`):`);
console.log(`- cells with A > 100 Ir: ${sefer.length}; better ${better}, worse ${worse}, equal ${same}`);
console.log(`- mimalloc control cells: ${mimalloc.length}, all Δ = 0: ${controlsExact ? 'yes' : 'NO — run invalid'}`);
console.log('');
console.log('| cell (largest gains) | A Ir | B Ir | Δ | Δ % |');
console.log('|---|---:|---:|---:|---:|');
for (const c of sorted.slice(0, 8)) console.log(fmt(c));
console.log('');
console.log('| cell (all regressions) | A Ir | B Ir | Δ | Δ % |');
console.log('|---|---:|---:|---:|---:|');
for (const c of sorted.filter((x) => x.b > x.a).reverse()) console.log(fmt(c));
console.log('');
console.log('mt-bench, production, SeferAlloc / mimalloc ops-per-sec ratio (run1 / run2, same job):');
console.log('');
console.log('| OS | workload | T | base `2730d02a` | PR #87 |');
console.log('|---|---|---:|---|---|');
for (const r of mtRows) console.log(`| ${r.os} | ${r.wl} | ${r.t} | ${r.b.map((x) => x.toFixed(2)).join(' / ')} | ${r.p.map((x) => x.toFixed(2)).join(' / ')} |`);
