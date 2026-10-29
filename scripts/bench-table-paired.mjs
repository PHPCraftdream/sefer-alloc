#!/usr/bin/env node
// Paired short-sample companion of scripts/bench-table.mjs. Builds and runs
// examples/bench_table_paired.rs (every round measures every arm once per
// (group, size) in a small sample; the three samples of a triplet run back to
// back so they see one noise state; the arm order walks all six
// permutations) in `--processes` fresh processes, keeps each process's
// per-sample JSON lines as the raw artifact plus a manifest (commit, clean
// tree, features, binary sha256, rustc, CPU, OS), and derives the tables.
// Statistic: per process, the MINIMUM ns/pair over rounds (the least
// disturbed sample); a cell is the MEDIAN of those per-process minimums,
// with their range in parentheses (process-level state such as address
// layout is not removed by a minimum over rounds).
//
// Usage (from repo root):
//   node scripts/bench-table-paired.mjs [--rounds 200] [--calls 2] [--regime steady|fresh:K]
//        [--processes 3] [--features production] [--raw <prefix>] [--csv <path>]
//   node scripts/bench-table-paired.mjs --from <prefix> [--csv <path>]
// A run writes <prefix>.manifest.json and <prefix>.p<N>.jsonl; --from reads
// them back (each raw file may also be <prefix>.p<N>.jsonl.gz) and prints
// the same output byte for byte.

import { execFileSync } from "node:child_process";
import crypto from "node:crypto";
import fs from "node:fs";
import os from "node:os";
import process from "node:process";
import zlib from "node:zlib";

const arg = (name, dflt) => {
  const i = process.argv.indexOf(name);
  return i > 0 ? process.argv[i + 1] : dflt;
};
const from = arg("--from", null);
const prefix = from ?? arg("--raw", "target/bench-table-paired");
const csvPath = arg("--csv", null);

const GROUPS = { direct: "Warm repeated bulk burst (alloc 1024 then free 1024)", churn: "Churn, non-writing (working set 256)", churn_write: "Churn + write (16 B written after each alloc)" };
const SIZES = [16, 64, 256, 1024];
const ARMS = ["sefer", "mimalloc", "system"];
const KEYS = ["round", "group", "size", "arm", "order", "calls", "regime", "pairs", "ns_per_pair"];
const OPS = 1024;
// Arm orders of examples/bench_table_paired.rs: cell (group, size) in round r uses PERMS[(r + cell) % 6].
const PERMS = [[0, 1, 2], [0, 2, 1], [1, 0, 2], [1, 2, 0], [2, 0, 1], [2, 1, 0]];
const sha256 = (t) => crypto.createHash("sha256").update(t).digest("hex");

let manifest;
let texts;
if (from) {
  manifest = JSON.parse(fs.readFileSync(`${prefix}.manifest.json`, "utf8"));
  texts = [];
  for (let p = 1; p <= manifest.processes; p++) {
    const f = `${prefix}.p${p}.jsonl`;
    texts.push(fs.existsSync(f) ? fs.readFileSync(f, "utf8") : zlib.gunzipSync(fs.readFileSync(`${f}.gz`)).toString("utf8"));
  }
} else {
  const rounds = Number(arg("--rounds", 200));
  const calls = Number(arg("--calls", 2));
  const regime = arg("--regime", "steady");
  const processes = Number(arg("--processes", 3));
  const features = arg("--features", "production");
  const env = { ...process.env, RUSTC_WRAPPER: "" };
  execFileSync("cargo", ["build", "--release", "-q", "--example", "bench_table_paired", "--features", features], { stdio: "inherit", env });
  const dir = process.env.CARGO_TARGET_DIR ?? "target";
  const exe = `${dir}/release/examples/bench_table_paired${process.platform === "win32" ? ".exe" : ""}`;
  manifest = {
    commit: execFileSync("git", ["rev-parse", "HEAD"], { encoding: "utf8" }).trim(),
    dirty: execFileSync("git", ["status", "--porcelain", "--untracked-files=no"], { encoding: "utf8" }).trim() !== "",
    features,
    rounds,
    calls,
    regime,
    processes,
    exe_sha256: crypto.createHash("sha256").update(fs.readFileSync(exe)).digest("hex"),
    rustc: execFileSync("rustc", ["-vV"], { encoding: "utf8" }).trim().split(/\r?\n/),
    cpu: `${os.cpus()[0].model.trim()} x${os.cpus().length}`,
    os: `${os.version()} ${os.release()} ${os.arch()}`,
  };
  fs.writeFileSync(`${prefix}.manifest.json`, JSON.stringify(manifest, null, 2) + "\n");
  texts = [];
  for (let p = 1; p <= processes; p++) {
    const text = execFileSync(exe, [String(rounds), String(calls), regime], { encoding: "utf8", maxBuffer: 1 << 26 });
    fs.writeFileSync(`${prefix}.p${p}.jsonl`, text);
    texts.push(text);
  }
  // Bind the captured raw files to this manifest (identity above was written before measuring).
  manifest.raw_sha256 = texts.map(sha256);
  fs.writeFileSync(`${prefix}.manifest.json`, JSON.stringify(manifest, null, 2) + "\n");
}

