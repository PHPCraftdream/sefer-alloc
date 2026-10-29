#!/usr/bin/env node
// B4 J1′ interleave judge — table + verdict generator (ONE script from raw).
//
// Takes THREE raw JSONL files in ONE invocation:
//   --aa-sefer <path>  A/A calibration sefer×sefer
//   --aa-mi <path>     A/A calibration mimalloc×mimalloc
//   --ab <path>        the judge: sefer vs mimalloc
// and emits ONE markdown table + ONE CSV. Per kernel:
//   - A/A sefer: median ln(tB/tA), bootstrap 95 % CI, half-width, trimmed mean;
//   - A/A mimalloc: median, CI, half-width, trimmed mean;
//   - A/B: median, CI, trimmed mean;
//   - admission = BOTH A/A modes pass the pre-registered threshold
//     (|median| ≤ 0.01 AND CI half-width ≤ 0.01);
//   - A/B verdict (A = sefer, B = mimalloc, ln(t_B/t_A)): MIMALLOC_FASTER
//     iff the UPPER 95 % CI bound < 0, SEFER_FASTER iff the LOWER bound > 0,
//     else EQUAL_CI_HAS_0 — issued ONLY for admitted kernels; not-admitted
//     kernels get «не допущена» and NO verdict.
// Bootstrap resamples must be ≥ 2000 (smaller values are an error).
//
// Completeness / integrity asserts over the raw data:
//   - identity: taken ONLY from the judge-written JSON header (first line,
//     `"header":true`); ALL identity fields are REQUIRED and asserted EQUAL
//     across the three raws — mode EXCEPTED, blocks INCLUDED;
//   - for every (mode, kernel): exactly the expected blocks 0..N−1 (N from
//     the header), no duplicates, no gaps;
//   - STRICT ABBA/BAAB balance per kernel: EXACTLY N/2 of each (N must be
//     even). The judge guarantees this by CONSTRUCTION (per-kernel list of
//     N/2 ABBA + N/2 BAAB, Fisher–Yates shuffled with the recorded
//     `order_seed`), so no tolerance is needed anywhere. NOTE (honest
//     history): in review round 1 a failed dry-run led to weakening this to
//     max(5 % of N/2, 4σ) — that weakening was ROLLED BACK; the strict
//     exact-by-construction check is restored here;
//   - all 11 kernels present in every mode; EVERY record's `kernel` must be
//     in the registry KERNEL_NAMES (unknown names are errors); the TOTAL
//     number of records in a raw is EXACTLY 11 × N;
//   - `order` is strictly "ABBA"|"BAAB"; a1/b1/b2/a2 are positive finite
//     safe integers; every `*_calls`/`*_exp`/`*_foreign` is a non-negative
//     safe integer; harness_allocs == 0;
//   - sha fields (head_sha, source_diff_sha256, exe_sha256) are 64-hex;
//   - window oracle: foreign-window shim calls == 0 for every portion;
//   - activation oracle, 100 % NO slack: for EVERY portion the observed
//     shim-call count must equal the judge-simulated `*_exp` field.
//     SOURCE OF EXPECTATIONS: ONLY the Rust simulation in the judge
//     (`simulate_portion_calls`, shared verbatim with the correctness test
//     via `include!`). The generator does NOT re-simulate on its own — an
//     earlier JS BigInt replay existed but duplicated (and once diverged
//     from) the Rust truth; its removal is deliberate. The Rust simulation's
//     exactness for INCOMPLETE / partially filled rings is proven by the
//     mutant test in `tests/b4_interleave_judge_correctness.rs` (same
//     function: sim == real tracked counters on every portion; a seed
//     shifted by 1 is guaranteed to diverge). What the generator DOES check
//     independently: each record's `seed` hex must equal the portion seed
//     derived from the header `seed` and the block number by the SAME
//     formula the judge uses (seed + block·0x1000000000000001, rotl64 by
//     the kernel's registry index) — BigInt port below (rotl64 only).
//
// Usage:
//   node scripts/b4_interleave_table.mjs --aa-sefer <raw.jsonl> \
//        --aa-mi <raw.jsonl> --ab <raw.jsonl> [--bootstrap 2000] [--seed 0x5EED]

import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import path from "node:path";

