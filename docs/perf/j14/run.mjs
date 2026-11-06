// J14: paired campaign of the big-block judge. Arms are labels mapped to a binary and an allocator: base, x (large-cache-adaptive),
// xt (x plus the J13 hint, Linux only), base2 (a byte-identical copy of base, the A/A control) and mi (mimalloc, from the base binary).
// Per block the cells are shuffled (xorshift, fixed seed); within a cell the arms run in a rotation that moves by block. One process per run.
// usage: node run.mjs <win|linux> BIN_DIR OUT BLOCKS REPEATS     (BIN_DIR holds base, x, base2 [, xt])
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
const [plat, dir, out, blocks, repeats] = [process.argv[2], process.argv[3], process.argv[4], Number(process.argv[5]), Number(process.argv[6])];
// scenario, size in MiB, reps inside one process, k, repeats override (null = REPEATS)
// many: k blocks of `size` MiB — 128 x 16 MiB and 16 x 128 MiB; idle: the 128 x 16 MiB habit, then 12 s of light activity
export const CELLS = [
  ['many', 16, 5, 128, null], ['many', 128, 5, 16, null], ['idle', 16, 3, 128, 1],
  ['cycle', 1, 20, 1, null], ['cycle', 128, 6, 1, null], ['grow', 128, 4, 1, null], ['edge', 128, 6, 1, null],
];
const ARMS = [['base', 'base', 'sefer'], ['x', 'x', 'sefer'], ['base2', 'base2', 'sefer'], ['mi', 'base', 'mi']];
// the binaries live inside WSL: test there, not with Node's existsSync (false for a WSL path)
if (plat === 'linux' && spawnSync('wsl.exe', ['-e', 'test', '-x', `${dir}/xt`], { env: { ...process.env, MSYS2_ARG_CONV_EXCL: '*' } }).status === 0) ARMS.splice(2, 0, ['xt', 'xt', 'sefer']);
writeFileSync(out, '');
let s = 0x9e3779b97f4a7c15n;
const rnd = (n) => { s ^= s << 13n; s &= (1n << 64n) - 1n; s ^= s >> 7n; s ^= s << 17n; s &= (1n << 64n) - 1n; return Number(s % BigInt(n)); };
for (let b = 0; b < blocks; b++) {
  const order = [...CELLS];
  for (let i = order.length - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
  for (const [sc, size, reps, k, rp] of order) {
    const arms = ARMS.map((_, i) => ARMS[(i + b) % ARMS.length]);
    for (const [label, bin, arm] of arms) for (let r = 0; r < (rp ?? repeats); r++) {
      const args = [sc, arm, String(size), String(reps), ...(sc === 'many' || sc === 'idle' ? [String(k)] : [])];
      const exe = plat === 'linux' ? `${dir}/${bin}` : `${dir}/${bin}.exe`;
      const p = plat === 'linux'
        ? spawnSync('wsl.exe', ['-e', exe, ...args], { encoding: 'utf8', env: { ...process.env, MSYS2_ARG_CONV_EXCL: '*' } })
        : spawnSync(exe, args, { encoding: 'utf8' });
      if (p.status !== 0) throw new Error(`${label} ${args.join(' ')}: ${p.status} ${p.stderr}`);
      appendFileSync(out, JSON.stringify({ block: b, arm: label, r: JSON.parse(p.stdout.trim().split('\n').pop()) }) + '\n');
    }
  }
}
console.log('done');
