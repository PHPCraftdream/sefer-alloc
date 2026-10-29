#!/usr/bin/env node
// B4 J1′ interleave judge — ONE-SHOT phase-2 runner (used by the orchestrator).
//
// REQUIRED ENVIRONMENT: RUSTFLAGS must contain BOTH
//   -C llvm-args=-align-all-functions=6
//   -C llvm-args=-align-all-nofallthru-blocks=5
// (function alignment is part of the measured configuration identity; a
// build without these flags is NOT comparable, so the runner refuses to
// start: exit != 0 with a clear message).
//
// EVERY child command runs with a hard timeout and a checked exit code
// (non-zero or timeout → throw with the captured output). CARGO_TARGET_DIR
// must be set in the environment or the runner aborts before doing anything.
//
// Does, strictly in order:
//   1. builds the judge (`cargo build --profile <p> --example b4_interleave_judge
//      --features production`), unless --skip-build;
//   2. computes the identity: sha256 over the EXACT bytes of `git diff`
//      (NO trim), `git rev-parse HEAD` (40 hex, passed as --head-sha), and
//      `rustc --version`;
//   3. runs the judge THREE times sequentially (aa_sefer, aa_mi, ab) with the
//      exact --blocks and all REQUIRED identity arguments
//      (--head-sha, --source-diff-sha, --features, --profile, --rustc,
//      --order-seed);
//   4. runs the table generator over the three raws (ONE table + ONE CSV).
//
// Usage:
//   node scripts/b4_interleave_run.mjs --blocks 20000 [--profile release|bench-user]
//        [--features production] [--seed 0xb4110001] [--order-seed 0x0b4d5eed]
//        [--out-prefix docs/perf/_raw_b4_interleave] [--cargo cargo]
//        [--skip-build] [--build-timeout 900] [--run-timeout 900]
//        [--reps <R>]
//
// --reps <R> (PROCESS-REPLICATED mode): each mode (aa_sefer, aa_mi, ab) is
// run R times as R INDEPENDENT judge processes with order seed = base + i
// (i = 0..R-1), raw files <dir>/<mode>_r<i>.jsonl (dir = dirname of the
// out-prefix), then the process-replicated generator
// scripts/b4_interleave_rep_table.mjs is invoked (--dir <dir> --reps R).
// WITHOUT --reps the legacy single-process path is preserved exactly.
//
// CLI: --skip-build is a FLAG (no value); the other options take exactly one
// value; ANY unknown argument (or a flag-like value) is an error. Timeouts
// (--run-timeout, --build-timeout) are SECONDS (positive integers).

import { execFileSync, spawnSync } from "node:child_process";
import { createHash } from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

// Strict parser: explicit list of flags vs options-with-a-value.
const FLAGS = new Set(["--skip-build"]);
const OPTS = new Set([
  "--blocks", "--profile", "--features", "--seed", "--order-seed",
  "--out-prefix", "--cargo", "--build-timeout", "--run-timeout", "--reps",
]);
const argv = process.argv.slice(2);
const opt = {};
for (let i = 0; i < argv.length; i++) {
  const a = argv[i];
  if (FLAGS.has(a)) {
    opt[a] = true;
  } else if (OPTS.has(a)) {
    const v = argv[++i];
    if (v === undefined || v.startsWith("--")) throw new Error(`${a} needs a value`);
    if (opt[a] !== undefined) throw new Error(`${a} given twice`);
    opt[a] = v;
  } else {
    throw new Error(`unknown argument ${a} (known flags: ${[...FLAGS, ...OPTS].join(" ")})`);
  }
}
const blocks = opt["--blocks"];
if (!blocks || !/^\d+$/.test(blocks))
  throw new Error("--blocks <N> is REQUIRED (exact block count for all three runs)");
if (Number(blocks) % 2 !== 0)
  throw new Error(`--blocks must be EVEN (strict ABBA=BAAB=N/2 balance), got ${blocks}`);
const profile = opt["--profile"] ?? "release";
if (profile !== "release" && profile !== "bench-user")
  throw new Error(`--profile must be release|bench-user, got ${profile}`);
