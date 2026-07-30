// R31-0 (task #471): derives `docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv`
// from the raw per-cell rows the bench prints to stdout (captured as
// `docs/perf/_raw_r31_0_off.log` and `_raw_r31_0_on.log`). This is the ONE
// checked script that turns the raw log into the report's machine-readable
// summary (CLAUDE.md "tables derived by one checked script, not hand-transcribed").
//
// Usage:
//   node scripts/r31_0_summary.mjs [landing_commit_sha]
//
// `landing_commit_sha` is this report's own landing commit (chicken-and-egg: a
// commit cannot cite its own SHA inside its own tree). Omit on first generation
// (column reads "UNFILLED"); re-run with the real SHA in the follow-up commit
// that fills the placeholder -- mirrors the 1272a52/9335979 precedent.
//
// Parses ONLY lines starting with `R31_0_`; ignores cargo preamble and `#`
// comment lines. Reads the PRIMARY run-1 logs (the cited evidence); the run-2
// logs (_raw_r31_0_*_run2.log) are cited in the report only for the noise-floor
// discussion and are NOT folded into the summary.

import { readFileSync, writeFileSync } from "node:fs";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const landingCommit = process.argv[2] || "UNFILLED";

// Machine identity (same host as R30-3/R29-16; recorded here so the CSV is
// self-describing without re-reading prose).
const META = {
  commit_sha: "14a9ef34145cc62188d734cf6987bcfd4dbcb088", // base `main` HEAD this run measured against
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045",
  rustc: "1.97.0",
};

const VIRGIN_RECYCLED_HEADER =
  "commit_sha,feature_set,virgin_zero_skip,cpu,os,rustc,scenario,size,touch,reps,refill_n,mean_hits,min_hits,expected_hits,mean_zp,min_zp,mean_ns_per_op,p50_ns_per_op,min_ns_per_op,max_ns_per_op,mean_act_pct,min_act_pct,oracle,landing_commit";
const RETENTION_HEADER =
  "commit_sha,feature_set,virgin_zero_skip,cpu,os,rustc,kind,size,refill_n,retained,expected_retained,mask,expected_mask,all_zero,oracle,landing_commit";

function parseRows(logText) {
  const out = [];
  for (const line of logText.split(/\r?\n/)) {
    if (!line.startsWith("R31_0_")) continue;
    out.push(line.split(","));
  }
  return out;
}

// virgin/recycled row: R31_0_{scenario},size,touch,reps,refill_n,mean_hits,min_hits,expected_hits,mean_zp,min_zp,mean_ns,p50_ns,min_ns,max_ns,mean_act,min_act,oracle
// (index: 0=tag,1=size,2=touch,3=reps,4=refill_n,5=mean_hits,6=min_hits,7=expected_hits,8=mean_zp,9=min_zp,10=mean_ns,11=p50,12=min,13=max,14=mean_act,15=min_act,16=oracle)
function fmtVirginRecycled(featSet, vzs, fields) {
  const scenario = fields[0].slice("R31_0_".length); // "virgin" | "recycled"
  return [
    META.commit_sha,
    `"${featSet}"`,
    vzs,
    META.cpu,
    META.os,
    META.rustc,
    scenario,
    fields[1],
    fields[2],
    fields[3],
    fields[4],
    fields[5],
    fields[6],
    fields[7],
    fields[8],
    fields[9],
    fields[10],
    fields[11],
    fields[12],
    fields[13],
    fields[14],
    fields[15],
    fields[16],
    landingCommit,
  ].join(",");
}

// retention row: R31_0_RETENTION,size,refill_n,retained,expected_retained,mask,expected_mask,all_zero,oracle
function fmtRetention(featSet, vzs, fields) {
  return [
    META.commit_sha,
    `"${featSet}"`,
    vzs,
    META.cpu,
    META.os,
    META.rustc,
    "retention",
    fields[1],
    fields[2],
    fields[3],
    fields[4],
    fields[5],
    fields[6],
    fields[7],
    fields[8],
    landingCommit,
  ].join(",");
}

const offRows = parseRows(read("docs/perf/_raw_r31_0_off.log"));
const onRows = parseRows(read("docs/perf/_raw_r31_0_on.log"));

const OFF_FEAT = "production alloc-stats";
const ON_FEAT = "production alloc-stats virgin-zero-skip";

const lines = [VIRGIN_RECYCLED_HEADER];
for (const r of offRows) {
  if (r[0] === "R31_0_RETENTION") continue; // OFF has no retention probe
  lines.push(fmtVirginRecycled(OFF_FEAT, "false", r));
}
for (const r of onRows) {
  if (r[0] === "R31_0_RETENTION") {
    lines.push(fmtRetention(ON_FEAT, "true", r));
  } else {
    lines.push(fmtVirginRecycled(ON_FEAT, "true", r));
  }
}

const outPath = new URL(
  "docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv",
  ROOT,
);
writeFileSync(outPath, lines.join("\n") + "\n");
console.error(
  `wrote ${lines.length - 1} data rows to docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv (landing_commit=${landingCommit})`,
);
