#!/usr/bin/env node
// CF-2 report generator: every number of docs/perf/CF2_ROUTE_PIN_CACHE_SLOTS.md comes from docs/perf/cf2/ through this
// script; ratios are asserted against the printed values and the per-arm configuration evidence is asserted.
// Input: counting-build lines of the g1_rhythm_probe `pincycle` mode (hits / misses, requested / resolved slots),
// callgrind program totals, time per foreign free (9 interleaved processes per cell, gzip).
// Output: docs/perf/cf2/tables.md, docs/perf/CF2_ROUTE_PIN_CACHE_SLOTS_summary.csv and, when doc_head.md / doc_tail.md
// exist, docs/perf/CF2_ROUTE_PIN_CACHE_SLOTS.md.
import fs from 'node:fs';
import zlib from 'node:zlib';
import assert from 'node:assert/strict';

const dir = 'docs/perf/cf2/';
const nl = new RegExp('\\r?\\n');
const rd = (n) => (n.endsWith('.gz') ? zlib.gunzipSync(fs.readFileSync(dir + n)).toString('utf8') : fs.readFileSync(dir + n, 'utf8'));
const f1 = (v) => v.toFixed(1);
const f2 = (v) => v.toFixed(2);
const f3 = (v) => v.toFixed(3);
const med = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; };
const md = [], csv = ['table,case,metric,a,b,ratio_a_over_b'];
const add = (t, c, m, a, b) => csv.push([t, c, m, a, b, f3(a / b)].join(','));
const SLOTS = ['4', '8', '16', '32', '64'], ARMS = ['base', ...SLOTS];
const MS = [4, 5, 8, 16, 32, 64], TM = [4, 8, 16, 32, 64], ORDERS = ['seq', 'cyclic', 'random'];

// ---- A. hit rate (counting builds) with per-arm configuration evidence ----
const C = rd('counters.txt').split(nl).filter(Boolean).map((l) => { const i = l.indexOf(' '); return { arm: l.slice(0, i), ...JSON.parse(l.slice(i + 1)) }; });
const cell = (arm, m, o) => {
  const r = C.filter((x) => x.arm === arm && x.m === m && x.order === o);
  assert.equal(r.length, 2, `${arm} ${m} ${o}: two counting runs`);
  assert.equal(r[0].pin_hits, r[1].pin_hits, 'hit counts are deterministic');
  assert.equal(r[0].pin_hits + r[0].pin_misses, r[0].n, 'hits + misses = frees');
  if (arm !== 'base') { assert.equal(r[0].slots_req, Number(arm), 'requested slots = arm'); assert.equal(r[0].slots_res, Number(arm), 'resolved slots read back from the producer thread = arm'); }
  return r[0];
};
const hit = (arm, m, o) => cell(arm, m, o).pin_hits / cell(arm, m, o).n;
md.push('### A. Pin cache hit rate = hits / frees (counting build; requested = resolved slots asserted per run)', '',
  `| m | order | base (main) | ${SLOTS.map((s) => `${s} slots`).join(' | ')} |`, `|---:|---|---:|${SLOTS.map(() => '---:').join('|')}|`);
for (const m of MS) for (const o of ORDERS) {
  md.push(`| ${m} | ${o} | ${f2(hit('base', m, o))} | ${SLOTS.map((s) => f2(hit(s, m, o))).join(' | ')} |`);
  for (const s of SLOTS) csv.push(['hit_rate', `${m} ${o}`, `${s} slots`, f3(hit(s, m, o)), f3(hit('base', m, o)), f3(hit(s, m, o) / hit('base', m, o))].join(','));
}
md.push('');

// ---- B. time per foreign free ----
const T = rd('time.txt.gz').split(nl).filter(Boolean).map((l) => JSON.parse(l));
const ns = (arm, m, o) => { const r = T.filter((x) => x.arm === arm && x.m === m && x.order === o); assert.equal(r.length, 9, `${arm} ${m} ${o}: 9 processes`); return med(r.map((x) => x.free_ns)) / r[0].n; };
md.push('### B. Time per foreign free in ns (medians of 9 interleaved processes) and the ratio to base and to mimalloc', '',
  `| m | order | base | ${SLOTS.map((s) => `${s} slots`).join(' | ')} | mimalloc |`, `|---:|---|---:|${SLOTS.map(() => '---:').join('|')}|---:|`);
for (const m of TM) for (const o of ORDERS) {
  md.push(`| ${m} | ${o} | ${f1(ns('base', m, o))} | ${SLOTS.map((s) => `${f1(ns(s, m, o))} (${f2(ns(s, m, o) / ns('base', m, o))})`).join(' | ')} | ${f1(ns('mi', m, o))} |`);
  for (const s of SLOTS) add('time_ns_per_free_over_base', `${m} ${o} ${s} slots`, 'ns', ns(s, m, o), ns('base', m, o));
}
md.push('', 'Values in parentheses: ns per free of the arm / ns per free of base (main, 32 slots, no mask).', '');

