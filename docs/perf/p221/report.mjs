// P221: derive the report tables and the summary CSV from the raw A/B samples (ab.jsonl.gz).
// Per block: min over repeats per arm; ratio = geometric mean over blocks of the per-block ratio, 95 % paired t interval.
// usage: node docs/perf/p221/report.mjs [--csv <out.csv>]   (prints the Markdown tables)
import fs from 'node:fs';
import { gunzipSync } from 'node:zlib';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
const here = dirname(fileURLToPath(import.meta.url));
const recs = gunzipSync(fs.readFileSync(join(here, 'ab.jsonl.gz'))).toString().trim().split(/\r?\n/).map((l) => JSON.parse(l));
const T95 = { 11: 2.201 };
const blocks = [...new Set(recs.map((x) => x.block))];
const reps = [...new Set(recs.map((x) => x.rep))];
const scenarios = [...new Set(recs.map((x) => x.r.scenario))];
const M = { cpu: (r) => r.user_ms + r.kernel_ms, user: (r) => r.user_ms, kernel: (r) => r.kernel_ms, wall_ns_per_op: (r) => r.ns_per_op };
const PAIRS = [['x', 'base'], ['base', 'mi'], ['x', 'mi']];
for (const sc of scenarios) for (const a of ['base', 'x', 'mi']) for (const b of blocks) {
  const n = recs.filter((r) => r.block === b && r.arm === a && r.r.scenario === sc).length;
  if (n !== reps.length) throw new Error(`incomplete cell ${sc}/${a}/block ${b}: ${n} samples`);
}
function stat(sc, f, x, y) {
  const l = blocks.map((b) => {
    const v = (a) => Math.min(...recs.filter((r) => r.block === b && r.arm === a && r.r.scenario === sc).map((r) => f(r.r)));
    const [p, q] = [v(x), v(y)];
    return p > 0 && q > 0 ? Math.log(p / q) : NaN;
  });
  if (!l.every(Number.isFinite) || l.every((z) => z === 0)) return null;
  const n = l.length, m = l.reduce((s, z) => s + z, 0) / n;
  const sd = Math.sqrt(l.reduce((s, z) => s + (z - m) ** 2, 0) / (n - 1)), h = (T95[n - 1] * sd) / Math.sqrt(n);
  const r = { n, ratio: Math.exp(m), lo: Math.exp(m - h), hi: Math.exp(m + h) };
  if (!(r.lo <= r.ratio && r.ratio <= r.hi)) throw new Error('interval arithmetic');
  return r;
}
const rows = [];
for (const sc of scenarios) for (const [mk, f] of Object.entries(M)) for (const [x, y] of PAIRS) {
  const s = stat(sc, f, x, y);
  rows.push({ scenario: sc, metric: mk, pair: `${x}/${y}`, ...(s ?? { n: blocks.length, ratio: null, lo: null, hi: null }) });
}
const fmt = (s) => (s.ratio === null ? 'n/a' : `${s.ratio.toFixed(3)} (${s.lo.toFixed(3)}–${s.hi.toFixed(3)})${s.hi < 1 ? ' ▼' : s.lo > 1 ? ' ▲' : ''}`);
console.log(`Samples: ${recs.length} = ${scenarios.length} scenarios × 3 arms × ${blocks.length} blocks × ${reps.length} repeats; statistic: geometric mean over ${blocks.length} blocks of the per-block ratio of per-arm minima, 95 % paired t interval; ▼/▲ = interval wholly below/above 1; n/a = a zero sample (the timer resolution of the kernel or CPU field).\n`);
console.log('| scenario | metric | x/base | base/mi | x/mi |\n|---|---|---|---|---|');
for (const sc of scenarios) for (const mk of Object.keys(M)) {
  const c = PAIRS.map(([x, y]) => fmt(rows.find((r) => r.scenario === sc && r.metric === mk && r.pair === `${x}/${y}`)));
  console.log(`| ${sc} | ${mk} | ${c.join(' | ')} |`);
}
const xb = rows.filter((r) => r.pair === 'x/base' && r.ratio !== null);
const sig = xb.filter((r) => r.lo > 1 || r.hi < 1);
console.log(`\nx/base cells with an interval excluding 1: ${sig.length} of ${xb.length} (${sig.map((r) => `${r.scenario}/${r.metric}`).join(', ') || 'none'}).`);
const i = process.argv.indexOf('--csv');
if (i > 0) {
  const head = 'scenario,metric,pair,blocks,ratio,ci95_lo,ci95_hi';
  const body = rows.map((r) => [r.scenario, r.metric, r.pair, r.n, r.ratio?.toFixed(4) ?? '', r.lo?.toFixed(4) ?? '', r.hi?.toFixed(4) ?? ''].join(','));
  fs.writeFileSync(process.argv[i + 1], [head, ...body].join('\n') + '\n');
}
