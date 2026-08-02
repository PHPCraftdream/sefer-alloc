// R32-8 (task #499, F9): derives `docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE_summary.csv`
// from the two raw per-child CSV sections this task's two example harnesses
// produced (`docs/perf/_raw_r32_8_clock_read_ab_gate.log` — the confound-free
// isolation gate — and `docs/perf/_raw_r32_8_stride_fix_gate.log` — the
// stride-throttle fix's own before/after validation in the above-headroom
// regime). This is the ONE checked script that turns the raw logs into the
// report's machine-readable summary (CLAUDE.md "tables derived by one
// checked script, not hand-transcribed").
//
// What this measures:
//   1. `_raw_r32_8_clock_read_ab_gate.log` — isolates the raw per-call
//      `Instant::now()` cost inside `AllocCore::maybe_decay_large_cache`
//      (`src/alloc_core/alloc_core_large_cache.rs`) at a FIXED headroom the
//      workload never crosses (guard-real vs guard-forced via
//      `FORCE_DECAY_CLOCK_READ`).
//   2. `_raw_r32_8_stride_fix_gate.log` — validates the stride-throttle FIX's
//      benefit in the regime it targets (workload genuinely above headroom
//      throughout — the `LowHeadroom`/`Trimmed64MiB` regime), comparing the
//      OLD unconditional-clock-read shape (`forced=true`, bypasses the
//      stride) against the NEW stride-throttled shape (`forced=false`).
//
// Both raw logs were captured in the main working tree (no worktree
// isolation needed — this is a runtime-instrument A/B via a process-wide
// `bench-internals`-gated switch, not a source-diff before/after) at the
// commit this task lands on (`landing_commit`, filled by a same-task or
// follow-up commit per this project's placeholder convention).
//
// Usage:
//   node scripts/r32_8_decay_clock_read_summary.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from "node:fs";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const landingCommit = process.argv[2] || "UNFILLED";

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

// ── Gate 1: confound-free isolation A/B ─────────────────────────────────────

const isolationLog = read("docs/perf/_raw_r32_8_clock_read_ab_gate.log");
const { header: h1, rows: r1 } = parseCsvSection(isolationLog);
const isolationRecords = toRecords(h1, r1);

const CYCLES_ISOLATION = 200000;

const guardReal = isolationRecords.filter((r) => r.arm === "guard-real");
const guardForced = isolationRecords.filter((r) => r.arm === "guard-forced");

for (const r of isolationRecords) {
  if (r.oracle_pass !== "1") {
    throw new Error(`isolation gate: arm ${r.arm} rep ${r.repetition} failed the path-activation oracle`);
  }
  if (r.config_conflicts_delta !== "0") {
    throw new Error(`isolation gate: nonzero config_conflicts_delta`);
  }
}
// Path-activation oracle assertions (re-derived, not trusted from the log's
// own prose): guard-real's guard_passed_delta must be 0 (fixed headroom the
// workload never crosses); guard-forced's must equal expected_calls exactly.
for (const r of guardReal) {
  if (Number(r.guard_passed_delta) !== 0) {
    throw new Error(`isolation gate: guard-real rep ${r.repetition} has nonzero guard_passed_delta`);
  }
}
for (const r of guardForced) {
  if (Number(r.guard_passed_delta) !== Number(r.expected_calls)) {
    throw new Error(`isolation gate: guard-forced rep ${r.repetition} guard_passed_delta != expected_calls`);
  }
}

const realElapsedMedian = median(guardReal.map((r) => Number(r.elapsed_ns)));
const forcedElapsedMedian = median(guardForced.map((r) => Number(r.elapsed_ns)));
const isolationDeltaNsPerCycle = (forcedElapsedMedian - realElapsedMedian) / CYCLES_ISOLATION;
const isolationDeltaNsPerCall = isolationDeltaNsPerCycle / 2; // 2 calls/cycle

// ── Gate 2: stride-throttle fix validation (above-headroom regime) ─────────

const strideLog = read("docs/perf/_raw_r32_8_stride_fix_gate.log");
const { header: h2, rows: r2 } = parseCsvSection(strideLog);
const strideRecords = toRecords(h2, r2);

const CYCLES_STRIDE = 200000;

const oldShape = strideRecords.filter((r) => r.arm === "old-shape");
const newShape = strideRecords.filter((r) => r.arm === "new-shape");

for (const r of strideRecords) {
  if (r.oracle_pass !== "1") {
    throw new Error(`stride-fix gate: arm ${r.arm} rep ${r.repetition} failed the path-activation oracle`);
  }
  if (r.config_conflicts_delta !== "0") {
    throw new Error(`stride-fix gate: nonzero config_conflicts_delta`);
  }
  if (r.stayed_above_headroom !== "1") {
    throw new Error(`stride-fix gate: arm ${r.arm} rep ${r.repetition} did not stay above headroom`);
  }
}
for (const r of oldShape) {
  if (Number(r.guard_passed_delta) !== Number(r.expected_calls)) {
    throw new Error(`stride-fix gate: old-shape rep ${r.repetition} guard_passed_delta != expected_calls`);
  }
}
for (const r of newShape) {
  const ratio = Number(r.expected_calls) / Number(r.guard_passed_delta);
  if (!(ratio > 4)) {
    throw new Error(`stride-fix gate: new-shape rep ${r.repetition} reduction ratio ${ratio} not > 4x`);
  }
}

