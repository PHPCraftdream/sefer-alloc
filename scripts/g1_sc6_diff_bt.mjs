#!/usr/bin/env node
// SC-6 prototype diff, single-thread control: paired bench:table runs in the
// order base, proto, proto, base. Per cell: SeferAlloc ns/pair before/after
// (mean of the two runs per arm), mimalloc, and win/loss before → after.
// Usage: node scripts/g1_sc6_diff_bt.mjs docs/perf/g1_scan/sc6_diff/bt [summary.csv]

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
const runs = { b1: load("base_r1"), p1: load("proto_r1"), p2: load("proto_r2"), b2: load("base_r2") };
const cells = Object.keys(runs.b1).filter((k) => k.endsWith(" sefer")).map((k) => k.slice(0, -" sefer".length));
const wl = (r) => (r < 0.97 ? "win" : r > 1.03 ? "LOSS" : "par");
const mean = (a, b) => (a + b) / 2;
const out = [
  "| Cell | before ns (r1 / r2) | after ns (r1 / r2) | Δ after vs before | Δ pair 1 / pair 2 | mimalloc ns | sefer/mi before | sefer/mi after | before → after |",
  "|---|---:|---:|---:|---:|---:|---:|---:|---|",
];
const csv = ["cell,before_r1,before_r2,after_r1,after_r2,delta_pct,delta1_pct,delta2_pct,mi_ns,ratio_before,ratio_after"];
for (const k of cells) {
  const s = (r) => r[`${k} sefer`];
  const mi = mean(mean(runs.b1[`${k} mimalloc`], runs.b2[`${k} mimalloc`]), mean(runs.p1[`${k} mimalloc`], runs.p2[`${k} mimalloc`]));
  const [b, p] = [mean(s(runs.b1), s(runs.b2)), mean(s(runs.p1), s(runs.p2))];
  const d = (p / b - 1) * 100;
  const d1 = (s(runs.p1) / s(runs.b1) - 1) * 100;
  const d2 = (s(runs.p2) / s(runs.b2) - 1) * 100;
  const [rb, ra] = [b / mi, p / mi];
  const f = (x) => `${x >= 0 ? "+" : ""}${x.toFixed(1)} %`;
  out.push(
    `| ${k} B | ${s(runs.b1).toFixed(2)} / ${s(runs.b2).toFixed(2)} | ${s(runs.p1).toFixed(2)} / ${s(runs.p2).toFixed(2)} | ${f(d)} | ${f(d1)} / ${f(d2)} | ${mi.toFixed(2)} | ${rb.toFixed(3)} | ${ra.toFixed(3)} | ${wl(rb)} → ${wl(ra)} |`,
  );
  csv.push([k.replace(" ", "_"), s(runs.b1), s(runs.b2), s(runs.p1), s(runs.p2), d.toFixed(2), d1.toFixed(2), d2.toFixed(2), mi.toFixed(4), rb.toFixed(4), ra.toFixed(4)].join(","));
}
out.push("", "Cell = median over 3 processes of the per-process minimum ns/pair; mimalloc = mean of its four runs; win/loss: < 0.97 win, > 1.03 LOSS, else par.");
console.log(out.join("\n"));
if (process.argv[3]) fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
