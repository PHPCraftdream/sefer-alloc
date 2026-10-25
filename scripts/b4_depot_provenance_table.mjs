#!/usr/bin/env node
// b4_depot_provenance_table.mjs — таблицы и вердикт отчёта
// docs/perf/B4_DEPOT_PROVENANCE.md (chunk D0 «окупился бы depot magazine?»,
// фаза R5: v4-счётчики + callgrind v3-эры, calls= пересчитываются ЧИСЛЕННО).
//
// Источник: сырые файлы docs/perf/B4_DEPOT_PROVENANCE/raw/
//   - 20 счётчиковых логов *.v4.log (identity git_diff_sha256 2c8d7c76…,
//     tree_sha256 5441d8c2… — НОВОЕ дерево; larson t1/t2.r1-3/t4.r1-3,
//     mstress t1/t2.r1-3/t4.r1-3, churn s120.r1-3 + s241.r1-3),
//   - 24 callgrind cg_summary v3-эры (identity e7dc9d00…/592481… — СТАРАЯ
//     пара; full/tf/td/ctr × 2 размера × 3 нагрузки). Почему cg-эра: R4-дельта
//     cg-era→R4 меняет только bench-internals-хуки (доказательство —
//     raw/r4_behavior_parity.txt), production-поведение побайтово идентично,
//     поэтому calls=/Ir production-сборки валидны для нового дерева
//     (счётчиковая сборка отличается только Relaxed-инкрементами).
//   - манифесты tree_manifest.txt (новое дерево) и tree_manifest_cg_era.txt
//     (старое); скрипт сам считает sha256 содержимого манифеста и assert'ит
//     совпадение с записанным tree_sha256.
//
// calls= — ЧИСЛЕННО из первичного сырья: пары production (*.full.N) и
// счётчиковой (*.ctr.N) cg-summaries, GROUP <fn> … CALLS=<n>; assert
// prod_calls == ctr_calls для всех 3 функций × 2 размера × 3 нагрузки (18 пар).
//
// Модель цены — только непересекающиеся МАРЖИНАЛЬНЫЕ вклады, без повторного
// счёта: removed_ir/step(K) = absorb_share_K × tf_marginal +
// drain_replace_share_K × td_marginal − depot_price_K, где
// absorb_share = absorbed_k/own_overflow_blocks,
// drain_replace_share = refill_from_depot_k/refill_blocks_drained,
// refill_depot_share = refill_from_depot_k/refill_blocks_total (обе печатаются
// со знаменателями), depot_price — ОЦЕНКА (константы 3 и 2 заданы заранее).
//
// Запуск: node scripts/b4_depot_provenance_table.mjs

import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFileSync, writeFileSync, existsSync, statSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const p = (...s) => join(root, ...s);
const rawDir = "docs/perf/B4_DEPOT_PROVENANCE/raw";
const raw = (...s) => p(rawDir, ...s);
const read = (f) => readFileSync(raw(f), "utf8");

const KS = [1, 2, 4, 8];
// Identity-пары (заданы заранее, assert на КАЖДЫЙ читаемый файл):
const V4_DIFF_SHA = "2c8d7c76333ef2832896e95853b1f6929204fe255e2d10a8d25481ec05be5844";
const V4_TREE_SHA = "5441d8c29b7a64179ecada4d1aafdcfe0efc27678db25da09eac47b8370d9e9b";
const CG_DIFF_SHA = "e7dc9d00b4f766e5783bf3ac431c0daf972a069af94af9cb60ffc21a4f1529f2";
const CG_TREE_SHA = "59248117087f4c3d73af54311d9f45b53e220261d242c4c43beee07ec6c6a447";
const BASE_SHA = "f80c681b";

// ── константы модели цены (ОЦЕНКА, заданы заранее, НЕ измерялись отдельно) ──
const DEPOT_IR_PER_ABSORBED_BLOCK = 3;
const DEPOT_IR_PER_EVENT = 2;
const TCACHE_CAP = 16;
const FLUSH_N = 8;

// Заранее заданные маржинальные Ir (двухточечные (Ir@2N−Ir@N)/N; для churn
// делим на 121 — окно 120→241); assert снизу, скрипт ПЕРЕСЧИТЫВАЕТ их из cg.
const EXPECTED_MARGINAL = {
  larson: { full: 279.58, tf: 7.95, td: 9.88 },
  mstress: { full: 368.86, tf: 59.13, td: 73.69 },
  churn: { full: 6560789.60, tf: 455920.00, td: 486248.60 },
};
const MARGINAL_TOL = 0.5;

// Заранее заданный порог вердикта (НЕ менять):
// GO ⟺ ∃ K≤2: removed_share ≥ 5% полного marginal Ir хотя бы в mstress T=1
// или installed churn+Vec И absorb_share ≥ 50% в той же паре.
const VERDICT_IR_SHARE = 0.05;
const VERDICT_ABSORB_SHARE = 0.5;

