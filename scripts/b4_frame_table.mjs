#!/usr/bin/env node
// b4_frame_table.mjs — B4 "frame tax" chunk report tables.
// Reads raw iai-callgrind logs docs/perf/_raw_b4_frame_*.log and prints
// markdown tables for B4_FRAME_TAX.md §3 (accepted chain) and §4 (NO-GO).
// No cargo, no external deps. Run from repo root: node scripts/b4_frame_table.mjs
import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const PERF = 'docs/perf';

function readLog(name) {
  try {
    return readFileSync(join(PERF, name), 'utf8');
  } catch {
    return null;
  }
}

const num = (m) => (m ? Number(m[1]) : undefined);

// Header shapes seen in the raw logs:
//   == sefer w_alloc marginal Ir/op: 63                                  (ao / thrash)
//   == sefer w_free marginal Ir/op: 115                                  (ao / thrash)
//   == sefer w_alloc marginal Ir/call (256/round): 136                   (batch probe)
//   == sefer w_alloc marginal Ir/call (258/round): 133                   (batch-large probe)
//   == sefer w_alloc16 marginal Ir/op: 26 | cache: ... EstCycles/op=26.0 (const mode)
//   == sefer w_alloc cache: D1m/op=0.05 LLm/op=0.06 EstCycles/op=62.0    (ao cache events)
//   MARGINAL Ir/op (sefer larson T=1): 319  /  (...mstress...)  + PROGRAM TOTALS
function parseLog(name) {
  const txt = readLog(name);
  if (txt === null) return null;
  const d = {};
  const alloc = txt.match(/^== sefer w_alloc marginal Ir\/op:\s*([\d.]+)/m);
  const free = txt.match(/^== sefer w_free marginal Ir\/op:\s*([\d.]+)/m);
  if (alloc) d.ao_alloc = num(alloc);
  if (free) d.ao_free = num(free);
  const bA = txt.match(/^== sefer w_alloc marginal Ir\/call \(256\/round\):\s*([\d.]+)/m);
  const bF = txt.match(/^== sefer w_free marginal Ir\/call \(256\/round\):\s*([\d.]+)/m);
  if (bA) d.batch_alloc = num(bA);
  if (bF) d.batch_free = num(bF);
  const blA = txt.match(/^== sefer w_alloc marginal Ir\/call \(258\/round\):\s*([\d.]+)/m);
  const blF = txt.match(/^== sefer w_free marginal Ir\/call \(258\/round\):\s*([\d.]+)/m);
  if (blA) d.batchl_alloc = num(blA);
  if (blF) d.batchl_free = num(blF);
  // const mode (16/64-byte const variants; never the plain ao headers)
  const cA = txt.match(/^== sefer w_alloc16 marginal Ir\/op:\s*([\d.]+)/m);
  const cF = txt.match(/^== sefer w_free16 marginal Ir\/op:\s*([\d.]+)/m);
  if (cA) d.const_alloc = num(cA);
  if (cF) d.const_free = num(cF);
  // EstCycles: ao cache-event lines take precedence, const lines fill in
  const ac = txt.match(/^== sefer w_alloc cache:[^\n]*EstCycles\/op=([\d.]+)/m);
  const fc = txt.match(/^== sefer w_free cache:[^\n]*EstCycles\/op=([\d.]+)/m);
  const ac16 = txt.match(/^== sefer w_alloc16 marginal[^\n]*EstCycles\/op=([\d.]+)/m);
  const fc16 = txt.match(/^== sefer w_free16 marginal[^\n]*EstCycles\/op=([\d.]+)/m);
  if (ac16) d.const_alloc_cycles = num(ac16);
  if (fc16) d.const_free_cycles = num(fc16);
  if (ac) d.ao_alloc_cycles = num(ac);
  if (fc) d.ao_free_cycles = num(fc);
  // larson / mstress judges: exact value lives on the PROGRAM TOTALS line
  const pt = txt.match(/MARGINAL Ir\/op \(sefer (larson|mstress) T=1\):\s*\d+\n[\s\S]{0,400}?([\d.]+)\tPROGRAM TOTALS/);
  if (pt) d[pt[1]] = Number(pt[2]);
  else {
    const lar = txt.match(/MARGINAL Ir\/op \(sefer larson T=1\):\s*(\d+)/);
    const mst = txt.match(/MARGINAL Ir\/op \(sefer mstress T=1\):\s*(\d+)/);
    if (lar) d.larson = num(lar);
    if (mst) d.mstress = num(mst);
  }
  return d;
}

