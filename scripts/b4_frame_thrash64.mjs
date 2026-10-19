#!/usr/bin/env node
// Derives the B4_FRAME_TAX §9 thrash64 table and CSV from the raw callgrind logs.
// Usage: node scripts/b4_frame_thrash64.mjs
import { readFileSync, writeFileSync } from "node:fs";

const DIR = "docs/perf";
const ARMS = [
  { label: "t64-base", arm: "main f93c1b52", alloc: "sefer", identity: "f93c1b52 (src clean)" },
  {
    label: "t64-split",
    arm: "b4-split final",
    alloc: "sefer",
    identity: "f93c1b52 + diff sha256 1638450c744e6adf86c5763d6be9be00c8fcfcc3e2c0229c240fd2a8b1922a97",
  },
  { label: "t64-mi2", arm: "mimalloc", alloc: "mimalloc", identity: "f93c1b52 harness" },
];

function parse(label, alloc) {
  const text = readFileSync(`${DIR}/_raw_b4_frame_${label}.log`, "utf8");
  const num = (re) => {
    const m = text.match(re);
    if (!m) throw new Error(`${label}: missing ${re}`);
    return Number(m[1]);
  };
  const act = text.match(/distinct 16B-window segments: (\d+)/);
  const segments = act ? Number(act[1]) : null;
  if (alloc === "sefer" && !(segments >= 32)) {
    throw new Error(`${label}: activation oracle failed (segments=${segments})`);
  }
  return {
    segments,
    allocIr: num(new RegExp(`== ${alloc} w_alloc marginal Ir/op: (-?\\d+)`)),
    freeIr: num(new RegExp(`== ${alloc} w_free marginal Ir/op: (-?\\d+)`)),
    allocCyc: num(new RegExp(`== ${alloc} w_alloc cache:.*EstCycles/op=([\\d.]+)`)),
    freeCyc: num(new RegExp(`== ${alloc} w_free cache:.*EstCycles/op=([\\d.]+)`)),
  };
}

const rows = ARMS.map((a) => ({ ...a, ...parse(a.label, a.alloc) }));
const base = rows[0];
const split = rows[1];
const dFree = split.freeIr - base.freeIr;
const dAlloc = split.allocIr - base.allocIr;
if (dFree !== split.freeIr - base.freeIr || dAlloc !== split.allocIr - base.allocIr) {
  throw new Error("delta arithmetic");
}

const md = [
  "| Плечо | Источник | Сегментов (оракул) | w_alloc Ir/op | w_free Ir/op | w_alloc EstCycles | w_free EstCycles | Лог |",
  "|---|---|---:|---:|---:|---:|---:|---|",
  ...rows.map(
    (r) =>
      `| ${r.arm} | ${r.identity} | ${r.segments ?? "—"} | ${r.allocIr} | ${r.freeIr} | ${r.allocCyc} | ${r.freeCyc} | \`${DIR}/_raw_b4_frame_${r.label}.log\` |`,
  ),
  `| Δ b4-split − main | | | ${dAlloc >= 0 ? "+" : ""}${dAlloc} | ${dFree >= 0 ? "+" : ""}${dFree} | | | |`,
].join("\n");

const csv = [
  "label,arm,source_identity,segments,w_alloc_ir,w_free_ir,w_alloc_estcycles,w_free_estcycles",
  ...rows.map(
    (r) =>
      `${r.label},${r.arm},${r.identity},${r.segments ?? ""},${r.allocIr},${r.freeIr},${r.allocCyc},${r.freeCyc}`,
  ),
].join("\n");

writeFileSync(`${DIR}/B4_FRAME_TAX_thrash64_summary.csv`, csv + "\n");
console.log(md);
