#!/usr/bin/env node
// G1-K7 Addendum 6, run 2 — the main judge over a balanced series of runs of
// two trees, and the speed decision of Addendum 6 rules 1–2.
//
// Input: run directories of scripts/b4_interleave_run.mjs, each labelled with
// its tree (`--run <dir>:<K7|K5>`, in execution order). A directory holds the
// per-process raws ab_r<i>.jsonl (i in 0..R-1) and the judge's own
// B4_INTERLEAVE_JUDGE_rep_summary.csv (copied there by the campaign script),
// whose `admitted` column is the per-(run, kernel) admission (A/A sefer and
// A/A mimalloc |median| <= 1 % and CI half-width <= 1 %, unchanged).
//
// Per process and kernel: median over blocks of ln((b1+b2)/(a1+a2)), arm a =
// sefer, arm b = mimalloc, i.e. ln(t_mi/t_sefer). Per run: s = median over
// processes (asserted equal to the summary's ab_median). Per tree T: M_T =
// mean of s over T's admitted runs of that kernel; CI_T by a two-level
// bootstrap (runs with replacement, then the processes within each drawn
// run). A kernel is judgeable iff each tree has >= --min-admitted admitted
// runs. Win: CI_K7 entirely above CI_K5; loss: entirely below.
//
// With --ablation <csv> (scripts/g1_k7_a6_ablation.mjs, G11) and --gl <csv>
// (repeatable, GL1/GL2 without control) the script prints the speed decision:
//   rule 1: a judgeable run-2 win whose kernel has E_v and D CI upper < 0;
//   rule 2a: no G11 kernel with E_v CI lower > 0 unless layout_exception and
//            judgeable in run 2; no GL kernel with E_v CI lower > 0;
//   rule 2b: no judgeable run-2 loss; a non-judgeable kernel is decided by
//            run 1 alone, without the exception.
//
// Usage: node scripts/g1_k7_a6_judge.mjs --run <dir>:K7 --run <dir>:K5 ...
//   --reps 20 [--kernels <csv>] [--min-admitted 4] [--bootstrap 10000]
//   [--seed 0x5EED] [--csv <path>] [--ablation <csv>] [--gl <csv> ...]

import fs from "node:fs";
import path from "node:path";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const G11 = "pair16,pair64,pair_rt,bulk64,bulk16k,churn16,churn64,churn256,churn1024,vec512,larson";
const args = { runs: [], gl: [], bootstrap: 10000, seed: 0x5eed, minAdmitted: 4, kernels: G11.split(",") };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  const val = () => {
    const v = process.argv[++i];
    if (v === undefined || v.startsWith("--")) throw new Error(`${a} needs a value`);
    return v;
  };
  if (a === "--run") {
    const v = val();
    const k = v.lastIndexOf(":");
    const tree = v.slice(k + 1);
    if (k < 0 || !["K7", "K5"].includes(tree)) throw new Error(`--run ${v}: expected <dir>:K7|K5`);
    args.runs.push({ dir: v.slice(0, k), tree });
  } else if (a === "--reps") args.reps = Number(val());
  else if (a === "--kernels") args.kernels = val().split(",").map((s) => s.trim()).filter(Boolean);
  else if (a === "--min-admitted") args.minAdmitted = Number(val());
  else if (a === "--bootstrap") args.bootstrap = Number(val());
  else if (a === "--seed") args.seed = Number(val());
  else if (a === "--csv") args.csv = val();
  else if (a === "--ablation") args.ablation = val();
  else if (a === "--gl") args.gl.push(val());
  else throw new Error(`unknown argument ${a}`);
}
if (!Number.isInteger(args.reps) || args.reps < 3) throw new Error("--reps must be an integer >= 3");
for (const t of ["K7", "K5"])
  if (!args.runs.some((r) => r.tree === t)) throw new Error(`no run of tree ${t}`);

