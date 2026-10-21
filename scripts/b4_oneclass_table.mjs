#!/usr/bin/env node
/**
 * B4 ONE-CLASS — judge-table generator / consistency checker (D1′ + OPT).
 *
 * Входы (docs/perf/):
 *   _raw_b4_oneclass_{base|b|b2}-<cell>.log  — callgrind-логи плеч
 *       base → A (bbc3c7b4), b → B′ (интермедиат до OPT-правки), b2 → B (голова с OPT)
 *   _raw_b4_oneclass_mem-<a|b>-<full|sparse|cold>.log — MEM-строки судьи памяти
 *
 * Скрипт: (1) извлекает ИЗМЕРЕНО-значения из сырых логов, (2) строгие assert'ы
 * арифметики (Δ = B − A точно; проценты с округлением до 0.1; committed Δ = 0),
 * (3) печатает таблицу судей с вердиктами по порогам §6.1 oneclass-дизайна,
 * (4) пишет docs/perf/B4_ONECLASS_summary.csv.
 * Никакого cargo: только чтение файлов и stdout. Несошедшийся assert или
 * отсутствующий лог → ненулевой код выхода.
 */
'use strict';

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PERF = path.join(__dirname, '..', 'docs', 'perf');
const CSV = path.join(PERF, 'B4_ONECLASS_summary.csv');

const failures = [];
// Self-excluding source manifest digest is passed by the caller.
const SEGMENT = 4 * 1024 * 1024;
const SNAPSHOT_PATCH_SHA256 = process.env.B4_SNAPSHOT_PATCH_SHA256 || '';
function fail(msg) {
  failures.push(msg);
}
if (!/^[0-9a-f]{64}$/.test(SNAPSHOT_PATCH_SHA256)) fail('B4_SNAPSHOT_PATCH_SHA256 absent/invalid; complete patch identity required');
function readLog(name) {
  const p = path.join(PERF, name);
  if (!fs.existsSync(p)) {
    fail(`лог не найден: ${name}`);
    return '';
  }
  return fs.readFileSync(p, 'utf8');
}

function firstMatch(text, re) {
  const m = text.match(re);
  return m ? Number(m[1]) : undefined;
}

// ---------- парсеры ----------

// Каждый лог содержит только свою секцию — нужны только «свои» ключи.
function parseSuiteLog(text, label, keys) {
  const need = (name, re) => {
    const v = firstMatch(text, re);
    if (v === undefined) fail(`${label}: не найдено "${name}"`);
    return v;
  };
  const RES = {
    mstress: /^MARGINAL Ir\/op \(sefer mstress T=1\): (\d+)/m,
    larson: /^MARGINAL Ir\/op \(sefer larson T=1\): (\d+)/m,
    foreign: /^MARGINAL Ir\/free \(foreign locality\): (\d+)/m,
    ao_alloc: /^== sefer w_alloc marginal Ir\/op: (\d+)/m,
    ao_free: /^== sefer w_free marginal Ir\/op: (\d+)/m,
    ao_alloc_cycles: /^== sefer w_alloc cache:.*EstCycles\/op=([\d.]+)/m,
    ao_free_cycles: /^== sefer w_free cache:.*EstCycles\/op=([\d.]+)/m,
    rt_alloc: /^== sefer w_alloc \(mode=rt\) marginal Ir\/op: (\d+)/m,
    rt_free: /^== sefer w_free \(mode=rt\) marginal Ir\/op: (\d+)/m,
    c16_alloc: /^== sefer w_alloc16 \(mode=c16\) marginal Ir\/op: (\d+)/m,
    c16_free: /^== sefer w_free16 \(mode=c16\) marginal Ir\/op: (\d+)/m,
    c64_alloc: /^== sefer w_alloc64 \(mode=c64\) marginal Ir\/op: (\d+)/m,
    c64_free: /^== sefer w_free64 \(mode=c64\) marginal Ir\/op: (\d+)/m,
    batch_alloc: /^== sefer w_alloc marginal Ir\/call \(256\/round\): (\d+)/m,
    batch_free: /^== sefer w_free marginal Ir\/call \(256\/round\): (\d+)/m,
    batchc_alloc: /^== sefer w_alloc16 \(const 16B\) marginal Ir\/call \(256\/round\): (\d+)/m,
    batchc_free: /^== sefer w_free16 \(const 16B\) marginal Ir\/call \(256\/round\): (\d+)/m,
    batchl_alloc: /^== sefer w_alloc marginal Ir\/call \(258\/round\): (\d+)/m,
    batchl_free: /^== sefer w_free marginal Ir\/call \(258\/round\): (\d+)/m,
  };
  const out = {};
  for (const k of keys) out[k] = need(k, RES[k]);
  return out;
}

