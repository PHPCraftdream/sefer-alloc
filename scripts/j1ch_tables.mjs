#!/usr/bin/env node
// J1-CH attribution tables (docs/perf/J1_CROSS_CLASS_ATTRIBUTION.md):
// (1) per regime, sefer and mimalloc ticks/op from the base arm of a paired
//     `b4_pmu_probe inter` CSV; (2) callgrind inclusive Ir per op of the four
//     shims from docs/perf/j1ch/cg_incl.<scenario>.txt.
// Usage: node scripts/j1ch_tables.mjs <probe.csv> <base-label> <cg-dir>

import fs from "node:fs";
import process from "node:process";

const [probe, base, cgDir] = process.argv.slice(2);
const median = (a) => {
  const s = [...a].sort((x, y) => x - y);
  const m = s.length >> 1;
  return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2;
};

const rows = fs
  .readFileSync(probe, "utf8")
  .trim()
  .split(/\r?\n/)
  .slice(1)
  .map((l) => l.split(","))
  .filter((c) => c[1] === base)
  .map((c) => ({ scenario: c[2], sefer: +c[3], mi: +c[4] }));
const out = [
  "| Scenario | runs | sefer ticks/op | mimalloc ticks/op | sefer/mi |",
  "|---|---:|---:|---:|---:|",
];
const csv = ["scenario,runs,sefer_ticks,mi_ticks,sefer_over_mi"];
for (const s of [...new Set(rows.map((r) => r.scenario))]) {
  const r = rows.filter((x) => x.scenario === s);
  csv.push([s, r.length, median(r.map((x) => x.sefer)), median(r.map((x) => x.mi)), median(r.map((x) => x.sefer / x.mi)).toFixed(4)].join(","));
  out.push(
    `| ${s} | ${r.length} | ${median(r.map((x) => x.sefer)).toFixed(2)} | ${median(r.map((x) => x.mi)).toFixed(2)} | ${median(r.map((x) => x.sefer / x.mi)).toFixed(3)} |`,
  );
}
out.push("", "Median over runs of the base arm; sefer/mi = median of the per-run in-process ratio.", "");

// Ops per arm of the callgrind runs: rounds * 2 * portion + the 100 000-op warm-up.
const CG = { x1664: [2000, 128], c16: [2000, 128], pair16: [2000, 512] };
const SHIMS = { sefer_dealloc: "free", sefer_alloc: "alloc", mi_dealloc: "free", mi_alloc: "alloc" };
out.push(
  "| Scenario | ops per arm | sefer free | sefer alloc | sefer pair | mimalloc free | mimalloc alloc | mimalloc pair |",
  "|---|---:|---:|---:|---:|---:|---:|---:|",
);
for (const [s, [rounds, portion]] of Object.entries(CG)) {
  const ops = rounds * 2 * portion + 100000;
  const ir = {};
  for (const l of fs.readFileSync(`${cgDir}/cg_incl.${s}.txt`, "utf8").split(/\r?\n/)) {
    const m = l.match(/^\s*([\d,]+) .*b4_pmu_probe::(sefer_dealloc|sefer_alloc|mi_dealloc|mi_alloc) /);
    if (m && !(m[2] in ir)) ir[m[2]] = Number(m[1].replace(/,/g, "")) / ops;
  }
  for (const k of Object.keys(SHIMS)) if (!(k in ir)) throw new Error(`${s}: no ${k}`);
  const f = (x) => x.toFixed(1);
  out.push(
    `| ${s} | ${ops} | ${f(ir.sefer_dealloc)} | ${f(ir.sefer_alloc)} | ${f(ir.sefer_dealloc + ir.sefer_alloc)} | ${f(ir.mi_dealloc)} | ${f(ir.mi_alloc)} | ${f(ir.mi_dealloc + ir.mi_alloc)} |`,
  );
}
out.push("", "Inclusive Ir of the `#[inline(never)]` shims divided by the arm's ops (callgrind, one run per scenario).");
console.log(out.join("\n"));
if (process.env.J1CH_CSV) fs.writeFileSync(process.env.J1CH_CSV, csv.join("\n") + "\n");

// (3) FG-1/A single-pair bench:table (base vs a1), when the two CSVs are given.
const [, , , , , fgBase, fgA1] = process.argv;
if (fgBase && fgA1) {
  const load = (f) => {
    const m = {};
    for (const l of fs.readFileSync(f, "utf8").trim().split(/\r?\n/).slice(1)) {
      const c = l.split(",");
      m[`${c[0]} ${c[1]} ${c[2]}`] = Number(c[8]);
    }
    return m;
  };
  const b = load(fgBase);
  const a = load(fgA1);
  const t = ["", "| Cell | sefer ns base | sefer ns FG-1/A | Δ sefer | sefer/mi base | sefer/mi FG-1/A |", "|---|---:|---:|---:|---:|---:|"];
  for (const k of Object.keys(b).filter((x) => x.endsWith(" sefer")).map((x) => x.slice(0, -6))) {
    const d = a[`${k} sefer`] / b[`${k} sefer`] - 1;
    t.push(
      `| ${k} B | ${b[`${k} sefer`].toFixed(2)} | ${a[`${k} sefer`].toFixed(2)} | ${d >= 0 ? "+" : ""}${(d * 100).toFixed(1)} % | ${(b[`${k} sefer`] / b[`${k} mimalloc`]).toFixed(3)} | ${(a[`${k} sefer`] / a[`${k} mimalloc`]).toFixed(3)} |`,
    );
  }
  t.push("", "One run per arm (screening, not a paired verdict).");
  console.log(t.join("\n"));
}
