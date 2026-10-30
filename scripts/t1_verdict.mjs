#!/usr/bin/env node
// T-1 verdict from paired bench:table runs (order base, t1, t1, base) per
// docs/perf/T1_TINY_CLASS_INDEX_PREREG.md: per cell and pair d = (t1 sefer/mi)
// / (base sefer/mi) - 1; WIN if d < -1.5 % in both pairs, LOSS if d > +1.5 %
// in both, GO if >= 1 WIN and no LOSS.
// Usage: node scripts/t1_verdict.mjs <dir> [summary.csv]

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
const r = { b1: load("base_r1"), x1: load("t1_r1"), x2: load("t1_r2"), b2: load("base_r2") };
const cells = Object.keys(r.b1).filter((k) => k.endsWith(" sefer")).map((k) => k.slice(0, -6));
const ratio = (run, k) => run[`${k} sefer`] / run[`${k} mimalloc`];
const cls = (x) => (x < 0.97 ? "win" : x > 1.03 ? "LOSS" : "par");
const pct = (x) => `${x >= 0 ? "+" : ""}${(x * 100).toFixed(1)} %`;
const T = 0.015;

const out = [
  "| Cell | sefer ns before (r1 / r2) | sefer ns after (r1 / r2) | mimalloc ns | sefer/mi before | sefer/mi after | d pair 1 / pair 2 | cell verdict | class before → after |",
  "|---|---:|---:|---:|---:|---:|---:|---|---|",
];
const csv = ["cell,base_r1,base_r2,f1c_r1,f1c_r2,mi_mean,ratio_base,ratio_f1c,d1,d2,verdict"];
let wins = 0;
let losses = 0;
for (const k of cells) {
  const d1 = ratio(r.x1, k) / ratio(r.b1, k) - 1;
  const d2 = ratio(r.x2, k) / ratio(r.b2, k) - 1;
  const v = d1 < -T && d2 < -T ? "WIN" : d1 > T && d2 > T ? "LOSS" : "no change";
  if (v === "WIN") wins++;
  if (v === "LOSS") losses++;
  const rb = (ratio(r.b1, k) + ratio(r.b2, k)) / 2;
  const rx = (ratio(r.x1, k) + ratio(r.x2, k)) / 2;
  const mi = ["b1", "x1", "x2", "b2"].reduce((s, a) => s + r[a][`${k} mimalloc`], 0) / 4;
  const s = (a) => r[a][`${k} sefer`].toFixed(2);
  out.push(
    `| ${k} B | ${s("b1")} / ${s("b2")} | ${s("x1")} / ${s("x2")} | ${mi.toFixed(2)} | ${rb.toFixed(3)} | ${rx.toFixed(3)} | ${pct(d1)} / ${pct(d2)} | ${v} | ${cls(rb)} → ${cls(rx)} |`,
  );
  csv.push([k.replace(" ", "_"), s("b1"), s("b2"), s("x1"), s("x2"), mi.toFixed(4), rb.toFixed(4), rx.toFixed(4), d1.toFixed(4), d2.toFixed(4), v].join(","));
}
const verdict = wins >= 1 && losses === 0 ? "GO" : "NO-GO";
out.push(
  "",
  `WIN cells: ${wins} of ${cells.length}; LOSS cells: ${losses} of ${cells.length}. Verdict: **${verdict}**.`,
  "",
  "ns = median over 3 processes of the per-process minimum ns/pair; sefer/mi = mean of the two runs per arm; class: < 0.97 win, > 1.03 LOSS, else par.",
);
console.log(out.join("\n"));
if (process.argv[3]) fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
