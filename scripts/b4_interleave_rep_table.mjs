#!/usr/bin/env node
// B4 J1′ interleave judge — PROCESS-REPLICATED table generator (reps mode).
//
// Input: a directory with R raw JSONL files per mode (i in 0..R-1):
//   <dir>/aa_sefer_r<i>.jsonl   A/A calibration sefer×sefer
//   <dir>/aa_mi_r<i>.jsonl      A/A calibration mimalloc×mimalloc
//   <dir>/ab_r<i>.jsonl         the judge: sefer vs mimalloc
// The unit of replication is the PROCESS (see b4_ablate_rep_table.mjs for
// the rationale): per process, median over blocks of ln(t_B/t_A); then the
// median over processes with a 95 % percentile bootstrap (2000 resamples,
// deterministic PRNG) over PROCESSES.
//
// Hard checks (any failure aborts with exit != 0):
//   - identity (header, first line "header":true) EQUAL across ALL raws of
//     all modes and reps — every field of ID_FIELDS; `order_seed` is the
//     documented exception: it must be pairwise DISTINCT within a mode
//     (process replication) and equal-checked nowhere;
//   - per kernel per raw: exactly header.blocks records, blocks 0..N-1
//     without gaps/duplicates, STRICT ABBA = BAAB = N/2 (exact balance),
//     harness_allocs == 0, foreign == 0 for all four portions,
//     *_calls == *_exp for all four portions (activation oracle);
//   - record `seed` must equal the judge's portion-seed formula
//     (seed + block*0x1000000000000001, rotl64 by kernel registry index) —
//     BigInt port, same as b4_interleave_table.mjs;
//   - every record kernel must be in KERNEL_NAMES and in --kernels.
//
// Statistic (A = sefer, B = mimalloc, ln(t_B/t_A)):
//   - per process: median over blocks; per mode: median over processes;
//   - bootstrap CI over processes (2000 resamples, deterministic PRNG);
//   - absolute ticks/call for A and for B (per-process medians, then the
//     median over processes with the same bootstrap);
//   - t_mimalloc/t_sefer = exp(median), asserted (|log(exp(m)) − m| < 1e-12);
//   - per-process min…max of the ab median logs.
// Admission (pre-registered): |median| <= 1 % AND CI half-width <= 1 % for
// aa_sefer AND aa_mi. Verdict (admitted only): MIMALLOC_FASTER iff the CI
// upper bound < 0, SEFER_FASTER iff the lower bound > 0, else EQUAL_CI_HAS_0.
//
// Usage:
//   node scripts/b4_interleave_rep_table.mjs --dir <raw dir> --reps <R>
//        --kernels <comma-separated> [--csv <path>] [--bootstrap 2000]
//        [--seed 0x5EED]
// Usage errors exit with a non-zero code and a message (no stack).

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const THRESHOLD = 0.01; // |median| <= 1 % and CI half-width <= 1 %
const MODES = ["aa_sefer", "aa_mi", "ab"];
const KERNEL_NAMES = [
  "pair16", "pair64", "pair_rt", "bulk64", "bulk16k",
  "churn16", "churn64", "churn256", "churn1024", "vec512", "larson",
  "bulk64k", "bulk16k_32",
];
const ID_FIELDS = [
  "blocks", "seed", "head_sha", "cpu", "os", "os_version",
  "source_diff_sha256", "exe_sha256", "features", "profile", "rustc",
  "tsc_invariant", "qpc_freq", "pair_ops", "bulk64_n", "bulk64_passes",
  "bulk16k_n", "bulk16k_passes", "bulk64k_n", "bulk64k_passes", "bulk16k32_n",
  "bulk16k32_passes", "churn_live", "churn_ops", "churn1024_ops",
  "vec_elems", "vec_cycles", "larson_live", "larson_ops", "larson_min",
  "larson_max", "larson_step", "rt_min", "rt_max",
  "rt_granule", "portion_seed_g", "larson_align", "vec_align", "churn_align",
  "pair_align", "warmup_portions", "kernels",
];
const MASK64 = (1n << 64n) - 1n;
function rotl64(x, k) {
  const kk = BigInt(k);
  return ((x << kk) | (x >> (64n - kk))) & MASK64;
}
function portionSeed(headerSeedBig, block, ki) {
  return rotl64((headerSeedBig + BigInt(block) * 0x1000000000000001n) & MASK64, ki);
}

function median(xs) {
  if (xs.length === 0) throw new Error("median of empty set");
  const s = [...xs].sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[n >> 1] : (s[n / 2 - 1] + s[n / 2]) / 2;
}

