#!/usr/bin/env node
// B1 — flush single pass + cold full-magazine free tail: gate report.
// Derives every table of docs/perf/B1_FLUSH_FREE_GATE.md from committed CSVs
// and asserts the arithmetic it prints. Thresholds: docs/perf/b1_flush_free/
// B1_PREREG.md (fixed before the run):
//   1. win: bulk16k OR bulk64 effect CI upper < 0;
//   2. pair16, pair64, larson: effect <= +1 %, CI upper <= +1.5 %;
//   3. other kernels: not (CI lower > 0 AND effect > +2 %).
// Inputs (docs/perf/b1_flush_free/):
//   vsnull_<variant>.csv — effect net of null (scripts/b4_ablate_vs_null.mjs);
//   vs_mimalloc/rep_summary_b1ac.csv (optional) — main J1′ v2 judge,
//     sefer with b1ac vs mimalloc; compared with
//     b1_depot_gate/vs_mimalloc/rep_summary_depot.csv (main before B1).
//
// Usage: node scripts/b1_flush_free_gate_report.mjs [--csv <summary.csv>]

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
const VARIANTS = ["b1a_flush", "b1c_free_cold", "b1ac"];
const runs = Object.fromEntries(
  VARIANTS.map((v) => {
    const rows = byKernel(readCsv(P("b1_flush_free", `vsnull_${v}.csv`)));
    for (const k of KERNELS) {
      const r = rows[k];
      if (!r) throw new Error(`${v}: kernel ${k} missing`);
      if (r.variant !== v) throw new Error(`${v} ${k}: variant ${r.variant}`);
      if (Number(r.reps) !== 20) throw new Error(`${v} ${k}: reps ${r.reps}`);
      if (Math.abs(Math.exp(+r.effect) - +r.ratio) > 1e-9) throw new Error(`${v} ${k}: ratio != exp(effect)`);
      if (Math.abs(+r.variant_ab_median - +r.null_ab_median - +r.effect) > 1e-12) throw new Error(`${v} ${k}: effect != variant − null`);
    }
    return [v, rows];
  }),
);

const pct = (x) => (x * 100).toFixed(2);
const word = (ratio) => (ratio < 1 ? `быстрее в ${(1 / ratio).toFixed(3)}×` : `медленнее в ${ratio.toFixed(3)}×`);
const cell = (r) => (+r.effect_ci_lo <= 0 && +r.effect_ci_hi >= 0 ? "равно" : word(+r.ratio));

function gate(rows) {
  const fails = [];
  if (!["bulk16k", "bulk64"].some((k) => +rows[k].effect_ci_hi < 0)) fails.push("1:bulk");
  for (const k of ["pair16", "pair64", "larson"]) {
    const x = rows[k];
    if (!(+x.effect <= 0.01 && +x.effect_ci_hi <= 0.015)) fails.push(`2:${k}`);
  }
  for (const k of KERNELS.filter((k) => !["pair16", "pair64", "larson"].includes(k))) {
    const x = rows[k];
    if (+x.effect_ci_lo > 0 && +x.effect > 0.02) fails.push(`3:${k}`);
  }
  return fails;
}

console.log("# B1 gate — effect = median_processes ln(t_variant/t_sefer) net of null (20 processes x 2000 blocks each); «равно» = CI contains 0");
console.log("");
console.log("| Ядро | " + VARIANTS.map((v) => `${v}: эффект % [CI]`).join(" | ") + " | b1ac | null точен |");
console.log("|---|" + VARIANTS.map(() => "---").join("|") + "|---|---|");
for (const k of KERNELS) {
  const cells = VARIANTS.map((v) => {
    const r = runs[v][k];
    return `${pct(+r.effect)} [${pct(+r.effect_ci_lo)}, ${pct(+r.effect_ci_hi)}]`;
  });
  console.log(`| ${k} | ${cells.join(" | ")} | ${cell(runs.b1ac[k])} | ${runs.b1ac[k].precise} |`);
}
const verdicts = VARIANTS.map((v) => {
  const f = gate(runs[v]);
  return { v, fails: f, verdict: f.length ? "NO-GO" : "GO" };
});
console.log("");
for (const x of verdicts) console.log(`# ${x.v}: ${x.verdict}${x.fails.length ? " — failed " + x.fails.join(", ") : ""}`);
const ship = verdicts.find((x) => x.v === "b1ac");
if (ship.verdict !== "GO") throw new Error("b1ac is not GO; nothing ships");
const faster = KERNELS.filter((k) => +runs.b1ac[k].effect_ci_hi < 0);
const slower = KERNELS.filter((k) => +runs.b1ac[k].effect_ci_lo > 0);
console.log(`# b1ac: faster on ${faster.length}/${KERNELS.length} (${faster.join(",")}); slower on ${slower.length}/${KERNELS.length}${slower.length ? " (" + slower.join(",") + ")" : ""}`);

