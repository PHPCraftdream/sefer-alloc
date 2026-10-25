#!/usr/bin/env node
// B4-ablate — variant effect NET of the null copy (difference in medians).
//
// Why: the A/A sefer×sefer calibration carries a systematic slot bias of
// about -0.5…-1.1 % on churn (measured over 20 processes, same code in both
// slots), so a variant's raw A/B median mixes that bias with the variant's
// effect. The null variant (copy == original) runs the SAME A/B slot layout
// on its own build; subtracting its median removes the shared bias.
//
// Input: <dir>/<variant>_ab_variant_r<i>.jsonl and <dir>/null_ab_variant_r<i>.jsonl,
// i in 0..R-1. Per process and kernel: median over blocks of
// ln((b1+b2)/(a1+a2)). Effect = median over variant processes − median over
// null processes; 95 % CI by an independent percentile bootstrap of both
// process sets (2000 resamples, deterministic PRNG). Precision gate: null
// CI half-width <= 0.01 per kernel (else "imprecise"). Checks: header mode,
// head_sha / source_diff_sha256 equal across all raws, order seeds distinct
// within a set, exact ABBA = BAAB balance, zero foreign/harness calls, exact
// oracle.
//
// Usage: node scripts/b4_ablate_vs_null.mjs --dir <dir> --variant <name>
//   --reps <R> --kernels <csv> [--csv <path>] [--bootstrap 2000] [--seed 0x5EED]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const args = { bootstrap: 2000, seed: 0x5eed };
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
  else if (a === "--bootstrap") args.bootstrap = Number(val());
  else if (a === "--seed") args.seed = Number(val());
  else throw new Error(`unknown argument ${a}`);
}
if (!args.dir || !args.variant || !args.kernels?.length) throw new Error("--dir, --variant, --kernels are required");
if (args.variant === "null") throw new Error("--variant must not be null");
if (!Number.isInteger(args.reps) || args.reps < 3) throw new Error("--reps must be an integer >= 3");

const median = (xs) => {
  const s = [...xs].sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[n >> 1] : (s[n / 2 - 1] + s[n / 2]) / 2;
};

let id0 = null;
function load(variant) {
  const per = {};
  const seeds = new Set();
  let vsha = null;
  for (let r = 0; r < args.reps; r++) {
    const p = path.join(args.dir, `${variant}_ab_variant_r${r}.jsonl`);
    const lines = fs.readFileSync(p, "utf8").split("\n").filter(Boolean);
    const h = JSON.parse(lines[0]);
    if (h.header !== true || h.mode !== "ab_variant") throw new Error(`${p}: bad header`);
    if (id0 === null) id0 = h;
    for (const f of ["head_sha", "source_diff_sha256", "cpu", "blocks", "profile", "rustc"])
      if (h[f] !== id0[f]) throw new Error(`${p}: ${f} differs`);
    if (vsha === null) vsha = { variant_sha256: h.variant_sha256, exe_sha256: h.exe_sha256 };
    if (h.variant_sha256 !== vsha.variant_sha256 || h.exe_sha256 !== vsha.exe_sha256)
      throw new Error(`${p}: variant/exe identity differs within the set`);
    if (h.variant_name !== variant) throw new Error(`${p}: variant_name ${h.variant_name}`);
    if (seeds.has(h.order_seed)) throw new Error(`${p}: duplicate order_seed`);
    seeds.add(h.order_seed);
    const byK = {};
    for (const l of lines.slice(1)) {
      const x = JSON.parse(l);
      if (x.harness_allocs !== 0) throw new Error(`${p}: harness_allocs`);
      for (const q of ["a1", "b1", "b2", "a2"]) {
        if (x[`${q}_foreign`] !== 0) throw new Error(`${p}: foreign calls`);
        if (x[`${q}_calls`] !== x[`${q}_exp`]) throw new Error(`${p}: oracle mismatch`);
      }
      (byK[x.kernel] ??= []).push(x);
    }
    for (const k of args.kernels) {
      const recs = byK[k];
      if (!recs || recs.length !== h.blocks) throw new Error(`${p} ${k}: block count`);
      const abba = recs.filter((x) => x.order === "ABBA").length;
      if (abba * 2 !== h.blocks) throw new Error(`${p} ${k}: ABBA=${abba}`);
      (per[k] ??= []).push(median(recs.map((x) => Math.log((x.b1 + x.b2) / (x.a1 + x.a2)))));
    }
  }
  return { per, ...vsha };
}

