// R34-11 (task #530): derives `docs/perf/R34_11_CATCHUP_DECAY_GATE_summary.csv`
// AND the report's markdown tables from the raw log in
// `docs/perf/_raw_r34_11_catchup_decay_gate.log`.
//
// This is the ONE checked script that turns the raw log into the report's
// machine-readable summary + tables (CLAUDE.md "tables derived by one checked
// script, not hand-transcribed"). It ASSERTS the path-activation oracle and
// config-resolution invariants — a wrong invariant is a THROW, not a printed
// claim.
//
// Two regimes in one log:
// - sparse_* lines: per-interval time series (allocfree, events {1,2,4,8} × 40
//   intervals × 2 arms). Reports peak/final gap, persistence, total released,
//   catch-up oracle.
// - throughput_ts lines: per-child metrics (200K cycles × 2 arms × 7 reps).
//   Reports median ns/cycle, guard_passed_delta, % benefit.
//
// Usage:
//   node scripts/r34_11_catchup_decay_summary.mjs <source_identity>
//
// <source_identity> is the immutable source identity captured BEFORE
// measurement (a `git write-tree` tree SHA per CLAUDE.md's R29-6 rule).

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const sourceIdentity = process.argv[2] || (() => {
  throw new Error("usage: node scripts/r34_11_catchup_decay_summary.mjs <source_identity>");
})();

const MIB = 1024 * 1024;
const SEGMENT_BYTES = 4 * 1024 * 1024;

const META = {
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045",
  feature_set: "production alloc-stats bench-internals internals",
  source_identity: sourceIdentity,
  base_commit: execSync("git rev-parse HEAD", { cwd: new URL(".", import.meta.url), encoding: "utf8" }).trim(),
};

// ── Parse the raw log ───────────────────────────────────────────────────────

const log = read("docs/perf/_raw_r34_11_catchup_decay_gate.log");

function parseResultLine(line) {
  const rest = line.slice("RESULT ".length);
  const toks = rest.split(/\s+/);
  const rec = {};
  for (const t of toks) {
    const eq = t.indexOf("=");
    if (eq === -1) continue;
    rec[t.slice(0, eq)] = t.slice(eq + 1);
  }
  return rec;
}

const sparseTs = [];
const sparseConfig = [];
const sparseOracle = [];
const tpTs = [];

for (const line of log.split("\n")) {
  if (!line.startsWith("RESULT ")) continue;
  const rec = parseResultLine(line);
  if (rec.sparse_ts === "1") sparseTs.push(rec);
  else if (rec.sparse_config === "1") sparseConfig.push(rec);
  else if (rec.sparse_oracle === "1") sparseOracle.push(rec);
  else if (rec.throughput_ts === "1") tpTs.push(rec);
}

// ── Validate sparse invariants ──────────────────────────────────────────────

for (const c of sparseConfig) {
  if (c.config_conflicts_delta !== "0") {
    throw new Error(`sparse child ${c.events}/${c.arm}: config_conflicts_delta=${c.config_conflicts_delta}`);
  }
  if (c.verified_headroom !== c.headroom_bytes) {
    throw new Error(`sparse child ${c.events}/${c.arm}: verified_headroom != headroom_bytes`);
  }
  if (c.verified_interval_ms !== c.decay_interval_ms) {
    throw new Error(`sparse child ${c.events}/${c.arm}: verified_interval_ms != decay_interval_ms`);
  }
  if (c.headroom_crossed !== "1") {
    throw new Error(`sparse child ${c.events}/${c.arm}: headroom_crossed != 1`);
  }
  if (c.unthrottled_read !== "1") {
    throw new Error(`sparse child ${c.events}/${c.arm}: unthrottled_read != 1`);
  }
  // Catch-up oracle: throttled arm must have released MORE segments than
  // clock reads (guard_passed_delta), proving the catch-up loop fired
  // multiple steps per read.
  if (c.arm === "throttled" && c.catchup_active !== "1") {
    throw new Error(`sparse child ${c.events}/${c.arm}: catchup_active != 1 (released_delta=${c.released_delta} guard_passed_delta=${c.guard_passed_delta})`);
  }
}
for (const o of sparseOracle) {
  if (o.oracle_pass !== "1") {
    throw new Error(`sparse child ${o.events}/${o.arm}: oracle_pass != 1`);
  }
}