const args = {};
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  if (a === "--aa-sefer") args.aaSefer = process.argv[++i];
  else if (a === "--aa-mi") args.aaMi = process.argv[++i];
  else if (a === "--ab") args.ab = process.argv[++i];
  else if (a === "--bootstrap") args.bootstrap = Number(process.argv[++i]);
  else if (a === "--seed") args.seed = Number(process.argv[++i]);
  else throw new Error(`unknown arg ${a}`);
}
for (const [k, v] of [["--aa-sefer", args.aaSefer], ["--aa-mi", args.aaMi], ["--ab", args.ab]]) {
  if (!v) throw new Error(`${k} <raw.jsonl> is required (all THREE raws at once)`);
}

const BOOTSTRAP = args.bootstrap ?? 2000;
if (!Number.isInteger(BOOTSTRAP) || !Number.isFinite(BOOTSTRAP) || BOOTSTRAP < 2000)
  throw new Error(`bootstrap must be a finite integer >= 2000, got ${args.bootstrap}`);
const SEED = args.seed ?? 0x5eed;
// Pre-registered A/A thresholds (docs/perf/B4_INTERLEAVE_JUDGE.md — DO NOT
// change after measurements).
const AA_MEDIAN_MAX = 0.01; // |median ln-ratio| ≤ 1 %
const AA_HALFWIDTH_MAX = 0.01; // 95 % CI half-width ≤ 1 %

const KERNEL_NAMES = [
  "pair16", "pair64", "pair_rt", "bulk64", "bulk16k",
  "churn16", "churn64", "churn256", "churn1024", "vec512", "larson",
];

// ── mulberry32 (seeded bootstrap resampling) ────────────────────────────────
function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function median(sorted) {
  const n = sorted.length;
  if (n === 0) return NaN;
  return n % 2 ? sorted[(n - 1) / 2] : (sorted[n / 2 - 1] + sorted[n / 2]) / 2;
}

function trimmedMean(xs, frac = 0.1) {
  const s = [...xs].sort((a, b) => a - b);
  const k = Math.floor(s.length * frac);
  const core = s.slice(k, s.length - k);
  if (core.length === 0) return NaN;
  return core.reduce((x, y) => x + y, 0) / core.length;
}

function bootstrapCI(xs, resamples, seed) {
  const rng = mulberry32(seed);
  const n = xs.length;
  const meds = new Float64Array(resamples);
  for (let r = 0; r < resamples; r++) {
    const s = new Float64Array(n);
    for (let i = 0; i < n; i++) s[i] = xs[(rng() * n) | 0];
    s.sort();
    meds[r] = median(s);
  }
  meds.sort();
  const lo = meds[Math.floor((resamples - 1) * 0.025)];
  const hi = meds[Math.ceil((resamples - 1) * 0.975)];
  return { lo, hi, half: (hi - lo) / 2 };
}

// ── validation helpers ──────────────────────────────────────────────────────
function isHex64(s) {
  return typeof s === "string" && s.length === 64 && /^[0-9a-f]{64}$/.test(s);
}
function isHex40(s) {
  return typeof s === "string" && s.length === 40 && /^[0-9a-f]{40}$/.test(s);
}
function posSafeInt(v) {
  return Number.isSafeInteger(v) && v > 0;
}
function nonNegSafeInt(v) {
  return Number.isSafeInteger(v) && v >= 0;
}

// ── read one raw ────────────────────────────────────────────────────────────
function readRaw(p, expectedMode) {
  const lines = readFileSync(p, "utf8").split(/\r?\n/).filter(Boolean);
  const headerLine = lines[0];
  if (!headerLine.startsWith("{")) throw new Error(`${p}: first line is not the JSON header`);
  const header = JSON.parse(headerLine);
  if (header.header !== true) throw new Error(`${p}: first line has no "header":true`);
  if (header.mode !== expectedMode)
    throw new Error(`${p}: header mode ${header.mode} != expected ${expectedMode}`);
  const recs = [];
  for (const l of lines.slice(1)) {
    if (!l.startsWith("{")) throw new Error(`${p}: non-JSON line after header: ${l.slice(0, 40)}`);
    const r = JSON.parse(l);
    if (r.mode !== expectedMode) throw new Error(`${p}: record mode ${r.mode} != ${expectedMode}`);
    recs.push(r);
  }
  if (recs.length === 0) throw new Error(`${p}: no block records`);
  return { path: p, header, recs };
}

