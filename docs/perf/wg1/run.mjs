// WG-1 step 1: Windows sefer/mi map on current main (judge arms sefer and mi in one binary).
// usage: node wg1_map.mjs EXE OUT BLOCKS REPEATS
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
const [exe, out, blocks, repeats] = [process.argv[2], process.argv[3], Number(process.argv[4]), Number(process.argv[5])];
const SCEN = ['grow64', 'grow1k', 'regrow', 'growmix', 'wild', 'phase', 'large'];
const ARMS = ['sefer', 'mi'];
writeFileSync(out, '');
let s = 0x9e3779b97f4a7c15n;
const rnd = (n) => { s ^= s << 13n; s &= (1n << 64n) - 1n; s ^= s >> 7n; s ^= s << 17n; s &= (1n << 64n) - 1n; return Number(s % BigInt(n)); };
for (let b = 0; b < blocks; b++) {
  const order = [...SCEN];
  for (let i = order.length - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
  for (const sc of order) {
    const arms = b % 2 ? [...ARMS].reverse() : ARMS;
    for (const arm of arms) for (let k = 0; k < repeats; k++) {
      const p = spawnSync(exe, [sc, arm, '1', '1'], { encoding: 'utf8' });
      if (p.status !== 0) throw new Error(`${sc} ${arm}: ${p.status} ${p.stderr}`);
      appendFileSync(out, JSON.stringify({ block: b, arm, r: JSON.parse(p.stdout.trim().split('\n').pop()) }) + '\n');
    }
  }
}
console.log('done');
