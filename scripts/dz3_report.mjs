#!/usr/bin/env node
// DZ-3 report generator: every number of docs/perf/DZ3_PIN_CACHE_DIRECT_MAPPED.md comes from docs/perf/dz3/ through
// this script; ratios are asserted against the printed values.
// Input: per-process JSON lines of the g1_rhythm_probe `pincycle` mode (time per foreign free), counting-build lines
// (pin cache hits / misses), callgrind program totals; design-iteration runs of scratch prototypes.
// Output: docs/perf/dz3/tables.md, docs/perf/DZ3_PIN_CACHE_DIRECT_MAPPED_summary.csv and, when doc_head.md / doc_tail.md
// exist, docs/perf/DZ3_PIN_CACHE_DIRECT_MAPPED.md.
import fs from 'node:fs';
import assert from 'node:assert/strict';

const dir = 'docs/perf/dz3/';
const nl = new RegExp('\\r?\\n');
const rd = (n) => fs.readFileSync(dir + n, 'utf8');
const jl = (n) => rd(n).split(nl).filter(Boolean).map((l) => JSON.parse(l));
const f1 = (v) => v.toFixed(1);
const f2 = (v) => v.toFixed(2);
const f3 = (v) => v.toFixed(3);
const med = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; };
const md = [], csv = ['table,case,metric,a,b,ratio_a_over_b'];
const add = (t, c, m, a, b) => csv.push([t, c, m, a, b, f3(a / b)].join(','));
const MS = [4, 5, 8, 16, 64], ORDERS = ['seq', 'cyclic', 'random'];

// ---- A. final change against base and mimalloc ----
const T = jl('final_time.txt');
const nsPer = (arm, m, o) => { const r = T.filter((x) => x.arm === arm && x.m === m && x.order === o); assert.equal(r.length, 9, `${arm} ${m} ${o}: 9 processes`); return med(r.map((x) => x.free_ns)) / r[0].n; };
const C = rd('final_counters.txt').split(nl).filter(Boolean).map((l) => { const i = l.indexOf(' '); return { arm: l.slice(0, i), ...JSON.parse(l.slice(i + 1)) }; });
const hitRate = (arm, m, o) => { const r = C.filter((x) => x.arm === arm && x.m === m && x.order === o); assert.equal(r.length, 2, 'two counting runs'); assert.equal(r[0].pin_hits, r[1].pin_hits, 'counters are deterministic'); assert.equal(r[0].pin_hits + r[0].pin_misses, r[0].n, 'hits + misses = frees'); return r[0].pin_hits / r[0].n; };
const IR = {};
for (const l of rd('final_ir.txt').split(nl).filter(Boolean)) { const m = l.match(/^(\w+) (\d+) (\w+) total=(\d+)/); IR[`${m[1]} ${m[2]} ${m[3]}`] = Number(m[4]); }
const extraIr = (arm, m, o) => (IR[`${arm} ${m} ${o}`] - IR[`${arm} ${m} seq`]) / (m * 2048);
md.push('### A. Foreign free of m x 2048 blocks of 1 KiB by one other thread (Linux; time: medians of 9 processes; hit rate: counting build; Ir: callgrind)', '',
  '| m | order | pin hit rate base / new | extra Ir per free vs seq, base / new | ns per free base | new | mimalloc | new / base | new / mimalloc |', '|---:|---|---|---|---:|---:|---:|---:|---:|');
for (const m of MS) for (const o of ORDERS) {
  const b = nsPer('base', m, o), n = nsPer('new', m, o), x = nsPer('mi', m, o);
  const ex = o === 'seq' ? '-' : `${Math.round(extraIr('base', m, o))} / ${Math.round(extraIr('new', m, o))}`;
  md.push(`| ${m} | ${o} | ${f2(hitRate('base', m, o))} / ${f2(hitRate('new', m, o))} | ${ex} | ${f1(b)} | ${f1(n)} | ${f1(x)} | ${f2(n / b)} | ${f2(n / x)} |`);
  add('final_time_ns_per_free_new_over_base', `${m} ${o}`, 'ns', n, b);
  add('final_time_ns_per_free_new_over_mi', `${m} ${o}`, 'ns', n, x);
}
md.push('');

