// R34-10 (task #529): derives `docs/perf/R34_10_SPARSE_DECAY_GATE_summary.csv`
// AND the report's markdown tables from the raw per-interval time series in
// `docs/perf/_raw_r34_10_sparse_decay_gate.log`.
//
// This is the ONE checked script that turns the raw log into the report's
// machine-readable summary + tables (CLAUDE.md "tables derived by one checked
// script, not hand-transcribed"). It ASSERTS the path-activation oracle and
// config-resolution invariants — a wrong invariant is a THROW, not a printed
// claim. It does NOT assert the verdict (bound holds / fails): that is prose
// in the report, resting on the numbers this script prints.
//
// What this measures: over 40 CONSECUTIVE sparse decay intervals, does the
// retention gap between the throttled arm (stride=64) and the unthrottled arm
// (stride=1) ACCUMULATE beyond one segment? For each (profile, events) cell it
// reports the peak and final gap in bytes/MiB/segments, plus the "ops late"
// (clock-read deficit) and "seconds late" (interval-equivalents) axes.
//
// Usage:
//   node scripts/r34_10_sparse_decay_summary.mjs <source_identity>
//
// <source_identity> is the immutable source identity captured BEFORE
// measurement (a `git write-tree` tree SHA or temp-commit SHA per CLAUDE.md's
// R29-6 rule). It is baked into the CSV for reproducibility.

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";

const ROOT = new URL("../", import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), "utf8");

const sourceIdentity = process.argv[2] || (() => {
  throw new Error("usage: node scripts/r34_10_sparse_decay_summary.mjs <source_identity>");
})();

const MIB = 1024 * 1024;
// OBJ_BYTES = 2 MiB request → 1 segment (4 MiB) cached span.
const SEGMENT_BYTES = 4 * 1024 * 1024;

const META = {
  cpu: "Intel_Core_i7-11800H_2.30GHz",
  os: "Windows_10_Pro_10.0.19045",
  feature_set: "production alloc-stats bench-internals internals",
  source_identity: sourceIdentity,
  base_commit: execSync("git rev-parse HEAD", { cwd: new URL(".", import.meta.url), encoding: "utf8" }).trim(),
};

// ── Parse the raw log ───────────────────────────────────────────────────────

const log = read("docs/perf/_raw_r34_10_sparse_decay_gate.log");

