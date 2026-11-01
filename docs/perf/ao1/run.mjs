#!/usr/bin/env node
// AO-1 confirmation campaign runner (measurement only). Prints the schedule
// with --emit, or runs it: one judge process per (block, arm, scenario),
// one JSON line per run appended to --out.
// Usage: node docs/perf/ao1/run.mjs --scenarios a,b --scale S --blocks N
//        [--repeats K] [--emit] [--base EXE --base2 EXE --x EXE --out FILE]
// --repeats K runs each (block, arm, scenario) K times back to back.
import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';

const arg = (k, d) => { const i = process.argv.indexOf(k); return i >= 0 ? process.argv[i + 1] : d; };
const scen = arg('--scenarios', 'grow64,grow1k,regrow,growmix,wild,phase,large').split(',');
const scale = arg('--scale', '1');
const blocks = +arg('--blocks', '8');
const repeats = +arg('--repeats', '1');
const PERMS = [['base', 'base2', 'x'], ['x', 'base', 'base2'], ['base2', 'x', 'base'],
  ['base', 'x', 'base2'], ['base2', 'base', 'x'], ['x', 'base2', 'base']];
let s = 0x9e3779b97f4a7c15n;
const rnd = (n) => { s ^= s << 13n; s &= (1n << 64n) - 1n; s ^= s >> 7n; s ^= s << 17n; s &= (1n << 64n) - 1n; return Number(s % BigInt(n)); };
const sched = [];
for (let b = 0; b < blocks; b++) {
  const order = [...scen];
  for (let i = order.length - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
  for (const sc of order) for (const arm of PERMS[b % PERMS.length]) for (let k = 0; k < repeats; k++) sched.push([b, arm, sc]);
}
if (process.argv.includes('--emit')) { for (const r of sched) console.log(r.join(' ')); process.exit(0); }
const exe = { base: arg('--base'), base2: arg('--base2'), x: arg('--x') };
const out = arg('--out');
for (const [b, arm, sc] of sched) {
  const r = spawnSync(exe[arm], [sc, 'sefer', '1', scale], { encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`${arm} ${sc}: exit ${r.status} ${r.stderr}`);
  appendFileSync(out, JSON.stringify({ block: b, arm, r: JSON.parse(r.stdout.trim()) }) + '\n');
}
