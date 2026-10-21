#!/usr/bin/env node
// B4-31 N9 v3 — exact receipts: CI iai A/B (non-regression) + local exact
// thrash64 (target) for docs/perf/B4_N9V3_EXACT.md. Reads only committed raw
// files; no cargo. Usage: node scripts/b4_n9v3_table.mjs [--csv]
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';

const IAI = 'docs/perf/_raw_b4_n9v3_iai_ab.csv';
const THRASH = 'docs/perf/_raw_b4_n9v3_thrash64.log';
// Pre-registered (OPEN_ITEMS item 102 (а), docs/perf/B4_MICRO_PACK.md §0.1):
const TARGET_FREE_DELTA = -2; // thrash64 free Ir/op, exact
const MAX_CELL_REGRESSION_PCT = 0.2; // every iai cell with A > 100 Ir
const MIN_ACTIVE_SEGMENTS = 64; // thrash64 activation oracle

// --- CI iai A/B ---
const lines = readFileSync(IAI, 'utf8').trim().split(/\r?\n/);
const head = lines[0].split(',');
const ix = (n) => head.indexOf(n);
const cells = lines.slice(1).map((l) => l.split(',')).map((r) => ({
  cell: r[ix('cell')], a: Number(r[ix('A_Ir')]), b: Number(r[ix('B_Ir')]),
  shaA: r[ix('sha_a')], shaB: r[ix('sha_b')], features: r[ix('features')],
}));
const mimalloc = cells.filter((c) => c.cell.startsWith('mimalloc_'));
assert.ok(mimalloc.length > 0, 'mimalloc control cells missing');
assert.ok(mimalloc.every((c) => c.a === c.b), 'mimalloc control moved: run invalid');
const sefer = cells.filter((c) => !c.cell.startsWith('mimalloc_') && c.a > 100);
for (const c of sefer) c.pct = ((c.b - c.a) / c.a) * 100;
const better = sefer.filter((c) => c.b < c.a);
const worse = sefer.filter((c) => c.b > c.a);
const same = sefer.length - better.length - worse.length;
assert.equal(better.length + worse.length + same, sefer.length);
const maxWorse = worse.reduce((m, c) => (c.pct > m.pct ? c : m), { pct: 0, cell: '—', a: 0, b: 0 });
const constPlus6 = worse.filter((c) => c.b - c.a === 6).length;

// --- local exact thrash64 ---
const t = readFileSync(THRASH, 'utf8');
const idA = t.match(/^identity A: ([0-9a-f]{40}) diff=([0-9a-f]{64})/m);
const idB = t.match(/^identity B: ([0-9a-f]{40}) diff=([0-9a-f]{64})/m);
assert.ok(idA && idB, 'identity lines missing');
assert.equal(idA[1], cells[0].shaA, 'thrash64 A arm is not the iai A SHA');
assert.equal(idB[1], cells[0].shaB, 'thrash64 B arm is not the iai B SHA');
const EMPTY_DIFF = 'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855';
assert.equal(idA[2], EMPTY_DIFF, 'A arm measured with a dirty tree');
assert.equal(idB[2], EMPTY_DIFF, 'B arm measured with a dirty tree');
const act = [...t.matchAll(/distinct 16B-window segments: (\d+)/g)].map((m) => Number(m[1]));
assert.equal(act.length, 2, 'one activation line per arm');
assert.ok(act.every((n) => n >= MIN_ACTIVE_SEGMENTS), 'thrash64 did not reach the miss-heavy regime');
const exact = {};
for (const m of t.matchAll(/^EXACT (\S+) (w_alloc|w_free) Ir\/op = ([\d.]+) \(TOTALS (\d+) -> (\d+)\)/gm)) {
  const v = Number(m[3]);
  assert.ok(Math.abs((Number(m[5]) - Number(m[4])) / 50000 - v) < 1e-4, `${m[1]} ${m[2]}: TOTALS arithmetic`);
  exact[`${m[1]}:${m[2]}`] = v;
}
for (const k of ['n9x-a:w_alloc', 'n9x-a:w_free', 'n9x-b:w_alloc', 'n9x-b:w_free']) assert.ok(k in exact, `missing ${k}`);
const cyc = [...t.matchAll(/== sefer w_free cache: .*EstCycles\/op=([\d.]+)/g)].map((m) => Number(m[1]));
assert.equal(cyc.length, 2, 'one free EstCycles line per arm');
const dFree = exact['n9x-b:w_free'] - exact['n9x-a:w_free'];
const dAlloc = exact['n9x-b:w_alloc'] - exact['n9x-a:w_alloc'];

