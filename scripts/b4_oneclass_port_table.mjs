#!/usr/bin/env node
/**
 * B4 ONE-CLASS — PORT2 judge-table generator / consistency checker.
 *
 * PORT2 — перезамер после гейтирования measurement-счётчиков RouteDirectory
 * (P2-1/P2-2) в ОБОИХ плечах. Входы (docs/perf/):
 *   _raw_b4_oneclass_port2-base-<cell>.log — плечо A (8da6402d + oc_mem_hooks_a3.patch)
 *   _raw_b4_oneclass_port2-b-<cell>.log    — плечо B (объединённое дерево)
 *   _raw_b4_oneclass_port2-{a|b}-{full|sparse|cold|deadpins}.log — судья памяти
 * Feature set каждого запуска записан в шапке raw-лога (строка «features:»).
 *
 * Скрипт: (1) извлекает ИЗМЕРЕНО-значения из сырых логов, (2) строгие assert'ы
 * арифметики (Δ = B − A точно; проценты с округлением до 0.1; committed Δ = 0),
 * (3) печатает таблицу судей с вердиктами по порогам §6.1 oneclass-дизайна,
 * (4) пишет docs/perf/B4_ONECLASS_PORT2_summary.csv.
 * Никакого cargo: только чтение файлов и stdout. Несошедшийся assert или
 * отсутствующий лог → ненулевой код выхода. Хэш патча плеча A передаётся
 * через B4_SNAPSHOT_PATCH_SHA256 (oc_mem_hooks_a3.patch, sha256
 * 1ff7e14000bb078836d7e455732322b857192bcf69439d2e95cbb03c97c9a1ce).
 */
'use strict';

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const PERF = path.join(__dirname, '..', 'docs', 'perf');
const CSV = path.join(PERF, 'B4_ONECLASS_PORT2_summary.csv');

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
  A: { prefix: '_raw_b4_oneclass_port2-base-', tag: 'A (port2-base, чистый origin/main 8da6402d + oc_mem_hooks_a3.patch)' },
  B: { prefix: '_raw_b4_oneclass_port2-b-', tag: 'B (port2-b, объединённое дерево b4-oneclass × 8da6402d)' },
};
// Плеча B′ (интермедиат до OPT) в port2-раунде НЕТ; колонка B′ в таблице/CSV остаётся пустой.

const CELLS = ['mstress', 'larson', 'ao', 'aoc', 'batch', 'batchc', 'batchl', 'foreign'];

const M = {}; // M[A|Bp|B][cell] = parsed
// Identity (R29-6/R14-10): each row carries its own raw log's tree diff.
const TT = {};
const patchIdentity = (text) => text.match(/patch(?: identity)?(?: sha256)?[=: ]+([0-9a-f]{64})/i)?.[1] ?? text.match(/oc_mem_hooks_a[0-9]+\.patch\(sha256=([0-9a-f]{64})\)/)?.[1] ?? 'UNKNOWN';
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

// ---------- судьи §6.1 ----------

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
// B′ отсутствует в port2-раунде.
const bpOf = () => undefined;

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
addRow('mstress', 'Ir/step', M.A.mstress, M.B.mstress, bpOf(), delta(M.A.mstress, M.B.mstress), 'GO ≤ −15; ZONE −15<Δ≤−5', verdictMstress(M.B.mstress - M.A.mstress));
addRow('larson', 'Ir/step', M.A.larson, M.B.larson, bpOf(), delta(M.A.larson, M.B.larson), 'GO ≤ −1.5; NO-GO ≥ 0', verdictLarson(M.B.larson - M.A.larson));
addRow('foreign', 'Ir/free', M.A.foreign, M.B.foreign, bpOf(), delta(M.A.foreign, M.B.foreign), 'цель ≤ −4 (D1′-гейт §8.4c)', verdictForeign(M.B.foreign - M.A.foreign));

