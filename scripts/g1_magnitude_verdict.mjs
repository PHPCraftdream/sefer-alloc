#!/usr/bin/env node
// Magnitude view of a paired bench:table A/B (order base, x, x, base): per
// cell d1/d2 = (x sefer/mi) / (base sefer/mi) - 1 per pair, a cell effect
// that counts only when both pairs agree in sign (the smaller |d|, signed),
// the sefer-time change (mean of the two pairs) as a denominator-free
// cross-check, and the aggregate: geometric mean over all cells, total win
// and loss magnitude over the agreeing cells, and the largest loss.
// Usage: node scripts/g1_magnitude_verdict.mjs <dir> <base-prefix> <x-prefix> [label]

import fs from "node:fs";
import process from "node:process";

const [dir, bp, xp, label = xp] = process.argv.slice(2);
const load = (f) => {
  const m = {};
  for (const l of fs.readFileSync(`${dir}/${f}.csv`, "utf8").trim().split(/\r?\n/).slice(1)) {
    const c = l.split(",");
    m[`${c[0]} ${c[1]} ${c[2]}`] = Number(c[8]);
  }
  return m;
};
const r = { b1: load(`${bp}_r1`), x1: load(`${xp}_r1`), x2: load(`${xp}_r2`), b2: load(`${bp}_r2`) };
const cells = Object.keys(r.b1).filter((k) => k.endsWith(" sefer")).map((k) => k.slice(0, -6));
const ratio = (run, k) => run[`${k} sefer`] / run[`${k} mimalloc`];
const pct = (x) => `${x >= 0 ? "+" : ""}${(x * 100).toFixed(1)} %`;

const out = [
  `| Cell (${label}) | ratio d pair 1 / 2 | ratio agreed | sefer time pair 1 / 2 | sefer time agreed | sefer/mi before → after |`,
  "|---|---:|---:|---:|---:|---|",
];
let logSum = 0;
let win = 0;
let loss = 0;
let maxLoss = 0;
let tLog = 0;
let tWin = 0;
let tLoss = 0;
let tMax = 0;
const agree = (a, b) => (Math.sign(a) === Math.sign(b) ? Math.sign(a) * Math.min(Math.abs(a), Math.abs(b)) : 0);
for (const k of cells) {
  const d1 = ratio(r.x1, k) / ratio(r.b1, k) - 1;
  const d2 = ratio(r.x2, k) / ratio(r.b2, k) - 1;
  const agreed = Math.sign(d1) === Math.sign(d2) ? Math.sign(d1) * Math.min(Math.abs(d1), Math.abs(d2)) : 0;
  const t1 = r.x1[`${k} sefer`] / r.b1[`${k} sefer`] - 1;
  const t2 = r.x2[`${k} sefer`] / r.b2[`${k} sefer`] - 1;
  const ta = agree(t1, t2);
  tLog += Math.log(Math.sqrt((1 + t1) * (1 + t2)));
  if (ta < 0) tWin -= ta;
  if (ta > 0) tLoss += ta;
  tMax = Math.max(tMax, ta);
  logSum += Math.log(Math.sqrt((1 + d1) * (1 + d2)));
  if (agreed < 0) win -= agreed;
  if (agreed > 0) loss += agreed;
  maxLoss = Math.max(maxLoss, agreed);
  const rb = (ratio(r.b1, k) + ratio(r.b2, k)) / 2;
  const rx = (ratio(r.x1, k) + ratio(r.x2, k)) / 2;
  out.push(`| ${k} B | ${pct(d1)} / ${pct(d2)} | ${pct(agreed)} | ${pct(t1)} / ${pct(t2)} | ${pct(ta)} | ${rb.toFixed(3)} → ${rx.toFixed(3)} |`);
}
const gm = Math.exp(logSum / cells.length) - 1;
out.push(
  "",
  `Sefer time (A/B of the two builds): geometric mean ${pct(Math.exp(tLog / cells.length) - 1)}; agreed wins ${(tWin * 100).toFixed(1)} pp, agreed losses ${(tLoss * 100).toFixed(1)} pp; largest agreed loss ${pct(tMax)}.`,
  `Ratio to mimalloc: geometric mean over ${cells.length} cells: ${pct(gm)}; agreed wins ${(win * 100).toFixed(1)} pp, agreed losses ${(loss * 100).toFixed(1)} pp (ratio ${loss > 0 ? (win / loss).toFixed(2) : "∞"}); largest agreed loss ${pct(maxLoss)}.`,
);
console.log(out.join("\n"));

// Magnitude rule (owner, 2026-10-30): GO if the sefer-time geometric mean is
// <= -0.5 %, agreed wins >= 2 x agreed losses, the largest agreed loss <= 3 %,
// and no cell turns into a LOSS class (> 1.03) against mimalloc.
const tGm = Math.exp(tLog / cells.length) - 1;
let newLoss = 0;
for (const k of cells) {
  const rb = (ratio(r.b1, k) + ratio(r.b2, k)) / 2;
  const rx = (ratio(r.x1, k) + ratio(r.x2, k)) / 2;
  if (rb <= 1.03 && rx > 1.03) newLoss++;
}
const go = tGm <= -0.005 && tWin >= 2 * tLoss && tMax <= 0.03 && newLoss === 0;
console.log(
  `\nMagnitude rule: gm ${pct(tGm)} (<= -0.5 %), wins/losses ${(tWin * 100).toFixed(1)}/${(tLoss * 100).toFixed(1)} pp (>= 2x), largest loss ${pct(tMax)} (<= 3 %), new LOSS cells ${newLoss} (0) => **${go ? "GO" : "NO-GO"}**`,
);
