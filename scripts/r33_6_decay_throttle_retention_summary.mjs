// R33-6 (task #511): derives `docs/perf/R33_6_DECAY_THROTTLE_RETENTION_COST_summary.csv`
// from the raw per-child CSV section in
// `docs/perf/_raw_r33_6_decay_throttle_retention_cost_gate.log`.
//
// This is the ONE checked script that turns the raw log into the report's
// machine-readable summary (CLAUDE.md "tables derived by one checked script,
// not hand-transcribed"). It ASSERTS the headline retention-cost numbers and
// the path-activation oracle — a wrong number is a THROW, not a printed claim.
//
// What this measures: the RETENTION COST of R32-8's decay-clock-read stride
// throttle (DECAY_CLOCK_CHECK_STRIDE=64) in the LOW-throughput regime. For
// each (profile, n_ops) cell, compares used_after_ops between the OLD shape
// (forced=true, unthrottled) and the NEW shape (forced=false, stride-throttled).
//
// Usage:
//   node scripts/r33_6_decay_throttle_retention_summary.mjs [landing_commit_sha]

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

// F6 recommendation (R32-2 review): derive the measurement-commit SHA from
// `git rev-parse HEAD` at derive time rather than requiring a hand-passed
// argument or leaving a placeholder that needs a follow-up commit to fill.
const landingCommit = process.argv[2]
  || execSync("git rev-parse HEAD", { cwd: new URL(".", import.meta.url), encoding: "utf8" }).trim();

// ── Parse the CSV section from the raw log ─────────────────────────────────

