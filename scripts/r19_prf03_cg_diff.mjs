// Function-level exclusive-Ir diff of two retained Callgrind profiles (diagnostic, read-only).
// Usage: node scripts/r19_prf03_cg_diff.mjs <profileA> <profileB> [topN]
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

export function selfCosts(text) {
  const names = new Map(), cost = new Map();
  let cur = null, skip = false;
  const resolveName = s => {
    const m = /^\((\d+)\)(?: (.*))?$/.exec(s.trim());
    if (!m) return s.trim();
    if (m[2] !== undefined) names.set(m[1], m[2]);
    return names.get(m[1]) ?? `#${m[1]}`;
  };
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith('fn=')) { cur = resolveName(line.slice(3)); skip = false; continue; }
    if (line.startsWith('cfn=')) { resolveName(line.slice(4)); continue; }
    if (/^(cfi|cfl|cob|ob|fl|fi|fe)=/.test(line)) continue;
    if (line.startsWith('calls=')) { skip = true; continue; } // next cost line is the inclusive callee cost
    if (/^[+\-*]?\d* \d+/.test(line)) {
      if (skip) { skip = false; continue; }
      const c = Number(line.trim().split(/\s+/)[1]);
      if (cur && Number.isFinite(c)) cost.set(cur, (cost.get(cur) ?? 0) + c);
    }
  }
  return cost;
}

export function diffProfiles(textA, textB, topN = 25) {
  const A = selfCosts(textA), B = selfCosts(textB);
  const sum = m => [...m.values()].reduce((x, y) => x + y, 0);
  const keys = new Set([...A.keys(), ...B.keys()]);
  const rows = [...keys].map(k => ({ fn: k, a: A.get(k) ?? 0, b: B.get(k) ?? 0 })).map(r => ({ ...r, d: r.b - r.a })).filter(r => r.d !== 0)
    .sort((p, q) => Math.abs(q.d) - Math.abs(p.d));
  const totalA = sum(A), totalB = sum(B);
  const delta = rows.reduce((s, r) => s + r.d, 0);
  if (delta !== totalB - totalA) throw Error('exclusive-cost arithmetic mismatch');
  return { totalA, totalB, delta: totalB - totalA, rows: rows.slice(0, topN) };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [a, b, top = '25'] = process.argv.slice(2);
  const r = diffProfiles(readFileSync(a, 'utf8'), readFileSync(b, 'utf8'), Number(top));
  console.log(`exclusive totals A=${r.totalA} B=${r.totalB} delta=${r.delta}`);
  for (const x of r.rows) console.log(`${String(x.d).padStart(7)} ${String(x.a).padStart(8)} ${String(x.b).padStart(8)} ${x.fn.slice(0, 150)}`);
}
