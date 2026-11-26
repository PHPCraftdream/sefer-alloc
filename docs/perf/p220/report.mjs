// #220: derives the P220 tables. (1) CPU time vs wall time of the P218 A/B (../p218/ab_raw.jsonl); (2) simulated cache
// misses of growmix by code category and by enclosing function (cg_growmix.{sefer,mi}.out.gz). Writes
// ../P220_TEARDOWN_MISS_ATTRIBUTION_summary.csv and prints the Markdown tables. Asserts every printed figure.
import { readFileSync, writeFileSync } from 'node:fs';
import { gunzipSync } from 'node:zlib';
const dir = new URL('./', import.meta.url);
function assert(c, m) { if (!c) throw new Error(m); }
let csv = 'section,scenario_or_arm,key,value\n';
const row = (s, a, k, v) => { csv += `${s},${a},${k},${v}\n`; };

// (1) P218 A/B: per-block min over 3 repeats, geometric mean of ratios over 12 blocks, 95 % t (df 11).
const recs = readFileSync(new URL('../p218/ab_raw.jsonl', dir), 'utf8').trim().split(/\r?\n/).map((l) => JSON.parse(l));
const blocks = [...new Set(recs.map((x) => x.block))];
assert(blocks.length === 12 && recs.length === 432, 'P218 design');
const T11 = 2.201;
const metric = { cpu: (r) => r.user_ms + r.kernel_ms, user: (r) => r.user_ms, kernel: (r) => r.kernel_ms, wall: (r) => r.ns_per_op };
const ci = (l) => {
  const n = l.length, m = l.reduce((a, b) => a + b, 0) / n, sd = Math.sqrt(l.reduce((a, b) => a + (b - m) ** 2, 0) / (n - 1)), h = (T11 * sd) / Math.sqrt(n);
  return { g: Math.exp(m), lo: Math.exp(m - h), hi: Math.exp(m + h) };
};
const s3 = (c) => `${c.g.toFixed(3)} (${c.lo.toFixed(3)}–${c.hi.toFixed(3)})`;
console.log('| scenario | metric | v1/base | v3/base | base/mi |\n|---|---|---:|---:|---:|');
for (const sc of ['growmix', 'regrow', 'wild']) for (const [mk, f] of Object.entries(metric)) {
  if (mk === 'kernel' && sc === 'wild') continue; // wild kernel time is 1-2 timer ticks (15.6 ms), often 0
  const cells = [['v1', 'base'], ['v3', 'base'], ['base', 'mi']].map(([x, y]) => {
    const l = blocks.map((b) => {
      const v = (a) => Math.min(...recs.filter((r) => r.block === b && r.arm === a && r.r.scenario === sc).map((r) => f(r.r)));
      const [p, q] = [v(x), v(y)];
      assert(p > 0 && q > 0, `${sc} ${mk} zero`);
      return Math.log(p / q);
    });
    const c = ci(l);
    row('ab_ratio', sc, `${mk}:${x}/${y}`, `${c.g.toFixed(4)};${c.lo.toFixed(4)};${c.hi.toFixed(4)}`);
    return s3(c);
  });
  console.log(`| ${sc} | ${mk} | ${cells.join(' | ')} |`);
}
// Off-CPU share under the 10 % CPU cap: process CPU time / wall time of the timed window, median per (scenario, arm).
console.log('\n| scenario | arm | median wall ms | median CPU ms | CPU / wall |\n|---|---|---:|---:|---:|');
const med = (a) => [...a].sort((x, y) => x - y)[a.length >> 1];
for (const sc of ['growmix', 'regrow', 'wild']) for (const a of ['base', 'mi']) {
  const R = recs.filter((r) => r.arm === a && r.r.scenario === sc).map((r) => r.r);
  const wall = med(R.map((r) => (r.ns_per_op * r.ops) / 1e6)), cpu = med(R.map((r) => r.user_ms + r.kernel_ms));
  assert(cpu < wall, 'cpu < wall');
  row('cpu_share', `${sc}:${a}`, 'cpu_over_wall', (cpu / wall).toFixed(3));
  console.log(`| ${sc} | ${a} | ${wall.toFixed(0)} | ${cpu.toFixed(0)} | ${(cpu / wall).toFixed(3)} |`);
}

