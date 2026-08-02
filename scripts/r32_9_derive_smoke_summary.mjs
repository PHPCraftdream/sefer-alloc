// R32-9 (task #500) — derives
// `docs/perf/R32_9_MACRO_MULTISEG_STEADY_STATE_HARNESS_summary.csv` from the
// raw smoke-test log `docs/perf/_raw_r32_9_smoke_test.log` (the stdout of
// `examples/r32_9_macro_multiseg_steady_state_ab_gate`, run once on `HEAD` as
// this task's own required "prove the harness works" smoke test).
//
// ONE checked script, per CLAUDE.md's R30-9 rule: raw per-sample data (the
// example's own `RESULT key=value` / CSV-block stdout) written first, the
// summary CSV and headline numbers computed and ASSERTED here, never
// hand-transcribed into the design-note report's prose. Pattern follows
// `scripts/r32_0_derive_report_data.mjs`.
//
// Usage:
//   node scripts/r32_9_derive_smoke_summary.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from 'node:fs';

const ROOT = new URL('../', import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), 'utf8');

const landingCommit = process.argv[2] || 'UNFILLED';

// Immutable source identity (CLAUDE.md's R29-6 rule, option 2: a git
// tree-object SHA via `git write-tree` against the real index with exactly
// this task's changed/added files staged — computed BEFORE this derive
// script ran, recorded here, not re-derived).
const baseCommit = 'a632dd4bcb2d12a5b083fbd60058678feb63005c';
const sourceTreeSha = '6ca05075b66dc0901134cca6de40888850621603';

// ── Parse the example's own CSV block out of the raw smoke-test log ────────
const RAW_LOG = 'docs/perf/_raw_r32_9_smoke_test.log';
const text = read(RAW_LOG);
const lines = text.split(/\r?\n/);
const headerIdx = lines.findIndex((l) => l.startsWith('# thread_count,'));
if (headerIdx === -1) {
  throw new Error(`${RAW_LOG}: could not find the "# thread_count,..." CSV header line`);
}
const cols = lines[headerIdx].replace(/^#\s*/, '').split(',');
const rows = [];
for (let i = headerIdx + 1; i < lines.length; i++) {
  const line = lines[i];
  if (!line || !/^\d/.test(line)) break; // CSV block ends at the first non-data line
  const vals = line.split(',');
  const row = {};
  cols.forEach((c, j) => {
    row[c] = vals[j];
  });
  rows.push(row);
}
if (rows.length !== 10) {
  throw new Error(`HEADLINE ASSERTION FAILED: expected 10 rows (2 thread-counts x 5 reps), got ${rows.length}`);
}

// ── Headline assertion 1: path-activation oracle passed on EVERY row — the
//    harness's own required proof that it actually achieved its >=64-live-
//    segment target working set, not just requested it. ───────────────────
for (const r of rows) {
  if (r.oracle_pass !== '1') {
    throw new Error(`HEADLINE ASSERTION FAILED: row threads=${r.thread_count} rep=${r.repetition} has oracle_pass=${r.oracle_pass} (expected 1)`);
  }
  const minTable = Number(r.min_table_count_observed);
  const oracleThreshold = Number(r.min_live_segments_oracle);
  if (!(minTable >= oracleThreshold)) {
    throw new Error(
      `HEADLINE ASSERTION FAILED: row threads=${r.thread_count} rep=${r.repetition} min_table_count_observed=${minTable} < min_live_segments_oracle=${oracleThreshold}`,
    );
  }
}

// ── Headline assertion 2: config_conflicts_delta == 0 on every row (R26-4
//    process-identity evidence — subprocess-per-arm isolation held). ───────
for (const r of rows) {
  if (r.config_conflicts_delta !== '0') {
    throw new Error(`HEADLINE ASSERTION FAILED: row threads=${r.thread_count} rep=${r.repetition} config_conflicts_delta=${r.config_conflicts_delta} (expected 0)`);
  }
}

// ── Compute median ns_per_op per thread-count arm (the report's headline
//    numbers), asserted here rather than hand-copied from the console table. ─
function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  return sorted[Math.floor(sorted.length / 2)];
}

