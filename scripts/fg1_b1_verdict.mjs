#!/usr/bin/env node
// FG-1 / B1 (docs/perf/FG1_B1_COLD_BIND_BYREF_PREREG.md): SeferAlloc/mimalloc
// ratio per cell for base and b1 in two pairs, and the registered rule.
// Usage: node scripts/fg1_b1_verdict.mjs docs/perf/fg1/b1 [summary.csv]

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
const runs = { b1: load("base_r1"), a1: load("b1_r1"), b2: load("base_r2"), a2: load("b1_r2") };
const cells = Object.keys(runs.b1).filter((k) => k.endsWith(" sefer")).map((k) => k.slice(0, -" sefer".length));
const ratio = (r, k) => r[`${k} sefer`] / r[`${k} mimalloc`];
let pass = true;
const out = [
  "| Cell | ratio base→b1, pair 1 | Δ | ratio base→b1, pair 2 | Δ | SeferAlloc ns (base1, b1, b1, base2) | Rule |",
  "|---|---:|---:|---:|---:|---|---|",
];
const csv = ["cell,ratio_base1,ratio_b1_1,delta1_pct,ratio_base2,ratio_b1_2,delta2_pct,rule"];
for (const k of cells) {
  const [rb1, ra1, rb2, ra2] = [ratio(runs.b1, k), ratio(runs.a1, k), ratio(runs.b2, k), ratio(runs.a2, k)];
  const d1 = (ra1 / rb1 - 1) * 100;
  const d2 = (ra2 / rb2 - 1) * 100;
  const rule = [];
  if (k === "direct 16" && !(d1 < 0 && d2 < 0)) rule.push("FAIL: bulk 16 B ratio not lower in both pairs");
  if (d1 > 3 && d2 > 3) rule.push("FAIL: ratio higher by > 3 % in both pairs");
  if (rule.length) pass = false;
  const ns = [runs.b1, runs.a1, runs.a2, runs.b2].map((r) => r[`${k} sefer`].toFixed(2)).join(", ");
  out.push(`| ${k} B | ${rb1.toFixed(3)} → ${ra1.toFixed(3)} | ${d1.toFixed(1)} % | ${rb2.toFixed(3)} → ${ra2.toFixed(3)} | ${d2.toFixed(1)} % | ${ns} | ${rule.join("; ") || "ok"} |`);
  csv.push([k.replace(" ", "_"), rb1.toFixed(4), ra1.toFixed(4), d1.toFixed(2), rb2.toFixed(4), ra2.toFixed(4), d2.toFixed(2), rule.length ? "fail" : "ok"].join(","));
}
out.push("", `Verdict: ${pass ? "PASS" : "FAIL"}`);
console.log(out.join("\n"));
if (process.argv[3]) fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
