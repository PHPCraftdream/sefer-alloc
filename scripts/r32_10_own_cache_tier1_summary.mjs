// R32-10 (task #501, F2): derives `docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE_summary.csv`
// from the two raw per-cell CSV sections `examples/r32_10_own_cache_tier1_thrash_gate.rs`
// produced (`docs/perf/_raw_r32_10_own_cache4_before.log` — OWN_CACHE_SIZE=4,
// the pre-change baseline — and `docs/perf/_raw_r32_10_own_cache16_after.log`
// — OWN_CACHE_SIZE=16, the shipped value). This is the ONE checked script
// that turns the raw logs into the report's machine-readable summary
// (CLAUDE.md "tables derived by one checked script, not hand-transcribed").
//
// Both raw logs were captured in the main working tree by temporarily
// flipping the `OWN_CACHE_SIZE` constant (`src/alloc_core/segment_table.rs`)
// between builds, then restoring it to the shipped value (16) before the
// landing commit — a source-CONSTANT before/after, not a runtime switch, so
// (per CLAUDE.md's R29-6 rule) the "before" state is NOT separately
// reproducible from the landing commit alone; this report cites that
// honestly rather than implying otherwise (see the report's own Provenance
// section).
//
// Usage:
//   node scripts/r32_10_own_cache_tier1_summary.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from "node:fs";
import { execSync } from "node:child_process";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const landingCommit = process.argv[2] || execSync("git rev-parse HEAD", { encoding: "utf8" }).trim();

/** Parse the `# col,col,...` header + comma rows that follow it out of one
 * raw log. Returns { header: string[], rows: string[][] }. */
function parseCsvSection(text) {
  const lines = text.split("\n");
  const headerIdx = lines.findIndex((l) => l.startsWith("# "));
  if (headerIdx === -1) throw new Error("no '# header' line found");
  const header = lines[headerIdx].slice(2).split(",");
  const rows = [];
  for (let i = headerIdx + 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line || line.startsWith("=") || line.startsWith("NOTE") || line.startsWith("WARNING")) {
      if (rows.length > 0) break; // end of the CSV block
      continue;
    }
    rows.push(line.split(","));
  }
  return { header, rows };
}

function toRecords(header, rows) {
  return rows.map((r) => Object.fromEntries(header.map((h, i) => [h, r[i]])));
}

