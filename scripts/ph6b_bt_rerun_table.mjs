// Ph6b supplementary judge: derives the quiet-machine re-measure of the single
// bench-table id `global_alloc_churn/*/1024B` (the INCONCLUSIVE max-ratio id of
// PH6B_COST_AB.md §3.2) from docs/perf/_raw_ph6b_bt_rerun_quiet.log.
//
// Per run the log holds criterion's `time: [lo mid hi]` for SeferAlloc and
// mimalloc on the SAME id; r(run) = mid[SeferAlloc] / mid[mimalloc] (numerator
// Sefer, denominator mimalloc). Per side the statistic is the median of r over
// the alternating runs; C/B = med(r_C) / med(r_B). Mann-Whitney U (two-sided,
// normal approximation, average ranks for ties) tests the B r-samples against
// the C r-samples. The id is a DETECTED REGRESSION only if C/B > 1.10 (the
// registered max-ratio limit) AND p < 0.05; otherwise NO DETECTABLE REGRESSION.
//
// Usage: node scripts/ph6b_bt_rerun_table.mjs   (exit 1 on a detected regression)
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const text = readFileSync(join(root, 'docs/perf/_raw_ph6b_bt_rerun_quiet.log'), 'utf8').replace(/\r\n?/g, '\n');

const UNIT = { ns: 1, 'µs': 1e3, us: 1e3, ms: 1e6, s: 1e9 };
function mids() {
  const out = { B: [], C: [] };
  let c = null, i = null;
  for (const line of text.split('\n')) {
    let m = /^== ([BC]) run=(\d+)$/.exec(line);
    if (m) { c = { s: m[1], run: Number(m[2]), v: {} }; i = null; out[m[1]].push(c); continue; }
    m = /^global_alloc_churn\/(SeferAlloc|mimalloc)\/1024B$/.exec(line);
    if (m) { i = m[1]; continue; }
    m = /^\s+time:\s+\[([0-9.]+) (\S+) ([0-9.]+) (\S+) ([0-9.]+) (\S+)\]$/.exec(line);
    if (m && c && i) { c.v[i] = Number(m[3]) * UNIT[m[4]]; i = null; }
  }
  return out;
}
const runs = mids();
const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const erf = (x) => {
  const sign = Math.sign(x);
  x = Math.abs(x);
  const t = 1 / (1 + 0.3275911 * x);
  const y = 1 - ((((1.061405429 * t - 1.453152027) * t + 1.421413741) * t - 0.284496736) * t + 0.254829592) * t * Math.exp(-x * x);
  return sign * y;
};
function mannWhitneyP(x, y) {
  const all = [...x.map((v) => [v, 0]), ...y.map((v) => [v, 1])].sort((a, b) => a[0] - b[0]);
  const ranks = new Array(all.length);
  for (let i = 0; i < all.length;) {
    let j = i;
    while (j + 1 < all.length && all[j + 1][0] === all[i][0]) j++;
    for (let k = i; k <= j; k++) ranks[k] = (i + j) / 2 + 1;
    i = j + 1;
  }
  let rx = 0;
  all.forEach((a, i) => { if (a[1] === 0) rx += ranks[i]; });
  const n1 = x.length, n2 = y.length;
  const u = rx - (n1 * (n1 + 1)) / 2;
  const z = (u - (n1 * n2) / 2) / Math.sqrt((n1 * n2 * (n1 + n2 + 1)) / 12);
  return 2 * (1 - 0.5 * (1 + erf(Math.abs(z) / Math.SQRT2)));
}

const r = { B: [], C: [] }, sef = { B: [], C: [] }, mim = { B: [], C: [] };
for (const s of ['B', 'C']) {
  for (const c of runs[s]) {
    if (!(c.v.SeferAlloc > 0) || !(c.v.mimalloc > 0)) throw new Error(`incomplete run ${s} ${c.run}`);
    sef[s].push(c.v.SeferAlloc); mim[s].push(c.v.mimalloc); r[s].push(c.v.SeferAlloc / c.v.mimalloc);
  }
}
if (r.B.length < 15 || r.B.length !== r.C.length) throw new Error('need equal >=15 alternating runs per side');
const rb = median(r.B), rc = median(r.C), ratio = rc / rb;
if (Math.abs(ratio * rb - rc) > 1e-9 * rc) throw new Error('ratio round-trip failed');
const p = mannWhitneyP(r.B, r.C);
// Seeded (xorshift32) percentile bootstrap of C/B: 10000 resamples of each side.
let seed = 0x9e3779b9;
const rnd = () => { seed ^= seed << 13; seed >>>= 0; seed ^= seed >>> 17; seed ^= seed << 5; seed >>>= 0; return seed / 4294967296; };
const resample = (a) => a.map(() => a[Math.floor(rnd() * a.length)]);
const boots = [];
for (let k = 0; k < 10000; k++) boots.push(median(resample(r.C)) / median(resample(r.B)));
boots.sort((x, y) => x - y);
const ciLo = boots[Math.floor(0.025 * boots.length)], ciHi = boots[Math.floor(0.975 * boots.length)];
const bad = ratio > 1.10 && p < 0.05;
console.log(`# Ph6b bench-table re-measure (quiet machine): global_alloc_churn/*/1024B, ${r.B.length} alternating runs per side`);
console.log('r = ns[SeferAlloc] / ns[mimalloc] within one run (numerator Sefer, denominator mimalloc); statistic = median over runs; C/B = med(r_C) / med(r_B)');
console.log('');
console.log('| side | n | med Sefer ns | med mimalloc ns | med r | min r | max r |');
console.log('|---|---:|---:|---:|---:|---:|---:|');
for (const s of ['B', 'C']) {
  console.log(`| ${s} | ${r[s].length} | ${median(sef[s]).toFixed(0)} | ${median(mim[s]).toFixed(0)} | ${median(r[s]).toFixed(4)} | ${Math.min(...r[s]).toFixed(4)} | ${Math.max(...r[s]).toFixed(4)} |`);
}
console.log('');
console.log(`C/B = ${rc.toFixed(4)} / ${rb.toFixed(4)} = ${ratio.toFixed(4)} | Mann-Whitney p = ${p.toFixed(3)} | bootstrap 95% CI of C/B (10000 resamples, seeded) = [${ciLo.toFixed(3)}, ${ciHi.toFixed(3)}] | limit max-ratio <= 1.10 -> ${bad ? 'REGRESSION' : 'NO DETECTABLE REGRESSION'}`);
process.exit(bad ? 1 : 0);