// thrash64 — отдельный лог (свой example, ao-строк в нём нет).
function parseThrashLog(text, label) {
  const need = (name, re) => {
    const v = firstMatch(text, re);
    if (v === undefined) fail(`${label}: не найдено "${name}"`);
    return v;
  };
  return {
    alloc: need('alloc', /^== sefer w_alloc marginal Ir\/op: (\d+)/m),
    free: need('free', /^== sefer w_free marginal Ir\/op: (\d+)/m),
    alloc_cycles: need('alloc_cycles', /^== sefer w_alloc cache:.*EstCycles\/op=([\d.]+)/m),
    free_cycles: need('free_cycles', /^== sefer w_free cache:.*EstCycles\/op=([\d.]+)/m),
  };
}

function parseMemLog(text, label, shoulder) {
  const m = text.match(
    /^MEM committed_bytes=(\d+) system_sidecar_bytes=(\d+) retained_pins=(-?\d+) descriptor_entries=(-?\d+) rss_kib=(\d+) config_conflicts_total=(\d+) requested_pool=(\S+) resolved_pool=(\S+)/m,
  );
  if (!m) {
    // Legacy (WITHDRAWN) judge format: pin_bytes duplicated sidecar bytes and
    // retained_pins counted retained allocations — those logs were recalled;
    // fail loudly instead of silently mixing formats.
    const legacy = text.match(/^MEM /m);
    if (legacy) {
      fail(`${label}: старый (отозванный) формат MEM — ожидается mem3-судья`);
    } else {
      fail(`${label}: MEM-строка не найдена`);
    }
    return null;
  }
  const header = text.match(/^identity: shoulder=(a|b) (.*)$/m);
  const identity = header ? (header[1] === 'a' ? `base=bbc3c7b4; ${header[2]}` : header[2]) : 'UNKNOWN';
  if (!header) fail(`${label}: source identity header absent`);
  const treeDiff = header && header[1] === 'b' ? text.match(/tree-diff-sha256\(src\+crates\)=([0-9a-f]{64})/)?.[1] : undefined;
  const sourceManifest = header && header[1] === 'b' ? text.match(/source-manifest-sha256=([0-9a-f]{64})/)?.[1] : undefined;
  const identityType = header && header[1] === 'a' ? `instrumentation-patch:${patchIdentity(text)}` : header && header[1] === 'b' ? (identity.includes('UNKNOWN_CURRENT_BUILD') ? 'UNKNOWN_CURRENT_BUILD' : sourceManifest ? 'source-manifest' : treeDiff ? 'src+crates-diff' : 'UNKNOWN') : 'UNKNOWN';
  const opt = (name, re) => {
    const v = text.match(re);
    return v ? Number(v[1]) : undefined;
  };
  return {
    committed: Number(m[1]),
    sidecar: Number(m[2]),
    pins: Number(m[3]),
    descriptor_entries: Number(m[4]),
    rss_kib: Number(m[5]),
    conflicts: Number(m[6]),
    requested_pool: m[7],
    resolved_pool: m[8],
    resolved_pool_byte_cap: opt('resolved_pool_byte_cap', / resolved_pool_byte_cap=(\d+)/),
    route_registrations: opt('route_registrations', / route_registrations=(\d+)/),
    dead_route_bytes: opt('dead_route_bytes', / dead_route_bytes=(\d+)/),
    retained_pins_after_sweep: opt(
      'retained_pins_after_sweep',
      / retained_pins_after_sweep=(-?\d+)/,
    ),
    sweep_freed_pins: opt('sweep_freed_pins', / sweep_freed_pins=(\d+)/),
    dead_route_bytes_per_freed_pin: opt(
      'dead_route_bytes_per_freed_pin',
      / dead_route_bytes_per_freed_pin=(\d+)/,
    ),
    tree_diff_sha256: treeDiff || sourceManifest || 'UNKNOWN',
    source_identity: identity,
    identity_type: identityType,
  };
}

