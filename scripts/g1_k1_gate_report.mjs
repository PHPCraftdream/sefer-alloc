#!/usr/bin/env node
// G1-K1 — drain without the class-code re-issue: gate report.
// Derives every table of docs/perf/G1_K1_DRAIN_GATE.md from committed files
// and asserts the arithmetic it prints. Thresholds: docs/perf/g1_k1/
// G1_K1_PREREG.md (fixed before the run):
//   1. win: bulk16k effect CI upper < 0;
//   2. pair16, pair64, larson: effect <= +1 %, CI upper <= +1.5 %;
//   3. other kernels: not (CI lower > 0 AND effect > +2 %).
// Inputs (docs/perf/g1_k1/):
//   vsnull_k1_drain_no_reissue.csv — effect net of null (b4_ablate_vs_null.mjs);
//   vs_mimalloc/rep_summary_k1.csv — main J1′ v2 judge, sefer with K1 vs
//     mimalloc; compared with b2_drain/vs_mimalloc/rep_summary_b2.csv;
//   _raw_callgrind_*.log — callgrind base (g1pmu1) vs K1 (g1k1), whole process.
//
// Usage: node scripts/g1_k1_gate_report.mjs [--csv <summary.csv>]

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
const SHIP = "k1_drain_no_reissue";
const run = byKernel(readCsv(P("g1_k1", `vsnull_${SHIP}.csv`)));
for (const k of KERNELS) {
  const r = run[k];
  if (!r) throw new Error(`kernel ${k} missing`);
  if (r.variant !== SHIP) throw new Error(`${k}: variant ${r.variant}`);
  if (Number(r.reps) !== 20) throw new Error(`${k}: reps ${r.reps}`);
  if (Math.abs(Math.exp(+r.effect) - +r.ratio) > 1e-9) throw new Error(`${k}: ratio != exp(effect)`);
  if (Math.abs(+r.variant_ab_median - +r.null_ab_median - +r.effect) > 1e-12) throw new Error(`${k}: effect != variant − null`);
}

const pct = (x) => (x * 100).toFixed(2);
const word = (ratio) => (ratio < 1 ? `быстрее в ${(1 / ratio).toFixed(3)}×` : `медленнее в ${ratio.toFixed(3)}×`);
const cell = (r) => (+r.effect_ci_lo <= 0 && +r.effect_ci_hi >= 0 ? "равно" : word(+r.ratio));
const NOHARM = ["pair16", "pair64", "larson"];

const fails = [];
if (!(+run.bulk16k.effect_ci_hi < 0)) fails.push("1:bulk16k");
for (const k of NOHARM) {
  const x = run[k];
  if (!(+x.effect <= 0.01 && +x.effect_ci_hi <= 0.015)) fails.push(`2:${k}`);
}
for (const k of KERNELS.filter((k) => !NOHARM.includes(k))) {
  const x = run[k];
  if (+x.effect_ci_lo > 0 && +x.effect > 0.02) fails.push(`3:${k}`);
}
const verdict = fails.length ? "NO-GO" : "GO";

console.log("# G1-K1 gate — effect = median_processes ln(t_variant/t_sefer) net of null (20 processes x 2000 blocks each); «равно» = CI contains 0");
console.log("");
console.log(`| Ядро | ${SHIP}: эффект % [CI] | итог | null точен |`);
console.log("|---|---|---|---|");
for (const k of KERNELS) {
  const r = run[k];
  console.log(`| ${k} | ${pct(+r.effect)} [${pct(+r.effect_ci_lo)}, ${pct(+r.effect_ci_hi)}] | ${cell(r)} | ${r.precise} |`);
}
console.log("");
console.log(`# ${SHIP}: ${verdict}${fails.length ? " — failed " + fails.join(", ") : ""}`);
if (verdict !== "GO") throw new Error(`${SHIP} is not GO; nothing ships`);
const faster = KERNELS.filter((k) => +run[k].effect_ci_hi < 0);
const slower = KERNELS.filter((k) => +run[k].effect_ci_lo > 0);
console.log(`# ${SHIP}: faster on ${faster.length}/${KERNELS.length} (${faster.join(",")}); slower on ${slower.length}/${KERNELS.length}${slower.length ? " (" + slower.join(",") + ")" : ""}`);
for (const k of slower) {
  const x = run[k];
  if (!(+x.effect <= 0.02)) throw new Error(`${k}: slower beyond the +2 % tolerance yet GO`);
}

