// per-op PMU deltas: (count at N ops - count at 0 ops) / (N * 1.1) [warm-up = N/10]
import fs from 'node:fs';
const L = fs.readFileSync(process.argv[2], 'utf8').trim().split('\n').map((l) => l.trim().split(' ')).filter((x) => x.length === 5);
const m = {}; const evs = [];
for (const [a, live, n, ev, v] of L) { if (!evs.includes(ev)) evs.push(ev); ((m[`${a} ${live}`] ??= {})[ev] ??= {})[n] = Number(v); }
console.log(['alloc live', ...evs.map((e) => e.replace(/:u$/, ''))].join(' | '));
for (const k in m) {
  const r = evs.map((e) => { const x = m[k][e]; const n = Number(Object.keys(x).find((y) => y !== '0')); return ((x[n] - x['0']) / (n * 1.1)).toFixed(2); });
  console.log([k, ...r].join(' | '));
}