// ---- B. design iterations: each prototype against the base arm of ITS OWN run ----
const iter = (file, key, baseKey, variants) => {
  const R = jl(file);
  const ratio = (variant, m, o) => {
    const v = R.filter((x) => String(x[key]) === variant && x.m === m && x.order === o), b = R.filter((x) => String(x[key]) === baseKey && x.m === m && x.order === o);
    assert.equal(v.length, 9, `${file} ${variant} ${m} ${o}`); assert.equal(b.length, 9, `${file} base ${m} ${o}`);
    return med(v.map((x) => x.free_ns)) / med(b.map((x) => x.free_ns));
  };
  return Object.fromEntries(variants.map(([label, variant]) => [label, (m, o) => ratio(variant, m, o)]));
};
const V = {
  ...iter('iter_linear_time.txt', 'slots', '4', [['16 slots, linear search, round-robin', '16'], ['64 slots, linear search, round-robin', '64']]),
  ...iter('iter_memo_time.txt', 'arm', '4', [['16 slots, linear search + last-hit memo, incremental sweep', 'M']]),
  ...iter('iter_direct_time.txt', 'arm', '4', [['direct-mapped 16, incremental sweep', 'D16'], ['direct-mapped 64, incremental sweep', 'D64']]),
  ...iter('iter_direct32_time.txt', 'arm', '4', [['direct-mapped 32, incremental sweep (prototype of the change)', 'D32']]),
};
const POINTS = [[5, 'cyclic'], [16, 'cyclic'], [16, 'random'], [16, 'seq'], [64, 'cyclic'], [64, 'random']];
md.push('### B. Design iterations (scratch prototypes; ns per free of the variant / ns per free of the 4-slot base measured in the same run, 9 processes each)', '',
  `| variant | ${POINTS.map(([m, o]) => `m=${m} ${o}`).join(' | ')} |`, `|---|${POINTS.map(() => '---:').join('|')}|`);
for (const [label, fn] of Object.entries(V)) {
  md.push(`| ${label} | ${POINTS.map(([m, o]) => f2(fn(m, o))).join(' | ')} |`);
  for (const [m, o] of POINTS) csv.push(['iteration_ratio', label, `${m} ${o}`, f3(fn(m, o)), 1, f3(fn(m, o))].join(','));
}
md.push(`| change as measured (section A), same ratio | ${POINTS.map(([m, o]) => f2(nsPer('new', m, o) / nsPer('base', m, o))).join(' | ')} |`, '');

// ---- C. sweep budget ----
const S = jl('sweep_budget_time.txt');
const sn = (arm, m, o) => { const r = S.filter((x) => x.arm === arm && x.m === m && x.order === o); assert.equal(r.length, 9, `sweep ${arm} ${m} ${o}`); return med(r.map((x) => x.free_ns)) / r[0].n; };
const I4 = {}, I8 = {};
for (const l of rd('sweep_budget4_ir.txt').split(nl).filter(Boolean)) { const m = l.match(/^b4 (\d+) (\w+) total=(\d+)/); I4[`${m[1]} ${m[2]}`] = Number(m[3]); }
for (const l of rd('sweep_budget8_ir.txt').split(nl).filter(Boolean)) { const m = l.match(/^(\w+) (\d+) (\w+) pf=\S* total=(\d+)/); I8[`${m[1]} ${m[2]} ${m[3]}`] = Number(m[4]); }
md.push('### C. Sweep budget: occupied slots examined per sweep call, 8 against 4 (time: medians of 9 processes in one interleaved run; extra Ir per free vs seq from callgrind program totals)', '',
  '| m | order | ns per free base | budget 8 | budget 4 | budget 8 / base | budget 4 / base | extra Ir per free, budget 8 / budget 4 |', '|---:|---|---:|---:|---:|---:|---:|---|');
for (const m of [16, 64]) for (const o of ORDERS) {
  const b = sn('base', m, o), e8 = sn('new', m, o), e4 = sn('b4', m, o);
  const ex = o === 'seq' ? '-' : `${Math.round((I8[`new ${m} ${o}`] - I8[`new ${m} seq`]) / (m * 2048))} / ${Math.round((I4[`${m} ${o}`] - I4[`${m} seq`]) / (m * 2048))}`;
  md.push(`| ${m} | ${o} | ${f1(b)} | ${f1(e8)} | ${f1(e4)} | ${f2(e8 / b)} | ${f2(e4 / b)} | ${ex} |`);
  add('sweep_budget', `${m} ${o}`, 'budget4_over_base', e4, b);
}
md.push('');

// ---- D. multi-thread controls (cross-thread frees with a hit rate of about 1.0 before the change) ----
const MT = rd('final_mt.txt').split(nl).filter(Boolean).map((l) => { const m = l.match(/^(\w+) (\w+) (\w+) T=(\d+) ([\d.]+) Mops/); return { arm: m[1], w: m[2], t: Number(m[4]), v: Number(m[5]) }; });
md.push('### D. Multi-thread controls (g1_mt_one, 1000000 steps per thread, aggregate Mops/s, medians of 7 interleaved runs)', '',
  '| workload | threads | base (min-max) | change (min-max) | mimalloc | change / base | reading |', '|---|---:|---|---|---:|---:|---|');
