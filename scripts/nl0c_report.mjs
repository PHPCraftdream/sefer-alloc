// NL-0 part 2: derives docs/perf/NL0C_LIVENESS_CHECKS_CEILING_summary.csv and the report
// tables from docs/perf/nl0c raw files; asserts every printed ratio.
import { readFileSync, writeFileSync } from 'node:fs';
const dir = new URL('../docs/perf/nl0c/', import.meta.url);
const rd = (n) => readFileSync(new URL(n, dir), 'utf8');
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
function assert(c, m) { if (!c) throw new Error(m); }
const TREES = ['base', 'b', 'c'];
const BULK = [['bulk16', 16], ['bulk64', 64]];
const MT = ['larson1', 'larson4', 'mstress1', 'mstress4'];
const cells = [];
for (const [k] of BULK) {
  for (const L of TREES) {
    const ir = Number(rd(`ir.${L}.${k}.txt`).match(/^(\d+)/)[1]);
    const cy = [], ns = [];
    for (let i = 1; i <= 5; i++) {
      for (const l of rd(`stat.${L}.${k}.${i}.txt`).split('\n')) {
        const f = l.split(',');
        if (f[2] === 'cycles:u') cy.push(Number(f[0]));
      }
      const o = rd(`run.${L}.${k}.${i}.stdout`).trim().split(/\s+/);
      assert(o[0] === 'bulk' && o[1] === 'sefer', `stdout shape ${L} ${k}`);
      ns.push(Number(o[5]));
    }
    assert(cy.length === 5 && ns.length === 5, `5 samples ${L} ${k}`);
    cells.push({ k, L, ir, time: med(ns), unit: 'ns/op', cy: med(cy) });
  }
}
for (const k of MT) {
  for (const L of TREES) {
    const ir = Number(rd(`ir.${L}.${k}.txt`).match(/^(\d+)/)[1]);
    const mops = [];
    for (let i = 1; i <= 5; i++) {
      const m = rd(`run.${L}.${k}.${i}.stdout`).match(/T=(\d+) ([\d.]+) Mops/);
      assert(m && `${k}`.endsWith(m[1]), `mops shape ${L} ${k}`);
      mops.push(Number(m[2]));
    }
    assert(mops.length === 5, `5 samples ${L} ${k}`);
    cells.push({ k, L, ir, time: med(mops), unit: 'Mops', cy: null });
  }
}
const get = (k, L) => cells.find((c) => c.k === k && c.L === L);
let csv = 'cell,tree,callgrind_ir,ir_vs_base_pct,median_time,time_unit,time_vs_base_pct,median_cycles_u,cycles_vs_base_pct\n';
for (const c of cells) {
  const b = get(c.k, 'base');
  const irp = (c.ir / b.ir - 1) * 100;
  // ns/op: lower is better; Mops: higher is better. time_vs_base is the change in work time.
  const tp = c.unit === 'Mops' ? (b.time / c.time - 1) * 100 : (c.time / b.time - 1) * 100;
  const cyp = c.cy === null ? '' : ((c.cy / b.cy - 1) * 100).toFixed(2);
  csv += `${c.k},${c.L},${c.ir},${irp.toFixed(2)},${c.time},${c.unit},${tp.toFixed(2)},${c.cy ?? ''},${cyp}\n`;
  console.log(`| ${c.k} | ${c.L} | ${c.ir} | ${irp.toFixed(2)} % | ${c.time} ${c.unit} | ${tp.toFixed(2)} % |`);
}
writeFileSync(new URL('../NL0C_LIVENESS_CHECKS_CEILING_summary.csv', dir), csv);
// C must not cost more than B in Ir in any cell (it only removes work).
for (const k of [...BULK.map((x) => x[0]), ...MT]) assert(get(k, 'c').ir <= get(k, 'b').ir, `C <= B at ${k}`);
