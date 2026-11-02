#!/usr/bin/env node
// AO-1 campaign 3 controls report (measurement only): the path-activation
// oracle (WJ_ORDER lines) of the campaign-3 judge builds and the three paired
// bench:table verdicts, parsed from the committed raw files in docs/perf/ao1/.
// Percentages name numerator and denominator in the column heads.
// Usage: node scripts/ao1_campaign3_report.mjs
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import assert from 'node:assert/strict';
import { REPO_ROOT } from './lib.mjs';

const dir = join(REPO_ROOT, 'docs', 'perf', 'ao1');
const lines = (f) => readFileSync(join(dir, f), 'utf8').trim().split(/\r?\n/);

console.log('| Platform | Scenario | Arm | steps of +block size | total steps | share (steps / total) |');
console.log('|---|---|---|---:|---:|---:|');
for (const [plat, f] of [['win', 'win3_order.txt'], ['linux', 'linux3_order.txt']]) {
  for (const l of lines(f)) {
    const m = l.match(/^(\w+) (\w+) order: (\d+) of (\d+) steps are \+(\d+) B/);
    assert.ok(m, l);
    const [, arm, sc, up, tot] = m;
    const share = (100 * up / tot).toFixed(2);
    if (arm === 'base') assert.equal(+up, 0, `${plat}/${sc}: base must not issue ascending`);
    else assert.ok(+share >= 99.9, `${plat}/${sc}: x must issue ascending`);
    console.log(`| ${plat} | ${sc} | ${arm} | ${up} | ${tot} | ${share} % |`);
  }
}

console.log('\n| bench:table paired set | sefer-time gm (x / base − 1) | largest agreed loss | new LOSS cells vs mimalloc | magnitude rule |');
console.log('|---|---:|---:|---:|---|');
const d256 = [];
for (const s of ['s1', 's2', 's3']) {
  const t = readFileSync(join(dir, 'bt3', `verdict_${s}.md`), 'utf8');
  const r = t.match(/Magnitude rule: gm ([+-][\d.]+) %.*largest loss ([+-][\d.]+) %.*new LOSS cells (\d+).*=> \*\*(\S+)\*\*/);
  assert.ok(r, s);
  console.log(`| ${s} | ${r[1]} % | ${r[2]} % | ${r[3]} | ${r[4]} |`);
  const c = t.match(/\| direct 256 B \| [^|]*\| [^|]*\| ([+-][\d.]+) % \/ ([+-][\d.]+) % \|/);
  assert.ok(c, `${s}: direct 256 B row`);
  d256.push(+c[1], +c[2]);
}

assert.equal(d256.length, 6);
const mean = d256.reduce((a, b) => a + b, 0) / d256.length;
console.log(`\ndirect 256 B, sefer time (x / base − 1) per pair over 3 sets: ${d256.map((v) => `${v >= 0 ? '+' : ''}${v.toFixed(1)} %`).join(', ')}; mean of 6 pairs ${mean >= 0 ? '+' : ''}${mean.toFixed(1)} %, pairs > 0: ${d256.filter((v) => v > 0).length} of 6`);
