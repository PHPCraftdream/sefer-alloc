#!/usr/bin/env node
// Ph3c B3 integrated spike (#2109) — iai A/B table builder (callgrind, deterministic).
//
// Gates are the PRE-REGISTERED escalation §4.1 limits
// (docs/design/2026-10-02-adr-addendum-ph3c-escalation.md), fixed before any code:
//   hot   (small_churn_16b, churn_256b, churn_write_256b, aligned_churn_640b_a128):
//         Ir <= 1.02 AND EstCycles <= 1.02
//   refill/flush (cold_alloc_free_256x{16,64}b, recycle_alloc_free_256x{16,64}b,
//         multiseg_cold_256k): EstCycles <= 1.10 AND Ir <= 1.20
//   A/A control: every mimalloc arm DeltaIr == 0.000% exactly.
// Determinism: base side may carry up to three runs (--base2/--base3); all runs
// must agree on Ir bit-for-bit for every bench.
//
// Usage (from the ph3c-b3s worktree root):
//   node scripts/ph3c_b3s_iai_table.mjs [--base <log>] [--b3 <log>]
//        [--base2 <log>] [--base3 <log>] [--b32 <log>] [--b33 <log>]
// Writes docs/perf/PH3C_B3P_STEP1PRIME_IAI_summary.csv as a side effect.

import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

const BASE_SHA = 'c584a3ae';
const PATCH_S_SHA256 = '94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee';
const FEATURES = 'production bench-internals internals';

const DEFAULT_BASE = join(REPO_ROOT, 'docs', 'perf', '_raw_ph3c_b3s_iai_base.log');
const DEFAULT_B3 = join(REPO_ROOT, 'docs', 'perf', '_raw_ph3c_b3s_iai_b3.log');
const CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PH3C_B3P_STEP1PRIME_IAI_summary.csv');

const argv = process.argv.slice(2);
const opts = { base: DEFAULT_BASE, b3: DEFAULT_B3, base2: null, base3: null, b32: null, b33: null };
for (let i = 0; i < argv.length; i++) {
  const k = argv[i];
  if (k === '--base') opts.base = resolve(argv[++i]);
  else if (k === '--b3') opts.b3 = resolve(argv[++i]);
  else if (k === '--base2') opts.base2 = resolve(argv[++i]);
  else if (k === '--base3') opts.base3 = resolve(argv[++i]);
  else if (k === '--b32') opts.b32 = resolve(argv[++i]);
  else if (k === '--b33') opts.b33 = resolve(argv[++i]);
  else throw new Error(`unknown argument: ${k}`);
}

const ADR_HOT = [
  'small_churn_16b',
  'churn_256b',
  'churn_write_256b',
  'aligned_churn_640b_a128',
];
const ADR_REFILL = [
  'cold_alloc_free_256x16b',
  'cold_alloc_free_256x64b',
  'recycle_alloc_free_256x16b',
  'recycle_alloc_free_256x64b',
  'multiseg_cold_256k',
];
const HOT_IR = 1.02, HOT_CYC = 1.02, REFILL_IR = 1.20, REFILL_CYC = 1.10;
const AA_PREFIX = 'mimalloc';

const ROW_LABEL = new Set(['Instructions', 'Estimated Cycles']);
const METRIC_ROW_RE = /^([A-Za-z][A-Za-z0-9 +]*?):\s+(\d[\d,]*)\s*\|\s*(\d[\d,]*|N\/A)\b/;
const HEADER_RE = /^([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)+)\s*$/;

function shortName(full) {
  const parts = full.split('::');
  return parts[parts.length - 1];
}

function parseLog(text, path) {
  const rows = new Map();
  const order = [];
  let current = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    const h = HEADER_RE.exec(line.trim());
    if (h) {
      current = h[1];
      if (!rows.has(current)) {
        rows.set(current, { key: current, name: shortName(current), ir: null, cyc: null });
        order.push(current);
      }
      continue;
    }
    const m = METRIC_ROW_RE.exec(line.trim());
    if (!m || !current || !rows.has(current)) continue;
    if (!ROW_LABEL.has(m[1])) continue;
    const value = Number.parseInt(m[2].replace(/,/g, ''), 10);
    assert(Number.isFinite(value), `${path}: non-numeric value`);
    const row = rows.get(current);
    if (m[1] === 'Instructions') {
      assert(row.ir === null, `${path}: dup Instructions for ${current}`);
      row.ir = value;
    } else {
      assert(row.cyc === null, `${path}: dup Cycles for ${current}`);
      row.cyc = value;
    }
  }
  assert(rows.size > 0, `${path}: no benches parsed`);
  for (const key of order) {
    assert(rows.get(key).ir !== null, `${path}: ${key} missing Instructions`);
    assert(rows.get(key).cyc !== null, `${path}: ${key} missing Estimated Cycles`);
  }
  return { rows, order };
}

