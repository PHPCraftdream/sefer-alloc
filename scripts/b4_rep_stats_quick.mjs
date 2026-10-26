// process-level stats: per-process median ln(tB/tA), then median across processes + bootstrap CI over processes
import fs from 'node:fs';
const files = process.argv.slice(2);
const per = {};
for (const f of files) {
  const by = {};
  for (const l of fs.readFileSync(f, 'utf8').split('\n')) {
    if (!l || l.includes('"header"')) continue;
    const r = JSON.parse(l); if (r.a1 === undefined) continue;
    (by[r.kernel] ??= []).push(Math.log((r.b1 + r.b2) / (r.a1 + r.a2)));
  }
  for (const [k, v] of Object.entries(by)) { v.sort((x, y) => x - y); (per[k] ??= []).push(v[v.length >> 1]); }
}
const med = (a) => { const s = [...a].sort((x, y) => x - y); const n = s.length; return n % 2 ? s[n >> 1] : (s[n / 2 - 1] + s[n / 2]) / 2; };
let seed = 12345; const rnd = () => (seed = (seed * 1103515245 + 12345) % 2147483648) / 2147483648;
for (const [k, v] of Object.entries(per)) {
  const bs = []; for (let b = 0; b < 2000; b++) { const s = v.map(() => v[Math.floor(rnd() * v.length)]); bs.push(med(s)); }
  bs.sort((x, y) => x - y);
  const m = med(v), lo = bs[50], hi = bs[1949];
  console.log(`${k} R=${v.length} median=${(m*100).toFixed(2)}% CI=[${(lo*100).toFixed(2)}, ${(hi*100).toFixed(2)}] half=${((hi-lo)/2*100).toFixed(2)}% ratio=${Math.exp(m).toFixed(4)} min=${(Math.min(...v)*100).toFixed(2)} max=${(Math.max(...v)*100).toFixed(2)}`);
}
