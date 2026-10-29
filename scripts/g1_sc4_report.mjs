#!/usr/bin/env node
// G1-SC-4 report (docs/perf/g1_scan/SC4_PREREG.md): per (size, S) and arm the
// median / min ns per allocation of the growth phase, r = median_sefer /
// median_mi with a 95 % bootstrap interval (10 000 resamples, fixed seed), and
// the registered verdict lines. Usage:
//   node scripts/g1_sc4_report.mjs <sc4_growth.jsonl> [--csv <path>]
import fs from "node:fs";
import process from "node:process";

const [file] = process.argv.slice(2);
const csvAt = process.argv.indexOf("--csv");
const csvPath = csvAt > 0 ? process.argv[csvAt + 1] : null;
const rows = fs.readFileSync(file, "utf8").trim().split(/\r?\n/).map((l) => JSON.parse(l));
for (const r of rows) {
  if (r.mode !== "growth" || !["sefer", "mimalloc"].includes(r.alloc) || r.counters !== false) throw new Error(`unexpected row ${JSON.stringify(r)}`);
  if (!Number.isInteger(r.n) || !Number.isInteger(r.phase_ns) || r.phase_ns <= 0) throw new Error("bad row");
}
const PER = { 64: 65536, 1024: 4096 };
const SS = [8, 16, 32, 64, 128, 256];
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) >> 1] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
let seed = 0x5eed;
const rand = () => ((seed = (seed * 1103515245 + 12345) >>> 0) / 2 ** 32);
const boot = (a, b) => {
  const rs = [];
  for (let i = 0; i < 10000; i++) {
    const ra = a.map(() => a[Math.floor(rand() * a.length)]);
    const rb = b.map(() => b[Math.floor(rand() * b.length)]);
    rs.push(median(ra) / median(rb));
  }
  rs.sort((x, y) => x - y);
  return [rs[249], rs[9749]];
};
const out = ["| Size | S | n | sefer ns/alloc median (min) | mimalloc ns/alloc median (min) | r = sefer / mi | 95 % interval | runs |", "|---:|---:|---:|---:|---:|---:|---|---:|"];
const csv = ["size,S,n,sefer_median_ns,sefer_min_ns,mi_median_ns,mi_min_ns,r,r_lo,r_hi,runs_per_arm"];
const med = {};
const verdict = [];
for (const size of [64, 1024]) {
  let onset = null;
  for (const S of SS) {
    const n = S * PER[size];
    const pick = (a) => rows.filter((r) => r.size === size && r.n === n && r.alloc === a).map((r) => r.phase_ns / n);
    const s = pick("sefer"), m = pick("mimalloc");
    if (s.length !== 20 || m.length !== 20) throw new Error(`size ${size} S ${S}: ${s.length}/${m.length} runs, want 20/20`);
    const r = median(s) / median(m);
    const [lo, hi] = boot(s, m);
    med[`${size}/${S}`] = { s: median(s), m: median(m) };
    if (onset === null && lo > 1.05) onset = S;
    out.push(`| ${size} B | ${S} | ${n} | ${median(s).toFixed(2)} (${Math.min(...s).toFixed(2)}) | ${median(m).toFixed(2)} (${Math.min(...m).toFixed(2)}) | ${r.toFixed(3)} | [${lo.toFixed(3)}, ${hi.toFixed(3)}] | ${s.length} + ${m.length} |`);
    csv.push([size, S, n, median(s).toFixed(4), Math.min(...s).toFixed(4), median(m).toFixed(4), Math.min(...m).toFixed(4), r.toFixed(4), lo.toFixed(4), hi.toFixed(4), s.length].join(","));
  }
  const gs = med[`${size}/256`].s / med[`${size}/64`].s;
  const gm = med[`${size}/256`].m / med[`${size}/64`].m;
  verdict.push(`- ${size} B: SeferAlloc per-allocation growth S 64→256 ×${gs.toFixed(2)}, mimalloc ×${gm.toFixed(2)} → quadratic ${gs >= 1.5 && gm <= 1.2 ? "VISIBLE" : "not visible"} in time; loss against mimalloc ${onset === null ? "at no S" : `from S = ${onset}`}`);
}
console.log([...out, "", ...verdict].join("\n"));
if (csvPath) fs.writeFileSync(csvPath, csv.join("\n") + "\n");
