// CF-1 Windows sweep: same design as sweep.sh (one process per run, arm order rotated per rep).
// usage: node win_sweep.mjs REPS SCALE
import { spawnSync } from 'node:child_process';
import { writeFileSync, appendFileSync } from 'node:fs';
import { join } from 'node:path';
const W = process.env.CF1_DIR + '/winout';
const [reps, scale] = [Number(process.argv[2]), process.argv[3]];
const LABELS = ['sefer', 'p0', 'p4', 'p8', 'p16', 'p32', 'p64', 'p1024', 'mi'];
const SCEN = ['grow64', 'grow1k', 'growmix', 'regrow', 'large', 'wild', 'phase'];
const run = (bin, sc, arm, sc2) => {
  const p = spawnSync(join(W, 'bin', bin), [sc, arm, '1', sc2], { encoding: 'utf8' });
  if (p.status !== 0) throw new Error(`${bin} ${sc} ${arm}: status ${p.status} ${p.stderr}`);
  return JSON.parse(p.stdout.trim().split('\n').pop());
};
writeFileSync(join(W, 'evidence.jsonl'), '');
for (const n of [0, 4, 8, 16, 32, 64, 1024]) {
  appendFileSync(join(W, 'evidence.jsonl'), JSON.stringify({ want: n, out: run(`ev_${n}.exe`, 'regrow', 'sefer_pool', '0.1') }) + '\n');
}
writeFileSync(join(W, 'raw.jsonl'), '');
for (const sc of SCEN) {
  for (let rep = 0; rep < reps; rep++) {
    for (let k = 0; k < LABELS.length; k++) {
      const l = LABELS[(k + rep) % LABELS.length];
      const [bin, arm] = l === 'sefer' ? ['time_1024.exe', 'sefer'] : l === 'mi' ? ['time_1024.exe', 'mi'] : [`time_${l.slice(1)}.exe`, 'sefer_pool'];
      appendFileSync(join(W, 'raw.jsonl'), JSON.stringify({ label: l, rep, run: run(bin, sc, arm, scale) }) + '\n');
    }
  }
  console.log('done', sc);
}
console.log('sweep-done');
