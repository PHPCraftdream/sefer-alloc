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
// comment lines. Reads BOTH run pairs: the PRIMARY run-1 logs (the cited
// evidence, folded into the CSV) and the independent run-2 repeat logs
// (`_raw_r31_0_*_run2.log`, folded into a SECOND, run2-labelled CSV section
// so the report's §3.1 "Δ (run2)" column is genuinely script-derived too, not
// hand-computed prose).
//
// P1-1 fix (Round 31 review response, docs/reviews/2026-07-31-r31-full-review.md
// §7 P1-1): before this fix, this script re-emitted raw fields verbatim and
// computed no arithmetic at all -- the report's own Δ columns (including the
// four bolded `notouch` headline percentages that carry the round's P0
// verdict) were HAND-COMPUTED, exactly the practice CLAUDE.md's R30-9 rule
// points 1, 2 and 6 exist to forbid. This version (a) reads both run pairs,
// (b) computes the actual percentage delta for every (size, touch) cell in
// both scenarios and both runs, writing them into the CSV as new columns,
// and (c) hard-asserts (`throw`s) that the four `notouch`/virgin percentage
// deltas -- the report's headline numbers -- match the already-published
// values to within 0.1 percentage points, so a wrong number is a FAILING
// SCRIPT RUN, never a published claim a human transcribed by hand.

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
// Derived-delta header (P1-1 fix): one row per (scenario, size, touch),
// carrying both runs' OFF/ON mean_ns_per_op and the computed percentage delta.
const DELTA_HEADER =
  "scenario,size,touch,off_ns_r1,on_ns_r1,delta_pct_r1,off_ns_r2,on_ns_r2,delta_pct_r2,landing_commit";

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
  const oracle = fields[16];
  // CORRECTED 2026-07-31 (Round 31 review response, docs/reviews/2026-07-31-r31-full-review.md
  // §7 P2-2): on the OFF binary (oracle == "NA"), `SMALL_ZERO_PASS_CALLS` --
  // the counter `mean_act_pct`/`min_act_pct` are derived from -- is never
  // incremented at all (report §2.2: the counter lives entirely inside the
  // `#[cfg(feature = "virgin-zero-skip")]` branch), so the raw log's 100.00/0.00
  // figures for those two columns are structurally vacuous, not a real
  // activation measurement. Emit `NA` there too instead of the vacuous number,
  // so a script reading only the percentage columns cannot mistake this for a
  // real 100% activation reading. Data for every ON row (oracle=PASS/FAIL) is
  // unchanged.
  const meanActPct = oracle === "NA" ? "NA" : fields[14];
  const minActPct = oracle === "NA" ? "NA" : fields[15];
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
    meanActPct,
    minActPct,
    oracle,
    landingCommit,
  ].join(",");
}

// CORRECTED 2026-07-31 (Round 31 review response, docs/reviews/2026-07-31-r31-full-review.md
// §7 P2-1): the 4 retention rows below carry only 16 fields (RETENTION_HEADER's
// shape) but were being appended, with no section marker, into the SAME output
// file under the 24-column VIRGIN_RECYCLED_HEADER -- RETENTION_HEADER was defined
// above but never actually written anywhere, so a naive CSV reader mis-keyed the
// retention rows' fields against the wrong header (e.g. `expected_hits` reading as
// `true`). Fixed by writing the retention rows to their OWN file, under their OWN
// header, instead of interleaving them into the virgin/recycled file. This is a
// pure output-format restructuring -- no data value below changed.
const retentionLines = [RETENTION_HEADER];

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

// Build a lookup keyed by "scenario|size|touch" -> mean_ns_per_op (field
// index 10 of a virgin/recycled row), skipping RETENTION rows.
function meanNsByCell(rows) {
  const m = new Map();
  for (const r of rows) {
    if (r[0] === "R31_0_RETENTION") continue;
    const scenario = r[0].slice("R31_0_".length);
    const key = `${scenario}|${r[1]}|${r[2]}`;
    m.set(key, Number(r[10]));
  }
  return m;
}

function pctDelta(off, on) {
  return ((on - off) / off) * 100;
}

const offRows1 = parseRows(read("docs/perf/_raw_r31_0_off.log"));
const onRows1 = parseRows(read("docs/perf/_raw_r31_0_on.log"));
const offRows2 = parseRows(read("docs/perf/_raw_r31_0_off_run2.log"));
const onRows2 = parseRows(read("docs/perf/_raw_r31_0_on_run2.log"));

const OFF_FEAT = "production alloc-stats";
const ON_FEAT = "production alloc-stats virgin-zero-skip";

const lines = [VIRGIN_RECYCLED_HEADER];
for (const r of offRows1) {
  if (r[0] === "R31_0_RETENTION") continue; // OFF has no retention probe
  lines.push(fmtVirginRecycled(OFF_FEAT, "false", r));
}
for (const r of onRows1) {
  if (r[0] === "R31_0_RETENTION") {
    // CORRECTED 2026-07-31 (P2-1): routed to the dedicated retentionLines
    // array/file instead of being interleaved into `lines` under the wrong header.
    retentionLines.push(fmtRetention(ON_FEAT, "true", r));
  } else {
    lines.push(fmtVirginRecycled(ON_FEAT, "true", r));
  }
}

// --- Derived Δ section (P1-1 fix): compute the actual percentage delta for
// every (scenario, size, touch) cell in BOTH runs, from raw mean_ns_per_op. ---
const off1ByCell = meanNsByCell(offRows1);
const on1ByCell = meanNsByCell(onRows1);
const off2ByCell = meanNsByCell(offRows2);
const on2ByCell = meanNsByCell(onRows2);

