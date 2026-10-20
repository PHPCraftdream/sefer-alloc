#!/usr/bin/env node
// B4 (ClassLeaves / state-byte) summary table printer.
// Reads docs/perf/B4_CLASS_LEAVES_FREE_summary.csv and prints a markdown
// table: step -> ao alloc/free (rt/const) -> batch -> larson -> mstress ->
// delta -> verdict.
//
// Usage: node scripts/b4_leaves_table.mjs docs/perf/B4_CLASS_LEAVES_FREE_summary.csv
// No cargo, no deps (node >= 18).

import { readFileSync } from "node:fs";

const file = process.argv[2] ?? "docs/perf/B4_CLASS_LEAVES_FREE_summary.csv";

function parseCsv(text) {
  const lines = text.split(/\r?\n/).filter((l) => l.trim() !== "");
  const header = lines[0].split(",");
  return lines.slice(1).map((line) => {
    // simple CSV: no quoted commas in this file
    const cells = line.split(",");
    const row = {};
    header.forEach((h, i) => (row[h] = (cells[i] ?? "").trim()));
    return row;
  });
}

const rows = parseCsv(readFileSync(file, "utf8"));

// group by step, keep insertion order
const steps = [];
const byStep = new Map();
for (const r of rows) {
  if (!byStep.has(r.step)) {
    byStep.set(r.step, []);
    steps.push(r.step);
  }
  byStep.get(r.step).push(r);
}
const pick = (stepRows, mode) => stepRows.find((r) => r.mode === mode);
const num = (v) => (v === "" ? null : Number(v));
const fmt = (v) => (v === null || v === undefined ? "—" : String(v));
const d = (a, b) => (a === null || b === null ? null : a - b);
const sign = (v) => (v === null ? "—" : v > 0 ? `+${v}` : String(v));

// deltas vs base (rt mode)
const baseRt = pick(byStep.get("base") ?? [], "rt");

const lines = [];
lines.push(
  "| шаг | ao alloc/free (rt) | ao alloc/free (const) | batch alloc/free (rt) | batch alloc/free (const) | larson | mstress | Δ free rt (vs base) | вердикт |",
);
lines.push("|---|---|---|---|---|---|---|---|---|");
for (const step of steps) {
  const rs = byStep.get(step);
  const rt = pick(rs, "rt");
  const cst = pick(rs, "const");
  const batchRtA = num(rt.batch_alloc_ir);
  const batchRtF = num(rt.batch_free_ir);
  const batchCstA = num(cst.batch_alloc_ir);
  const batchCstF = num(cst.batch_free_ir);
  const dfree = d(num(rt.free_ir), num(baseRt?.free_ir));
  lines.push(
    `| ${step} | ${fmt(rt.alloc_ir)}/${fmt(rt.free_ir)} | ${fmt(cst.alloc_ir)}/${fmt(cst.free_ir)} | ${fmt(batchRtA)}/${fmt(batchRtF)} | ${fmt(batchCstA)}/${fmt(batchCstF)} | ${fmt(rt.larson_ir)} | ${fmt(rt.mstress_ir)} | ${sign(dfree)} | ${(rt.verdict || "").trim()} |`,
  );
}

console.log(lines.join("\n"));