function merge(...names) {
  const out = {};
  for (const n of names.filter(Boolean)) {
    const d = parseLog(n);
    if (d) Object.assign(out, d);
  }
  return out;
}

function fmt(v, dash = '—') {
  if (v === undefined || Number.isNaN(v)) return dash;
  return String(v);
}

const pair = (a, b) => `${fmt(a)}/${fmt(b)}`;

// ---- accepted chain: [ao, batch, batchl, const, thrash, larson, mstress] ----
const CHAIN = [
  { step: 'база main f93c1b52', sha: '(пустой diff)', verdict: 'база',
    files: ['_raw_b4_frame_split-base.log', '_raw_b4_frame_split-base-b2.log', '_raw_b4_frame_split-base-bl.log', '_raw_b4_frame_split-base-const.log', '_raw_b4_frame_split-base-th.log', '_raw_b4_frame_split-base-l.log', '_raw_b4_frame_split-base-m.log'] },
  { step: 'N1-alloc (split-a1)', sha: '2992e8c8…', verdict: 'GO',
    files: ['_raw_b4_frame_split-a1.log', '_raw_b4_frame_split-a1-batch.log', null, null, null, '_raw_b4_frame_split-a1-larson.log', '_raw_b4_frame_split-a1-mstress.log'] },
  { step: 'N8 n8b+n8c (split-n8-fin)', sha: 'd061cdbc…', verdict: 'GO',
    files: ['_raw_b4_frame_split-n8c-ao.log', '_raw_b4_frame_split-n8c.log', null, null, null, '_raw_b4_frame_split-base-n8fin-l.log', '_raw_b4_frame_split-base-n8fin-m.log'] },
  { step: 'N9 (split-n9c/n9d)', sha: '639e44c3…', verdict: 'GO',
    files: ['_raw_b4_frame_split-n9c.log', '_raw_b4_frame_split-n9c.batch.log', '_raw_b4_frame_split-n9c.batchl.log', '_raw_b4_frame_split-n9c.const.log', '_raw_b4_frame_split-n9c-th.log', '_raw_b4_frame_split-n9c.larson.log', '_raw_b4_frame_split-n9c.mstress.log'] },
  { step: 'N4 (split-n4, итог)', sha: 'a0daba8c…', verdict: 'финал',
    files: ['_raw_b4_frame_split-n4.log', '_raw_b4_frame_split-n4-b.log', '_raw_b4_frame_split-n4-bl.log', '_raw_b4_frame_split-n4-c.log', '_raw_b4_frame_split-n4-th.log', '_raw_b4_frame_split-n4-l.log', '_raw_b4_frame_split-n4-m.log'] },
];