const rows = KERNELS.map((k) => [k, ...VARIANTS.flatMap((v) => [runs[v][k].effect, runs[v][k].effect_ci_lo, runs[v][k].effect_ci_hi]), runs.b1ac[k].precise]);
let miRows = null;
const miPath = P("b1_flush_free", "vs_mimalloc", "rep_summary_b1ac.csv");
if (fs.existsSync(miPath)) {
  const before = byKernel(readCsv(P("b1_depot_gate", "vs_mimalloc", "rep_summary_depot.csv")));
  const after = byKernel(readCsv(miPath));
  console.log("");
  console.log("# J1′ v2, sefer vs mimalloc: t_mimalloc / t_sefer (> 1: sefer faster); before = main with the depot, after = + b1ac");
  console.log("");
  console.log("| Ядро | до B1 t_mi/t_sefer | допущено | после B1 t_mi/t_sefer | допущено | sefer после против mimalloc | sefer после против до (ОЦЕНКА, разные прогоны) |");
  console.log("|---|---:|---|---:|---|---|---|");
  miRows = [];
  for (const k of KERNELS) {
    const b = before[k], a = after[k];
    const rb = +b.ratio_t_mi_over_t_sefer, ra = +a.ratio_t_mi_over_t_sefer;
    if (Math.abs(Math.exp(+b.ab_median) - rb) > 1e-9 || Math.abs(Math.exp(+a.ab_median) - ra) > 1e-9) throw new Error(`${k}: ratio != exp(ab_median)`);
    const pt = ra >= 1 ? `sefer быстрее в ${ra.toFixed(3)}×` : `mimalloc быстрее в ${(1 / ra).toFixed(3)}×`;
    const want = { MIMALLOC_FASTER: ra < 1, SEFER_FASTER: ra > 1, EQUAL_CI_HAS_0: true, "not admitted": true }[a.verdict];
    if (!want) throw new Error(`${k}: verdict ${a.verdict} disagrees with ratio ${ra}`);
    const vs = a.verdict === "EQUAL_CI_HAS_0" ? `равно (CI содержит 0; точка: ${pt})` : a.verdict === "not admitted" ? `не допущено A/A (точка: ${pt})` : pt;
    const self = ra / rb; // t_before / t_after of sefer, mimalloc arm as the common yardstick
    if (Math.abs(self * rb - ra) > 1e-12) throw new Error(`${k}: after/before arithmetic`);
    const selfWord = self >= 1 ? `быстрее в ${self.toFixed(3)}×` : `медленнее в ${(1 / self).toFixed(3)}×`;
    console.log(`| ${k} | ${rb.toFixed(4)} | ${b.admitted} | ${ra.toFixed(4)} | ${a.admitted} | ${vs} | ${selfWord} |`);
    miRows.push([rb, b.admitted, ra, a.admitted]);
  }
}