const oldElapsedMedian = median(oldShape.map((r) => Number(r.elapsed_ns)));
const newElapsedMedian = median(newShape.map((r) => Number(r.elapsed_ns)));
const strideDeltaNsPerCycle = (oldElapsedMedian - newElapsedMedian) / CYCLES_STRIDE;
const strideDeltaNsPerCall = strideDeltaNsPerCycle / 2;
const stridePctReduction = (100 * (oldElapsedMedian - newElapsedMedian)) / oldElapsedMedian;
const guardPassedReductionRatio =
  Number(oldShape[0].expected_calls) / Number(newShape[0].guard_passed_delta);

// ── Assert the headline arithmetic (CLAUDE.md rule: a script that computes
// a headline ratio must assert it, not just print a hand-computed string) ──

if (!(isolationDeltaNsPerCall > 0)) {
  throw new Error("isolation gate: expected a positive per-call clock-read cost");
}
if (!(strideDeltaNsPerCall > 0)) {
  throw new Error("stride-fix gate: expected a positive old-shape-minus-new-shape delta");
}
if (!(stridePctReduction > 0 && stridePctReduction < 100)) {
  throw new Error("stride-fix gate: pct reduction out of sane range");
}

// ── Write the summary CSV ───────────────────────────────────────────────────

const HEADER =
  "gate,cpu,os,feature_set,arm,repetitions,cycles,elapsed_ns_median,ns_per_cycle,ns_per_call,guard_passed_delta_first_rep,expected_calls,landing_commit";

const META = {
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045",
  feature_set: "production alloc-stats bench-internals",
};

const lines = [HEADER];
lines.push(
  [
    "isolation",
    META.cpu,
    META.os,
    META.feature_set,
    "guard-real",
    guardReal.length,
    CYCLES_ISOLATION,
    realElapsedMedian,
    (realElapsedMedian / CYCLES_ISOLATION).toFixed(2),
    (realElapsedMedian / CYCLES_ISOLATION / 2).toFixed(2),
    guardReal[0].guard_passed_delta,
    guardReal[0].expected_calls,
    landingCommit,
  ].join(","),
);
lines.push(
  [
    "isolation",
    META.cpu,
    META.os,
    META.feature_set,
    "guard-forced",
    guardForced.length,
    CYCLES_ISOLATION,
    forcedElapsedMedian,
    (forcedElapsedMedian / CYCLES_ISOLATION).toFixed(2),
    (forcedElapsedMedian / CYCLES_ISOLATION / 2).toFixed(2),
    guardForced[0].guard_passed_delta,
    guardForced[0].expected_calls,
    landingCommit,
  ].join(","),
);
lines.push(
  [
    "stride_fix",
    META.cpu,
    META.os,
    META.feature_set,
    "old-shape",
    oldShape.length,
    CYCLES_STRIDE,
    oldElapsedMedian,
    (oldElapsedMedian / CYCLES_STRIDE).toFixed(2),
    (oldElapsedMedian / CYCLES_STRIDE / 2).toFixed(2),
    oldShape[0].guard_passed_delta,
    oldShape[0].expected_calls,
    landingCommit,
  ].join(","),
);
lines.push(
  [
    "stride_fix",
    META.cpu,
    META.os,
    META.feature_set,
    "new-shape",
    newShape.length,
    CYCLES_STRIDE,
    newElapsedMedian,
    (newElapsedMedian / CYCLES_STRIDE).toFixed(2),
    (newElapsedMedian / CYCLES_STRIDE / 2).toFixed(2),
    newShape[0].guard_passed_delta,
    newShape[0].expected_calls,
    landingCommit,
  ].join(","),
);

const outPath = new URL(
  "docs/perf/R32_8_LARGE_CACHE_DECAY_CLOCK_READ_GATE_summary.csv",
  ROOT,
);
writeFileSync(outPath, lines.join("\n") + "\n");

console.log("=== R32-8 summary derived ===");
console.log(`isolation: guard-real median=${realElapsedMedian}ns guard-forced median=${forcedElapsedMedian}ns`);
console.log(`isolation: delta = ${isolationDeltaNsPerCycle.toFixed(2)} ns/cycle = ${isolationDeltaNsPerCall.toFixed(2)} ns/call`);
console.log(`stride_fix: old-shape median=${oldElapsedMedian}ns new-shape median=${newElapsedMedian}ns`);
console.log(`stride_fix: delta = ${strideDeltaNsPerCycle.toFixed(2)} ns/cycle = ${strideDeltaNsPerCall.toFixed(2)} ns/call (${stridePctReduction.toFixed(1)}% reduction)`);
console.log(`stride_fix: guard_passed_delta reduction ratio (old/new) = ${guardPassedReductionRatio.toFixed(1)}x`);
console.log(`wrote ${outPath.pathname}`);
