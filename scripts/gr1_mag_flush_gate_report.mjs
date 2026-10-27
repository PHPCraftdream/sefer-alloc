#!/usr/bin/env node
// GR-1 — magazine flush without per-block re-checks: gate report.
// Derives every table of docs/perf/GR1_MAG_FLUSH_GATE.md from committed raw
// data and asserts the arithmetic it prints. Thresholds: docs/perf/gr1_mag_flush/
// GR1_PREREG.md (fixed before the run):
//   1. win: bulk16k OR bulk64 effect CI upper < 0;
//   2. pair16, pair64, larson: effect <= +1 %, CI upper <= +1.5 %;
//   3. other kernels: not (CI lower > 0 AND effect > +2 %).
// Inputs (docs/perf/gr1_mag_flush/):
//   vsnull_gr1_mag_flush.csv — J1′ v2 effect net of null (scripts/b4_ablate_vs_null.mjs);
//   _raw_callgrind_growth16_{base,gr1}.log — IR_TOTAL lines of .rush/wsl/ut_prof_u31.sh.
//
// Usage: node scripts/gr1_mag_flush_gate_report.mjs [--csv <summary.csv>]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")), "..");
const P = (...p) => path.join(root, "docs", "perf", "gr1_mag_flush", ...p);
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

const VARIANT = "gr1_mag_flush";
const REPS = 20;
const KERNELS = ["pair16", "pair64", "pair_rt", "bulk64", "bulk16k", "churn16", "churn64", "churn256", "churn1024", "vec512", "larson"];
const rows = Object.fromEntries(readCsv(P(`vsnull_${VARIANT}.csv`)).map((r) => [r.kernel, r]));
for (const k of KERNELS) {
  const r = rows[k];
  if (!r) throw new Error(`kernel ${k} missing`);
  if (r.variant !== VARIANT) throw new Error(`${k}: variant ${r.variant}`);
  if (Number(r.reps) !== REPS) throw new Error(`${k}: reps ${r.reps}`);
  if (Math.abs(Math.exp(+r.effect) - +r.ratio) > 1e-9) throw new Error(`${k}: ratio != exp(effect)`);
  if (Math.abs(+r.variant_ab_median - +r.null_ab_median - +r.effect) > 1e-12) throw new Error(`${k}: effect != variant − null`);
  if (!(+r.effect_ci_lo <= +r.effect && +r.effect <= +r.effect_ci_hi)) throw new Error(`${k}: effect outside its CI`);
}

const pct = (x) => (x * 100).toFixed(2);
const word = (ratio) => (ratio < 1 ? `быстрее в ${(1 / ratio).toFixed(3)}×` : `медленнее в ${ratio.toFixed(3)}×`);
const cell = (r) => (+r.effect_ci_lo <= 0 && +r.effect_ci_hi >= 0 ? "равно" : word(+r.ratio));

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
const verdict = fails.length ? "NO-GO" : "GO";

console.log(`# GR-1 gate — effect = median over ${REPS} processes of ln(t_variant/t_sefer), net of null (2000 blocks each); «равно» = CI contains 0`);
console.log("");
console.log("| Ядро | эффект % [95 % CI] | GR-1 против main | null точен |");
console.log("|---|---|---|---|");
for (const k of KERNELS) {
  const r = rows[k];
  console.log(`| ${k} | ${pct(+r.effect)} [${pct(+r.effect_ci_lo)}, ${pct(+r.effect_ci_hi)}] | ${cell(r)} | ${r.precise} |`);
}
const faster = KERNELS.filter((k) => +rows[k].effect_ci_hi < 0);
const slower = KERNELS.filter((k) => +rows[k].effect_ci_lo > 0);
console.log("");
console.log(`# faster on ${faster.length}/${KERNELS.length} kernels (${faster.join(",") || "none"}); slower on ${slower.length}/${KERNELS.length}${slower.length ? " (" + slower.join(",") + ")" : ""}`);
console.log(`# ${VARIANT}: ${verdict}${fails.length ? " — failed " + fails.join(", ") : ""}`);

