#!/usr/bin/env node
// B4-25 ceiling report generator: derives every number of
// docs/perf/B4_25_REMOTE_CEILING.md from the committed callgrind log
// docs/perf/b4_25_ceiling/_raw_callgrind_mt.log, writes the summary CSV and
// prints the Markdown tables.
//
// Remote-reception path = files that run only to publish, scan and reclaim
// remote frees: src/alloc_core/segment/remote_bitmap/** (incl. the producer's
// sidecar_bitmap.rs and leaf_classes.rs), both sidecar_drain.rs,
// find_segment/** and alloc_core_small_reclaim.rs (the bin push of a
// reclaimed record). Its Ir at T threads per thread, minus the same at T=1,
// bounds what any change to remote reception can save (ceiling).
//
// Usage: node scripts/b4_25_ceiling_report.mjs [--check]
//   --check: fail if the CSV on disk differs from the regenerated one.
import { readFileSync, writeFileSync, existsSync } from "node:fs";
import assert from "node:assert/strict";

const RAW = "docs/perf/b4_25_ceiling/_raw_callgrind_mt.log";
const CSV = "docs/perf/B4_25_REMOTE_CEILING_summary.csv";
const check = process.argv.includes("--check");
const REMOTE = /remote_bitmap\/|sidecar_drain\.rs$|find_segment\/|alloc_core_small_reclaim\.rs$/;

const total = {};
const files = {};
const oracle = {};
let identity = "";
for (const l of readFileSync(RAW, "utf8").split("\n")) {
  let m;
  if ((m = /^identity (\S+)/.exec(l))) identity = m[1];
  if ((m = /^IR_TOTAL (\w+) (\w+) (\d+) (\d+)$/.exec(l))) total[`${m[1]}:${m[2]}:${m[3]}`] = Number(m[4]);
  if ((m = /^ORACLE sefer (\w+) (\d+) publish_foreign_calls (\d+)$/.exec(l))) oracle[`${m[1]}:${m[2]}`] = Number(m[3]);
  if ((m = /^FILE_IR sefer (\w+) (\d+) (\S+) (\d+)$/.exec(l))) {
    const k = `${m[1]}:${m[2]}`;
    (files[k] ??= {})[m[3]] = Number(m[4]);
  }
}
assert.match(identity, /^commit-[0-9a-f]{40}$/, "identity must be a full commit SHA");

const rows = [];
for (const wl of ["mstress", "larson"]) {
  const remotePt = {};
  for (const t of [1, 2, 4]) {
    const s = total[`sefer:${wl}:${t}`];
    const mi = total[`mimalloc:${wl}:${t}`];
    const f = files[`${wl}:${t}`];
    assert.ok(s > 0 && mi > 0 && f && Number.isInteger(oracle[`${wl}:${t}`]), `missing run ${wl} T=${t}`);
    const remote = Object.entries(f).filter(([p]) => REMOTE.test(p)).reduce((a, [, v]) => a + v, 0);
    const annotated = Object.values(f).reduce((a, v) => a + v, 0);
    assert.ok(annotated <= s, `${wl} T=${t}: per-file sum exceeds program total`);
    remotePt[t] = remote / t;
    rows.push({
      wl, t, sefer: s, mimalloc: mi, ratio: s / mi,
      seferPt: s / t, gapPt: (s - mi) / t,
      remote, remotePt: remote / t, publish: oracle[`${wl}:${t}`],
    });
  }
  for (const r of rows.filter((x) => x.wl === wl)) {
    r.extraPt = r.remotePt - remotePt[1];
    r.ceilingPct = (100 * r.extraPt) / r.seferPt;
    r.remoteSharePct = (100 * r.remotePt) / r.seferPt;
    r.remoteOfGapPct = (100 * r.remotePt) / r.gapPt;
    assert.ok(Math.abs((r.ceilingPct / 100) * r.seferPt - r.extraPt) < 1, "ceiling arithmetic");
  }
}
// Path activation: calls into HeapCore::publish_foreign, the remote-free entry.
for (const wl of ["mstress", "larson"]) {
  assert.equal(rows.find((r) => r.wl === wl && r.t === 1).publish, 0, `${wl} T=1 must not publish`);
  for (const t of [2, 4]) assert.ok(rows.find((r) => r.wl === wl && r.t === t).publish > 0, `${wl} T=${t} must publish`);
}

const f2 = (x) => x.toFixed(2);
const csv = ["identity,workload,threads,sefer_ir,mimalloc_ir,sefer_over_mimalloc,sefer_ir_per_thread,gap_ir_per_thread,remote_path_ir,remote_path_ir_per_thread,publish_foreign_calls,remote_extra_ir_per_thread_vs_t1,ceiling_pct_of_sefer,remote_share_pct_of_sefer,remote_pct_of_gap"];
for (const r of rows) {
  csv.push([identity, r.wl, r.t, r.sefer, r.mimalloc, f2(r.ratio), Math.round(r.seferPt), Math.round(r.gapPt), r.remote, Math.round(r.remotePt), r.publish, Math.round(r.extraPt), f2(r.ceilingPct), f2(r.remoteSharePct), f2(r.remoteOfGapPct)].join(","));
}
const text = csv.join("\n") + "\n";
if (check) {
  assert.ok(existsSync(CSV), `${CSV} missing`);
  assert.equal(readFileSync(CSV, "utf8"), text, `${CSV} is stale`);
} else {
  writeFileSync(CSV, text);
}

console.log("| workload | T | Sefer Ir | mimalloc Ir | Sefer / mimalloc | gap Ir per thread | `publish_foreign` calls |");
console.log("|---|---:|---:|---:|---:|---:|---:|");
for (const r of rows) console.log(`| ${r.wl} | ${r.t} | ${r.sefer} | ${r.mimalloc} | ${f2(r.ratio)}× | ${Math.round(r.gapPt)} | ${r.publish} |`);
console.log("\n| workload | T | remote-path Ir per thread | its share of Sefer Ir per thread | its share of the gap per thread | extra over T=1 per thread | ceiling = extra / Sefer Ir per thread |");
console.log("|---|---:|---:|---:|---:|---:|---:|");
for (const r of rows) console.log(`| ${r.wl} | ${r.t} | ${Math.round(r.remotePt)} | ${f2(r.remoteSharePct)} % (${Math.round(r.remotePt)} / ${Math.round(r.seferPt)}) | ${f2(r.remoteOfGapPct)} % (${Math.round(r.remotePt)} / ${Math.round(r.gapPt)}) | ${Math.round(r.extraPt)} | ${f2(r.ceilingPct)} % (${Math.round(r.extraPt)} / ${Math.round(r.seferPt)}) |`);
const worst = Math.max(...rows.filter((r) => r.t > 1).map((r) => r.ceilingPct));
console.log(`\nMAX_CEILING_PCT ${f2(worst)}`);