const median = (xs) => {
  const s = [...xs].sort((x, y) => x - y);
  const n = s.length;
  return n % 2 ? s[n >> 1] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const mean = (xs) => xs.reduce((a, b) => a + b, 0) / xs.length;

function readCsv(p) {
  const [head, ...lines] = fs.readFileSync(p, "utf8").split("\n").filter(Boolean);
  const cols = head.split(",");
  return lines.map((l) => {
    // The summary's trailing quoted fields hold no commas before `cpu`; split
    // on the first cols.length-1 commas only.
    const out = [];
    let rest = l;
    for (let c = 0; c < cols.length - 1; c++) {
      const q = rest.startsWith('"');
      const end = q ? rest.indexOf('",') + 1 : rest.indexOf(",");
      out.push(rest.slice(q ? 1 : 0, q ? end - 1 : end));
      rest = rest.slice(end + 1);
    }
    out.push(rest.replace(/^"|"$/g, ""));
    return Object.fromEntries(cols.map((c, i) => [c, out[i]]));
  });
}

// Per run: identity, per-kernel process statistics, admission.
const exeOf = {};
for (const r of args.runs) {
  r.per = {};
  r.seeds = new Set();
  for (let i = 0; i < args.reps; i++) {
    const p = path.join(r.dir, `ab_r${i}.jsonl`);
    const lines = fs.readFileSync(p, "utf8").split("\n").filter(Boolean);
    const h = JSON.parse(lines[0]);
    if (h.header !== true || h.mode !== "ab") throw new Error(`${p}: bad header`);
    r.exe ??= h.exe_sha256;
    if (h.exe_sha256 !== r.exe) throw new Error(`${p}: exe differs within the run`);
    if (r.seeds.has(h.order_seed)) throw new Error(`${p}: duplicate order_seed`);
    r.seeds.add(h.order_seed);
    const byK = {};
    for (const l of lines.slice(1)) {
      const x = JSON.parse(l);
      if (x.mode !== "ab") continue;
      (byK[x.kernel] ??= []).push(Math.log((x.b1 + x.b2) / (x.a1 + x.a2)));
    }
    for (const k of args.kernels) {
      if (!byK[k]?.length) throw new Error(`${p}: no blocks for ${k}`);
      (r.per[k] ??= []).push(median(byK[k]));
    }
  }
  // One build per tree for the whole series.
  exeOf[r.tree] ??= r.exe;
  if (exeOf[r.tree] !== r.exe) throw new Error(`${r.dir}: ${r.tree} exe differs between runs`);
  const sum = readCsv(path.join(r.dir, "B4_INTERLEAVE_JUDGE_rep_summary.csv"));
  r.admitted = {};
  for (const k of args.kernels) {
    const row = sum.find((x) => x.kernel === k);
    if (!row) throw new Error(`${r.dir}: summary lacks ${k}`);
    r.admitted[k] = row.admitted === "yes";
    const s = median(r.per[k]);
    if (Math.abs(s - Number(row.ab_median)) > 1e-9)
      throw new Error(`${r.dir} ${k}: recomputed ${s} != summary ab_median ${row.ab_median}`);
  }
}
if (exeOf.K7 === exeOf.K5) throw new Error("K7 and K5 runs share one executable");

function ci(a) {
  a.sort((x, y) => x - y);
  return [a[Math.floor(0.025 * a.length)], a[Math.ceil(0.975 * a.length) - 1]];
}

function judge(k, ki) {
  const out = {};
  for (const [ti, t] of ["K7", "K5"].entries()) {
    const runs = args.runs.filter((r) => r.tree === t && r.admitted[k]);
    const s = runs.map((r) => median(r.per[k]));
    out[t] = { n: runs.length, total: args.runs.filter((r) => r.tree === t).length, m: runs.length ? mean(s) : NaN, ci: [NaN, NaN] };
    if (runs.length < 2) continue;
    let st = (args.seed + 101 * ki + 7 * ti) >>> 0;
    const rnd = () => ((st = (Math.imul(st, 1664525) + 1013904223) >>> 0) / 4294967296);
    const pick = (a) => a[Math.floor(rnd() * a.length)];
    const draws = [];
    for (let b = 0; b < args.bootstrap; b++) {
      let acc = 0;
      for (let j = 0; j < runs.length; j++) {
        const p = pick(runs).per[k];
        acc += median(p.map(() => pick(p)));
      }
      draws.push(acc / runs.length);
    }
    out[t].ci = ci(draws);
  }
  const judgeable = out.K7.n >= args.minAdmitted && out.K5.n >= args.minAdmitted;
  const win = judgeable && out.K7.ci[0] > out.K5.ci[1];
  const loss = judgeable && out.K7.ci[1] < out.K5.ci[0];
  return { ...out, judgeable, win, loss };
}

const r3 = (x) => (Number.isFinite(x) ? Math.exp(x).toFixed(3) : "-");
console.log(`# G1-K7 A6 run 2 — ${args.runs.length} runs (order ${args.runs.map((r) => r.tree).join(" ")}), reps/run=${args.reps}, bootstrap=${args.bootstrap}, seed=0x${args.seed.toString(16)}, judgeable iff >= ${args.minAdmitted} admitted runs per tree`);
console.log(`# exe K7=${exeOf.K7.slice(0, 12)} K5=${exeOf.K5.slice(0, 12)}`);
console.log("# statistic: per run s = median over processes of per-process median ln(t_mi/t_sefer); M_T = mean of s over admitted runs; CI two-level bootstrap (runs, then processes); ratio = exp(M) = t_mi/t_sefer, higher = sefer faster");
console.log("");
console.log("| Kernel | K7 admitted | K5 admitted | K7 ratio [CI] | K5 ratio [CI] | K7/K5 | judgeable | verdict |");
console.log("|---|---:|---:|---|---|---:|---|---|");
const res = {};
const rows = [];
args.kernels.forEach((k, ki) => {
  const j = judge(k, ki);
  res[k] = j;
  const rel = Math.exp(j.K7.m - j.K5.m);
  if (Number.isFinite(rel) && Math.abs(Math.log(rel) - (j.K7.m - j.K5.m)) > 1e-12) throw new Error("ratio arithmetic");
  const verdict = !j.judgeable ? "not judgeable" : j.win ? "K7_FASTER" : j.loss ? "K7_SLOWER" : "no detected difference";
  rows.push([k, j.K7.n, j.K7.total, j.K5.n, j.K5.total, j.K7.m, j.K7.ci[0], j.K7.ci[1], j.K5.m, j.K5.ci[0], j.K5.ci[1], rel, j.judgeable ? "yes" : "no", verdict]);
  console.log(`| ${k} | ${j.K7.n}/${j.K7.total} | ${j.K5.n}/${j.K5.total} | ${r3(j.K7.m)} [${r3(j.K7.ci[0])}, ${r3(j.K7.ci[1])}] | ${r3(j.K5.m)} [${r3(j.K5.ci[0])}, ${r3(j.K5.ci[1])}] | ${Number.isFinite(rel) ? rel.toFixed(4) : "-"} | ${j.judgeable ? "yes" : "no"} | ${verdict} |`);
});
console.log("");
console.log("Per run s (ratio), in execution order:");
for (const r of args.runs)
  console.log(`  ${r.tree} ${path.basename(r.dir)}: ${args.kernels.map((k) => `${k}=${r3(median(r.per[k]))}${r.admitted[k] ? "" : "*"}`).join(" ")}`);
console.log("  (* = not admitted)");
if (args.csv) {
  const head = "kernel,k7_admitted,k7_runs,k5_admitted,k5_runs,k7_mean_ln,k7_ci_lo,k7_ci_hi,k5_mean_ln,k5_ci_lo,k5_ci_hi,k7_over_k5,judgeable,verdict";
  fs.mkdirSync(path.dirname(args.csv), { recursive: true });
  fs.writeFileSync(args.csv, [head, ...rows.map((r) => r.join(","))].join("\n") + "\n");
  console.log(`# wrote ${path.basename(args.csv)}`);
}

if (args.ablation) {
  const ab = readCsv(args.ablation);
  const get = (k) => {
    const x = ab.find((y) => y.kernel === k);
    if (!x) throw new Error(`ablation csv lacks ${k}`);
    return x;
  };
  const gl = args.gl.flatMap(readCsv);
  console.log("");
  console.log("## Speed decision (Addendum 6 rules 1–2)");
  const wins = args.kernels.filter((k) => {
    const a = get(k);
    return res[k].win && Number(a.e_v_ci_hi) < 0 && a.d_ci_hi !== "" && Number(a.d_ci_hi) < 0;
  });
  const lossesR1 = args.kernels.filter((k) => {
    const a = get(k);
    if (!(Number(a.e_v_ci_lo) > 0)) return false;
    return !(a.layout_exception === "yes" && res[k].judgeable);
  });
  const lossesGL = gl.filter((x) => Number(x.e_v_ci_lo) > 0).map((x) => x.kernel);
  const lossesR2 = args.kernels.filter((k) => res[k].loss);
  const exempted = args.kernels.filter((k) => Number(get(k).e_v_ci_lo) > 0 && get(k).layout_exception === "yes" && res[k].judgeable);
  console.log(`rule 1 (win): ${wins.length ? wins.join(", ") : "none"}`);
  console.log(`rule 2a (run 1, G11 losses after the layout exception): ${lossesR1.length ? lossesR1.join(", ") : "none"}${exempted.length ? `; exempted to run 2: ${exempted.join(", ")}` : ""}`);
  console.log(`rule 2a (run 1, GL losses): ${lossesGL.length ? lossesGL.join(", ") : "none"}`);
  console.log(`rule 2b (run 2 losses): ${lossesR2.length ? lossesR2.join(", ") : "none"}`);
  const pass = wins.length > 0 && !lossesR1.length && !lossesGL.length && !lossesR2.length;
  console.log(`SPEED: ${pass ? "PASS (rules 3-5 next)" : "FAIL — NO-GO"}`);
}