// ---------- снятие измерений ----------

const SHOULDER = {
  A: { prefix: '_raw_b4_oneclass_base-', tag: 'A (oc-base, bbc3c7b4)' },
  Bp: { prefix: '_raw_b4_oneclass_b-', tag: "B' (oc-b, до OPT-правки)" },
  B: { prefix: '_raw_b4_oneclass_b2-', tag: 'B (oc-b2, голова с OPT)' },
};

const CELLS = ['mstress', 'larson', 'ao', 'aoc', 'batch', 'batchc', 'batchl', 'foreign'];

const M = {}; // M[A|Bp|B][cell] = parsed
// Identity (R29-6/R14-10): each row carries its own raw log's tree diff.
const TT = {};
const patchIdentity = (text) => text.match(/patch(?: identity)?(?: sha256)?[=: ]+([0-9a-f]{64})/i)?.[1] ?? text.match(/oc_mem_hooks_a\.patch\(sha256=([0-9a-f]{64})\)/)?.[1] ?? 'UNKNOWN';
function treeOf(text, label) {
  const m = text.match(/^tree-diff-sha256 ([0-9a-f]{64})/m);
  if (!m) {
    fail(`${label}: tree-diff-sha256 не найден (identity обязателен)`);
    return 'MISSING';
  }
  return m[1];
}
for (const [key, sh] of Object.entries(SHOULDER)) {
  M[key] = {};
  TT[key] = {};
  for (const cell of CELLS) {
    const KEYS = {
      ao: ['ao_alloc', 'ao_free', 'ao_alloc_cycles', 'ao_free_cycles'],
      aoc: ['rt_alloc', 'rt_free', 'c16_alloc', 'c16_free', 'c64_alloc', 'c64_free'],
      batch: ['batch_alloc', 'batch_free'],
      batchc: ['batchc_alloc', 'batchc_free'],
      batchl: ['batchl_alloc', 'batchl_free'],
    };
    const t = readLog(`${sh.prefix}${cell}.log`);
    if (!t) continue;
    TT[key][cell] = treeOf(t, `${sh.prefix}${cell}.log`);
    if (KEYS[cell]) {
      M[key][cell] = parseSuiteLog(t, `${sh.prefix}${cell}.log`, KEYS[cell]);
    } else {
      const re =
        cell === 'mstress'
          ? /^MARGINAL Ir\/op \(sefer mstress T=1\): (\d+)/m
          : cell === 'larson'
            ? /^MARGINAL Ir\/op \(sefer larson T=1\): (\d+)/m
            : /^MARGINAL Ir\/free \(foreign locality\): (\d+)/m;
      const v = firstMatch(t, re);
      if (v === undefined) fail(`${sh.prefix}${cell}.log: маргинальная строка не найдена`);
      else M[key][cell] = v;
    }
  }
  const t = readLog(`${sh.prefix}thrash64.log`);
  if (t) {
    TT[key].thrash64 = treeOf(t, `${sh.prefix}thrash64.log`);
    M[key].thrash64 = parseThrashLog(t, `${sh.prefix}thrash64.log`);
  }
}
// Согласованность деревьев внутри плеча (разные хэши у ячеек одного плеча = чужой замер).
for (const key of Object.keys(TT)) {
  const seen = {};
  for (const [cell, th] of Object.entries(TT[key])) seen[th] = seen[th] || cell;
  if (Object.keys(seen).length > 1) {
    fail(`плечо ${key}: разные tree-diff внутри плеча: ${JSON.stringify(seen)}`);
  }
}

// ---------- кросс-проверка ИЗМЕРЕНО (референс прогона) ----------