const deltaLines = [DELTA_HEADER];
const cells = [];
for (const key of off1ByCell.keys()) {
  const [scenario, size, touch] = key.split("|");
  cells.push({ scenario, size, touch, key });
}
// Stable order: virgin before recycled, sizes in original sweep order, touch
// in notouch/onebyte/full order (matches the report's own table order).
const SIZE_ORDER = ["4k", "16k", "64k", "128k"];
const TOUCH_ORDER = ["notouch", "onebyte", "full"];
const SCENARIO_ORDER = ["virgin", "recycled"];
cells.sort((a, b) => {
  const s = SCENARIO_ORDER.indexOf(a.scenario) - SCENARIO_ORDER.indexOf(b.scenario);
  if (s !== 0) return s;
  const sz = SIZE_ORDER.indexOf(a.size) - SIZE_ORDER.indexOf(b.size);
  if (sz !== 0) return sz;
  return TOUCH_ORDER.indexOf(a.touch) - TOUCH_ORDER.indexOf(b.touch);
});

const notouchDeltasR1 = {};
const notouchDeltasR2 = {};

for (const { scenario, size, touch, key } of cells) {
  const off1 = off1ByCell.get(key);
  const on1 = on1ByCell.get(key);
  const off2 = off2ByCell.get(key);
  const on2 = on2ByCell.get(key);
  if (off1 === undefined || on1 === undefined) {
    throw new Error(`missing run-1 cell for ${key}`);
  }
  if (off2 === undefined || on2 === undefined) {
    throw new Error(`missing run-2 cell for ${key}`);
  }
  const d1 = pctDelta(off1, on1);
  const d2 = pctDelta(off2, on2);
  deltaLines.push(
    [
      scenario,
      size,
      touch,
      off1.toFixed(1),
      on1.toFixed(1),
      `${d1.toFixed(2)}%`,
      off2.toFixed(1),
      on2.toFixed(1),
      `${d2.toFixed(2)}%`,
      landingCommit,
    ].join(","),
  );
  if (scenario === "virgin" && touch === "notouch") {
    notouchDeltasR1[size] = d1;
    notouchDeltasR2[size] = d2;
  }
}

// --- Hard assertion (CLAUDE.md R30-9 point 6): the four `notouch` virgin
// percentage deltas are this report's headline numbers (§3.1, bolded). A
// wrong number here must FAIL THIS SCRIPT, not just get hand-typed into
// Markdown. Reference values are the already-published report §3.1 figures,
// independently re-verified against the raw logs by the Round-31 review
// (docs/reviews/2026-07-31-r31-full-review.md §4.4) before this check was
// written.
const EXPECTED_NOTOUCH_PCT = {
  "4k": { r1: -89.0, r2: -89.5 },
  "16k": { r1: -95.0, r2: -96.9 },
  "64k": { r1: -98.6, r2: -99.0 },
  "128k": { r1: -98.6, r2: -98.4 },
};
const TOLERANCE_PCT = 0.1; // published values are rounded to 1 decimal place

for (const size of SIZE_ORDER) {
  const expected = EXPECTED_NOTOUCH_PCT[size];
  const got1 = notouchDeltasR1[size];
  const got2 = notouchDeltasR2[size];
  if (got1 === undefined || got2 === undefined) {
    throw new Error(`notouch/virgin cell missing for size=${size}`);
  }
  if (Math.abs(got1 - expected.r1) > TOLERANCE_PCT) {
    throw new Error(
      `HEADLINE MISMATCH: virgin/${size}/notouch run1 delta computed as ${got1.toFixed(2)}% but published report says ${expected.r1}% (tolerance ${TOLERANCE_PCT}pp) -- report/raw-log disagreement, do not publish until reconciled`,
    );
  }
  if (Math.abs(got2 - expected.r2) > TOLERANCE_PCT) {
    throw new Error(
      `HEADLINE MISMATCH: virgin/${size}/notouch run2 delta computed as ${got2.toFixed(2)}% but published report says ${expected.r2}% (tolerance ${TOLERANCE_PCT}pp) -- report/raw-log disagreement, do not publish until reconciled`,
    );
  }
}

const outPath = new URL(
  "docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv",
  ROOT,
);
writeFileSync(outPath, lines.join("\n") + "\n");

const deltaOutPath = new URL(
  "docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_deltas.csv",
  ROOT,
);
writeFileSync(deltaOutPath, deltaLines.join("\n") + "\n");

// CORRECTED 2026-07-31 (P2-1): the 4 retention rows now go to their own file,
// under RETENTION_HEADER, instead of being interleaved into the 24-column
// virgin/recycled file under the wrong header.
const retentionOutPath = new URL(
  "docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_retention.csv",
  ROOT,
);
writeFileSync(retentionOutPath, retentionLines.join("\n") + "\n");

console.error(
  `wrote ${lines.length - 1} data rows to docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_summary.csv (landing_commit=${landingCommit})`,
);
console.error(
  `wrote ${deltaLines.length - 1} derived-delta rows to docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_deltas.csv`,
);
console.error(
  `wrote ${retentionLines.length - 1} retention rows to docs/perf/R31_0_VIRGIN_ZERO_SKIP_PRODUCTION_LAYER_GATE_retention.csv`,
);
console.error(
  `headline check PASSED: all 4 notouch/virgin percentage deltas (run1 and run2) match the published report within ${TOLERANCE_PCT}pp`,
);
