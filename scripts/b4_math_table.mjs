#!/usr/bin/env node
// B4 math-hacks: сводная таблица из сырых callgrind-логов docs/perf/_raw_b4_math_*.log
import { readFileSync, existsSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const perfDir = join(root, 'docs', 'perf');

const STEPS = [
  ['math-base', 'база'],
  ['math-h1', 'NO-GO: откат'],
  ['math-h4', 'NO-GO: откат'],
  ['math-h5b', 'NO-GO: откат'],
  ['math-h5a', 'NO-GO: откат'],
  ['math-h2', 'NO-GO: откат'],
  ['math-h6', 'NO-GO: откат'],
];

function readNumber(file, re, what) {
  const path = join(perfDir, file);
  if (!existsSync(path)) {
    console.error(`ОШИБКА: нет лога ${path}`);
    process.exit(1);
  }
  const m = readFileSync(path, 'utf8').match(re);
  if (!m) {
    console.error(`ОШИБКА: в ${path} не найдена строка «${what}»`);
    process.exit(1);
  }
  return Number(m[1]);
}

function aoPair(label) {
  const file = `_raw_b4_math_${label}.log`;
  const wAlloc = readNumber(file, /w_alloc marginal Ir\/op:\s*(\d+)/, 'w_alloc');
  const wFree = readNumber(file, /w_free marginal Ir\/op:\s*(\d+)/, 'w_free');
  return [wAlloc, wFree];
}

function margOne(label, suffix, what) {
  return readNumber(
    `_raw_b4_math_${label}${suffix}.log`,
    new RegExp(`MARGINAL Ir\\/op \\(sefer ${what} T=1\\):\\s*(\\d+)`),
    what,
  );
}

const rows = STEPS.map(([label, verdict]) => {
  const [wAlloc, wFree] = aoPair(label);
  const larson = margOne(label, '-l', 'larson');
  const mstress = margOne(label, '-m', 'mstress');
  return { label, verdict, wAlloc, wFree, larson, mstress };
});

const hdr = ['шаг', 'w_alloc', 'w_free', 'larson', 'mstress', 'Δ w_alloc', 'Δ w_free', 'Δ larson', 'Δ mstress', 'вердикт'];
let prev;
for (const r of rows) {
  r.prev = prev;
  
  prev = r;
}
const widths = hdr.map((h, i) => Math.max(h.length, ...rows.map((r) => rowCell(r, i).length)));

function rowCell(r, i) {
  switch (i) {
    case 0: return r.label;
    case 1: return String(r.wAlloc);
    case 2: return String(r.wFree);
    case 3: return String(r.larson);
    case 4: return String(r.mstress);
    case 5: return r.prev === undefined ? '—' : fmtDelta(r.wAlloc - r.prev.wAlloc);
    case 6: return r.prev === undefined ? '—' : fmtDelta(r.wFree - r.prev.wFree);
    case 7: return r.prev === undefined ? '—' : fmtDelta(r.larson - r.prev.larson);
    case 8: return r.prev === undefined ? '—' : fmtDelta(r.mstress - r.prev.mstress);
    case 9: return r.verdict;
    default: return '';
  }
}

function fmtDelta(n) {
  return n === 0 ? '0' : n > 0 ? `+${n}` : String(n);
}

const line = (cells) => `| ${cells.map((c, i) => c.padEnd(widths[i])).join(' | ')} |`;
const sep = `| ${widths.map((w) => '-'.repeat(w)).join(' | ')} |`;

console.log(line(hdr));
console.log(sep);
for (const r of rows) {
  console.log(line(hdr.map((_, i) => rowCell(r, i))));
}