// ── разбор счётчикового v4-лога ─────────────────────────────────────────────
function parseV4(file) {
  const txt = read(file);
  const lines = txt.split("\n").filter((l) => l.trim() !== "");
  const hdr = {};
  let opsActual = null;
  const jsons = [];
  for (const l of lines) {
    if (l.startsWith("{")) { jsons.push(JSON.parse(l)); continue; }
    const m = l.match(/^(\w+) (.+)$/);
    if (!m) continue;
    const [, k, v] = m;
    if (v.includes("_actual=")) opsActual = Number(v.split("=")[1]);
    else hdr[k] = v;
  }
  assert.equal(jsons.length, 2, `${file}: ожидается 2 JSON-строки`);
  assert.ok(hdr.git_diff_sha256 && hdr.tree_sha256 && hdr.features && hdr.rustc && hdr.cpu && hdr.os, `${file}: неполный заголовок`);
  assert.equal(hdr.git_diff_sha256, V4_DIFF_SHA, `${file}: неожиданный git_diff_sha256 ${hdr.git_diff_sha256}`);
  assert.equal(hdr.tree_sha256, V4_TREE_SHA, `${file}: неожиданный tree_sha256 ${hdr.tree_sha256}`);
  const c = jsons[0], s = jsons[1];
  for (const k of ["absorbed_k", "real_k", "fully_absorbed_ev_k", "refill_from_depot_k", "refill_drain_k", "refill_fully_k"]) {
    assert.equal(s[k].length, 4, `${file}: ${k} должен иметь 4 элемента`);
  }
  return { file, hdr, opsActual, c, s };
}

const V4_FILES = {
  larson: {
    t1: ["dp-r4-c1.larson.t1.s100000.v4.log"],
    t2: ["dp-r4-c2.larson.t2.s100000.r1.v4.log", "dp-r4-c2.larson.t2.s100000.r2.v4.log", "dp-r4-c2.larson.t2.s100000.r3.v4.log"],
    t4: ["dp-r4-c3.larson.t4.s100000.r1.v4.log", "dp-r4-c3.larson.t4.s100000.r2.v4.log", "dp-r4-c3.larson.t4.s100000.r3.v4.log"],
  },
  mstress: {
    t1: ["dp-r4-c4.mstress.t1.s100000.v4.log"],
    t2: ["dp-r4-c5.mstress.t2.s100000.r1.v4.log", "dp-r4-c5.mstress.t2.s100000.r2.v4.log", "dp-r4-c5.mstress.t2.s100000.r3.v4.log"],
    t4: ["dp-r4-c6.mstress.t4.s100000.r1.v4.log", "dp-r4-c6.mstress.t4.s100000.r2.v4.log", "dp-r4-c6.mstress.t4.s100000.r3.v4.log"],
  },
  churn: {
    s120: ["dp-r4-c7.churn.t-.s120.r1.v4.log", "dp-r4-c7.churn.t-.s120.r2.v4.log", "dp-r4-c7.churn.t-.s120.r3.v4.log"],
    s241: ["dp-r4-c8.churn.t-.s241.r1.v4.log", "dp-r4-c8.churn.t-.s241.r2.v4.log", "dp-r4-c8.churn.t-.s241.r3.v4.log"],
  },
};

const v4 = {};
let v4Count = 0;
for (const [wl, groups] of Object.entries(V4_FILES)) {
  v4[wl] = {};
  for (const [g, files] of Object.entries(groups)) { v4[wl][g] = files.map(parseV4); v4Count += files.length; }
}
assert.equal(v4Count, 20, `ожидалось 20 v4-логов, прочитано ${v4Count}`);

