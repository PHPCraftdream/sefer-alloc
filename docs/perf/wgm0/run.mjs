#!/usr/bin/env node
// WG-M0 runner (measurement only): one judge process per (block, position, arm,
// scenario). Every block runs the three arms in one of the 6 orders (position
// balanced), the scenario order is fixed, one JSON line per run.
// Usage: node docs/perf/wgm0/run.mjs --exe PATH --label NAME --out FILE [--blocks 2] [--scale 1]
//        [--scenarios grow64,grow1k,regrow,growmix]
import { spawnSync } from 'node:child_process';
import { appendFileSync } from 'node:fs';

const arg = (k, d) => { const i = process.argv.indexOf(k); return i >= 0 ? process.argv[i + 1] : d; };
const exe = arg('--exe'), label = arg('--label'), out = arg('--out');
const blocks = +arg('--blocks', '2'), scale = arg('--scale', '1');
const scen = arg('--scenarios', 'grow64,grow1k,regrow,growmix').split(',');
const PERMS = [['sefer', 'sefer_pool', 'mi'], ['sefer', 'mi', 'sefer_pool'], ['sefer_pool', 'sefer', 'mi'],
  ['sefer_pool', 'mi', 'sefer'], ['mi', 'sefer', 'sefer_pool'], ['mi', 'sefer_pool', 'sefer']];
for (const sc of scen) for (let b = 1; b <= blocks; b++) for (const p of PERMS) for (const arm of p) {
  const r = spawnSync(exe, [sc, arm, '1', scale], { encoding: 'utf8' });
  if (r.status !== 0) throw new Error(`${label} ${sc} ${arm}: exit ${r.status} ${r.stderr}`);
  appendFileSync(out, JSON.stringify({ label, blk: b, pos: p.join(' '), r: JSON.parse(r.stdout.trim()) }) + '\n');
}