const raws = {
  aa_sefer: readRaw(args.aaSefer, "aa_sefer"),
  aa_mi: readRaw(args.aaMi, "aa_mi"),
  ab: readRaw(args.ab, "ab"),
};

// ── identity: from the headers only; ALL fields required; equal across raws
// (mode EXCEPTED, blocks INCLUDED) ───────────────────────────────────────────
const IDENTITY_FIELDS = [
  "blocks", "seed", "order_seed", "head_sha", "cpu", "os", "os_version",
  "source_diff_sha256", "exe_sha256", "features", "profile", "rustc",
  "tsc_invariant", "qpc_freq", "pair_ops", "bulk64_n", "bulk64_passes",
  "bulk16k_n", "bulk16k_passes", "churn_live", "churn_ops", "churn1024_ops",
  "vec_elems", "vec_cycles", "larson_live", "larson_ops", "larson_min",
  "larson_max", "larson_step", "rt_min", "rt_max",
  "rt_granule", "portion_seed_g", "larson_align", "vec_align", "churn_align",
  "pair_align", "warmup_portions", "kernels",
];
const id0 = raws.aa_sefer.header;
for (const f of IDENTITY_FIELDS) {
  if (id0[f] === undefined)
    throw new Error(`aa_sefer header: REQUIRED identity field ${f} is MISSING`);
  for (const m of ["aa_mi", "ab"]) {
    const h = raws[m].header;
    if (h[f] === undefined)
      throw new Error(`${m} header: REQUIRED identity field ${f} is MISSING`);
    if (JSON.stringify(id0[f]) !== JSON.stringify(h[f]))
      throw new Error(`identity MISMATCH on ${f}: aa_sefer=${JSON.stringify(id0[f])} ${m}=${JSON.stringify(h[f])}`);
  }
}
for (const f of ["source_diff_sha256", "exe_sha256"]) {
  if (!isHex64(id0[f])) throw new Error(`identity ${f} must be 64 lowercase hex, got ${id0[f]}`);
}
if (!isHex40(id0.head_sha))
  throw new Error(`identity head_sha must be 40 lowercase hex (git sha), got ${id0.head_sha}`);
if (!id0.cpu || typeof id0.cpu !== "string") throw new Error("identity cpu must be a non-empty string");
if (!id0.os || typeof id0.os !== "string") throw new Error("identity os must be a non-empty string");
// os_version: `NN.N.NNNN` (Windows, parsed from `cmd /c ver` by the runner)
// or the literal `unknown` (non-Windows; documented choice).
if (id0.os_version !== "unknown" && !/^\d+\.\d+\.\d+$/.test(id0.os_version))
  throw new Error(`identity os_version must be "unknown" or "NN.N.NNNN", got ${JSON.stringify(id0.os_version)}`);
for (const f of ["larson_align", "vec_align", "churn_align", "pair_align", "rt_granule"])
  if (!posSafeInt(id0[f])) throw new Error(`identity ${f} must be a positive safe integer, got ${id0[f]}`);
if (!Number.isSafeInteger(id0.rt_min) || !Number.isSafeInteger(id0.rt_max))
  throw new Error("identity rt_min/rt_max must be safe integers");
if (id0.profile !== "release" && id0.profile !== "bench-user")
  throw new Error(`identity profile must be release|bench-user, got ${id0.profile}`);
for (const f of ["seed", "order_seed", "portion_seed_g"]) {
  if (typeof id0[f] !== "string" || !/^0x[0-9a-f]{1,16}$/.test(id0[f]))
    throw new Error(`identity ${f} must be a hex string "0x...", got ${JSON.stringify(id0[f])}`);
}
const BLOCKS = id0.blocks;
if (!Number.isSafeInteger(BLOCKS) || BLOCKS <= 0) throw new Error(`header blocks invalid: ${BLOCKS}`);
if (BLOCKS % 2 !== 0) throw new Error(`header blocks must be EVEN (strict balance), got ${BLOCKS}`);

// ── BigInt port of the judge's PORTION-SEED FORMULA (rotl64 only — the
// simulation itself lives ONLY in Rust, see the header comment) ────────────
const MASK64 = (1n << 64n) - 1n;

function rotl64(x, k) {
  const kk = BigInt(k);
  return ((x << kk) | (x >> (64n - kk))) & MASK64;
}

