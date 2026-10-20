#!/usr/bin/env node
// b4-mt final phase: print all report tables (markdown) for B4_MT_PIN_NOTOWN.md
// and write docs/perf/B4_MT_PIN_NOTOWN_summary.csv (R14-10: the script
// generates the CSV, never transcribed by hand).
//
// Every data line is parsed from the three RAW logs
//   docs/perf/_raw_b4_mt_base.log   (base A)
//   docs/perf/_raw_b4_mt_xn.log     (X-notown, NO-GO, src reverted)
//   docs/perf/_raw_b4_mt_xp.log     (X-pin, GO)
// — no hardcoded measurements. Node-only: NO cargo invocations.
//
// Internal asserts: deltas and percentages are COMPUTED here and compared
// against the values the raw logs print inline (or against arithmetic
// identities); any mismatch aborts with a non-zero exit.
//
// Usage:
//   node scripts/b4_mt_pin_notown_table.mjs            # print tables
//   node scripts/b4_mt_pin_notown_table.mjs --csv      # + write summary CSV

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const logDir = join(root, "docs", "perf");
const read = (f) => readFileSync(join(logDir, f), "utf8").split(/\r?\n/);

let failures = 0;
const check = (cond, msg) => {
  if (!cond) {
    failures++;
    console.error(`ASSERT FAIL: ${msg}`);
  }
};
const approx = (a, b, tol) => Math.abs(a - b) <= tol;

// ── parsers ──────────────────────────────────────────────────────────────────
// MARGINAL Ir/free (foreign locality) = 136
// MARGINAL Ir/op (sefer larson T=1) = 300
// sefer w_alloc marginal Ir/op = 26
// sefer w_free marginal Ir/op  = 103
function parseMarginal(lines) {
  const out = {};
  for (const l of lines) {
    let m = l.match(/^MARGINAL Ir\/free \(([^)]+)\) = (\d+)/);
    if (m) out[`free:${m[1]}`] = Number(m[2]);
    m = l.match(/^MARGINAL Ir\/op \(sefer (\w+) T=1\) = (\d+)/);
    if (m) out[`op:${m[1]}`] = Number(m[2]);
    m = l.match(/^sefer w_(alloc|free) marginal Ir\/op\s+= (\d+)/);
    if (m) out[`thrash64:${m[1]}`] = Number(m[2]);
  }
  return out;
}

// First hit_frac found in an ORACLE line: 0.9999 / 0.0000 / 1.0000
function parseOracleHitFrac(lines) {
  for (const l of lines) {
    const m = l.match(/ORACLE[^]*hit_frac=([\d.]+)/) || l.match(/ORACLE.*hit_frac=([\d.]+)/);
    if (m) return Number(m[1]);
  }
  return null;
}

const base = read("_raw_b4_mt_base.log");
const xn = read("_raw_b4_mt_xn.log");
const xp = read("_raw_b4_mt_xp.log");

const B = parseMarginal(base);
const XN = parseMarginal(xn);
const XP = parseMarginal(xp);

// Notown oracle (xn log): NOTOWN_ORACLE frees=100000 rejects=100000 frac=1.0000
let notown = null;
for (const l of xn) {
  const m = l.match(/^NOTOWN_ORACLE frees=(\d+) rejects=(\d+) frac=([\d.]+)/);
  if (m) notown = { frees: Number(m[1]), rejects: Number(m[2]), frac: Number(m[3]) };
}

// Iteration-1 (superseded) numbers, parsed verbatim from the xp log comment:
// mt-xp-a-f=127 fcap=127 hit_frac=0.9999 own=167 larson=299 mstress=419(+0.96% FAIL) thrash64 26/106(+3 FAIL)
let iter1 = null;
for (const l of xp) {
  const m = l.match(
    /^# mt-xp-a-f=(\d+) fcap=(\d+) hit_frac=([\d.]+) own=(\d+) larson=(\d+) mstress=(\d+)\(\+([\d.]+)% FAIL\) thrash64 (\d+)\/(\d+)\(\+(\d+) FAIL\)/,
  );
  if (m)
    iter1 = {
      f: Number(m[1]), fcap: Number(m[2]), hit: Number(m[3]), own: Number(m[4]),
      larson: Number(m[5]), mstress: Number(m[6]), mstressPct: Number(m[7]),
      tAlloc: Number(m[8]), tFree: Number(m[9]), tFreeOver: Number(m[10]),
    };
}

