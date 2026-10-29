#!/usr/bin/env node
// WB-1 / A-1 check 3 and 3b (docs/perf/WB1_A1_CLASS_FOR_LAYOUT_PREREG.md):
// SeferAlloc/mimalloc ratio per cell from the paired-table summaries of the
// base, a1, base, a1 runs, and the registered acceptance rules.
// Usage: node scripts/wb1_a1_verdict.mjs docs/perf/wb1_a1/<check3|check3b>

import fs from "node:fs";
import process from "node:process";

const dir = process.argv[2];
const load = (f) => {
  const m = {};
  for (const l of fs.readFileSync(`${dir}/${f}.csv`, "utf8").trim().split(/\r?\n/).slice(1)) {
    const c = l.split(",");
    m[`${c[0]} ${c[1]} ${c[2]}`] = Number(c[8]);
  }
  return m;
};
const runs = { b1: load("base_r1"), a1: load("a1_r1"), b2: load("base_r2"), a2: load("a1_r2") };
const cells = Object.keys(runs.b1).filter((k) => k.endsWith(" sefer")).map((k) => k.slice(0, -" sefer".length));
const ratio = (r, k) => r[`${k} sefer`] / r[`${k} mimalloc`];
let pass = true;
const out = [
  "| Cell | ratio base→a1, pair 1 | Δ | ratio base→a1, pair 2 | Δ | SeferAlloc ns (b1, a1, b2, a2) | Rule |",
  "|---|---:|---:|---:|---:|---|---|",
];
for (const k of cells) {
  const [b1, a1, b2, a2] = [ratio(runs.b1, k), ratio(runs.a1, k), ratio(runs.b2, k), ratio(runs.a2, k)];
  const d1 = (a1 / b1 - 1) * 100;
  const d2 = (a2 / b2 - 1) * 100;
  const rule = [];
  if (k === "direct 16" || k === "direct 64") {
    if (!(d1 < 0 && d2 < 0)) rule.push("FAIL: bulk ratio not lower in both pairs");
  }
  if (d1 > 3 && d2 > 3) rule.push("FAIL: ratio higher by > 3 % in both pairs");
  if (rule.length) pass = false;
  const ns = [runs.b1, runs.a1, runs.b2, runs.a2].map((r) => r[`${k} sefer`].toFixed(2)).join(", ");
  out.push(`| ${k.replace(" ", " ")} B | ${b1.toFixed(3)} → ${a1.toFixed(3)} | ${d1.toFixed(1)} % | ${b2.toFixed(3)} → ${a2.toFixed(3)} | ${d2.toFixed(1)} % | ${ns} | ${rule.join("; ") || "ok"} |`);
}
out.push("", `Verdict: ${pass ? "PASS" : "FAIL"}`);
console.log(out.join("\n"));
