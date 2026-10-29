#!/usr/bin/env node
// Paired short-sample companion of scripts/bench-table.mjs. Builds and runs
// examples/bench_table_paired.rs (every round measures every arm once per
// (group, size) in a small sample; the three samples of a triplet run back to
// back so they see one noise state; rotated order), keeps its per-sample JSON
// lines as the raw artifact plus a manifest (commit, features, binary sha256,
// rustc), and derives the tables from them: statistic = MINIMUM ns/pair over
// rounds per arm (the least disturbed sample), with the median shown.
//
// Usage (from repo root):
//   node scripts/bench-table-paired.mjs [--rounds 200] [--calls 2] [--batch 1]
//        [--features production] [--raw <path>] [--from <jsonl>]
// --from skips the run and derives the tables from an existing raw file.

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import process from "node:process";

const arg = (name, dflt) => {
  const i = process.argv.indexOf(name);
  return i > 0 ? process.argv[i + 1] : dflt;
};
const rounds = Number(arg("--rounds", 200));
const calls = Number(arg("--calls", 2));
const batch = Number(arg("--batch", 1));
const features = arg("--features", "production");
const from = arg("--from", null);
const rawPath = arg("--raw", "target/bench-table-paired.jsonl");

let text;
if (from) {
  text = fs.readFileSync(from, "utf8");
} else {
  const env = { ...process.env, RUSTC_WRAPPER: "" };
  execFileSync("cargo", ["build", "--release", "-q", "--example", "bench_table_paired", "--features", features], { stdio: "inherit", env });
  const dir = process.env.CARGO_TARGET_DIR ?? "target";
  const exe = `${dir}/release/examples/bench_table_paired${process.platform === "win32" ? ".exe" : ""}`;
  const manifest = {
    commit: execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(),
    dirty: execFileSync("git", ["status", "--porcelain", "--untracked-files=no"], { encoding: "utf8" }).trim() !== "",
    features,
    rounds,
    calls,
    batch,
    exe_sha256: crypto.createHash("sha256").update(fs.readFileSync(exe)).digest("hex"),
    rustc: execFileSync("rustc", ["-vV"], { encoding: "utf8" }).trim().split(/\r?\n/),
  };
  text = execFileSync(exe, [String(rounds), String(calls), String(batch)], { encoding: "utf8", maxBuffer: 1 << 26 });
  fs.writeFileSync(rawPath, text);
  fs.writeFileSync(`${rawPath}.manifest.json`, JSON.stringify(manifest, null, 2) + "\n");
  console.log(`# raw samples: ${rawPath} (manifest ${rawPath}.manifest.json)`);
}

const rows = text.split(/\r?\n/).filter((l) => l.trim()).map((l) => JSON.parse(l));
const GROUPS = { direct: "Warm repeated bulk burst (alloc 1024 then free 1024)", churn: "Churn, non-writing (working set 256)", churn_write: "Churn + write (16 B written after each alloc)" };
const SIZES = [16, 64, 256, 1024];
const ARMS = ["sefer", "mimalloc", "system"];
const n = new Set(rows.map((r) => r.round)).size;
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const pick = (g, size, arm) => rows.filter((r) => r.group === g && r.size === size && r.arm === arm);
console.log(`# bench:table (paired): ${n} rounds; statistic = min ns/pair over rounds (median in parentheses)`);
for (const [g, title] of Object.entries(GROUPS)) {
  const gr = rows.filter((r) => r.group === g);
  const pairs = new Set(gr.map((r) => r.pairs));
  const batches = new Set(gr.map((r) => r.batch));
  if (pairs.size !== 1 || batches.size !== 1) throw new Error(`${g}: mixed pairs/batch per sample`);
  console.log(`\n### ${title}\n\n${[...pairs][0]} pairs per sample, ${[...batches][0]} working set(s) per sample\n`);
  console.log("| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc (min) |");
  console.log("|---|---:|---:|---:|---:|");
  for (const size of SIZES) {
    const cell = {};
    for (const a of ARMS) {
      const xs = pick(g, size, a).map((r) => r.ns_per_pair);
      if (xs.length !== n) throw new Error(`${g} ${size} ${a}: ${xs.length} samples, want ${n}`);
      cell[a] = { min: Math.min(...xs), med: median(xs) };
    }
    const r = cell.sefer.min / cell.mimalloc.min;
    const ratio = r <= 1 ? `**${(1 / r).toFixed(2)}× faster**` : `${r.toFixed(2)}× slower`;
    const f = (c) => `${c.min.toFixed(1)} (${c.med.toFixed(1)})`;
    console.log(`| ${size}B | ${f(cell.sefer)} | ${f(cell.mimalloc)} | ${f(cell.system)} | ${ratio} |`);
  }
}
