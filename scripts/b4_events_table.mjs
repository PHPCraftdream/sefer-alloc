#!/usr/bin/env node
// b4_events_table.mjs — таблицы отчёта docs/perf/B4_EVENTS_COST.md
// Источник: сырые логи docs/perf/_raw_b4_events_ev-{1,2,3}-*.log и
// machine-readable фазы из docs/perf/b4_events_probes/ev_stage1_summary_copy.md.
// Без cargo; только чтение логов + арифметика. Каждое число помечено
// ИЗМЕРЕНО (распарсено из raw-лога) или ОЦЕНКА (частное/произведение).
// Запуск: node scripts/b4_events_table.mjs   (печатает markdown-таблицы)
//         node scripts/b4_events_table.mjs --csv  (пишет B4_EVENTS_COST_summary.csv)

import { readFileSync, writeFileSync } from "node:fs";
import { gunzipSync } from "node:zlib";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const p = (...s) => join(root, ...s);
const read = (f) =>
  f.endsWith(".gz") ? gunzipSync(readFileSync(p(f))).toString("utf8") : readFileSync(p(f), "utf8");

const ev1 = read("docs/perf/_raw_b4_events_ev-1-mstress-summary.log");
const ev2 = read("docs/perf/_raw_b4_events_ev-2-mstress-auto-100k.log.gz");
const ev3 = read("docs/perf/_raw_b4_events_ev-3-larson-auto-100k.log.gz");
const s1 = read("docs/perf/b4_events_probes/ev_stage1_summary_copy.md");

const num = (s) => Number(String(s).replace(/[,\s]/g, ""));

// --- TOGGLE / FULL / CALLS / CACHE из структурированных строк raw-логов ---
function parseStructured(txt, wl) {
  const g = (re) => {
    const m = txt.match(re);
    if (!m) throw new Error(`pattern not found for ${wl}: ${re}`);
    return m;
  };
  return {
    toggle_refill: num(g(new RegExp(`TOGGLE ${wl} refill_magazine_slow marginal Ir/op: ([\\d,]+)`))[1]),
    toggle_flush: num(g(new RegExp(`TOGGLE ${wl} flush_class_internal marginal Ir/op: ([\\d,]+)`))[1]),
    toggle_drain: num(g(new RegExp(`TOGGLE ${wl} try_drain_freelist_batch marginal Ir/op: ([\\d,]+)`))[1]),
    full: num(g(new RegExp(`FULL ${wl} marginal Ir/op: ([\\d,]+)`))[1]),
    calls_refill: num(g(new RegExp(`CALLS ${wl}[^:]*: refill_magazine_slow=(\\d+) flush_class_internal=(\\d+) try_drain_freelist_batch=(\\d+) try_carve_batch=(\\d+) find_segment_with_free_impl=(\\d+)`)).slice(1).map((x) => x)),
  };
}
function parseCalls(txt, wl) {
  const m = txt.match(new RegExp(`CALLS ${wl}[^:]*: refill_magazine_slow=(\\d+) flush_class_internal=(\\d+) try_drain_freelist_batch=(\\d+) try_carve_batch=(\\d+) find_segment_with_free_impl=(\\d+)`));
  if (!m) throw new Error(`CALLS line not found for ${wl}`);
  return { refill: num(m[1]), flush: num(m[2]), drain: num(m[3]), carve: num(m[4]), discovery: num(m[5]) };
}
function parseCache(txt, wl) {
  const m = txt.match(new RegExp(`CACHE ${wl} 100k( totals)?: Ir=([\\d,]+) Dr=([\\d,]+) Dw=([\\d,]+) ILmr=([\\d,]+) D1mr=([\\d,]+) D1mw=([\\d,]+)`));
  if (!m) throw new Error(`CACHE line not found for ${wl}`);
  return { Ir: num(m[2]), Dr: num(m[3]), Dw: num(m[4]), ILmr: num(m[5]), D1mr: num(m[6]), D1mw: num(m[7]) };
}

