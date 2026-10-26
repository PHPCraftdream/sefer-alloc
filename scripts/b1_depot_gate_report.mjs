#!/usr/bin/env node
// DP-5 — production depot gate (docs/design/2026-10-26-depot-design.md,
// «Порог приёмки»). Derives every table of docs/perf/B1_DEPOT_PRODUCTION_GATE.md
// from committed CSVs; asserts the arithmetic it prints.
//
// Inputs (all under docs/perf/):
//   b1_depot_gate/attempt{1,2}/vsnull_*.csv — effect net of null
//     (scripts/b4_ablate_vs_null.mjs, branch b4-ablate);
//   B4_INTERLEAVE_JUDGE_rep_summary.csv — J1′ v2 sefer vs mimalloc, base;
//   b1_depot_gate/vs_mimalloc/rep_summary_depot.csv — same judge, depot.
// Thresholds (pre-registered):
//   1. bulk64, churn64: effect <= ln(1/1.25), CI upper < 0;
//   2. pair16, pair64, larson: effect <= +1 %, CI upper <= +1.5 %;
//   3. other kernels admitted by the base J1′ v2 run: no significant
//      slowdown above 2 % (not: CI lower > 0 AND effect > 2 %).
//
// Usage: node scripts/b1_depot_gate_report.mjs [--csv <summary.csv>]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")), "..");
const P = (...p) => path.join(root, "docs", "perf", ...p);
let csvOut = null;
for (let i = 2; i < process.argv.length; i++) {
  if (process.argv[i] === "--csv") csvOut = process.argv[++i];
  else throw new Error(`unknown argument ${process.argv[i]}`);
}

function readCsv(p) {
  const [head, ...lines] = fs.readFileSync(p, "utf8").split("\n").filter(Boolean);
  const cols = head.split(",");
  return lines.map((l) => {
    const c = l.split(",");
    return Object.fromEntries(cols.map((k, i) => [k, c[i]]));
  });
}
const byKernel = (rows) => Object.fromEntries(rows.map((r) => [r.kernel, r]));

const KERNELS = ["pair16", "pair64", "pair_rt", "bulk64", "bulk16k", "churn16", "churn64", "churn256", "churn1024", "vec512", "larson"];
const att = [1, 2].map((n) => {
  const dir = P("b1_depot_gate", `attempt${n}`);
  const f = fs.readdirSync(dir).find((x) => x.startsWith("vsnull_"));
  const rows = byKernel(readCsv(path.join(dir, f)));
  for (const k of KERNELS) {
    const r = rows[k];
    if (!r) throw new Error(`attempt${n}: kernel ${k} missing`);
    if (Number(r.reps) !== 20) throw new Error(`attempt${n} ${k}: reps ${r.reps}`);
    if (Math.abs(Math.exp(+r.effect) - +r.ratio) > 1e-9) throw new Error(`attempt${n} ${k}: ratio != exp(effect)`);
  }
  return { n, variant: rows[KERNELS[0]].variant, rows };
});
const base = byKernel(readCsv(P("B4_INTERLEAVE_JUDGE_rep_summary.csv")));
const depot = byKernel(readCsv(P("b1_depot_gate", "vs_mimalloc", "rep_summary_depot.csv")));

const pct = (x) => (x * 100).toFixed(2);
const word = (ratio) => (ratio < 1 ? `быстрее в ${(1 / ratio).toFixed(3)}×` : `медленнее в ${ratio.toFixed(3)}×`);

const ADMITTED_BASE = KERNELS.filter((k) => base[k]?.admitted === "yes");
function gate(rows) {
  const fails = [];
  for (const k of ["bulk64", "churn64"]) {
    const x = rows[k];
    if (!(+x.effect <= Math.log(1 / 1.25) && +x.effect_ci_hi < 0)) fails.push(`1:${k}`);
  }
  for (const k of ["pair16", "pair64", "larson"]) {
    const x = rows[k];
    if (!(+x.effect <= 0.01 && +x.effect_ci_hi <= 0.015)) fails.push(`2:${k}`);
  }
  for (const k of ADMITTED_BASE.filter((k) => !["bulk64", "churn64", "pair16", "pair64", "larson"].includes(k))) {
    const x = rows[k];
    if (+x.effect_ci_lo > 0 && +x.effect > 0.02) fails.push(`3:${k}`);
  }
  return fails;
}

