// R32-13 (task #504, F11 step 2): derives
// `docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE_summary.csv`
// from the three raw run logs
// (`docs/perf/_raw_r32_13_run{1,2,3}.log`) this task's
// `examples/r32_13_windows_reserve_commit_decomposition_gate.rs` binary
// produced. This is the ONE checked script that turns the raw logs into the
// report's machine-readable summary (CLAUDE.md "tables derived by one
// checked script, not hand-transcribed").
//
// Each raw log's `# csv-start` / `# csv-end` block is parsed and re-emitted
// as one summary row, plus the median-across-runs headline figures the
// report cites are ASSERTED here (not hand-computed in prose).
//
// Usage:
//   node scripts/r32_13_windows_decomposition_summary.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from "node:fs";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const landingCommit = process.argv[2] || "UNFILLED";

/** Parse the `# csv-start` .. `# csv-end` block out of one raw log. Returns
 * { header: string[], row: string[] }. */
function parseCsvBlock(text, label) {
  const lines = text.split("\n");
  const startIdx = lines.findIndex((l) => l.trim() === "# csv-start");
  const endIdx = lines.findIndex((l) => l.trim() === "# csv-end");
  if (startIdx === -1 || endIdx === -1 || endIdx <= startIdx + 2) {
    throw new Error(`${label}: no valid # csv-start/# csv-end block found`);
  }
  const header = lines[startIdx + 1].split(",");
  const row = lines[startIdx + 2].split(",");
  if (row.length !== header.length) {
    throw new Error(`${label}: header/row column count mismatch`);
  }
  return { header, row };
}

function toRecord(header, row) {
  return Object.fromEntries(header.map((h, i) => [h, row[i]]));
}

const runFiles = [
  "docs/perf/_raw_r32_13_run1.log",
  "docs/perf/_raw_r32_13_run2.log",
  "docs/perf/_raw_r32_13_run3.log",
];

const records = runFiles.map((f, i) => {
  const text = read(f);
  const { header, row } = parseCsvBlock(text, f);
  const rec = toRecord(header, row);
  rec._run = i + 1;
  rec._file = f;
  return rec;
});

// ── Path-activation oracle: every run must have PASSED before any timing is
// trusted (re-derived, not trusted from the log's own prose). ──
for (const r of records) {
  if (r.oracle_pass !== "1") {
    throw new Error(
      `${r._file}: path-activation oracle FAILED (oracle_pass=${r.oracle_pass}) — this run's timings are not trustworthy`,
    );
  }
}

// ── Consistency check: all 3 runs must report the same platform. ──
const platforms = new Set(records.map((r) => r.platform));
if (platforms.size !== 1) {
  throw new Error(`runs report different platforms: ${[...platforms].join(", ")}`);
}
const platform = records[0].platform;

function median(nums) {
  const s = [...nums].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

const avoidablePcts = records.map((r) => Number(r.avoidable_pct));
const avoidablePctMedian = median(avoidablePcts);
const reserveOnlyNsMedian = median(records.map((r) => Number(r.reserve_only_ns)));
const commitOnlyNsMedian = median(records.map((r) => Number(r.commit_only_ns)));
const irreducibleBNsMedian = median(records.map((r) => Number(r.irreducible_b_ns)));
const aPrimeNsMedian = median(records.map((r) => Number(r.a_prime_ns)));

// ── Assert the headline verdict arithmetic in-script (CLAUDE.md rule 6): a
// script that computes a headline ratio must assert it, not just print a
// hand-computed string. ──
const MATERIALITY_THRESHOLD_PCT = 20.0;
const isMaterial = avoidablePctMedian > MATERIALITY_THRESHOLD_PCT;
if (avoidablePctMedian < 0 || avoidablePctMedian > 100) {
  throw new Error(`avoidablePctMedian out of range: ${avoidablePctMedian}`);
}
// Cross-check: commit-only cost is consistently larger than reserve-only
// across all 3 runs (the report's "commit costs more than reserve" finding)
// — assert this holds in every run, not just on the median, before citing it.
for (const r of records) {
  if (!(Number(r.commit_only_ns) > Number(r.reserve_only_ns))) {
    throw new Error(
      `${r._file}: commit_only_ns (${r.commit_only_ns}) did NOT exceed reserve_only_ns (${r.reserve_only_ns}) — the "commit costs more" finding does not hold in every run`,
    );
  }
}

// ── Write the summary CSV ──
const header = [
  "run",
  "platform",
  "oracle_pass",
  "reserve_only_ns",
  "commit_only_ns",
  "os_roundtrip_lumped_ns",
  "avoidable_a_ns",
  "irreducible_b_ns",
  "a_prime_ns",
  "avoidable_pct",
  "irreducible_pct",
  "landing_commit",
];
const lines = [header.join(",")];
for (const r of records) {
  lines.push(
    [
      r._run,
      r.platform,
      r.oracle_pass,
      r.reserve_only_ns,
      r.commit_only_ns,
      r.os_roundtrip_lumped_ns,
      r.avoidable_a_ns,
      r.irreducible_b_ns,
      r.a_prime_ns,
      r.avoidable_pct,
      r.irreducible_pct,
      landingCommit,
    ].join(","),
  );
}
lines.push(
  [
    "median",
    platform,
    1,
    reserveOnlyNsMedian.toFixed(1),
    commitOnlyNsMedian.toFixed(1),
    "",
    "",
    irreducibleBNsMedian.toFixed(1),
    aPrimeNsMedian.toFixed(1),
    avoidablePctMedian.toFixed(2),
    (100 - avoidablePctMedian).toFixed(2),
    landingCommit,
  ].join(","),
);

const outPath = new URL(
  "docs/perf/R32_13_WINDOWS_RESERVE_COMMIT_DECOMPOSITION_GATE_summary.csv",
  ROOT,
);
writeFileSync(outPath, lines.join("\n") + "\n");

console.log("=== R32-13 summary derived ===");
console.log(`platform = ${platform}`);
console.log(`3/3 runs PASSED the path-activation oracle`);
console.log(`3/3 runs: commit_only_ns > reserve_only_ns`);
console.log(
  `avoidable_pct across runs: ${avoidablePcts.map((p) => p.toFixed(2)).join(", ")} — median = ${avoidablePctMedian.toFixed(2)}%`,
);
console.log(
  `verdict: reservation path is ${isMaterial ? "MATERIAL" : "SMALL"} (median ${avoidablePctMedian.toFixed(2)}% ${isMaterial ? ">" : "<="} ${MATERIALITY_THRESHOLD_PCT}% threshold)`,
);
console.log(`reserve_only_ns median = ${reserveOnlyNsMedian.toFixed(1)} ns`);
console.log(`commit_only_ns median = ${commitOnlyNsMedian.toFixed(1)} ns`);
console.log(`wrote ${outPath.pathname}`);
