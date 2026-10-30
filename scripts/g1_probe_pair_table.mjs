#!/usr/bin/env node
// A-2 (plain LIVE store on a magazine hit): before/after table from the
// paired ABBA probe runs (`b4_pmu_probe inter`, base vs a2 per scenario).
// Per run the probe prints sefer and mimalloc ticks/op from one process;
// sefer/mi of that run is the cell's statistic (mimalloc is the in-process
// control). Cell = median over runs; Δ = median of per-pair ratios a2/base.
// Usage: node scripts/g1_a2_verdict.mjs <run.csv>... [--csv out.csv]

import fs from "node:fs";
import process from "node:process";

const args = process.argv.slice(2);
const ci = args.indexOf("--csv");
const csvOut = ci >= 0 ? args[ci + 1] : null;
const files = args.filter((a, i) => ci < 0 || (i !== ci && i !== ci + 1));

const rows = [];
for (const f of files) {
  for (const l of fs.readFileSync(f, "utf8").trim().split(/\r?\n/).slice(1)) {
    const [rep, arm, scenario, sefer, mi] = l.split(",");
    rows.push({ run: `${f}#${rep}`, arm, scenario, sefer: +sefer, mi: +mi });
  }
}
const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const wl = (r) => (r < 0.97 ? "win" : r > 1.03 ? "LOSS" : "par");
const pct = (x) => `${x >= 0 ? "+" : ""}${(x * 100).toFixed(1)} %`;

const scenarios = [...new Set(rows.map((r) => r.scenario))];
const out = [
  "| Scenario | sefer before | sefer after | Δ sefer (median of pairs) | pairs a2 faster | sefer/mi before | sefer/mi after | class before → after |",
  "|---|---:|---:|---:|---:|---:|---:|---|",
];
const csv = ["scenario,n_base,n_a2,sefer_base,sefer_a2,delta_median,pairs_faster,pairs,ratio_base,ratio_a2"];
for (const s of scenarios) {
  const base = rows.filter((r) => r.scenario === s && r.arm === (process.env.VBASE ?? "a2base"));
  const a2 = rows.filter((r) => r.scenario === s && r.arm === (process.env.VX ?? "a2"));
  // Pair the k-th base run with the k-th a2 run of the same rep (ABBA gives two each).
  const pairs = [];
  for (const run of new Set(base.map((r) => r.run))) {
    const b = base.filter((r) => r.run === run);
    const a = a2.filter((r) => r.run === run);
    for (let k = 0; k < Math.min(a.length, b.length); k++) pairs.push(a[k].sefer / b[k].sefer);
  }
  const d = median(pairs) - 1;
  const faster = pairs.filter((p) => p < 1).length;
  const sb = median(base.map((r) => r.sefer));
  const sa = median(a2.map((r) => r.sefer));
  const rb = median(base.map((r) => r.sefer / r.mi));
  const ra = median(a2.map((r) => r.sefer / r.mi));
  out.push(
    `| ${s} | ${sb.toFixed(2)} | ${sa.toFixed(2)} | ${pct(d)} | ${faster}/${pairs.length} | ${rb.toFixed(3)} | ${ra.toFixed(3)} | ${wl(rb)} → ${wl(ra)} |`,
  );
  csv.push([s, base.length, a2.length, sb.toFixed(3), sa.toFixed(3), d.toFixed(4), faster, pairs.length, rb.toFixed(4), ra.toFixed(4)].join(","));
}
out.push(
  "",
  "sefer = median ticks/op (TSC) over runs; Δ = median over ABBA pairs of a2/base − 1 (negative = A-2 faster); sefer/mi = median of the per-run in-process ratio; class: < 0.97 win, > 1.03 LOSS, else par.",
);
console.log(out.join("\n"));
if (csvOut) fs.writeFileSync(csvOut, csv.join("\n") + "\n");