// ---- C. instructions (callgrind program totals) ----
const IR = {};
for (const l of rd('ir.txt').split(nl).filter(Boolean)) { const m = l.match(/^(\w+) (\d+) (\w+) total=(\d+)/); IR[`${m[1]} ${m[2]} ${m[3]}`] = Number(m[4]); }
const PER = (m) => m * 2048;
md.push('### C. Instructions (callgrind program totals; the set-up is identical across arms, so the difference is the cost of the frees)', '',
  `| m | order | base total | ${SLOTS.map((s) => `${s} slots: extra Ir per free vs base`).join(' | ')} |`, `|---:|---|---:|${SLOTS.map(() => '---:').join('|')}|`);
for (const m of [4, 16, 32, 64]) for (const o of ['seq', 'cyclic']) {
  const b = IR[`base ${m} ${o}`];
  md.push(`| ${m} | ${o} | ${b} | ${SLOTS.map((s) => f2((IR[`${s} ${m} ${o}`] - b) / PER(m))).join(' | ')} |`);
  for (const s of SLOTS) csv.push(['extra_ir_per_free', `${m} ${o}`, `${s} slots`, f2((IR[`${s} ${m} ${o}`] - b) / PER(m)), 0, ''].join(','));
}
md.push('');

// ---- E. design iteration 1: the sweep wrapped its cursor with a variable modulus ----
const rdI = (n) => (n.endsWith('.gz') ? zlib.gunzipSync(fs.readFileSync(dir + 'iter_modulus/' + n)).toString('utf8') : fs.readFileSync(dir + 'iter_modulus/' + n, 'utf8'));
const IR1 = {};
for (const l of rdI('ir.txt').split(nl).filter(Boolean)) { const m = l.match(new RegExp('^(\\w+) (\\d+) (\\w+) total=(\\d+)')); IR1[`${m[1]} ${m[2]} ${m[3]}`] = Number(m[4]); }
const sweepIr = (a) => { const m = rdI(`fn_${a}.txt`).split(nl).find((l) => l.includes('sweep_dead_slots')).match(new RegExp('^\\s*([\\d,]+)')); return Number(m[1].replace(/,/g, '')); };
const nsFrom = (file, arm, m, o) => { const r = rdI(file).split(nl).filter(Boolean).map((l) => JSON.parse(l)).filter((x) => x.arm === arm && x.m === m && x.order === o); assert.equal(r.length, 9, `${file} ${arm} ${m} ${o}`); return med(r.map((x) => x.free_ns)) / r[0].n; };
md.push('### E. Design iteration 1 (tree e5e1e9b0, superseded): the sweep wrapped its cursor with a variable modulus', '',
  'In the cells where every free misses, the 32-slot arm of iteration 1 (same hit rate as base) was slower than base. Instructions (callgrind) and time (medians of 9 processes; fixed arm order and rotated arm order):', '',
  '| m | order | extra Ir per free, 32 slots vs base | sweep_dead_slots Ir (m = 32 cyclic), base / 32 slots | ns per free 32 slots / base, fixed order | rotated order |', '|---:|---|---:|---|---:|---:|');
for (const [m, o] of [[32, 'cyclic'], [64, 'cyclic']]) {
  const ex = (IR1[`32 ${m} ${o}`] - IR1[`base ${m} ${o}`]) / PER(m);
  md.push(`| ${m} | ${o} | ${f1(ex)} | ${m === 32 ? `${sweepIr('base')} / ${sweepIr('32')}` : '-'} | ${f2(nsFrom('time_fixed_order.txt.gz', '32', m, o) / nsFrom('time_fixed_order.txt.gz', 'base', m, o))} | ${f2(nsFrom('time_rot.txt.gz', '32', m, o) / nsFrom('time_rot.txt.gz', 'base', m, o))} |`);
  add('iter1_extra_ir_per_free_32_vs_base', `${m} ${o}`, 'ir', IR1[`32 ${m} ${o}`] - IR1[`base ${m} ${o}`], PER(m));
}
md.push('', 'Cause: `self.sweep_cursor % slots` with a run-time `slots` compiles to a division and a bounds check inside the loop of `sweep_dead_slots`, which runs twice per miss. Fix: wrap with the mask and mask the index with the array capacity (section C and B are measured on the fixed tree).', '');

// ---- D. retention bound ----
const DESC_MIB = 0.29;
md.push('### D. Worst-case retention of dead route descriptors per idle producer thread (slots x ~0.29 MiB, the DZ-3 per-pin figure for a segment with every class in use)', '',
  '| slots | worst case per thread | worst case at 32 producer threads |', '|---:|---:|---:|');
for (const s of SLOTS) md.push(`| ${s} | ${f1(Number(s) * DESC_MIB)} MiB | ${f1(Number(s) * DESC_MIB * 32)} MiB |`);
md.push('');