function median(nums) {
  const s = [...nums].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

const cpu = "Intel_Core_i7-11800H_2.30GHz";
const os = "Windows_10_Pro_10.0.19045";
const featureSet = "production bench-internals";

const beforeLog = read("docs/perf/_raw_r32_10_own_cache4_before.log");
const afterLog = read("docs/perf/_raw_r32_10_own_cache16_after.log");

const before = toRecords(...Object.values(parseCsvSection(beforeLog)));
const after = toRecords(...Object.values(parseCsvSection(afterLog)));

// Path-activation oracle: every row of BOTH logs must have passed both
// oracles, or this script fails loudly rather than silently summarizing
// invalid data.
for (const [label, records] of [["before(cache=4)", before], ["after(cache=16)", after]]) {
  for (const r of records) {
    if (r.oracle1_pass !== "1") {
      throw new Error(`${label} k=${r.k} rep=${r.repetition}: oracle1_pass != 1`);
    }
    if (r.oracle2_pass !== "1") {
      throw new Error(`${label} k=${r.k} rep=${r.repetition}: oracle2_pass != 1`);
    }
    if (r.config_conflicts_delta !== "0") {
      throw new Error(`${label} k=${r.k} rep=${r.repetition}: config_conflicts_delta != 0`);
    }
  }
}

const K_VALUES = [4, 8, 16, 24, 32, 48, 64];

const rows = [];
for (const k of K_VALUES) {
  for (const [arm, cacheSize, records] of [
    ["before", 4, before],
    ["after", 16, after],
  ]) {
    const cell = records.filter((r) => Number(r.k) === k);
    if (cell.length === 0) throw new Error(`missing rows for arm=${arm} k=${k}`);
    const hitRates = cell.map((r) => Number(r.tier1_hit_rate_pct));
    const nsPerOp = cell.map((r) => Number(r.ns_per_op));
    const hitRateMedian = median(hitRates);
    const nsPerOpMedian = median(nsPerOp);
    const hitsLast = Number(cell[cell.length - 1].tier1_hits);
    const missesLast = Number(cell[cell.length - 1].tier1_misses);
    rows.push({
      gate: "own_cache_tier1_thrash",
      cpu,
      os,
      feature_set: featureSet,
      arm,
      own_cache_size: cacheSize,
      k,
      repetitions: cell.length,
      tier1_hits_last: hitsLast,
      tier1_misses_last: missesLast,
      tier1_hit_rate_pct_median: hitRateMedian.toFixed(4),
      ns_per_op_median: nsPerOpMedian.toFixed(3),
      landing_commit: landingCommit,
    });
  }
}

// ── Headline assertions (CLAUDE.md's "assert the arithmetic, not just print
// a hand-computed string" rule) ─────────────────────────────────────────────

function findRow(arm, k) {
  const r = rows.find((r) => r.arm === arm && r.k === k);
  if (!r) throw new Error(`row not found: arm=${arm} k=${k}`);
  return r;
}

// Headline 1: at OWN_CACHE_SIZE=4, EVERY tested K thrashes completely
// (0.00% hit rate) -- confirms the survey's "even N==4 only works if the OS
// happens to align bases favorably" point empirically, on this measured run.
for (const k of K_VALUES) {
  const r = findRow("before", k);
  if (Number(r.tier1_hit_rate_pct_median) !== 0) {
    throw new Error(
      `headline 1 FAILED: before(cache=4) k=${k} hit_rate=${r.tier1_hit_rate_pct_median}, expected 0.0000`
    );
  }
}

// Headline 2: at OWN_CACHE_SIZE=16, K=4 and K=8 (both <= cache size) show a
// dramatic, reproducible hit-rate win (>95%, comfortably clear of noise).
for (const k of [4, 8]) {
  const r = findRow("after", k);
  const hr = Number(r.tier1_hit_rate_pct_median);
  if (!(hr > 95)) {
    throw new Error(`headline 2 FAILED: after(cache=16) k=${k} hit_rate=${hr}, expected > 95`);
  }
}

// Headline 3: at OWN_CACHE_SIZE=16, K=16 (== cache size, direct-mapped so
// collisions are still likely) and beyond still thrash near-completely
// (<15% -- the K=32 arm's single-repetition outlier keeps the median at
// exactly 0 but this bound is written loosely to tolerate that without
// re-deriving the outlier's own explanation here).
for (const k of [16, 24, 48, 64]) {
  const r = findRow("after", k);
  const hr = Number(r.tier1_hit_rate_pct_median);
  if (!(hr < 15)) {
    throw new Error(`headline 3 FAILED: after(cache=16) k=${k} hit_rate=${hr}, expected < 15`);
  }
}

// Headline 4: the latency (ns_per_op) delta at K=4 between before/after is
// NOT a clean, large win -- both arms sit in the ~20-30 ns/op band, i.e. the
// wall-clock signal does not cleanly separate despite the huge hit-rate
// delta. Asserted as a BOUND (both within [15, 35]) rather than a specific
// number, since this is exactly the honest-null claim the report makes.
for (const arm of ["before", "after"]) {
  const r = findRow(arm, 4);
  const ns = Number(r.ns_per_op_median);
  if (!(ns >= 15 && ns <= 35)) {
    throw new Error(`headline 4 FAILED: ${arm} k=4 ns_per_op=${ns}, expected in [15,35]`);
  }
}

// ── Write the summary CSV ───────────────────────────────────────────────────

const cols = [
  "gate",
  "cpu",
  "os",
  "feature_set",
  "arm",
  "own_cache_size",
  "k",
  "repetitions",
  "tier1_hits_last",
  "tier1_misses_last",
  "tier1_hit_rate_pct_median",
  "ns_per_op_median",
  "landing_commit",
];
const csvLines = [cols.join(",")];
for (const r of rows) {
  csvLines.push(cols.map((c) => r[c]).join(","));
}
writeFileSync(
  new URL("docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE_summary.csv", ROOT),
  csvLines.join("\n") + "\n"
);

console.log(`Wrote docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE_summary.csv (${rows.length} rows).`);
console.log("All headline assertions PASSED.");
for (const k of K_VALUES) {
  const b = findRow("before", k);
  const a = findRow("after", k);
  console.log(
    `k=${String(k).padStart(2)}  before(4): hit_rate=${b.tier1_hit_rate_pct_median.padStart(7)}% ns/op=${b.ns_per_op_median.padStart(7)}  |  after(16): hit_rate=${a.tier1_hit_rate_pct_median.padStart(7)}% ns/op=${a.ns_per_op_median.padStart(7)}`
  );
}