// Same formula as the judge: `seed + block*0x1000_0000_0000_0001`, then
// rotate_left by the kernel's index in ALL_KERNELS (= KERNEL_NAMES order).
function portionSeed(headerSeedBig, block, ki) {
  return rotl64((headerSeedBig + BigInt(block) * 0x1000000000000001n) & MASK64, ki);
}

const HSEED = BigInt(id0.seed);

// Analytic per-portion shim-call expectations (mirror of the Rust
// `Kernel::expected_shim_calls`, from the SAME header constants).
const vecPerCycle = 1 + Math.log2(id0.vec_elems) + 1;
const EXPECTED_CALLS = {
  pair16: 2 * id0.pair_ops,
  pair64: 2 * id0.pair_ops,
  pair_rt: 2 * id0.pair_ops,
  bulk64: 2 * id0.bulk64_n * id0.bulk64_passes,
  bulk16k: 2 * id0.bulk16k_n * id0.bulk16k_passes,
  churn16: 2 * id0.churn_ops,
  churn64: 2 * id0.churn_ops,
  churn256: 2 * id0.churn_ops,
  churn1024: 2 * id0.churn1024_ops,
  vec512: vecPerCycle * id0.vec_cycles,
  larson: 2 * id0.larson_ops,
};
for (const kn of Object.keys(EXPECTED_CALLS)) {
  if (!Number.isSafeInteger(EXPECTED_CALLS[kn]))
    throw new Error(`analytic expectation for ${kn} is not a safe integer`);
}

// ── per-raw completeness asserts ────────────────────────────────────────────
function checkRaw(m, { header, recs, path: p }) {
  const n = header.blocks;
  if (recs.length !== 11 * n)
    throw new Error(`${p} (${m}): total records ${recs.length} != 11 × N = ${11 * n}`);
  for (const kn of KERNEL_NAMES) {
    const rs = recs.filter((r) => r.kernel === kn);
    if (rs.length !== n)
      throw new Error(`${p} / ${kn}: ${rs.length} blocks, header N=${n}`);
    const seen = new Set();
    for (const r of rs) {
      if (!Number.isInteger(r.block) || r.block < 0 || r.block >= n)
        throw new Error(`${p} / ${kn}: block index out of range: ${r.block}`);
      if (seen.has(r.block)) throw new Error(`${p} / ${kn}: duplicate block ${r.block}`);
      seen.add(r.block);
      // Field shapes: strict.
      if (r.order !== "ABBA" && r.order !== "BAAB")
        throw new Error(`${p} / ${kn} block ${r.block}: order must be "ABBA"|"BAAB", got ${JSON.stringify(r.order)}`);
      if (typeof r.seed !== "string" || !/^0x[0-9a-f]{1,16}$/.test(r.seed))
        throw new Error(`${p} / ${kn} block ${r.block}: seed must be a hex string, got ${JSON.stringify(r.seed)}`);
      // Registry check (defensive; unknown kernels are errors).
      if (!KERNEL_NAMES.includes(r.kernel))
        throw new Error(`${p} / block ${r.block}: unknown kernel ${JSON.stringify(r.kernel)} (not in KERNEL_NAMES)`);
      // Seed formula (BigInt, rotl64): same derivation as the judge.
      const ki = KERNEL_NAMES.indexOf(kn);
      const wantSeed = portionSeed(HSEED, r.block, ki);
      if (BigInt(r.seed) !== wantSeed)
        throw new Error(`${p} / ${kn} block ${r.block}: seed ${r.seed} != portion_seed formula ${"0x" + wantSeed.toString(16)}`);
      for (const f of ["a1", "b1", "b2", "a2"])
        if (!posSafeInt(r[f]))
          throw new Error(`${p} / ${kn} block ${r.block}: ${f} must be a positive safe integer, got ${r[f]}`);
      for (const f of [
        "a1_calls", "b1_calls", "b2_calls", "a2_calls",
        "a1_exp", "b1_exp", "b2_exp", "a2_exp",
        "a1_foreign", "b1_foreign", "b2_foreign", "a2_foreign", "harness_allocs",
      ])
        if (!nonNegSafeInt(r[f]))
          throw new Error(`${p} / ${kn} block ${r.block}: ${f} must be a non-negative safe integer, got ${r[f]}`);
    }
    for (let b = 0; b < n; b++)
      if (!seen.has(b)) throw new Error(`${p} / ${kn}: missing block ${b}`);
    // STRICT ABBA/BAAB balance per kernel: exactly N/2 of each, guaranteed
    // by construction in the judge (recorded order_seed). NO tolerance.
    const abba = rs.filter((r) => r.order === "ABBA").length;
    const baab = rs.length - abba;
    if (abba !== n / 2 || baab !== n / 2)
      throw new Error(
        `${p} / ${kn}: STRICT order balance FAILED: ABBA=${abba} BAAB=${baab}, expected exactly ${n / 2}+${n / 2} of N=${n}`
      );
    // Window + activation oracles, per record / per portion: 100 %, no slack.
    for (const r of rs) {
      if (r.harness_allocs !== 0)
        throw new Error(`${p} / ${kn} block ${r.block}: harness_allocs=${r.harness_allocs}`);
      for (const f of ["a1_foreign", "b1_foreign", "b2_foreign", "a2_foreign"])
        if (r[f] !== 0)
          throw new Error(`${p} / ${kn} block ${r.block}: ${f}=${r[f]} (foreign shim calls)`);
      // (a) judge-simulated expectation, all kernels, every portion.
      for (const [cf, ef] of [
        ["a1_calls", "a1_exp"], ["b1_calls", "b1_exp"],
        ["b2_calls", "b2_exp"], ["a2_calls", "a2_exp"],
      ])
        if (r[cf] !== r[ef])
          throw new Error(
            `${p} / ${kn} block ${r.block}: ${cf}=${r[cf]} != judge expectation ${ef}=${r[ef]}`
          );
    }
  }
}
for (const m of Object.keys(raws)) {
  checkRaw(m, raws[m]);
}

