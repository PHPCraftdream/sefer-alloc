#!/usr/bin/env node
// AS-1 MT control table from malloc_macro raw output (ABBA runs of base and
// as1 builds, each printing SeferAlloc / mimalloc / System M ops/s per
// workload and T). Cell = median over runs of the SeferAlloc column; the
// mimalloc column of the same runs is the in-process control.
// Usage: node scripts/as1_mt_table.mjs <raw.txt> [summary.csv]

import fs from "node:fs";
import process from "node:process";

const lines = fs.readFileSync(process.argv[2], "utf8").split(/\r?\n/);
const rows = [];
let arm = null;
let workload = null;
for (const l of lines) {
  const a = l.match(/^== (\S+) (\d+)/);
  if (a) {
    arm = a[1] === "g1-base" ? "base" : "as1";
    continue;
  }
  const w = l.match(/^--- workload: (\w+)/);
  if (w) {
    workload = w[1];
    continue;
  }
  const m = l.match(/^\s*(\d+)\s+([\d.]+) M\s+([\d.]+) M\s+([\d.]+) M/);
  if (m && arm && workload) rows.push({ arm, workload, t: +m[1], sefer: +m[2], mi: +m[3] });
}
const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  const k = s.length >> 1;
  return s.length % 2 ? s[k] : (s[k - 1] + s[k]) / 2;
};
const out = [
  "| Workload | T | runs | sefer before (M ops/s) | sefer after | Δ | sefer/mi before | sefer/mi after |",
  "|---|---:|---:|---:|---:|---:|---:|---:|",
];
const csv = ["workload,threads,runs,sefer_base,sefer_as1,delta,ratio_base,ratio_as1"];
for (const wl of [...new Set(rows.map((r) => r.workload))]) {
  for (const t of [...new Set(rows.map((r) => r.t))]) {
    const b = rows.filter((r) => r.workload === wl && r.t === t && r.arm === "base");
    const x = rows.filter((r) => r.workload === wl && r.t === t && r.arm === "as1");
    if (!b.length || !x.length) continue;
    const sb = median(b.map((r) => r.sefer));
    const sx = median(x.map((r) => r.sefer));
    const rb = median(b.map((r) => r.sefer / r.mi));
    const rx = median(x.map((r) => r.sefer / r.mi));
    const d = sx / sb - 1;
    out.push(`| ${wl} | ${t} | ${b.length}/${x.length} | ${sb.toFixed(2)} | ${sx.toFixed(2)} | ${d >= 0 ? "+" : ""}${(d * 100).toFixed(1)} % | ${rb.toFixed(3)} | ${rx.toFixed(3)} |`);
    csv.push([wl, t, `${b.length}/${x.length}`, sb, sx, d.toFixed(4), rb.toFixed(4), rx.toFixed(4)].join(","));
  }
}
out.push("", "Throughput: higher is better; Δ = after/before − 1 of the run medians; sefer/mi > 1 means faster than mimalloc in the same run.");
console.log(out.join("\n"));
if (process.argv[3]) fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
