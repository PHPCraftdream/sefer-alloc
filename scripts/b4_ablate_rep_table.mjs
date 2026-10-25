#!/usr/bin/env node
// B4-ablate — PROCESS-REPLICATED table generator (J1′ v2 protocol).
//
// Why: one judge process gives a tight within-process CI, but the
// copy == original (null) A/B moves between processes far beyond that CI
// (measured: churn16 +0.46 % in one process, -1.83 % in another; single
// 4000-block processes up to +17 % on churn64 — address-placement state
// fixed for a process's lifetime). The unit of replication is therefore the
// PROCESS: R independent judge processes per mode (distinct order seeds),
// one median ln(t_B/t_A) per process and kernel, then the median across
// processes with a bootstrap over PROCESSES.
//
// Input: <dir>/<variant>_<mode>_r<i>.jsonl for i in 0..R-1 and the three
// modes aa_sefer, aa_variant, ab_variant (all of one build).
// Checks (hard errors): identity equal across all raws (head_sha,
// source_diff_sha256, exe_sha256, variant_name, variant_sha256, features,
// profile, rustc, cpu, blocks, kernels); order seeds pairwise distinct
// within a mode; per kernel per raw exact ABBA = BAAB = blocks/2;
// harness_allocs == 0, foreign calls == 0, *_calls == *_exp per portion.
// Statistic: per process, median over blocks of ln((b1+b2)/(a1+a2)); per
// kernel, median over processes; 95 % percentile bootstrap (2000 resamples
// of processes, deterministic PRNG). B−A ticks/call: per process the median
// over blocks of (b1+b2)/(b1_calls+b2_calls) − (a1+a2)/(a1_calls+a2_calls),
// then the median over processes with the same bootstrap. A ticks/call: the
// same per-process median of (a1+a2)/(a1_calls+a2_calls), median over
// processes (the base for the B−A column).
// Admission (pre-registered threshold of J1′): a kernel is admitted iff
// aa_sefer AND aa_variant both have |median| <= 0.01 and CI half-width <=
// 0.01. Null gate (--expect-admitted on variant "null"): additionally the
// ab_variant itself must pass the same threshold.
// Verdict (admitted only): VARIANT_FASTER iff CI upper < 0, BASE_FASTER iff
// CI lower > 0, else EQUAL_CI_HAS_0.
//
// Usage: node scripts/b4_ablate_rep_table.mjs --dir <raw dir> --variant <name>
//   --reps <R> --kernels <csv> [--csv <path>] [--summary-append <path>]
//   [--expect-admitted] [--bootstrap 2000] [--seed 0x5EED]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const THRESHOLD = 0.01;
const MODES = ["aa_sefer", "aa_variant", "ab_variant"];

const args = { bootstrap: 2000, seed: 0x5eed, expectAdmitted: false };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  const val = () => {
    const v = process.argv[++i];
    if (v === undefined || v.startsWith("--")) throw new Error(`${a} needs a value`);
    return v;
  };
  if (a === "--dir") args.dir = val();
  else if (a === "--variant") args.variant = val();
  else if (a === "--reps") args.reps = Number(val());
  else if (a === "--kernels") args.kernels = val().split(",").map((s) => s.trim()).filter(Boolean);
  else if (a === "--csv") args.csv = val();
  else if (a === "--summary-append") args.summaryAppend = val();
  else if (a === "--bootstrap") args.bootstrap = Number(val());
  else if (a === "--seed") args.seed = Number(val());
  else if (a === "--expect-admitted") args.expectAdmitted = true;
  else throw new Error(`unknown argument ${a}`);
}
if (!args.dir || !args.variant || !args.kernels?.length) throw new Error("--dir, --variant, --kernels are required");
if (!Number.isInteger(args.reps) || args.reps < 3) throw new Error(`--reps must be an integer >= 3, got ${args.reps}`);
if (!Number.isInteger(args.bootstrap) || args.bootstrap < 100) throw new Error("--bootstrap must be an integer >= 100");

const ID_FIELDS = ["head_sha", "source_diff_sha256", "exe_sha256", "variant_name", "variant_sha256",
  "features", "profile", "rustc", "cpu", "blocks"];

function median(xs) {
  if (xs.length === 0) throw new Error("median of empty set");
  const s = [...xs].sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[n >> 1] : (s[n / 2 - 1] + s[n / 2]) / 2;
}

// Deterministic 32-bit LCG (Numerical Recipes); bootstrap over processes.
function bootstrapCi(xs, seed) {
  let st = seed >>> 0;
  const rnd = () => ((st = (Math.imul(st, 1664525) + 1013904223) >>> 0) / 4294967296);
  const bs = [];
  for (let b = 0; b < args.bootstrap; b++) bs.push(median(xs.map(() => xs[Math.floor(rnd() * xs.length)])));
  bs.sort((x, y) => x - y);
  const lo = bs[Math.floor(0.025 * args.bootstrap)];
  const hi = bs[Math.ceil(0.975 * args.bootstrap) - 1];
  if (!(lo <= hi)) throw new Error("bootstrap CI inverted");
  return { lo, hi, half: (hi - lo) / 2 };
}

