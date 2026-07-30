// R31-1 (task #464): derives `docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE_summary.csv`
// from the raw per-child CSV block the crossing-regime harness prints to
// stdout (captured as `docs/perf/_raw_r31_1_large_cache_headroom_crossing_regime_gate.log`).
// This is the ONE checked script that turns the raw log into the report's
// machine-readable summary (CLAUDE.md "tables derived by one checked
// script, not hand-transcribed" — same pattern as
// `scripts/r31_0_summary.mjs`/`scripts/r31_2_derive_report_data.mjs`).
//
// Also asserts the report's own headline claim in-script (CLAUDE.md rule 6:
// "a script that computes a headline ratio must assert the arithmetic it
// prints"): AT_BOUNDARY_6MiB ties 64-vs-256 MiB hit rate; both crossing-regime
// arms (12 MiB, 34 MiB objects) show a real 64-vs-256 MiB hit-rate GAP.
//
// Usage:
//   node scripts/r31_1_derive_report_data.mjs [landing_commit_sha]
//
// `landing_commit_sha` is this report's own landing commit (chicken-and-egg:
// a commit cannot cite its own SHA inside its own tree). Omit on first
// generation (column reads "UNFILLED"); re-run with the real SHA in the
// follow-up commit that fills the placeholder — mirrors the
// 1272a52/9335979/f93e663 precedent.

import { readFileSync, writeFileSync } from 'node:fs';

const ROOT = new URL('../', import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), 'utf8');

const landingCommit = process.argv[2] || 'UNFILLED';
// The measured tree's TRUE base: `main` HEAD immediately before this task's
// changes (confirmed via `git rev-parse HEAD` at session start, matching
// this report's own "Base revision" citation) -- NOT re-derived from `git
// rev-parse HEAD` at script-run time, which would silently drift to
// whatever commit happens to be checked out when this script is re-run
// later (exactly the base-vs-landing conflation R31-2's own follow-up
// commit `f93e663` had to correct after the fact).
const baseCommit = 'f93e66311ad3ea47aaa1a2fe2461caeb4c0968fe';

const RAW_LOG = 'docs/perf/_raw_r31_1_large_cache_headroom_crossing_regime_gate.log';
const text = read(RAW_LOG);
const lines = text.split(/\r?\n/);

const headerIdx = lines.findIndex((l) => l.startsWith('# burst_label,'));
if (headerIdx === -1) {
  throw new Error(`could not find CSV header line in ${RAW_LOG}`);
}
const cols = lines[headerIdx].slice(2).split(',');
const rows = [];
for (let i = headerIdx + 1; i < lines.length; i++) {
  const line = lines[i];
  if (!line || line.startsWith('#') || !line.includes(',')) break;
  const vals = line.split(',');
  if (vals.length !== cols.length) break;
  const row = {};
  cols.forEach((c, idx) => {
    row[c] = vals[idx];
  });
  rows.push(row);
}
if (rows.length !== 54) {
  throw new Error(`expected 54 per-child rows (3 burst arms x 2 headroom x 3 threads x 3 reps), got ${rows.length}`);
}

