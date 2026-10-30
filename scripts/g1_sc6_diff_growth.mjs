#!/usr/bin/env node
// SC-6 prototype diff, growth: per (size, segments) the median ns/alloc of
// base / proto / mimalloc, the base→proto delta and the win/loss against
// mimalloc before and after. Usage:
//   node scripts/g1_sc6_diff_growth.mjs docs/perf/g1_scan/sc6_diff/growth.jsonl [summary.csv]

import fs from "node:fs";
import process from "node:process";

const rows = fs.readFileSync(process.argv[2], "utf8").trim().split(/\r?\n/).map((l) => JSON.parse(l));
const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
const cells = new Map();
for (const r of rows) {
  const per = r.size === 64 ? 65536 : 4096;
  const key = `${r.size} ${r.n / per}`;
  if (!cells.has(key)) cells.set(key, { size: r.size, s: r.n / per, base: [], proto: [], mi: [] });
  cells.get(key)[r.arm].push(r.phase_ns / r.n);
}
const wl = (r) => (r < 0.97 ? "win" : r > 1.03 ? "LOSS" : "par");
const out = [
  "| Size | Segments | base ns/alloc | proto ns/alloc | Δ proto vs base | mimalloc ns/alloc | sefer/mi before | sefer/mi after | before → after |",
  "|---:|---:|---:|---:|---:|---:|---:|---:|---|",
];
const csv = ["size,segments,runs_per_arm,base_ns,proto_ns,delta_pct,mi_ns,ratio_before,ratio_after"];
for (const c of cells.values()) {
  const [b, p, m] = [median(c.base), median(c.proto), median(c.mi)];
  const d = (p / b - 1) * 100;
  const [rb, ra] = [b / m, p / m];
  if (Math.abs(ra * m - p) > 1e-9 || Math.abs(rb * m - b) > 1e-9) throw new Error("arithmetic");
  out.push(
    `| ${c.size} B | ${c.s} | ${b.toFixed(2)} | ${p.toFixed(2)} | ${d >= 0 ? "+" : ""}${d.toFixed(1)} % | ${m.toFixed(2)} | ${rb.toFixed(2)} | ${ra.toFixed(2)} | ${wl(rb)} → ${wl(ra)} |`,
  );
  csv.push([c.size, c.s, c.base.length, b.toFixed(4), p.toFixed(4), d.toFixed(2), m.toFixed(4), rb.toFixed(4), ra.toFixed(4)].join(","));
}
out.push("", "Medians over runs per arm; win/loss: sefer/mi < 0.97 win, > 1.03 LOSS, else par.");
console.log(out.join("\n"));
if (process.argv[3]) fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
