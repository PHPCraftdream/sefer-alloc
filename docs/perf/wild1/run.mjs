#!/usr/bin/env node
// WILD-1 runner (measurement only): sefer vs mimalloc on the wild judge, one process per
// (block, scenario, arm, repeat). Scenario order is shuffled per block (fixed seed), the arm order
// alternates by block parity, the repeats of one (block, scenario, arm) run back to back.
// Usage: node run.mjs --exe PATH --out FILE [--blocks 12] [--repeats 3] [--scale 1] [--arms sefer,mi]
//        [--scenarios grow64,grow1k,regrow,growmix,wild,phase,large] [--emit]
import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';

const arg = (k, d) => { const i = process.argv.indexOf(k); return i >= 0 ? process.argv[i + 1] : d; };
const scen = arg('--scenarios', 'grow64,grow1k,regrow,growmix,wild,phase,large').split(',');
const scale = arg('--scale', '1');
const blocks = +arg('--blocks', '12');
const repeats = +arg('--repeats', '3');
const armList = arg('--arms', 'sefer,mi').split(',');
let s = 0x9e3779b97f4a7c15n;
const M = (1n << 64n) - 1n;
const rnd = (n) => { s ^= (s << 13n) & M; s ^= s >> 7n; s ^= (s << 17n) & M; return Number(s % BigInt(n)); };
const sched = [];
for (let b = 0; b < blocks; b++) {
  const order = [...scen];
  for (let i = order.length - 1; i > 0; i--) { const j = rnd(i + 1); [order[i], order[j]] = [order[j], order[i]]; }
  const k = b % armList.length, arms = armList.length === 2 ? (b % 2 === 0 ? armList : [...armList].reverse()) : [...armList.slice(k), ...armList.slice(0, k)];
  for (const sc of order) for (const arm of arms) for (let k = 0; k < repeats; k++) sched.push([b, sc, arm, k]);
}
if (process.argv.includes('--emit')) { for (const r of sched) console.log(r.join(' ')); process.exit(0); }
const exe = arg('--exe'), out = arg('--out');
for (const [b, sc, arm, k] of sched) {
  const r = spawnSync(exe, [sc, arm, '1', scale], { encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`${arm} ${sc}: exit ${r.status} ${r.stderr}`);
  appendFileSync(out, JSON.stringify({ block: b, rep: k, arm, r: JSON.parse(r.stdout.trim()) }) + '\n');
}