// Callgrind (WSL, instructions, not time): growth16 probe, IR_TOTAL <mode> <n> <ir>.
function irTotals(file) {
  const text = fs.readFileSync(P(file), "utf8").replace(/\r/g, "");
  const out = {};
  for (const m of text.matchAll(/^IR_TOTAL (sefer|mimalloc) (\d+) (\d+)$/gm)) out[`${m[1]}_${m[2]}`] = Number(m[3]);
  return out;
}
const base = irTotals("_raw_callgrind_growth16_base.log");
const gr1 = irTotals("_raw_callgrind_growth16_gr1.log");
const NS = [32768, 65536, 131072];
console.log("");
console.log("# callgrind growth16 (WSL): total Ir; instructions, not time");
console.log("");
console.log("| n | main | GR-1 | Δ % | mimalloc | main против mimalloc | GR-1 против mimalloc |");
console.log("|---:|---:|---:|---:|---:|---|---|");
const cgRows = [];
for (const n of NS) {
  const b = base[`sefer_${n}`], g = gr1[`sefer_${n}`], mi = gr1[`mimalloc_${n}`];
  if (!b || !g || !mi) throw new Error(`callgrind n=${n}: missing rows`);
  const d = (g - b) / b;
  if (Math.abs(b * (1 + d) - g) > 1e-6) throw new Error(`callgrind n=${n}: delta arithmetic`);
  if (!(g < b)) throw new Error(`callgrind n=${n}: GR-1 does not reduce Ir`);
  console.log(`| ${n} | ${b} | ${g} | ${pct(d)} | ${mi} | медленнее в ${(b / mi).toFixed(2)}× | медленнее в ${(g / mi).toFixed(2)}× |`);
  cgRows.push([n, b, g, d.toFixed(6), mi]);
}

// Bulk callgrind (b4_pmu_probe bulk sefer 64 16384 40): self Ir per function.
const CG_PAIRS = 44 * 16384; // (40 + 40/10 warm-up) passes x 16384 alloc+free pairs
const bulkText = fs.readFileSync(P("_raw_callgrind_bulk.log"), "utf8").replace(/\r/g, "");
const bulk = {};
for (const block of bulkText.split(/^== variant=/m).slice(1)) {
  const lines = block.split("\n");
  const v = lines[0].trim();
  const num = (re) => {
    const l = lines.find((x) => re.test(x));
    return l ? Number(l.trim().split(/\s+/)[0].replace(/,/g, "")) : 0;
  };
  bulk[v] = { total: num(/PROGRAM TOTALS/), flush: num(/flush_class_(internal|impl::<false>) /), free: num(/sefer_dealloc /), alloc: num(/sefer_alloc /) };
}
for (const v of ["base", "gr1"]) if (!bulk[v] || !bulk[v].total || !bulk[v].flush) throw new Error(`bulk callgrind ${v}: missing rows`);
if (bulk.base.free !== bulk.gr1.free || bulk.base.alloc !== bulk.gr1.alloc) throw new Error("bulk callgrind: free/alloc must be unchanged by GR-1");
const per = (x) => (x / CG_PAIRS).toFixed(1);
console.log("");
console.log(`# callgrind bulk 64 B x 16384 (WSL): Ir per alloc+free pair (${CG_PAIRS} pairs); instructions, not time`);
console.log("");
console.log("| | main | GR-1 | Δ % |");
console.log("|---|---:|---:|---:|");
const bulkRows = [];
for (const k of ["flush", "free", "alloc", "total"]) {
  const d = (bulk.gr1[k] - bulk.base[k]) / bulk.base[k];
  console.log(`| ${k} | ${per(bulk.base[k])} | ${per(bulk.gr1[k])} | ${pct(d)} |`);
  bulkRows.push([k, bulk.base[k], bulk.gr1[k], d.toFixed(6)]);
}
const timeBulk = +rows.bulk16k.effect;
console.log(`# bulk16k: Ir total ${pct((bulk.gr1.total - bulk.base.total) / bulk.base.total)} %, time ${pct(timeBulk)} % — fewer instructions, more time`);

console.log("");
console.log(`VERDICT=${verdict}`);
if (csvOut) {
  const head = ["kernel", "effect", "ci_lo", "ci_hi", "ratio", "null_precise", "reps"].join(",");
  const body = KERNELS.map((k) => [k, rows[k].effect, rows[k].effect_ci_lo, rows[k].effect_ci_hi, rows[k].ratio, rows[k].precise, rows[k].reps].join(","));
  fs.writeFileSync(csvOut, [head, ...body].join("\n") + "\n");
  console.log(`# wrote ${path.basename(csvOut)}`);
  const cgOut = csvOut.replace(/\.csv$/, "_callgrind.csv");
  fs.writeFileSync(cgOut, ["n,main_ir,gr1_ir,delta,mimalloc_ir", ...cgRows.map((r) => r.join(","))].join("\n") + "\n");
  console.log(`# wrote ${path.basename(cgOut)}`);
  const bulkOut = csvOut.replace(/\.csv$/, "_callgrind_bulk.csv");
  fs.writeFileSync(bulkOut, ["fn,main_ir,gr1_ir,delta", ...bulkRows.map((r) => r.join(","))].join("\n") + "\n");
  console.log(`# wrote ${path.basename(bulkOut)}`);
}