// ── stats per kernel per mode ───────────────────────────────────────────────
function ratios(rs) {
  return rs.map((r) => Math.log((r.b1 + r.b2) / (r.a1 + r.a2)));
}

function kernelStats(rs) {
  const xs = ratios(rs);
  const sorted = [...xs].sort((a, b) => a - b);
  const med = median(sorted);
  const ci = bootstrapCI(xs, BOOTSTRAP, SEED);
  return { n: rs.length, med, ci, tm: trimmedMean(xs) };
}

const stats = KERNEL_NAMES.map((name) => {
  const aaSefer = kernelStats(raws.aa_sefer.recs.filter((r) => r.kernel === name));
  const aaMi = kernelStats(raws.aa_mi.recs.filter((r) => r.kernel === name));
  const ab = kernelStats(raws.ab.recs.filter((r) => r.kernel === name));
  const aaPass = (s) => Math.abs(s.med) <= AA_MEDIAN_MAX && s.ci.half <= AA_HALFWIDTH_MAX;
  const admitted = aaPass(aaSefer) && aaPass(aaMi);
  // A = sefer (shim table 0), B = mimalloc (table 2); ln(t_B/t_A) < 0 ⇒ B faster.
  const verdict = !admitted
    ? "не допущена"
    : ab.ci.hi < 0
      ? "MIMALLOC_FASTER"
      : ab.ci.lo > 0
        ? "SEFER_FASTER"
        : "EQUAL_CI_HAS_0";
  return { name, aaSefer, aaMi, ab, admitted, verdict };
});

