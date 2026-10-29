#!/usr/bin/env node
// Paired short-sample companion of scripts/bench-table.mjs. Builds and runs
// examples/bench_table_paired.rs (every round measures every arm once per
// (group, size) in a small sample; the three samples of a triplet run back to
// back so they see one noise state; rotated order) in `--processes` fresh
// processes, keeps each process's per-sample JSON lines as the raw artifact
// plus a manifest (commit, features, binary sha256, rustc), and derives the
// tables from them. Statistic: per process, the MINIMUM ns/pair over rounds
// (the least disturbed sample); a cell is the MEDIAN of those per-process
// minimums, with their range in parentheses (process-level state such as
// address layout is not noise a minimum over rounds removes).
//
// Usage (from repo root):
//   node scripts/bench-table-paired.mjs [--rounds 200] [--calls 2] [--batch 1]
//        [--processes 3] [--features production] [--raw <prefix>] [--csv <path>]
//   node scripts/bench-table-paired.mjs --from a.jsonl[.gz],b.jsonl[.gz] [--csv <path>]
// --from skips the run and derives the tables from existing raw files.

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import process from "node:process";
import zlib from "node:zlib";

const arg = (name, dflt) => {
  const i = process.argv.indexOf(name);
  return i > 0 ? process.argv[i + 1] : dflt;
};
const rounds = Number(arg("--rounds", 200));
const calls = Number(arg("--calls", 2));
const batch = Number(arg("--batch", 1));
const processes = Number(arg("--processes", 3));
const features = arg("--features", "production");
const from = arg("--from", null);
const rawPrefix = arg("--raw", "target/bench-table-paired");
const csvPath = arg("--csv", null);

const read = (f) => (f.endsWith(".gz") ? zlib.gunzipSync(fs.readFileSync(f)) : fs.readFileSync(f)).toString("utf8");
const parse = (t) => t.split(/\r?\n/).filter((l) => l.trim()).map((l) => JSON.parse(l));

let runs;
if (from) {
  runs = from.split(",").map((f) => parse(read(f)));
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
    processes,
    exe_sha256: crypto.createHash("sha256").update(fs.readFileSync(exe)).digest("hex"),
    rustc: execFileSync("rustc", ["-vV"], { encoding: "utf8" }).trim().split(/\r?\n/),
  };
  fs.writeFileSync(`${rawPrefix}.manifest.json`, JSON.stringify(manifest, null, 2) + "\n");
  runs = [];
  for (let p = 1; p <= processes; p++) {
    const text = execFileSync(exe, [String(rounds), String(calls), String(batch)], { encoding: "utf8", maxBuffer: 1 << 26 });
    fs.writeFileSync(`${rawPrefix}.p${p}.jsonl`, text);
    runs.push(parse(text));
  }
  console.log(`# raw samples: ${rawPrefix}.p{1..${processes}}.jsonl, manifest ${rawPrefix}.manifest.json`);
}

const GROUPS = { direct: "Warm repeated bulk burst (alloc 1024 then free 1024)", churn: "Churn, non-writing (working set 256)", churn_write: "Churn + write (16 B written after each alloc)" };
const SIZES = [16, 64, 256, 1024];
const ARMS = ["sefer", "mimalloc", "system"];
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const n = new Set(runs[0].map((r) => r.round)).size;
for (const rows of runs) {
  if (new Set(rows.map((r) => r.round)).size !== n) throw new Error("processes differ in round count");
}
const csv = ["group,size,arm,batch,pairs,processes,rounds,process_min_ns,median_of_process_min_ns,sefer_over_mimalloc"];
console.log(`# bench:table (paired): ${runs.length} process(es) × ${n} rounds; cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)`);
for (const [g, title] of Object.entries(GROUPS)) {
  const gr = runs.flat().filter((r) => r.group === g);
  const pairs = new Set(gr.map((r) => r.pairs));
  const batches = new Set(gr.map((r) => r.batch));
  if (pairs.size !== 1 || batches.size !== 1) throw new Error(`${g}: mixed pairs/batch per sample`);
  const [p0, b0] = [[...pairs][0], [...batches][0]];
  console.log(`\n### ${title}\n\n${p0} pairs per sample, ${b0} working set(s) per sample\n`);
  console.log("| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |");
  console.log("|---|---:|---:|---:|---:|");
  for (const size of SIZES) {
    const cell = {};
    for (const a of ARMS) {
      const mins = runs.map((rows) => {
        const xs = rows.filter((r) => r.group === g && r.size === size && r.arm === a).map((r) => r.ns_per_pair);
        if (xs.length !== n) throw new Error(`${g} ${size} ${a}: ${xs.length} samples, want ${n}`);
        return Math.min(...xs);
      });
      cell[a] = { mins, med: median(mins) };
    }
    const r = cell.sefer.med / cell.mimalloc.med;
    const ratio = r <= 1 ? `**${(1 / r).toFixed(2)}× faster**` : `${r.toFixed(2)}× slower`;
    const f = (c) => `${c.med.toFixed(1)} (${Math.min(...c.mins).toFixed(1)}–${Math.max(...c.mins).toFixed(1)})`;
    console.log(`| ${size}B | ${f(cell.sefer)} | ${f(cell.mimalloc)} | ${f(cell.system)} | ${ratio} |`);
    for (const a of ARMS) {
      csv.push([g, size, a, b0, p0, runs.length, n, cell[a].mins.map((x) => x.toFixed(3)).join(";"), cell[a].med.toFixed(3), r.toFixed(4)].join(","));
    }
  }
}
if (csvPath) fs.writeFileSync(csvPath, csv.join("\n") + "\n");