// Референсные числа отчёта; расхождение логов с ними — фатал (источник истины — логи,
// но расхождение обязано быть замечено и объяснено).
const EXPECT = {
  A: { mstress: 380, larson: 282, foreign: 136 },
  Bp: { mstress: 386, larson: 283, foreign: 137 },
  B: { mstress: 347, larson: 278, foreign: 134 },
};
for (const [k, exp] of Object.entries(EXPECT)) {
  for (const [cell, v] of Object.entries(exp)) {
    if (M[k][cell] !== undefined && M[k][cell] !== v) {
      fail(`кросс-проверка ${k}/${cell}: в логе ${M[k][cell]}, ожидалось ${v}`);
    }
  }
}

// ---------- арифметика ----------

function delta(a, b) {
  const d = b - a;
  if (!Number.isInteger(a) || !Number.isInteger(b) || !Number.isInteger(d)) {
    fail(`Δ не целое: ${a} -> ${b}`);
  }
  return d;
}
function pct(a, b) {
  if (a === 0) return 'n/a'; // A=0: процент не определён (не подставляем 0)
  return Math.round(((b - a) / a) * 1000) / 10; // округление до 0.1
}
function assertPct(a, b, p, tag) {
  if (a === 0) {
    if (p !== 'n/a') fail(`pct при A=0 должен быть "n/a": ${tag}: ${p}`);
    return;
  }
  const exact = ((b - a) / a) * 100;
  const rounded = Math.round(exact * 10) / 10;
  if (rounded !== p) fail(`pct не сошёлся: ${tag || ''} ${a}->${b}: ${p} != ${rounded}`);
}
// Построчные assert-счётчики (итог печатается в консоль).
const ARITH = { checked: 0, pctChecked: 0, mismatches: 0 };
function checkRowArith(tag, a, b, d, p, floatOk = false) {
  ARITH.checked++;
  const okDelta = floatOk ? Math.abs(d - (b - a)) <= 1e-9 : d === b - a;
  if (!okDelta) {
    ARITH.mismatches++;
    fail(`Δ не сошёлся: ${tag} (delta=${d}, B−A=${b - a})`);
  }
  if (p !== undefined) {
    ARITH.pctChecked++;
    const before = failures.length;
    assertPct(a, b, p, tag);
    if (failures.length > before) ARITH.mismatches++;
  }
}

// ---------- судьи §6.1 ----------

function verdictMstress(d) {
  if (d <= -15) return 'GO';
  if (d <= -5) return 'ZONE';
  return 'NO-GO';
}
function verdictLarson(d) {
  if (d <= -1.5) return 'GO';
  if (d < 0) return 'ZONE';
  return 'NO-GO';
}
function verdictNonregress(d, limit = 1) {
  return d <= limit ? 'GO' : 'NO-GO';
}
function verdictBatchAlloc(d) {
  if (d <= -5) return 'GO';
  if (d <= 1) return 'ZONE';
  return 'NO-GO';
}
function verdictForeign(d) {
  // D1′-гейт (пересмотрен): категория ZONE убрана — порог ≤ A−4.
  if (d <= -4) return 'GO';
  return 'FAIL-THRESHOLD';
}

// ---------- строки таблицы ----------

