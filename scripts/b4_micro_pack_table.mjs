#!/usr/bin/env node
// b4_micro_pack_table.mjs — renders ALL B4-M micro-pack report tables
// (docs/perf/B4_MICRO_PACK.md §1, §2–§6.2b A/B tables, §9 mutants, §10 oracle)
// and fully rewrites docs/perf/B4_MICRO_PACK_summary.csv from the raw
// callgrind logs docs/perf/_raw_b4_micro_<step>_<judge>.log and
// docs/perf/_raw_b4_micro_counters.log. No cargo, no network.
//
// Fractional Ir: whenever a raw log carries an exact TOTALS pair
// ([t1=N t2=M] on the summary line of the regenerated logs, or the
// "TOTALS(50k)=.. TOTALS(100k)=.." line of the "regenerated alloc-rt
// section" in mc_xn2_aoc / mc_n92_aoc), the marginal is recomputed as
// (t2 - t1) / 50000 and printed with 2 decimals. Cells without an exact
// pair keep the integer from the summary line and are marked " int (±1)".
// int (±1) — ТОЛЬКО колонка «точность»/суффикс значения и пометка
// целевой int-строки; на арифметику порогов НЕ влияет (значение как есть).
//
// MERGE RULE (documented as required):
//   Rows "X-null" merges mc-xn and mc-xn2: mstress/larson/thrash64
//   (incl. EstCycles) come from mc_xn_*, judges ao/aoc/batch/batchc/batchl
//   come from mc_xn2_* (the newer re-measurement). If BOTH logs provide a
//   value for the same cell, the newer (mc-xn2) wins and a disagreement is
//   a hard error (exit 1). Analogously "N9 v3" merges mc-n9
//   (mstress/larson/thrash64) with mc-n92/mc-n92b (ao/aoc/batch*, batch
//   prefers mc_n92b_batch as the newer re-run; disagreement = exit 1).
//   N9 v1/v2 stay separate rows. All other steps parse their own logs.
//
// Exit code is 0 when all asserts pass (merge, delta, CSV re-check,
// report-vs-CSV verdict consistency, PREREG self-test). Step verdicts are
// COMPUTED from the single PREREG constant below — no other source of
// thresholds exists in this script.

