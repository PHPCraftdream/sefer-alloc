#!/usr/bin/env node
// G1 wild judge driver (measurement only): runs examples/g1_wild_judge.rs one
// process at a time (the machine is shared; each run is bounded to 4 GiB peak
// RSS by the example itself), sefer and mimalloc in ABBA order per round, and
// derives one table per scenario: min ns/op per arm, median peak RSS per arm.
//
// Usage: node scripts/g1_wild_judge.mjs --out DIR [--rounds 2] [--seed 1]
//          [--scale 1] [--scenarios grow64,grow1k,...] [--bin PATH]
// Writes DIR/raw.jsonl (one line per process), DIR/summary.csv, DIR/table.md.
import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, appendFileSync, existsSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const SCENARIOS = ['grow64', 'grow1k', 'growmix', 'regrow', 'large', 'wild', 'phase'];
const argv = process.argv.slice(2);
const opt = (k, d) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : d; };
const out = opt('--out', null);
if (!out) throw new Error('--out DIR is required');
const rounds = Number(opt('--rounds', '2'));
const seed = opt('--seed', '1');
const scale = opt('--scale', '1');
const scenarios = opt('--scenarios', SCENARIOS.join(',')).split(',');
for (const s of scenarios) if (!SCENARIOS.includes(s)) throw new Error(`unknown scenario ${s}`);

const exe = process.platform === 'win32' ? '.exe' : '';
let bin = opt('--bin', null);
if (!bin) {
  execFileSync('cargo', ['build', '--release', '-q', '--example', 'g1_wild_judge', '--features', 'production'],
    { cwd: REPO_ROOT, stdio: 'inherit' });
  bin = join(REPO_ROOT, 'target', 'release', 'examples', `g1_wild_judge${exe}`);
}
if (!existsSync(bin)) throw new Error(`binary not found: ${bin}`);
mkdirSync(out, { recursive: true });
const raw = join(out, 'raw.jsonl');
writeFileSync(raw, '');

const runs = [];
for (const sc of scenarios) {
  for (let r = 0; r < rounds; r++) {
    for (const arm of ['sefer', 'mi', 'mi', 'sefer']) {
      const p = spawnSync(bin, [sc, arm, seed, scale], { encoding: 'utf8' });
      if (p.status !== 0) {
        throw new Error(`${sc} ${arm} failed (status ${p.status}): ${p.stderr.trim()}`);
      }
      const line = p.stdout.trim().split('\n').pop();
      const rec = { ...JSON.parse(line), round: r };
      appendFileSync(raw, JSON.stringify(rec) + '\n');
      runs.push(rec);
      console.log(`${sc} r${r} ${arm} ${rec.ns_per_op} ns/op peak ${(rec.peak_rss / 2 ** 20).toFixed(0)} MiB`);
    }
  }
}

const median = (a) => { const s = [...a].sort((x, y) => x - y); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const rows = [];
for (const sc of scenarios) {
  const of = (arm) => runs.filter((x) => x.scenario === sc && x.arm === arm);
  const [s, m] = [of('sefer'), of('mi')];
  const sums = new Set([...s, ...m].map((x) => x.checksum));
  if (sums.size !== 1) throw new Error(`${sc}: arms ran different op sequences (checksums ${[...sums]})`);
  const ns = (v) => Math.min(...v.map((x) => x.ns_per_op));
  const rss = (v) => median(v.map((x) => x.peak_rss));
  const row = {
    scenario: sc, ops: s[0].ops, runs: s.length,
    sefer_ns: ns(s), mi_ns: ns(m), sefer_rss: rss(s), mi_rss: rss(m),
  };
  row.ns_ratio = row.sefer_ns / row.mi_ns;
  row.rss_ratio = row.sefer_rss / row.mi_rss;
  rows.push(row);
}

const head = 'scenario,ops,runs_per_arm,sefer_ns_min,mi_ns_min,ns_ratio,sefer_peak_rss_median,mi_peak_rss_median,rss_ratio,seed,scale';
writeFileSync(join(out, 'summary.csv'), [head, ...rows.map((r) =>
  [r.scenario, r.ops, r.runs, r.sefer_ns, r.mi_ns, r.ns_ratio.toFixed(4), r.sefer_rss, r.mi_rss, r.rss_ratio.toFixed(4), seed, scale].join(','))].join('\n') + '\n');
const mib = (b) => (b / 2 ** 20).toFixed(0);
const md = [
  `# g1_wild_judge: seed ${seed}, scale ${scale}, ${rounds} round(s) of ABBA per scenario, one process per run`,
  '',
  'ns = min over runs of ns per op (op = one alloc, free or realloc); RSS = median peak RSS. Ratio = sefer / mimalloc (< 1: sefer better).',
  '',
  '| Scenario | ops | sefer ns/op | mi ns/op | time ratio | sefer peak MiB | mi peak MiB | RSS ratio |',
  '|---|---:|---:|---:|---:|---:|---:|---:|',
  ...rows.map((r) => `| ${r.scenario} | ${r.ops} | ${r.sefer_ns.toFixed(2)} | ${r.mi_ns.toFixed(2)} | ${r.ns_ratio.toFixed(3)} | ${mib(r.sefer_rss)} | ${mib(r.mi_rss)} | ${r.rss_ratio.toFixed(3)} |`),
  '',
].join('\n');
writeFileSync(join(out, 'table.md'), md);
console.log('\n' + md);