// (2) callgrind --cache-sim=yes: self cost by code category and LL misses by enclosing function.
const cat = (f) => /examples\/g1_wild_judge\.rs/.test(f) ? 'judge' : /mimalloc/.test(f) ? 'mimalloc' : /sefer|crates\/|\/src\/global|\/src\/registry/.test(f) ? 'sefer' : /rustlib|library\/(core|std|alloc)/.test(f) ? 'std (inlined)' : 'other';
function parse(name) {
  const L = gunzipSync(readFileSync(new URL(name, dir))).toString('utf8').split(/\r?\n/);
  let ev = [], file = '', fl = '', fn = '', inCall = false;
  const byCat = new Map(), byFn = new Map();
  for (const l of L) {
    if (l.startsWith('events:')) { ev = l.slice(7).trim().split(/\s+/); continue; }
    if (l.startsWith('fl=')) { file = fl = l.slice(3); continue; }
    if (l.startsWith('fi=') || l.startsWith('fe=')) { file = l.slice(3); continue; }
    if (l.startsWith('fn=')) { fn = l.slice(3); file = fl; inCall = false; continue; }
    if (l.startsWith('calls=')) { inCall = true; continue; }
    if (!/^\d+( \d+)+$/.test(l)) continue;
    if (inCall) { inCall = false; continue; }
    const v = l.split(' ').map(Number);
    const add = (m, k) => { let a = m.get(k); if (!a) m.set(k, (a = new Array(ev.length).fill(0))); for (let i = 1; i < v.length; i++) a[i - 1] += v[i]; };
    add(byCat, cat(file)); add(byFn, fn.replace(/^0x[0-9a-f]+ /, ''));
  }
  return { ev, byCat, byFn };
}
const S = parse('cg_growmix.sefer.out.gz'), M = parse('cg_growmix.mi.out.gz');
assert(S.ev.join() === M.ev.join() && S.ev.includes('DLmr'), 'events');
const ix = (e) => S.ev.indexOf(e);
const SHOW = ['Ir', 'D1mr', 'D1mw', 'DLmr', 'DLmw'];
const tot = (X) => [...X.byCat.values()].reduce((t, a) => t.map((x, i) => x + a[i]), new Array(S.ev.length).fill(0));
const f = (n) => n.toLocaleString('en-US');
console.log(`\n| arm | category | ${SHOW.join(' | ')} |\n|---|---|${SHOW.map(() => '---:').join('|')}|`);
for (const [arm, X] of [['sefer', S], ['mi', M]]) {
  for (const k of ['judge', 'sefer', 'mimalloc', 'std (inlined)', 'other']) {
    const a = X.byCat.get(k) ?? new Array(S.ev.length).fill(0);
    SHOW.forEach((e) => row('cachesim_category', arm, `${k}:${e}`, a[ix(e)]));
    console.log(`| ${arm} | ${k} | ${SHOW.map((e) => f(a[ix(e)])).join(' | ')} |`);
  }
  const t = tot(X);
  SHOW.forEach((e) => row('cachesim_total', arm, e, t[ix(e)]));
  console.log(`| ${arm} | **total** | ${SHOW.map((e) => f(t[ix(e)])).join(' | ')} |`);
}
const ts = tot(S), tm = tot(M);
const llr = ts[ix('DLmr')] / tm[ix('DLmr')];
assert(llr > 5 && llr < 6, `LL read-miss ratio ${llr}`);
console.log(`\nLL read misses sefer/mi = ${f(ts[ix('DLmr')])} / ${f(tm[ix('DLmr')])} = ${llr.toFixed(2)}.`);
console.log(`\n| arm | enclosing function | DLmr | DLmw | D1mr |\n|---|---|---:|---:|---:|`);
for (const [arm, X] of [['sefer', S], ['mi', M]]) {
  const top = [...X.byFn].sort((p, q) => q[1][ix('DLmr')] + q[1][ix('DLmw')] - p[1][ix('DLmr')] - p[1][ix('DLmw')]).slice(0, 8);
  for (const [k, a] of top) {
    row('cachesim_fn', arm, `"${k}"`, `${a[ix('DLmr')]};${a[ix('DLmw')]};${a[ix('D1mr')]}`);
    const short = k.replace(/ \(\.llvm[^)]*\)/, '').replace(/[<>]/g, '').split('::').slice(-2).join('::');
    console.log(`| ${arm} | \`${short}\` | ${f(a[ix('DLmr')])} | ${f(a[ix('DLmw')])} | ${f(a[ix('D1mr')])} |`);
  }
}
writeFileSync(new URL('../P220_TEARDOWN_MISS_ATTRIBUTION_summary.csv', dir), csv);