import { readFileSync, existsSync, readdirSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const repoRoot = join(dirname(fileURLToPath(import.meta.url)), "..");
const rawDir = join(repoRoot, "docs", "perf");
const csvPath = join(rawDir, "B4_MICRO_PACK_summary.csv");

function fail(msg) {
  console.error(`ASSERT FAIL: ${msg}`);
  process.exit(1);
}
function assert(cond, msg) {
  if (!cond) fail(msg);
}
const r2 = (x) => Math.round(x * 100) / 100;

// ---------------------------------------------------------------------------
// Judge/column specs. `re` captures (intValue, t1?, t2?); a captured t1/t2
// pair means the marginal is recomputed exactly as (t2-t1)/50000.
// ---------------------------------------------------------------------------
const PAIR = "(?:\\s*\\[t1=(\\d+) t2=(\\d+)\\])?";
const COLS = [
  {
    judge: "mstress", token: "mstress",
    cols: [{ key: "op", label: "Ir/op", kind: "ir",
      re: new RegExp(`MARGINAL Ir/op \\(sefer mstress[^)]*\\):\\s*(\\d+)${PAIR}`) }],
  },
  {
    judge: "larson", token: "larson",
    cols: [{ key: "op", label: "Ir/op", kind: "ir",
      re: new RegExp(`MARGINAL Ir/op \\(sefer larson[^)]*\\):\\s*(\\d+)${PAIR}`) }],
  },
  {
    judge: "ao", token: "ao",
    cols: [
      { key: "alloc", label: "alloc Ir/op", kind: "ir",
        re: new RegExp(`== sefer w_alloc marginal Ir/op:\\s*(\\d+)${PAIR}`) },
      { key: "free", label: "free Ir/op", kind: "ir",
        re: new RegExp(`== sefer w_free marginal Ir/op:\\s*(\\d+)${PAIR}`) },
    ],
  },
  {
    judge: "aoc rt", token: "aoc",
    cols: [
      { key: "alloc", label: "alloc Ir/op", kind: "ir",
        // integer fallback; the exact value comes from the regenerated
        // alloc-rt TOTALS section (see parseLog)
        re: /== sefer w_alloc \(mode=rt\) marginal Ir\/op:\s*([\d.]+)/ },
      { key: "free", label: "free Ir/op", kind: "ir",
        re: /== sefer w_free \(mode=rt\) marginal Ir\/op:\s*(\d+)/ },
    ],
  },
  {
    judge: "aoc const16", token: "aoc",
    cols: [
      { key: "alloc", label: "alloc Ir/op", kind: "ir",
        re: /== sefer w_alloc16 \(mode=c16\) marginal Ir\/op:\s*(\d+)/ },
      { key: "free", label: "free Ir/op", kind: "ir",
        re: /== sefer w_free16 \(mode=c16\) marginal Ir\/op:\s*(\d+)/ },
    ],
  },
  {
    judge: "aoc const64", token: "aoc",
    cols: [
      { key: "alloc", label: "alloc Ir/op", kind: "ir",
        re: /== sefer w_alloc64 \(mode=c64\) marginal Ir\/op:\s*(\d+)/ },
      { key: "free", label: "free Ir/op", kind: "ir",
        re: /== sefer w_free64 \(mode=c64\) marginal Ir\/op:\s*(\d+)/ },
    ],
  },
  {
    judge: "batch", token: "batch",
    cols: [
      { key: "alloc", label: "alloc Ir/block", kind: "irblk",
        re: /== sefer w_alloc marginal Ir\/call \(256\/round\):\s*(\d+)/ },
      { key: "free", label: "free Ir/block", kind: "irblk",
        re: /== sefer w_free marginal Ir\/call \(256\/round\):\s*(\d+)/ },
    ],
  },
  {
    judge: "batchc", token: "batchc",
    cols: [
      { key: "alloc", label: "alloc Ir/block", kind: "irblk",
        re: /== sefer w_alloc16 \(const 16B\) marginal Ir\/call \(256\/round\):\s*(\d+)/ },
      { key: "free", label: "free Ir/block", kind: "irblk",
        re: /== sefer w_free16 \(const 16B\) marginal Ir\/call \(256\/round\):\s*(\d+)/ },
    ],
  },
  {
    judge: "batchl", token: "batchl",
    cols: [
      { key: "alloc", label: "alloc Ir/block", kind: "irblk",
        re: /== sefer w_alloc marginal Ir\/call \(258\/round\):\s*(\d+)/ },
      { key: "free", label: "free Ir/block", kind: "irblk",
        re: /== sefer w_free marginal Ir\/call \(258\/round\):\s*(\d+)/ },
    ],
  },
  {
    judge: "thrash64", token: "thrash64",
    cols: [
      { key: "alloc", label: "alloc Ir/op", kind: "ir",
        re: new RegExp(`== sefer w_alloc marginal Ir/op:\\s*(\\d+)${PAIR}`) },
      { key: "free", label: "free Ir/op", kind: "ir",
        re: new RegExp(`== sefer w_free marginal Ir/op:\\s*(\\d+)${PAIR}`) },
    ],
  },
  {
    judge: "thrash64 cyc", token: "thrash64",
    cols: [{ key: "cyc", label: "EstCycles/op free", kind: "cyc",
      re: /== sefer w_free cache:[^\n]*EstCycles\/op=([\d.]+)/ }],
  },
];
const JUDGES = COLS.map((j) => j.judge);

// A cell: { v: number|null, exact: boolean }
function parseLog(label) {
  const path = join(rawDir, `_raw_b4_micro_${label}.log`);
  if (!existsSync(path)) return null;
  const text = readFileSync(path, "utf8");
  const out = {};
  for (const spec of COLS) {
    if (!label.endsWith("_" + spec.token)) continue;
    for (const col of spec.cols) {
      const m = text.match(col.re);
      const slot = `${spec.judge}/${col.key}`;
      if (!m) { out[slot] = { v: null, exact: false }; continue; }
      const iv = parseFloat(m[1]);
      if (m[2] !== undefined && m[3] !== undefined) {
        out[slot] = { v: r2((Number(m[3]) - Number(m[2])) / 50000), exact: true };
      } else {
        out[slot] = { v: iv, exact: false };
      }
    }
  }
  // exact alloc-rt via the regenerated TOTALS section (mc_xn2_aoc / mc_n92_aoc)
  if (label.endsWith("_aoc") && text.includes("regenerated alloc-rt section")) {
    const t = text.match(/TOTALS\(50k\)=(\d+) TOTALS\(100k\)=(\d+)/);
    if (t) {
      assert(text.match(/== sefer w_alloc \(mode=rt\) marginal Ir\/op/),
        `${label}: regenerated alloc-rt section without an alloc (mode=rt) summary line`);
      out["aoc rt/alloc"] = { v: r2((Number(t[2]) - Number(t[1])) / 50000), exact: true };
    }
  }
  return out;
}

const logCache = new Map();
function getLog(label) {
  if (!logCache.has(label)) logCache.set(label, parseLog(label));
  return logCache.get(label);
}

// ---------------------------------------------------------------------------
// Merged summary rows (see MERGE RULE in the header comment).
// Each judge maps to a preference-ordered list of log labels; the FIRST log
// with a value wins, every later log that also has a value must agree
// (integer comparison; disagreement = exit 1).
// ---------------------------------------------------------------------------
function colSpec(judge, key) {
  const spec = COLS.find((c) => c.judge === judge);
  return spec.cols.find((c) => c.key === key);
}

const SUM_ROWS = [
  { name: "base (mc-base)", src: () => ({ mstress: ["mc_base_mstress"], larson: ["mc_base_larson"], ao: ["mc_base_ao"], "aoc rt": ["mc_base_aoc"], "aoc const16": ["mc_base_aoc"], "aoc const64": ["mc_base_aoc"], batch: ["mc_base_batch"], batchc: ["mc_base_batchc"], batchl: ["mc_base_batchl"], thrash64: ["mc_base_thrash64"], "thrash64 cyc": ["mc_base_thrash64"] }) },
  { name: "S1 (mc-s1)", src: () => ({ mstress: ["mc_s1_mstress"], larson: ["mc_s1_larson"], ao: ["mc_s1_ao"], "aoc rt": ["mc_s1_aoc"], "aoc const16": ["mc_s1_aoc"], "aoc const64": ["mc_s1_aoc"], batch: ["mc_s1_batch"], batchc: ["mc_s1_batchc"], batchl: ["mc_s1_batchl"], thrash64: ["mc_s1_thrash64"], "thrash64 cyc": ["mc_s1_thrash64"] }) },
  { name: "R3 (mc-r3)", src: () => ({ mstress: ["mc_r3_mstress"], larson: ["mc_r3_larson"], ao: ["mc_r3_ao"], "aoc rt": ["mc_r3_aoc"], "aoc const16": ["mc_r3_aoc"], "aoc const64": ["mc_r3_aoc"], batch: ["mc_r3_batch"], batchc: ["mc_r3_batchc"], batchl: ["mc_r3_batchl"], thrash64: ["mc_r3_thrash64"], "thrash64 cyc": ["mc_r3_thrash64"] }) },
  { name: "R3b (mc-r3b)", src: () => ({ mstress: ["mc_r3b_mstress"] }) },
  { name: "X-null", src: () => ({ mstress: ["mc_xn_mstress"], larson: ["mc_xn_larson"], ao: ["mc_xn2_ao"], "aoc rt": ["mc_xn2_aoc"], "aoc const16": ["mc_xn2_aoc"], "aoc const64": ["mc_xn2_aoc"], batch: ["mc_xn2_batch"], batchc: ["mc_xn2_batchc"], batchl: ["mc_xn2_batchl"], thrash64: ["mc_xn_thrash64"], "thrash64 cyc": ["mc_xn_thrash64"] }) },
  { name: "X-span (mc-xs)", src: () => ({ mstress: ["mc_xs_mstress"], larson: ["mc_xs_larson"], ao: ["mc_xs_ao"], "aoc rt": ["mc_xs_aoc"], "aoc const16": ["mc_xs_aoc"], "aoc const64": ["mc_xs_aoc"], batch: ["mc_xs_batch"], batchc: ["mc_xs_batchc"], batchl: ["mc_xs_batchl"], thrash64: ["mc_xs_thrash64"], "thrash64 cyc": ["mc_xs_thrash64"] }) },
  { name: "X-c1 (mc-xc1)", src: () => ({ mstress: ["mc_xc1_mstress"], larson: ["mc_xc1_larson"], ao: ["mc_xc1_ao"], "aoc rt": ["mc_xc1_aoc"], "aoc const16": ["mc_xc1_aoc"], "aoc const64": ["mc_xc1_aoc"], batch: ["mc_xc1_batch"], batchc: ["mc_xc1_batchc"], batchl: ["mc_xc1_batchl"], thrash64: ["mc_xc1_thrash64"], "thrash64 cyc": ["mc_xc1_thrash64"] }) },
  { name: "X-c1b (mc-xc1b)", src: () => ({ mstress: ["mc_xc1b_mstress"], larson: ["mc_xc1b_larson"], ao: ["mc_xc1b_ao"], "aoc rt": ["mc_xc1b_aoc"], "aoc const16": ["mc_xc1b_aoc"], "aoc const64": ["mc_xc1b_aoc"], batch: ["mc_xc1b_batch"], batchc: ["mc_xc1b_batchc"], batchl: ["mc_xc1b_batchl"], thrash64: ["mc_xc1b_thrash64"], "thrash64 cyc": ["mc_xc1b_thrash64"] }) },
  { name: "N9 v3 (mc-n9)", src: () => ({ mstress: ["mc_n9_mstress"], larson: ["mc_n9_larson"], ao: ["mc_n92_ao"], "aoc rt": ["mc_n92_aoc"], "aoc const16": ["mc_n92_aoc"], "aoc const64": ["mc_n92_aoc"], batch: ["mc_n92b_batch", "mc_n92_batch"], batchc: ["mc_n92_batchc"], batchl: ["mc_n92_batchl"], thrash64: ["mc_n9_thrash64"], "thrash64 cyc": ["mc_n9_thrash64"] }) },
  { name: "N9 v1 (mc-n9v1) отклонён", src: () => ({ mstress: ["mc_n9v1_mstress"], larson: ["mc_n9v1_larson"], thrash64: ["mc_n9v1_thrash64"], "thrash64 cyc": ["mc_n9v1_thrash64"] }) },
  { name: "N9 v2 (mc-n9v2) отклонён", src: () => ({ mstress: ["mc_n9v2_mstress"], larson: ["mc_n9v2_larson"], thrash64: ["mc_n9v2_thrash64"], "thrash64 cyc": ["mc_n9v2_thrash64"] }) },
];

// merged cell for one row/judge: { cols: { key: cell }, anyInt }
function mergedCell(rowName, judge) {
  const labels = rowName.src()[judge] || [];
  const spec = COLS.find((c) => c.judge === judge);
  const cols = {};
  let present = false;
  for (const col of spec.cols) {
    const slot = `${judge}/${col.key}`;
    let winner = null;
    for (const label of labels) {
      const lg = getLog(label);
      if (!lg || !lg[slot] || lg[slot].v === null) continue;
      if (winner === null) {
        winner = { ...lg[slot], from: label };
      } else {
        // conflict check: a later (newer) log that also has the value must agree
        assert(winner.v === lg[slot].v,
          `${rowName} / ${judge} / ${col.key}: merged logs disagree (${winner.from}=${winner.v} vs ${label}=${lg[slot].v})`);
        // newer log wins for the exact/int flag
        winner = { ...lg[slot], from: label };
      }
    }
    cols[col.key] = winner;
    if (winner) present = true;
  }
  return present ? { cols } : null;
}

function fmtCellVal(cell, key) {
  if (!cell) return null;
  const c = cell.cols[key];
  if (!c) return null;
  const isCyc = key === "cyc";
  if (c.exact || isCyc) return String(c.v % 1 === 0 && isCyc ? c.v.toFixed(1) : c.v);
  return `${c.v} int (±1)`;
}

// ---------------------------------------------------------------------------
// PREREG — единственный источник порогов в скрипте.
//
// target: целевое условие шага. Альтернативы (alts) объединяются указанным
//   способом (and/or): цель выполнена, если выполнена хотя бы одна
//   (для and — все). Проигравшая альтернатива сама по себе НЕ является FAIL.
//   alt: { judge, key, ir: N }  — улучшение base−variant ≥ N Ir;
//        { judge, key, pct: P } — улучшение ≥ P % БАЗЫ (base*(P/100)).
// regress: общий словарь нерегресс-допусков по судьям. Применяется к каждому
//   судье шага, присутствующему в сырье, КРОМЕ целевых судей шага.
//   Проверка: variant ≤ base + tol  (для pct: variant ≤ base*(1+p/100), т.е.
//   +0.2 % ОТ БАЗЫ).
//
// int (±1) — НЕ допуск: порог применяется к значению как есть.
// ---------------------------------------------------------------------------
const PREREG = {
  // целевые условия шагов
  targets: {
    S1: {
      combine: "and", reject: null,
      alts: [
        { judge: "mstress", key: "op", pct: 0.3 },
        { judge: "larson", key: "op", ir: 0 },
      ],
    },
    R3: {
      combine: "and", reject: "NO-GO",
      alts: [{ judge: "mstress", key: "op", pct: 0.3 }],
    },
    R3b: {
      combine: "and", reject: "NO-GO",
      alts: [{ judge: "mstress", key: "op", pct: 0.3 }],
    },
    "X-null": {
      combine: "or", reject: "NO-GO",
      alts: [
        { judge: "ao", key: "free", ir: 1 },
        { judge: "aoc rt", key: "alloc", ir: 1 },
      ],
    },
    "X-span": {
      combine: "or", reject: "NO-GO",
      alts: [
        { judge: "ao", key: "free", ir: 1 },
        { judge: "aoc rt", key: "alloc", ir: 1 },
      ],
    },
    "X-c1": {
      combine: "or", reject: "NO-GO",
      alts: [
        { judge: "ao", key: "free", ir: 1 },
        { judge: "aoc rt", key: "alloc", ir: 1 },
      ],
    },
    "X-c1b": {
      combine: "or", reject: "NO-GO",
      alts: [
        { judge: "ao", key: "free", ir: 1 },
        { judge: "aoc rt", key: "alloc", ir: 1 },
      ],
    },
    "N9 v1": {
      combine: "and", reject: "rejected",
      alts: [{ judge: "thrash64", key: "free", ir: 2 }],
    },
    "N9 v2": {
      combine: "and", reject: "rejected",
      alts: [{ judge: "thrash64", key: "free", ir: 2 }],
    },
    "N9 v3": {
      combine: "and", reject: "NO-GO",
      alts: [{ judge: "thrash64", key: "free", ir: 2 }],
    },
  },
  // общий словарь нерегресс-допусков (variant ≤ base + tol Ir, pct: от базы)
  regress: [
    { judges: ["mstress", "larson"], keys: ["op"], tol: 0, pct: 0.2 },
    { judges: ["ao", "aoc rt", "aoc const16", "aoc const64"], keys: ["alloc", "free"], tol: 0 },
    { judges: ["batch", "batchc", "batchl"], keys: ["alloc", "free"], tol: 1 },
    { judges: ["thrash64"], keys: ["alloc", "free"], tol: 1 },
    { judges: ["thrash64 cyc"], keys: ["cyc"], tol: 1 },
  ],
};

// Целевые (judge,key) пары шага — исключаются из нерегресс-проверки.
function targetCellsOf(stepName) {
  const t = PREREG.targets[stepName];
  return t ? t.alts.map((a) => [a.judge, a.key]) : [];
}
function isTargetCell(stepName, judge, key) {
  return targetCellsOf(stepName).some(([j, k]) => j === judge && k === key);
}

// Целевая альтернатива выполнена?
function altMet(alt, base, variant) {
  if (base === null || variant === null) return null; // не измерено
  const d = r2(base - variant); // улучшение
  if (alt.pct !== undefined) return d >= r2(base * alt.pct) / 100;
  return d >= alt.ir;
}

// Нерегресс-нарушение? (null = строка не входит в проверку)
function regressFail(judge, key, base, variant) {
  if (base === null || variant === null) return null;
  const e = PREREG.regress.find(
    (e) => e.judges.includes(judge) && e.keys.includes(key));
  if (!e) throw new Error(`PREREG.regress: нет допуска для ${judge}/${key}`);
  if (e.pct !== undefined) return variant > base * (1 + e.pct / 100);
  return variant > base + e.tol;
}

function regressLabel(judge, key) {
  const e = PREREG.regress.find(
    (e) => e.judges.includes(judge) && e.keys.includes(key));
  if (e.pct !== undefined) return `≤ +${e.pct} % базы`;
  return `≤ +${e.tol} Ir${e.judges.some((j) => j.startsWith("batch")) ? "/блок" : ""}`;
}
function targetLabel(alt) {
  if (alt.pct !== undefined)
    return `${alt.judge} ${alt.key} ≤ −${alt.pct} % базы`;
  return `${alt.judge} ${alt.key} ≤ −${alt.ir} Ir`;
}

// Вердикт шага из вычисленных строк: GO ⇔ цель выполнена И нет FAIL.
// rows: [{ role: "target"|"regress", met: bool|null, fail: bool|null }]
function computeOverall(stepName, rows) {
  const t = PREREG.targets[stepName];
  const metRows = rows.filter((r) => r.role === "target");
  // Цель выполнена ⇔ для and — КАЖДАЯ конфигурированная альтернатива имеет
  // присутствующую строку с met === true (отсутствующая ячейка = не met);
  // для or — достаточно одной met === true (отсутствующие не считаются).
  const targetMet = t.combine === "or"
    ? metRows.some((r) => r.met === true)
    : t.alts.every((alt) => metRows.some(
        (r) => r.judge === alt.judge && r.key === alt.key && r.met === true));
  const anyFail = rows.some((r) => r.role === "regress" && r.fail === true);
  if (targetMet && !anyFail) return "GO";
  return t.reject || "NO-GO";
}

// ---------------------------------------------------------------------------
// Merged rows by display name
// ---------------------------------------------------------------------------
const ROWS_BY_NAME = {};
for (const r of SUM_ROWS) ROWS_BY_NAME[r.name.split(" (")[0]] = r;
ROWS_BY_NAME["base"] = SUM_ROWS[0];
ROWS_BY_NAME["S1"] = SUM_ROWS[1];
ROWS_BY_NAME["R3"] = SUM_ROWS[2];
ROWS_BY_NAME["X-null"] = SUM_ROWS[4];
ROWS_BY_NAME["X-span"] = SUM_ROWS[5];
ROWS_BY_NAME["N9 v3"] = SUM_ROWS[8];

function cellOf(rowName, judge, key) {
  const row = ROWS_BY_NAME[rowName];
  assert(row, `unknown summary row ${rowName}`);
  const cell = mergedCell(row, judge);
  if (!cell || !cell.cols[key] || cell.cols[key].v === null) return null;
  return cell.cols[key];
}
function num(rowName, judge, key) {
  const c = cellOf(rowName, judge, key);
  return c ? c.v : null;
}

function fmtVal(c, kind) {
  if (!c) return "—";
  if (kind === "cyc") return String(c.v);
  return c.exact ? String(c.v) : `${c.v} int (±1)`;
}

const INT_NOTE = " (порог проверяется точным iai на CI)";
const INT_VERDICT_NOTE = "int (±1): локально; приёмка по точному iai на CI";

// Единая функция: вычисляет строки шага (для A/B-таблицы И для CSV) и overall.
// Строки — все 11 судей с значениями в merged-логах (base и variant),
// целевые ячейки проверяются по цели, остальные — по нерегресс-допускам.
function computeStepRows(stepName, aRow, bRow) {
  const tgt = PREREG.targets[stepName];
  const rows = [];
  for (const judge of JUDGES) {
    for (const col of COLS.find((c) => c.judge === judge).cols) {
      const key = col.key;
      const ca = cellOf(aRow, judge, key);
      const cb = cellOf(bRow, judge, key);
      const isTgt = isTargetCell(stepName, judge, key);
      if (!ca || !cb) {
        // Ячейка отсутствует в сырье. Для целевой строки НЕ молча пропускаем:
        // отсутствующая альтернатива = не выполнена (и для "and", и для "or").
        if (isTgt) {
          const alt = tgt.alts.find((a2) => a2.judge === judge && a2.key === key);
          rows.push({ judge, key, label: col.label, role: "target",
            a: null, b: null, d: "—", thr: targetLabel(alt), prec: "—",
            verdict: "нет данных (target)", met: false, fail: null,
            target: true, csvVerdict: "нет данных (target)",
            exactA: true, exactB: true });
        }
        continue; // нерегресс-ячейка отсутствует в сырье — не входит
      }
      const a = ca.v, b = cb.v;
      const d = r2(a - b);
      const isInt = (c) => c && !c.exact && col.kind !== "cyc";
      const prec = isInt(ca) || isInt(cb) ? "int (±1)" : "exact";
      let role, thr, verdict, met = null, fail = null;
      if (isTgt) {
        role = "target";
        const alt = tgt.alts.find((a2) => a2.judge === judge && a2.key === key);
        met = altMet(alt, a, b);
        thr = targetLabel(alt);
        verdict = met ? "GO (target)" : "не выполнена (target)";
      } else {
        role = "regress";
        thr = regressLabel(judge, key);
        fail = regressFail(judge, key, a, b);
        verdict = fail ? "FAIL" : "ok";
      }
      rows.push({ judge, key, label: col.label, role, a, b, d, thr, prec,
        verdict, met, fail, target: isTgt,
        exactA: !isInt(ca), exactB: !isInt(cb) });
      // вердикт в CSV-формате (та же функция — тот же набор строк)
      rows[rows.length - 1].csvVerdict =
        role === "target" && prec === "int (±1)" && met
          ? `GO (target; ${INT_VERDICT_NOTE})` : verdict;
    }
  }
  return { rows, overall: computeOverall(stepName, rows) };
}

// ---------------------------------------------------------------------------
// Section rendering
// ---------------------------------------------------------------------------
const L = [];
const out = (s = "") => L.push(s);

function section1() {
  out("### §1 Сводная: шаг × судья (рендер `scripts/b4_micro_pack_table.mjs`)");
  out("");
  out("Дробные значения — пересчёт по точной паре TOTALS из сырого лога");
  out("(`[t1=… t2=…]` либо секция «regenerated alloc-rt», маргинал =");
  out("(t2−t1)/50000, 2 знака). Суффикс `int (±1)` — в логе только целое из");
  out("summary-строки (старые prof-логи без точных пар).");
  out("");
  out("| шаг | mstress | larson | ao a/f | aoc rt a/f | aoc c16 a/f | aoc c64 a/f | batch a/f | batchc a/f | batchl a/f | thrash64 a/f | thrash64 cyc(free) |");
  out("|---|---|---|---|---|---|---|---|---|---|---|---|");
  for (const row of SUM_ROWS) {
    const pair = (judge, ka, kb) => {
      const cell = mergedCell(row, judge);
      if (!cell) return "—";
      const spec = COLS.find((c) => c.judge === judge);
      const f = (k) => fmtCellVal(cell, k);
      if (spec.cols.length === 1) return f(ka) ?? "—";
      return `${f(ka) ?? "—"}/${f(kb) ?? "—"}`;
    };
    const cells = [
      pair("mstress", "op"), pair("larson", "op"),
      pair("ao", "alloc", "free"),
      pair("aoc rt", "alloc", "free"),
      pair("aoc const16", "alloc", "free"),
      pair("aoc const64", "alloc", "free"),
      pair("batch", "alloc", "free"),
      pair("batchc", "alloc", "free"),
      pair("batchl", "alloc", "free"),
      pair("thrash64", "alloc", "free"),
      pair("thrash64 cyc", "cyc"),
    ];
    out(`| ${row.name} | ${cells.join(" | ")} |`);
  }
  out("");
}

const STEPS = [
  { name: "S1", sec: "2", aRow: "base", bRow: "S1", threshold: "S1: mstress ≤ −0.3 % И larson ≤ 0; остальные — нерегресс" },
  { name: "R3", sec: "6.1", aRow: "S1", bRow: "R3", threshold: "mstress ≤ −0.3 %" },
  { name: "X-null", sec: "3", aRow: "S1", bRow: "X-null", threshold: "ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir" },
  { name: "X-span", sec: "4", aRow: "X-null", bRow: "X-span", threshold: "ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir" },
  { name: "X-c1", sec: "6.2", aRow: "X-span", bRow: "X-c1", threshold: "ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir" },
  { name: "X-c1b", sec: "6.2b", aRow: "X-span", bRow: "X-c1b", threshold: "ao free ≤ −1 Ir ИЛИ aoc rt alloc ≤ −1 Ir" },
  { name: "N9 v3", sec: "5", aRow: "X-span", bRow: "N9 v3", threshold: "thrash64 free ≤ −2 Ir" },
];
const SUMMARY_STEPS = [
  ...STEPS,
  { name: "N9 v1", sec: null, aRow: "X-span", bRow: "N9 v1", threshold: "thrash64 free ≤ −2 Ir" },
  { name: "N9 v2", sec: null, aRow: "X-span", bRow: "N9 v2", threshold: "thrash64 free ≤ −2 Ir" },
];

const SECTION_NO = Object.fromEntries(STEPS.map((s) => [s.name, s.sec]));
function renderAB(st) {
  const { rows, overall } = computeStepRows(st.name, st.aRow, st.bRow);
  out(`### §${st.sec} A/B: ${st.name} (A = ${st.aRow}, B = ${st.bRow}) — порог: ${st.threshold}`);
  out("");
  out("| судья | метрика | A | B | Δ (base−variant) | порог | точность | вердикт |");
  out("|---|---|---|---|---|---|---|---|");
  for (const r of rows) {
    const kindA = colSpec(r.judge, r.key).kind;
    let thr = r.thr;
    let vstr = r.verdict;
    if (r.role === "target") {
      if (r.prec === "int (±1)") thr += INT_NOTE;
      if (r.prec === "int (±1)" && r.met) vstr = `GO (target; ${INT_VERDICT_NOTE})`;
    }
    const astr = fmtVal(cellOf(st.aRow, r.judge, r.key), kindA);
    const bstr = fmtVal(cellOf(st.bRow, r.judge, r.key), kindA);
    out(`| ${r.judge} | ${r.label} | ${astr} | ${bstr} | ${r.d} | ${thr} | ${r.prec} | ${vstr} |`);
  }
  out("");
  out(`**Вердикт ${st.name}: ${overall}.**`);
  out("");
  return overall;
}
function sectionAB() {
  for (const st of STEPS) renderAB(st);
}

// ---------------------------------------------------------------------------
// PREREG self-test — синтетические проверки функции вердикта (всегда).
// ---------------------------------------------------------------------------
function preregSelfTest() {
  const mk = (role, met = null, fail = null) => ({ role, met, fail });
  // (1) одна альтернатива цели прошла, другая нет, нерегрессы чисты → GO
  {
    const rows = [
      { role: "target", met: true, fail: null },   // ao free: improvement ≥ 1
      { role: "target", met: false, fail: null },  // aoc rt alloc: не прошла — НЕ FAIL
      { role: "regress", met: null, fail: false },
      { role: "regress", met: null, fail: false },
    ];
    assert(computeOverall("X-null", rows) === "GO",
      "selftest 1: X-null-like (одна альтернатива прошла, нерегрессы чисты) должен быть GO");
  }
  // (2) цель прошла, одна нерегресс-строка FAIL → NO-GO
  {
    const rows = [
      { role: "target", met: true, fail: null },
      { role: "regress", met: null, fail: false },
      { role: "regress", met: null, fail: true },
    ];
    assert(computeOverall("X-span", rows) === "NO-GO",
      "selftest 2: цель прошла, есть FAIL-нерегресс → должен быть NO-GO");
  }
  // (3) mstress-допуск: variant ≤ base*1.002
  assert(regressFail("mstress", "op", 100, 100.19) === false,
    "selftest 3a: mstress 100.19 при базе 100 не должен быть FAIL");
  assert(regressFail("mstress", "op", 100, 100.21) === true,
    "selftest 3b: mstress 100.21 при базе 100 должен быть FAIL");
  // дополнительно: altMet на процентах и Ir
  assert(altMet({ judge: "mstress", key: "op", pct: 0.3 }, 380, 372) === true,
    "selftest 4a: mstress −8 при базе 380 ≥ 0.3 % → альтернатива выполнена");
  assert(altMet({ judge: "mstress", key: "op", pct: 0.3 }, 380, 379) === false,
    "selftest 4b: mstress −1 при базе 380 < 1.14 → альтернатива не выполнена");
  assert(altMet({ judge: "thrash64", key: "free", ir: 2 }, 84, 81.43) === true,
    "selftest 4c: thrash64 free −2.57 ≥ 2 → альтернатива выполнена");
  // (5) AND-цель: одна альтернатива отсутствует в сырье (строки нет),
  // вторая met:true → НЕ GO, даже при чистых нерегрессах.
  {
    const rows = [
      // larson (вторая альтернатива S1): met:true
      { role: "target", judge: "larson", key: "op", met: true, fail: null },
      // mstress (первая альтернатива S1): строки в сырье нет — не попадает в rows
      { role: "regress", met: null, fail: false },
    ];
    assert(computeOverall("S1", rows) !== "GO",
      "selftest 5: AND-цель с отсутствующей альтернативой не должна быть GO");
  }
  // (6) OR-цель: одна альтернатива отсутствует, другая met:true → GO
  {
    const rows = [
      { role: "target", judge: "ao", key: "free", met: true, fail: null },
      // aoc rt alloc: строки нет
      { role: "regress", met: null, fail: false },
    ];
    assert(computeOverall("X-null", rows) === "GO",
      "selftest 6: OR-цель, одна альтернатива отсутствует, другая met → GO");
  }
  out("### Self-test PREREG (встроенный, всегда): пройден");
  out("");
}

// ---------------------------------------------------------------------------
// Oracle table (§10) from _raw_b4_micro_counters.log
// ---------------------------------------------------------------------------
function parseCounters() {
  const text = readFileSync(join(rawDir, "_raw_b4_micro_counters.log"), "utf8");
  const runs = text.split("-- run 2 (determinism check) --");
  const parse = (t) => ({
    s1n: t.match(/S1 numerator[^=]*= (\d+)/)?.[1],
    s1d: t.match(/S1 denominator.* = (\d+)/)?.[1],
    s1s: t.match(/S1 share = numerator \/ denominator = ([\d.]+)%/)?.[1],
    won: t.match(/N9_WON_PROBE_EXACT \(delta\) = (\d+)/)?.[1],
    fills: t.match(/N9_MEMO_FILLS_OWNCACHE_HIT \(delta\) = (\d+)/)?.[1],
    memo: t.match(/N9_MEMO_HITS \(delta\) = (\d+)/)?.[1],
    own: t.match(/N9_OWNCACHE_HITS \(delta\) = (\d+)/)?.[1],
    fshN: t.match(/N9 fill share = fills \/ \(fills \+ won_probe_exact\) = (\d+) \/ (\d+) = ([\d.]+)%/),
    msh: t.match(/memo-hit share = memo \/ \(memo \+ own\) = ([\d.]+)%/)?.[1],
    xnTotal: t.match(/XNULL_NULL_DEALLOC_TOTAL \(delta\) = (\d+)/)?.[1],
    xnTail: t.match(/XNULL_NULL_TAIL_GUARD \(delta\) = (\d+)/)?.[1],
    xsCalls: t.match(/XSPAN_MASK_BRANCH_CALLS \(delta\) = (\d+)/)?.[1],
    xsTaken: t.match(/XSPAN_MASK_BRANCH_TAKEN \(delta\) = (\d+)/)?.[1],
  });
  const r1 = parse(runs[0]);
  if (runs[1]) {
    const r2v = parse(runs[1]);
    assert(JSON.stringify(r1) === JSON.stringify(r2v),
      "counters log: run 1 and run 2 disagree (determinism check)");
  }
  return r1;
}

function sectionOracle() {
  const c = parseCounters();
  assert(c.s1n === c.s1d, `S1 oracle: numerator ${c.s1n} != denominator ${c.s1d}`);
  assert(c.xnTail === c.xnTotal, `X-null oracle: tail ${c.xnTail} != total ${c.xnTotal}`);
  const fills = Number(c.fills), won = Number(c.won);
  assert(c.fshN && Number(c.fshN[1]) === fills && Number(c.fshN[2]) === fills + won,
    `N9 oracle: fill share denominator ${c.fshN ? c.fshN[2] : "?"} != fills+won = ${fills + won}`);
  const memoShare = r2((100 * Number(c.memo)) / (Number(c.memo) + Number(c.own)));
  assert(String(memoShare) === c.msh,
    `N9 oracle: memo-share computed ${memoShare} != log ${c.msh}`);
  out("### §10 Оракулы активации (рендер `scripts/b4_micro_pack_table.mjs`, лог `_raw_b4_micro_counters.log`)");
  out("");
  out("| приём | числитель | знаменатель | доля |");
  out("|---|---|---|---|");
  out(`| S1 | ${c.s1n} (MAG-записи refill-блоков минус события) | ${c.s1d} (retained = сумма n_i−1) | ${c.s1s}% |`);
  out(`| X-null | tail guard в \`publish_foreign\` = ${c.xnTail} | ${c.xnTotal} | 100.00% |`);
  out(`| X-span | маска-ветка taken = ${c.xsTaken} | calls = ${c.xsCalls} (small 512 + Large 4) | ${r2((100 * Number(c.xsTaken)) / Number(c.xsCalls))}% (4 Large-вызова не в маске-ветке — по построению) |`);
  out(`| N9 | fill при own_cache-хите = ${c.fills} | 2 потенциальных fill-события (won-probe exact ${c.won} + own_cache-hit fill ${c.fills}) | ${c.fshN[3]}% — это доля отличий политики заполнения, а не активация ветки; раскладка попаданий memo:own_cache = ${c.memo}:${c.own} (memo-share ${c.msh}%) |`);
  out("");
  out("Оба прогона счётчиков в логе совпадают побитово (determinism check пройден ассертом скрипта).");
  out("");
}

// ---------------------------------------------------------------------------
// CSV
// ---------------------------------------------------------------------------
function csvRows() {
  const rows = [];

  // base reference rows
  for (const judge of JUDGES) {
    for (const col of COLS.find((c) => c.judge === judge).cols) {
      const v = num("base", judge, col.key);
      const cellStr = fmtVal(cellOf("base", judge, col.key), col.kind);
      rows.push(["base", "mc-base", judge, col.label, cellStr, cellStr, v === null ? "" : "0", "=base", col.kind === "cyc" ? "exact" : cellOf("base", judge, col.key)?.exact ? "exact" : "int (±1)", "", "GO (reference)"]);
    }
  }

  const csvStep = (st) => {
    const { rows: stepRows } = computeStepRows(st.name, st.aRow, st.bRow);
    for (const r of stepRows) {
      const intMark = (side, exact) => (side === null ? "—"
        : r.prec === "int (±1)" && !exact ? `${side} int (±1)` : `${side}`);
      let thr = r.thr.replace(/≤/g, "<=");
      if (r.role === "target" && r.prec === "int (±1)")
        thr += " (порог проверяется точным iai на CI)";
      rows.push([st.name, st.bRow.toLowerCase().replace(/\s/g, "-"), r.judge, r.label,
        intMark(r.a, r.exactA), intMark(r.b, r.exactB), String(r.d), thr, r.prec,
        r.role === "target" ? "target" : "", r.csvVerdict]);
    }
  };

  for (const st of SUMMARY_STEPS) {
    if (!ROWS_BY_NAME[st.bRow]) continue; // row has no merged data
    csvStep(st);
  }
  return rows;
}

// Значение из CSV-ячейки ("12.5 int (±1)" → 12.5)
function parseCsvNum(s) {
  if (s === "—" || s === "") return null;
  return parseFloat(s.replace(" int (±1)", ""));
}

// (а) пересчёт каждой строки CSV из сырья равен записанному
function verifyCsvAgainstRaw() {
  const text = readFileSync(csvPath, "utf8");
  const lines = text.split("\n").filter((l) => l.length > 0);
  const header = lines[0].split(",");
  assert(header.join(",") === "step,label,judge,metric,base,variant,delta,threshold,precision,target,verdict",
    `CSV header mismatch: ${header.join(",")}`);
  const expectedByStep = {};
  for (const st of SUMMARY_STEPS) {
    if (!ROWS_BY_NAME[st.bRow]) continue;
    expectedByStep[st.name] = computeStepRows(st.name, st.aRow, st.bRow);
  }
  let checked = 0;
  const perStepVerdicts = {};
  for (const line of lines.slice(1)) {
    // разбор с учётом кавычек
    const cells = [];
    let cur = "", inQ = false;
    for (let i = 0; i < line.length; i++) {
      const ch = line[i];
      if (inQ) {
        if (ch === '"' && line[i + 1] === '"') { cur += '"'; i++; }
        else if (ch === '"') inQ = false;
        else cur += ch;
      } else if (ch === '"') inQ = true;
      else if (ch === ",") { cells.push(cur); cur = ""; }
      else cur += ch;
    }
    cells.push(cur);
    const [step, , judge, metric, baseS, varS, deltaS, thrS, , , verdictS] = cells;
    if (step === "base") continue; // reference rows: base == variant, delta 0
    const exp = expectedByStep[step];
    assert(exp, `CSV: неизвестный шаг ${step}`);
    const row = exp.rows.find((r) => r.judge === judge && r.label === metric);
    assert(row, `CSV: строка ${step}/${judge}/${metric} отсутствует в пересчёте из сырья`);
    // base/variant: пере-вычислить из сырья и сравнить со строкой CSV
    const expBase = row.a;
    const expVar = row.b;
    assert(parseCsvNum(baseS) === expBase,
      `CSV recheck ${step}/${judge}/${metric}: base ${baseS} != ${expBase}`);
    assert(parseCsvNum(varS) === expVar,
      `CSV recheck ${step}/${judge}/${metric}: variant ${varS} != ${expVar}`);
    if (expBase === null || expVar === null) {
      // отсутствующая целевая ячейка: значения «—», дельта не определена
      assert(parseCsvNum(baseS) === null && parseCsvNum(varS) === null && deltaS === "—",
        `CSV recheck ${step}/${judge}/${metric}: отсутствующая ячейка должна быть «—» во всех колонках значений`);
      checked++;
      (perStepVerdicts[step] ??= []).push(row);
      continue;
    }
    assert(parseFloat(deltaS) === r2(expBase - expVar),
      `CSV recheck ${step}/${judge}/${metric}: delta ${deltaS} != ${r2(expBase - expVar)}`);
    assert(thrS.startsWith(row.thr.replace(/≤/g, "<=")),
      `CSV recheck ${step}/${judge}/${metric}: threshold "${thrS}" != "${row.thr}"`);
    assert(verdictS === row.csvVerdict,
      `CSV recheck ${step}/${judge}/${metric}: verdict "${verdictS}" != "${row.csvVerdict}"`);
    checked++;
    (perStepVerdicts[step] ??= []).push(row);
  }
  // (б) вердикт каждого шага в отчёте == вердикт, вычисленный из CSV-строк
  for (const st of SUMMARY_STEPS) {
    if (!expectedByStep[st.name]) continue;
    const rowsFromCsv = perStepVerdicts[st.name] ?? [];
    const overallFromCsv = computeOverall(st.name, rowsFromCsv);
    const reportOverall = expectedByStep[st.name].overall;
    assert(overallFromCsv === reportOverall,
      `CSV/report verdict mismatch for ${st.name}: from CSV "${overallFromCsv}" != report "${reportOverall}"`);
  }
  return checked;
}

function sectionCsv() {
  const rows = csvRows();
  const esc = (v) => {
    const s = String(v);
    return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
  };
  const csv = ["step,label,judge,metric,base,variant,delta,threshold,precision,target,verdict",
    ...rows.map((r) => r.map(esc).join(","))].join("\n") + "\n";
  writeFileSync(csvPath, csv);
  const nChecked = verifyCsvAgainstRaw();
  const nBase = rows.filter((r) => r[0] === "base").length;
  const nStep = rows.length - nBase;
  out(`### CSV — \`docs/perf/B4_MICRO_PACK_summary.csv\` (перезаписан скриптом: ${rows.length} строк данных, из них ${nBase} базовых; Δ = base − variant; ${nChecked} из ${nStep} шаг-строк перечитаны и пересчитаны из сырья — сверка пройдена (базовые ${nBase} — источник, не дельта); вердикты шагов из CSV совпадают с отчётом)`);
  out("");
}

// Базовая таблица (§0 отчёта) из mc_base_* логов.
function sectionBase() {
  out("### §0 Базовые судьи (mc-base, база пакета A) — рендер скрипта");
  out("");
  out("| судья | метрика | base | точность | пометка |");
  out("|---|---|---|---|---|");
  const row = SUM_ROWS[0];
  for (const judge of JUDGES) {
    const spec = COLS.find((c) => c.judge === judge);
    for (const col of spec.cols) {
      const cell = cellOf("base", judge, col.key);
      const prec = !cell || col.kind === "cyc" ? "exact" : cell.exact ? "exact" : "int (±1)";
      out(`| ${judge} | ${col.label} | ${fmtVal(cell, col.kind)} | ${prec} | ИЗМЕРЕНО |`);
    }
  }
  out("");
}

// Вердикты всех шагов — сводная таблица (вычислено, без expected).
function sectionVerdicts(verdicts) {
  out("### Сводка общих вердиктов шагов (вычислено из PREREG)");
  out("");
  out("| шаг | цель | вердикт |");
  out("|---|---|---|");
  for (const st of SUMMARY_STEPS) {
    const t = PREREG.targets[st.name];
    const tgt = t.combine === "or"
      ? t.alts.map(targetLabel).join(" ИЛИ ")
      : t.alts.map(targetLabel).join(" И ");
    out(`| ${st.name} | ${tgt} | ${verdicts[st.name]} |`);
  }
  out("");
}

// §9: таблица мутантов из _raw_b4_micro_mutant_<name>.log
const MUTANT_LOGS = ["s1a", "s1b", "xnull", "span", "mask", "n9"];
function sectionMutants() {
  out("### §9 Мутанты (команда = первая строка лога)");
  out("");
  out("| имя | лог | команда | ожидание RED | полученный exit | process exit | RED |");
  out("|---|---|---|---|---|---|---|");
  for (const name of MUTANT_LOGS) {
    const file = `_raw_b4_micro_mutant_${name}.log`;
    const text = readFileSync(join(rawDir, file), "utf8");
    const lines = text.split("\n");
    const cmd = lines[0].trim();
    const expected = (lines.find((l) => l.startsWith("EXPECTED")) ?? lines[0].match(/\(mutant: ([^;)]+)/)?.[0] ?? "").replace(/^EXPECTED.{0,2}/, "").replace(/\s*---\s*$/, "").trim();
    const exitM = text.match(/CARGO_EXIT_CODE=(\d+)/) ?? text.match(/(?:^|\n)RC=(\d+)/);
    const procM = text.match(/TEST_PROCESS_EXIT=(0x[0-9a-fA-F]+)/);
    const red = /test result: FAILED|error: test failed/.test(text);
    let note = "";
    if (name === "s1b" && text.includes("counter assert never reached"))
      note = " (счётчикный assert не достигнут (abort раньше))";
    const esc = (s) => s.replace(/\|/g, "\\|");
    out(`| ${name} | \`${file}\` | \`${esc(cmd)}\` | ${esc(expected)} | ${exitM ? exitM[1] : "—"}${note} | ${procM ? procM[1] : "—"} | ${red ? "RED ✓" : "NOT RED ✗"} |`);
  }
  out("");
}

function missingLogs() {
  const present = new Set(readdirSync(rawDir)
    .filter((f) => f.startsWith("_raw_b4_micro_mc_") && f.endsWith(".log"))
    .map((f) => f.slice("_raw_b4_micro_".length, -".log".length)));
  const missing = new Set();
  for (const row of SUM_ROWS) {
    for (const labels of Object.values(row.src())) {
      for (const l of labels) if (!present.has(l)) missing.add(l);
    }
  }
  return [...missing].sort();
}

function main() {
  preregSelfTest();
  sectionBase();
  section1();
  const verdicts = {};
  for (const st of SUMMARY_STEPS) verdicts[st.name] = null;
  for (const st of STEPS) verdicts[st.name] = renderAB(st);
  for (const st of SUMMARY_STEPS) {
    if (verdicts[st.name] === null) {
      // N9 v1/v2: без секции, только вычисление вердикта
      verdicts[st.name] = computeStepRows(st.name, st.aRow, st.bRow).overall;
    }
  }
  sectionVerdicts(verdicts);
  sectionMutants();
  sectionOracle();
  sectionCsv();
  const miss = missingLogs();
  if (miss.length) {
    out(`Отсутствующие сырые логи (ячейки «—»): ${miss.map((m) => `_raw_b4_micro_${m}.log`).join(", ")}`);
    out("");
  }
  const genPath = join(rawDir, "B4_MICRO_PACK_tables.generated.md");
  writeFileSync(genPath, L.join("\n") + "\n");
  console.log(L.join("\n"));
}

main();
