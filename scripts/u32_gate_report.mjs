#!/usr/bin/env node
// U32 gate report generator: derives every number of
// docs/perf/U32_CARVE_RANGE_GATE.md from the committed raws in
// docs/perf/u32_carve_range/ (judge JSONL + callgrind logs), writes the
// summary CSV and prints the Markdown tables. Same statistic as
// scripts/u32_growth_judge.mjs: median over pairs of ln(B/A) total time,
// 95 % CI by bootstrap over pairs (10 000 resamples, fixed seed).
//
// Usage: node scripts/u32_gate_report.mjs [--csv <path>] [--check]
//   --check: fail if the CSV on disk differs from the regenerated one.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import assert from "node:assert/strict";

const DIR = "docs/perf/u32_carve_range";
const csvPath = (() => {
  const i = process.argv.indexOf("--csv");
  return i < 0 ? "docs/perf/U32_CARVE_RANGE_GATE_summary.csv" : process.argv[i + 1];
})();
const check = process.argv.includes("--check");

const median = (v) => {
  const s = [...v].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
function judge(file) {
  const lines = readFileSync(`${DIR}/${file}`, "utf8").trim().split("\n").map((l) => JSON.parse(l));
  const head = lines[0];
  const rows = lines.slice(1);
  assert.equal(rows.length, head.pairs, `${file}: pair count`);
  rows.forEach((r, i) => assert.equal(r.order, i % 2 === 0 ? "ab" : "ba", `${file}: ABBA order`));
  const logs = rows.map((r) => Math.log(r.b.total_ns / r.a.total_ns));
  let seed = 0x5eed32n;
  const rnd = () => {
    seed = (seed * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn;
    return Number(seed >> 11n) / 2 ** 53;
  };
  const boots = [];
  for (let k = 0; k < 10000; k++) {
    boots.push(median(Array.from({ length: logs.length }, () => logs[Math.floor(rnd() * logs.length)])));
  }
  boots.sort((x, y) => x - y);
  const eff = median(logs);
  const lo = boots[249];
  const hi = boots[9749];
  assert.ok(lo <= eff && eff <= hi, `${file}: CI must bracket the effect`);
  const med = (f) => median(rows.map(f));
  return {
    file, label: head.label, n: head.n, pairs: head.pairs, a_sha256: head.a_sha256, b_sha256: head.b_sha256,
    effect: eff, lo, hi,
    a_alloc: med((r) => r.a.alloc_ns), b_alloc: med((r) => r.b.alloc_ns),
    a_free: med((r) => r.a.free_ns), b_free: med((r) => r.b.free_ns),
  };
}
function irTotals(file) {
  const t = {};
  for (const l of readFileSync(`${DIR}/${file}`, "utf8").split("\n")) {
    const m = /^IR_TOTAL (\w+) (\d+) (\d+)/.exec(l);
    if (m) t[`${m[1]}:${m[2]}`] = Number(m[3]);
  }
  return t;
}

const runs = [
  ["v1", "_raw_judge_v1_null_131072.jsonl"], ["v1", "_raw_judge_v1_131072.jsonl"], ["v1", "_raw_judge_v1_32768.jsonl"],
  ["v2", "_raw_judge_v2_null_131072.jsonl"], ["v2", "_raw_judge_v2_131072.jsonl"], ["v2", "_raw_judge_v2_32768.jsonl"],
].map(([v, f]) => ({ variant: v, ...judge(f) }));
for (const r of runs) {
  if (r.label === "null") assert.equal(r.a_sha256, r.b_sha256, `${r.file}: null must run one binary`);
  else assert.notEqual(r.a_sha256, r.b_sha256, `${r.file}: arms must differ`);
}
const pct = (x) => (100 * x).toFixed(2);
const verdict = (v) => {
  const gate = runs.find((r) => r.variant === v && r.label !== "null" && r.n === 131072);
  const nul = runs.find((r) => r.variant === v && r.label === "null");
  const win = gate.hi < 0;
  const calibrated = nul.lo <= 0 && 0 <= nul.hi;
  return { win, calibrated, go: win && calibrated };
};

const base = irTotals("_raw_callgrind_growth16_base.log");
const ir = { v1: irTotals("_raw_callgrind_growth16_v1.log"), v2: irTotals("_raw_callgrind_growth16_v2.log") };
const ns = [32768, 65536, 131072];

const csv = ["kind,variant,label,n,pairs_or_runs,effect_pct,ci_lo_pct,ci_hi_pct,a_alloc_ns,b_alloc_ns,a_free_ns,b_free_ns,base_ir,variant_ir,delta_ir_pct"];
for (const r of runs) {
  csv.push(["time", r.variant, r.label, r.n, r.pairs, pct(r.effect), pct(r.lo), pct(r.hi), r.a_alloc, r.b_alloc, r.a_free, r.b_free, "", "", ""].join(","));
}
for (const v of ["v1", "v2"]) {
  for (const n of ns) {
    const b = base[`sefer:${n}`], c = ir[v][`sefer:${n}`];
    assert.ok(b > 0 && c > 0, `missing Ir for ${v} n=${n}`);
    const d = (100 * (c - b)) / b;
    assert.ok(Math.abs(b * (1 + d / 100) - c) < 1, "Ir delta arithmetic");
    csv.push(["ir", v, "sefer", n, 1, "", "", "", "", "", "", "", b, c, d.toFixed(2)].join(","));
  }
}
const text = csv.join("\n") + "\n";
if (check) {
  assert.ok(existsSync(csvPath), `${csvPath} missing`);
  assert.equal(readFileSync(csvPath, "utf8"), text, `${csvPath} is stale`);
} else {
  writeFileSync(csvPath, text);
}

console.log("| variant | run | n | pairs | effect % [95 % CI] | alloc median ns A → B | free median ns A → B |");
console.log("|---|---|---:|---:|---|---|---|");
for (const r of runs) {
  console.log(`| ${r.variant} | ${r.label === "null" ? "null (base vs base)" : "base vs variant"} | ${r.n} | ${r.pairs} | ${pct(r.effect)} [${pct(r.lo)}, ${pct(r.hi)}] | ${r.a_alloc} → ${r.b_alloc} | ${r.a_free} → ${r.b_free} |`);
}
console.log("\n| n | base Ir | v1 Ir | v1 Δ % | v2 Ir | v2 Δ % | mimalloc Ir | base / mimalloc | v2 / mimalloc |");
console.log("|---:|---:|---:|---:|---:|---:|---:|---|---|");
for (const n of ns) {
  const b = base[`sefer:${n}`], m = base[`mimalloc:${n}`];
  const v1 = ir.v1[`sefer:${n}`], v2 = ir.v2[`sefer:${n}`];
  const d = (c) => ((100 * (c - b)) / b).toFixed(2);
  console.log(`| ${n} | ${b} | ${v1} | ${d(v1)} | ${v2} | ${d(v2)} | ${m} | ${(b / m).toFixed(2)}× | ${(v2 / m).toFixed(2)}× |`);
}
for (const v of ["v1", "v2"]) {
  const x = verdict(v);
  console.log(`\nVERDICT ${v}: win=${x.win} calibrated=${x.calibrated} -> ${x.go ? "GO (judge A)" : "NO-GO"}`);
}