function parseCsvSection(text) {
  const lines = text.split("\n");
  const headerIdx = lines.findIndex((l) => l.startsWith("# "));
  if (headerIdx === -1) throw new Error("no '# header' line found");
  const header = lines[headerIdx].slice(2).split(",");
  const rows = [];
  for (let i = headerIdx + 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line || line.startsWith("=") || line.startsWith("NOTE") || line.startsWith("WARNING")) {
      if (rows.length > 0) break;
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

const log = read("docs/perf/_raw_r33_6_decay_throttle_retention_cost_gate.log");
const { header, rows } = parseCsvSection(log);
const records = toRecords(header, rows);

// ── Validate every row ─────────────────────────────────────────────────────

for (const r of records) {
  if (r.oracle_pass !== "1") {
    throw new Error(`row profile=${r.profile} forced=${r.forced} n_ops=${r.n_ops} rep=${r.repetition}: oracle_pass != 1`);
  }
  if (r.config_conflicts_delta !== "0") {
    throw new Error(`row profile=${r.profile} forced=${r.forced} n_ops=${r.n_ops} rep=${r.repetition}: nonzero config_conflicts_delta`);
  }
  if (r.headroom_crossed !== "1") {
    throw new Error(`row profile=${r.profile} forced=${r.forced} n_ops=${r.n_ops} rep=${r.repetition}: headroom_crossed != 1`);
  }
}

const PROFILES = ["LowHeadroom", "Trimmed64MiB"];
const N_OPS = [1, 8, 32, 63];

// ── Compute per-cell medians and retention cost ────────────────────────────

const summaryRows = [];

for (const profile of PROFILES) {
  for (const nOps of N_OPS) {
    const forcedRows = records.filter(
      (r) => r.profile === profile && Number(r.n_ops) === nOps && r.forced === "1",
    );
    const unforcedRows = records.filter(
      (r) => r.profile === profile && Number(r.n_ops) === nOps && r.forced === "0",
    );

    if (forcedRows.length !== 3 || unforcedRows.length !== 3) {
      throw new Error(`expected 3 reps each for profile=${profile} n_ops=${nOps}, got forced=${forcedRows.length} unforced=${unforcedRows.length}`);
    }

    const medForcedAfter = median(forcedRows.map((r) => Number(r.used_after_ops)));
    const medUnforcedAfter = median(unforcedRows.map((r) => Number(r.used_after_ops)));
    const medForcedBefore = median(forcedRows.map((r) => Number(r.used_before_ops)));
    const medUnforcedBefore = median(unforcedRows.map((r) => Number(r.used_before_ops)));
    const medForcedGuard = median(forcedRows.map((r) => Number(r.guard_passed_delta)));
    const medUnforcedGuard = median(unforcedRows.map((r) => Number(r.guard_passed_delta)));
    const expectedCalls = Number(forcedRows[0].expected_calls);

    // Retention cost = how many MORE bytes the throttled (unforced) arm retains.
    const retentionCostBytes = medUnforcedAfter - medForcedAfter;
    const retentionCostMib = retentionCostBytes / (1024 * 1024);

    // ASSERT: forced arm's guard_passed_delta == expected_calls (every call read the clock).
    if (medForcedGuard !== expectedCalls) {
      throw new Error(`profile=${profile} n_ops=${nOps}: forced guard_passed_delta (${medForcedGuard}) != expected_calls (${expectedCalls})`);
    }

    // ASSERT: unforced arm's guard_passed_delta < expected_calls (stride throttle is reducing reads).
    if (!(medUnforcedGuard < expectedCalls)) {
      throw new Error(`profile=${profile} n_ops=${nOps}: unforced guard_passed_delta (${medUnforcedGuard}) not < expected_calls (${expectedCalls}) — stride throttle not working`);
    }

    // ASSERT: retention cost is non-negative (throttled arm never retains LESS).
    if (retentionCostBytes < 0) {
      throw new Error(`profile=${profile} n_ops=${nOps}: negative retention cost ${retentionCostBytes}`);
    }

    // ASSERT: retention cost is at most one segment (~36 MiB = 37748736 bytes),
    // because decay only fires once per interval and each tick evicts whole segments.
    const ONE_SEGMENT = 37748736; // 36 MiB
    if (retentionCostBytes > ONE_SEGMENT) {
      throw new Error(`profile=${profile} n_ops=${nOps}: retention cost ${retentionCostBytes} exceeds one segment (${ONE_SEGMENT})`);
    }

    summaryRows.push({
      profile,
      n_ops: nOps,
      used_before_ops_mib: (medUnforcedBefore / (1024 * 1024)).toFixed(2),
      forced_used_after_mib: (medForcedAfter / (1024 * 1024)).toFixed(2),
      unforced_used_after_mib: (medUnforcedAfter / (1024 * 1024)).toFixed(2),
      retention_cost_bytes: retentionCostBytes,
      retention_cost_mib: retentionCostMib.toFixed(2),
      forced_guard_delta: medForcedGuard,
      unforced_guard_delta: medUnforcedGuard,
      expected_calls: expectedCalls,
      reps: forcedRows.length,
    });
  }
}

// ── Write the summary CSV ──────────────────────────────────────────────────

const META = {
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045",
  feature_set: "production alloc-stats bench-internals",
  measurement_commit: landingCommit,
  base_commit: "b3b18bb637855cf77ec42f317be0a196ca0739bb",
};

const CSV_HEADER =
  "profile,n_ops,used_before_ops_mib,forced_used_after_mib,unforced_used_after_mib,retention_cost_bytes,retention_cost_mib,forced_guard_delta,unforced_guard_delta,expected_calls,reps,cpu,os,feature_set,measurement_commit,base_commit";

const csvLines = [CSV_HEADER];
for (const s of summaryRows) {
  csvLines.push([
    s.profile,
    s.n_ops,
    s.used_before_ops_mib,
    s.forced_used_after_mib,
    s.unforced_used_after_mib,
    s.retention_cost_bytes,
    s.retention_cost_mib,
    s.forced_guard_delta,
    s.unforced_guard_delta,
    s.expected_calls,
    s.reps,
    META.cpu,
    META.os,
    META.feature_set,
    META.measurement_commit,
    META.base_commit,
  ].join(","));
}

const outPath = new URL(
  "docs/perf/R33_6_DECAY_THROTTLE_RETENTION_COST_summary.csv",
  ROOT,
);
writeFileSync(outPath, csvLines.join("\n") + "\n");

console.log("=== R33-6 retention cost summary derived ===");
console.log(`source: docs/perf/_raw_r33_6_decay_throttle_retention_cost_gate.log (${records.length} child rows)`);
console.log();
for (const s of summaryRows) {
  console.log(
    `  profile=${s.profile.padEnd(14)} n_ops=${String(s.n_ops).padEnd(3)} ` +
    `retention_cost=${s.retention_cost_bytes} bytes (${s.retention_cost_mib} MiB)  ` +
    `[forced_after=${s.forced_used_after_mib} MiB, unforced_after=${s.unforced_used_after_mib} MiB]  ` +
    `guard_delta: forced=${s.forced_guard_delta}/${s.expected_calls}, unforced=${s.unforced_guard_delta}/${s.expected_calls}`,
  );
}
console.log();
console.log(`wrote ${outPath.pathname}`);
