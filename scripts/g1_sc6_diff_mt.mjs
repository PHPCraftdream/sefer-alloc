#!/usr/bin/env node
// SC-6 prototype diff, MT: median wall ns per (workload, T) over the base and
// proto runs of docs/perf/g1_scan/sc6_diff/mt.txt, and the delta.
// Usage: node scripts/g1_sc6_diff_mt.mjs docs/perf/g1_scan/sc6_diff/mt.txt [summary.csv]

import fs from "node:fs";
import process from "node:process";

const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};
// Deterministic bootstrap: 95 % CI of median(proto)/median(base).
let seed = 0x9e3779b9;
const rnd = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32);
const resample = (a) => a.map(() => a[Math.floor(rnd() * a.length)]);
const ciRatio = (b, p) => {
  const r = [];
  for (let i = 0; i < 10000; i++) r.push(median(resample(p)) / median(resample(b)));
  r.sort((x, y) => x - y);
  return [r[249], r[9749]];
};
const cells = new Map();
for (const m of fs.readFileSync(process.argv[2], "utf8").matchAll(/RESULT arm=(\w+) mt_ns workload=(\w+) T=(\d+) ns=(\d+)/g)) {
  const k = `${m[2]} ${m[3]}`;
  if (!cells.has(k)) cells.set(k, { w: m[2], t: Number(m[3]), base: [], proto: [] });
  cells.get(k)[m[1]].push(Number(m[4]));
}
const out = [
  "| Workload | T | runs (base/proto) | base ms (median) | proto ms (median) | Δ proto vs base | 95 % CI of Δ | verdict |",
  "|---|---:|---:|---:|---:|---:|---|---|",
];
const csv = ["workload,threads,runs_base,runs_proto,base_ns_median,proto_ns_median,delta_pct,ci_lo_pct,ci_hi_pct"];
for (const c of cells.values()) {
  const [b, p] = [median(c.base), median(c.proto)];
  const d = (p / b - 1) * 100;
  const [lo, hi] = ciRatio(c.base, c.proto).map((x) => (x - 1) * 100);
  const v = lo > 0 ? "slower (CI > 0)" : hi < 0 ? "faster (CI < 0)" : "no difference";
  const f = (x) => `${x >= 0 ? "+" : ""}${x.toFixed(1)} %`;
  out.push(`| ${c.w} | ${c.t} | ${c.base.length}/${c.proto.length} | ${(b / 1e6).toFixed(2)} | ${(p / 1e6).toFixed(2)} | ${f(d)} | ${f(lo)} … ${f(hi)} | ${v} |`);
  csv.push([c.w, c.t, c.base.length, c.proto.length, b, p, d.toFixed(2), lo.toFixed(2), hi.toFixed(2)].join(","));
}
out.push("", "Wall time of the whole run per (workload, T); lower is better. Medians over runs per arm; CI: 10 000 bootstrap resamples, fixed seed.");
console.log(out.join("\n"));
if (process.argv[3]) fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