// ── output ──────────────────────────────────────────────────────────────────
console.log(`# B4 INTERLEAVE JUDGE — blocks/mode: aa_sefer=${raws.aa_sefer.header.blocks} aa_mi=${raws.aa_mi.header.blocks} ab=${raws.ab.header.blocks} bootstrap=${BOOTSTRAP} seed=${SEED}`);
console.log(`# identity (from judge headers): head=${id0.head_sha} sha(diff)=${id0.source_diff_sha256.slice(0, 12)} exe=${id0.exe_sha256.slice(0, 12)} profile=${id0.profile} rustc="${id0.rustc}" cpu="${id0.cpu}" os="${id0.os}"`);
console.log(`# pre-registered A/A threshold: |median| <= ${AA_MEDIAN_MAX}, CI half-width <= ${AA_HALFWIDTH_MAX}; A = sefer, B = mimalloc, ratio = t_mimalloc / t_sefer; verdict (admitted kernels only): MIMALLOC_FASTER iff upper 95 % CI of ln(t_B/t_A) < 0, SEFER_FASTER iff lower 95 % CI > 0, else EQUAL_CI_HAS_0`);
console.log(`# order balance: STRICT (ABBA = BAAB = N/2 per kernel, exact by construction, order_seed=${id0.order_seed}); activation oracle: 100 % exact`);
console.log("");
console.log("| Kernel | A/A sefer median | CI | half | trimmed | A/A mi median | CI | half | trimmed | A/B median | CI | trimmed | ratio | Допущена | Вердикт A/B |");
console.log("|---|---:|---|---:|---:|---:|---|---:|---:|---:|---|---:|---:|---|---|");
for (const s of stats) {
  const f = (x) => x.toFixed(5);
  console.log(
    `| ${s.name} | ${f(s.aaSefer.med)} | ${f(s.aaSefer.ci.lo)} … ${f(s.aaSefer.ci.hi)} | ${f(s.aaSefer.ci.half)} | ${f(s.aaSefer.tm)}`
    + ` | ${f(s.aaMi.med)} | ${f(s.aaMi.ci.lo)} … ${f(s.aaMi.ci.hi)} | ${f(s.aaMi.ci.half)} | ${f(s.aaMi.tm)}`
    + ` | ${f(s.ab.med)} | ${f(s.ab.ci.lo)} … ${f(s.ab.ci.hi)} | ${f(s.ab.tm)} | ${Math.exp(s.ab.med).toFixed(4)}`
    + ` | ${s.admitted ? "да" : "НЕТ"} | ${s.verdict} |`
  );
}
console.log("");
const miFaster = stats.filter((s) => s.verdict === "MIMALLOC_FASTER").length;
const seferFaster = stats.filter((s) => s.verdict === "SEFER_FASTER").length;
console.log(
  `# verdict summary: MIMALLOC_FASTER ${miFaster} / SEFER_FASTER ${seferFaster} / ${stats.length} kernels; not admitted: ${stats.filter((s) => !s.admitted).length} / ${stats.length}`
  + ` (a kernel is admitted iff BOTH A/A calibrations pass the pre-registered threshold)`
);

const csvDir = "docs/perf";
mkdirSync(csvDir, { recursive: true });
const csvPath = path.join(csvDir, "B4_INTERLEAVE_JUDGE_summary.csv");
const id = id0;
const rows = [
  ["kernel", "aa_sefer_median", "aa_sefer_ci_lo", "aa_sefer_ci_hi", "aa_sefer_half", "aa_sefer_trimmed_mean",
   "aa_mi_median", "aa_mi_ci_lo", "aa_mi_ci_hi", "aa_mi_half", "aa_mi_trimmed_mean",
   "ab_median", "ab_ci_lo", "ab_ci_hi", "ab_ci_half", "ab_trimmed_mean", "ab_ratio",
   "admitted", "ab_verdict",
   "blocks_aa_sefer", "blocks_aa_mi", "blocks_ab", "bootstrap", "seed", "order_seed",
   "head_sha", "cpu", "os", "os_version",
   "source_diff_sha256", "exe_sha256", "features", "profile", "rustc", "tsc_invariant", "qpc_freq"],
  ...stats.map((s) => [
    s.name,
    s.aaSefer.med.toFixed(6), s.aaSefer.ci.lo.toFixed(6), s.aaSefer.ci.hi.toFixed(6), s.aaSefer.ci.half.toFixed(6), s.aaSefer.tm.toFixed(6),
    s.aaMi.med.toFixed(6), s.aaMi.ci.lo.toFixed(6), s.aaMi.ci.hi.toFixed(6), s.aaMi.ci.half.toFixed(6), s.aaMi.tm.toFixed(6),
    s.ab.med.toFixed(6), s.ab.ci.lo.toFixed(6), s.ab.ci.hi.toFixed(6), s.ab.ci.half.toFixed(6),
    s.ab.tm.toFixed(6), Math.exp(s.ab.med).toFixed(6),
    s.admitted ? "yes" : "no", s.verdict,
    raws.aa_sefer.header.blocks, raws.aa_mi.header.blocks, raws.ab.header.blocks,
    String(BOOTSTRAP), String(SEED), id.order_seed,
    id.head_sha, `"${id.cpu}"`, `"${id.os}"`, `"${id.os_version}"`,
    id.source_diff_sha256, id.exe_sha256, `"${id.features}"`, id.profile, `"${id.rustc}"`,
    id.tsc_invariant, id.qpc_freq,
  ]),
];
writeFileSync(csvPath, rows.map((r) => r.join(",")).join("\n") + "\n");
console.log(`# wrote ${csvPath}`);