function ratioOf(base, spike, what) {
  assert(Number.isFinite(base) && Number.isFinite(spike), `non-finite ${what}`);
  assert(base !== 0, `zero denominator for ${what}`);
  const r = spike / base;
  assert(Number.isFinite(r), `non-finite ratio ${what}`);
  const relErr = spike === 0 ? 0 : Math.abs(base * r - spike) / Math.abs(spike);
  assert(relErr <= 1e-4, `round-trip mismatch ${what}`);
  return r;
}
const pct = (b, s, w) => (ratioOf(b, s, w) - 1) * 100;
const fmtPct = (b, s, w) => {
  const p = pct(b, s, w);
  return `${p >= 0 ? '+' : '-'}${Math.abs(p).toFixed(3)}%`;
};
const rel = (p) => relative(REPO_ROOT, resolve(p)).replace(/\\/g, '/');

const base = parseLog(readFileSync(opts.base, 'utf8'), opts.base);
const b3 = parseLog(readFileSync(opts.b3, 'utf8'), opts.b3);
const extraRuns = [];
for (const [tag, path] of [['base2', opts.base2], ['base3', opts.base3]]) {
  if (path) extraRuns.push([tag, parseLog(readFileSync(path, 'utf8'), path)]);
}
const extraRunsB3 = [];
for (const [tag, path] of [['b32', opts.b32], ['b33', opts.b33]]) {
  if (path) extraRunsB3.push([tag, parseLog(readFileSync(path, 'utf8'), path)]);
}

const benchKeys = base.order;
assert.deepStrictEqual(benchKeys.slice().sort(), b3.order.slice().sort(), 'bench sets differ');
for (const [tag, log] of extraRuns) {
  assert.deepStrictEqual(benchKeys.slice().sort(), log.order.slice().sort(), `${tag} bench set differs`);
}

// Determinism: every extra run's Ir must equal run 1 bit-for-bit.
for (const [tag, log] of extraRuns) {
  for (const key of benchKeys) {
    assert.strictEqual(log.rows.get(key).ir, base.rows.get(key).ir, `${tag}: Ir drift at ${key}`);
  }
}
for (const [tag, log] of extraRunsB3) {
  for (const key of benchKeys) {
    assert.strictEqual(log.rows.get(key).ir, b3.rows.get(key).ir, `${tag}: Ir drift at ${key}`);
  }
}

const csv = ['bench,ir_base,ir_b3,ir_delta_pct,ir_ratio,cyc_base,cyc_b3,cyc_delta_pct,cyc_ratio,gate'];
const out = [];
const say = (l = '') => out.push(l);

