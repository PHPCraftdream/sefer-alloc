// CF-1 Windows mechanism oracle: counters per pool size (bench-internals builds), REPS runs per cell.
import { spawnSync } from 'node:child_process';
import { writeFileSync, appendFileSync } from 'node:fs';
import { join } from 'node:path';
const W = process.env.CF1_DIR + '/winout';
const reps = Number(process.argv[2]);
writeFileSync(join(W, 'mech.jsonl'), '');
for (const sc of ['grow64', 'grow1k', 'growmix', 'regrow', 'large', 'wild', 'phase']) {
  for (let rep = 0; rep < reps; rep++) {
    for (const n of [0, 4, 8, 16, 32, 64, 1024]) {
      const p = spawnSync(join(W, 'bin', `ev_${n}.exe`), [sc, 'sefer_pool', '1', '1'], { encoding: 'utf8' });
      if (p.status !== 0) throw new Error(`ev_${n} ${sc}: ${p.status} ${p.stderr}`);
      appendFileSync(join(W, 'mech.jsonl'), JSON.stringify({ want: n, rep, faults: 0, run: JSON.parse(p.stdout.trim().split('\n').pop()) }) + '\n');
    }
  }
}
console.log('mech-done');