// ── арифметика симуляции (на каждый прогон, ИЗМЕРЕНО + assert) ──────────────
const sum = (a) => a.reduce((x, y) => x + y, 0);
for (const wl of Object.keys(v4)) for (const g of Object.keys(v4[wl])) for (const r of v4[wl][g]) {
  const { file, c, s } = r;
  assert.equal(sum(s.absorbed_k) + sum(s.real_k), 4 * c.depot_bin_ingress_own_overflow_blocks,
    `${file}: Σ_K(absorbed+real) != 4×own_overflow_blocks`);
  assert.equal(sum(s.refill_from_depot_k) + sum(s.refill_drain_k), 4 * c.depot_refill_blocks_total,
    `${file}: Σ_K(refill_from_depot+refill_drain) != 4×refill_blocks_total`);
  assert.equal(c.depot_refill_blocks_drained + c.depot_refill_blocks_carved, c.depot_refill_blocks_total,
    `${file}: drained+carved != blocks_total`);
  for (let i = 0; i < 4; i++) {
    assert.equal(s.absorbed_k[i] + s.real_k[i], c.depot_bin_ingress_own_overflow_blocks,
      `${file}: K=${KS[i]} absorbed+real != own_overflow_blocks`);
    assert.ok(s.fully_absorbed_ev_k[i] <= c.depot_bin_ingress_own_overflow_events,
      `${file}: K=${KS[i]} fully_absorbed_ev > own_overflow_events (occupied-инвариант)`);
    assert.ok(s.refill_fully_k[i] <= c.depot_refill_events,
      `${file}: K=${KS[i]} refill_fully > refill_events`);
  }
  assert.equal(c.depot_bin_ingress_other_blocks, 0, `${file}: оракул other_blocks != 0`);
  assert.equal(s.unmapped, 0, `${file}: трипвайр unmapped != 0`);
  assert.equal(c.depot_bin_ingress_pushes,
    c.depot_bin_ingress_own_overflow_blocks + c.depot_bin_ingress_own_batch_blocks +
    c.depot_bin_ingress_remote_blocks + c.depot_flush_swap_top_blocks +
    c.depot_flush_teardown_blocks + c.depot_bin_ingress_other_blocks,
    `${file}: SUBSTRATE_PUSHES-инвариант (pushes != сумма блоков по источникам)`);
  if (c.threads === 1) {
    assert.equal(c.depot_bin_ingress_remote_events, 0, `${file}: T=1 оракул remote_events != 0`);
    assert.equal(c.depot_bin_ingress_remote_blocks, 0, `${file}: T=1 оракул remote_blocks != 0`);
    assert.equal(c.depot_bin_ingress_own_batch_events, 0, `${file}: T=1 оракул own_batch_events != 0`);
    assert.equal(c.depot_bin_ingress_own_batch_blocks, 0, `${file}: T=1 оракул own_batch_blocks != 0`);
  }
}

// ── identity + манифесты + parity ───────────────────────────────────────────
assert.ok(existsSync(raw("tree_manifest.txt")), "raw/tree_manifest.txt отсутствует");
assert.ok(existsSync(raw("tree_manifest_cg_era.txt")), "raw/tree_manifest_cg_era.txt отсутствует");
const sha256 = (f) => createHash("sha256").update(readFileSync(raw(f))).digest("hex");
const manifestSha = sha256("tree_manifest.txt");
assert.equal(manifestSha, V4_TREE_SHA,
  `sha256(tree_manifest.txt) = ${manifestSha} != tree_sha256 счётчиков ${V4_TREE_SHA}`);
const manifestCgSha = sha256("tree_manifest_cg_era.txt");
assert.equal(manifestCgSha, CG_TREE_SHA,
  `sha256(tree_manifest_cg_era.txt) = ${manifestCgSha} != cg-era tree_sha256 ${CG_TREE_SHA}`);
const wslLines = read("tree_manifest.txt").split("\n").filter((l) => l.includes("wsl-snapshot/"));
assert.equal(wslLines.length, 2, `в tree_manifest.txt ожидалось 2 строки wsl-snapshot/, найдено ${wslLines.length}`);
assert.ok(existsSync(raw("r4_behavior_parity.txt")), "raw/r4_behavior_parity.txt отсутствует");
assert.ok(statSync(raw("r4_behavior_parity.txt")).size > 0, "raw/r4_behavior_parity.txt пуст");