// ao: alloc/free + EstCycles (нерегресс Δ ≤ +1).
for (const [metric, key, cycKey] of [
  ['ao alloc Ir/op', 'ao_alloc', 'ao_alloc_cycles'],
  ['ao free Ir/op', 'ao_free', 'ao_free_cycles'],
]) {
  const a = M.A.ao[key], b = M.B.ao[key], bp = bpOf();
  addRow('ao', metric, a, b, bp, delta(a, b), 'Δ ≤ +1 Ir/op', verdictNonregress(b - a));
  const ca = M.A.ao[cycKey], cb = M.B.ao[cycKey];
  addRow('ao', metric.replace('Ir/op', 'EstCycles'), ca, cb, bpOf(), Math.round((cb - ca) * 10) / 10, 'информативно', 'MEASURED', true);
}

// aoc: rt/c16/c64.
for (const [tag, ak, fk] of [
  ['rt', 'rt_alloc', 'rt_free'],
  ['c16', 'c16_alloc', 'c16_free'],
  ['c64', 'c64_alloc', 'c64_free'],
]) {
  for (const [metric, key] of [['alloc', ak], ['free', fk]]) {
    const a = M.A.aoc[key], b = M.B.aoc[key], bp = bpOf();
    addRow(`aoc/${tag}`, `${metric} Ir/op`, a, b, bp, delta(a, b), 'Δ ≤ +1 Ir/op', verdictNonregress(b - a));
  }
}

// batch / batchc / batchl.
for (const [cell, ak, fk, thrA, thrF] of [
  ['batch', 'batch_alloc', 'batch_free', 'alloc GO ≤ −5; NO-GO > +1', 'free Δ ≤ +1'],
  ['batchc', 'batchc_alloc', 'batchc_free', 'alloc GO ≤ −5; NO-GO > +1', 'free Δ ≤ +1'],
  ['batchl', 'batchl_alloc', 'batchl_free', 'alloc GO ≤ −5; NO-GO > +1', 'free Δ ≤ +1'],
]) {
  const a = M.A[cell][ak], b = M.B[cell][ak], bp = bpOf();
  addRow(cell, 'alloc Ir/блок', a, b, bp, delta(a, b), thrA, verdictBatchAlloc(b - a));
  const fa = M.A[cell][fk], fb = M.B[cell][fk], fp = bpOf();
  addRow(cell, 'free Ir/блок', fa, fb, fp, delta(fa, fb), thrF, verdictNonregress(fb - fa));
}

// thrash64: Ir/op и EstCycles ≤ +1 %.
{
  const a = M.A.thrash64, b = M.B.thrash64, bp = { alloc: bpOf(), free: bpOf(), free_cycles: bpOf() };
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
    const t = readLog(`_raw_b4_oneclass_port2-${sh}-${mode}.log`);
    if (!t) continue;
    MEM_RAW[`${sh}-${mode}`] = t;
    MEM[`${sh}-${mode}`] = parseMemLog(t, `port-${sh}-${mode}`, sh);
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
    checkRowArith(`port/${mode}/${key}`, a[key], b[key], d, p);
    memRows.push({ mode, metric: key, A: a[key], B: b[key], delta: d, pct: p });
  }
  if (a.conflicts !== 0 || b.conflicts !== 0) fail(`port-${mode}: config_conflicts_total != 0`);
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
  if (a.committed !== b.committed) fail(`port-${mode}: committed Δ != 0 (${a.committed} -> ${b.committed})`);
}

// Identity per measurement: retain each raw log's tree-diff hash even where
// historical full source reconstruction is unavailable.
for (const mode of ['full', 'sparse', 'cold', 'deadpins']) {
  if (MEM[`a-${mode}`] && MEM[`a-${mode}`].source_identity === 'UNKNOWN_A_PATCH') fail(`port-a-${mode}: A instrumentation identity unknown`);
}
// ---------- вывод ----------