const features = opt["--features"] ?? "production";
const seed = opt["--seed"] ?? "0xb4110001";
const orderSeed = opt["--order-seed"] ?? "0x0b4d5eed";
const outPrefix = opt["--out-prefix"] ?? "docs/perf/_raw_b4_interleave";
const cargo = opt["--cargo"] ?? "cargo";
// Timeouts in SECONDS (positive integers), converted to ms for spawnSync.
function secondsOpt(name, def) {
  const raw = opt[name] ?? String(def);
  const v = Number(raw);
  if (!Number.isInteger(v) || !Number.isFinite(v) || v <= 0)
    throw new Error(`${name} must be a positive integer number of SECONDS, got ${raw}`);
  return v * 1000;
}
const buildTimeout = secondsOpt("--build-timeout", 900);
const runTimeout = secondsOpt("--run-timeout", 900);
const skipBuild = opt["--skip-build"] === true;

const repsOpt = opt["--reps"];
let reps = null; // null = legacy single-process path
if (repsOpt !== undefined) {
  reps = Number(repsOpt);
  if (!Number.isInteger(reps) || reps < 3)
    throw new Error(`--reps must be an integer >= 3 (process replication), got ${repsOpt}`);
}
const targetDir = process.env.CARGO_TARGET_DIR;
if (!targetDir) throw new Error("CARGO_TARGET_DIR must be set (shared target dir, NOT in the repo)");

// RUSTFLAGS gate: both alignment flags are part of the measured identity.
const rf = process.env.RUSTFLAGS ?? "";
const RF1 = "-C llvm-args=-align-all-functions=6";
const RF2 = "-C llvm-args=-align-all-nofallthru-blocks=5";
if (!rf.includes(RF1) || !rf.includes(RF2))
  throw new Error(
    `RUSTFLAGS must contain BOTH "${RF1}" AND "${RF2}" (found: ${JSON.stringify(rf)}); ` +
    `set e.g. RUSTFLAGS=${JSON.stringify(RF1 + " " + RF2)} and rebuild`);

// run(cmd, args, {timeout, label}): spawnSync with a hard timeout; throws
// with stdout+stderr on a non-zero exit, a signal or a timeout.
function run(cmd, cmdArgs, { timeout, label }) {
  const r = spawnSync(cmd, cmdArgs, { encoding: "buffer", timeout });
  const buf = Buffer.concat([r.stdout ?? Buffer.alloc(0), r.stderr ?? Buffer.alloc(0)]).toString("utf8");
  if (r.error) throw new Error(`[${label}] ${cmd} failed to start / TIMED OUT (${timeout} ms): ${r.error.message}\n${buf}`);
  if (r.status !== 0 || r.signal)
    throw new Error(`[${label}] ${cmd} ${cmdArgs.join(" ")} exited with status=${r.status} signal=${r.signal}\n${buf}`);
  return Buffer.concat([r.stdout ?? Buffer.alloc(0), r.stderr ?? Buffer.alloc(0)]);
}

// runAndEcho: like run, but ECHOES the child's captured stdout+stderr so the
// orchestrator log contains the full judge/generator output verbatim.
function runAndEcho(cmd, cmdArgs, opts) {
  const out = run(cmd, cmdArgs, opts);
  process.stdout.write(out.toString("utf8"));
  return out;
}

// 1. Build.
if (!skipBuild) {
  console.log(`[run] cargo build --profile ${profile} --example b4_interleave_judge --features ${features}`);
  run(cargo, ["build", "--profile", profile, "--example", "b4_interleave_judge", "--features", features],
    { timeout: buildTimeout, label: "build" });
}
const exe = (process.platform === "win32" ? `${targetDir}/${profile}/examples/b4_interleave_judge.exe` : `${targetDir}/${profile}/examples/b4_interleave_judge`);
console.log(`[run] judge binary: ${exe}`);
run("node", ["-e", `require("node:fs").accessSync(${JSON.stringify(exe)})`], { timeout: 10000, label: "check-exe" });

// 2. Identity: diff sha256 over EXACT bytes (no trim), HEAD sha, rustc version.
const diffBytes = run("git", ["diff"], { timeout: 60000, label: "git-diff" });
const diffSha = createHash("sha256").update(diffBytes).digest("hex");
const headSha = run("git", ["rev-parse", "HEAD"], { timeout: 10000, label: "git-rev-parse" }).toString("utf8").trim();
if (!/^[0-9a-f]{40}$/.test(headSha))
  throw new Error(`git rev-parse HEAD returned a non-40-hex value: ${headSha}`);
