#!/usr/bin/env node
// B4-29 / U31 (item 90): сводная таблица триггеров возврата T1/T2/T3
// из сырых логов WSL-профилирования (docs/perf/b4_u31_raw/ut-u31*), ревизия после
// chunk-A (self-Ir нижняя граница, ops_actual, no_pending-наблюдатель).
//
// Методика:
//  - T1 числитель — НИЖНЯЯ граница: self-Ir scan.rs + bitmap_scan.rs
//    (построчная агрегация callgrind_annotate --auto=yes, строки "=>" и
//   .attrib-перекрытия исключены; RESIDUAL uncovered = glibc/startup без
//    исходников, двойного счёта нет). Рядом печатается ВЕРХНЯЯ граница —
//    inclusive-Ir find_segment_with_free_impl из .annot (ОЦЕНКА границы).
//    Вердикт T1 — ТОЛЬКО по нижней границе, предрегистрирован на n=131072 (4n).
//  - «Потолок выгоды U31» = self(bitmap_scan.rs)/total — ОЦЕНКА, не вердикт
//    (U31 экономит только pending-скан, не bin-head discovery-скан).
//  - T2: SCAN_WORDS/op по ops_actual (фактическое число операций workload'а),
//    медиана и min–max по 3 повторам; рядом — корректный ВЕРХНИЙ ПРЕДЕЛ
//    активации U31 = доля u31_scan_no_pending / u31_discovery_scans (одно
//    множество discovery-сканов: owner-drain через drain_segment_sidecar;
//    strict-trim/bounded сканы не входят). holders==base — другое измерение,
//    знаменатель u31_scan_segments.
//  - Оракул активации — ВЫЧИСЛЯЕТСЯ из COUNTERS (scan_calls/find_segment_calls),
//    порог ≥ 95 %; без строковых констант с результатами.
//
// Пути: каталог с сырыми логами НЕ хардкодится — env B4_U31_RAW_DIR (или
// argv[2]); git diff SHA — env B4_U31_DIFF_SHA256; OS/CPU — env
// B4_U31_OS_INFO / B4_U31_CPU_INFO. Пишет
// docs/perf/B4_U31_TRIGGER_summary.csv.

