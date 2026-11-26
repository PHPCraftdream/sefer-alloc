// #218: derives the P218 tables from docs/perf/p218/{ab_raw,oracle_raw}.jsonl and identity.json, writes
// docs/perf/P218_SCATTERED_FREE_NULL_summary.csv, prints the Markdown tables. Asserts every printed figure.
import { readFileSync, writeFileSync } from 'node:fs';
const dir = new URL('./', import.meta.url);
const perf = new URL('../', import.meta.url);
function assert(c, m) { if (!c) throw new Error(m); }
const lines = (n) => readFileSync(new URL(n, dir), 'utf8').trim().split(/\r?\n/).map((l) => JSON.parse(l));
const id = JSON.parse(readFileSync(new URL('identity.json', dir), 'utf8'));
const recs = lines('ab_raw.jsonl');
const T = { 11: 2.201 }; // two-sided 95 % t quantile, df = blocks - 1
const ARMS = ['base', 'v1', 'v3', 'mi'], SC = ['growmix', 'regrow', 'wild'];
const blocks = [...new Set(recs.map((x) => x.block))].sort((p, q) => p - q);
const reps = Math.max(...recs.map((x) => x.rep)) + 1;
assert(blocks.length === id.blocks && reps === id.repeats, 'design');
assert(recs.length === blocks.length * reps * ARMS.length * SC.length, `record count ${recs.length}`);
const ci = (v) => {
  const n = v.length, m = v.reduce((a, b) => a + b, 0) / n;
  const sd = Math.sqrt(v.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1)), t = T[n - 1];
  assert(t, `no t for n=${n}`);
  const h = (t * sd) / Math.sqrt(n);
  const [g, lo, hi] = [Math.exp(m), Math.exp(m - h), Math.exp(m + h)];
  assert(lo <= g && g <= hi, 'ci order');
  return { g, lo, hi, s: `${g.toFixed(3)} (${lo.toFixed(3)}–${hi.toFixed(3)})`, verdict: hi < 1 ? 'faster' : lo > 1 ? 'slower' : 'tie' };
};
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
const PAIRS = [['v1', 'base'], ['v3', 'base'], ['base', 'mi'], ['v1', 'mi'], ['v3', 'mi']];
let csv = 'scenario,pair,ratio_geomean,ci95_lo,ci95_hi,verdict,blocks,repeats\n';
const rows = [];
for (const sc of SC) {
  const v = {};
  for (const b of blocks) for (const a of ARMS) {
    const R = recs.filter((x) => x.block === b && x.arm === a && x.r.scenario === sc);
    assert(R.length === reps, `${sc} ${a} block ${b}`);
    assert(new Set(R.map((x) => x.r.checksum)).size === 1, `checksum ${sc} ${a}`);
    (v[a] ??= []).push(Math.min(...R.map((x) => x.r.ns_per_op)));
  }
  const cs = new Set(['base', 'v1', 'v3'].map((a) => recs.find((x) => x.arm === a && x.r.scenario === sc).r.checksum));
  assert(cs.size === 1, `Sefer variants disagree on ${sc} checksum`);
  const cell = {};
  for (const [x, y] of PAIRS) {
    const c = ci(blocks.map((_, i) => Math.log(v[x][i] / v[y][i])));
    cell[`${x}/${y}`] = c;
    csv += `${sc},${x}/${y},${c.g.toFixed(4)},${c.lo.toFixed(4)},${c.hi.toFixed(4)},${c.verdict},${blocks.length},${reps}\n`;
  }
  const mdn = (a, k) => med(recs.filter((x) => x.arm === a && x.r.scenario === sc).map((x) => x.r[k]));
  rows.push({ sc, cell, ns: ARMS.map((a) => mdn(a, 'ns_per_op').toFixed(1)).join(' / '),
    rss: ARMS.map((a) => (mdn(a, 'end_rss') / 1048576).toFixed(0)).join(' / ') });
}
writeFileSync(new URL('P218_SCATTERED_FREE_NULL_summary.csv', perf), csv);
const wins = rows.flatMap((r) => ['v1/base', 'v3/base'].filter((k) => r.cell[k].verdict === 'faster'));
assert(wins.length === 0, 'a variant won: this is not a NULL report');

console.log(`| scenario | v1/base | v3/base | base/mi | v1/mi | v3/mi | median ns/op base / v1 / v3 / mi | median end RSS MiB base / v1 / v3 / mi |`);
console.log(`|---|---:|---:|---:|---:|---:|---:|---:|`);
for (const r of rows) console.log(`| ${r.sc} | ${PAIRS.map(([x, y]) => r.cell[`${x}/${y}`].s).join(' | ')} | ${r.ns} | ${r.rss} |`);
console.log(`\nVerdicts (95 % interval wholly below / above 1): ${rows.map((r) => `${r.sc} ${PAIRS.map(([x, y]) => `${x}/${y} ${r.cell[`${x}/${y}`].verdict}`).join(', ')}`).join('; ')}.`);

// Path-activation oracle: one bench-internals run per (variant, scenario).
const o = lines('oracle_raw.jsonl');
assert(o.length === 9, 'oracle runs');
const get = (v, sc) => { const z = o.find((x) => x.variant === v && x.r.scenario === sc); assert(z, `${v} ${sc}`); return z.r; };
console.log(`\n| scenario | base lane_second_chance | v1 lane_second_chance / p218_lane | v3 lane_second_chance / p218_lane / p218_flush | checksum equal |`);
console.log(`|---|---:|---:|---:|---|`);
for (const sc of SC) {
  const [b, x1, x3] = ['base', 'v1', 'v3'].map((v) => get(v, sc));
  assert(b.lane_second_chance > 0 && b.p218_lane === undefined, `base ${sc}`);
  assert(x1.lane_second_chance === 0 && x1.p218_lane > 0, `v1 ${sc}`);
  assert(x3.lane_second_chance === 0 && x3.p218_lane > 0 && x3.p218_flush > 0, `v3 ${sc}`);
  assert(b.checksum === x1.checksum && b.checksum === x3.checksum, `oracle checksum ${sc}`);
  const f = (n) => n.toLocaleString('en-US');
  console.log(`| ${sc} | ${f(b.lane_second_chance)} | ${f(x1.lane_second_chance)} / ${f(x1.p218_lane)} | ${f(x3.lane_second_chance)} / ${f(x3.p218_lane)} / ${f(x3.p218_flush)} | yes |`);
}