// Validate: manifest parameters, raw files bound to the manifest, exact keys
// and types, one row per (round, group, size, arm), every cell in every
// round, order and pairs as the harness produces them.
const posInt = (x) => Number.isSafeInteger(x) && x > 0;
if (![manifest.rounds, manifest.calls, manifest.processes].every(posInt)) throw new Error("manifest: rounds/calls/processes must be positive integers");
const fresh = /^fresh:([1-9][0-9]*)$/.exec(manifest.regime);
if (manifest.regime !== "steady" && !fresh) throw new Error(`manifest: regime ${manifest.regime}`);
if (texts.length !== manifest.processes) throw new Error("process count differs from manifest");
if (!Array.isArray(manifest.raw_sha256) || manifest.raw_sha256.join() !== texts.map(sha256).join()) throw new Error("raw files do not match manifest raw_sha256");
const n = manifest.rounds;
if (new Set(texts).size !== texts.length) throw new Error("duplicate process input");
const groupNames = Object.keys(GROUPS);
const pairsFor = (g) => OPS * (g === "direct" || !fresh ? manifest.calls : Number(fresh[1]));
const runs = texts.map((text, pi) => {
  const rows = text.split(/\r?\n/).filter((l) => l.trim()).map((l) => JSON.parse(l));
  const seen = new Set();
  for (const r of rows) {
    if (Object.keys(r).join() !== KEYS.join()) throw new Error(`p${pi + 1}: unexpected keys ${Object.keys(r)}`);
    if (!groupNames.includes(r.group) || !SIZES.includes(r.size) || !ARMS.includes(r.arm)) throw new Error(`p${pi + 1}: unknown cell`);
    if (!Number.isInteger(r.round) || r.round < 0 || r.round >= n) throw new Error(`p${pi + 1}: round ${r.round}`);
    if (!Number.isFinite(r.ns_per_pair) || r.ns_per_pair <= 0) throw new Error(`p${pi + 1}: ns_per_pair ${r.ns_per_pair}`);
    if (r.calls !== manifest.calls || r.regime !== (r.group === "direct" ? "direct" : manifest.regime)) throw new Error(`p${pi + 1}: parameters differ from manifest`);
    if (r.pairs !== pairsFor(r.group)) throw new Error(`p${pi + 1}: pairs ${r.pairs}, want ${pairsFor(r.group)}`);
    const cell = groupNames.indexOf(r.group) * SIZES.length + SIZES.indexOf(r.size);
    if (r.order !== PERMS[(r.round + cell) % PERMS.length].indexOf(ARMS.indexOf(r.arm))) throw new Error(`p${pi + 1}: order of ${r.round}/${r.group}/${r.size}/${r.arm}`);
    const key = `${r.round}/${r.group}/${r.size}/${r.arm}`;
    if (seen.has(key)) throw new Error(`p${pi + 1}: duplicate ${key}`);
    seen.add(key);
  }
  if (seen.size !== n * Object.keys(GROUPS).length * SIZES.length * ARMS.length) throw new Error(`p${pi + 1}: ${seen.size} samples, want a full grid`);
  return rows;
});

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  return s.length % 2 ? s[(s.length - 1) / 2] : (s[s.length / 2 - 1] + s[s.length / 2]) / 2;
};
const out = [];
const id = `commit ${manifest.commit.slice(0, 8)}${manifest.dirty ? "+dirty" : ""}, features \`${manifest.features}\`, churn regime \`${manifest.regime}\`, ${manifest.calls} bodies/sample for direct${fresh ? "" : " and steady churn"}; ${manifest.cpu}; ${manifest.os}`;
out.push(`# bench:table (paired): ${runs.length} process(es) × ${n} rounds; ${id}`);
out.push("# cell = median over processes of the per-process min ns/pair (range of per-process minimums in parentheses)");
const csv = ["group,size,arm,regime,pairs,processes,rounds,process_min_ns,median_of_process_min_ns,sefer_over_mimalloc,commit,features"];
const spreads = [];
for (const [g, title] of Object.entries(GROUPS)) {
  const pairs = new Set(runs.flat().filter((r) => r.group === g).map((r) => r.pairs));
  if (pairs.size !== 1) throw new Error(`${g}: mixed pairs per sample`);
  const p0 = [...pairs][0];
  out.push("", `### ${title}`, "", `${p0} pairs per sample${g === "direct" ? "" : `, churn regime \`${manifest.regime}\``}`, "");
  out.push("| Size | SeferAlloc (ns/pair) | mimalloc (ns/pair) | System (ns/pair) | Sefer vs mimalloc |");
  out.push("|---|---:|---:|---:|---:|");
  for (const size of SIZES) {
    const cell = {};
    for (const a of ARMS) {
      const mins = runs.map((rows) => Math.min(...rows.filter((r) => r.group === g && r.size === size && r.arm === a).map((r) => r.ns_per_pair)));
      cell[a] = { mins, med: median(mins), spread: Math.max(...mins) - Math.min(...mins) };
      spreads.push({ g, size, a, ...cell[a] });
    }
    const r = cell.sefer.med / cell.mimalloc.med;
    const ratio = r.toFixed(2) === "1.00" && (1 / r).toFixed(2) === "1.00" ? "1.00× (tie)" : r < 1 ? `**${(1 / r).toFixed(2)}× faster**` : `${r.toFixed(2)}× slower`;
    const f = (c) => `${c.med.toFixed(1)} (${Math.min(...c.mins).toFixed(1)}–${Math.max(...c.mins).toFixed(1)})`;
    out.push(`| ${size}B | ${f(cell.sefer)} | ${f(cell.mimalloc)} | ${f(cell.system)} | ${ratio} |`);
    for (const a of ARMS) {
      csv.push([g, size, a, g === "direct" ? "direct" : manifest.regime, p0, runs.length, n, cell[a].mins.map((x) => x.toFixed(3)).join(";"), cell[a].med.toFixed(3), r.toFixed(4), manifest.commit, `"${manifest.features}"`].join(","));
    }
  }
}
// Spread of per-process minimums (process-to-process), per arm.
out.push("", "Spread of the per-process minimums (max − min over processes), per arm:", "");
for (const a of ARMS) {
  const xs = spreads.filter((s) => s.a === a);
  const w = xs.reduce((m, s) => (s.spread / s.med > m.spread / m.med ? s : m));
  const within = xs.filter((s) => s.spread <= 0.3).length;
  out.push(`- ${a}: ${within}/${xs.length} cells within 0.3 ns; widest relative ${w.g} ${w.size}B ${w.spread.toFixed(3)} ns of ${w.med.toFixed(3)} (${((100 * w.spread) / w.med).toFixed(1)} %)`);
}
console.log(out.join("\n"));
if (csvPath) fs.writeFileSync(csvPath, csv.join("\n") + "\n");
