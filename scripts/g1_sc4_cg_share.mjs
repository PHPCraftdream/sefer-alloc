#!/usr/bin/env node
// G1-SC-4 mechanism share: from callgrind_annotate --inclusive=no listings
// (docs/perf/g1_scan/sc4_cg/<arm>.<S>.fn.txt), Ir per allocation excluding the
// probe's own post-phase segment count (hashbrown in g1_rhythm_probe::segments),
// and the self Ir of the discovery-scan files (find_segment/scan.rs,
// remote_bitmap/bitmap_scan.rs, segment_table/active_kind_index.rs) per allocation.
import fs from "node:fs";
const dir = process.argv[2];
const SCAN = /find_segment\/scan\.rs|remote_bitmap\/bitmap_scan\.rs|segment_table\/active_kind_index\.rs/;
const out = ["| Arm | S | n | Ir/alloc (excl. probe count) | discovery-scan self Ir/alloc | share |", "|---|---:|---:|---:|---:|---:|"];
for (const arm of ["sefer", "mi"]) for (const S of [8, 64]) {
  const n = S * 65536;
  let total = 0, probe = 0, scan = 0;
  for (const l of fs.readFileSync(`${dir}/${arm}.${S}.fn.txt`, "utf8").split(/\r?\n/)) {
    const m = /^\s*([\d,]+)\s+(?:\([^)]*\)\s+)?(.*)$/.exec(l);
    if (!m) continue;
    const ir = Number(m[1].replace(/,/g, ""));
    if (m[2].startsWith("PROGRAM TOTALS")) { total = ir; continue; }
    if (m[2].includes("g1_rhythm_probe::segments")) probe += ir;
    if (SCAN.test(m[2])) scan += ir;
  }
  out.push(`| ${arm} | ${S} | ${n} | ${((total - probe) / n).toFixed(1)} | ${(scan / n).toFixed(1)} | ${((100 * scan) / (total - probe)).toFixed(1)} % |`);
}
console.log(out.join("\n"));
