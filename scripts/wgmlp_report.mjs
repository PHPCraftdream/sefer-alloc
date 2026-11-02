// Derives docs/perf/wgmlp/summary.csv from raw.log and asserts the printed ratios.
import { readFileSync, writeFileSync } from 'node:fs';
const dir = new URL('../docs/perf/wgmlp/', import.meta.url);
const rows = [];
for (const l of readFileSync(new URL('raw.log', dir), 'utf8').split('\n')) {
  const m = l.match(/^mb=(\d+) work=(\d+) single=([\d.]+)ns batch16=([\d.]+)ns \(([\d.]+)x\) batch64=([\d.]+)ns \(([\d.]+)x\)/);
  if (!m) continue;
  const [mb, work, s, b16, r16, b64, r64] = m.slice(1).map(Number);
  // inputs are rounded to 0.05 ns, so allow the matching ratio error
  const tol = (b, r) => r * (0.05 / s + 0.05 / b) + 0.002;
  assert(Math.abs(b16 / s - r16) <= tol(b16, r16) && Math.abs(b64 / s - r64) <= tol(b64, r64), `ratio mismatch ${l}`);
  rows.push({ mb, work, s, b16, r16, b64, r64 });
}
function assert(c, msg) { if (!c) throw new Error(msg); }
assert(rows.length === 4, 'rows');
const big = rows.filter(r => r.mb === 87);
const worst = Math.max(...big.map(r => r.r16)), best = Math.min(...big.map(r => r.r16));
// kill criterion: batch16 > 0.6x single => do not implement
assert(best > 0.6, 'criterion would pass; revisit');
let csv = 'table_mb,work,single_ns,batch16_ns,batch16_over_single,batch64_ns,batch64_over_single\n';
for (const r of rows) csv += `${r.mb},${r.work},${r.s},${r.b16},${r.r16},${r.b64},${r.r64}\n`;
writeFileSync(new URL('summary.csv', dir), csv);
console.log(`87 MB: batch16/single in ${best.toFixed(3)}..${worst.toFixed(3)} (kill if > 0.6): NO-GO`);
