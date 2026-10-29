#!/usr/bin/env node
// WB-3 C1 checks 1 / 1b (docs/perf/WB3_C1_DEPOT_FAST_LANE_PREREG.md): from the
// per-process sweep lines of scripts/wb2_m1_bulk_sweep.mjs for runs
// <arm>_r<k>_s<size>.jsonl (arm base | c1), per pair k and size: SeferAlloc
// t(16), t(1024) (min over processes, ns/pair), D = t(1024) - t(16), and
// D(c1) - D(base); then the registered rules.
// Usage: node scripts/wb3_c1_sweep_verdict.mjs <dir> <pairs> <rule: per-pair|median>
import fs from "node:fs";
import process from "node:process";
const [dir, pairsArg, rule] = process.argv.slice(2);
const pairs = Number(pairsArg);
const min = (f, arm, n) =>
  Math.min(...fs.readFileSync(`${dir}/${f}`, "utf8").trim().split(/\r?\n/).map((l) => JSON.parse(l)).filter((x) => x.arm === arm && x.n === n).map((x) => 2 * x.ns_per_op));
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) >> 1] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const out = ["| Size | Pair | base t(16) | base t(1024) | c1 t(16) | c1 t(1024) | D(c1) − D(base) | t(16) diff | t(1024) diff |", "|---:|---:|---:|---:|---:|---:|---:|---:|---:|"];
const verdict = [];
for (const size of [16, 64]) {
  const dD = [], d16 = [];
  for (let k = 1; k <= pairs; k++) {
    const t = {};
    for (const arm of ["base", "c1"]) {
      const f = `${arm}_r${k}_s${size}.jsonl`;
      t[arm] = { t16: min(f, "sefer", 16), t1024: min(f, "sefer", 1024) };
    }
    const d = t.c1.t1024 - t.c1.t16 - (t.base.t1024 - t.base.t16);
    dD.push(d);
    d16.push(t.c1.t16 - t.base.t16);
    out.push(`| ${size} B | ${k} | ${t.base.t16.toFixed(2)} | ${t.base.t1024.toFixed(2)} | ${t.c1.t16.toFixed(2)} | ${t.c1.t1024.toFixed(2)} | ${d.toFixed(2)} | ${(t.c1.t16 - t.base.t16).toFixed(2)} | ${(t.c1.t1024 - t.base.t1024).toFixed(2)} |`);
  }
  let pass;
  if (rule === "per-pair") pass = dD.every((d, i) => d <= -0.3 && Math.abs(d16[i]) <= 0.1);
  else pass = median(dD) <= -0.3 && Math.abs(median(d16)) <= 0.1;
  verdict.push(`- ${size} B: ${rule === "median" ? `median D diff ${median(dD).toFixed(2)}, median t(16) diff ${median(d16).toFixed(2)} → ` : ""}${pass ? "PASS" : "FAIL"}`);
}
console.log([...out, "", `Rule: ${rule}`, ...verdict].join("\n"));