const V = load(args.variant);
const N = load("null");

function boot(xs, ys, seed) {
  let st = seed >>> 0;
  const rnd = () => ((st = (Math.imul(st, 1664525) + 1013904223) >>> 0) / 4294967296);
  const pick = (a) => a.map(() => a[Math.floor(rnd() * a.length)]);
  const d = [], n = [];
  for (let b = 0; b < args.bootstrap; b++) {
    const nm = median(pick(ys));
    d.push(median(pick(xs)) - nm);
    n.push(nm);
  }
  const ci = (a) => {
    a.sort((x, y) => x - y);
    return [a[Math.floor(0.025 * a.length)], a[Math.ceil(0.975 * a.length) - 1]];
  };
  return { d: ci(d), n: ci(n) };
}

const pct = (x) => (x * 100).toFixed(2);
console.log(`# B4 ABLATE — variant net of null: variant=${args.variant} reps=${args.reps} blocks/process=${id0.blocks} bootstrap=${args.bootstrap}`);
console.log(`# identity: head=${id0.head_sha} sha(diff)=${id0.source_diff_sha256.slice(0, 12)} variant_sha=${V.variant_sha256.slice(0, 12)} exe=${V.exe_sha256.slice(0, 12)} null_exe=${N.exe_sha256.slice(0, 12)}`);
console.log("# effect = median_processes ln(t_variant/t_sefer) − median_processes ln(t_null/t_sefer); NET_FASTER iff CI upper < 0, NET_SLOWER iff CI lower > 0");
console.log("");
console.log("| Kernel | variant A/B % | null A/B % [CI] | effect % [CI] | ratio exp(effect) | precise | verdict |");
console.log("|---|---:|---|---|---:|---|---|");
const rows = [];
args.kernels.forEach((k, ki) => {
  const vm = median(V.per[k]), nm = median(N.per[k]);
  const eff = vm - nm;
  const b = boot(V.per[k], N.per[k], args.seed + 31 * ki);
  const precise = (b.n[1] - b.n[0]) / 2 <= 0.01;
  const ratio = Math.exp(eff);
  if (Math.abs(Math.log(ratio) - eff) > 1e-12) throw new Error("ratio != exp(effect)");
  const verdict = !precise ? "imprecise" : b.d[1] < 0 ? "NET_FASTER" : b.d[0] > 0 ? "NET_SLOWER" : "EQUAL_CI_HAS_0";
  rows.push([args.variant, k, args.reps, vm, nm, b.n[0], b.n[1], eff, b.d[0], b.d[1], ratio, precise ? "yes" : "no", verdict]);
  console.log(`| ${k} | ${pct(vm)} | ${pct(nm)} [${pct(b.n[0])}, ${pct(b.n[1])}] | ${pct(eff)} [${pct(b.d[0])}, ${pct(b.d[1])}] | ${ratio.toFixed(4)} | ${precise ? "yes" : "NO"} | ${verdict} |`);
});
if (args.csv) {
  const head = "variant,kernel,reps,variant_ab_median,null_ab_median,null_ci_lo,null_ci_hi,effect,effect_ci_lo,effect_ci_hi,ratio,precise,verdict";
  fs.mkdirSync(path.dirname(args.csv), { recursive: true });
  fs.writeFileSync(args.csv, [head, ...rows.map((r) => r.join(","))].join("\n") + "\n");
  console.log(`# wrote ${path.basename(args.csv)}`);
}