// ── Validate throughput invariants ──────────────────────────────────────────

for (const t of tpTs) {
  if (t.config_conflicts_delta !== "0") {
    throw new Error(`throughput child forced=${t.forced} rep=${t.rep}: config_conflicts_delta != 0`);
  }
  if (t.stayed_above_headroom !== "1") {
    throw new Error(`throughput child forced=${t.forced} rep=${t.rep}: stayed_above_headroom != 1`);
  }
  if (t.oracle_pass !== "1") {
    throw new Error(`throughput child forced=${t.forced} rep=${t.rep}: oracle_pass != 1`);
  }
}

// ── Sparse regime: compute per-events gap time series ──────────────────────

const EVENTS = [1, 2, 4, 8];
const INTERVALS = Number(sparseConfig[0]?.intervals || 40);

const configByKey = new Map();
for (const c of sparseConfig) {
  configByKey.set(`${c.events}|${c.arm}`, c);
}

const sparseSummary = [];

for (const events of EVENTS) {
  const cfgT = configByKey.get(`${events}|throttled`);
  const cfgU = configByKey.get(`${events}|unthrottled`);
  if (!cfgT || !cfgU) {
    throw new Error(`missing config row for events=${events}`);
  }

  const tsT = sparseTs
    .filter((r) => Number(r.events) === events && r.arm === "throttled")
    .sort((a, b) => Number(a.interval) - Number(b.interval));
  const tsU = sparseTs
    .filter((r) => Number(r.events) === events && r.arm === "unthrottled")
    .sort((a, b) => Number(a.interval) - Number(b.interval));

  if (tsT.length !== INTERVALS || tsU.length !== INTERVALS) {
    throw new Error(`events=${events}: expected ${INTERVALS} intervals, got T=${tsT.length} U=${tsU.length}`);
  }

  let peakGapBytes = 0;
  let peakGapInterval = -1;
  let intervalsAtGte3 = 0;
  let intervalsAtGte4 = 0;
  for (let i = 0; i < INTERVALS; i++) {
    const usedT = Number(tsT[i].used_post);
    const usedU = Number(tsU[i].used_post);
    const gap = usedT - usedU;
    if (gap > peakGapBytes) {
      peakGapBytes = gap;
      peakGapInterval = i;
    }
    if (gap >= 3 * SEGMENT_BYTES) intervalsAtGte3++;
    if (gap >= 4 * SEGMENT_BYTES) intervalsAtGte4++;
  }

  const lastT = tsT[INTERVALS - 1];
  const lastU = tsU[INTERVALS - 1];
  const finalGapBytes = Number(lastT.used_post) - Number(lastU.used_post);

  const guardDeltaT = Number(cfgT.guard_passed_delta);
  const guardDeltaU = Number(cfgU.guard_passed_delta);
  const releasedDeltaT = Number(cfgT.released_delta);
  const releasedDeltaU = Number(cfgU.released_delta);

  sparseSummary.push({
    events,
    intervals: INTERVALS,
    peak_gap_bytes: peakGapBytes,
    peak_gap_mib: (peakGapBytes / MIB).toFixed(2),
    peak_gap_segments: (peakGapBytes / SEGMENT_BYTES).toFixed(2),
    peak_gap_interval: peakGapInterval,
    final_gap_bytes: finalGapBytes,
    final_gap_mib: (finalGapBytes / MIB).toFixed(2),
    final_gap_segments: (finalGapBytes / SEGMENT_BYTES).toFixed(2),
    intervals_at_gte3: intervalsAtGte3,
    intervals_at_gte3_pct: ((intervalsAtGte3 / INTERVALS) * 100).toFixed(1),
    intervals_at_gte4: intervalsAtGte4,
    intervals_at_gte4_pct: ((intervalsAtGte4 / INTERVALS) * 100).toFixed(1),
    throttled_final_used_mib: (Number(lastT.used_post) / MIB).toFixed(2),
    unthrottled_final_used_mib: (Number(lastU.used_post) / MIB).toFixed(2),
    throttled_guard_delta: guardDeltaT,
    unthrottled_guard_delta: guardDeltaU,
    throttled_released_delta: releasedDeltaT,
    unthrottled_released_delta: releasedDeltaU,
  });
}