const targetMet = dFree <= TARGET_FREE_DELTA;
const nonRegression = maxWorse.pct <= MAX_CELL_REGRESSION_PCT && dAlloc <= 0;
const verdict = targetMet && nonRegression ? 'GO' : 'NO-GO';

if (process.argv.includes('--csv')) {
  console.log('kind,key,a,b,delta,delta_pct,sha_a,sha_b,features');
  console.log(`thrash64_exact,free Ir/op,${exact['n9x-a:w_free']},${exact['n9x-b:w_free']},${dFree.toFixed(4)},,${idA[1]},${idB[1]},production`);
  console.log(`thrash64_exact,alloc Ir/op,${exact['n9x-a:w_alloc']},${exact['n9x-b:w_alloc']},${dAlloc.toFixed(4)},,${idA[1]},${idB[1]},production`);
  console.log(`thrash64_cache,free EstCycles/op,${cyc[0]},${cyc[1]},${(cyc[1] - cyc[0]).toFixed(1)},,${idA[1]},${idB[1]},production`);
  for (const c of sefer) console.log(`iai,${c.cell},${c.a},${c.b},${c.b - c.a},${c.pct.toFixed(4)},${c.shaA},${c.shaB},${c.features}`);
  process.exit(0);
}

const f4 = (x) => x.toFixed(4);
console.log(`A = \`${idA[1].slice(0, 8)}\` (main), B = \`${idB[1].slice(0, 8)}\` (N9 v3); both trees clean (diff sha256 = empty).`);
console.log('');
console.log('Target — thrash64, local callgrind, exact (TOTALS(100k) − TOTALS(50k)) / 50k, `--features production`:');
console.log('');
console.log('| metric | A | B | Δ | pre-registered | result |');
console.log('|---|---:|---:|---:|---|---|');
console.log(`| free Ir/op | ${f4(exact['n9x-a:w_free'])} | ${f4(exact['n9x-b:w_free'])} | ${f4(dFree)} | ≤ ${TARGET_FREE_DELTA} | ${targetMet ? 'met' : 'NOT met'} |`);
console.log(`| alloc Ir/op | ${f4(exact['n9x-a:w_alloc'])} | ${f4(exact['n9x-b:w_alloc'])} | ${f4(dAlloc)} | ≤ 0 | ${dAlloc <= 0 ? 'ok' : 'FAIL'} |`);
console.log(`| free EstCycles/op | ${cyc[0]} | ${cyc[1]} | ${(cyc[1] - cyc[0]).toFixed(1)} | — | — |`);
console.log('');
console.log(`Activation oracle: distinct 16 B-window segments ${act.join(' / ')} (≥ ${MIN_ACTIVE_SEGMENTS} in both arms).`);
console.log('');
console.log(`Non-regression — CI iai A/B (\`${cells[0].features}\`): of ${sefer.length} cells with A > 100 Ir, ${better.length} better, ${worse.length} worse (${constPlus6} of them exactly +6 Ir), ${same} equal; mimalloc controls: ${mimalloc.length}, all Δ = 0.`);
console.log(`Largest regression: \`${maxWorse.cell}\` ${maxWorse.a} → ${maxWorse.b} (${maxWorse.pct.toFixed(3)} %), threshold ≤ ${MAX_CELL_REGRESSION_PCT} %.`);
console.log('');
console.log('| cell (largest gains) | A Ir | B Ir | Δ | Δ % |');
console.log('|---|---:|---:|---:|---:|');
for (const c of [...better].sort((x, y) => x.pct - y.pct).slice(0, 8)) console.log(`| \`${c.cell}\` | ${c.a} | ${c.b} | ${c.b - c.a} | ${c.pct.toFixed(3)} % |`);
console.log('');
console.log(`**Verdict N9 v3: ${verdict}** (target ${targetMet ? 'met' : 'not met'}; non-regression ${nonRegression ? 'held' : 'violated'}).`);