fs.writeFileSync(dir + 'tables.md', md.join('\n'));
fs.writeFileSync('docs/perf/CF2_ROUTE_PIN_CACHE_SLOTS_summary.csv', csv.join('\n') + '\n');
if (fs.existsSync(dir + 'doc_head.md') && fs.existsSync(dir + 'doc_tail.md')) {
  const slotsAtLeast = (m, o) => SLOTS.filter((s) => hit(s, m, o) >= 0.9).join('/');
  const seqIr = (s, m) => f2((IR[`${s} ${m} seq`] - IR[`base ${m} seq`]) / PER(m));
  const seqPct = (s, m) => f2((IR[`${s} ${m} seq`] / IR[`base ${m} seq`] - 1) * 100);
  const subs = {
    TREE_ID: 'git tree `' + rd('ident.txt').split(nl)[0].replace(/^tree /, '').split(' ')[0] + '` (branch `g1-cf2`)',
    HIT16C: SLOTS.map((s) => `${s}: ${f2(hit(s, 16, 'cyclic'))}`).join(', '),
    HIT32C: SLOTS.map((s) => `${s}: ${f2(hit(s, 32, 'cyclic'))}`).join(', '),
    HIT64C: SLOTS.map((s) => `${s}: ${f2(hit(s, 64, 'cyclic'))}`).join(', '),
    OK16C: slotsAtLeast(16, 'cyclic'), OK32C: slotsAtLeast(32, 'cyclic'),
    SEQ32: `${seqIr('32', 16)} Ir per free (${seqPct('32', 16)} %)`,
    SEQ64: `${seqIr('64', 16)} Ir per free (${seqPct('64', 16)} %)`,
    SEQ4: `${seqIr('4', 16)} Ir per free (${seqPct('4', 16)} %)`,
    NS16C: SLOTS.map((s) => `${s}: ${f1(ns(s, 16, 'cyclic'))}`).join(', '),
    NS16C_BASE: f1(ns('base', 16, 'cyclic')), NS16C_MI: f1(ns('mi', 16, 'cyclic')),
    NS32C: SLOTS.map((s) => `${s}: ${f1(ns(s, 32, 'cyclic'))}`).join(', '),
    NS32C_BASE: f1(ns('base', 32, 'cyclic')), NS32C_MI: f1(ns('mi', 32, 'cyclic')),
    MISS_IR: (() => { const r = []; for (const s of SLOTS) for (const m of [4, 16]) if (hit(s, m, 'cyclic') === 0 && hit('base', m, 'cyclic') >= 0.99) r.push((IR[`${s} ${m} cyclic`] - IR[`base ${m} cyclic`]) / PER(m)); assert.ok(r.length > 0); return `${Math.round(Math.min(...r))}-${Math.round(Math.max(...r))}`; })(),
    MISS_NS: (() => { const r = []; for (const s of SLOTS) for (const m of TM) for (const o of ORDERS) if (hit(s, m, o) === 0 && hit('base', m, o) >= 0.99) r.push(ns(s, m, o) / ns('base', m, o)); assert.ok(r.length > 0); return `${f1(Math.min(...r))}-${f1(Math.max(...r))}`; })(),
    NS32R: SLOTS.map((s) => `${s}: ${f1(ns(s, 32, 'random'))}`).join(', '),
    NS32R_BASE: f1(ns('base', 32, 'random')), NS32R_MI: f1(ns('mi', 32, 'random')),
    NS64C: SLOTS.map((s) => `${s}: ${f1(ns(s, 64, 'cyclic'))}`).join(', '),
    NS64C_BASE: f1(ns('base', 64, 'cyclic')), NS64C_MI: f1(ns('mi', 64, 'cyclic')),
    MISS32_NS: (() => { const r = []; for (const m of [32, 64]) for (const o of ['cyclic', 'random']) if (hit('32', m, o) < 0.99) r.push(ns('32', m, o) / ns('base', m, o)); assert.ok(r.length > 0); return `${f2(Math.min(...r))}-${f2(Math.max(...r))}`; })(),
    SEQ_NS: (() => { const r = []; for (const s of SLOTS) for (const m of TM) r.push(ns(s, m, 'seq') / ns('base', m, 'seq')); return `${f2(Math.min(...r))}-${f2(Math.max(...r))}`; })(),
    SEQ_NS32: (() => { const r = TM.map((m) => ns('32', m, 'seq') / ns('base', m, 'seq')); return `${f2(Math.min(...r))}-${f2(Math.max(...r))}`; })(),
  };
  // the 32-slot arm must reproduce the base's hit pattern (same slots, same mapping), up to the cold sweep differences
  for (const m of MS) for (const o of ORDERS) assert.ok(Math.abs(hit('32', m, o) - hit('base', m, o)) < 0.01, `32 slots = base hit rate at m=${m} ${o}`);
  let head = rd('doc_head.md'), tail = rd('doc_tail.md');
  for (const [k, v] of Object.entries(subs)) { head = head.split(`{{${k}}}`).join(v); tail = tail.split(`{{${k}}}`).join(v); }
  assert.ok(!/\{\{[A-Z_0-9]+\}\}/.test(head + tail), 'unfilled placeholder');
  fs.writeFileSync('docs/perf/CF2_ROUTE_PIN_CACHE_SLOTS.md', head + md.join('\n') + '\n' + tail);
}
console.log(md.join('\n'));
