#!/usr/bin/env node
// G1-K7 Addendum 6, rule 4 — Ir per bench against K5 2bc830cc, from the
// gate-run summary.csv (scripts/r20_r6_ab_iai_run.mjs). Loss on a bench with
// zero overflow parks: (Ir − B) of v5 exceeds the base's by more than 10,
// B = large_alloc_free_cycle (the bootstrap proxy).
//
// Usage: node scripts/g1_k7_a6_iai.mjs <summary.csv>

import fs from "node:fs";
import process from "node:process";

const rows = fs.readFileSync(process.argv[2], "utf8").trim().split(/\r?\n/).slice(1).map((l) => l.split(","));
const m = new Map(rows.map((r) => [r[0], { a: Number(r[1]), b: Number(r[2]) }]));
const B = m.get("large_alloc_free_cycle");
if (!B) throw new Error("no large_alloc_free_cycle row");
const out = [...m].map(([k, v]) => ({ k, ...v, norm: v.b - B.b - (v.a - B.a) }));
out.sort((x, y) => y.norm - x.norm);
console.log(`# rule 4: ${out.length} benches; B = large_alloc_free_cycle (${B.a} -> ${B.b}, +${B.b - B.a}); norm = (Ir_v5 − B_v5) − (Ir_K5 − B_K5)`);
console.log("| bench | K5 Ir | v5 Ir | norm Δ Ir | > 10 |");
console.log("|---|---:|---:|---:|---|");
for (const o of out) console.log(`| ${o.k} | ${o.a} | ${o.b} | ${o.norm} | ${o.norm > 10 ? "yes" : "no"} |`);
const over = out.filter((o) => o.norm > 10).map((o) => o.k);
console.log(`\nover +10: ${over.length ? over.join(", ") : "none"}`);