function parseResultLine(line) {
  // Shape: "RESULT <tag>=<n> key=value ...". The first token after RESULT is a
  // discriminator (ts / config / oracle); the rest are key=value pairs.
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

const tsRows = [];
const configRows = [];
const oracleRows = [];

for (const line of log.split("\n")) {
  if (!line.startsWith("RESULT ")) continue;
  const rec = parseResultLine(line);
  if (rec.ts === "1") tsRows.push(rec);
  else if (rec.config === "1") configRows.push(rec);
  else if (rec.oracle === "1") oracleRows.push(rec);
}

// ── Validate invariants on every child ──────────────────────────────────────

for (const c of configRows) {
  if (c.config_conflicts_delta !== "0") {
    throw new Error(`child ${c.profile}/${c.events}/${c.arm}: config_conflicts_delta=${c.config_conflicts_delta} (expected 0)`);
  }
  if (c.verified_headroom !== c.headroom_bytes) {
    throw new Error(`child ${c.profile}/${c.events}/${c.arm}: verified_headroom=${c.verified_headroom} != headroom_bytes=${c.headroom_bytes}`);
  }
  if (c.verified_interval_ms !== c.decay_interval_ms) {
    throw new Error(`child ${c.profile}/${c.events}/${c.arm}: verified_interval_ms=${c.verified_interval_ms} != decay_interval_ms=${c.decay_interval_ms}`);
  }
  if (c.headroom_crossed !== "1") {
    throw new Error(`child ${c.profile}/${c.events}/${c.arm}: headroom_crossed != 1`);
  }
  if (c.unthrottled_read !== "1") {
    throw new Error(`child ${c.profile}/${c.events}/${c.arm}: unthrottled_read != 1`);
  }
}
for (const o of oracleRows) {
  if (o.oracle_pass !== "1") {
    throw new Error(`child ${o.profile}/${o.events}/${o.arm}: oracle_pass != 1`);
  }
}

// ── Group config rows by (profile, events, arm) for baselines ───────────────

const configByKey = new Map();
for (const c of configRows) {
  configByKey.set(`${c.profile}|${c.events}|${c.arm}`, c);
}

const PROFILES = ["allocfree", "deallocate", "allocate"];
const EVENTS = [1, 2, 4, 8];
const INTERVALS = Number(configRows[0]?.intervals || 40);

// ── Compute per-interval deltas + gap time series per (profile, events) ─────

const summaryRows = [];

for (const profile of PROFILES) {
  for (const events of EVENTS) {
    const cfgT = configByKey.get(`${profile}|${events}|throttled`);
    const cfgU = configByKey.get(`${profile}|${events}|unthrottled`);
    if (!cfgT || !cfgU) {
      throw new Error(`missing config row for ${profile}/${events} (throttled=${!!cfgT}, unthrottled=${!!cfgU})`);
    }

    // Per-interval rows for each arm.
    const tsT = tsRows
      .filter((r) => r.profile === profile && Number(r.events) === events && r.arm === "throttled")
      .sort((a, b) => Number(a.interval) - Number(b.interval));
    const tsU = tsRows
      .filter((r) => r.profile === profile && Number(r.events) === events && r.arm === "unthrottled")
      .sort((a, b) => Number(a.interval) - Number(b.interval));

    if (tsT.length !== INTERVALS || tsU.length !== INTERVALS) {
      throw new Error(`${profile}/${events}: expected ${INTERVALS} intervals, got throttled=${tsT.length} unthrottled=${tsU.length}`);
    }

    // Compute the gap (throttled - unthrottled used_post) at each interval, and
    // track the peak.
    let peakGapBytes = 0;
    let peakGapInterval = -1;
    for (let i = 0; i < INTERVALS; i++) {
      const usedT = Number(tsT[i].used_post);
      const usedU = Number(tsU[i].used_post);
      const gap = usedT - usedU; // throttled retains MORE ⇒ positive
      if (gap > peakGapBytes) {
        peakGapBytes = gap;
        peakGapInterval = i;
      }
    }

    const lastT = tsT[INTERVALS - 1];
    const lastU = tsU[INTERVALS - 1];
    const finalGapBytes = Number(lastT.used_post) - Number(lastU.used_post);

    // "ops late": the throttled arm's clock-read deficit vs unthrottled, i.e.
    // how many fewer clock reads the throttle caused over the whole run.
    const guardDeltaT = Number(lastT.guard_passed_cum) - Number(cfgT.guard_passed_baseline);
    const guardDeltaU = Number(lastU.guard_passed_cum) - Number(cfgU.guard_passed_baseline);
    const opsLate = guardDeltaU - guardDeltaT; // ≥ 0 (throttled reads ≤ unthrottled)

    // "seconds late": each clock-read the throttle skipped is one decay
    // interval's worth of promptness the throttled arm did not get, IF that
    // skipped read would have found a due tick. We approximate the upper bound
    // as opsLate × decay_interval (the wall-clock span the throttled arm spent
    // not looking at the clock). Reported in ms and scaled-to-default in prose.
    const decayIntervalMs = Number(cfgT.decay_interval_ms);
    const secondsLateMs = opsLate * decayIntervalMs;

    const releasedDeltaT = Number(lastT.released_cum) - Number(cfgT.released_baseline);
    const releasedDeltaU = Number(lastU.released_cum) - Number(cfgU.released_baseline);

    const rssDeltaT = Number(lastT.rss_kib) - Number(cfgT.rss_baseline_kib);
    const rssDeltaU = Number(lastU.rss_kib) - Number(cfgU.rss_baseline_kib);

    summaryRows.push({
      profile,
      events,
      intervals: INTERVALS,
      headroom_bytes: Number(cfgT.headroom_bytes),
      obj_bytes: Number(cfgT.obj_bytes),
      decay_interval_ms: decayIntervalMs,
      stride: Number(cfgT.stride),
      throttled_used_baseline_mib: (Number(cfgT.used_baseline) / MIB).toFixed(2),
      unthrottled_used_baseline_mib: (Number(cfgU.used_baseline) / MIB).toFixed(2),
      throttled_final_used_mib: (Number(lastT.used_post) / MIB).toFixed(2),
      unthrottled_final_used_mib: (Number(lastU.used_post) / MIB).toFixed(2),
      peak_gap_bytes: peakGapBytes,
      peak_gap_mib: (peakGapBytes / MIB).toFixed(2),
      peak_gap_segments: (peakGapBytes / SEGMENT_BYTES).toFixed(2),
      peak_gap_interval: peakGapInterval,
      final_gap_bytes: finalGapBytes,
      final_gap_mib: (finalGapBytes / MIB).toFixed(2),
      throttled_guard_delta: guardDeltaT,
      unthrottled_guard_delta: guardDeltaU,
      ops_late: opsLate,
      seconds_late_ms: secondsLateMs,
      throttled_released_delta: releasedDeltaT,
      unthrottled_released_delta: releasedDeltaU,
      throttled_rss_delta_kib: rssDeltaT,
      unthrottled_rss_delta_kib: rssDeltaU,
    });
  }
}

// ── Write the summary CSV ───────────────────────────────────────────────────

const CSV_COLS = [
  "profile", "events", "intervals", "headroom_bytes", "obj_bytes", "decay_interval_ms", "stride",
  "throttled_used_baseline_mib", "unthrottled_used_baseline_mib",
  "throttled_final_used_mib", "unthrottled_final_used_mib",
  "peak_gap_bytes", "peak_gap_mib", "peak_gap_segments", "peak_gap_interval",
  "final_gap_bytes", "final_gap_mib",
  "throttled_guard_delta", "unthrottled_guard_delta", "ops_late", "seconds_late_ms",
  "throttled_released_delta", "unthrottled_released_delta",
  "throttled_rss_delta_kib", "unthrottled_rss_delta_kib",
  "reps", "cpu", "os", "feature_set", "source_identity", "base_commit",
];

const csvLines = [CSV_COLS.join(",")];
for (const s of summaryRows) {
  csvLines.push(CSV_COLS.map((c) => {
    if (c === "reps") return "1";
    if (c === "cpu") return META.cpu;
    if (c === "os") return META.os;
    if (c === "feature_set") return META.feature_set;
    if (c === "source_identity") return META.source_identity;
    if (c === "base_commit") return META.base_commit;
    return s[c];
  }).join(","));
}

const outPath = new URL("docs/perf/R34_10_SPARSE_DECAY_GATE_summary.csv", ROOT);
writeFileSync(outPath, csvLines.join("\n") + "\n");

// ── Print the report tables (markdown) ──────────────────────────────────────

function miB(bytes) { return (bytes / MIB).toFixed(2); }
function seg(bytes) { return (bytes / SEGMENT_BYTES).toFixed(2); }

console.log("=== R34-10 sparse-decay accumulation summary derived ===");
console.log(`source: docs/perf/_raw_r34_10_sparse_decay_gate.log`);
console.log(`source_identity (captured before measurement): ${META.source_identity}`);
console.log(`base_commit (HEAD at derive time): ${META.base_commit}`);
console.log(`invariants: ${configRows.length} config rows, ${oracleRows.length} oracle rows — all passed`);
console.log();

// Table 1: peak gap per (profile, events) — the headline accumulation table.
console.log("## Table 1 — peak retention gap (throttled − unthrottled used_post), all profiles");
console.log();
console.log("| profile | events/interval | peak gap (MiB) | peak gap (segments) | peak @ interval | throttled final (MiB) | unthrottled final (MiB) |");
console.log("|---|---:|---:|---:|---:|---:|---:|");
for (const s of summaryRows) {
  console.log(
    `| ${s.profile} | ${s.events} | ${miB(s.peak_gap_bytes)} | ${seg(s.peak_gap_bytes)} | ${s.peak_gap_interval} | ${s.throttled_final_used_mib} | ${s.unthrottled_final_used_mib} |`,
  );
}
console.log();

// Table 2: ops-late / seconds-late per (profile, events).
console.log("## Table 2 — ops-late vs seconds-late (the two axes the review demanded)");
console.log();
console.log("| profile | events/interval | throttled clock-reads | unthrottled clock-reads | ops late (deficit) | seconds late (ms @100ms interval) |");
console.log("|---|---:|---:|---:|---:|---:|");
for (const s of summaryRows) {
  console.log(
    `| ${s.profile} | ${s.events} | ${s.throttled_guard_delta} | ${s.unthrottled_guard_delta} | ${s.ops_late} | ${s.seconds_late_ms} |`,
  );
}
console.log();

// Table 3: allocfree time series (the primary profile) at events=1 — the full
// 40-interval trace showing the gap opening and persisting.
console.log("## Table 3 — allocfree, events=1/interval: per-interval time series (the primary trace)");
console.log();
const tsT1 = tsRows
  .filter((r) => r.profile === "allocfree" && Number(r.events) === 1 && r.arm === "throttled")
  .sort((a, b) => Number(a.interval) - Number(b.interval));
const tsU1 = tsRows
  .filter((r) => r.profile === "allocfree" && Number(r.events) === 1 && r.arm === "unthrottled")
  .sort((a, b) => Number(a.interval) - Number(b.interval));
const cfgT1 = configByKey.get("allocfree|1|throttled");
const cfgU1 = configByKey.get("allocfree|1|unthrottled");
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

console.log(`wrote ${outPath.pathname}`);
