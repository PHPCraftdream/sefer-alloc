// Derives the summary CSV from the bench-table markdown tables in the raw log (no hand transcription).
// Usage: node bench-table-to-csv.mjs <raw.log> <out.csv> <source_sha> <wnice_sha256> <limits>
import { readFileSync, writeFileSync } from 'node:fs';
const [log, out, sha, wnice, limits] = process.argv.slice(2);
const lines = readFileSync(log, 'utf8').split(/\r?\n/);
const SIZED = ['global_alloc', 'global_alloc_churn', 'global_alloc_churn_write', 'global_alloc_churn_with_teardown'];
const rows = [['group', 'size', 'sefer', 'mimalloc', 'system', 'sefer_vs_mimalloc_x', 'unit', 'source_sha', 'wnice_exe_sha256', 'limits']];
let sizedIdx = -1;
let single = null; // { id, unit } for the single-row tables
let group = null;
let unit = '';
const ratioOf = (text) => {
  const x = /([\d.]+)×\s*(faster|slower)/.exec(text.replace(/\*/g, ''));
  return x ? (x[2] === 'faster' ? Number(x[1]) : 1 / Number(x[1])) : null;
};
const push = (g, size, s, m, y, ratio, u) => {
  if (ratio !== null && m !== '') {
    const calc = Number(m) / Number(s);
    if (Math.abs(calc - ratio) / calc > 0.08) throw new Error(`ratio mismatch ${g} ${size}: printed ${ratio.toFixed(3)} vs ${calc.toFixed(3)}`);
  }
  rows.push([g, size, s, m, y, ratio === null ? '' : ratio.toFixed(4), u, sha, wnice, limits]);
};
for (const raw of lines) {
  const h = /^###\s+(.*)$/.exec(raw);
  if (h) {
    const title = h[1];
    single = null; group = null;
    if (/^(Warm|Churn)/.test(title)) { sizedIdx += 1; group = SIZED[sizedIdx]; unit = 'ns/pair'; }
    else if (/^manual_realloc_sim/.test(title)) single = { id: 'manual_realloc_sim', unit: 'ns/closure' };
    else if (/^`segment_decommit_cycle`/.test(title)) single = { id: 'segment_decommit_cycle', unit: 'ns/batch' };
    else if (/^`working_set_cycle`/.test(title)) group = 'working_set_cycle';
    continue;
  }
  const sized = /^\|\s*(\d+B)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*(.+?)\s*\|\s*$/.exec(raw);
  if (sized && group && group !== 'working_set_cycle') { push(group, sized[1], sized[2], sized[3], sized[4], ratioOf(sized[5]), unit); continue; }
  const ws = /^\|\s*(\d+B)\s*\|\s*([\d.]+)\s*\|\s*$/.exec(raw);
  if (ws && group === 'working_set_cycle') { push(group, ws[1], ws[2], '', '', null, 'ns/batch'); continue; }
  const one = /^\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*([\d.]+)\s*\|\s*(.+?)\s*\|\s*$/.exec(raw);
  if (one && single) { push(single.id, '-', one[1], one[2], one[3], ratioOf(one[4]), single.unit); single = null; }
}
writeFileSync(out, rows.map(r => r.join(',')).join('\n') + '\n');
console.log(`wrote ${rows.length - 1} rows to ${out}`);
