#!/usr/bin/env node
// M1 (segment map instead of the own_cache hash on free): assessment table.
// Reads docs/perf/m1_segment_map/_raw_m1_memo_miss.txt, asserts the expected
// free counts per workload and rate = memo_miss / entry, prints the table and
// optionally writes the summary CSV.
// Usage: node scripts/m1_memo_miss_report.mjs [--csv <summary.csv>]
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = path.resolve(path.dirname(new URL(import.meta.url).pathname.replace(/^\/([A-Za-z]:)/, "$1")), "..");
const raw = fs.readFileSync(path.join(root, "docs", "perf", "m1_segment_map", "_raw_m1_memo_miss.txt"), "utf8");
let csvOut = null;
for (let i = 2; i < process.argv.length; i++) {
  if (process.argv[i] === "--csv") csvOut = process.argv[++i];
  else throw new Error(`unknown argument ${process.argv[i]}`);
}
// Expected frees: bulk = (passes + max(passes/10, 1)) x n; random mode = live + ops (+10 % warm-up ops).
const EXPECT = {
  bulk64_16k: 44 * 16384,
  bulk64_256k: 5 * 262144,
  rand768_512: 768 + 2200000,
  rand256_1024: 256 + 2200000,
  rand262144_64: 262144 + 2200000, // control: 16 MiB live (4 segments), random free order
};
const CONTROL = "rand262144_64";
const rows = [];
for (const line of raw.split(/\r?\n/)) {
  const m = line.match(/^(\S+) entry=(\d+) memo_miss=(\d+) rate=([0-9.]+)$/);
  if (!m) continue;
  const [, tag, e, mi, r] = m;
  const entry = Number(e), miss = Number(mi);
  if (!(tag in EXPECT)) throw new Error(`unexpected workload ${tag}`);
  if (entry !== EXPECT[tag]) throw new Error(`${tag}: entry ${entry} != expected ${EXPECT[tag]}`);
  if (Math.abs(miss / entry - Number(r)) > 5e-6) throw new Error(`${tag}: rate mismatch`);
  rows.push({ tag, entry, miss, rate: miss / entry });
}
for (const t of Object.keys(EXPECT)) if (!rows.find((x) => x.tag === t)) throw new Error(`missing workload ${t}`);
console.log("| Нагрузка | free (вход) | промах memo → own_cache | доля промахов (промахи / free) |");
console.log("|---|---:|---:|---:|");
for (const x of rows) console.log(`| ${x.tag} | ${x.entry} | ${x.miss} | ${x.miss}/${x.entry} = ${(x.rate * 100).toFixed(4)} % |`);
const judgeLike = rows.filter((x) => x.tag !== CONTROL);
const worst = judgeLike.reduce((a, b) => (b.rate > a.rate ? b : a));
// Upper bound of what M1 can save where every live block sits in one or a few
// segments freed in order: the whole own_cache probe on every miss (measured
// below on the control, ~15 Ir; ADR v2 §2.1 estimated 10).
const ctl = rows.find((x) => x.tag === CONTROL);
const sect = raw.slice(raw.indexOf("# memo-miss path excerpt"));
let missIr = 0;
for (const l of sect.split(/\r?\n/)) {
  const m = l.match(/^\s*([0-9a-f]+)\s+([0-9.]+)\s/);
  if (m && parseInt(m[1], 16) >= 0x29760 && parseInt(m[1], 16) <= 0x29798) missIr += Number(m[2]);
}
if (!(missIr > 0)) throw new Error("control: miss-path excerpt missing");
const perMiss = missIr / ctl.rate;
const bound = worst.rate * perMiss;
if (!(bound < 0.01)) throw new Error("M1 bound on judge-like workloads no longer negligible");
console.log(`# judge-like worst ${worst.tag}: ${worst.miss}/${worst.entry}; M1 upper bound = rate x ${perMiss.toFixed(1)} Ir = ${bound.toFixed(5)} Ir/free`);
console.log(`# control ${CONTROL}: ${ctl.miss}/${ctl.entry} = ${(ctl.rate * 100).toFixed(1)} % misses; own_cache hit path ${missIr.toFixed(2)} Ir/free = ${perMiss.toFixed(1)} Ir per miss`);
console.log("VERDICT=NO-GO (judge kernels); DEFERRED with trigger (many live segments, random free order)");
if (csvOut) {
  fs.writeFileSync(csvOut, ["workload,frees,memo_miss,miss_rate,exe_sha256", ...rows.map((x) => `${x.tag},${x.entry},${x.miss},${x.rate},29313dbbadf2b1d76afde05979d99e0415ad9206b7bbf4f5e4c45b13f9be4aef`)].join("\n") + "\n");
  console.log(`# wrote ${path.basename(csvOut)}`);
}
