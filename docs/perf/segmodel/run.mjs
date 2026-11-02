#!/usr/bin/env node
// Own-thread free lane model (measurement only, counts): runs the `sefer` arm of the wild judge with WJ_SEGMODEL=1
// and records, per scenario and run, the memo / own_cache / hash-probe counts of each modelled cache shape.
// Usage: node docs/perf/segmodel/run.mjs <judge-exe> <out.jsonl> [RUNS]
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';

const [exe, out, runs = '3'] = process.argv.slice(2);
if (!exe || !out) throw new Error('usage: run.mjs <judge-exe> <out.jsonl> [RUNS]');
const SCEN = ['wild', 'growmix', 'phase', 'regrow', 'large', 'grow64', 'grow1k'];
writeFileSync(out, '');
for (const sc of SCEN) {
  for (let i = 0; i < Number(runs); i++) {
    const p = spawnSync(exe, [sc, 'sefer', '1', '1'], { encoding: 'utf8', env: { ...process.env, WJ_SEGMODEL: '1' } });
    if (p.status !== 0) throw new Error(`${sc}: ${p.status} ${p.stderr}`);
    appendFileSync(out, JSON.stringify({ scenario: sc, run: i, r: JSON.parse(p.stdout.trim().split('\n').pop()) }) + '\n');
  }
}
