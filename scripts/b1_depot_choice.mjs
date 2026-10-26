#!/usr/bin/env node
// DP-2 — pre-registered storage choice for the production depot
// (docs/design/2026-10-26-depot-design.md, branch b4-depot).
//
// Rule: take S3 (chain) iff on bulk64 AND churn64 its effect is >= 80 % of
// S2's (array) effect, by median log, AND S3 passes the gate thresholds;
// otherwise S2. Gate thresholds (prototype, indicative for DP-5):
//   1. bulk64, churn64: effect <= ln(1/1.25), CI upper < 0;
//   2. pair16, pair64, larson: effect <= +1 %, CI upper <= +1.5 %;
//   3. other kernels: no significant slowdown above 2 %.
//
// Input: two CSVs written by scripts/b4_ablate_vs_null.mjs --csv.
// Usage: node scripts/b1_depot_choice.mjs --s2 <csv> --s3 <csv>

import fs from "node:fs";
import process from "node:process";

process.on("uncaughtException", (e) => {
  console.error(`error: ${e?.message ?? e}`);
  process.exit(1);
});

const args = {};
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  const v = process.argv[++i];
  if (v === undefined) throw new Error(`${a} needs a value`);
  if (a === "--s2") args.s2 = v;
  else if (a === "--s3") args.s3 = v;
  else throw new Error(`unknown argument ${a}`);
}
if (!args.s2 || !args.s3) throw new Error("--s2 and --s3 are required");

function load(p) {
  const [head, ...lines] = fs.readFileSync(p, "utf8").split("\n").filter(Boolean);
  const cols = head.split(",");
  const out = {};
  let variant = null;
  for (const l of lines) {
    const c = l.split(",");
    const r = Object.fromEntries(cols.map((k, i) => [k, c[i]]));
    if (variant === null) variant = r.variant;
    if (r.variant !== variant) throw new Error(`${p}: mixed variants`);
    if (r.precise !== "yes") throw new Error(`${p} ${r.kernel}: null imprecise`);
    out[r.kernel] = { eff: +r.effect, lo: +r.effect_ci_lo, hi: +r.effect_ci_hi, ratio: +r.ratio };
    if (Math.abs(Math.exp(out[r.kernel].eff) - out[r.kernel].ratio) > 1e-9) throw new Error(`${p}: ratio != exp(effect)`);
  }
  return { variant, k: out };
}

const S2 = load(args.s2), S3 = load(args.s3);
const kernels = Object.keys(S2.k);
for (const k of kernels) if (!S3.k[k]) throw new Error(`kernel ${k} missing in S3`);

const word = (eff) => {
  const r = Math.exp(eff);
  return r < 1 ? `быстрее в ${(1 / r).toFixed(3)}×` : `медленнее в ${r.toFixed(3)}×`;
};
const pct = (x) => (x * 100).toFixed(2);

function gate(v) {
  const fails = [];
  for (const k of ["bulk64", "churn64"]) {
    const x = v.k[k];
    if (!(x.eff <= Math.log(1 / 1.25) && x.hi < 0)) fails.push(`${k}: эффект ${pct(x.eff)} % > ${pct(Math.log(1 / 1.25))} % или верх CI >= 0`);
  }
  for (const k of ["pair16", "pair64", "larson"]) {
    const x = v.k[k];
    if (!(x.eff <= 0.01 && x.hi <= 0.015)) fails.push(`${k}: эффект ${pct(x.eff)} % или верх CI ${pct(x.hi)} % выше порога`);
  }
  for (const k of kernels.filter((k) => !["bulk64", "churn64", "pair16", "pair64", "larson"].includes(k))) {
    const x = v.k[k];
    if (x.lo > 0 && x.eff > 0.02) fails.push(`${k}: значимое замедление ${pct(x.eff)} % > 2 %`);
  }
  return fails;
}

console.log(`# DP-2 storage choice: S2=${S2.variant} S3=${S3.variant}; effect = ln(t_variant/t_sefer) net of null`);
console.log("");
console.log("| Ядро | S2: эффект % [CI] | S2 | S3: эффект % [CI] | S3 | S3/S2 |");
console.log("|---|---|---|---|---|---:|");
for (const k of kernels) {
  const a = S2.k[k], b = S3.k[k];
  const share = a.eff !== 0 ? b.eff / a.eff : NaN;
  console.log(`| ${k} | ${pct(a.eff)} [${pct(a.lo)}, ${pct(a.hi)}] | ${word(a.eff)} | ${pct(b.eff)} [${pct(b.lo)}, ${pct(b.hi)}] | ${word(b.eff)} | ${Number.isFinite(share) ? share.toFixed(3) : "—"} |`);
}
console.log("");

const shares = {};
for (const k of ["bulk64", "churn64"]) {
  if (!(S2.k[k].eff < 0)) throw new Error(`S2 ${k}: no speedup, rule undefined`);
  shares[k] = S3.k[k].eff / S2.k[k].eff;
  if (Math.abs(shares[k] * S2.k[k].eff - S3.k[k].eff) > 1e-12) throw new Error("share arithmetic");
  console.log(`# ${k}: S3/S2 = ${pct(S3.k[k].eff)} / ${pct(S2.k[k].eff)} = ${(shares[k] * 100).toFixed(1)} % (порог 80 %)`);
}
const g2 = gate(S2), g3 = gate(S3);
console.log(`# пороги гейта, S2: ${g2.length ? g2.join("; ") : "пройдены"}`);
console.log(`# пороги гейта, S3: ${g3.length ? g3.join("; ") : "пройдены"}`);
const s3ok = shares.bulk64 >= 0.8 && shares.churn64 >= 0.8 && g3.length === 0;
console.log(`CHOICE=${s3ok ? "S3" : "S2"}`);