// Deterministic PRNG (mulberry32) + percentile bootstrap over PROCESSES.
function bootstrapCi(xs, seed, nBoot) {
  let st = seed >>> 0;
  const rnd = () => {
    st = (st + 0x6d2b79f5) | 0;
    let t = Math.imul(st ^ (st >>> 15), 1 | st);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
  const bs = [];
  for (let b = 0; b < nBoot; b++) {
    bs.push(median(Array.from({ length: xs.length }, () => xs[Math.floor(rnd() * xs.length)])));
  }
  bs.sort((x, y) => x - y);
  const lo = bs[Math.floor(0.025 * nBoot)];
  const hi = bs[Math.ceil(0.975 * nBoot) - 1];
  if (!(lo <= hi)) throw new Error("bootstrap CI inverted");
  return { lo, hi, half: (hi - lo) / 2 };
}

function main() {
  // ── args ──────────────────────────────────────────────────────────────────
  const args = { bootstrap: 2000, seed: 0x5eed };
  for (let i = 2; i < process.argv.length; i++) {
    const a = process.argv[i];
    const val = () => {
      const v = process.argv[++i];
      if (v === undefined || v.startsWith("--")) throw new Error(`${a} needs a value`);
      return v;
    };
    if (a === "--dir") args.dir = val();
    else if (a === "--reps") args.reps = Number(val());
    else if (a === "--kernels") args.kernels = val().split(",").map((s) => s.trim()).filter(Boolean);
    else if (a === "--csv") args.csv = val();
    else if (a === "--bootstrap") args.bootstrap = Number(val());
    else if (a === "--seed") args.seed = Number(val());
    else throw new Error(`unknown argument ${a}`);
  }
  if (!args.dir || !args.kernels?.length) throw new Error("--dir, --reps, --kernels are required");
  if (!Number.isInteger(args.reps) || args.reps < 3) throw new Error(`--reps must be an integer >= 3, got ${args.reps}`);
  if (!Number.isInteger(args.bootstrap) || args.bootstrap < 100) throw new Error(`--bootstrap must be an integer >= 100, got ${args.bootstrap}`);
  for (const k of args.kernels)
    if (!KERNEL_NAMES.includes(k)) throw new Error(`unknown kernel ${k} (not in KERNEL_NAMES)`);

  // ── read raws + hard checks ───────────────────────────────────────────────
  let id0 = null;
  // stats[mode][kernel] = { logs: [], ticksB: [], ticksA: [] } (per-process).
  const stats = {};
  for (const mode of MODES) {
    stats[mode] = {};
    const orderSeeds = new Set();
    for (let r = 0; r < args.reps; r++) {
      const p = path.join(args.dir, `${mode}_r${r}.jsonl`);
      const lines = fs.readFileSync(p, "utf8").split(/\r?\n/).filter(Boolean);
      const header = JSON.parse(lines[0]);
      if (header.header !== true) throw new Error(`${p}: first line is not a header`);
      if (header.mode !== mode) throw new Error(`${p}: header mode ${header.mode} != ${mode}`);
      if (id0 === null) id0 = header;
      for (const f of ID_FIELDS)
        if (JSON.stringify(header[f]) !== JSON.stringify(id0[f]))
          throw new Error(`${p}: identity field ${f}=${JSON.stringify(header[f])} != ${JSON.stringify(id0[f])}`);
      if (orderSeeds.has(header.order_seed))
        throw new Error(`${p}: duplicate order_seed ${header.order_seed} within ${mode} (order seeds must be pairwise distinct per process)`);
      orderSeeds.add(header.order_seed);
      if (!Number.isSafeInteger(header.blocks) || header.blocks <= 0 || header.blocks % 2 !== 0)
        throw new Error(`${p}: header blocks must be a positive EVEN integer, got ${header.blocks}`);
      const HSEED = BigInt(header.seed);
      const per = {};
      for (const l of lines.slice(1)) {
        const rec = JSON.parse(l);
        if (rec.mode !== mode) throw new Error(`${p}: record mode ${rec.mode} != ${mode}`);
        if (!KERNEL_NAMES.includes(rec.kernel))
          throw new Error(`${p}: unknown kernel ${JSON.stringify(rec.kernel)} (not in KERNEL_NAMES)`);
        if (!args.kernels.includes(rec.kernel))
          throw new Error(`${p}: unexpected kernel ${rec.kernel} (not in --kernels)`);
        if (rec.harness_allocs !== 0) throw new Error(`${p} ${rec.kernel} block ${rec.block}: harness_allocs=${rec.harness_allocs}`);
        const wantSeed = portionSeed(HSEED, rec.block, KERNEL_NAMES.indexOf(rec.kernel));
        if (BigInt(rec.seed) !== wantSeed)
          throw new Error(`${p} ${rec.kernel} block ${rec.block}: seed ${rec.seed} != portion_seed formula 0x${wantSeed.toString(16)}`);
        for (const q of ["a1", "b1", "b2", "a2"]) {
          if (rec[`${q}_foreign`] !== 0) throw new Error(`${p} ${rec.kernel} block ${rec.block}: ${q}_foreign=${rec[`${q}_foreign`]}`);
          if (rec[`${q}_calls`] !== rec[`${q}_exp`]) throw new Error(`${p} ${rec.kernel} block ${rec.block}: ${q}_calls=${rec[`${q}_calls`]} != ${q}_exp=${rec[`${q}_exp`]}`);
          if (!(rec[q] > 0)) throw new Error(`${p} ${rec.kernel} block ${rec.block}: ${q}=${rec[q]} (must be positive)`);
        }
        (per[rec.kernel] ??= []).push(rec);
      }
      for (const k of args.kernels) {
        const recs = per[k];
        if (!recs) throw new Error(`${p}: kernel ${k} missing`);
        if (recs.length !== header.blocks) throw new Error(`${p} ${k}: ${recs.length} blocks != ${header.blocks}`);
        const seen = new Set(recs.map((x) => x.block));
        if (seen.size !== recs.length) throw new Error(`${p} ${k}: duplicate block index`);
        for (let b = 0; b < header.blocks; b++)
          if (!seen.has(b)) throw new Error(`${p} ${k}: missing block ${b}`);
        const abba = recs.filter((x) => x.order === "ABBA").length;
        const baab = recs.filter((x) => x.order === "BAAB").length;
        if (abba + baab !== recs.length) {
          const bad = recs.find((x) => x.order !== "ABBA" && x.order !== "BAAB");
          throw new Error(`${p} ${k} block ${bad.block}: unknown order ${JSON.stringify(bad.order)} (must be "ABBA" or "BAAB")`);
        }
        if (!(abba === baab && abba === header.blocks / 2))
          throw new Error(`${p} ${k}: ABBA=${abba} BAAB=${baab} of ${recs.length} (must be ABBA == BAAB == N/2 = ${header.blocks / 2})`);
        const logs = recs.map((x) => Math.log((x.b1 + x.b2) / (x.a1 + x.a2)));
        const ticksB = recs.map((x) => (x.b1 + x.b2) / (x.b1_calls + x.b2_calls));
        const ticksA = recs.map((x) => (x.a1 + x.a2) / (x.a1_calls + x.a2_calls));
        const s = (stats[mode][k] ??= { logs: [], ticksB: [], ticksA: [] });
        s.logs.push(median(logs));
        s.ticksB.push(median(ticksB));
        s.ticksA.push(median(ticksA));
      }
    }
  }

  // ── per-kernel table ──────────────────────────────────────────────────────
  const rows = [];
  args.kernels.forEach((k, ki) => {
    const row = { kernel: k };
    MODES.forEach((mode, mi) => {
      const s = stats[mode][k];
      if (s.logs.length !== args.reps) throw new Error(`${mode} ${k}: ${s.logs.length} processes != ${args.reps}`);
      const m = median(s.logs);
      const ci = bootstrapCi(s.logs, args.seed + 97 * ki + 13 * mi, args.bootstrap);
      // t_mimalloc / t_sefer = exp(median), asserted arithmetic.
      const ratio = Math.exp(m);
      if (Math.abs(Math.log(ratio) - m) > 1e-12) throw new Error(`${mode} ${k}: exp/log round-trip failed`);
      const bT = median(s.ticksB);
      const aT = median(s.ticksA);
      const bTci = bootstrapCi(s.ticksB, args.seed + 97 * ki + 13 * mi + 7, args.bootstrap);
      const aTci = bootstrapCi(s.ticksA, args.seed + 97 * ki + 13 * mi + 11, args.bootstrap);
      row[mode] = {
        m, ratio, ...ci,
        pass: Math.abs(m) <= THRESHOLD && ci.half <= THRESHOLD,
        b: bT, bLo: bTci.lo, bHi: bTci.hi, a: aT, aLo: aTci.lo, aHi: aTci.hi,
        pmin: Math.min(...s.logs), pmax: Math.max(...s.logs),
      };
    });
    row.admitted = row.aa_sefer.pass && row.aa_mi.pass;
    const ab = row.ab;
    row.verdict = !row.admitted ? "not admitted"
      : ab.hi < 0 ? "MIMALLOC_FASTER"
      : ab.lo > 0 ? "SEFER_FASTER"
      : "EQUAL_CI_HAS_0";
    rows.push(row);
  });

  // ── output ────────────────────────────────────────────────────────────────
  const pct = (x) => (x * 100).toFixed(2);
  console.log(`# B4 INTERLEAVE JUDGE — process-replicated: reps=${args.reps} blocks/process=${id0.blocks} bootstrap=${args.bootstrap} (over processes) seed=${args.seed}`);
  console.log(`# identity: head=${id0.head_sha} sha(diff)=${id0.source_diff_sha256.slice(0, 12)} exe=${id0.exe_sha256.slice(0, 12)} profile=${id0.profile} rustc="${id0.rustc}" cpu="${id0.cpu}" os="${id0.os}"`);
  console.log(`# statistic: per-process median of ln(t_B/t_A) over blocks, then median over ${args.reps} processes; 95 % percentile bootstrap over processes; A = sefer, B = mimalloc; ratio = t_mimalloc / t_sefer = exp(median)`);
  console.log(`# admission: |median| <= ${THRESHOLD} AND CI half-width <= ${THRESHOLD} for aa_sefer AND aa_mi; verdict (admitted only): MIMALLOC_FASTER iff upper CI < 0, SEFER_FASTER iff lower CI > 0, else EQUAL_CI_HAS_0`);
  console.log("");
  console.log("| Kernel | A/A sefer median % [CI] | A/A mi median % [CI] | A/B median % [CI] | per-process A/B min…max % | t_mi/t_sefer | A/B A ticks/call [CI] | A/B B ticks/call [CI] | Admitted | Verdict |");
  console.log("|---|---|---|---|---|---:|---|---|---|---|");
  for (const r of rows) {
    const f = (s) => `${pct(s.m)} [${pct(s.lo)}, ${pct(s.hi)}]`;
    const ratioStr = r.ab.ratio.toFixed(4);
    if (Math.exp(r.ab.m).toFixed(4) !== ratioStr)
      throw new Error(`${r.kernel}: ratio ${ratioStr} != exp(median) ${Math.exp(r.ab.m).toFixed(4)}`);
    console.log(`| ${r.kernel} | ${f(r.aa_sefer)} | ${f(r.aa_mi)} | ${f(r.ab)} | ${pct(r.ab.pmin)}…${pct(r.ab.pmax)} | ${ratioStr} | ${r.ab.a.toFixed(2)} [${r.ab.aLo.toFixed(2)}, ${r.ab.aHi.toFixed(2)}] | ${r.ab.b.toFixed(2)} [${r.ab.bLo.toFixed(2)}, ${r.ab.bHi.toFixed(2)}] | ${r.admitted ? "yes" : "NO"} | ${r.verdict} |`);
  }
  const nf = rows.filter((r) => r.verdict === "MIMALLOC_FASTER").length;
  const sf = rows.filter((r) => r.verdict === "SEFER_FASTER").length;
  console.log("");
  console.log(`# verdict summary: MIMALLOC_FASTER ${nf} / SEFER_FASTER ${sf} / ${rows.length} kernels; not admitted: ${rows.filter((r) => !r.admitted).length} / ${rows.length}`);

  const CSV_HEAD = ["kernel", "reps", "blocks_per_process", "admitted", "verdict",
    "aa_sefer_median", "aa_sefer_ci_lo", "aa_sefer_ci_hi",
    "aa_mi_median", "aa_mi_ci_lo", "aa_mi_ci_hi",
    "ab_median", "ab_ci_lo", "ab_ci_hi", "ab_process_min", "ab_process_max",
    "ratio_t_mi_over_t_sefer",
    "ab_a_ticks_per_call", "ab_a_ticks_ci_lo", "ab_a_ticks_ci_hi", "ab_b_ticks_per_call", "ab_b_ticks_ci_lo", "ab_b_ticks_ci_hi",
    "bootstrap", "seed", "order_seeds_note", "head_sha", "source_diff_sha256", "exe_sha256", "cpu"];
  const csvRows = rows.map((r) => [r.kernel, args.reps, id0.blocks, r.admitted ? "yes" : "no", r.verdict,
    r.aa_sefer.m, r.aa_sefer.lo, r.aa_sefer.hi,
    r.aa_mi.m, r.aa_mi.lo, r.aa_mi.hi,
    r.ab.m, r.ab.lo, r.ab.hi, r.ab.pmin, r.ab.pmax, r.ab.ratio,
    r.ab.a, r.ab.aLo, r.ab.aHi, r.ab.b, r.ab.bLo, r.ab.bHi,
    args.bootstrap, args.seed, `"distinct per process"`, id0.head_sha, id0.source_diff_sha256, id0.exe_sha256, `"${id0.cpu}"`].join(","));
  if (args.csv) {
    fs.mkdirSync(path.dirname(args.csv), { recursive: true });
    fs.writeFileSync(args.csv, [CSV_HEAD.join(","), ...csvRows].join("\n") + "\n");
    console.log(`# wrote ${args.csv}`);
  }
}

try {
  main();
} catch (e) {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
}