let id0 = null;
// stats[mode][kernel] = { logs: [per-process], ticks: [per-process] }
const stats = {};
for (const mode of MODES) {
  stats[mode] = {};
  const orderSeeds = new Set();
  for (let r = 0; r < args.reps; r++) {
    const p = path.join(args.dir, `${args.variant}_${mode}_r${r}.jsonl`);
    const lines = fs.readFileSync(p, "utf8").split("\n").filter(Boolean);
    const header = JSON.parse(lines[0]);
    if (header.header !== true) throw new Error(`${p}: first line is not a header`);
    if (header.mode !== mode) throw new Error(`${p}: header mode ${header.mode} != ${mode}`);
    if (id0 === null) id0 = header;
    for (const f of ID_FIELDS)
      if (header[f] !== id0[f]) throw new Error(`${p}: identity field ${f}=${header[f]} != ${id0[f]}`);
    if (orderSeeds.has(header.order_seed)) throw new Error(`${p}: duplicate order_seed ${header.order_seed} within ${mode}`);
    orderSeeds.add(header.order_seed);
    const per = {};
    for (const l of lines.slice(1)) {
      const rec = JSON.parse(l);
      if (rec.mode !== mode) throw new Error(`${p}: record mode ${rec.mode}`);
      if (rec.harness_allocs !== 0) throw new Error(`${p} ${rec.kernel} block ${rec.block}: harness_allocs=${rec.harness_allocs}`);
      for (const q of ["a1", "b1", "b2", "a2"]) {
        if (rec[`${q}_foreign`] !== 0) throw new Error(`${p} ${rec.kernel} block ${rec.block}: ${q}_foreign=${rec[`${q}_foreign`]}`);
        if (rec[`${q}_calls`] !== rec[`${q}_exp`]) throw new Error(`${p} ${rec.kernel} block ${rec.block}: ${q}_calls != ${q}_exp`);
        if (!(rec[q] > 0)) throw new Error(`${p} ${rec.kernel} block ${rec.block}: ${q}=${rec[q]}`);
      }
      (per[rec.kernel] ??= []).push(rec);
    }
    for (const k of args.kernels) {
      const recs = per[k];
      if (!recs) throw new Error(`${p}: kernel ${k} missing`);
      if (recs.length !== header.blocks) throw new Error(`${p} ${k}: ${recs.length} blocks != ${header.blocks}`);
      const abba = recs.filter((x) => x.order === "ABBA").length;
      if (abba * 2 !== header.blocks || recs.length - abba !== abba) throw new Error(`${p} ${k}: ABBA=${abba} of ${recs.length} (must be exactly N/2)`);
      const logs = recs.map((x) => Math.log((x.b1 + x.b2) / (x.a1 + x.a2)));
      const ticks = recs.map((x) => (x.b1 + x.b2) / (x.b1_calls + x.b2_calls) - (x.a1 + x.a2) / (x.a1_calls + x.a2_calls));
      const aTicks = recs.map((x) => (x.a1 + x.a2) / (x.a1_calls + x.a2_calls));
      const s = (stats[mode][k] ??= { logs: [], ticks: [], aTicks: [] });
      s.logs.push(median(logs));
      s.ticks.push(median(ticks));
      s.aTicks.push(median(aTicks));
    }
    for (const k of Object.keys(per))
      if (!args.kernels.includes(k)) throw new Error(`${p}: unexpected kernel ${k} (not in --kernels)`);
  }
}

const rows = [];
args.kernels.forEach((k, ki) => {
  const row = { kernel: k };
  MODES.forEach((mode, mi) => {
    const s = stats[mode][k];
    if (s.logs.length !== args.reps) throw new Error(`${mode} ${k}: ${s.logs.length} processes != ${args.reps}`);
    const m = median(s.logs);
    const ci = bootstrapCi(s.logs, args.seed + 97 * ki + 13 * mi);
    const t = median(s.ticks);
    const tci = bootstrapCi(s.ticks, args.seed + 97 * ki + 13 * mi + 7);
    row[mode] = { m, ...ci, pass: Math.abs(m) <= THRESHOLD && ci.half <= THRESHOLD, t, tlo: tci.lo, thi: tci.hi, a: median(s.aTicks),
      pmin: Math.min(...s.logs), pmax: Math.max(...s.logs) };
  });
  row.admitted = row.aa_sefer.pass && row.aa_variant.pass;
  const ab = row.ab_variant;
  row.ratio = Math.exp(ab.m);
  if (Math.abs(Math.log(row.ratio) - ab.m) > 1e-12) throw new Error("ratio != exp(median)");
  row.verdict = !row.admitted ? "not admitted" : ab.hi < 0 ? "VARIANT_FASTER" : ab.lo > 0 ? "BASE_FASTER" : "EQUAL_CI_HAS_0";
  rows.push(row);
});