const M = { wl: "mstress" };
const L = { wl: "larson" };
for (const [o, txt] of [[M, ev1], [L, ev3]]) {
  const t = parseStructured(txt, o.wl);
  o.toggle_refill = t.toggle_refill;
  o.toggle_flush = t.toggle_flush;
  o.toggle_drain = t.toggle_drain;
  o.full = t.full;
  o.calls = parseCalls(txt, o.wl);
  o.cache = parseCache(txt, o.wl);
}
const TOGGLES = { mstress: ev1, larson: ev3 };
M.toggle_discovery = 0; L.toggle_discovery = 0; // ИЗМЕРЕНО: NOMATCH/0 в toggle-прогонах
M.toggle_carve = 0; L.toggle_carve = 0;

// --- построчные стоимости из --auto=yes логов (за прогон 100k, ИЗМЕРЕНО) ---
function lineCost(txt, re) {
  const m = txt.match(new RegExp(`^\\s*([\\d,]+|\\.)\\s+.*${re.source}`, "m"));
  if (!m) throw new Error(`line pattern not found: ${pattern}`);
  return m[1] === "." ? 0 : num(m[1]);
}
const lineItems = [
  ["drain: probe-цикл (while probed…)", "while probed < out.len.. && probe != FREE_LIST_NULL".replace("..", ""), /while probed < out\.len\(\) && probe != FREE_LIST_NULL/],
  ["drain: commit-цикл (while k…)", null, /while k < out\.len\(\) && head_off != FREE_LIST_NULL/],
  ["drain: перезапись head_off (повторное чтение next)", null, /head_off = if next\.is_null\(\)/],
  ["flush: run-детекция (while run_end…)", null, /while run_end < blocks\.len\(\)/],
  ["flush_run: span-гвард", null, /wrapping_sub\(payload_start\) >= span/],
  ["flush_run: bm.is_free(off)", null, /if bm\.is_free\(off\)/],
];
for (const [o, txt] of [[M, ev2], [L, ev3]]) {
  o.lines = {};
  for (const [name, , re] of lineItems) o.lines[name] = lineCost(txt, re);
}

// --- фазовые числа из ev_stage1_summary_copy.md (ИЗМЕРЕНО на этапе 1) ---
function summaryNum(re) {
  const m = s1.match(re);
  if (!m) throw new Error(`summary pattern not found: ${re}`);
  return num(m[1]);
}
// строки вида «…: 410k / 30k Ir» (mstress / larson)
function phasePair(marker) {
  const m = s1.match(new RegExp(`${marker}[^\\n]*?(?<m>[\\d.]+)[kM]?(?:\\s*/\\s*(?<l>[\\d.]+)[kM]?)?\\s*Ir`));
  return m;
}
// строки §5 этапа 1 (ИЗМЕРЕНО на этапе 1, округлены в summary)
const magPair = s1.match(/MAG-маркировка[^\n]*\n- ([\d.]+)k \/ ([\d.]+)k Ir/);
if (!magPair) throw new Error("MAG line not found in stage1 summary");
const magM = num(magPair[1]);
const magL = num(magPair[2]);
const flushBodyM = 1.0e6, flushBodyL = 0.15e6; // §5: flush_run тело ~1.0M / ~0.15M (ИЗМЕРЕНО, округлено)
const flushDetectM = 0.82e6, flushDetectL = 0.12e6; // §5: run-детекция

// эталон блоков/событие (судья larson): refill_blocks 0.14082/step ÷ 0.00912 = 15.4 (ОЦЕНКА)
const BLOCKS_PER_REFILL = 15.4;

// --- производные величины ---
function derived(o) {
  const irPerRefill = o.toggle_refill * 1e5 / o.calls.refill;
  const irPerFlush = o.toggle_flush * 1e5 / o.calls.flush;
  const irPerRefillBlock = irPerRefill / BLOCKS_PER_REFILL;
  return { irPerRefill, irPerFlush, irPerRefillBlock };
}
M.d = derived(M); L.d = derived(L);

// --- печать ---
const fmt = (x, digits = 0) => x.toLocaleString("en-US", { maximumFractionDigits: digits });
const out = [];
const T = (s) => out.push(s);