const threadCounts = [...new Set(rows.map((r) => r.thread_count))];
const perArm = {};
for (const tc of threadCounts) {
  const cell = rows.filter((r) => r.thread_count === tc);
  const nsValues = cell.map((r) => Number(r.ns_per_op));
  const minTableValues = cell.map((r) => Number(r.min_table_count_observed));
  perArm[tc] = {
    n: cell.length,
    ns_per_op_median: median(nsValues),
    ns_per_op_min: Math.min(...nsValues),
    ns_per_op_max: Math.max(...nsValues),
    min_table_count: Math.min(...minTableValues),
  };
}

// Cross-check: the console's own printed median for threads=1 must match
// this script's independently recomputed median (the "assert the arithmetic
// it prints" rule) — reproduced from the raw log's own aggregated-table
// section rather than the CSV block, as a second independent read of the
// same underlying numbers.
const aggIdx = lines.findIndex((l) => l.trim().startsWith('threads') && l.includes('min_table_count') && l.includes('ns_per_op'));
if (aggIdx === -1) {
  throw new Error(`${RAW_LOG}: could not find the aggregated-table header line`);
}
for (let i = aggIdx + 1; i < lines.length; i++) {
  const line = lines[i].trim();
  if (!line) break;
  const parts = line.split(/\s+/);
  if (parts.length !== 3) break;
  const [tc, , printedNs] = parts;
  const recomputed = perArm[tc].ns_per_op_median;
  if (Math.abs(Number(printedNs) - recomputed) > 0.05) {
    throw new Error(
      `HEADLINE ASSERTION FAILED: printed median ns_per_op for threads=${tc} (${printedNs}) does not match this script's independently recomputed median (${recomputed.toFixed(1)})`,
    );
  }
}

console.log('=== Headline assertions (checked in-script per CLAUDE.md rule 6) ===\n');
console.log(`Path-activation oracle: 10/10 rows PASS (min_table_count_observed >= 64 on every arm/rep) -- confirmed`);
console.log(`Config-conflict identity: 10/10 rows config_conflicts_delta == 0 -- confirmed`);
for (const tc of threadCounts) {
  const a = perArm[tc];
  console.log(
    `threads=${tc}: n=${a.n} min_table_count=${a.min_table_count} ns_per_op median=${a.ns_per_op_median.toFixed(1)} range=[${a.ns_per_op_min.toFixed(1)}, ${a.ns_per_op_max.toFixed(1)}]`,
  );
}

// ── Write summary CSV ──────────────────────────────────────────────────────
const META = {
  cpu: 'unspecified_dev_host', // no wmic CPU probe was run for this infra smoke test; see report §0 caveat
  os: 'Windows_10_Pro_10.0.19045',
  rustc: '1.97.0',
};

const header = [
  'base_commit',
  'immutable_tree_sha',
  'landing_commit',
  'artifact',
  'metric',
  'arm',
  'repetition',
  'value',
  'unit',
  'notes',
].join(',');

const csvLines = [header];
function pushRow(artifact, metric, arm, repetition, value, unit, notes) {
  csvLines.push(
    [baseCommit, sourceTreeSha, landingCommit, artifact, metric, arm, repetition, value, unit, `"${String(notes).replace(/"/g, "'")}"`].join(','),
  );
}

for (const r of rows) {
  pushRow(
    'r32_9_wallclock_smoke_test',
    'ns_per_op',
    `threads_${r.thread_count}`,
    r.repetition,
    Number(r.ns_per_op).toFixed(3),
    'ns',
    `min_table_count_observed=${r.min_table_count_observed} oracle_pass=${r.oracle_pass} config_conflicts_delta=${r.config_conflicts_delta} total_ops=${r.total_ops}`,
  );
}
for (const tc of threadCounts) {
  const a = perArm[tc];
  pushRow(
    'r32_9_wallclock_smoke_test_aggregate',
    'ns_per_op_median',
    `threads_${tc}`,
    '',
    a.ns_per_op_median.toFixed(3),
    'ns',
    `n=${a.n} range=[${a.ns_per_op_min.toFixed(1)},${a.ns_per_op_max.toFixed(1)}] min_table_count=${a.min_table_count}`,
  );
}

const outPath = 'docs/perf/R32_9_MACRO_MULTISEG_STEADY_STATE_HARNESS_summary.csv';
writeFileSync(new URL(outPath, ROOT), csvLines.join('\n') + '\n');
console.log(`\nwrote ${csvLines.length - 1} data rows to ${outPath} (base_commit=${baseCommit}, immutable_tree_sha=${sourceTreeSha}, landing_commit=${landingCommit})`);
console.log(`(meta: cpu=${META.cpu} os=${META.os} rustc=${META.rustc})`);