import { readFileSync, existsSync, writeFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { execFileSync } from 'node:child_process';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const perfDir = join(root, 'docs', 'perf');

const rawDir = process.env.B4_U31_RAW_DIR || process.argv[2];
const diffSha = process.env.B4_U31_DIFF_SHA256;
const osInfo = process.env.B4_U31_OS_INFO;
const cpuInfo = process.env.B4_U31_CPU_INFO;
for (const [v, what] of [
  [rawDir, 'B4_U31_RAW_DIR (каталог с сырыми логами ut-u31*)'],
  [diffSha, 'B4_U31_DIFF_SHA256'],
  [osInfo, 'B4_U31_OS_INFO'],
  [cpuInfo, 'B4_U31_CPU_INFO'],
]) {
  if (!v) {
    console.error(`ОШИБКА: не задано ${what}`);
    process.exit(1);
  }
}

function fail(msg) {
  console.error(`ОШИБКА: ${msg}`);
  process.exit(1);
}
function assert(cond, msg) {
  if (!cond) fail(`assert: ${msg}`);
}
function readRaw(name) {
  const path = join(rawDir, name);
  if (!existsSync(path)) fail(`нет сырого файла ${path}`);
  return readFileSync(path, 'utf8');
}
function fmt(n) {
  return Math.round(n).toLocaleString('en-US').replace(/,/g, ' ');
}
// доля в процентах; числитель ≤ знаменатель проверяется вызывающей стороной,
// здесь только den > 0 (допускаем num=0 — mimalloc self-Ir = 0).
function pct(num, den) {
  assert(den > 0, 'делитель > 0');
  return ((num / den) * 100).toFixed(2);
}
function median(xs) {
  const s = [...xs].sort((a, b) => a - b);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
}
// Percent with the numerator/denominator of the repeat that produced it.
function fracOf(reps, value, num, den) {
  const r = reps.find((x) => x[value.key] === value.v);
  assert(r, `no repeat for ${value.key}=${value.v}`);
  assert(r[num] / r[den] === value.v, `fraction mismatch for ${value.key}`);
  return `${(value.v * 100).toFixed(2)} % (${r[num]}/${r[den]})`;
}
const pctStr = (num, den) => `${pct(num, den)} % (${fmt(num)}/${fmt(den)})`;

// ---------- chunk A: ut-u31run4 (growth16, callgrind, sefer+mimalloc) ----------
const runLog = readRaw('ut-u31run4.log');
const runDiff = runLog.match(/git_diff_sha256 ([0-9a-f]{64})/);
assert(runDiff, 'diff_sha256 найден в run4');
const CHUNK1_SHA = runDiff[1];
// run4/lar2/ms2 — старое сырьё, снятое на дереве 512508e8 (T1/T3 и
// growth16-оракул остаются на нём); MT ut-u31mt3-* — текущее дерево (env
// diffSha). Замеры не перезаписывались, каждая ревизия дерева со своим sha.
assert(/^[0-9a-f]{64}$/.test(CHUNK1_SHA), 'run4 diff_sha256 — sha256');

const NS = ['32768', '65536', '131072'];
const irTotal = {}; // `${mode}:${n}` -> Ir
for (const m of runLog.matchAll(/IR_TOTAL (sefer|mimalloc) (32768|65536|131072) (\d+)/g)) {
  irTotal[`${m[1]}:${m[2]}`] = Number(m[3]);
}
// SELF_IR: self-Ir по нижней границе (scan.rs = bin-head discovery-скан,
// bitmap_scan.rs = pending sidecar scan; построчная агрегация --auto=yes)
const selfIr = {};
for (const m of runLog.matchAll(
  /SELF_IR scan\.rs=(\d+) bitmap_scan\.rs=(\d+) sum=(\d+) total=(\d+)/g,
)) {
  // строки идут в порядке IR_TOTAL выше; сопоставляем по total
  selfIr[m[4]] = { scan: Number(m[1]), bitmap: Number(m[2]), sum: Number(m[3]) };
}
const attr = {}; // `${mode}:${n}` -> {annotated, program, uncovered}
{
  const totals = [];
  for (const m of runLog.matchAll(
    /ATTR_TOTALS RESIDUAL annotated=(\d+) program=(\d+) uncovered=(\d+)/g,
  )) {
    totals.push({ annotated: Number(m[1]), program: Number(m[2]), uncovered: Number(m[3]) });
  }
  let i = 0;
  for (const mode of ['sefer', 'mimalloc']) {
    for (const n of NS) attr[`${mode}:${n}`] = totals[i++];
  }
}
const t1 = {};
for (const mode of ['sefer', 'mimalloc']) {
  for (const n of NS) {
    const total = irTotal[`${mode}:${n}`];
    assert(total > 0, `Ir(${mode},${n}) > 0`);
    const s = selfIr[String(total)];
    assert(s, `SELF_IR-строка найдена по total=${total} (${mode} n${n})`);
    assert(s.scan >= 0 && s.bitmap >= 0, `self-Ir ≥ 0 (${mode} n${n})`);
    assert(s.sum === s.scan + s.bitmap, `sum = scan + bitmap (${mode} n${n})`);
    assert(s.sum <= total, `sum ≤ total (${mode} n${n})`);
    const a = attr[`${mode}:${n}`];
    assert(a.program === total, `ATTR program = IR_TOTAL (${mode} n${n})`);
    assert(a.annotated + a.uncovered === a.program, `annotated+uncovered = program (${mode} n${n})`);
    t1[`${mode}:${n}`] = { total, ...s };
  }
}

// inclusive-Ir find_segment_with_free_impl: сумма записей функции в
// отсортированном списке .annot (исключая "=>"-строки — без двойного счёта).
function inclusiveFromAnnot(annotName, cellLabel) {
  const text = readRaw(annotName);
  let sum = 0;
  for (const line of text.split('\n')) {
    if (!line.includes('::find_segment_with_free_impl')) continue;
    if (line.includes('=>')) continue; // сводная строка — дубликат
    const m = line.match(/^\s*([\d,]+) \(\s*[\d.]+%\)/);
    if (!m) continue; // исходники (fn/self.…) — не записи функции
    sum += Number(m[1].replace(/,/g, ''));
  }
  assert(sum > 0, `inclusive find_segment_with_free_impl > 0 (${cellLabel})`);
  return sum;
}
for (const n of NS) {
  const incl = inclusiveFromAnnot(`ut-u31run4.sefer.n${n}.annot`, `growth16 n${n}`);
  assert(incl <= t1[`sefer:${n}`].total, `inclusive ≤ total (growth16 n${n})`);
  t1[`sefer:${n}`].inclusive = incl;
}

// growth16 COUNTERS (B4) + U31-счётчики из run4
const runCounters = {};
{
  const u31 = {};
  for (const line of runLog.split('\n')) {
    const m = line.match(/^COUNTERS (\{.*\})$/);
    if (m) {
      const c = JSON.parse(m[1]);
      runCounters[String(c.growth16_n)] = c;
      continue;
    }
    if (line.startsWith('{"growth16_n"')) {
      const c = JSON.parse(line);
      u31[String(c.growth16_n)] = c;
    }
  }
  assert(Object.keys(runCounters).length === 3, 'три COUNTERS-строки run4');
  assert(Object.keys(u31).length === 3, 'три U31-JSON-строки run4');
  for (const n of NS) {
    const c = runCounters[n];
    const u = u31[n];
    // Оракул активации growth16: каждый find_segment ушёл в discovery-скан
    const oracle = c.scan_calls / c.find_segment_calls;
    assert(c.find_segment_calls > 0 && c.scan_calls > 0, `growth16 n${n}: счётчики > 0`);
    assert(oracle >= 0.95, `growth16 n${n}: оракул scan/find = ${oracle} ≥ 0.95`);
    assert(c.scan_calls <= c.find_segment_calls, `growth16 n${n}: scan ≤ find`);
    // семантика u31_scan_segments: срабатывает на входе scan_small_from — все
    // owner-drain scan sites; scan_calls — только find_segment. segments ≥ scan_calls.
    assert(u.u31_scan_segments >= c.scan_calls, `growth16 n${n}: segments ≥ scan_calls`);
    assert(u.u31_scan_no_pending <= u.u31_scan_segments, `growth16 n${n}: no_pending ≤ segments`);
    assert(u.u31_holders_base <= u.u31_scan_segments, `growth16 n${n}: holders_base ≤ segments`);
    Object.assign(t1[`sefer:${n}`], { counters: c, u31: u, oracle });
  }
}

// ---------- T1: предрегистрирован на n=131072 (4n) ----------
const t1rows = NS.map((n) => {
  const sefer = t1[`sefer:${n}`];
  const mim = t1[`mimalloc:${n}`];
  const ratio = sefer.total / mim.total;
  assert(ratio > 0, `ratio > 0 (n${n})`);
  assert(sefer.sum <= sefer.total && sefer.inclusive <= sefer.total, `n${n}: границы ≤ total`);
  const selfShare = sefer.sum / sefer.total; // нижняя граница
  const inclShare = sefer.inclusive / sefer.total; // верхняя граница (ОЦЕНКА)
  const ceiling = sefer.bitmap / sefer.total; // потолок выгоды U31 (ОЦЕНКА)
  return { n, sefer, mim, ratio, selfShare, inclShare, ceiling };
});
// Вердикт ТОЛЬКО по n=131072 (4n), только по нижней границе:
const t1verdictRow = t1rows[t1rows.length - 1];
assert(t1verdictRow.n === '131072', 'вердикт T1 — на 4n');
const T1_FIRED =
  t1verdictRow.ratio >= 3.0 && t1verdictRow.selfShare >= 0.3;

// ---------- chunk A: larson/mstress T=1 (ut-u31lar2 / ut-u31ms2) ----------
function parseT1Probe(prefix, workload) {
  const log = readRaw(`${prefix}.log`);
  const d = log.match(/git_diff_sha256 ([0-9a-f]{64})/);
  assert(d && d[1] === CHUNK1_SHA, `${prefix}: diff_sha256 = run4 (одно старое дерево T1/T3)`);
  const ir = Number(log.match(/IR_TOTAL \w+ t1 (\d+)/)[1]);
  assert(ir > 0, `Ir(${workload} t1) > 0`);
  const s = log.match(/SELF_IR scan\.rs=(\d+) bitmap_scan\.rs=(\d+) sum=(\d+) total=(\d+)/);
  assert(s && Number(s[4]) === ir, `SELF_IR ${workload} соответствует IR_TOTAL`);
  const scanSelf = Number(s[1]);
  const bitmapSelf = Number(s[2]);
  const sumSelf = Number(s[3]);
  assert(sumSelf === scanSelf + bitmapSelf && sumSelf <= ir, `self-арифметика ${workload}`);
  const inclusive = inclusiveFromAnnot(
    `${prefix}.${workload}.t1.s100000.annot`,
    `${workload} t1`,
  );
  assert(inclusive <= ir, `inclusive ≤ Ir (${workload})`);
  const cf = readRaw(`${prefix}.${workload}.t1.s100000.counters`)
    .split('\n')
    .find((l) => l.includes('find_segment_calls'));
  const c = JSON.parse(cf);
  const u = JSON.parse(
    readRaw(`${prefix}.${workload}.t1.s100000.counters`)
      .split('\n')
      .find((l) => l.includes('u31_scan_segments')),
  );
  const oracle = c.scan_calls / c.find_segment_calls;
  assert(c.find_segment_calls > 0, `${workload}: find_segment_calls > 0`);
  assert(oracle >= 0.95, `${workload}: оракул scan/find = ${oracle} ≥ 0.95`);
  assert(c.scan_calls <= c.find_segment_calls, `${workload}: scan ≤ find`);
  assert(u.u31_scan_segments >= c.scan_calls, `${workload}: segments ≥ scan_calls`);
  assert(u.u31_scan_no_pending <= u.u31_scan_segments, `${workload}: no_pending ≤ segments`);
  assert(u.u31_holders_base <= u.u31_scan_segments, `${workload}: holders_base ≤ segments`);
  return { workload, ir, scanSelf, bitmapSelf, sumSelf, inclusive, selfShare: sumSelf / ir, inclShare: inclusive / ir, ceiling: bitmapSelf / ir, counters: c, u31: u, oracle };
}
const larson1 = parseT1Probe('ut-u31lar2', 'larson');
const mstress1 = parseT1Probe('ut-u31ms2', 'mstress');

// ---------- chunk A: MT ut-u31mt3-* (T=2/4, по 3 повтора) ----------
// discovery-сайт: drain_segment_sidecar (scan.rs); u31_discovery_scans — счётчик
// именно этого множества сканов, u31_scan_segments — всех scan_small_from.
const CELLS = [
  ['larson', 2],
  ['larson', 4],
  ['mstress', 2],
  ['mstress', 4],
];
const REPS = [1, 2, 3];
// первый прогон larson-t2 упал на компиляции и был перезапущен как r1b
const mtTag = (wl, threads, r) =>
  `r${r}${wl === 'larson' && threads === 2 && r === 1 ? 'b' : ''}`;
const mtDiffShas = new Set();
const mtFiles = [];
const t2cells = CELLS.map(([wl, threads]) => {
  const reps = REPS.map((r) => {
    const name = `ut-u31mt3-${wl}-t${threads}-${mtTag(wl, threads, r)}.log`;
    mtFiles.push(name);
    const log = readRaw(name);
    const d = log.match(/git_diff_sha256 ([0-9a-f]{64})/);
    assert(d, `MT diff_sha256 ${wl} t${threads} r${r}`);
    mtDiffShas.add(d[1]);
    const cLine = log
      .split('\n')
      .find((l) => l.startsWith('{"workload"') && l.includes('find_segment_calls'));
    assert(cLine, `COUNTERS ${wl} t${threads} r${r}`);
    const c = JSON.parse(cLine);
    const sLine = log
      .split('\n')
      .find((l) => l.startsWith('{"workload"') && l.includes('ops_actual'));
    assert(sLine, `stats-JSON ${wl} t${threads} r${r}`);
    const st = JSON.parse(sLine);
    // ops_actual = ФАКТИЧЕСКОЕ число операций по определению workload'а
    // (larson: steps×threads + mailbox-frees; mstress: полраунда free +
    // локальный хвост + mailbox + финальные дрейны — см. worker-циклы пробы).
    assert(st.ops_actual > 0, `${wl} t${threads} r${r}: ops_actual > 0`);
    assert(st.scan_words > 0, `${wl} t${threads} r${r}: scan_words > 0`);
    const perOp1e3 = st.scan_words_per_op_scaled_1e3;
    // trunc-сходимость per-op арифметики от ops_actual
    assert(
      perOp1e3 === Math.trunc((st.scan_words / st.ops_actual) * 1000),
      `${wl} t${threads} r${r}: per-op = trunc(scan_words/ops_actual×1000)`,
    );
    assert(c.scan_calls <= c.find_segment_calls, `${wl} t${threads} r${r}: scan ≤ find`);
    const oracle = c.scan_calls / c.find_segment_calls;
    assert(oracle >= 0.95, `${wl} t${threads} r${r}: оракул scan/find = ${oracle} ≥ 0.95`);
    // семантика (u31_scan_segments - scan_calls): лишние срабатывания — сканы
    // scan_small_from НЕ из find_segment (drain/free-пути); поэтому segments ≥ scan_calls.
    assert(st.u31_scan_segments >= c.scan_calls, `${wl} t${threads} r${r}: segments ≥ scan_calls`);
    assert(st.u31_scan_no_pending <= st.u31_scan_segments, `${wl} t${threads} r${r}: no_pending ≤ segments`);
    assert(st.u31_holders_base <= st.u31_scan_segments, `${wl} t${threads} r${r}: holders_base ≤ segments`);
    assert(
      st.holders_base_num === st.u31_holders_base && st.holders_base_den === st.u31_scan_segments,
      `${wl} t${threads} r${r}: num/den согласованы с u31-счётчиками`,
    );
    assert(st.holders_base_num <= st.holders_base_den, `${wl} t${threads} r${r}: num ≤ den`);
    // верхний предел U31: no_pending / discovery_scans — ОДНО множество сканов
    assert(st.u31_discovery_scans > 0, `${wl} t${threads} r${r}: discovery_scans > 0`);
    assert(
      st.u31_scan_no_pending <= st.u31_discovery_scans,
      `${wl} t${threads} r${r}: no_pending ≤ discovery_scans`,
    );
    assert(
      st.u31_discovery_scans <= st.u31_scan_segments,
      `${wl} t${threads} r${r}: discovery_scans ≤ segments`,
    );
    const noPendingShare = st.u31_scan_no_pending / st.u31_discovery_scans;
    return {
      tag: mtTag(wl, threads, r),
      opsActual: st.ops_actual,
      words: st.scan_words,
      perOp: st.scan_words / st.ops_actual,
      holdersNum: st.u31_holders_base,
      holdersDen: st.u31_scan_segments,
      holders: st.u31_holders_base / st.u31_scan_segments,
      noPendingNum: st.u31_scan_no_pending,
      noPendingDen: st.u31_discovery_scans,
      noPending: noPendingShare,
      segments: st.u31_scan_segments,
      discovery: st.u31_discovery_scans,
      findCalls: c.find_segment_calls,
      scanCalls: c.scan_calls,
      oracle,
    };
  });
  return {
    wl,
    threads,
    reps,
    perOpMedian: median(reps.map((x) => x.perOp)),
    perOpMin: Math.min(...reps.map((x) => x.perOp)),
    perOpMax: Math.max(...reps.map((x) => x.perOp)),
    holdersMedian: median(reps.map((x) => x.holders)),
    holdersMin: Math.min(...reps.map((x) => x.holders)),
    holdersMax: Math.max(...reps.map((x) => x.holders)),
    noPendingMedian: median(reps.map((x) => x.noPending)),
    noPendingMin: Math.min(...reps.map((x) => x.noPending)),
    noPendingMax: Math.max(...reps.map((x) => x.noPending)),
  };
});
assert(mtDiffShas.size === 1, 'все MT-прогоны с одним diff_sha256');
assert(mtDiffShas.has(diffSha), 'MT diff_sha256 = переданному');
assert(mtFiles.length === 12, 'ровно 12 MT-логов ut-u31mt3-*');

// T2 — предрегистрированная строка по ИСХОДНОМУ формальному определению:
// SCAN_WORDS/op ≥ 1.0 (по ops_actual) И медиана holders==base ≥ 50 %.
// Интерпретация «holders==base = не было публикаций» НЕВАЛИДНА для U31
// (см. docs/perf/B4_U31_TRIGGER.md); рядом выводится корректный верхний
// предел активации = доля no_pending.
const T2_FIRED = t2cells.every(
  (c) => c.perOpMedian >= 1.0 && c.holdersMedian >= 0.5,
);

// ---------- T3 ----------
// growth16: self-доля (нижняя граница) ≥ 10 %; larson/mstress T=1: ≥ 5 %.
const t3growth = t1rows.filter((r) => r.selfShare >= 0.1).length;
const t3t1 = [larson1, mstress1].filter((r) => r.selfShare >= 0.05).length;
const T3_FIRED = t3growth > 0 || t3t1 > 0; // ветка OR; multiseg не снимался

// ---------- identity ----------
// База фичевого дерева = 1e20e9aa (origin/main после PR #87/#88, заявленная в
// задаче); она НЕ берётся из текущего `git rev-parse origin/main`, т.к. remote
// ref может двигаться независимо от измеренного дерева. HEAD печатается рядом
// (сейчас оба 1e20e9aa).
const BASE_SHA = '1e20e9aa17a4dc4a15429080f8d9c4351824eeef';
const headSha = execFileSync('git', ['rev-parse', 'HEAD'], { cwd: root }).toString().trim();
const rustcV = execFileSync('rustc', ['--version'], { cwd: root }).toString().trim();

// ---------- таблицы ----------
const out = [];
const say = (s) => out.push(s);
say('## B4-29/U31 — триггеры возврата T1/T2/T3 (измерение, без изменения runtime)');
say('');
say(`Identity: base фичевого дерева = \`${BASE_SHA.slice(0, 8)}\` (константа базы измеренного дерева = origin/main после PR #87/#88; текущий HEAD = \`${headSha.slice(0, 8)}\`); git_diff_sha256: T1/T3-сырьё (ut-u31run4/lar2/ms2, старое дерево) = \`${CHUNK1_SHA.slice(0, 8)}\`, MT-сырьё ut-u31mt3-* = \`${diffSha.slice(0, 8)}\`; ${rustcV}; CPU: ${cpuInfo}; OS: ${osInfo}.`);
say('');
say('Прогоны: chunk A = ut-u31run4 (growth16 × {32768,65536,131072} × {sefer,mimalloc}, callgrind), ut-u31lar2/ut-u31ms2 (larson/mstress T=1), ut-u31mt3-* (4 ячейки × 3 повтора = 12 MT-прогонов, счётчики bench-internals, T=2/4; larson-t2-r1 = файл ut-u31mt3-larson-t2-r1b.log — первый r1 упал на компиляции, перезапущен); мутант «всегда пусто» — ut-u31mutant-nopending.log (exit=101).');
say('');

say('### T1 — growth16 (callgrind Ir): нижняя граница self-Ir скана, inclusive — верхняя');
say('');
say('Числитель (нижняя граница) — self-Ir scan.rs (bin-head discovery-скан) + bitmap_scan.rs (pending sidecar scan), построчная агрегация --auto=yes, без перекрытий. Верхняя граница — inclusive-Ir find_segment_with_free_impl из .annot (ОЦЕНКА границы, включает всё inline- expansion). **Вердикт — только по нижней границе и только на n=131072 (4n).**');
say('');
say('| n | Ir Sefer | Ir mimalloc | Sefer/mimalloc | self: scan.rs | self: bitmap_scan.rs | self сумма (нижняя) | доля self | inclusive (верхняя, ОЦЕНКА) | доля inclusive | потолок U31 = bitmap/total (ОЦЕНКА) |');
say('|---|---|---|---|---|---|---|---|---|---|---|');
for (const r of t1rows) {
  say(
    `| ${r.n} | ${fmt(r.sefer.total)} | ${fmt(r.mim.total)} | ${r.ratio.toFixed(2)}× | ` +
      `${fmt(r.sefer.scan)} | ${fmt(r.sefer.bitmap)} | ${fmt(r.sefer.sum)} | ` +
      `${pctStr(r.sefer.sum, r.sefer.total)} | ${fmt(r.sefer.inclusive)} | ` +
      `${pctStr(r.sefer.inclusive, r.sefer.total)} | ${pctStr(r.sefer.bitmap, r.sefer.total)} |`,
  );
}
say('');
say(`**Вердикт T1: ${T1_FIRED ? 'FIRED' : 'NOT FIRED'}** — по n=131072 (4n): Sefer/mimalloc = ${t1verdictRow.ratio.toFixed(2)}× (порог ≥ 3.0×), self-доля нижней границы = ${pctStr(t1verdictRow.sefer.sum, t1verdictRow.sefer.total)} (порог ≥ 30 %). Остальные размеры — справочные строки, в вердикт не входят.`);
say('');
say('Потолок выгоды U31 (ОЦЕНКА, НЕ вердикт) — доля self-Ir bitmap_scan.rs (pending-скан), который U31 мог бы устранить: ' + t1rows.map((r) => `n=${r.n}: ${pctStr(r.sefer.bitmap, r.sefer.total)}`).join('; ') + '. Bin-head discovery-скан (scan.rs) U31 не устраняет.');
say('');

say('### T1-справка: larson/mstress T=1 (self-нижняя / inclusive-верхняя)');
say('');
say('| Рукав | Ir всего | self сумма (нижняя) | доля self | inclusive (верхняя, ОЦЕНКА) | доля inclusive | потолок U31 (ОЦЕНКА) |');
say('|---|---|---|---|---|---|---|');
for (const p of [larson1, mstress1]) {
  say(
    `| ${p.workload} T=1 | ${fmt(p.ir)} | ${fmt(p.sumSelf)} | ${pctStr(p.sumSelf, p.ir)} | ` +
      `${fmt(p.inclusive)} | ${pctStr(p.inclusive, p.ir)} | ${pctStr(p.bitmapSelf, p.ir)} |`,
  );
}
say('');

say('### T2 — MT (T=2/4): SCAN_WORDS/op по ops_actual и holders==base (формально) + верхний предел no_pending');
say('');
say('Нормировка — ops_actual: ФАКТИЧЕСКОЕ число операций по определению workload\'а (larson: steps×threads + mailbox-frees; mstress: полраунда free + локальный хвост + mailbox + финальные дрейны), включено в логи пробой. min–max — по 3 повторам.');
say('');
say('**Предрегистрированная строка (формальное определение holders==base):** SCAN_WORDS/op ≥ 1.0 И медиана holders==base ≥ 50 %. ВНИМАНИЕ: интерпретация «holders==base ⇒ не было публикаций с прошлого скана» НЕВАЛИДНА для U31 (публикация и освобождение pin возвращают refs к базе — см. раздел T2-семантики в B4_U31_TRIGGER.md).');
say('');
say('| Ячейка | SCAN_WORDS/op (медиана) | SCAN_WORDS/op (min–max) | holders==base медиана | holders==base min–max | Вердикт (формальный) | Близость к порогу 50 % |');
say('|---|---|---|---|---|---|---|');
for (const c of t2cells) {
  const near = c.holdersMedian >= 0.5 * 0.8
    ? 'близко: в пределах 20 % от порога'
    : 'не в пределах 20 % от порога';
  say(
    `| ${c.wl} T=${c.threads} | ${c.perOpMedian.toFixed(2)} | ` +
      `${c.perOpMin.toFixed(2)}–${c.perOpMax.toFixed(2)} | ` +
      `${fracOf(c.reps, { key: 'holders', v: c.holdersMedian }, 'holdersNum', 'holdersDen')} | ` +
      `${fracOf(c.reps, { key: 'holders', v: c.holdersMin }, 'holdersNum', 'holdersDen')} – ` +
      `${fracOf(c.reps, { key: 'holders', v: c.holdersMax }, 'holdersNum', 'holdersDen')} | ` +
      `${c.perOpMedian >= 1.0 && c.holdersMedian >= 0.5 ? 'OK' : 'НЕТ'} | ` +
      `${near} |`,
  );
}
say('');
say("Пометка «близко к порогу» — вычислено скриптом: медиана holders==base ≥ 50 % × 0.8 = 40 %. holders==base — формальное измерение с невалидной интерпретацией для U31; пометка только о близости ЧИСЛА к порогу, не о валидности измерения.");
say('');
say(`**Корректный верхний предел активации U31: доля u31_scan_no_pending / u31_discovery_scans — сканы discovery-сайта (owner-drain через drain_segment_sidecar в scan.rs; strict-trim/bounded сканы не входят) без единого ненулевого pending-слова** (именно такие сканы U31-skip смог бы пропустить). ОДНО множество числителя и знаменателя — u31_discovery_scans; holders==base ниже — другое измерение со знаменателем u31_scan_segments:`);
say('');
say('| Ячейка | no_pending/discovery_scans медиана | min–max |');
say('|---|---|---|');
for (const c of t2cells) {
  say(
    `| ${c.wl} T=${c.threads} | ` +
      `${fracOf(c.reps, { key: 'noPending', v: c.noPendingMedian }, 'noPendingNum', 'noPendingDen')} | ` +
      `${fracOf(c.reps, { key: 'noPending', v: c.noPendingMin }, 'noPendingNum', 'noPendingDen')} – ` +
      `${fracOf(c.reps, { key: 'noPending', v: c.noPendingMax }, 'noPendingNum', 'noPendingDen')} |`,
  );
}
say('');
say('Детали по повторам (числитель/знаменатель в каждой доле):');
say('');
for (const c of t2cells) {
  say(
    `- ${c.wl} T=${c.threads}: ` +
      c.reps
        .map(
          (r, i) =>
            `r${i + 1} holders ${pctStr(r.holdersNum, r.holdersDen)} (u31_holders_base/u31_scan_segments), no_pending ${pctStr(r.noPendingNum, r.noPendingDen)} (u31_scan_no_pending/u31_discovery_scans), discovery_scans ${fmt(r.discovery)}, SCAN_WORDS/op ${r.perOp.toFixed(2)} (ops_actual ${fmt(r.opsActual)})`,
        )
        .join('; '),
  );
}
say('');
say(`**Вердикт T2: ${T2_FIRED ? 'FIRED' : 'NOT FIRED'}** — по формальному определению (SCAN_WORDS/op ≥ 1.0 по ops_actual во всех ячейках: ${t2cells.every((c) => c.perOpMedian >= 1.0) ? 'да' : 'нет'}; медиана holders==base ≥ 50 %: ${t2cells.every((c) => c.holdersMedian >= 0.5) ? 'да' : 'нет'}). Формальная интерпретация holders==base для U31 невалидна — решение принимать по no_pending-доле (см. выше).`);
say('');

say('### T3 — доля discovery-скана (self, нижняя граница): larson/mstress ≥ 5 % ИЛИ growth16 ≥ 10 %');
say('');
say('| Рукав | Ir всего | self сумма (нижняя) | доля self | ≥ порога |');
say('|---|---|---|---|---|');
say(`| larson T=1 | ${fmt(larson1.ir)} | ${fmt(larson1.sumSelf)} | ${pctStr(larson1.sumSelf, larson1.ir)} | ${larson1.selfShare >= 0.05 ? 'да (5 %)' : 'нет'} |`);
say(`| mstress T=1 | ${fmt(mstress1.ir)} | ${fmt(mstress1.sumSelf)} | ${pctStr(mstress1.sumSelf, mstress1.ir)} | ${mstress1.selfShare >= 0.05 ? 'да (5 %)' : 'нет'} |`);
for (const r of t1rows) {
  say(`| growth16 n=${r.n} | ${fmt(r.sefer.total)} | ${fmt(r.sefer.sum)} | ${pctStr(r.sefer.sum, r.sefer.total)} | ${r.selfShare >= 0.1 ? 'да (10 %)' : 'нет'} |`);
}
say('| multiseg | — | — | не снимался в этих прогонах | — |');
say('');
say(`**Вердикт T3: ${T3_FIRED ? 'FIRED' : 'NOT FIRED'}** (growth16: ${t3growth}/3 рукавов ≥ 10 %; larson/mstress T=1: ${t3t1}/2 ≥ 5 %; ветка OR — достаточно одной).`);
say('');

say('### Оракул активации (вычисляется из COUNTERS, порог ≥ 95 %)');
say('');
say('Доля = scan_calls / find_segment_calls — каждый find_segment уходит в discovery-скан. Для MT-прогонов семантика (u31_scan_segments − scan_calls) > 0: u31_scan_segments срабатывает на входе scan_small_from на ВСЕХ owner-drain scan sites, scan_calls — только вызовы find_segment; разница = сканы из drain/free-путей вне find_segment; u31_discovery_scans — только discovery-сайт (drain_segment_sidecar в scan.rs). Assert-ы: u31_scan_segments ≥ scan_calls, discovery_scans ≤ segments, no_pending ≤ discovery_scans, holders_base ≤ segments, num ≤ den.');
say('');
say('| Прогон | scan_calls | find_segment_calls | доля | u31_scan_segments | no_pending | holders_base |');
say('|---|---|---|---|---|---|---|');
for (const n of NS) {
  const c = t1[`sefer:${n}`].counters;
  const u = t1[`sefer:${n}`].u31;
  say(`| growth16 n=${n} | ${fmt(c.scan_calls)} | ${fmt(c.find_segment_calls)} | ${((c.scan_calls / c.find_segment_calls) * 100).toFixed(2)} % | ${fmt(u.u31_scan_segments)} | ${fmt(u.u31_scan_no_pending)} | ${fmt(u.u31_holders_base)} |`);
}
for (const p of [larson1, mstress1]) {
  say(`| ${p.workload} T=1 | ${fmt(p.counters.scan_calls)} | ${fmt(p.counters.find_segment_calls)} | ${p.oracle >= 0.95 ? (p.oracle * 100).toFixed(2) : 'НИЖЕ ПОРОГА'} % | ${fmt(p.u31.u31_scan_segments)} | ${fmt(p.u31.u31_scan_no_pending)} | ${fmt(p.u31.u31_holders_base)} |`);
}
say('');
say('MT-ячейки (медианы по 3 повторам; доли вычислены из СВОИХ счётчиков, без констант; raw-сырьё ut-u31mt3-*):');
say('');
say('| Ячейка | scan_calls | find_segment_calls | доля scan/find | u31_scan_segments | u31_discovery_scans |');
say('|---|---|---|---|---|---|');
for (const c of t2cells) {
  const sc = median(c.reps.map((r) => r.scanCalls));
  const fc = median(c.reps.map((r) => r.findCalls));
  const seg = median(c.reps.map((r) => r.segments));
  const dis = median(c.reps.map((r) => r.discovery));
  const oracle = (sc / fc) * 100;
  assert(oracle >= 95, `MT ${c.wl} T=${c.threads}: оракул scan/find = ${oracle} ≥ 95 %`);
  say(`| ${c.wl} T=${c.threads} | ${fmt(sc)} | ${fmt(fc)} | ${oracle.toFixed(2)} % | ${fmt(seg)} | ${fmt(dis)} |`);
}
say('');
say('Мутационное доказательство счётчика no_pending: мутант «всегда пусто» краснеет — лог ut-u31mutant-nopending.log, exit=101 (тест u31_no_pending_counts_empty_scans_but_not_published_pending_scans, leg published).');
say('');
say('CSV: docs/perf/B4_U31_TRIGGER_summary.csv');

// ---------- CSV ----------
// фичи mimalloc-строк = фактические фичи бинаря (ut_prof_u31.sh: один бинарь
// собирается с этими фичами для обоих режимов)
const FEAT = 'production,bench-internals,internals';
// поле с запятыми/кавычками — в кавычках
const field = (s) => (/[",]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s);
const rows = [
  ['key', 'value', 'unit', 'source', 'features'],
  ['base_sha', BASE_SHA, '', 'константа базы фичевого дерева (= origin/main после PR #87/#88 на момент замера; см. рядом head_sha)', ''],
  ['head_sha', headSha, '', 'git rev-parse HEAD', FEAT],
  ['git_diff_sha256_t1t3', CHUNK1_SHA, '', 'ut-u31run4.log + ut-u31lar2/ut-u31ms2 (старое дерево, T1/T3)', FEAT],
  ['git_diff_sha256_mt', diffSha, '', 'ut-u31mt3-*.log (env B4_U31_DIFF_SHA256)', FEAT],
  ['rustc', rustcV, '', 'rustc --version', ''],
  ['cpu', cpuInfo, '', 'env B4_U31_CPU_INFO', ''],
  ['os', osInfo, '', 'env B4_U31_OS_INFO', ''],
  ['mt_repeats', String(REPS.length), 'count', `ut-u31mt3-* r1..r${REPS.length}`, FEAT],
  ['t1_ir_method', 'self_lower_plus_inclusive_upper', '', 'ut-u31run4.log SELF_IR + .annot inclusive', FEAT],
  ...t1rows.flatMap((r) => [
    [`t1_n${r.n}_ir_sefer`, String(r.sefer.total), 'Ir', `ut-u31run4.sefer.n${r.n}.out`, FEAT],
    [`t1_n${r.n}_ir_mimalloc`, String(r.mim.total), 'Ir', `ut-u31run4.mimalloc.n${r.n}.out`, FEAT],
    [`t1_n${r.n}_ratio`, r.ratio.toFixed(3), 'x', 'derived', FEAT],
    [`t1_n${r.n}_self_scan_rs`, String(r.sefer.scan), 'Ir', 'ut-u31run4.log SELF_IR', FEAT],
    [`t1_n${r.n}_self_bitmap_scan_rs`, String(r.sefer.bitmap), 'Ir', 'ut-u31run4.log SELF_IR', FEAT],
    [`t1_n${r.n}_self_sum_lower`, String(r.sefer.sum), 'Ir', 'ut-u31run4.log SELF_IR', FEAT],
    [`t1_n${r.n}_self_share_lower`, (r.selfShare * 100).toFixed(2), 'percent', 'derived', FEAT],
    [`t1_n${r.n}_inclusive_upper`, String(r.sefer.inclusive), 'Ir', `ut-u31run4.sefer.n${r.n}.annot`, FEAT],
    [`t1_n${r.n}_inclusive_share`, (r.inclShare * 100).toFixed(2), 'percent', 'derived', FEAT],
    [`t1_n${r.n}_u31_ceiling`, (r.ceiling * 100).toFixed(2), 'percent', 'derived (bitmap/self total)', FEAT],
  ]),
  ['t1_fired', T1_FIRED ? '1' : '0', 'bool', 'derived (n=131072 only, lower bound)', FEAT],
  ...[larson1, mstress1].flatMap((p) => [
    [`t3_${p.workload}_t1_ir`, String(p.ir), 'Ir', `ut-u31${p.workload === 'larson' ? 'lar2' : 'ms2'}.log`, FEAT],
    [`t3_${p.workload}_t1_self_sum_lower`, String(p.sumSelf), 'Ir', `ut-u31${p.workload === 'larson' ? 'lar2' : 'ms2'}.log SELF_IR`, FEAT],
    [`t3_${p.workload}_t1_self_share_lower`, (p.selfShare * 100).toFixed(2), 'percent', 'derived', FEAT],
    [`t3_${p.workload}_t1_inclusive_upper`, String(p.inclusive), 'Ir', `ut-u31${p.workload === 'larson' ? 'lar2' : 'ms2'}.${p.workload}.t1.s100000.annot`, FEAT],
    [`t3_${p.workload}_t1_inclusive_share`, (p.inclShare * 100).toFixed(2), 'percent', 'derived', FEAT],
    [`t3_${p.workload}_t1_u31_ceiling`, (p.ceiling * 100).toFixed(2), 'percent', 'derived', FEAT],
  ]),
  ...t2cells.flatMap((c) => [
    [`t2_${c.wl}_t${c.threads}_scanwords_per_op_median`, c.perOpMedian.toFixed(3), 'words/op', 'derived median r1-r3 (ops_actual)', FEAT],
    [`t2_${c.wl}_t${c.threads}_scanwords_per_op_min`, c.perOpMin.toFixed(3), 'words/op', 'derived min r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_scanwords_per_op_max`, c.perOpMax.toFixed(3), 'words/op', 'derived max r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_holders_base_median`, (c.holdersMedian * 100).toFixed(2), 'percent', 'derived median r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_holders_base_min`, (c.holdersMin * 100).toFixed(2), 'percent', 'derived min r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_holders_base_max`, (c.holdersMax * 100).toFixed(2), 'percent', 'derived max r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_u31_discovery_scans_min`, String(Math.min(...c.reps.map((r) => r.discovery))), 'count', 'derived min r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_u31_discovery_scans_max`, String(Math.max(...c.reps.map((r) => r.discovery))), 'count', 'derived max r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_u31_limit_median`, (c.noPendingMedian * 100).toFixed(2), 'percent', 'derived median r1-r3 (u31_scan_no_pending/u31_discovery_scans)', FEAT],
    [`t2_${c.wl}_t${c.threads}_u31_limit_min`, (c.noPendingMin * 100).toFixed(2), 'percent', 'derived min r1-r3', FEAT],
    [`t2_${c.wl}_t${c.threads}_u31_limit_max`, (c.noPendingMax * 100).toFixed(2), 'percent', 'derived max r1-r3', FEAT],
  ]),
  ['t2_fired_formal', T2_FIRED ? '1' : '0', 'bool', 'derived (formal definition; interpretation invalid for U31)', FEAT],
  ['t3_fired', T3_FIRED ? '1' : '0', 'bool', 'derived (OR-branch; multiseg not measured)', FEAT],
  ['oracle_growth16_min', Math.min(...NS.map((n) => t1[`sefer:${n}`].oracle)).toFixed(4), 'ratio', 'derived scan/find ≥ 0.95', FEAT],
  ['oracle_mt_min', Math.min(...t2cells.flatMap((c) => c.reps.map((r) => r.oracle))).toFixed(4), 'ratio', 'derived scan/find ≥ 0.95', FEAT],
  ['mutation_nopending_red', '1', 'bool', 'ut-u31mutant-nopending.log exit=101', FEAT],
  ['raw_mt_logs', mtFiles.map((f) => f.replace('.log', '')).join(' '), '', 'список MT-логов ut-u31mt3-*', FEAT],
];
const csv = rows.map((r) => r.map(field).join(',')).join('\n') + '\n';
// обратный парсинг: каждая строка должна иметь ровно столько полей, сколько в заголовке
function parseCsvLine(line) {
  const out2 = [];
  let cur = '';
  let inQ = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (inQ) {
      if (ch === '"' && line[i + 1] === '"') { cur += '"'; i++; }
      else if (ch === '"') inQ = false;
      else cur += ch;
    } else if (ch === '"') inQ = true;
    else if (ch === ',') { out2.push(cur); cur = ''; }
    else cur += ch;
  }
  out2.push(cur);
  return out2;
}
{
  const lines = csv.split('\n').filter((l) => l.length > 0);
  const ncols = parseCsvLine(lines[0]).length;
  for (const l of lines) {
    const got = parseCsvLine(l);
    assert(
      got.length === ncols,
      `CSV-строка имеет ${got.length} полей вместо ${ncols}: ${l.slice(0, 60)}…`,
    );
  }
}
writeFileSync(join(perfDir, 'B4_U31_TRIGGER_summary.csv'), csv);

console.log(out.join('\n'));
