// Round-6 iai A/B gate table: rows derived from a summary CSV; asserts B - A = delta.
// Usage: node scripts/r6_gate_table.mjs <summary.csv> <cell>...
import fs from 'node:fs';
const [csv, ...cells] = process.argv.slice(2);
const rows = fs.readFileSync(csv, 'utf8').trim().split('\n').slice(1).map(l => l.split(','));
console.log('| cell | A Ir | B Ir | delta Ir | delta / A |');
console.log('|---|---:|---:|---:|---:|');
for (const c of cells) {
  const r = rows.find(x => x[0] === c);
  if (!r) throw new Error('missing cell ' + c);
  const a = +r[1], b = +r[2], d = +r[3];
  if (b - a !== d) throw new Error('delta mismatch ' + c);
  const s = d > 0 ? '+' : '';
  console.log(`| \`${c}\` | ${a} | ${b} | ${s}${d} | ${s}${d} / ${a} = ${(100 * d / a).toFixed(2)} % |`);
}
const pos = rows.filter(r => +r[3] > 0).length, neg = rows.filter(r => +r[3] < 0).length;
console.log(`\ncells: ${rows.length}; delta > 0: ${pos}; delta < 0: ${neg}; delta = 0: ${rows.length - pos - neg}`);
