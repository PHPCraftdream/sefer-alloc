// J11: the big-block judge campaign. Per block the cells are shuffled (xorshift, fixed seed); within a cell the two arms alternate their
// order by block (ABBA by block) and run `repeats` times each. One process per run. On Linux the judge is started through `wsl.exe -e`
// (the timings are taken inside the judge, so the launcher is outside every sample).
// usage: node run.mjs <win|linux> EXE OUT BLOCKS REPEATS
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
const [plat, exe, out, blocks, repeats] = [process.argv[2], process.argv[3], process.argv[4], Number(process.argv[5]), Number(process.argv[6])];
// scenario, size in MiB, reps inside one process, k (`many`)
export const CELLS = [
  ['cycle', 1, 20, 1], ['cycle', 16, 10, 1], ['cycle', 128, 6, 1], ['cycle', 1024, 3, 1], ['cycle', 4096, 3, 1],
  ['edge', 16, 10, 1], ['edge', 128, 6, 1], ['edge', 1024, 3, 1], ['edge', 4096, 3, 1],
  ['grow', 128, 4, 1], ['grow', 1024, 3, 1], ['grow', 2048, 3, 1],
  ['many', 16, 3, 128], ['many', 128, 3, 16], ['many', 1024, 2, 2],
];
const ARMS = ['sefer', 'mi'];
writeFileSync(out, '');
let s = 0x9e3779b97f4a7c15n;
const rnd = (n) => { s ^= s << 13n; s &= (1n << 64n) - 1n; s ^= s >> 7n; s ^= s << 17n; s &= (1n << 64n) - 1n; return Number(s % BigInt(n)); };
for (let b = 0; b < blocks; b++) {
  const order = [...CELLS];
  for (let i = order.length - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
  for (const [sc, size, reps, k] of order) {
    const arms = b % 2 ? [...ARMS].reverse() : ARMS;
    for (const arm of arms) for (let r = 0; r < repeats; r++) {
      const args = [sc, arm, String(size), String(reps), ...(sc === 'many' ? [String(k)] : [])];
      const p = plat === 'linux'
        ? spawnSync('wsl.exe', ['-e', exe, ...args], { encoding: 'utf8', env: { ...process.env, MSYS2_ARG_CONV_EXCL: '*' } })
        : spawnSync(exe, args, { encoding: 'utf8' });
      if (p.status !== 0) throw new Error(`${args.join(' ')}: ${p.status} ${p.stderr}`);
      appendFileSync(out, JSON.stringify({ block: b, arm, r: JSON.parse(p.stdout.trim().split('\n').pop()) }) + '\n');
    }
  }
}
console.log('done');
