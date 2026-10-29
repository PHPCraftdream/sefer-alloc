#!/usr/bin/env node
// G1-K7 Addendum 6, run 1 — ablation effects of v5 and of the layout
// control U, net of the null copy, pooled over two passes.
//
// Input: one or two pass directories written by scripts/b4_ablate_run.mjs
// (`<variant>_ab_variant_r<i>.jsonl`, i in 0..R-1, null included). Per
// process and kernel the statistic of scripts/b4_ablate_vs_null.mjs: median
// over blocks of ln((b1+b2)/(a1+a2)). Per variant set the processes of all
// passes are pooled:
//   E_v = median(v) − median(null),  E_u = median(u) − median(null),
//   D   = median(v) − median(u).
// 95 % percentile CIs by a process bootstrap stratified by pass (each pass's
// processes resampled within that pass, then pooled), all three contrasts
// from the same draws. Units: 100·ln (printed as %). The null precision label
// (CI half-width of median(null) <= 0.01) is reported; it changes no rule.
// Checks as in b4_ablate_vs_null.mjs: header, identity within a set, distinct
// order seeds, ABBA = BAAB, zero foreign/harness calls, exact oracle.
//
// Usage: node scripts/g1_k7_a6_ablation.mjs --pass <dir> [--pass <dir>]
//   --variant <name> [--control <name>] --reps <R> --kernels <csv>
//   [--csv <path>] [--bootstrap 10000] [--seed 0x5EED]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const args = { passes: [], bootstrap: 10000, seed: 0x5eed };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  const val = () => {
    const v = process.argv[++i];
    if (v === undefined || v.startsWith("--")) throw new Error(`${a} needs a value`);
    return v;
  };
  if (a === "--pass") args.passes.push(val());
  else if (a === "--variant") args.variant = val();
  else if (a === "--control") args.control = val();
  else if (a === "--reps") args.reps = Number(val());
  else if (a === "--kernels") args.kernels = val().split(",").map((s) => s.trim()).filter(Boolean);
  else if (a === "--csv") args.csv = val();
  else if (a === "--bootstrap") args.bootstrap = Number(val());
  else if (a === "--seed") args.seed = Number(val());
  else throw new Error(`unknown argument ${a}`);
}
if (!args.passes.length || !args.variant || !args.kernels?.length)
  throw new Error("--pass, --variant, --kernels are required");
if (!Number.isInteger(args.reps) || args.reps < 3) throw new Error("--reps must be an integer >= 3");

const median = (xs) => {
  const s = [...xs].sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[n >> 1] : (s[n / 2 - 1] + s[n / 2]) / 2;
};