const rows = []; // {cell, metric, A, B, Bp, delta, pct, threshold, verdict}
const csvHeader = ['cell','metric','A','B','Bp','delta','pct','threshold','verdict','A_tree_diff_sha256','Bp_tree_diff_sha256','B_tree_diff_sha256','identity_kind','source_completeness','config_identity','shoulder_a_identity_note','shoulder_bp_identity_note','shoulder_b_identity_note','identity_detail','manifest_digest'];
const csvRecords = [csvHeader];
function csvRecord(record) {
  if (record.length !== csvHeader.length) fail(`CSV record width ${record.length}, expected ${csvHeader.length}`);
  csvRecords.push(record);
}
function csvEscape(value) { const s = String(value ?? ''); return /[",\r\n]/.test(s) ? `"${s.replaceAll('"', '""')}"` : s; }
function addRow(cell, metric, a, b, bp, d, threshold, verdict, floatOk = false) {
  const p = pct(a, b);
  checkRowArith(`${cell}/${metric}`, a, b, d, p, floatOk);
  rows.push({ cell, metric, A: a, B: b, Bp: bp, delta: d, pct: p, threshold, verdict });
}

// Простейшие скалярные судьи.
addRow('mstress', 'Ir/step', M.A.mstress, M.B.mstress, M.Bp.mstress, delta(M.A.mstress, M.B.mstress), 'GO ≤ −15; ZONE −15<Δ≤−5', verdictMstress(M.B.mstress - M.A.mstress));
addRow('larson', 'Ir/step', M.A.larson, M.B.larson, M.Bp.larson, delta(M.A.larson, M.B.larson), 'GO ≤ −1.5; NO-GO ≥ 0', verdictLarson(M.B.larson - M.A.larson));
addRow('foreign', 'Ir/free', M.A.foreign, M.B.foreign, M.Bp.foreign, delta(M.A.foreign, M.B.foreign), 'цель ≤ −4 (D1′-гейт §8.4c)', verdictForeign(M.B.foreign - M.A.foreign));

// ao: alloc/free + EstCycles (нерегресс Δ ≤ +1).
for (const [metric, key, cycKey] of [
  ['ao alloc Ir/op', 'ao_alloc', 'ao_alloc_cycles'],
  ['ao free Ir/op', 'ao_free', 'ao_free_cycles'],
]) {
  const a = M.A.ao[key], b = M.B.ao[key], bp = M.Bp.ao[key];
  addRow('ao', metric, a, b, bp, delta(a, b), 'Δ ≤ +1 Ir/op', verdictNonregress(b - a));
  const ca = M.A.ao[cycKey], cb = M.B.ao[cycKey];
  addRow('ao', metric.replace('Ir/op', 'EstCycles'), ca, cb, M.Bp.ao[cycKey], Math.round((cb - ca) * 10) / 10, 'информативно', 'MEASURED', true);
}

// aoc: rt/c16/c64.
for (const [tag, ak, fk] of [
  ['rt', 'rt_alloc', 'rt_free'],
  ['c16', 'c16_alloc', 'c16_free'],
  ['c64', 'c64_alloc', 'c64_free'],
]) {
  for (const [metric, key] of [['alloc', ak], ['free', fk]]) {
    const a = M.A.aoc[key], b = M.B.aoc[key], bp = M.Bp.aoc[key];
    addRow(`aoc/${tag}`, `${metric} Ir/op`, a, b, bp, delta(a, b), 'Δ ≤ +1 Ir/op', verdictNonregress(b - a));
  }
}

// batch / batchc / batchl.
for (const [cell, ak, fk, thrA, thrF] of [
  ['batch', 'batch_alloc', 'batch_free', 'alloc GO ≤ −5; NO-GO > +1', 'free Δ ≤ +1'],
  ['batchc', 'batchc_alloc', 'batchc_free', 'alloc GO ≤ −5; NO-GO > +1', 'free Δ ≤ +1'],
  ['batchl', 'batchl_alloc', 'batchl_free', 'alloc GO ≤ −5; NO-GO > +1', 'free Δ ≤ +1'],
]) {
  const a = M.A[cell][ak], b = M.B[cell][ak], bp = M.Bp[cell][ak];
  addRow(cell, 'alloc Ir/блок', a, b, bp, delta(a, b), thrA, verdictBatchAlloc(b - a));
  const fa = M.A[cell][fk], fb = M.B[cell][fk], fp = M.Bp[cell][fk];
  addRow(cell, 'free Ir/блок', fa, fb, fp, delta(fa, fb), thrF, verdictNonregress(fb - fa));
}

// thrash64: Ir/op и EstCycles ≤ +1 %.
{
  const a = M.A.thrash64, b = M.B.thrash64, bp = M.Bp.thrash64;
  addRow('thrash64', 'alloc Ir/op', a.alloc, b.alloc, bp.alloc, delta(a.alloc, b.alloc), 'Δ ≤ +1 Ir/op', verdictNonregress(b.alloc - a.alloc));
  const dFree = delta(a.free, b.free);
  const cycPctExact = ((b.free_cycles - a.free_cycles) / a.free_cycles) * 100;
  const cycPct = Math.round(cycPctExact * 10) / 10;
  const v = dFree <= 1 && cycPct <= 1 ? 'GO' : 'NO-GO';
  const row = { cell: 'thrash64', metric: 'free Ir/op', A: a.free, B: b.free, Bp: bp.free, delta: dFree, pct: pct(a.free, b.free), threshold: `Δ ≤ +1 Ir/op И EstCycles ≤ +1 % (Δcyc=${cycPct} %)`, verdict: v };
  rows.push(row);
  checkRowArith('thrash64/free Ir/op', a.free, b.free, dFree, row.pct);
  addRow('thrash64', 'free EstCycles', a.free_cycles, b.free_cycles, bp.free_cycles, Math.round((b.free_cycles - a.free_cycles) * 10) / 10, `EstCycles ≤ +1 % (точно ${cycPct} %)`, cycPct <= 1 ? 'GO' : 'NO-GO', true);
}

// ---------- судья памяти ----------

const MEM = {};
const MEM_RAW = {}; // repo-local raw evidence only
for (const sh of ['a', 'b']) {
  for (const mode of ['full', 'sparse', 'cold', 'deadpins']) {
    const t = readLog(`_raw_b4_oneclass_mem4-${sh}-${mode}.log`);
    if (!t) continue;
    MEM_RAW[`${sh}-${mode}`] = t;
    MEM[`${sh}-${mode}`] = parseMemLog(t, `mem4-${sh}-${mode}`, sh);
  }
}

const memRows = [];
for (const mode of ['full', 'sparse', 'cold', 'deadpins']) {
  const a = MEM[`a-${mode}`], b = MEM[`b-${mode}`];
  const aRaw = MEM_RAW[`a-${mode}`] ?? '';
  const bRaw = MEM_RAW[`b-${mode}`] ?? '';
  if (!a || !b) continue;
  const memKeys = [
    'committed',
    'sidecar',
    'pins',
    'descriptor_entries',
    'rss_kib',
    'route_registrations',
    'dead_route_bytes',
    'dead_route_bytes_per_freed_pin',
    'retained_pins_after_sweep',
  ];
  for (const key of memKeys) {
    if (a[key] === undefined || b[key] === undefined) continue;
    const d = b[key] - a[key];
    const p = pct(a[key], b[key]);
    checkRowArith(`mem4/${mode}/${key}`, a[key], b[key], d, p);
    memRows.push({ mode, metric: key, A: a[key], B: b[key], delta: d, pct: p });
  }
  if (a.conflicts !== 0 || b.conflicts !== 0) fail(`mem4-${mode}: config_conflicts_total != 0`);
  // И1-следствие D1′: раскладка сегмента не меняется → committed Δ = 0,
  // КРОМЕ sparse: переработанный sparse-сценарий намеренно заставляет
  // регистрацию свежих small-маршрутов (по одному сегменту 4 МиБ на
  // регистрацию), поэтому committed Δ обязан равняться
  // registrations_delta × SEGMENT из того же raw-лога.
  const get = (raw, regex, label) => {
    const m = raw.match(regex);
    if (!m) { fail(`${mode}: ${label} missing`); return null; }
    return Number(m[1]);
  };
  const planEvidence = mode === 'cold'
    ? { requested: aRaw.match(/cold: iteration 0 live 4-KiB blocks = (\d+)/), other: bRaw.match(/cold: iteration 0 live 4-KiB blocks = (\d+)/), planA: aRaw.match(/claimed cycles=(\d+)/), planB: bRaw.match(/claimed cycles=(\d+)/) }
    : mode === 'sparse'
      ? { requested: aRaw.match(/sparse: allocated (\d+) 4-KiB blocks \(plan (\d+)/), other: bRaw.match(/sparse: allocated (\d+) 4-KiB blocks \(plan (\d+)/), planA: aRaw.match(/plan (\d+)/), planB: bRaw.match(/plan (\d+)/) }
      : mode === 'full'
        ? { requested: aRaw.match(/full: live 16-B blocks = (\d+)/), other: bRaw.match(/full: live 16-B blocks = (\d+)/) }
        : { requested: aRaw.match(/checkpoint phase-1 (\d+) fixture routes registered/), other: bRaw.match(/checkpoint phase-1 (\d+) fixture routes registered/), planA: aRaw.match(/route_registrations=(\d+)/), planB: bRaw.match(/route_registrations=(\d+)/) };
  if (!planEvidence.requested || !planEvidence.other || planEvidence.requested[1] !== planEvidence.other[1]) fail(`${mode}: A/B workload mismatch`);
  if (planEvidence.planA && (!planEvidence.planB || planEvidence.planA[1] !== planEvidence.planB[1])) fail(`${mode}: A/B plan mismatch`);
  const configA = aRaw.match(/requested_pool=(\S+) resolved_pool=(\S+)/);
  const configB = bRaw.match(/requested_pool=(\S+) resolved_pool=(\S+)/);
  if (!configA || !configB || configA[1] !== configB[1] || configA[2] !== configB[2]) fail(`${mode}: A/B config evidence mismatch`);
  if (mode === 'sparse') {
    const regsA = Number(aRaw.match(/sparse: oracle registrations_delta=(\d+)/)?.[1] ?? 0);
    const regsB = Number(bRaw.match(/sparse: oracle registrations_delta=(\d+)/)?.[1] ?? 0);
    if (regsA < 4 || regsB < 4) fail(`sparse activation below 95% plan: A=${regsA}/4 B=${regsB}/4`);
  } else if (mode === 'cold') {
    const registrationsA = Number(aRaw.match(/registrations_delta=(\d+)/)?.[1] ?? 0);
    const registrationsB = Number(bRaw.match(/registrations_delta=(\d+)/)?.[1] ?? 0);
    if (registrationsA !== registrationsB) fail('cold: registration denominators differ A/B');
    for (const [sh, raw] of [['A', aRaw], ['B', bRaw]]) {
      const released = raw.match(/registrations=(\d+), unlink_routes=(\d+), census_removed=(\d+)/);
      if (!released) { fail(`cold ${sh}: release evidence missing`); continue; }
      const regs=Number(released[1]), unlinks=Number(released[2]), census=Number(released[3]);
      if (census !== unlinks || unlinks * 100 < regs * 95) fail(`cold ${sh}: invalid release oracle ${unlinks}/${regs}, census=${census}`);
    }
  }
  if (a.committed !== b.committed) fail(`mem4-${mode}: committed Δ != 0 (${a.committed} -> ${b.committed})`);
}

// Identity per measurement: retain each raw log's tree-diff hash even where
// historical full source reconstruction is unavailable.
for (const mode of ['full', 'sparse', 'cold', 'deadpins']) {
  if (MEM[`a-${mode}`] && MEM[`a-${mode}`].source_identity === 'UNKNOWN_A_PATCH') fail(`mem4-a-${mode}: A instrumentation identity unknown`);
}
// ---------- вывод ----------

function fmt(n) { return typeof n === 'number' && !Number.isInteger(n) ? n.toFixed(1) : String(n); }
console.log('B4 ONE-CLASS — таблица судей (ИЗМЕРЕНО, callgrind T=1, маргинально 50k→100k)');
console.log("A = oc-base (bbc3c7b4), B' = oc-b (интермедиат до OPT-правки), B = oc-b2 (голова с OPT)");
console.log('');
console.log('| Плечо | Метрика | A | B′ | B | Δ=B−A | Δ% | Порог | Вердикт |');
console.log('|---|---|---|---|---|---|---|---|---|');
for (const r of rows) console.log(`| ${r.cell} | ${r.metric} | ${fmt(r.A)} | ${fmt(r.Bp)} | ${fmt(r.B)} | ${r.delta > 0 ? '+' : ''}${fmt(r.delta)} | ${r.pct > 0 ? '+' : ''}${r.pct} % | ${r.threshold} | ${r.verdict} |`);
console.log('');
console.log('Судья памяти (ИЗМЕРЕНО, актуальные mem4 raw-логи; расходы памяти A/B из этих логов):');
console.log('| Режим | Метрика | A | B | Δ |'); console.log('|---|---|---|---|---|');
for (const r of memRows) console.log(`| ${r.mode} | ${r.metric} | ${r.A} | ${r.B} | ${r.delta} |`);

const t3 = readLog('_raw_b4_oneclass_b3-foreign.log');
let b3Tree = t3 ? (t3.match(/^tree-diff-sha256 ([0-9a-f]{64})/m)?.[1] ?? 'MISSING') : 'MISSING';
const v3 = t3 ? firstMatch(t3, /^MARGINAL Ir\/free \(foreign locality\): (\d+)/m) : undefined;
if (t3 && v3 !== undefined) {
  b3Tree = treeOf(t3, '_raw_b4_oneclass_b3-foreign.log');
  const d = v3 - M.A.foreign, p = pct(M.A.foreign, v3); checkRowArith('foreign/b3', M.A.foreign, v3, d, p);
  rows.push({cell:'foreign',metric:'Ir/free (b3, историческое дерево oc3)',A:M.A.foreign,B:v3,Bp:M.Bp.foreign,delta:d,pct:p,threshold:'цель ≤ −4 (D1′-гейт §8.4c)',verdict:verdictForeign(d)});
  console.log(`\nforeign на историческом дереве b3/oc3: ${v3} Ir/free, Δ=${d}, вердикт ${verdictForeign(d)} (tree_diff=${b3Tree.slice(0,12)}; not final-current).`);
} else fail('_raw_b4_oneclass_b3-foreign.log: маргинальная строка не найдена');
const perfCell = (r) => r.cell.startsWith('aoc/') ? 'aoc' : r.cell;
for (const r of rows) {
  const cell = perfCell(r);
  const isB3 = r.metric.includes('b3');
  const aIdentity = TT.A[cell] ?? 'MISSING';
  const bpIdentity = TT.Bp[cell] ?? 'MISSING';
  const bIdentity = isB3 ? b3Tree : (TT.B[cell] ?? 'MISSING');
  const sourceNotes = [`A base bbc3c7b4; source_reconstruction=partial; diff=${aIdentity}`,`B′ source_reconstruction=partial; diff=${bpIdentity}`,`${isB3 ? 'B3 historical; not final-current' : 'B source_reconstruction=partial; diff=' + bIdentity}`];
  csvRecord([r.cell,r.metric,r.A,r.B,r.Bp,r.delta,r.pct,r.threshold,r.verdict,aIdentity,bpIdentity,bIdentity,'tree-diff-sha256','partial source identity; full source reconstruction unavailable','unknown (no perf config in own raw log)',...sourceNotes,'',`manifest=${SNAPSHOT_PATCH_SHA256}`]);
}
for (const r of memRows) {
  const a=MEM[`a-${r.mode}`], b=MEM[`b-${r.mode}`];
  const rawA=MEM_RAW[`a-${r.mode}`];
  const aid=patchIdentity(rawA);
  const aKind=`instrumentation-patch:${aid}`;
  const bKind=b.identity_type;
  const completeness=b.identity_type==='UNKNOWN_CURRENT_BUILD' ? 'unknown full source identity' : 'partial full source identity';
  csvRecord([`mem4/${r.mode}`,r.metric,r.A,r.B,'',r.delta,r.pct,'MEASURED','MEASURED',aid,b.tree_diff_sha256,`${aKind} / ${bKind}`,completeness,`${b.requested_pool}; ${b.resolved_pool}; cap=${b.resolved_pool_byte_cap}`,`A own raw patch identity=${aid}`,`B kind=${bKind}`,`B source reconstruction is incomplete`, `Diff hashes identify measured trees; full source identities are separate`, `A/B identity details are distinct per mode`, `source manifest ${SNAPSHOT_PATCH_SHA256}`]);
}
// ---------- итог ----------

if (failures.length > 0) {
  console.error('\nASSERT FAILURES:');
  for (const f of failures) console.error(' - ' + f);
  process.exit(1);
}
const noGo = rows.filter((r) => r.verdict === 'NO-GO');
console.log(`\nПострочные арифметические проверки: ${ARITH.checked}; процентов проверено: ${ARITH.pctChecked}; mismatches: ${ARITH.mismatches}`);
console.log('Workload/config symmetry checks passed for full, sparse, cold, deadpins (A/B raw headers).');
console.log(`NO-GO строк: ${noGo.length}`);
for (const r of noGo) console.error(`   NO-GO: ${r.cell} / ${r.metric}: Δ=${r.delta} (${r.threshold})`);
fs.writeFileSync(CSV, csvRecords.map((r) => r.map(csvEscape).join(',')).join(String.fromCharCode(10)) + String.fromCharCode(10));
console.log(`CSV записан: docs/perf/B4_ONECLASS_summary.csv`);
process.exit(0);
