#!/usr/bin/env node
// AO-1 campaign 2 controls report (measurement only): the path-activation
// oracle (WJ_ORDER lines), callgrind Ir of the wild-judge growth scenarios and
// of g1_mt_one T=1 (v2 vs v2b vs base), and the two paired bench:table
// verdicts, all parsed from the committed raw files in docs/perf/ao1/.
// Percentages name numerator and denominator in the column heads.
// Usage: node scripts/ao1_controls_report.mjs
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const dir = join(REPO_ROOT, 'docs', 'perf', 'ao1');
const lines = (f) => readFileSync(join(dir, f), 'utf8').trim().split(/\r?\n/);

console.log('| Platform | Scenario | Arm | steps of +block size | total steps | share (steps / total) |');
console.log('|---|---|---|---:|---:|---:|');
for (const [plat, f] of [['win', 'win2_order.txt'], ['linux', 'linux2_order.txt']]) {
  for (const l of lines(f)) {
    const m = l.match(/^(\w+) (\w+) order: (\d+) of (\d+) steps are \+(\d+) B/);
    assert.ok(m, l);
    const [, arm, sc, up, tot] = m;
    console.log(`| ${plat} | ${sc} | ${arm} | ${up} | ${tot} | ${(100 * up / tot).toFixed(2)} % |`);
  }
}

const cg = {};
for (const l of lines('linux2_callgrind.txt')) {
  const m = l.match(/^(\w+) (\w+) .*Collected : (\d+)/); assert.ok(m, l);
  cg[`${m[2]}/${m[1]}`] = +m[3];
}
console.log('\n| wild judge Linux, scale 0.03 | base Ir | v2 Ir | v2b Ir | v2 / base − 1 | v2b / base − 1 |');
console.log('|---|---:|---:|---:|---:|---:|');
for (const sc of ['grow64', 'grow1k']) {
  const b = cg[`${sc}/base`], x = cg[`${sc}/x`], y = cg[`${sc}/y`];
  assert.ok(b && x && y);
  console.log(`| ${sc} | ${b} | ${x} | ${y} | ${(100 * (x / b - 1)).toFixed(2)} % | ${(100 * (y / b - 1)).toFixed(2)} % |`);
}
const mt = {};
for (const l of lines('linux2_mt_callgrind.txt')) {
  const m = l.match(/^(\w+) (\w+) .*Collected : (\d+)/); assert.ok(m, l);
  mt[`${m[2]}/${m[1]}`] = +m[3];
}
console.log('\n| g1_mt_one T=1 | base Ir | v2b Ir | v2b / base − 1 |');
console.log('|---|---:|---:|---:|');
for (const w of ['mstress', 'larson']) {
  console.log(`| ${w} | ${mt[`${w}/base`]} | ${mt[`${w}/y`]} | ${(100 * (mt[`${w}/y`] / mt[`${w}/base`] - 1)).toFixed(3)} % |`);
}

console.log('\n| bench:table paired set | sefer-time gm (x / base − 1) | largest agreed loss | new LOSS cells vs mimalloc | magnitude rule |');
console.log('|---|---:|---:|---:|---|');
for (const s of ['s1', 's2']) {
  const t = readFileSync(join(dir, 'bt', `verdict_${s}.md`), 'utf8');
  const r = t.match(/Magnitude rule: gm ([+-][\d.]+) %.*largest loss ([+-][\d.]+) %.*new LOSS cells (\d+).*=> \*\*(\S+)\*\*/);
  assert.ok(r, s);
  console.log(`| ${s} | ${r[1]} % | ${r[2]} % | ${r[3]} | ${r[4]} |`);
}
