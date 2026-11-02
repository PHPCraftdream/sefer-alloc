#!/usr/bin/env node
// RET-1 measurement A (Windows; measurement only): judge-window ns/op AND the
// whole-process elapsed time (spawn to exit) per pool size, one process per run,
// pool order rotated per repetition.
// Usage: RET1_BIN_DIR=<dir with time_<N>.exe> RET1_OUT=<jsonl> node docs/perf/ret1/run.mjs [REPS]
// time_<N>.exe = examples/g1_wild_judge built with WJ_POOL=<N> (ident.txt).
import { spawnSync } from 'node:child_process';
import { appendFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

const BIN = process.env.RET1_BIN_DIR;
const OUT = process.env.RET1_OUT;
if (!BIN || !OUT) throw new Error('set RET1_BIN_DIR and RET1_OUT');
const REPS = Number(process.argv[2] ?? 9);
const POOLS = [4, 16, 64, 1024];
const SCEN = ['grow64', 'grow1k', 'regrow', 'growmix'];
writeFileSync(OUT, '');
for (const sc of SCEN) {
  for (let rep = 0; rep < REPS; rep++) {
    for (let k = 0; k < POOLS.length; k++) {
      const n = POOLS[(k + rep) % POOLS.length];
      const t0 = process.hrtime.bigint();
      const p = spawnSync(join(BIN, `time_${n}.exe`), [sc, 'sefer_pool', '1', '1'], { encoding: 'utf8' });
      const ms = Number(process.hrtime.bigint() - t0) / 1e6;
      if (p.status !== 0) throw new Error(`${n} ${sc}: ${p.status} ${p.stderr}`);
      const r = JSON.parse(p.stdout.trim().split('\n').pop());
      appendFileSync(OUT, JSON.stringify({ pool: n, scenario: sc, rep, proc_ms: ms, r }) + '\n');
    }
  }
}