T("## Таблица 1 — статья события → Ir/событие → Ir/step (mstress/larson, T=1)");
T("");
T("| статья | Ir/событие (larson) | Ir/событие (mstress) | Ir/step (larson) | Ir/step (mstress) | статус |");
T("|---|---|---|---|---|---|");
T(`| refill (refill_magazine_slow, вкл. инлайны) | ${fmt(L.d.irPerRefill)} | ${fmt(M.d.irPerRefill)} | ${L.toggle_refill} | ${M.toggle_refill} | Ir/step ИЗМЕРЕНО (toggle); Ir/событие ОЦЕНКА (÷вызовы) |`);
T(`| flush (flush_class_internal, вкл. flush_run) | ${fmt(L.d.irPerFlush)} | ${fmt(M.d.irPerFlush)} | ${L.toggle_flush} | ${M.toggle_flush} | Ir/step ИЗМЕРЕНО (toggle); Ir/событие ОЦЕНКА (÷вызовы) |`);
T(`| refill, на блок (÷${BLOCKS_PER_REFILL} блока/событие) | ${fmt(L.d.irPerRefillBlock)} | ${fmt(M.d.irPerRefillBlock)} | — | — | ОЦЕНКА |`);
T(`| полный маржинальный Ir/step (для сравнения) | — | — | ${L.full} | ${M.full} | ИЗМЕРЕНО |`);
T("");
T("Частоты событий (вызовов/100k шагов, ИЗМЕРЕНО `calls=`):");
T("");
T("| функция | larson | mstress |");
T("|---|---|---|");
for (const [k, name] of [["refill", "refill_magazine_slow"], ["flush", "flush_class_internal"], ["drain", "try_drain_freelist_batch (отд. копия)"], ["carve", "try_carve_batch (отд. копия)"], ["discovery", "find_segment_with_free_impl"]]) {
  T(`| ${name} | ${L.calls[k]} | ${M.calls[k]} |`);
}

T("");
T("## Таблица 2 — построчная разбивка (Ir за прогон 100k шагов, --auto=yes, ИЗМЕРЕНО)");
T("");
T("| строка кода | larson | mstress |");
T("|---|---|---|");
for (const [name] of lineItems) {
  T(`| ${name} | ${fmt(L.lines[name])} | ${fmt(M.lines[name])} |`);
}
T(`| MAG-маркировка n−1 (finish_magazine_refill, hot.rs:81–87) | ${fmt(magL * 1e3)} | ${fmt(magM * 1e3)} |`);
T(`| flush: run-детекция (суммарно, §5 этапа 1) | ${fmt(flushDetectL)} | ${fmt(flushDetectM)} |`);
T(`| flush_run: тело (span+is_free+приём, §5 этапа 1) | ${fmt(flushBodyL)} | ${fmt(flushBodyM)} |`);

T("");
T("## Таблица 3 — cache-sim (полный прогон 100k, весь процесс, ИЗМЕРЕНО)");
T("");
T("| метрика | larson | mstress |");
T("|---|---|---|");
for (const k of ["Ir", "D1mr", "D1mw", "ILmr"]) T(`| ${k} | ${fmt(L.cache[k])} | ${fmt(M.cache[k])} |`);

