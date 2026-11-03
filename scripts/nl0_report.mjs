// Derives docs/perf/NL0_LIVENESS_CEILING_summary.csv and prints the report tables
// from docs/perf/nl0 raw files; asserts every printed ratio.
import { readFileSync, writeFileSync } from 'node:fs';
const dir = new URL('../docs/perf/nl0/', import.meta.url);
const rd = (n) => readFileSync(new URL(n, dir), 'utf8');
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
const LABELS = { base: 'base (main 908f2aba)', a: 'A (no free compare)', b: 'B (A + no hot state writes)' };
const rows = [];
for (const sz of [16, 64]) {
  for (const L of ['base', 'a', 'b']) {
    const ir = Number(rd(`ir.${L}.${sz}.txt`).match(/^(\d+)/)[1]);
    const ns = [], cy = [], ins = [];
    for (let i = 1; i <= 5; i++) {
      const out = rd(`run.${L}.${sz}.${i}.stdout`).trim().split(/\s+/);
      assert(out[0] === 'bulk' && out[1] === 'sefer' && Number(out[3]) === sz, `stdout shape ${L} ${sz} ${i}`);
      ns.push(Number(out[5]));
      for (const l of rd(`stat.${L}.${sz}.${i}.txt`).split('\n')) {
        const f = l.split(',');
        if (f[2] === 'cycles:u') cy.push(Number(f[0]));
        if (f[2] === 'instructions:u') ins.push(Number(f[0]));
      }
    }
    assert(cy.length === 5 && ins.length === 5 && ns.length === 5, `5 samples ${L} ${sz}`);
    rows.push({ sz, L, ir, ns: med(ns), cy: med(cy), ins: med(ins) });
  }
}
function assert(c, m) { if (!c) throw new Error(m); }
const get = (sz, L) => rows.find((r) => r.sz === sz && r.L === L);
const pct = (x, base) => ((x / base - 1) * 100).toFixed(2);
let csv = 'size_b,tree,callgrind_ir,ir_vs_base_pct,median_ns_per_op,ns_vs_base_pct,median_cycles_u,cycles_vs_base_pct,median_instructions_u\n';
for (const r of rows) {
  const b = get(r.sz, 'base');
  const irp = (r.ir / b.ir - 1) * 100, nsp = (r.ns / b.ns - 1) * 100, cyp = (r.cy / b.cy - 1) * 100;
  assert(Math.abs(Number(pct(r.ir, b.ir)) - irp) < 0.006, 'ir pct');
  csv += `${r.sz},${r.L},${r.ir},${irp.toFixed(2)},${r.ns},${nsp.toFixed(2)},${r.cy},${cyp.toFixed(2)},${r.ins}\n`;
  console.log(`| ${r.sz} B | ${LABELS[r.L]} | ${r.ir} | ${irp.toFixed(2)} % | ${r.ns} | ${nsp.toFixed(2)} % | ${cyp.toFixed(2)} % |`);
}
writeFileSync(new URL('../NL0_LIVENESS_CEILING_summary.csv', dir), csv);
// kill/priority rule used in the report: B must lower Ir by at least 5 % at both sizes.
for (const sz of [16, 64]) assert(get(sz, 'b').ir / get(sz, 'base').ir < 0.95, `B ceiling >= 5 % at ${sz}`);
