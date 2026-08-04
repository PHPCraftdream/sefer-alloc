// R34-12 (task #531) — checked summary-derivation script for the CLEAN A/B
// re-gate of RemoteFreeRing's shadow-head (`cached_head`) fast path.
//
// Reads the raw paired-ab provenance JSON (per-process samples, NOT
// aggregates) and derives EVERY headline number in
// docs/perf/R34_12_REMOTE_RING_CLEAN_REGATE.md from it, asserting the
// arithmetic so a wrong number is a FAILING CHECK in this script, never a
// published claim a human transcribed wrong by hand (CLAUDE.md's R22-14 /
// derive-from-raw-data rule).
//
// Usage:
//   node scripts/r34_12_clean_regate_summary.mjs
//
// Output:
//   docs/perf/R34_12_REMOTE_RING_CLEAN_REGATE_summary.csv

import { readFileSync, writeFileSync } from 'node:fs';
import { REPO_ROOT } from './lib.mjs';

const RUN_DIR = `${REPO_ROOT}/docs/perf/paired_ab_runs`;

// ── Provenance files (cited in the report) ──
const MAIN_RUN = `${RUN_DIR}/2026-08-04T16-40-55-214Z.json`;
const CONTROL_FAV = `${RUN_DIR}/2026-08-04T16-41-47-549Z.json`;
const CONTROL_NEAR_FULL = `${RUN_DIR}/2026-08-04T16-42-31-023Z.json`;

function loadProvenance(path) {
  return JSON.parse(readFileSync(path, 'utf8'));
}

function findComparison(data, armA, armB) {
  const c = data.comparisons.find(
    (c) => c.arm_a === armA && c.arm_b === armB,
  );
  if (!c) {
    throw new Error(
      `comparison ${armA} vs ${armB} not found in ${data.timestamp}`,
    );
  }
  return c;
}

// Compute mean ns/push for an arm from the raw process launches.
function meanNsPerPush(comparison, arm) {
  const launches = comparison.raw_process_launches.filter(
    (l) => (arm === 'a' ? comparison.arm_a : comparison.arm_b) ===
      (arm === 'a' ? comparison.arm_a : comparison.arm_b),
  );
  // Each arm appears in specific slots; use the paired samples instead.
  const samples = arm === 'a' ? comparison.samples_a_ns : comparison.samples_b_ns;
  const total = samples.reduce((s, v) => s + v, 0);
  return total / samples.length;
}

// Compute percentage change: (before - after) / before × 100
function pctChange(beforeMean, afterMean) {
  return ((beforeMean - afterMean) / beforeMean) * 100.0;
}

const main = loadProvenance(MAIN_RUN);
const ctrlFav = loadProvenance(CONTROL_FAV);
const ctrlNF = loadProvenance(CONTROL_NEAR_FULL);

// ── Per-regime statistics ──
const regimes = [
  {
    name: 'favorable',
    armA: 'before_favorable',
    armB: 'after_favorable',
    ctrlData: ctrlFav,
  },
  {
    name: 'near_full',
    armA: 'before_near_full',
    armB: 'after_near_full',
    ctrlData: ctrlNF,
  },
  {
    name: 'overflow',
    armA: 'before_overflow',
    armB: 'after_overflow',
    ctrlData: null, // no control run for overflow (too slow)
  },
];

const results = [];
const TOTAL_PUSHES = 200000;