let id0 = null;
// Per-process statistics of one variant in one pass: { kernel: [x per process] }.
function load(dir, variant) {
  const per = {};
  const seeds = new Set();
  let vsha = null;
  for (let r = 0; r < args.reps; r++) {
    const p = path.join(dir, `${variant}_ab_variant_r${r}.jsonl`);
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

const sets = ["null", args.variant, ...(args.control ? [args.control] : [])];
// data[set][pass] = { per, variant_sha256, exe_sha256 }
const data = Object.fromEntries(sets.map((v) => [v, args.passes.map((d) => load(d, v))]));
// The same variant must be the same build identity in every pass.
for (const v of sets)
  for (const x of data[v])
    if (x.variant_sha256 !== data[v][0].variant_sha256)
      throw new Error(`${v}: variant_sha256 differs between passes`);

const pooled = (v, k) => data[v].flatMap((x) => x.per[k]);

function ci(a) {
  a.sort((x, y) => x - y);
  return [a[Math.floor(0.025 * a.length)], a[Math.ceil(0.975 * a.length) - 1]];
}

function boot(k, seed) {
  let st = seed >>> 0;
  const rnd = () => ((st = (Math.imul(st, 1664525) + 1013904223) >>> 0) / 4294967296);
  const draw = (v) => data[v].flatMap((x) => x.per[k].map(() => x.per[k][Math.floor(rnd() * x.per[k].length)]));
  const ev = [], eu = [], d = [], nm = [];
  for (let b = 0; b < args.bootstrap; b++) {
    const n = median(draw("null"));
    const v = median(draw(args.variant));
    nm.push(n);
    ev.push(v - n);
    if (args.control) {
      const u = median(draw(args.control));
      eu.push(u - n);
      d.push(v - u);
    }
  }
  return { ev: ci(ev), eu: args.control ? ci(eu) : null, d: args.control ? ci(d) : null, n: ci(nm) };
}

const pct = (x) => (x * 100).toFixed(2);
const fmt = (e, c) => `${pct(e)} [${pct(c[0])}, ${pct(c[1])}]`;
console.log(
  `# G1-K7 A6 run 1 — variant=${args.variant} control=${args.control ?? "-"} passes=${args.passes.length} reps/pass=${args.reps} blocks/process=${id0.blocks} bootstrap=${args.bootstrap} seed=0x${args.seed.toString(16)}`,
);
console.log(`# identity: head=${id0.head_sha} sha(diff)=${id0.source_diff_sha256.slice(0, 12)}`);
for (const v of sets)
  console.log(`#   ${v}: variant_sha=${data[v][0].variant_sha256.slice(0, 12)} exe=${data[v].map((x) => x.exe_sha256.slice(0, 12)).join("/")}`);
console.log("# statistics: per process median over blocks of ln((b1+b2)/(a1+a2)); E_v = median(variant) − median(null), E_u = median(control) − median(null), D = median(variant) − median(control); pooled over passes, bootstrap stratified by pass");
console.log("");
console.log("| Kernel | E_v % [CI] | E_u % [CI] | D % [CI] | null median % [CI] | null precise | E_v loss (CI lo > 0) | layout exception (D lo <= 0, E_u lo > 0) |");
console.log("|---|---|---|---|---|---|---|---|");
const rows = [];
args.kernels.forEach((k, ki) => {
  const n = median(pooled("null", k));
  const v = median(pooled(args.variant, k));
  const ev = v - n;
  const b = boot(k, args.seed + 31 * ki);
  const precise = (b.n[1] - b.n[0]) / 2 <= 0.01;
  let eu = null, d = null;
  if (args.control) {
    const u = median(pooled(args.control, k));
    eu = u - n;
    d = v - u;
    if (Math.abs(ev - eu - d) > 1e-12) throw new Error(`${k}: E_v − E_u != D`);
  }
  const loss = b.ev[0] > 0;
  const exception = args.control ? b.d[0] <= 0 && b.eu[0] > 0 : false;
  rows.push([
    args.variant, args.control ?? "", k, args.passes.length, pooled(args.variant, k).length,
    ev, b.ev[0], b.ev[1], eu ?? "", b.eu?.[0] ?? "", b.eu?.[1] ?? "", d ?? "", b.d?.[0] ?? "", b.d?.[1] ?? "",
    n, b.n[0], b.n[1], precise ? "yes" : "no", loss ? "yes" : "no", exception ? "yes" : "no",
  ]);
  console.log(
    `| ${k} | ${fmt(ev, b.ev)} | ${args.control ? fmt(eu, b.eu) : "-"} | ${args.control ? fmt(d, b.d) : "-"} | ${fmt(n, b.n)} | ${precise ? "yes" : "no"} | ${loss ? "yes" : "no"} | ${exception ? "yes" : "no"} |`,
  );
});
if (args.csv) {
  const head =
    "variant,control,kernel,passes,processes,e_v,e_v_ci_lo,e_v_ci_hi,e_u,e_u_ci_lo,e_u_ci_hi,d,d_ci_lo,d_ci_hi,null_median,null_ci_lo,null_ci_hi,null_precise,e_v_loss,layout_exception";
  fs.mkdirSync(path.dirname(args.csv), { recursive: true });
  fs.writeFileSync(args.csv, [head, ...rows.map((r) => r.join(","))].join("\n") + "\n");
  console.log(`# wrote ${path.basename(args.csv)}`);
}