console.log("# DP-5 production depot gate — effect = median_processes ln(t_depot/t_sefer) net of null (20 processes x 2000 blocks each)");
console.log(`# threshold-3 kernels (admitted in the base J1′ v2 run): ${ADMITTED_BASE.join(",")}`);
console.log("");
console.log("| Ядро | попытка 1: эффект % [CI] | попытка 1 | попытка 2: эффект % [CI] | попытка 2 | null точен (2) |");
console.log("|---|---|---|---|---|---|");
for (const k of KERNELS) {
  const a = att[0].rows[k], b = att[1].rows[k];
  console.log(`| ${k} | ${pct(+a.effect)} [${pct(+a.effect_ci_lo)}, ${pct(+a.effect_ci_hi)}] | ${word(+a.ratio)} | ${pct(+b.effect)} [${pct(+b.effect_ci_lo)}, ${pct(+b.effect_ci_hi)}] | ${word(+b.ratio)} | ${b.precise} |`);
}
const verdicts = att.map((a) => {
  const f = gate(a.rows);
  return { n: a.n, variant: a.variant, fails: f, verdict: f.length ? "NO-GO" : "GO" };
});
console.log("");
for (const v of verdicts) console.log(`# attempt ${v.n} (${v.variant}): ${v.verdict}${v.fails.length ? " — failed " + v.fails.join(", ") : ""}`);
if (verdicts[0].verdict !== "NO-GO" || !verdicts[0].fails.includes("3:bulk16k")) throw new Error("attempt 1 verdict changed");

console.log("");
console.log("# J1′ v2, sefer vs mimalloc: ratio = t_mimalloc / t_sefer (> 1: sefer faster); base = main before the depot, depot = branch b4-depot");
console.log("");
console.log("| Ядро | до склада t_mi/t_sefer | допущено | со складом t_mi/t_sefer | допущено | sefer со складом против sefer до |");
console.log("|---|---:|---|---:|---|---|");
const rows = [];
for (const k of KERNELS) {
  const b = base[k], d = depot[k];
  const rb = +b.ratio_t_mi_over_t_sefer, rd = +d.ratio_t_mi_over_t_sefer;
  if (Math.abs(Math.exp(+b.ab_median) - rb) > 1e-9 || Math.abs(Math.exp(+d.ab_median) - rd) > 1e-9) throw new Error(`${k}: ratio != exp(ab_median)`);
  // t_sefer_depot / t_sefer_base, both measured against the same mimalloc
  const self = rb / rd;
  if (Math.abs(self * rd - rb) > 1e-12) throw new Error("self ratio arithmetic");
  console.log(`| ${k} | ${rb.toFixed(4)} | ${b.admitted} | ${rd.toFixed(4)} | ${d.admitted} | ${word(self)} (ОЦЕНКА, разные прогоны) |`);
  const e2 = att[1].rows[k];
  rows.push([k, att[0].rows[k].effect, att[0].rows[k].effect_ci_lo, att[0].rows[k].effect_ci_hi, e2.effect, e2.effect_ci_lo, e2.effect_ci_hi, e2.precise, rb, b.admitted, rd, d.admitted]);
}
console.log("");
console.log(`VERDICT=${verdicts[1].verdict}`);
if (csvOut) {
  const head = "kernel,att1_effect,att1_ci_lo,att1_ci_hi,att2_effect,att2_ci_lo,att2_ci_hi,att2_null_precise,base_t_mi_over_t_sefer,base_admitted,depot_t_mi_over_t_sefer,depot_admitted";
  fs.writeFileSync(csvOut, [head, ...rows.map((r) => r.join(","))].join("\n") + "\n");
  console.log(`# wrote ${path.basename(csvOut)}`);
}