const pct = (x) => (x * 100).toFixed(2);
console.log(`# B4 ABLATE — process-replicated (J1′ v2): variant=${args.variant} reps=${args.reps} blocks/process=${id0.blocks} bootstrap=${args.bootstrap} (over processes) seed=${args.seed}`);
console.log(`# identity: head=${id0.head_sha} sha(diff)=${id0.source_diff_sha256.slice(0, 12)} exe=${id0.exe_sha256.slice(0, 12)} variant_sha=${id0.variant_sha256.slice(0, 12)} profile=${id0.profile} rustc="${id0.rustc}" cpu="${id0.cpu}"`);
console.log(`# statistic: per-process median of ln(t_B/t_A) (A = sefer base, B = variant), then median over ${args.reps} processes; admission |median| <= ${THRESHOLD} and CI half <= ${THRESHOLD} for aa_sefer AND aa_variant; oracle exact, balance exact`);
console.log("");
console.log("| Kernel | A/A sefer median % [CI] | A/A variant median % [CI] | A/B median % [CI] | per-process A/B min…max % | ratio t_B/t_A | A ticks/call | B−A ticks/call [CI] | Admitted | Verdict |");
console.log("|---|---|---|---|---|---:|---:|---|---|---|");
for (const r of rows) {
  const f = (s) => `${pct(s.m)} [${pct(s.lo)}, ${pct(s.hi)}]`;
  console.log(`| ${r.kernel} | ${f(r.aa_sefer)} | ${f(r.aa_variant)} | ${f(r.ab_variant)} | ${pct(r.ab_variant.pmin)}…${pct(r.ab_variant.pmax)} | ${r.ratio.toFixed(4)} | ${r.ab_variant.a.toFixed(2)} | ${r.ab_variant.t.toFixed(2)} [${r.ab_variant.tlo.toFixed(2)}, ${r.ab_variant.thi.toFixed(2)}] | ${r.admitted ? "yes" : "NO"} | ${r.verdict} |`);
}

const CSV_HEAD = ["variant", "variant_sha256", "kernel", "reps", "blocks_per_process", "admitted", "verdict",
  "aa_sefer_median", "aa_sefer_ci_lo", "aa_sefer_ci_hi", "aa_variant_median", "aa_variant_ci_lo", "aa_variant_ci_hi",
  "ab_median", "ab_ci_lo", "ab_ci_hi", "ab_process_min", "ab_process_max", "ratio", "a_ticks_per_call", "ticks_per_call_b_minus_a",
  "ticks_ci_lo", "ticks_ci_hi", "head_sha", "source_diff_sha256", "exe_sha256", "cpu"];
const csvRows = rows.map((r) => [args.variant, id0.variant_sha256, r.kernel, args.reps, id0.blocks, r.admitted ? "yes" : "no", r.verdict,
  r.aa_sefer.m, r.aa_sefer.lo, r.aa_sefer.hi, r.aa_variant.m, r.aa_variant.lo, r.aa_variant.hi,
  r.ab_variant.m, r.ab_variant.lo, r.ab_variant.hi, r.ab_variant.pmin, r.ab_variant.pmax, r.ratio, r.ab_variant.a, r.ab_variant.t,
  r.ab_variant.tlo, r.ab_variant.thi, id0.head_sha, id0.source_diff_sha256, id0.exe_sha256, `"${id0.cpu}"`].join(","));
if (args.csv) {
  fs.mkdirSync(path.dirname(args.csv), { recursive: true });
  fs.writeFileSync(args.csv, [CSV_HEAD.join(","), ...csvRows].join("\n") + "\n");
  console.log(`# wrote ${args.csv}`);
}
if (args.summaryAppend) {
  const exists = fs.existsSync(args.summaryAppend);
  if (exists && fs.readFileSync(args.summaryAppend, "utf8").split("\n")[0] !== CSV_HEAD.join(","))
    throw new Error(`${args.summaryAppend}: header mismatch`);
  fs.appendFileSync(args.summaryAppend, (exists ? "" : CSV_HEAD.join(",") + "\n") + csvRows.join("\n") + "\n");
  console.log(`# appended ${rows.length} rows to ${args.summaryAppend}`);
}

if (args.expectAdmitted) {
  const bad = rows.filter((r) => !r.admitted).map((r) => r.kernel);
  if (bad.length) {
    console.error(`EXPECT_ADMITTED FAILED: not admitted: ${bad.join(",")}`);
    process.exit(2);
  }
  if (args.variant === "null") {
    const off = rows.filter((r) => !r.ab_variant.pass).map((r) => r.kernel);
    if (off.length) {
      console.error(`NULL_GATE FAILED (variant "null" must be a copy of the original): ab_variant outside the A/A threshold on: ${off.join(",")}`);
      process.exit(3);
    }
  }
}
