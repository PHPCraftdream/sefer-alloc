#!/usr/bin/env node
// U32 growth16 process-level time judge (measurement only).
// Each sample is one fresh process of examples/u32_growth_time_probe.rs.
// Pairs run in ABBA order (pair i: A then B for even i, B then A for odd i);
// effect = median over pairs of ln(B.total_ns / A.total_ns); 95 % CI by
// bootstrap over pairs (fixed seed). The null run uses the SAME binary as A
// and B and must contain 0 in its CI for the run to count.
//
// Usage: node scripts/u32_growth_judge.mjs --a <exe> --b <exe> --n <blocks>
//          --pairs <P> --raw <out.jsonl> [--label <name>]
import { spawnSync } from "node:child_process";
import { writeFileSync, appendFileSync, readFileSync } from "node:fs";
import { createHash } from "node:crypto";

const arg = (k, d) => {
  const i = process.argv.indexOf(`--${k}`);
  if (i < 0) {
    if (d === undefined) throw new Error(`missing --${k}`);
    return d;
  }
  return process.argv[i + 1];
};
const A = arg("a"), B = arg("b"), n = Number(arg("n")), P = Number(arg("pairs"));
const raw = arg("raw"), label = arg("label", "u32");
const sha = (p) => createHash("sha256").update(readFileSync(p)).digest("hex");
const run = (exe) => {
  const r = spawnSync(exe, [String(n)], { encoding: "utf8", timeout: 120000 });
  if (r.status !== 0) throw new Error(`${exe} failed: ${r.stderr}`);
  return JSON.parse(r.stdout.trim());
};
writeFileSync(raw, JSON.stringify({ label, a: A, b: B, a_sha256: sha(A), b_sha256: sha(B), n, pairs: P }) + "\n");
const logs = [];
for (let i = 0; i < P; i++) {
  const order = i % 2 === 0 ? ["a", "b"] : ["b", "a"];
  const s = {};
  for (const arm of order) s[arm] = run(arm === "a" ? A : B);
  const row = { pair: i, order: order.join(""), a: s.a, b: s.b };
  appendFileSync(raw, JSON.stringify(row) + "\n");
  logs.push(Math.log(s.b.total_ns / s.a.total_ns));
}
const median = (v) => { const s = [...v].sort((x, y) => x - y); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
let seed = 0x5eed32n;
const rnd = () => { seed = (seed * 6364136223846793005n + 1442695040888963407n) & 0xffffffffffffffffn; return Number(seed >> 11n) / 2 ** 53; };
const boots = [];
for (let k = 0; k < 10000; k++) {
  const res = Array.from({ length: logs.length }, () => logs[Math.floor(rnd() * logs.length)]);
  boots.push(median(res));
}
boots.sort((x, y) => x - y);
const eff = median(logs), lo = boots[249], hi = boots[9749];
const pct = (x) => (100 * x).toFixed(2);
const out = { label, n, pairs: P, effect_pct: pct(eff), ci_lo_pct: pct(lo), ci_hi_pct: pct(hi), ratio: Math.exp(eff).toFixed(4) };
console.log(JSON.stringify(out));