function median(nums) {
  const s = [...nums].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

const burstArms = [...new Set(rows.map((r) => r.burst_label))];
const headroomArms = [...new Set(rows.map((r) => r.headroom_mib))];
const threadArms = [...new Set(rows.map((r) => r.thread_count))];

const cellRows = [];
for (const burst of burstArms) {
  for (const hb of headroomArms) {
    for (const tc of threadArms) {
      const cell = rows.filter(
        (r) => r.burst_label === burst && r.headroom_mib === hb && r.thread_count === tc,
      );
      if (cell.length !== 3) throw new Error(`expected 3 reps for ${burst}/${hb}/${tc}, got ${cell.length}`);
      const hits = median(cell.map((r) => Number(r.burst2_hits_sum)));
      const poss = Number(cell[0].burst2_possible_sum);
      const hitRatePct = (100 * hits) / poss;
      const burst1UsedMax = median(cell.map((r) => Number(r.burst1_used_max)));
      const rssBurst2 = median(cell.map((r) => Number(r.rss_burst2_kib)));
      const rssIdle = median(cell.map((r) => Number(r.rss_idle_kib)));
      const oracleAllPass = cell.every((r) => r.oracle_pass === '1');
      cellRows.push({
        burst_label: burst,
        headroom_mib: hb,
        thread_count: tc,
        burst1_used_max_bytes_median: burst1UsedMax,
        burst2_hits_median: hits,
        burst2_possible: poss,
        hit_rate_pct: hitRatePct,
        rss_burst2_kib_median: rssBurst2,
        rss_idle_kib_median: rssIdle,
        oracle_pass_all_reps: oracleAllPass ? 1 : 0,
      });
    }
  }
}

// ── Assert the headline arithmetic (CLAUDE.md rule 6) ──────────────────────
function hitRateFor(burst, hbMib) {
  const cells = cellRows.filter((c) => c.burst_label === burst && c.headroom_mib === String(hbMib));
  return cells.map((c) => c.hit_rate_pct);
}

const atBoundary64 = hitRateFor('AT_BOUNDARY_6MiB', 64);
const atBoundary256 = hitRateFor('AT_BOUNDARY_6MiB', 256);
const modest64 = hitRateFor('CROSSING_MODEST_12MiB', 64);
const modest256 = hitRateFor('CROSSING_MODEST_12MiB', 256);
const r29_13_64 = hitRateFor('CROSSING_R29_13_34MiB', 64);
const r29_13_256 = hitRateFor('CROSSING_R29_13_34MiB', 256);

const allEqual = (a, b) => a.every((v, i) => v === b[i]);

if (!allEqual(atBoundary64, atBoundary256)) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: AT_BOUNDARY_6MiB was expected to TIE at 64 vs 256 MiB headroom, ` +
      `got 64MiB=${JSON.stringify(atBoundary64)} vs 256MiB=${JSON.stringify(atBoundary256)}`,
  );
}
if (allEqual(modest64, modest256)) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: CROSSING_MODEST_12MiB was expected to show a REAL GAP at 64 vs 256 ` +
      `MiB headroom (this is the whole point of the crossing-regime burst), got tied: ${JSON.stringify(modest64)}`,
  );
}
if (allEqual(r29_13_64, r29_13_256)) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: CROSSING_R29_13_34MiB was expected to show a REAL GAP at 64 vs 256 ` +
      `MiB headroom, got tied: ${JSON.stringify(r29_13_64)}`,
  );
}
// The gap must be the same 12.5-percentage-point step R30-6 found at
// 0/16 MiB (whole-slot eviction granularity: 1 of 8 large-cache slots
// evicted).
const modestGap = modest256[0] - modest64[0];
const r29_13Gap = r29_13_256[0] - r29_13_64[0];
if (Math.abs(modestGap - 12.5) > 1e-9 || Math.abs(r29_13Gap - 12.5) > 1e-9) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: expected a 12.5-percentage-point hit-rate gap at both crossing-regime ` +
      `sizes, got modest=${modestGap} r29_13=${r29_13Gap}`,
  );
}

console.log('=== Headline assertions (checked in-script per CLAUDE.md rule 6) ===\n');
console.log(`AT_BOUNDARY_6MiB (64 MiB burst):    64 MiB == 256 MiB headroom hit rate -> TIE confirmed (${atBoundary64.join(',')}% both)`);
console.log(`CROSSING_MODEST_12MiB (128 MiB burst): 64 MiB=${modest64.join(',')}% vs 256 MiB=${modest256.join(',')}% -> GAP=${modestGap.toFixed(1)}pp confirmed`);
console.log(`CROSSING_R29_13_34MiB (288 MiB burst): 64 MiB=${r29_13_64.join(',')}% vs 256 MiB=${r29_13_256.join(',')}% -> GAP=${r29_13Gap.toFixed(1)}pp confirmed`);

// ── Write summary CSV ───────────────────────────────────────────────────────
const META = {
  cpu: 'Intel_Core_i7-11800H_2.30GHz',
  os: 'Windows_10_Pro_10.0.19045',
  rustc: '1.97.0',
  feature_set: 'production alloc-stats bench-internals',
};

const header = [
  'base_commit',
  'landing_commit',
  'feature_set',
  'cpu',
  'os',
  'rustc',
  'burst_label',
  'headroom_mib',
  'thread_count',
  'burst1_used_max_bytes_median',
  'burst2_hits_median',
  'burst2_possible',
  'hit_rate_pct',
  'rss_burst2_kib_median',
  'rss_idle_kib_median',
  'oracle_pass_all_reps',
].join(',');

const csvLines = [header];
for (const c of cellRows) {
  csvLines.push(
    [
      baseCommit,
      landingCommit,
      `"${META.feature_set}"`,
      META.cpu,
      META.os,
      META.rustc,
      c.burst_label,
      c.headroom_mib,
      c.thread_count,
      c.burst1_used_max_bytes_median,
      c.burst2_hits_median,
      c.burst2_possible,
      c.hit_rate_pct.toFixed(1),
      c.rss_burst2_kib_median,
      c.rss_idle_kib_median,
      c.oracle_pass_all_reps,
    ].join(','),
  );
}

const outPath = 'docs/perf/R31_1_LARGE_CACHE_HEADROOM_CROSSING_REGIME_GATE_summary.csv';
writeFileSync(new URL(outPath, ROOT), csvLines.join('\n') + '\n');
console.log(`\nwrote ${cellRows.length} data rows to ${outPath} (base_commit=${baseCommit}, landing_commit=${landingCommit})`);