say('# Ph3c step 1 — шаг 1′ (ph3c-b3p: B3 + per-class carve), iai A/B (callgrind, детерминирован)');
say();
say(`base:  \`${rel(opts.base)}\` (main ${BASE_SHA} + patch S, sha256 ${PATCH_S_SHA256})`);
say(`b3:    \`${rel(opts.b3)}\` (main ${BASE_SHA} + patch S + B3)`);
for (const [tag, log] of extraRuns) say(`${tag}:   детерминизм Ir базы — побитово равен прогону 1 (все ${benchKeys.length} бенчей)`);
for (const [tag, log] of extraRunsB3) say(`${tag}:   детерминизм Ir B3 — побитово равен прогону 1 (все ${benchKeys.length} бенчей)`);
say(`features: \`${FEATURES}\` | бенчей: ${benchKeys.length}`);
say();
say('Пределы (эскалация §4.1, пред-регистрированы): hot Ir/EstCycles ≤ 1.02; refill/flush EstCycles ≤ 1.10 И Ir ≤ 1.20.');
say('ΔX% = (b3 − base) / base × 100; каждый процент черезguarded-принтер с round-trip assert ±0.01%.');
say();
say('## Контроль A/A (mimalloc-плечи, код спайка не исполняется)');
say();
let aaFail = 0, aaCycMax = 0;
for (const key of benchKeys) {
  const b = base.rows.get(key), s = b3.rows.get(key);
  if (!b.name.startsWith(AA_PREFIX)) continue;
  const p = pct(b.ir, s.ir, `${key} Ir`);
  aaCycMax = Math.max(aaCycMax, Math.abs(pct(b.cyc, s.cyc, `${key} Cycles`)));
  if (p !== 0) aaFail += 1;
  say(`- ${b.name}: ΔIr ${fmtPct(b.ir, s.ir, key)} ${p === 0 ? 'OK (0.000%)' : 'FAIL'}`);
}
say();
say(`A/A Ir: ${aaFail === 0 ? 'все 0.000% ровно — OK' : `FAIL: ${aaFail} плеч(о) с ΔIr ≠ 0`}; max |ΔEstCycles| = ${aaCycMax.toFixed(3)}%.`);
say();
say('## Гейты ADR');
say();
const gate = (name, irLim, cycLim) => {
  const r = base.rows.get((base.rows.has(`perf_gate_iai::perf_gate::${name}`) ? `perf_gate_iai::perf_gate::${name}` : benchKeys.find((k) => k.endsWith('::' + name))));
  assert(r !== undefined, `bench ${name} not found`);
  const b = base.rows.get(r.key), s = b3.rows.get(r.key);
  const irR = ratioOf(b.ir, s.ir, `${name} Ir`);
  const cycR = ratioOf(b.cyc, s.cyc, `${name} Cycles`);
  const ok = irR <= irLim && cycR <= cycLim;
  say(`- ${name}: Ir ${b.ir}→${s.ir} (x${irR.toFixed(5)}, ${fmtPct(b.ir, s.ir, name)}) | EstCycles ${b.cyc}→${s.cyc} (x${cycR.toFixed(5)}) | пределы Ir≤${irLim} Cyc≤${cycLim} → ${ok ? 'PASS' : 'FAIL'}`);
  csv.push(`${name},${b.ir},${s.ir},${pct(b.ir, s.ir, name).toFixed(4)},${irR.toFixed(6)},${b.cyc},${s.cyc},${pct(b.cyc, s.cyc, name).toFixed(4)},${cycR.toFixed(6)},${ok ? 'PASS' : 'FAIL'}`);
  return ok;
};
let hotOk = true, refillOk = true;
for (const n of ADR_HOT) hotOk = gate(n, HOT_IR, HOT_CYC) && hotOk;
for (const n of ADR_REFILL) refillOk = gate(n, REFILL_IR, REFILL_CYC) && refillOk;
say();
say(`hot: ${hotOk ? 'PASS' : 'FAIL'} | refill/flush: ${refillOk ? 'PASS' : 'FAIL'} | A/A: ${aaFail === 0 ? 'PASS' : 'FAIL'}`);
say();
say('## Все 85 бенчей (ΔIr, ΔCycles)');
say();
for (const key of benchKeys) {
  const b = base.rows.get(key), s = b3.rows.get(key);
  const irR = ratioOf(b.ir, s.ir, key);
  const cycR = ratioOf(b.cyc, s.cyc, key);
  say(`- ${b.name}: Ir ${b.ir}→${s.ir} (${fmtPct(b.ir, s.ir, key)}, x${irR.toFixed(5)}) | Cyc ${b.cyc}→${s.cyc} (${fmtPct(b.cyc, s.cyc, key)}, x${cycR.toFixed(5)})`);
  csv.push(`${b.name},${b.ir},${s.ir},${pct(b.ir, s.ir, key).toFixed(4)},${irR.toFixed(6)},${b.cyc},${s.cyc},${pct(b.cyc, s.cyc, key).toFixed(4)},${cycR.toFixed(6)},`);
}

writeFileSync(CSV_PATH, csv.join('\n') + '\n');
console.log(out.join('\n'));
console.log(`\nCSV: ${rel(CSV_PATH)}`);