// ---- NO-GO: same file slots; `-batch` logs of n1a..d hold per-event totals,
// not Ir/call, so they are parsed but never shown as ao/batch columns. ----
const NOGO = [
  { step: 'N1a (cnt-перенос)', why: 'larson 316 — регресс против a1 311.4; перенос счётчика не окупается',
    files: ['_raw_b4_frame_split-n1a.log', null, null, null, null, '_raw_b4_frame_split-n1a-larson.log', null] },
  { step: 'N1b (хвосты)', why: 'w_free 110 (−5), fn w_free 10.9, но larson 322 — регресс',
    files: ['_raw_b4_frame_split-n1b.log', null, null, null, null, '_raw_b4_frame_split-n1b-larson.log', null] },
  { step: 'N1c (контекст хвостам)', why: 'w_free 111; larson 320 — регресс',
    files: ['_raw_b4_frame_split-n1c.log', null, null, null, null, '_raw_b4_frame_split-n1c-larson.log', null] },
  { step: 'N1d (counterfactual)', why: 'w_free 110 ≈ n1c 111 → контекст не при чём (c≈d); larson 320',
    files: ['_raw_b4_frame_split-n1d.log', null, null, null, null, '_raw_b4_frame_split-n1d-larson.log', null] },
  { step: 'N2 (inline free-хвоста)', why: 'fn w_free 15.0 не сдвинулся — кадр держат живые значения guard-цепи (~8-9 значений) + TLS-диспетчеризация, не вызовы; batch free 176',
    files: ['_raw_b4_frame_split-n2.log', '_raw_b4_frame_split-n2-batch.log', null, null, null, '_raw_b4_frame_split-n2-larson.log', null] },
  { step: 'N3 (root-only Tier-1)', why: '+1 Ir/free стабильно (116; n3b 116); const free 75; batch −1 не компенсирует',
    files: ['_raw_b4_frame_split-n3.log', '_raw_b4_frame_split-n3.batch.log', null, '_raw_b4_frame_split-n3.const.log', null, '_raw_b4_frame_split-n3-bl.log', null] },
  { step: 'N6 (enum ClassIdx)', why: '−2/−2 Ir (61/113), но пороги не достигнуты: w_free 113 > 112, cnt-строки 5.0/4.0 > 3; range-метаданные сворачиваются лишь частично',
    files: ['_raw_b4_frame_split-n6.log', '_raw_b4_frame_split-n6-batch.log', null, null, null, '_raw_b4_frame_split-n6-larson.log', null] },
  { step: 'N7 (refill до середины)', why: 'активация −52 %/−38 %, ao 50/101 (−5/−4), larson 293 (−9), НО batch alloc 141 (+15), batchl alloc 146 (+15) → NO-GO; пин r6_t39 не тронут',
    files: ['_raw_b4_frame_split-n7-ao.log', '_raw_b4_frame_split-n7-batch.log', '_raw_b4_frame_split-n7-batchl.log', '_raw_b4_frame_split-n7-const.log', '_raw_b4_frame_split-n7-thrash.log', '_raw_b4_frame_split-n7-larson.log', '_raw_b4_frame_split-n7-mstress.log'] },
  { step: 'N7b (refill, вариант b)', why: 'ao 52/102, larson 297 (−5), mstress 422 (−72), НО batch alloc 131 (+5), batchl alloc 137 (+11) → NO-GO',
    files: ['_raw_b4_frame_split-n7b-ao.log', '_raw_b4_frame_split-n7b-batch.log', '_raw_b4_frame_split-n7b-batchl.log', '_raw_b4_frame_split-n7b-const.log', '_raw_b4_frame_split-n7b-thrash.log', '_raw_b4_frame_split-n7b-larson.log', '_raw_b4_frame_split-n7b-mstress.log'] },
  { step: 'N8a (слияние поиска runs)', why: '+90 Ir/событие (codegen): ao free 116, batch free 175',
    files: ['_raw_b4_frame_split-n8a-ao.log', '_raw_b4_frame_split-n8a.log', null, null, null, null, null] },
  { step: 'N8d (маска)', why: '−6.6 Ir/событие < порога 8',
    files: [null, '_raw_b4_frame_split-n8d.log', null, null, null, null, null] },
  { step: 'N5 (LUT класс+divides)', why: '+8/+9 Ir ao (free 113, EstCycles 118.2); const-режим сворачивает полностью (расхождения режимов нет)',
    files: ['_raw_b4_frame_split-n5.log', '_raw_b4_frame_split-n5-b.log', null, '_raw_b4_frame_split-n5-c.log', null, null, null] },
  { step: 'N5b (counterfactual)', why: 'free 114 ≈ n5 113 → counterfactual n5≡n5b: совместная загрузка divisibility не подтвердилась',
    files: ['_raw_b4_frame_split-n5b.log', '_raw_b4_frame_split-n5b-b.log', null, '_raw_b4_frame_split-n5b-c.log', null, null, null] },
];

function rowOf(c) {
  const [ao, batch, batchl, cst, thr, lar, mst] = c.files;
  const d = merge(ao, batch, cst, lar, mst); // NOTE: thrash/batchl parsed separately to avoid header collisions
  return {
    ao: pair(d.ao_alloc, d.ao_free),
    cyc: pair(d.ao_alloc_cycles, d.ao_free_cycles),
    batch: pair(d.batch_alloc, d.batch_free),
    cst: pair(d.const_alloc, d.const_free),
    batchl: batchl ? pair(parseLog(batchl)?.batchl_alloc, parseLog(batchl)?.batchl_free) : '—',
    thrash: thr ? pair(parseLog(thr)?.ao_alloc, parseLog(thr)?.ao_free) : '—',
    larson: fmt(d.larson),
    mstress: fmt(d.mstress),
  };
}

