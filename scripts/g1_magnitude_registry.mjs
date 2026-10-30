#!/usr/bin/env node
// Magnitude registry of the paired bench:table A/B runs of rejected
// fast-path candidates: one row per run set, derived by
// scripts/g1_magnitude_verdict.mjs (sefer-time geometric mean, agreed wins /
// losses, largest agreed loss, magnitude-rule verdict).
// Usage: node scripts/g1_magnitude_registry.mjs

import { execFileSync } from "node:child_process";
import process from "node:process";

const SETS = [
  ["FG-1/B1 (screen)", "docs/perf/fg1/b1", "base", "b1"],
  ["FG-1/B1-R set 1", "docs/perf/fg1/b1r/set1", "base", "b1"],
  ["FG-1/B1-R set 2", "docs/perf/fg1/b1r/set2", "base", "b1"],
  ["F-1", "docs/perf/f1", "base", "f1c"],
  ["T-1", "docs/perf/t1", "base", "t1"],
  ["WB-1 A-1 (check3)", "docs/perf/wb1_a1/check3", "base", "a1"],
  ["WB-1 A-1 (check3b)", "docs/perf/wb1_a1/check3b", "base", "a1"],
  ["WB-3 C1 (check2)", "docs/perf/wb3_c1/check2", "base", "a1"],
  ["L-1a set 1", "docs/perf/l1", "s1base", "s1l1"],
  ["L-1a set 2", "docs/perf/l1", "s2base", "s2l1"],
  ["L-1a set 3", "docs/perf/l1", "s3base", "s3l1"],
];
const out = [
  "| # | Candidate (run set) | sefer-time geomean | agreed wins / losses | largest agreed loss | ratio-to-mimalloc geomean | magnitude rule |",
  "|---:|---|---:|---:|---:|---:|---|",
];
SETS.forEach(([label, dir, b, x], i) => {
  const text = execFileSync(process.execPath, ["scripts/g1_magnitude_verdict.mjs", dir, b, x, label], { encoding: "utf8" });
  const t = text.match(/Sefer time .*?geometric mean ([^;]+); agreed wins ([\d.]+) pp, agreed losses ([\d.]+) pp; largest agreed loss ([^.]+\.\d %)/);
  const q = text.match(/Ratio to mimalloc: geometric mean over \d+ cells: ([^;]+);/);
  const v = text.match(/=> \*\*(GO|NO-GO)\*\*/);
  if (!t || !q || !v) throw new Error(`${label}: unparsed output`);
  out.push(`| ${i + 1} | ${label} | ${t[1]} | ${t[2]} / ${t[3]} pp | ${t[4]} | ${q[1]} | ${v[1]} |`);
});
console.log(out.join("\n"));