const rustc = run("rustc", ["--version"], { timeout: 30000, label: "rustc-version" }).toString("utf8").trim();

// OS version: on Windows `cmd /c ver` (e.g. "Microsoft Windows [Version
// 10.0.26200.7462]") parsed into major.minor.build; on non-Windows the
// literal "unknown" (documented choice: a numeric winver equivalent needs
// platform-specific APIs; the judge accepts and records either).
function osVersion() {
  if (process.platform !== "win32") return "unknown";
  const out = run("cmd", ["/c", "ver"], { timeout: 15000, label: "win-ver" }).toString("utf8");
  const m2 = out.match(/Version\s+(\d+)\.(\d+)\.(\d+)/);
  if (!m2) throw new Error(`cannot parse Windows version from "cmd /c ver" output: ${JSON.stringify(out)}`);
  return `${m2[1]}.${m2[2]}.${m2[3]}`;
}
const osVersionStr = osVersion();
console.log(`[run] head_sha=${headSha}`);
console.log(`[run] source_diff_sha256=${diffSha}`);
console.log(`[run] ${rustc}`);
console.log(`[run] os_version=${osVersionStr}`);

// 3+4. Judge runs and the generator.
if (reps !== null) {
  // PROCESS-REPLICATED path: R independent judge processes per mode with
  // order seed = base + i, raw files <dir>/<mode>_r<i>.jsonl.
  // Parse the order seed as BigInt: 64-bit seeds exceed Number's safe range.
  // BigInt() accepts both "0x..." and decimal strings.
  const baseSeed = BigInt(orderSeed);
  if (baseSeed < 0n)
    throw new Error(`--order-seed must be a non-negative integer, got ${orderSeed}`);
  const outDir = path.dirname(outPrefix);
  fs.mkdirSync(outDir, { recursive: true });
  for (const mode of ["aa_sefer", "aa_mi", "ab"]) {
    for (let i = 0; i < reps; i++) {
      const out = path.join(outDir, `${mode}_r${i}.jsonl`);
      const os = `0x${(BigInt(baseSeed) + BigInt(i)).toString(16)}`;
      console.log(`[run] judge --mode ${mode} --blocks ${blocks} --order-seed ${os} -> ${out}`);
      runAndEcho(exe,
        ["--mode", mode, "--blocks", blocks, "--seed", seed, "--order-seed", os,
         "--out", out, "--head-sha", headSha,
         "--source-diff-sha", diffSha, "--features", features, "--profile", profile, "--rustc", rustc,
         "--os-version", osVersionStr],
        { timeout: runTimeout, label: `judge-${mode}-r${i}` });
    }
  }
  const kernels = "pair16,pair64,pair_rt,bulk64,bulk16k,churn16,churn64,churn256,churn1024,vec512,larson";
  console.log(`[run] generator (process-replicated, reps=${reps}, order seeds base..base+${reps - 1})`);
  runAndEcho(process.execPath,
    ["scripts/b4_interleave_rep_table.mjs", "--dir", outDir, "--reps", String(reps),
     "--kernels", kernels, "--csv", "docs/perf/B4_INTERLEAVE_JUDGE_rep_summary.csv"],
    { timeout: 300000, label: "generator-rep" });
} else {
  // LEGACY single-process path: three runs, one shared order seed.
  fs.mkdirSync(path.dirname(outPrefix), { recursive: true });
  for (const mode of ["aa_sefer", "aa_mi", "ab"]) {
    const out = `${outPrefix}_${mode}.jsonl`;
    console.log(`[run] judge --mode ${mode} --blocks ${blocks} -> ${out}`);
    runAndEcho(exe,
      ["--mode", mode, "--blocks", blocks, "--seed", seed, "--order-seed", orderSeed,
       "--out", out, "--head-sha", headSha,
       "--source-diff-sha", diffSha, "--features", features, "--profile", profile, "--rustc", rustc,
       "--os-version", osVersionStr],
      { timeout: runTimeout, label: `judge-${mode}` });
  }
  console.log("[run] generator");
  runAndEcho(process.execPath,
    ["scripts/b4_interleave_table.mjs", "--aa-sefer", `${outPrefix}_aa_sefer.jsonl`,
     "--aa-mi", `${outPrefix}_aa_mi.jsonl`, "--ab", `${outPrefix}_ab.jsonl`],
    { timeout: 300000, label: "generator" });
}
console.log("[run] OK");