function chainTable() {
  const lines = [];
  lines.push('### 3.1 Принятая цепочка (все числа ИЗМЕРЕНО — извлечены скриптом из `docs/perf/_raw_b4_frame_*.log`)');
  lines.push('');
  lines.push('| Шаг | diff-sha | ao a/f Ir | ao a/f EstCycles | batch a/f Ir/call | const a/f | batchl a/f | thrash a/f | larson | mstress |');
  lines.push('|---|---|---|---|---|---|---|---|---|---|');
  for (const c of CHAIN) {
    const r = rowOf(c);
    lines.push(`| ${c.step} | ${c.sha} | ${r.ao} | ${r.cyc} | ${r.batch} | ${r.cst} | ${r.batchl} | ${r.thrash} | ${r.larson} | ${r.mstress} |`);
  }
  lines.push('');
  return lines.join('\n');
}

function nogoTable() {
  const lines = [];
  lines.push('### 4.1 NO-GO-сводка (все числа ИЗМЕРЕНО — извлечены скриптом из тех же сырых логов)');
  lines.push('');
  lines.push('| Шаг | ao a/f Ir | batch a/f | const a/f | batchl a/f | larson | mstress | причина NO-GO |');
  lines.push('|---|---|---|---|---|---|---|---|');
  for (const c of NOGO) {
    const r = rowOf(c);
    lines.push(`| ${c.step} | ${r.ao} | ${r.batch} | ${r.cst} | ${r.batchl} | ${r.larson} | ${r.mstress} | ${c.why} |`);
  }
  lines.push('');
  return lines.join('\n');
}

function deltaTable() {
  const base = merge(...CHAIN[0].files.filter((f) => !f.includes('-th.')));
  const fin = merge(...CHAIN[4].files.filter((f) => !f.includes('-th.')));
  const pct = (b, f) => (b !== undefined && f !== undefined ? ` (${(((f - b) / b) * 100).toFixed(1)} %)` : '');
  const rows = [
    ['ao alloc Ir/op', base.ao_alloc, fin.ao_alloc],
    ['ao free Ir/op', base.ao_free, fin.ao_free],
    ['ao alloc EstCycles/op', base.ao_alloc_cycles, fin.ao_alloc_cycles],
    ['ao free EstCycles/op', base.ao_free_cycles, fin.ao_free_cycles],
    ['batch alloc Ir/call', base.batch_alloc, fin.batch_alloc],
    ['batch free Ir/call', base.batch_free, fin.batch_free],
    ['const alloc Ir/op', base.const_alloc, fin.const_alloc],
    ['const free Ir/op', base.const_free, fin.const_free],
    ['batchl alloc Ir/call', base.batchl_alloc, fin.batchl_alloc],
    ['batchl free Ir/call', base.batchl_free, fin.batchl_free],
    ['thrash alloc Ir/op', parseLog('_raw_b4_frame_split-base-th.log')?.ao_alloc, parseLog('_raw_b4_frame_split-n4-th.log')?.ao_alloc],
    ['thrash free Ir/op', parseLog('_raw_b4_frame_split-base-th.log')?.ao_free, parseLog('_raw_b4_frame_split-n4-th.log')?.ao_free],
    ['larson Ir/op', base.larson, fin.larson],
    ['mstress Ir/op', base.mstress, fin.mstress],
  ];
  const lines = ['### 3.0 Итоговые Δ (база main f93c1b52 → N4-финал), ИЗМЕРЕНО', '', '| Метрика | база | финал | Δ |', '|---|---|---|---|'];
  for (const [n, b, f] of rows) {
    const d = b !== undefined && f !== undefined ? `${Number((f - b).toFixed(1))}${pct(b, f)}` : '—';
    lines.push(`| ${n} | ${fmt(b)} | ${fmt(f)} | ${d} |`);
  }
  lines.push('');
  return lines.join('\n');
}

function appendix() {
  const files = readdirSync(PERF).filter((f) => f.startsWith('_raw_b4_frame_')).sort();
  const logs = files.filter((f) => f.endsWith('.log'));
  const states = files.filter((f) => f.endsWith('.state'));
  const lines = ['### 8. Приложение: сырые артефакты', '', `Логи (${logs.length}):`, ''];
  for (const f of logs) lines.push(`- \`docs/perf/${f}\``);
  lines.push('', `State-файлы (${states.length}; фиксируют diff-sha256 состояния дерева на момент каждого замера):`, '');
  for (const f of states) lines.push(`- \`docs/perf/${f}\``);
  return lines.join('\n');
}

console.log('<!-- Сгенерировано scripts/b4_frame_table.mjs из docs/perf/_raw_b4_frame_*.log; не редактировать вручную -->');
console.log();
console.log(deltaTable());
console.log(chainTable());
console.log(nogoTable());
console.log(appendix());