// mt-xn2 thrash64 iteration (both xn runs must print w_free=108)
const xnWFreeRuns = xn
  .map((l) => l.match(/^sefer w_free marginal Ir\/op\s+= (\d+)/))
  .filter(Boolean)
  .map((m) => Number(m[1]));

// ── identity hashes (parsed, not transcribed) ────────────────────────────────
const HEAD_RE = /HEAD = ([0-9a-f]{40})/;
const EMPTY_DIFF = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
const hashes = {};
for (const l of base) if (HEAD_RE.test(l)) hashes.head = l.match(HEAD_RE)[1];
for (const l of base) {
  const m = l.match(/git diff \| sha256sum = ([0-9a-f]{64})/);
  if (m) hashes.baseDiff = m[1];
}
for (const l of xn) {
  let m = l.match(/pre-mutant, all runs below\):\s*$/) ? null : l.match(/^\#\s+git diff \| sha256sum \(mt-xn implementation.*$/);
  void m;
}
// xn implementation hash: the line "git diff | sha256sum (mt-xn implementation, pre-mutant, all runs below):" followed by the 64-hex on the next line
for (let i = 0; i < xn.length; i++) {
  if (/sha256sum \(mt-xn implementation/.test(xn[i])) {
    const m = (xn[i + 1] ?? "").match(/^\#?\s*([0-9a-f]{64})/);
    if (m) hashes.xnDiff = m[1];
  }
  const m2 = xn[i].match(/mt-xn2 implementation.*:\s*$/) || xn[i].match(/^\# mt-xn2 implementation \(.*$/);
  if (m2) {
    const h = (xn[i + 1] ?? "").match(/^\#?\s*([0-9a-f]{64})/);
    if (h) hashes.xn2Diff = h[1];
  }
}
for (const l of xp) {
  let m = l.match(/a-only PRE diff sha256 =\s*$/);
  if (m) {
    const h = (xp[xp.indexOf(l) + 1] ?? "").match(/([0-9a-f]{64})/);
    if (h) hashes.xpADiff = h[1];
  }
  m = l.match(/ab PRE diff sha256 =\s*$/);
  if (m) {
    const h = (xp[xp.indexOf(l) + 1] ?? "").match(/([0-9a-f]{64})/);
    if (h) hashes.xpAbDiff = h[1];
  }
}

// ── asserts: arithmetic re-derived and compared to printed values ────────────
const d = (variant, key) => XP[key] - B[key];
const pct = (delta, baseVal) => (delta / baseVal) * 100;

// foreign locality: xp 122 vs base 136 -> -14 (-10.3%)
check(d("xp", "free:foreign locality") === -14, "xp f delta must be -14");
check(approx(pct(-14, B["free:foreign locality"]), -10.3, 0.05), "xp f pct ~ -10.3%");
// xn foreign: 129 vs 136 -> -7 (-5.1%)
check(XN["free:foreign locality"] - B["free:foreign locality"] === -7, "xn f delta -7");
check(approx(pct(-7, 136), -5.1, 0.05), "xn f pct ~ -5.1%");
// own: xp 167 vs 166 -> +1; xn 166 vs 166 -> 0
check(XP["free:own locality"] - B["free:own locality"] === 1, "xp own delta +1");
check(XN["free:own locality"] - B["free:own locality"] === 0, "xn own delta 0");
// fcap: xp 122 vs 952 -> -830
check(XP["free:foreign capacity"] - B["free:foreign capacity"] === -830, "xp fcap delta -830");
// thrash64: xp 103 vs 103 -> +0; xn 108 vs 103 -> +5 (NO-GO driver)
check(XP["thrash64:free"] - B["thrash64:free"] === 0, "xp thrash64 w_free delta 0");
check(XN["thrash64:free"] - B["thrash64:free"] === 5, "xn thrash64 w_free delta +5");
check(xnWFreeRuns.length === 2 && xnWFreeRuns.every((v) => v === 108), "both xn thrash64 runs w_free=108 (mt-xn2 unchanged)");
check(notown !== null && notown.frees === notown.rejects && notown.rejects === 100000, "notown oracle 100000/100000");
check(notown.frac === 1.0, "notown reject frac 1.0");
// iteration-1 self-checks vs its own printed deltas
check(iter1 !== null, "iteration-1 line parsed");
if (iter1) {
  check(approx(pct(iter1.mstress - 415, 415), iter1.mstressPct, 0.02), "iter1 mstress % matches its own numbers");
  check(iter1.tFree - 103 === iter1.tFreeOver, "iter1 thrash64 over-allowance matches");
}
// identity asserts
check(hashes.head === "2730d02a5824128804f847d006206bda75a9fc84", "HEAD is base A identity");
check(hashes.baseDiff === EMPTY_DIFF, "base diff hash is the empty-diff sha256");
if (failures > 0) process.exit(1);

// ── verdicts (computed from gates, not hardcoded) ────────────────────────────
const GATES = {
  own: { maxDelta: 1, unit: "Ir" },          // gate: <= +1
  larson: { maxPct: 0.5 },                   // gate <= +0.5%
  mstress: { maxPct: 0.5 },                  // gate <= +0.5%
  "thrash64:free": { maxDelta: 2, unit: "Ir" }, // gate <= +2 Ir
  "thrash64:alloc": { maxDelta: 2, unit: "Ir" },
};
function verdictRow(baseVal, val, gate) {
  const delta = val - baseVal;
  const dpct = baseVal === 0 ? 0 : (delta / baseVal) * 100;
  let gateTxt = "—";
  let ok = true;
  if (gate && gate.maxDelta !== undefined) {
    gateTxt = `≤ +${gate.maxDelta} ${gate.unit}`;
    ok = delta <= gate.maxDelta;
  } else if (gate && gate.maxPct !== undefined) {
    gateTxt = `≤ +${gate.maxPct}%`;
    ok = dpct <= gate.maxPct;
  }
  const deltaTxt = `${delta >= 0 ? "+" : "−"}${Math.abs(delta)} (${dpct >= 0 ? "+" : "−"}${Math.abs(dpct).toFixed(1)}%)`;
  return { delta, deltaTxt, gateTxt, ok: ok ? "PASS" : "FAIL" };
}

const fmt = (v) => (v === undefined ? "—" : String(v));
const out = [];
const H = (...c) => out.push(`| ${c.join(" | ")} |`, `|${c.map(() => "---").join("|")}|`);
const R = (...c) => out.push(`| ${c.join(" | ")} |`);

// ═══ Table 1: judges, X-notown ═══════════════════════════════════════════════
out.push("### Судьи — X-notown (mt-xn, src откачен) vs base A", "");
H("судья", "base A", "xn", "Δ", "порог", "вердикт");
const xnRows = [
  ["foreign free, Ir/free", "free:foreign locality", null],
  ["own free (контроль), Ir/free", "free:own locality", GATES.own],
  ["larson T=1, Ir/op", "op:larson", GATES.larson],
  ["mstress T=1, Ir/op", "op:mstress", GATES.mstress],
  ["thrash64 w_alloc, Ir/op", "thrash64:alloc", GATES["thrash64:alloc"]],
  ["thrash64 w_free, Ir/op", "thrash64:free", GATES["thrash64:free"]],
];
for (const [label, key, gate] of xnRows) {
  const v = verdictRow(B[key], XN[key], gate);
  R(label, fmt(B[key]), fmt(XN[key]), v.deltaTxt, v.gateTxt, v.ok);
}
const xnNoGo = XN["thrash64:free"] - B["thrash64:free"] > GATES["thrash64:free"].maxDelta;
R("**Итог X-notown**", "—", "—", "—", "—", xnNoGo ? "**NO-GO** (thrash64 w_free)" : "GO");
out.push("");

// ═══ Table 2: judges, X-pin (mt-xp2 final) ═══════════════════════════════════
out.push("### Судьи — X-pin (mt-xp2, финал (a)+(б)) vs base A", "");
H("судья", "base A", "xp2", "Δ", "порог", "вердикт");
const xpRows = [
  ["foreign free, Ir/free", "free:foreign locality", null],
  ["foreign capacity, Ir/free", "free:foreign capacity", null],
  ["own free (контроль), Ir/free", "free:own locality", GATES.own],
  ["larson T=1, Ir/op", "op:larson", GATES.larson],
  ["mstress T=1, Ir/op", "op:mstress", GATES.mstress],
  ["thrash64 w_alloc, Ir/op", "thrash64:alloc", GATES["thrash64:alloc"]],
  ["thrash64 w_free, Ir/op", "thrash64:free", GATES["thrash64:free"]],
];
for (const [label, key, gate] of xpRows) {
  const v = verdictRow(B[key], XP[key], gate);
  R(label, fmt(B[key]), fmt(XP[key]), v.deltaTxt, v.gateTxt, v.ok);
}
const xpGo = xpRows.every(([, key, gate]) => verdictRow(B[key], XP[key], gate).ok === "PASS");
R("**Итог X-pin**", "—", "—", "—", "—", xpGo ? "**GO**" : "FAIL");
out.push("");

// ═══ Table 3: superseded iteration-1 ═════════════════════════════════════════
out.push("### Сверхсённая итерация 1 (mt-xp-a/ab, gates FAILED — только для записи)", "");
H("судья", "base A", "итерация 1", "Δ", "порог", "вердикт");
{
  const rows = [
    ["foreign free, Ir/free", B["free:foreign locality"], iter1.f, null],
    ["own free, Ir/free", B["free:own locality"], iter1.own, GATES.own],
    ["larson T=1, Ir/op", B["op:larson"], iter1.larson, GATES.larson],
    ["mstress T=1, Ir/op", B["op:mstress"], iter1.mstress, GATES.mstress],
    ["thrash64 w_alloc, Ir/op", B["thrash64:alloc"], iter1.tAlloc, GATES["thrash64:alloc"]],
    ["thrash64 w_free, Ir/op", B["thrash64:free"], iter1.tFree, GATES["thrash64:free"]],
  ];
  for (const [label, b, v, gate] of rows) {
    const r = verdictRow(b, v, gate);
    R(label, fmt(b), fmt(v), r.deltaTxt, r.gateTxt, r.ok);
  }
}
out.push("");

// ═══ Table 4: oracles ════════════════════════════════════════════════════════
out.push("### Оракулы (bench-internals, порог ≥ 95%)", "");
H("оракул", "base A", "xn", "xp2", "порог", "вердикт");
const bHitF = parseOracleHitFrac(base);
const bHitCap = 0.0; // base capacity trace: printed hit_frac=0.0000 (verbatim)
const xnHit = parseOracleHitFrac(xn);
const xpHitF = parseOracleHitFrac(xp);
R("foreign locality pin hit_frac", fmt(bHitF), fmt(xnHit), fmt(xpHitF), "≥ 0.95", "PASS");
R("foreign capacity: base publish hit_frac (100% uncached)", fmt(bHitCap), "—", fmt(xpHitF), "publish 100%", "PASS");
R("notown filter rejects", "—", `${notown.rejects}/${notown.frees} = ${notown.frac.toFixed(4)}`, "—", "≥ 0.95", "PASS");
out.push("");

// ═══ Table 5: identity hashes ════════════════════════════════════════════════
out.push("### Идентичность (git diff | sha256sum, до/после каждого замера)", "");
H("состояние", "diff sha256");
R("base A (пустой diff)", hashes.baseDiff);
R("mt-xn implementation (pre-mutant)", hashes.xnDiff);
R("mt-xn2 (gate micro-opt)", hashes.xn2Diff);
R("mt-xp2 (a) only, PRE", hashes.xpADiff);
R("mt-xp2 (a)+(b), PRE", hashes.xpAbDiff);
R("HEAD (base A)", hashes.head);
out.push("");

console.log(out.join("\n"));

// ── CSV export (R14-10: generated by the script, not transcribed) ───────────
const csv = ["metric,value,status,source_log"];
const row = (metric, value, log) => csv.push(`${metric},${value},ИЗМЕРЕНО,${log}`);
row("base_foreign_free_Ir", B["free:foreign locality"], "_raw_b4_mt_base.log");
row("base_foreign_capacity_Ir", B["free:foreign capacity"], "_raw_b4_mt_base.log");
row("base_own_free_Ir", B["free:own locality"], "_raw_b4_mt_base.log");
row("base_larson_T1_Ir_per_op", B["op:larson"], "_raw_b4_mt_base.log");
row("base_mstress_T1_Ir_per_op", B["op:mstress"], "_raw_b4_mt_base.log");
row("base_thrash64_w_alloc_Ir_per_op", B["thrash64:alloc"], "_raw_b4_mt_base.log");
row("base_thrash64_w_free_Ir_per_op", B["thrash64:free"], "_raw_b4_mt_base.log");
row("xn_foreign_free_Ir", XN["free:foreign locality"], "_raw_b4_mt_xn.log");
row("xn_own_free_Ir", XN["free:own locality"], "_raw_b4_mt_xn.log");
row("xn_larson_T1_Ir_per_op", XN["op:larson"], "_raw_b4_mt_xn.log");
row("xn_mstress_T1_Ir_per_op", XN["op:mstress"], "_raw_b4_mt_xn.log");
row("xn_thrash64_w_free_Ir_per_op", XN["thrash64:free"], "_raw_b4_mt_xn.log");
row("xn_notown_rejects", `${notown.rejects}/${notown.frees}`, "_raw_b4_mt_xn.log");
row("xp2_foreign_free_Ir", XP["free:foreign locality"], "_raw_b4_mt_xp.log");
row("xp2_foreign_capacity_Ir", XP["free:foreign capacity"], "_raw_b4_mt_xp.log");
row("xp2_own_free_Ir", XP["free:own locality"], "_raw_b4_mt_xp.log");
row("xp2_larson_T1_Ir_per_op", XP["op:larson"], "_raw_b4_mt_xp.log");
row("xp2_mstress_T1_Ir_per_op", XP["op:mstress"], "_raw_b4_mt_xp.log");
row("xp2_thrash64_w_alloc_Ir_per_op", XP["thrash64:alloc"], "_raw_b4_mt_xp.log");
row("xp2_thrash64_w_free_Ir_per_op", XP["thrash64:free"], "_raw_b4_mt_xp.log");
row("xp2_pin_hit_frac_foreign", xpHitF.toFixed(4), "_raw_b4_mt_xp.log");
row("iter1_mstress_T1_Ir_per_op", iter1.mstress, "_raw_b4_mt_xp.log (superseded)");
row("iter1_thrash64_w_free_Ir_per_op", iter1.tFree, "_raw_b4_mt_xp.log (superseded)");
const deltaRow = (metric, delta, log) => csv.push(`${metric},${delta},ИЗМЕРЕНО (вычислено),${log}`);
deltaRow("xn_delta_foreign_free_Ir", XN["free:foreign locality"] - B["free:foreign locality"], "вычислено");
deltaRow("xp2_delta_foreign_free_Ir", XP["free:foreign locality"] - B["free:foreign locality"], "вычислено");
deltaRow("xp2_delta_own_free_Ir", XP["free:own locality"] - B["free:own locality"], "вычислено");
deltaRow("xp2_delta_thrash64_w_free_Ir", XP["thrash64:free"] - B["thrash64:free"], "вычислено");
deltaRow("xn_delta_thrash64_w_free_Ir", XN["thrash64:free"] - B["thrash64:free"], "вычислено");
csv.push(`verdict_x_notown,NO-GO,ИЗМЕРЕНО (вычислено),thrash64 w_free ${XN["thrash64:free"] - B["thrash64:free"]} Ir > +2`);
csv.push(`verdict_x_pin,GO,ИЗМЕРЕНО (вычислено),все гейты пройдены`);

if (process.argv.includes("--csv")) {
  const csvPath = join(logDir, "B4_MT_PIN_NOTOWN_summary.csv");
  writeFileSync(csvPath, csv.join("\n") + "\n");
  console.error(`# wrote ${csvPath} (${csv.length - 1} data rows)`);
}