// Callgrind (WSL, instructions, not time): _raw_callgrind_bulk.txt, one block per variant.
const CG_PAIRS = 44 * 16384; // (40 + 40/10 warm-up) passes x 16384 alloc+free pairs, examples/b4_pmu_probe.rs `bulk`
const CG_FNS = [
  ["free", /b4_pmu_probe::sefer_dealloc$/],
  ["free_magazine_full", /HeapCore>::free_magazine_full$/],
  ["flush_class_internal", /AllocCore>::flush_class_internal$/],
  ["try_drain_freelist_batch", /AllocCore>::try_drain_freelist_batch$/],
  ["alloc", /b4_pmu_probe::sefer_alloc$/],
  ["total", /PROGRAM TOTALS$/],
];
const CG_VARIANTS = ["base", "b1a_flush", "b1c_free_cold", "b1ac"];
const cgText = fs.readFileSync(P("b1_flush_free", "_raw_callgrind_bulk.txt"), "utf8").replace(/\r/g, "");
if (!cgText.includes(`(44 passes x 16384 = ${CG_PAIRS} alloc+free pairs)`)) throw new Error("callgrind raw: pair count header mismatch");
const cg = {};
for (const block of cgText.split(/^== variant=/m).slice(1)) {
  const lines = block.split("\n");
  const v = lines[0].trim();
  cg[v] = Object.fromEntries(
    CG_FNS.map(([k, re]) => {
      const line = lines.find((l) => re.test(l.trim()));
      return [k, line ? Number(line.trim().split(/\s+/)[0].replace(/,/g, "")) : 0];
    }),
  );
  if (!cg[v].total || !cg[v].free || !cg[v].flush_class_internal) throw new Error(`callgrind ${v}: missing rows`);
}
for (const v of CG_VARIANTS) if (!cg[v]) throw new Error(`callgrind: variant ${v} missing`);
for (const v of ["base", "b1a_flush"]) if (cg[v].free_magazine_full !== 0) throw new Error(`callgrind ${v}: free_magazine_full before b1c`);
for (const v of ["b1c_free_cold", "b1ac"]) if (cg[v].free_magazine_full === 0) throw new Error(`callgrind ${v}: free_magazine_full missing`);
const per = (x) => (x / CG_PAIRS).toFixed(1);
console.log("");
console.log(`# callgrind bulk 64 B x 16384: Ir per alloc+free pair (${CG_PAIRS} pairs), self cost per function; instructions, not time`);
console.log("");
console.log("| Функция | " + CG_VARIANTS.join(" | ") + " |");
console.log("|---|" + CG_VARIANTS.map(() => "---:").join("|") + "|");
for (const [k] of CG_FNS) console.log(`| ${k} | ${CG_VARIANTS.map((v) => per(cg[v][k])).join(" | ")} |`);
const dTotal = (cg.base.total - cg.b1ac.total) / CG_PAIRS;
const dFree = (cg.base.free - cg.b1ac.free - cg.b1ac.free_magazine_full) / CG_PAIRS;
const dFlush = (cg.base.flush_class_internal - cg.b1ac.flush_class_internal) / CG_PAIRS;
if (!(dTotal > 0)) throw new Error("callgrind: b1ac does not reduce total Ir");
console.log(`# b1ac vs base: total −${dTotal.toFixed(1)} Ir/pair (free incl. free_magazine_full −${dFree.toFixed(1)}, flush −${dFlush.toFixed(1)})`);

console.log("");
console.log(`VERDICT=${ship.verdict}`);
if (csvOut) {
  const head = ["kernel", ...VARIANTS.flatMap((v) => [`${v}_effect`, `${v}_ci_lo`, `${v}_ci_hi`]), "null_precise", ...(miRows ? ["before_t_mi_over_t_sefer", "before_admitted", "after_t_mi_over_t_sefer", "after_admitted"] : [])].join(",");
  const body = rows.map((r, i) => [...r, ...(miRows ? miRows[i] : [])].join(","));
  fs.writeFileSync(csvOut, [head, ...body].join("\n") + "\n");
  console.log(`# wrote ${path.basename(csvOut)}`);
  const cgOut = csvOut.replace(/\.csv$/, "_callgrind.csv");
  const cgHead = ["variant", ...CG_FNS.map(([k]) => `${k}_ir`), "pairs"].join(",");
  const cgBody = CG_VARIANTS.map((v) => [v, ...CG_FNS.map(([k]) => cg[v][k]), CG_PAIRS].join(","));
  fs.writeFileSync(cgOut, [cgHead, ...cgBody].join("\n") + "\n");
  console.log(`# wrote ${path.basename(cgOut)}`);
}
