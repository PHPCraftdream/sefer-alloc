#!/usr/bin/env node
// DZ-2 (docs/perf/DZ2_BENCH_INTERNALS_HOT_COUNTER.md): SeferAlloc ns/pair per
// cell for `production` vs `production bench-internals`, before and after the
// unlocked Tier-1 counter bump, from the paired-table summary CSVs.
// Usage: node scripts/dz2_verdict.mjs docs/perf/dz2 [summary.csv]

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
const r = {
  bp: load("before/production"),
  bi: load("before/production_internals"),
  bb: load("before/production_bench-internals"),
  ap: load("after/production"),
  ab: load("after/production_bench-internals"),
};
const cells = Object.keys(r.bp).filter((k) => k.endsWith(" sefer")).map((k) => k.slice(0, -" sefer".length));
const s = (m, k) => m[`${k} sefer`];
const out = [
  "| Cell | before: production | before: + internals | before: + bench-internals | Δ before | after: production | after: + bench-internals | Δ after | Δ removed |",
  "|---|---:|---:|---:|---:|---:|---:|---:|---:|",
];
const csv = ["cell,before_production,before_internals,before_bench_internals,after_production,after_bench_internals,delta_before_ns,delta_after_ns"];
let removedSum = 0;
let beforeSum = 0;
for (const k of cells) {
  const db = s(r.bb, k) - s(r.bp, k);
  const da = s(r.ab, k) - s(r.ap, k);
  const removed = db - da;
  if (Math.abs(db - da - removed) > 1e-12) throw new Error("arithmetic");
  csv.push([k.replace(" ", "_"), s(r.bp, k), s(r.bi, k), s(r.bb, k), s(r.ap, k), s(r.ab, k), db.toFixed(3), da.toFixed(3)].join(","));
  removedSum += removed;
  beforeSum += db;
  out.push(
    `| ${k} B | ${s(r.bp, k).toFixed(2)} | ${s(r.bi, k).toFixed(2)} | ${s(r.bb, k).toFixed(2)} | ${db.toFixed(2)} | ${s(r.ap, k).toFixed(2)} | ${s(r.ab, k).toFixed(2)} | ${da.toFixed(2)} | ${(100 * removed / db).toFixed(0)} % |`,
  );
}
const share = removedSum / beforeSum;
out.push("", `bench-internals overhead removed, summed over ${cells.length} cells: ${removedSum.toFixed(2)} of ${beforeSum.toFixed(2)} ns/pair (${(100 * share).toFixed(0)} %)`);
console.log(out.join("\n"));
if (process.argv[3]) fs.writeFileSync(process.argv[3], csv.join("\n") + "\n");