// ── Throughput regime: compute median ns/cycle per arm ─────────────────────

// TP_CYCLES is 200_000, mirrored from the harness (examples/r34_11_catchup_decay_gate.rs).
const TP_CYCLES_CONST = 200_000;

function median(v) {
  if (v.length === 0) return 0;
  const s = [...v].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

const tpSummary = {};
for (const forced of ["0", "1"]) {
  const cell = tpTs.filter((t) => t.forced === forced);
  const elapsed = cell.map((t) => Number(t.elapsed_ns));
  const med = median(elapsed);
  tpSummary[forced] = {
    arm: forced === "1" ? "old-shape" : "new-shape",
    median_elapsed_ns: med,
    ns_per_cycle: med / TP_CYCLES_CONST,
    guard_passed_delta: Number(cell[0]?.guard_passed_delta || 0),
    expected_calls: Number(cell[0]?.expected_calls || 0),
    reps: cell.length,
  };
}

const tpNewShape = tpSummary["0"];
const tpOldShape = tpSummary["1"];
const tpDeltaPerCycle = tpOldShape.ns_per_cycle - tpNewShape.ns_per_cycle;
const tpDeltaPerCall = tpDeltaPerCycle / 2.0; // 2 calls per cycle
const tpPct = tpOldShape.ns_per_cycle > 0
  ? (100.0 * (tpOldShape.ns_per_cycle - tpNewShape.ns_per_cycle) / tpOldShape.ns_per_cycle)
  : 0.0;

// ── Write the summary CSV ───────────────────────────────────────────────────

const csvParts = [];

// Sparse section
csvParts.push("# sparse regime (allocfree, events × 40 intervals × 2 arms)");
const sparseCols = [
  "events", "intervals",
  "peak_gap_bytes", "peak_gap_mib", "peak_gap_segments", "peak_gap_interval",
  "final_gap_bytes", "final_gap_mib", "final_gap_segments",
  "intervals_at_gte3", "intervals_at_gte3_pct",
  "intervals_at_gte4", "intervals_at_gte4_pct",
  "throttled_final_used_mib", "unthrottled_final_used_mib",
  "throttled_guard_delta", "unthrottled_guard_delta",
  "throttled_released_delta", "unthrottled_released_delta",
];
csvParts.push(sparseCols.join(","));
for (const s of sparseSummary) {
  csvParts.push(sparseCols.map((c) => s[c]).join(","));
}

csvParts.push("");
csvParts.push("# throughput regime (200K cycles × 2 arms × 7 reps, median)");
const tpCols = [
  "arm", "median_elapsed_ns", "ns_per_cycle", "guard_passed_delta",
  "expected_calls", "reps", "delta_ns_per_cycle", "delta_ns_per_call", "pct_benefit",
];
csvParts.push(tpCols.join(","));
for (const forced of ["0", "1"]) {
  const s = tpSummary[forced];
  csvParts.push(tpCols.map((c) => {
    if (c === "delta_ns_per_cycle") return tpDeltaPerCycle.toFixed(4);
    if (c === "delta_ns_per_call") return tpDeltaPerCall.toFixed(4);
    if (c === "pct_benefit") return tpPct.toFixed(1);
    return s[c];
  }).join(","));
}

csvParts.push("");
csvParts.push(`# meta: cpu=${META.cpu}, os=${META.os}, feature_set=${META.feature_set}`);
csvParts.push(`# meta: source_identity=${META.source_identity}, base_commit=${META.base_commit}`);

const outPath = new URL("docs/perf/R34_11_CATCHUP_DECAY_GATE_summary.csv", ROOT);
writeFileSync(outPath, csvParts.join("\n") + "\n");

// ── Print report tables ─────────────────────────────────────────────────────

function miB(bytes) { return (bytes / MIB).toFixed(2); }
function seg(bytes) { return (bytes / SEGMENT_BYTES).toFixed(2); }

console.log("=== R34-11 catch-up decay summary derived ===");
console.log(`source: docs/perf/_raw_r34_11_catchup_decay_gate.log`);
console.log(`source_identity (captured before measurement): ${META.source_identity}`);
console.log(`base_commit (HEAD at derive time): ${META.base_commit}`);
console.log(`invariants: ${sparseConfig.length} sparse config, ${sparseOracle.length} sparse oracle, ${tpTs.length} throughput — all passed`);
console.log();

// Sparse Table 1: peak/final gap per events
console.log("## Sparse Table 1 — peak retention gap (throttled − unthrottled used_post), allocfree");
console.log();
console.log("| events/interval | peak gap (MiB) | peak gap (segments) | peak @ interval | final gap (MiB) | final gap (segments) | throttled final (MiB) | unthrottled final (MiB) | throttled released Δ | unthrottled released Δ |");
console.log("|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|");
for (const s of sparseSummary) {
  console.log(
    `| ${s.events} | ${s.peak_gap_mib} | ${s.peak_gap_segments} | ${s.peak_gap_interval} | ${s.final_gap_mib} | ${s.final_gap_segments} | ${s.throttled_final_used_mib} | ${s.unthrottled_final_used_mib} | ${s.throttled_released_delta} | ${s.unthrottled_released_delta} |`,
  );
}
console.log();

// Sparse Table 2: persistence
console.log("## Sparse Table 2 — gap persistence (intervals at ≥3 and ≥4 segments)");
console.log();
console.log("| events/interval | intervals at ≥3 seg | % of run | intervals at ≥4 seg | % of run |");
console.log("|---:|---:|---:|---:|---:|");
for (const s of sparseSummary) {
  console.log(
    `| ${s.events} | ${s.intervals_at_gte3}/${INTERVALS} | ${s.intervals_at_gte3_pct}% | ${s.intervals_at_gte4}/${INTERVALS} | ${s.intervals_at_gte4_pct}% |`,
  );
}
console.log();

// Sparse Table 3: events=1 full per-interval trace
console.log(`## Sparse Table 3 — events=1/interval: per-interval time series`);
console.log();
const tsT1 = sparseTs
  .filter((r) => Number(r.events) === 1 && r.arm === "throttled")
  .sort((a, b) => Number(a.interval) - Number(b.interval));
const tsU1 = sparseTs
  .filter((r) => Number(r.events) === 1 && r.arm === "unthrottled")
  .sort((a, b) => Number(a.interval) - Number(b.interval));
const cfgT1 = configByKey.get("1|throttled");
const cfgU1 = configByKey.get("1|unthrottled");
console.log("| interval | throttled used (MiB) | unthrottled used (MiB) | gap (MiB) | gap (segments) | throttled released Δ | unthrottled released Δ |");
console.log("|---:|---:|---:|---:|---:|---:|---:|");
for (let i = 0; i < INTERVALS; i++) {
  const usedT = Number(tsT1[i].used_post);
  const usedU = Number(tsU1[i].used_post);
  const gap = usedT - usedU;
  const relT = Number(tsT1[i].released_cum) - Number(cfgT1.released_baseline);
  const relU = Number(tsU1[i].released_cum) - Number(cfgU1.released_baseline);
  console.log(`| ${i} | ${miB(usedT)} | ${miB(usedU)} | ${miB(gap)} | ${seg(gap)} | ${relT} | ${relU} |`);
}
console.log();

// Throughput Table
console.log("## Throughput Table — ns/cycle median (200K cycles × 7 reps), R32-8 benefit preservation");
console.log();
console.log("| arm | ns/cycle (median) | guard_passed / expected | reps | oracle |");
console.log("|---|---:|---:|---:|---|");
for (const forced of ["0", "1"]) {
  const s = tpSummary[forced];
  console.log(
    `| ${s.arm} | ${s.ns_per_cycle.toFixed(2)} | ${s.guard_passed_delta}/${s.expected_calls} | ${s.reps} | PASS |`,
  );
}
console.log();
console.log(
  `HEADLINE: old-shape − new-shape = ${tpDeltaPerCycle.toFixed(2)} ns/cycle (${tpDeltaPerCall.toFixed(2)} ns/call, ${tpPct.toFixed(1)}% of old-shape).`,
);
console.log();

console.log(`wrote ${outPath.pathname}`);