for (const w of ['larson', 'mstress']) for (const t of [2, 4]) {
  const g = (a) => { const r = MT.filter((x) => x.arm === a && x.w === w && x.t === t).map((x) => x.v); assert.equal(r.length, 7, `${w} T=${t} ${a}`); return r; };
  const b = g('base'), n = g('new'), x = g('mi');
  const overlap = Math.min(...b) <= Math.max(...n) && Math.min(...n) <= Math.max(...b);
  md.push(`| ${w} | ${t} | ${f2(med(b))} (${f1(Math.min(...b))}-${f1(Math.max(...b))}) | ${f2(med(n))} (${f1(Math.min(...n))}-${f1(Math.max(...n))}) | ${f2(med(x))} | ${f3(med(n) / med(b))} | ${overlap ? 'ranges overlap: no difference readable' : 'ranges separate'} |`);
  add('mt_control', `${w} T=${t}`, 'mops_change_over_base', med(n), med(b));
}
md.push('');

fs.writeFileSync(dir + 'tables.md', md.join('\n'));
fs.writeFileSync('docs/perf/DZ3_PIN_CACHE_DIRECT_MAPPED_summary.csv', csv.join('\n') + '\n');
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  const pair = (m, o) => `${f1(nsPer('base', m, o))} -> ${f1(nsPer('new', m, o))} ns (mimalloc ${f1(nsPer('mi', m, o))})`;
  const subs = {
    CYC5: pair(5, 'cyclic'), CYC16: pair(16, 'cyclic'), RND16: pair(16, 'random'), RND8: pair(8, 'random'), SEQ16: pair(16, 'seq'),
    CYC64: pair(64, 'cyclic'), RND64: pair(64, 'random'),
    EX5: `${Math.round(extraIr('base', 5, 'cyclic'))} -> ${Math.round(extraIr('new', 5, 'cyclic'))}`,
    EX16R: `${Math.round(extraIr('base', 16, 'random'))} -> ${Math.round(extraIr('new', 16, 'random'))}`,
    HIT16R: `${f2(hitRate('base', 16, 'random'))} -> ${f2(hitRate('new', 16, 'random'))}`,
    HIT64R: `${f2(hitRate('base', 64, 'random'))} -> ${f2(hitRate('new', 64, 'random'))}`,
    CYC64R: f2(nsPer('new', 64, 'cyclic') / nsPer('base', 64, 'cyclic')),
    C64A: f2(nsPer('new', 64, 'cyclic') / nsPer('base', 64, 'cyclic')),
    C64C: f2(sn('b4', 64, 'cyclic') / sn('base', 64, 'cyclic')),
    C64DIFF: f2(Math.abs(nsPer('new', 64, 'cyclic') / nsPer('base', 64, 'cyclic') - sn('b4', 64, 'cyclic') / sn('base', 64, 'cyclic'))),
    SEQ_RANGE: (() => { const r = MS.map((m) => nsPer('new', m, 'seq') / nsPer('base', m, 'seq')); return `${f2(Math.min(...r))}-${f2(Math.max(...r))}`; })(),
    SEQ_IR: (() => { const r = MS.map((m) => Math.abs(IR[`new ${m} seq`] / IR[`base ${m} seq`] - 1) * 100); return `${f2(Math.max(...r))} % (largest over m, ${MS.map((m) => `${IR[`new ${m} seq`]} / ${IR[`base ${m} seq`]}`).join('; ')})`; })(),
    SLOW_RANGE: (() => { const r = []; for (const m of [5, 8, 16, 64]) for (const o of ['cyclic', 'random']) r.push(nsPer('base', m, o) / nsPer('mi', m, o)); return `${f1(Math.min(...r))}-${f1(Math.max(...r))}`; })(),
    SEQ_FAST: (() => { const r = MS.map((m) => nsPer('mi', m, 'seq') / nsPer('base', m, 'seq')); return `${f1(Math.min(...r))}-${f1(Math.max(...r))}`; })(),
  };
  for (const m of [4, 5, 8, 16]) for (const o of ORDERS) assert.ok(hitRate('new', m, o) >= 0.99, `hit rate >= 0.99 (cold misses only) at m=${m} ${o}`);
  let head = rd('doc_head.md'), tail = rd('doc_tail.md');
  for (const [k, v] of Object.entries(subs)) { head = head.split(`{{${k}}}`).join(v); tail = tail.split(`{{${k}}}`).join(v); }
  assert.ok(!/\{\{[A-Z_0-9]+\}\}/.test(head + tail), 'unfilled placeholder');
  fs.writeFileSync('docs/perf/DZ3_PIN_CACHE_DIRECT_MAPPED.md', head + md.join('\n') + '\n' + tail);
}
console.log(md.join('\n'));