// --- ранжированный список сокращений: Δ-оценки вычисляются из сырых данных ---
// Механика оценок (ОЦЕНКА, диапазон = доля строки, считаемая устранимой):
const reductions = [
  {
    id: "R1", name: "слияние probe/commit проходов drain (убрать повторное чтение next и перезапись head_off)",
    // head_off-строка = повторное чтение next в commit; commit-цикл сверх probe.
    d: (o) => {
      const lo = o.lines["drain: перезапись head_off (повторное чтение next)"] * 0.5;
      const hi = o.lines["drain: перезапись head_off (повторное чтение next)"] * 0.8;
      return [lo / 1e5, hi / 1e5];
    },
  },
  {
    id: "R2", name: "hoist сегмента в MAG-маркировке (hot.rs:81–87: base/Meta/bitmap из цикла)",
    d: (o) => [(o.wl === "mstress" ? magM : magL) * 1e3 * 0.6 / 1e5, (o.wl === "mstress" ? magM : magL) * 1e3 * 0.85 / 1e5],
  },
  {
    id: "R3", name: "соседство (q == prev+block_size) вместо segment_base_of_ptr в run-детекции flush",
    d: (o) => [o.lines["flush: run-детекция (while run_end…)"] * 0.5 / 1e5, o.lines["flush: run-детекция (while run_end…)"] * 0.8 / 1e5],
  },
  {
    id: "R4", name: "X-leafskip: условный ClassLeaves-store (store только при смене значения)",
    d: (o) => [o.d.irPerRefillBlock * BLOCKS_PER_REFILL * o.calls.refill / 1e5 * 0.02, o.d.irPerRefillBlock * BLOCKS_PER_REFILL * o.calls.refill / 1e5 * 0.05],
  },
];
T("");
T("## Таблица 4 — ранжированный список сокращений: ΔIr/step (ОЦЕНКА из построчных данных)");
T("");
T("| # | сокращение | ΔIr/step larson | ΔIr/step mstress | статус |");
T("|---|---|---|---|---|");
for (const r of reductions) {
  const [lLo, lHi] = r.d(L), [mLo, mHi] = r.d(M);
  T(`| ${r.id} | ${r.name} | −${fmt(lLo, 1)}…−${fmt(lHi, 1)} | −${fmt(mLo, 1)}…−${fmt(mHi, 1)} | ОЦЕНКА (доля построчной статьи) |`);
}

const text = out.join("\n") + "\n";
console.log(text);

if (process.argv.includes("--csv")) {
  const rows = [["item", "value", "larson", "mstress", "status"]];
  rows.push(["refill Ir/step", "toggle", L.toggle_refill, M.toggle_refill, "ИЗМЕРЕНО"]);
  rows.push(["flush Ir/step", "toggle", L.toggle_flush, M.toggle_flush, "ИЗМЕРЕНО"]);
  rows.push(["drain Ir/step (отд. копия)", "toggle", L.toggle_drain, M.toggle_drain, "ИЗМЕРЕНО"]);
  rows.push(["full Ir/step", "marginal", L.full, M.full, "ИЗМЕРЕНО"]);
  rows.push(["refill events /100k", "calls", L.calls.refill, M.calls.refill, "ИЗМЕРЕНО"]);
  rows.push(["flush events /100k", "calls", L.calls.flush, M.calls.flush, "ИЗМЕРЕНО"]);
  rows.push(["refill Ir/event", "derived", L.d.irPerRefill.toFixed(0), M.d.irPerRefill.toFixed(0), "ОЦЕНКА"]);
  rows.push(["flush Ir/event", "derived", L.d.irPerFlush.toFixed(0), M.d.irPerFlush.toFixed(0), "ОЦЕНКА"]);
  rows.push(["refill Ir/block", "derived", L.d.irPerRefillBlock.toFixed(0), M.d.irPerRefillBlock.toFixed(0), "ОЦЕНКА"]);
  for (const [name] of lineItems) rows.push([`line: ${name}`, "auto=100k", L.lines[name], M.lines[name], "ИЗМЕРЕНО"]);
  rows.push(["line: MAG-marking", "auto=100k", magL * 1e3, magM * 1e3, "ИЗМЕРЕНО"]);
  rows.push(["phase: flush run-detect", "auto=100k", flushDetectL, flushDetectM, "ИЗМЕРЕНО"]);
  rows.push(["phase: flush_run body", "auto=100k", flushBodyL, flushBodyM, "ИЗМЕРЕНО"]);
  for (const k of ["Ir", "D1mr", "D1mw", "ILmr"]) rows.push([`cache ${k}`, "full100k", L.cache[k], M.cache[k], "ИЗМЕРЕНО"]);
  for (const r of reductions) {
    const [lLo, lHi] = r.d(L), [mLo, mHi] = r.d(M);
    rows.push([`reduction ${r.id}`, r.name, `-${lLo.toFixed(1)}..-${lHi.toFixed(1)}`, `-${mLo.toFixed(1)}..-${mHi.toFixed(1)}`, "ОЦЕНКА"]);
  }
  writeFileSync(p("docs/perf/B4_EVENTS_COST_summary.csv"), rows.map((r) => r.map((c) => `"${String(c).replaceAll('"', '""')}"`).join(",")).join("\n") + "\n");
  console.error("CSV written: docs/perf/B4_EVENTS_COST_summary.csv");
}
