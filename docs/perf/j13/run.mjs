// J13: paired campaign of the Linux big-block judge, binaries base / x / base2 (a byte-identical copy of base, the A/A control) of the
// Sefer arm plus the mimalloc arm of the base binary. Per block the cells are shuffled (xorshift, fixed seed); within a cell the four
// arms run in a rotation that moves by block, `repeats` runs each. One process per run, started through `wsl.exe -e`.
// usage: node run.mjs BIN_DIR OUT BLOCKS REPEATS     (BIN_DIR holds base, x, base2)
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
const [dir, out, blocks, repeats] = [process.argv[2], process.argv[3], Number(process.argv[4]), Number(process.argv[5])];
// scenario, size in MiB, reps inside one process, k (`many`)
export const CELLS = [
  ['cycle', 1, 20, 1], ['cycle', 16, 10, 1], ['cycle', 128, 6, 1], ['cycle', 1024, 3, 1], ['cycle', 4096, 3, 1],
  ['grow', 128, 4, 1], ['grow', 1024, 3, 1],
  ['many', 16, 3, 128], ['many', 128, 3, 16],
  ['edge', 128, 6, 1],
];
// label, binary, allocator arm
const ARMS = [['base', 'base', 'sefer'], ['x', 'x', 'sefer'], ['base2', 'base2', 'sefer'], ['mi', 'base', 'mi']];
writeFileSync(out, '');
let s = 0x9e3779b97f4a7c15n;
const rnd = (n) => { s ^= s << 13n; s &= (1n << 64n) - 1n; s ^= s >> 7n; s ^= s << 17n; s &= (1n << 64n) - 1n; return Number(s % BigInt(n)); };
for (let b = 0; b < blocks; b++) {
  const order = [...CELLS];
  for (let i = order.length - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
  for (const [sc, size, reps, k] of order) {
    const arms = ARMS.map((_, i) => ARMS[(i + b) % ARMS.length]);
    for (const [label, bin, arm] of arms) for (let r = 0; r < repeats; r++) {
      const args = [sc, arm, String(size), String(reps), ...(sc === 'many' ? [String(k)] : [])];
      const p = spawnSync('wsl.exe', ['-e', `${dir}/${bin}`, ...args], { encoding: 'utf8', env: { ...process.env, MSYS2_ARG_CONV_EXCL: '*' } });
      if (p.status !== 0) throw new Error(`${label} ${args.join(' ')}: ${p.status} ${p.stderr}`);
      appendFileSync(out, JSON.stringify({ block: b, arm: label, r: JSON.parse(p.stdout.trim().split('\n').pop()) }) + '\n');
    }
  }
}
console.log('done');