// Main judge against mimalloc: before = main with B2 (b2_drain), after = + K1.
const before = byKernel(readCsv(P("b2_drain", "vs_mimalloc", "rep_summary_b2.csv")));
const after = byKernel(readCsv(P("g1_k1", "vs_mimalloc", "rep_summary_k1.csv")));
console.log("");
console.log("# J1′ v2, sefer vs mimalloc: t_mimalloc / t_sefer (> 1: sefer faster); before = main with B2 (b2_drain run), after = main 37780bc7 + K1");
console.log("");
console.log("| Ядро | до K1 t_mi/t_sefer | допущено | после K1 t_mi/t_sefer | допущено | sefer после против mimalloc | sefer после против до (ОЦЕНКА, разные прогоны) |");
console.log("|---|---:|---|---:|---|---|---|");
const miRows = [];
for (const k of KERNELS) {
  const b = before[k], a = after[k];
  if (!b || !a) throw new Error(`${k}: vs-mimalloc row missing`);
  if (Number(a.reps) !== 20 || Number(a.blocks_per_process) !== 2000) throw new Error(`${k}: vs-mimalloc run shape`);
  const rb = +b.ratio_t_mi_over_t_sefer, ra = +a.ratio_t_mi_over_t_sefer;
  if (Math.abs(Math.exp(+b.ab_median) - rb) > 1e-9 || Math.abs(Math.exp(+a.ab_median) - ra) > 1e-9) throw new Error(`${k}: ratio != exp(ab_median)`);
  const pt = ra >= 1 ? `sefer быстрее в ${ra.toFixed(3)}×` : `mimalloc быстрее в ${(1 / ra).toFixed(3)}×`;
  const want = { MIMALLOC_FASTER: ra < 1, SEFER_FASTER: ra > 1, EQUAL_CI_HAS_0: true, "not admitted": true }[a.verdict];
  if (!want) throw new Error(`${k}: verdict ${a.verdict} disagrees with ratio ${ra}`);
  const vs = a.verdict === "EQUAL_CI_HAS_0" ? `равно (CI содержит 0; точка: ${pt})` : a.verdict === "not admitted" ? `не допущено A/A (точка: ${pt})` : pt;
  const self = ra / rb;
  if (Math.abs(self * rb - ra) > 1e-12) throw new Error(`${k}: after/before arithmetic`);
  const selfWord = self >= 1 ? `быстрее в ${self.toFixed(3)}×` : `медленнее в ${(1 / self).toFixed(3)}×`;
  console.log(`| ${k} | ${rb.toFixed(4)} | ${b.admitted} | ${ra.toFixed(4)} | ${a.admitted} | ${vs} | ${selfWord} |`);
  miRows.push([rb, b.admitted, ra, a.admitted]);
}
const head = after.bulk16k.head_sha;
if (!KERNELS.every((k) => after[k].head_sha === head)) throw new Error("vs-mimalloc: head_sha differs across kernels");
console.log(`# vs-mimalloc identity: head=${head} sha(diff)=${after.bulk16k.source_diff_sha256.slice(0, 12)} exe=${after.bulk16k.exe_sha256.slice(0, 12)}`);

// Callgrind (WSL, examples/b4_pmu_probe.rs, whole process Ir; instructions, not time).
const cgFiles = { base: ["_raw_callgrind_g1pmu1_cg.log", "_raw_callgrind_g1pmu1_cg2.log"], k1: ["_raw_callgrind_g1k1_cg.log", "_raw_callgrind_g1k1_cg2.log"] };
const SCEN = ["bulk16k", "mixed16_64", "mixed256_1024", "bulk64", "pair16"];
const cg = {};
for (const [v, files] of Object.entries(cgFiles)) {
  cg[v] = {};
  for (const f of files) {
    for (const line of fs.readFileSync(P("g1_k1", f), "utf8").replace(/\r/g, "").split("\n")) {
      const m = line.match(/^FN (\S+) sefer\s+([\d,]+) \(\s*[\d.]+%\)\s+(.*)$/);
      if (!m) continue;
      const [, sc, ir, what] = m;
      cg[v][sc] ??= { total: 0, route: 0 };
      const n = Number(ir.replace(/,/g, ""));
      if (what === "PROGRAM TOTALS") cg[v][sc].total = n;
      else if (what.includes("segment_table/route_slots.rs:") && what.includes("try_drain_freelist_batch")) cg[v][sc].route += n;
    }
  }
}
console.log("");
console.log("# callgrind, b4_pmu_probe, sefer arm, whole-process Ir (instructions, not time); route = route_slots.rs self Ir inside try_drain_freelist_batch");
console.log("");
console.log("| Форма | Ir база | Ir K1 | изменение | route база | route K1 |");
console.log("|---|---:|---:|---:|---:|---:|");
const cgRows = [];
for (const sc of SCEN) {
  const b = cg.base[sc], a = cg.k1[sc];
  if (!b?.total || !a?.total) throw new Error(`callgrind ${sc}: totals missing`);
  const d = (a.total - b.total) / b.total;
  console.log(`| ${sc} | ${b.total} | ${a.total} | ${pct(d)} % | ${b.route} | ${a.route} |`);
  cgRows.push([sc, b.total, a.total, d, b.route, a.route]);
}
if (!(cg.base.bulk16k.route > 0 && cg.k1.bulk16k.route === 0)) throw new Error("callgrind: drain route cost not removed on bulk16k");
if (!(cg.k1.bulk16k.total < cg.base.bulk16k.total)) throw new Error("callgrind: K1 does not reduce bulk16k Ir");

console.log("");
console.log(`VERDICT=${verdict}`);
if (csvOut) {
  const h = ["kernel", "effect", "ci_lo", "ci_hi", "null_precise", "before_t_mi_over_t_sefer", "before_admitted", "after_t_mi_over_t_sefer", "after_admitted"].join(",");
  const body = KERNELS.map((k, i) => [k, run[k].effect, run[k].effect_ci_lo, run[k].effect_ci_hi, run[k].precise, ...miRows[i]].join(","));
  fs.writeFileSync(csvOut, [h, ...body].join("\n") + "\n");
  console.log(`# wrote ${path.basename(csvOut)}`);
  const cgOut = csvOut.replace(/\.csv$/, "_callgrind.csv");
  fs.writeFileSync(cgOut, ["scenario,base_ir,k1_ir,rel_change,base_route_ir,k1_route_ir", ...cgRows.map((r) => r.join(","))].join("\n") + "\n");
  console.log(`# wrote ${path.basename(cgOut)}`);
}
