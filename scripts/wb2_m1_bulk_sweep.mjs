#!/usr/bin/env node
// WB-2 M1 (measurement only): warm bulk n-sweep on ONE b4_pmu_probe binary.
// For n in {16,32,64,256,1024} (passes = 2^20 / n, so every run does ~2^20
// alloc+free pairs), run `b4_pmu_probe bulk <sefer|mi> <n> <size> <passes>`
// in fresh processes, alternating the arm order per repetition, and keep
// every line as raw JSONL. n = 16 fits one magazine (no park/reload); the
// growth from n = 16 to n = 1024 is the cost of the depot traffic.
// Usage: node scripts/wb2_m1_bulk_sweep.mjs <probe.exe> <out.jsonl> [reps=21] [size=16]
import { execFileSync } from "node:child_process";
import fs from "node:fs";
const [exe, out, repsArg, sizeArg] = process.argv.slice(2);
const reps = Number(repsArg ?? 21);
const size = Number(sizeArg ?? 16);
const NS = [16, 32, 64, 256, 1024];
const lines = [];
for (let r = 0; r < reps; r++) {
  for (const n of NS) {
    const passes = Math.floor((1 << 20) / n);
    const arms = r % 2 ? ["mi", "sefer"] : ["sefer", "mi"];
    for (const arm of arms) {
      const o = execFileSync(exe, ["bulk", arm, String(n), String(size), String(passes)], { encoding: "utf8" }).trim();
      const f = o.split(/\s+/);
      if (f[0] !== "bulk" || f[1] !== arm || Number(f[2]) !== n) throw new Error(`unexpected output: ${o}`);
      lines.push(JSON.stringify({ rep: r, arm, n, size, passes, ns_per_op: Number(f[5]) }));
    }
  }
}
fs.writeFileSync(out, lines.join("\n") + "\n");
const rows = lines.map((l) => JSON.parse(l));
const stat = (arm, n) => {
  const xs = rows.filter((x) => x.arm === arm && x.n === n).map((x) => x.ns_per_op).sort((a, b) => a - b);
  return { min: xs[0], med: xs[(xs.length - 1) >> 1] };
};
console.log(`| n | passes | sefer ns/pair min (median) | mimalloc ns/pair min (median) | sefer − mi (min) |`);
console.log(`|---:|---:|---:|---:|---:|`);
for (const n of NS) {
  const s = stat("sefer", n), m = stat("mi", n);
  console.log(`| ${n} | ${Math.floor((1 << 20) / n)} | ${(2 * s.min).toFixed(2)} (${(2 * s.med).toFixed(2)}) | ${(2 * m.min).toFixed(2)} (${(2 * m.med).toFixed(2)}) | ${(2 * (s.min - m.min)).toFixed(2)} |`);
}