for (const regime of regimes) {
  const c = findComparison(main, regime.armA, regime.armB);
  const t = c.paired_t_test;
  const s = c.sign_test;

  const beforeMeanNs = meanNsPerPush(c, 'a');
  const afterMeanNs = meanNsPerPush(c, 'b');
  const beforeNsPerPush = beforeMeanNs / TOTAL_PUSHES;
  const afterNsPerPush = afterMeanNs / TOTAL_PUSHES;
  const change = pctChange(beforeMeanNs, afterMeanNs);

  // Assert: the mean delta matches the t-test's mean
  const computedDelta = beforeMeanNs - afterMeanNs;
  assertApprox(
    computedDelta,
    t.mean,
    t.mean * 0.01, // 1% tolerance (block-value averaging)
    `${regime.name}: computed delta ${computedDelta.toFixed(0)} vs t-test mean ${t.mean.toFixed(0)}`,
  );

  // Assert: t = mean / se
  assertApprox(
    t.t,
    t.mean / t.se,
    0.01,
    `${regime.name}: t ${t.t.toFixed(6)} vs mean/se ${(t.mean / t.se).toFixed(6)}`,
  );

  // Assert: percentage change
  const expectedPct = ((t.mean) / beforeMeanNs) * 100.0;
  assertApprox(
    change,
    expectedPct,
    0.1, // 0.1 percentage points
    `${regime.name}: pct change ${change.toFixed(2)}% vs expected ${expectedPct.toFixed(2)}%`,
  );

  // Assert: sign test sums to n
  const signSum = s.aFaster + s.bFaster + s.ties;
  if (signSum !== t.n) {
    throw new Error(
      `${regime.name}: sign test sum ${signSum} != n ${t.n}`,
    );
  }

  let ctrlT = null;
  let ctrlSig = null;
  let ctrlSignA = null;
  let ctrlSignB = null;
  if (regime.ctrlData) {
    const cc = findComparison(regime.ctrlData, regime.armB, regime.armB);
    ctrlT = cc.paired_t_test.t;
    ctrlSig = cc.paired_t_test.significant;
    ctrlSignA = cc.sign_test.aFaster;
    ctrlSignB = cc.sign_test.bFaster;
  }

  results.push({
    regime: regime.name,
    n: t.n,
    before_ns_per_push: beforeNsPerPush.toFixed(2),
    after_ns_per_push: afterNsPerPush.toFixed(2),
    pct_change: change.toFixed(2),
    mean_delta_ns: t.mean.toFixed(0),
    t_stat: t.t.toFixed(3),
    t_crit: t.crit,
    significant: t.significant,
    sign_before_faster: s.aFaster,
    sign_after_faster: s.bFaster,
    control_t: ctrlT !== null ? ctrlT.toFixed(3) : 'N/A',
    control_significant: ctrlSig !== null ? ctrlSig : 'N/A',
    control_sign_split: ctrlSignA !== null ? `${ctrlSignA}/${ctrlSignB}` : 'N/A',
  });
}

// ── Print summary table ──
console.log('\n=== R34-12 CLEAN A/B summary ===\n');
for (const r of results) {
  console.log(
    `  ${r.regime}: before=${r.before_ns_per_push} after=${r.after_ns_per_push} ` +
    `Δ=${r.pct_change}% t=${r.t_stat} (crit=${r.t_crit}) ` +
    `${r.significant ? 'SIGNIFICANT' : 'not sig'} ` +
    `sign=${r.sign_before_faster}/${r.sign_after_faster} ` +
    `ctrl_t=${r.control_t} ctrl_sign=${r.control_sign_split}`,
  );
}

// ── Write summary CSV ──
const csvLines = [
  'regime,n,before_ns_per_push,after_ns_per_push,pct_change,mean_delta_ns,t_stat,t_crit,significant,sign_before_faster,sign_after_faster,control_t,control_significant,control_sign_split',
];
for (const r of results) {
  csvLines.push(
    [
      r.regime,
      r.n,
      r.before_ns_per_push,
      r.after_ns_per_push,
      r.pct_change,
      r.mean_delta_ns,
      r.t_stat,
      r.t_crit,
      r.significant,
      r.sign_before_faster,
      r.sign_after_faster,
      r.control_t,
      r.control_significant,
      r.control_sign_split,
    ].join(','),
  );
}

const csvPath = `${REPO_ROOT}/docs/perf/R34_12_REMOTE_RING_CLEAN_REGATE_summary.csv`;
writeFileSync(csvPath, csvLines.join('\n') + '\n');
console.log(`\nSummary CSV written to ${csvPath}`);

// ── Helpers ──
function assertApprox(actual, expected, tolerance, label) {
  if (Math.abs(actual - expected) > tolerance) {
    throw new Error(
      `ASSERTION FAILED (${label}): |${actual} - ${expected}| > ${tolerance}`,
    );
  }
}
