// Derives the cross-cell summary CSV from the per-cell reducer reports (no hand transcription).
// Usage: node j13-summary.mjs <evidence-dir> <out.csv>
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
const [dir, out] = process.argv.slice(2);
const CELLS = ['cycle-1MiB', 'cycle-8MiB-minus-1', 'cycle-8MiB', 'cycle-16MiB', 'cycle-128MiB', 'cycle-1024MiB', 'grow-1GiB', 'many-128x16MiB', 'edge-128MiB', 'cache-16MiB'];
const rows = [['cell', 'regime', 'always_over_off', 'ci95_lo', 'ci95_hi', 'aa_ratio', 'aa_lo', 'aa_hi', 'aa_interval_contains_1', 'off_median_ns', 'always_median_ns']];
const num = (s) => Number(s);
for (const cell of CELLS) {
  const md = readFileSync(join(dir, cell, 'report.md'), 'utf8').split('\n');
  const line = md.find(l => l.startsWith(`| ${cell} |`) && l.includes('| selected ('));
  if (!line) throw new Error(`${cell}: selected-metric row missing`);
  const cols = line.split('|').map(s => s.trim()).filter(Boolean);
  const regime = cols[1];
  const ab = /([\d.]+) \(([\d.]+)[–-]([\d.]+)\)/.exec(cols[4]);
  const aa = /([\d.]+) \(([\d.]+)[–-]([\d.]+)\)/.exec(cols[5]);
  const med = /([\d.]+)\s*(?:→|->)\s*([\d.]+)/.exec(cols[6] ?? '');
  if (!ab || !aa) throw new Error(`${cell}: cannot parse ratio/A-A from: ${cols[4]} | ${cols[5]}`);
  const contains = num(aa[2]) <= 1 && num(aa[3]) >= 1;
  if (!!/contains 1/.test(cols[5]) !== contains) throw new Error(`${cell}: A/A containment text disagrees with interval`);
  rows.push([cell, regime, ab[1], ab[2], ab[3], aa[1], aa[2], aa[3], contains, med?.[1] ?? '', med?.[2] ?? '']);
}
writeFileSync(out, rows.map(r => r.join(',')).join('\n') + '\n');
console.log(rows.map(r => r.join(' | ')).join('\n'));