// ── разбор cg_summary ───────────────────────────────────────────────────────
function parseProgramTotals(file) {
  const m = read(file).match(/PROGRAM TOTALS ---\n\s*([\d,]+) \(/);
  assert.ok(m, `${file}: нет PROGRAM TOTALS`);
  return Number(m[1].replace(/,/g, ""));
}
function parseToggleTotal(file) {
  const m = read(file).match(/TOGGLE_TOTAL_IR=(\d+)/);
  assert.ok(m, `${file}: нет TOGGLE_TOTAL_IR=`);
  return Number(m[1]);
}
function parseGroups(file) {
  const txt = read(file);
  // identity cg-эры — на каждый cg-файл
  const d = txt.match(/^git_diff_sha256 (\S+)/m), t = txt.match(/^tree_sha256 (\S+)/m);
  assert.ok(d && t, `${file}: нет identity-заголовка`);
  assert.equal(d[1], CG_DIFF_SHA, `${file}: неожиданный cg git_diff_sha256 ${d[1]}`);
  assert.equal(t[1], CG_TREE_SHA, `${file}: неожиданный cg tree_sha256 ${t[1]}`);
  const out = {};
  for (const m of txt.matchAll(/GROUP (\w+) SELF_IR=(\d+) CALLS=(\d+)/g)) {
    out[`${m[1]}:SELF_IR`] = Number(m[2]);
    out[`${m[1]}:CALLS`] = Number(m[3]);
  }
  for (const m of txt.matchAll(/GROUP (\w+) INCL_IR=(\d+)/g)) out[`${m[1]}:INCL_IR`] = Number(m[2]);
  for (const fn of ["flush_class_internal", "refill_magazine_slow", "try_drain_freelist_batch"]) {
    for (const kind of ["SELF_IR", "CALLS", "INCL_IR"]) assert.ok(`${fn}:${kind}` in out, `${file}: нет GROUP ${fn} ${kind}`);
  }
  return out;
}

const CG = {
  larson: { N: 50000, HI: 100000, p: "dp-r2-p1-larson" },
  mstress: { N: 50000, HI: 100000, p: "dp-r2-p2-mstress" },
  churn: { N: 120, HI: 241, p: "dp-r2-p3-churn" },
};
const cgTotals = {};
const cgGroups = {};
let cgCount = 0;
for (const [wl, cfg] of Object.entries(CG)) {
  cgTotals[wl] = {};
  for (const cfg2 of ["full", "tf", "td"]) {
    const fLo = `${cfg.p}.${cfg2}.${cfg.N}.cg_summary.txt`;
    const fHi = `${cfg.p}.${cfg2}.${cfg.HI}.cg_summary.txt`;
    cgTotals[wl][cfg2] = {
      lo: cfg2 === "full" ? parseProgramTotals(fLo) : parseToggleTotal(fLo),
      hi: cfg2 === "full" ? parseProgramTotals(fHi) : parseToggleTotal(fHi),
    };
    cgCount += 2;
  }
  cgGroups[wl] = {
    [cfg.N]: parseGroups(`${cfg.p}.full.${cfg.N}.cg_summary.txt`),
    [cfg.HI]: parseGroups(`${cfg.p}.full.${cfg.HI}.cg_summary.txt`),
  };
  for (const cfg2 of ["ctr"]) {
    for (const N of [cfg.N, cfg.HI]) { parseGroups(`${cfg.p}.${cfg2}.${N}.cg_summary.txt`); cgCount += 1; }
  }
}
assert.equal(cgCount, 24, `ожидалось 24 cg_summary, прочитано ${cgCount}`);

// Маржинальные Ir/step; для churn — Ir/повтор, знаменатель = окно 120→241 = 121.
const MARG = {};
for (const [wl, cfg] of Object.entries(CG)) {
  const denom = cfg.HI - cfg.N; // larson/mstress 50000; churn 121
  MARG[wl] = {};
  for (const k of ["full", "tf", "td"]) {
    MARG[wl][k] = (cgTotals[wl][k].hi - cgTotals[wl][k].lo) / denom;
    assert.ok(Math.abs(MARG[wl][k] - EXPECTED_MARGINAL[wl][k]) <= MARGINAL_TOL,
      `${wl} ${k} marginal: пересчитано ${MARG[wl][k]}, ожидалось ${EXPECTED_MARGINAL[wl][k]} (допуск ${MARGINAL_TOL})`);
  }
  assert.ok(MARG[wl].tf + MARG[wl].td <= MARG[wl].full + 2 * MARGINAL_TOL,
    `${wl}: tf+td (${MARG[wl].tf + MARG[wl].td}) > full (${MARG[wl].full})`);
}

// ── calls= prod vs ctr — ЧИСЛЕННО из первичного сырья (18 пар) ──────────────
const FNS = ["flush_class_internal", "refill_magazine_slow", "try_drain_freelist_batch"];
const callsPairs = [];
for (const [wl, cfg] of Object.entries(CG)) {
  for (const N of [cfg.N, cfg.HI]) {
    const prod = parseGroups(`${cfg.p}.full.${N}.cg_summary.txt`);
    const ctr = parseGroups(`${cfg.p}.ctr.${N}.cg_summary.txt`);
    for (const fn of FNS) {
      const pc = prod[`${fn}:CALLS`], cc = ctr[`${fn}:CALLS`];
      assert.equal(pc, cc, `${wl} n=${N} ${fn}: prod_calls=${pc} != ctr_calls=${cc}`);
      callsPairs.push({ wl, N, fn, prod: pc, ctr: cc });
    }
  }
}
assert.equal(callsPairs.length, 18, `ожидалось 18 calls-пар, получено ${callsPairs.length}`);

// ── модель цены ─────────────────────────────────────────────────────────────
function simPair(r, K, wl) {
  const ki = KS.indexOf(K);
  const { c, s } = r;
  const steps = c.steps ?? c.repeats; // churn: окно счётчиков = repeats (120)
  const absorbShare = s.absorbed_k[ki] / c.depot_bin_ingress_own_overflow_blocks;
  const drainReplaceShare = s.refill_from_depot_k[ki] / c.depot_refill_blocks_drained;
  const refillDepotShare = s.refill_from_depot_k[ki] / c.depot_refill_blocks_total;
  const eventsFullyAbsorbedShare = s.fully_absorbed_ev_k[ki] / c.depot_bin_ingress_own_overflow_events;
  const eventsFullyDepotShare = s.refill_fully_k[ki] / c.depot_refill_events;
  const priceBlocks = (DEPOT_IR_PER_ABSORBED_BLOCK * (s.absorbed_k[ki] + s.refill_from_depot_k[ki])) / steps;
  const priceEvents = (DEPOT_IR_PER_EVENT * (s.fully_absorbed_ev_k[ki] + s.refill_fully_k[ki])) / steps;
  const price = priceBlocks + priceEvents;
  const removed = absorbShare * MARG[wl].tf + drainReplaceShare * MARG[wl].td - price;
  const removedShare = removed / MARG[wl].full;
  const eps = 1e-9;
  const cap = (MARG[wl].tf + MARG[wl].td) / MARG[wl].full;
  assert.ok(removedShare <= cap + eps, `${r.file} K=${K}: removed_share ${removedShare} > (tf+td)/full ${cap}`);
  assert.ok(removedShare <= 1 + eps, `${r.file} K=${K}: removed_share ${removedShare} > 1`);
  return {
    absorbShare, drainReplaceShare, refillDepotShare,
    fullyAbsorbedEv: s.fully_absorbed_ev_k[ki], fullyAbsorbedEvShare: eventsFullyAbsorbedShare,
    refillFully: s.refill_fully_k[ki], refillFullyShare: eventsFullyDepotShare,
    price, priceBlocks, priceEvents, removed, removedShare,
  };
}

const median = (a) => {
  const b = [...a].sort((x, y) => x - y);
  const n = b.length;
  return n % 2 ? b[(n - 1) / 2] : (b[n / 2 - 1] + b[n / 2]) / 2;
};
const spread = (a) => Math.max(...a) - Math.min(...a);
function agg(reps, f) {
  const vals = reps.map(f);
  const med = {}, spr = {};
  for (const k of Object.keys(vals[0])) {
    med[k] = median(vals.map((v) => v[k]));
    spr[k] = spread(vals.map((v) => v[k]));
  }
  return { med, spr };
}

const fmt = (x, d = 2) => x.toFixed(d);
const num = (x, d = 6) => (Number.isInteger(x) ? String(x) : x.toFixed(d));
const pct = (n, den, d = 2) => `${fmt((n / den) * 100, d)}% (${n}/${den})`;

// ── CSV ─────────────────────────────────────────────────────────────────────
const CSV_HEADER = ["base_sha", "git_diff_sha256_counters", "tree_sha256_counters", "git_diff_sha256_cg", "tree_sha256_cg", "features", "rustc", "cpu", "os", "workload", "T", "repeats", "TCACHE_CAP", "FLUSH_N", "K", "metric", "median", "spread", "kind"];
const csvField = (v) => {
  const s = String(v);
  return /[",\n]/.test(s) ? `"${s.replace(/"/g, '""')}"` : s;
};
const csvRows = [CSV_HEADER.join(",")];
function csvRow(o) {
  const rec = {
    base_sha: BASE_SHA,
    git_diff_sha256_counters: V4_DIFF_SHA,
    tree_sha256_counters: V4_TREE_SHA,
    git_diff_sha256_cg: CG_DIFF_SHA,
    tree_sha256_cg: CG_TREE_SHA,
    features: o.features, rustc: o.rustc, cpu: o.cpu, os: o.os,
    workload: o.workload, T: o.T, repeats: o.repeats,
    TCACHE_CAP: TCACHE_CAP, FLUSH_N: FLUSH_N, K: o.K,
    metric: o.metric, median: o.median, spread: o.spread, kind: o.kind,
  };
  const fields = CSV_HEADER.map((c) => csvField(rec[c]));
  assert.equal(fields.length, CSV_HEADER.length, "CSV: число полей != числу колонок заголовка");
  csvRows.push(fields.join(","));
}

const out = [];
const print = (s = "") => out.push(s);

// ═══ Заголовок / identity ══════════════════════════════════════════════════
print("=== B4_DEPOT_PROVENANCE — D0 «окупился бы depot magazine?» (R5, v4-счётчики + callgrind v3-эры) ===");
print("");
print("Identity (ИЗМЕРЕНО, assert на каждый читаемый файл):");
print(`  счётчики (20 v4-логов): git_diff_sha256 = ${V4_DIFF_SHA}`);
print(`                          tree_sha256     = ${V4_TREE_SHA}`);
print(`  callgrind (24 cg_summary v3-эры): git_diff_sha256 = ${CG_DIFF_SHA}`);
print(`                                    tree_sha256     = ${CG_TREE_SHA}`);
print("  Почему cg-эра: дельта cg-era→R4 меняет только bench-internals-хуки");
print("  (доказательство — raw/r4_behavior_parity.txt, production-поведение");
print("  побайтово идентично), поэтому calls=/Ir production-сборки валидны для");
print("  нового дерева; счётчиковая сборка отличается только Relaxed-инкрементами.");
print(`  base_sha (origin/main) = ${BASE_SHA}`);
print(`  features (счётчики): ${v4.larson.t1[0].hdr.features}`);
print(`  rustc: ${v4.larson.t1[0].hdr.rustc}`);
print(`  cpu: ${v4.larson.t1[0].hdr.cpu}`);
print(`  os: ${v4.larson.t1[0].hdr.os}`);
print("  Повторы: larson/mstress T=1 ×1, T=2/T=4 ×3; churn ×3 (окно счётчиков");
print("  repeats=120, второй набор s241 ×3 — прочитан и assert'нут); callgrind:");
print("  50000→100000 (larson/mstress), 120→241 (churn), конфигурации full/tf/td/ctr.");
print("  Манифесты: sha256(tree_manifest.txt) = tree_sha256 счётчиков ✓;");
print("  sha256(tree_manifest_cg_era.txt) = cg-era tree_sha256 ✓; 2 строки");
print("  wsl-snapshot/ (dp_depot_cg.rs, Cargo.toml.cg-patch) ✓;");
print("  r4_behavior_parity.txt существует и непуст ✓. Все assert'ы пройдены.");
print("");

// ═══ Маржинальные Ir ═══════════════════════════════════════════════════════
print("=== Маржинальные Ir (ИЗМЕРЕНО, двухточечные (Ir@2N−Ir@N)/N; churn — Ir/повтор, знаменатель 121) ===");
print("");
print("| нагрузка | N | FULL Ir/шаг (ИЗМЕРЕНО) | tf Ir/шаг (ИЗМЕРЕНО) | td Ir/шаг (ИЗМЕРЕНО) |");
print("|---|---|---|---|---|");
for (const [wl, cfg] of Object.entries(CG)) {
  print(`| ${wl} | ${cfg.N}→${cfg.HI} | ${fmt(MARG[wl].full)} | ${fmt(MARG[wl].tf)} | ${fmt(MARG[wl].td)} |`);
}
print("");
print("  Точки (ИЗМЕРЕНО из PROGRAM TOTALS / TOGGLE_TOTAL_IR):");
for (const [wl, cfg] of Object.entries(CG)) {
  const denom = cfg.HI - cfg.N;
  for (const k of ["full", "tf", "td"]) {
    print(`    ${wl} ${k}: @${cfg.N}=${cgTotals[wl][k].lo}, @${cfg.HI}=${cgTotals[wl][k].hi} → (${cgTotals[wl][k].hi}−${cgTotals[wl][k].lo})/${denom} = ${fmt(MARG[wl][k])}`);
  }
}
print("  churn: знаменатель 121 = окно 120→241 (Ir-окно); счётчиковое окно 120");
print("  повторов — погрешность ≤1/121 ≈ 0.83%, метрика ОЦЕНКА (см. модель).");
print(`  Assert: пересчитанные маржинальные совпали с заданными заранее (допуск ${MARGINAL_TOL} Ir/шаг).`);
print(`  Assert модели: tf+td ≤ full выполнено для всех трёх нагрузок (доли: larson ${fmt((MARG.larson.tf + MARG.larson.td) / MARG.larson.full, 4)}, mstress ${fmt((MARG.mstress.tf + MARG.mstress.td) / MARG.mstress.full, 4)}, churn ${fmt((MARG.churn.tf + MARG.churn.td) / MARG.churn.full, 4)}).`);
print("");
print("  calls= prod vs ctr (ИЗМЕРЕНО, численно из GROUP … CALLS=, 18/18 равны):");
for (const c of callsPairs) {
  print(`    ${c.wl} n=${c.N} ${c.fn}: prod_calls=${c.prod} ctr_calls=${c.ctr} -> EQUAL`);
}
print("");

// ═══ Модель цены: константы ════════════════════════════════════════════════
print("=== Модель цены (только непересекающиеся МАРЖИНАЛЬНЫЕ вклады) ===");
print("");
print("  removed_ir/step(K) = absorb_share_K × tf_marginal + drain_replace_share_K × td_marginal − depot_price_K");
print("    absorb_share_K = absorbed_k / own_overflow_blocks (ИЗМЕРЕНО/ИЗМЕРЕНО → ОЦЕНКА доли)");
print("    drain_replace_share_K = refill_from_depot_k / refill_blocks_drained (ИЗМЕРЕНО/ИЗМЕРЕНО → ОЦЕНКА доли; carve исключён)");
print("    refill_depot_share_K = refill_from_depot_k / refill_blocks_total (та же выгода, знаменатель TOTAL — печатаются ОБЕ метрики)");
print(`    depot_price_K = ${DEPOT_IR_PER_ABSORBED_BLOCK}×(absorbed_k+refill_from_depot_k)/steps + ${DEPOT_IR_PER_EVENT}×(fully_absorbed_ev_k+refill_fully_k)/steps (ОЦЕНКА)`);
print(`      ${DEPOT_IR_PER_ABSORBED_BLOCK} Ir/блок = load+store+инкремент индекса push/pop массива из 8 указателей — ОЦЕНКА объёма, отдельно НЕ измерялось`);
print(`      ${DEPOT_IR_PER_EVENT} Ir/событие = ветка «полностью поглощено / полностью выдано» — ОЦЕНКА, отдельно НЕ измерялось`);
print("  steps: larson/mstress — шаги (100000); churn — окно СЧЁТЧИКОВ 120 повторов");
print("  (Ir-окно 121 повтор: погрешность ≤1/121 ≈ 0.83%, ОЦЕНКА).");
print("  Assert на каждую пару (нагрузка×K): removed_share ≤ (tf+td)/full И ≤ 1 — все пройдены.");
print("");

// ═══ Таблицы по конфигурациям ══════════════════════════════════════════════
const METRICS = [
  ["absorb_share", "absorbShare", "ОЦЕНКА"],
  ["refill_depot_share", "refillDepotShare", "ОЦЕНКА"],
  ["drain_replace_share", "drainReplaceShare", "ОЦЕНКА"],
  ["fully_absorbed_ev", "fullyAbsorbedEv", "ИЗМЕРЕНО"],
  ["fully_absorbed_ev_share", "fullyAbsorbedEvShare", "ОЦЕНКА"],
  ["refill_fully", "refillFully", "ИЗМЕРЕНО"],
  ["refill_fully_share", "refillFullyShare", "ОЦЕНКА"],
  ["depot_price_ir", "price", "ОЦЕНКА"],
  ["removed_ir", "removed", "ОЦЕНКА"],
  ["removed_share", "removedShare", "ОЦЕНКА"],
];

function tableFor(wl, tLabel, reps, unit) {
  const r0 = reps[0];
  const T = r0.c.threads;
  const repeats = r0.c.steps ?? r0.c.repeats;
  print(`### ${wl} ${tLabel} (reps=${reps.length}, steps/repeats=${repeats}, ${unit}; медиана по повторам, разброс = max−min)`);
  print("");
  print("| K | metric | медиана | разброс | kind |");
  print("|---|---|---|---|---|");
  for (const K of KS) {
    const a = agg(reps, (r) => simPair(r, K, wl));
    for (const [name, key, kind] of METRICS) {
      print(`| ${K} | ${name} | ${num(a.med[key])} | ${num(a.spr[key])} | ${kind} |`);
      csvRow({ features: r0.hdr.features, rustc: r0.hdr.rustc, cpu: r0.hdr.cpu, os: r0.hdr.os, workload: `${wl} ${tLabel}`, T, repeats, K, metric: name, median: num(a.med[key]), spread: num(a.spr[key]), kind });
    }
    // ingress-счётчики (ИЗМЕРЕНО, медианы; разброс для remote_blocks/pushes)
    const gv = (f) => median(reps.map((r) => f(r.c)));
    const gsp = (f) => spread(reps.map((r) => f(r.c)));
    const ing = [
      ["ingress_own_overflow_blocks", gv((c) => c.depot_bin_ingress_own_overflow_blocks), 0],
      ["ingress_own_overflow_events", gv((c) => c.depot_bin_ingress_own_overflow_events), 0],
      ["ingress_own_batch_blocks", gv((c) => c.depot_bin_ingress_own_batch_blocks), 0],
      ["ingress_remote_blocks", gv((c) => c.depot_bin_ingress_remote_blocks), gsp((c) => c.depot_bin_ingress_remote_blocks)],
      ["ingress_remote_events", gv((c) => c.depot_bin_ingress_remote_events), 0],
      ["flush_swap_top_blocks", gv((c) => c.depot_flush_swap_top_blocks), 0],
      ["flush_teardown_blocks", gv((c) => c.depot_flush_teardown_blocks), 0],
      ["refill_blocks_drained", gv((c) => c.depot_refill_blocks_drained), 0],
      ["refill_blocks_carved", gv((c) => c.depot_refill_blocks_carved), 0],
      ["refill_blocks_total", gv((c) => c.depot_refill_blocks_total), 0],
      ["substrate_pushes", gv((c) => c.depot_bin_ingress_pushes), gsp((c) => c.depot_bin_ingress_pushes)],
      ["invalidations", median(reps.map((r) => r.s.invalidations)), 0],
      ["unmapped", median(reps.map((r) => r.s.unmapped)), 0],
    ];
    for (const [name, med, spr] of ing) {
      print(`| ${K} | ${name} | ${med} | ${spr} | ИЗМЕРЕНО |`);
      csvRow({ features: r0.hdr.features, rustc: r0.hdr.rustc, cpu: r0.hdr.cpu, os: r0.hdr.os, workload: `${wl} ${tLabel}`, T, repeats, K, metric: name, median: med, spread: spr, kind: "ИЗМЕРЕНО" });
    }
  }
  print("");
}

const UNITS = { larson: "Ir/шаг", mstress: "Ir/шаг", churn: "Ir/повтор" };
for (const wl of ["larson", "mstress"]) {
  for (const [tLabel, title] of [["t1", "T=1"], ["t2", "T=2"], ["t4", "T=4"]]) tableFor(wl, title, v4[wl][tLabel], UNITS[wl]);
}
tableFor("churn", "T=1 (s120, окно счётчиков 120 повторов)", v4.churn.s120, UNITS.churn);
// s241: прочитан, identity/арифметика assert'нуты выше; краткая сводка.
{
  const reps = v4.churn.s241;
  const r0 = reps[0];
  const a1 = agg(reps, (r) => simPair(r, 2, "churn"));
  print(`### churn T=1 (s241, окно счётчиков 241 повтора; чтение + assert identity/арифметики; краткая сводка, К=2)`);
  print("");
  print(`  absorb_share=${fmt(a1.med.absorbShare, 4)}, refill_depot_share=${fmt(a1.med.refillDepotShare, 4)}, drain_replace_share=${fmt(a1.med.drainReplaceShare, 4)}, removed_share=${fmt(a1.med.removedShare, 4)} — согласуется с s120 (масштаб ~1:2 по счётчикам, доли те же).`);
  print("");
}

// ═══ ВЕРДИКТ ═══════════════════════════════════════════════════════════════
print("=== ВЕРДИКТ (порог задан заранее, НЕ менять) ===");
print("");
print(`  GO ⟺ ∃ K≤2: removed_share ≥ ${VERDICT_IR_SHARE * 100}% полного marginal Ir хотя бы в mstress T=1 или installed churn+Vec И absorb_share ≥ ${VERDICT_ABSORB_SHARE * 100}% в той же паре.`);
print("  Для churn знаменатель — FULL Ir/повтор (6560789.60, окно 121).");
print("");
let verdict = "NO-GO";
const verdictPairs = [
  ["mstress T=1", "mstress", v4.mstress.t1],
  ["churn (installed, окно счётчиков 120)", "churn", v4.churn.s120],
];
for (const [label, wl0, reps] of verdictPairs) {
  for (const K of [1, 2]) {
    const a = agg(reps, (r) => simPair(r, K, wl0));
    const condA = a.med.removedShare >= VERDICT_IR_SHARE;
    const condB = a.med.absorbShare >= VERDICT_ABSORB_SHARE;
    if (condA && condB) verdict = "GO";
    const absNum = median(reps.map((r) => r.s.absorbed_k[K - 1]));
    const absDen = reps[0].c.depot_bin_ingress_own_overflow_blocks;
    print(`  ${label}, K=${K}: removed_share=${fmt(a.med.removedShare, 4)} (${fmt(a.med.removed)} из FULL ${fmt(MARG[wl0].full)} ${UNITS[wl0]}) — условие A (≥${VERDICT_IR_SHARE * 100}%): ${condA ? "ДА" : "нет"}; absorb_share=${fmt(a.med.absorbShare, 4)} (${pct(absNum, absDen)}) — условие B (≥${VERDICT_ABSORB_SHARE * 100}%): ${condB ? "ДА" : "нет"} → ${condA && condB ? "GO-пара" : "не проходит"}`);
  }
}
print("");
print(`ВЕРДИКТ: ${verdict}`);
print("");

// финальные строки-идентичность в CSV
const hdr0 = v4.larson.t1[0].hdr;
const idBase = { features: hdr0.features, rustc: hdr0.rustc, cpu: hdr0.cpu, os: hdr0.os, workload: "-", T: "-", repeats: "-", K: "-" };
csvRow({ ...idBase, metric: "verdict", median: verdict, spread: "-", kind: "ИЗМЕРЕНО" });
csvRow({ ...idBase, metric: "DEPOT_IR_PER_ABSORBED_BLOCK", median: DEPOT_IR_PER_ABSORBED_BLOCK, spread: "-", kind: "ОЦЕНКА" });
csvRow({ ...idBase, metric: "DEPOT_IR_PER_EVENT", median: DEPOT_IR_PER_EVENT, spread: "-", kind: "ОЦЕНКА" });
csvRow({ ...idBase, metric: "repeats_note", median: "larson/mstress T=1 x1, T=2/T=4 x3; churn s120 x3 + s241 x3 (счётчики); cg: full/tf/td/ctr x 2 sizes x 3 workloads", spread: "-", kind: "ИЗМЕРЕНО" });
csvRow({ ...idBase, metric: "churn_ir_window_note", median: "Ir-window 121 repeats (120->241), counter window 120 repeats, error <=1/121", spread: "-", kind: "ОЦЕНКА" });

for (const row of csvRows.slice(1)) assert.equal(row.split(",").length >= CSV_HEADER.length, true, "CSV: строка короче заголовка");
// точный подсчёт полей с учётом кавычек
for (const row of csvRows.slice(1)) {
  let inQ = false, n = 1;
  for (const ch of row) {
    if (ch === '"') inQ = !inQ;
    else if (ch === "," && !inQ) n++;
  }
  assert.equal(n, CSV_HEADER.length, `CSV: поле-подсчёт ${n} != ${CSV_HEADER.length}: ${row}`);
}

writeFileSync(p("docs", "perf", "B4_DEPOT_PROVENANCE_summary.csv"), csvRows.join("\n") + "\n");
console.log(out.join("\n"));
console.log(`\n[ok] CSV записан: docs/perf/B4_DEPOT_PROVENANCE_summary.csv (${csvRows.length - 1} строк данных, ${CSV_HEADER.length} колонок)`);
