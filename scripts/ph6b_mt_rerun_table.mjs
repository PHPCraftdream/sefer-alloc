// Ph6b supplementary judge: derives the quiet-machine MT re-measure table from
// docs/perf/_raw_ph6b_mt_rerun_quiet.log (30 alternating B/C runs of the same
// binaries as PH6B_COST_AB.md §3.3). Statistic names are printed by this code.
//
// Per cell (workload,T): median ns per side, C/B = med(C)/med(B) (numerator
// med(C), denominator med(B); >1 = C slower), min ns per side, and a two-sided
// Mann-Whitney U test (normal approximation, average ranks for ties) of the 30
// B samples against the 30 C samples. A cell is a DETECTED REGRESSION only if
// the median ratio exceeds the Mops>=0.90 limit (ns ratio > 1/0.90) AND
// p < 0.05/8 (Bonferroni over the 8 cells); otherwise NO DETECTABLE REGRESSION.
//
// Usage: node scripts/ph6b_mt_rerun_table.mjs   (exit 1 on a detected regression)
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const text = readFileSync(join(root, 'docs/perf/_raw_ph6b_mt_rerun_quiet.log'), 'utf8').replace(/\r\n?/g, '\n');

const cells = new Map();
const re = /^([BC]) run=(\d+) RESULT mt_ns workload=(\w+) T=(\d+) ns=(\d+)$/;
for (const line of text.split('\n')) {
  const m = re.exec(line);
  if (!m) continue;
  const key = `${m[3]} T=${m[4]}`;
  if (!cells.has(key)) cells.set(key, { B: [], C: [] });
  cells.get(key)[m[1]].push(Number(m[5]));
}
if (cells.size !== 8) throw new Error(`expected 8 cells, got ${cells.size}`);

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

const LIMIT = 1 / 0.90;           // Mops C/B >= 0.90  <=>  ns C/B <= 1/0.90
const ALPHA = 0.05 / cells.size;  // Bonferroni over the 8 cells
let regressions = 0;
console.log('# Ph6b MT re-measure (quiet machine): 30 alternating runs per side');
console.log(`ratio = med_ns(C) / med_ns(B) (numerator med C, denominator med B; >1 = C slower); regression iff ratio > ${LIMIT.toFixed(4)} AND Mann-Whitney p < ${ALPHA.toFixed(5)}`);
console.log('');
console.log('| cell | n B/C | med ns B | med ns C | C/B (med C / med B) | min ns B | min ns C | min C/B | MWU p | verdict |');
console.log('|---|---|---:|---:|---:|---:|---:|---:|---:|---|');
for (const [key, v] of cells) {
  if (v.B.length !== 30 || v.C.length !== 30) throw new Error(`${key}: expected 30+30 samples`);
  const mb = median(v.B), mc = median(v.C);
  const ratio = mc / mb;
  const p = mannWhitneyP(v.B, v.C);
  const minB = Math.min(...v.B), minC = Math.min(...v.C);
  const bad = ratio > LIMIT && p < ALPHA;
  if (bad) regressions++;
  if (Math.abs(ratio * mb - mc) > 1e-6 * mc) throw new Error('ratio round-trip failed');
  console.log(`| ${key} | ${v.B.length}/${v.C.length} | ${mb} | ${mc} | ${ratio.toFixed(3)} | ${minB} | ${minC} | ${(minC / minB).toFixed(3)} | ${p.toFixed(3)} | ${bad ? 'REGRESSION' : 'NO DETECTABLE REGRESSION'} |`);
}
console.log('');
console.log(`detected regressions: ${regressions} of ${cells.size}`);
process.exit(regressions === 0 ? 0 : 1);
