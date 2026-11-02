#!/usr/bin/env node
// WG-1 fair axis (measurement only): per scenario, arms `sefer` and `mi` of the wild judge, one process per run,
// arm order alternating by repetition. Records the judge window (ns_per_op), the whole-process wall time
// measured here (spawn to exit), and the judge's own CPU split and page faults taken at the end of the run.
// Usage: node docs/perf/fairaxis/run.mjs <judge-exe> <out.jsonl> [REPS]
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';

const [exe, out, reps = '9'] = process.argv.slice(2);
if (!exe || !out) throw new Error('usage: run.mjs <judge-exe> <out.jsonl> [REPS]');
const SCEN = ['growmix', 'regrow', 'wild', 'large', 'grow64', 'phase', 'grow1k'];
writeFileSync(out, '');
for (const sc of SCEN) {
  for (let rep = 0; rep < Number(reps); rep++) {
    const arms = rep % 2 ? ['mi', 'sefer'] : ['sefer', 'mi'];
    for (const arm of arms) {
      const t0 = process.hrtime.bigint();
      const p = spawnSync(exe, [sc, arm, '1', '1'], { encoding: 'utf8' });
      const ms = Number(process.hrtime.bigint() - t0) / 1e6;
      if (p.status !== 0) throw new Error(`${sc} ${arm}: ${p.status} ${p.stderr}`);
      const r = JSON.parse(p.stdout.trim().split('\n').pop());
      appendFileSync(out, JSON.stringify({ scenario: sc, arm, rep, proc_ms: ms, r }) + '\n');
    }
  }
}