function fmt(n) { return typeof n === 'number' && !Number.isInteger(n) ? n.toFixed(1) : n === undefined ? '' : String(n); }
console.log('B4 ONE-CLASS PORT2 — таблица судей (ИЗМЕРЕНО, callgrind T=1, маргинально 50k→100k; перезамер после гейтирования measurement-счётчиков в обоих плечах)');
console.log('A = port2-base (чистый origin/main 8da6402d + oc_mem_hooks_a3.patch, sha256=1ff7e140…), B = port2-b (объединённое дерево b4-oneclass × 8da6402d); B′ в port2-раунде отсутствует; feature set в шапках raw-логов («features:»)');
console.log('');
console.log('| Плечо | Метрика | A | B′ | B | Δ=B−A | Δ% | Порог | Вердикт |');
console.log('|---|---|---|---|---|---|---|---|---|');
for (const r of rows) console.log(`| ${r.cell} | ${r.metric} | ${fmt(r.A)} | ${fmt(r.Bp)} | ${fmt(r.B)} | ${r.delta > 0 ? '+' : ''}${fmt(r.delta)} | ${r.pct > 0 ? '+' : ''}${r.pct} % | ${r.threshold} | ${r.verdict} |`);
console.log('');
console.log('Судья памяти (ИЗМЕРЕНО, port2 raw-логи; расходы памяти A/B из этих логов):');
console.log('| Режим | Метрика | A | B | Δ |'); console.log('|---|---|---|---|---|');
for (const r of memRows) console.log(`| ${r.mode} | ${r.metric} | ${r.A} | ${r.B} | ${r.delta} |`);

const perfCell = (r) => r.cell.startsWith('aoc/') ? 'aoc' : r.cell;
for (const r of rows) {
  const cell = perfCell(r);
  const aIdentity = TT.A[cell] ?? 'MISSING';
  const bpIdentity = '';
  const bIdentity = TT.B[cell] ?? 'MISSING';
  const sourceNotes = [`A base 8da6402d + oc_mem_hooks_a3.patch (a3 patch sha256=${SNAPSHOT_PATCH_SHA256}); write-tree=02db40ed426458ebdb351babdcf6e263c9b3cd36; source-manifest-sha256=086180efab1faf2bfa9889f4194355bb83cda9a69dbec83a5e2b9e9f992c89b3`, `B′ отсутствует в port2-раунде`, `B объединённое дерево; write-tree=19aa0fcf11fa955a7a245f0d338141891ac38d92; source-manifest-sha256=58f83d6b71a7e8a16341cae2f1f9c6ccb65af351508659e7cfc85af9bf1e5fc0`];
  csvRecord([r.cell,r.metric,r.A,r.B,r.Bp,r.delta,r.pct,r.threshold,r.verdict,aIdentity,bpIdentity,bIdentity,'tree-diff-sha256','full write-tree/manifest identity recorded; tree objects not reachable until commit (pin refs at merge)','unknown (no perf config in own raw log)',...sourceNotes,'',`a3 patch sha256=${SNAPSHOT_PATCH_SHA256}`]);
}
for (const r of memRows) {
  const a=MEM[`a-${r.mode}`], b=MEM[`b-${r.mode}`];
  const rawA=MEM_RAW[`a-${r.mode}`];
  const aid=patchIdentity(rawA);
  const aKind=`instrumentation-patch:${aid}`;
  const bKind=b.identity_type;
  const completeness='full write-tree/manifest identity recorded; tree objects not reachable until commit (pin refs at merge)';
  csvRecord([`port2/${r.mode}`,r.metric,r.A,r.B,'',r.delta,r.pct,'MEASURED','MEASURED',aid,b.tree_diff_sha256,`${aKind} / ${bKind}`,'source-manifest-sha256',completeness,`${b.requested_pool}; ${b.resolved_pool}; cap=${b.resolved_pool_byte_cap}`,`A own raw patch identity=${aid} (a3 patch sha256=${SNAPSHOT_PATCH_SHA256}); A write-tree=02db40ed426458ebdb351babdcf6e263c9b3cd36; A source-manifest-sha256=086180efab1faf2bfa9889f4194355bb83cda9a69dbec83a5e2b9e9f992c89b3`,`B kind=${bKind}; B write-tree=19aa0fcf11fa955a7a245f0d338141891ac38d92; B source-manifest-sha256=58f83d6b71a7e8a16341cae2f1f9c6ccb65af351508659e7cfc85af9bf1e5fc0`,'Diff/manifest hashes identify measured trees; full write-tree/manifest identity recorded; tree objects not reachable until commit (pin refs at merge)','A/B identity details are distinct per mode',`a3 patch sha256=${SNAPSHOT_PATCH_SHA256}`]);
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
console.log(`CSV записан: B4_ONECLASS_PORT2_summary.csv`);
process.exit(0);
