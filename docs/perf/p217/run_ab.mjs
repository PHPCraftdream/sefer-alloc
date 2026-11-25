// #217: paired base/x/mi runner on g1_wild_judge (sefer arm from two builds, mi arm from base build).
// One process per (block, scenario, arm, repeat); scenario order shuffled per block, arm order rotated per block.
import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';
const arg = (k, d) => { const i = process.argv.indexOf(k); return i >= 0 ? process.argv[i + 1] : d; };
const scen = arg('--scenarios').split(',');
const blocks = +arg('--blocks', '12'), repeats = +arg('--repeats', '3');
const exe = { base: arg('--base'), x: arg('--x') };
const arms = [['base', exe.base, 'sefer'], ['x', exe.x, 'sefer'], ['mi', exe.base, 'mi']];
let s = 0x9e3779b97f4a7c15n; const M = (1n << 64n) - 1n;
const rnd = (n) => { s ^= (s << 13n) & M; s ^= s >> 7n; s ^= (s << 17n) & M; return Number(s % BigInt(n)); };
const out = arg('--out');
for (let b = 0; b < blocks; b++) {
  const order = [...scen];
  for (let i = order.length - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
  const k = b % arms.length, rot = [...arms.slice(k), ...arms.slice(0, k)];
  for (const sc of order) for (const [label, path, arm] of rot) for (let rep = 0; rep < repeats; rep++) {
    const r = spawnSync(path, [sc, arm, '1', '1'], { encoding: 'utf8' });
    if (r.status !== 0) throw new Error(`${label} ${sc}: exit ${r.status} ${r.stderr}`);
    appendFileSync(out, JSON.stringify({ block: b, rep, arm: label, r: JSON.parse(r.stdout.trim()) }) + '\n');
  }
}
