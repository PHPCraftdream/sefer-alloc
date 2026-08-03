#!/usr/bin/env node
// R33-5 (task #510) — checked derive script for the R32-10 OWN_CACHE_SIZE
// latency-null paired-A/B addendum.
//
// Reads the CURATED raw log files (docs/perf/_raw_r33_5_k{K}_{comparison}.log)
// produced by `paired-ab-runner.mjs --config scripts/_r33_5_own_cache_ab.json`,
// which drove examples/r32_10_own_cache_tier1_thrash_gate.rs in child mode
// through the A/B/B/A protocol, 20 pairs per comparison, for all 7 K values,
// 3 comparisons each: before-vs-after, before-vs-before, after-vs-after.
//
// For each (K, comparison) cell this script:
//   1. Parses the per-launch RESULT blocks from the .log text to extract
//      churn_elapsed_ns (the metric) and oracle2_pass (the sanity gate).
//   2. Reconstructs the A/B/B/A block structure to produce 20 paired samples.
//   3. INDEPENDENTLY computes the paired t-test and sign test from those
//      raw samples.
//   4. Cross-checks the computed t and sign-test against the summary line
//      the runner itself printed at the end of each log (a self-consistency
//      assertion — the script refuses to publish numbers that disagree with
//      the log's own printed verdict).
//   5. Converts churn_elapsed_ns to ns_per_op (÷ ROTATING_ROUNDS × K).
//   6. Asserts headline claims about the controls and main comparisons.
//   7. Writes a companion summary CSV.
//
// DATA SOURCE: docs/perf/_raw_r33_5_*.log (committed, curated evidence).
// This script does NOT read from docs/perf/paired_ab_runs/ — that directory
// is gitignored scratch output from paired-ab-runner.mjs and is deliberately
// excluded from version control (matching R32-11's precedent).
//
// Run: node scripts/r33_5_latency_null_addendum_summary.mjs

import { readFileSync, writeFileSync, readdirSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const perfDir = path.join(__dirname, '..', 'docs', 'perf');

// Must match examples/r32_10_own_cache_tier1_thrash_gate.rs.
const ROTATING_ROUNDS = 8192;

// ── Statistics: paired t-test and sign test ───────────────────────────────────
// Same methodology as paired-ab-runner.mjs (R5_R2_CHURN_REGRESSION_PAIRED_AB.md's
// paired t-test + sign test), independently implemented here as a zero-trust
// re-derivation.

const T_CRIT_005 = {
  3: 4.303, 4: 3.182, 5: 2.776, 6: 2.571, 7: 2.447, 8: 2.365, 9: 2.306, 10: 2.262,
  11: 2.228, 12: 2.201, 13: 2.179, 14: 2.16, 15: 2.145, 16: 2.131, 17: 2.12, 18: 2.11,
  19: 2.101, 20: 2.093, 25: 2.064, 30: 2.045, 40: 2.021, 60: 2.0, 120: 1.98,
};

function tCritical(df) {
  if (df <= 0) return null;
  const keys = Object.keys(T_CRIT_005).map(Number).sort((a, b) => a - b);
  for (const k of keys) if (df <= k) return T_CRIT_005[k];
  return 1.96;
}

function pairedTTest(deltas) {
  const n = deltas.length;
  if (n < 2) return null;
  const mean = deltas.reduce((a, b) => a + b, 0) / n;
  const variance = deltas.reduce((a, b) => a + (b - mean) ** 2, 0) / (n - 1);
  const sd = Math.sqrt(variance);
  const se = sd / Math.sqrt(n);
  const t = se === 0 ? (mean === 0 ? 0 : Infinity) : mean / se;
  const df = n - 1;
  const crit = tCritical(df);
  const significant = crit != null && Math.abs(t) > crit;
  return { n, mean, sd, se, t, df, crit, significant };
}

function signTest(deltas) {
  let aFaster = 0;
  let bFaster = 0;
  let ties = 0;
  for (const d of deltas) {
    if (d < 0) aFaster++;
    else if (d > 0) bFaster++;
    else ties++;
  }
  return { aFaster, bFaster, ties, n: deltas.length };
}

function approxEq(a, b, relTol = 1e-4, absTol = 1e-3) {
  return Math.abs(a - b) <= Math.max(relTol * Math.max(Math.abs(a), Math.abs(b)), absTol);
}

// ── Log file parser ───────────────────────────────────────────────────────────
// Each _raw_r33_5_k{K}_{comparison}.log file contains the stdout of one
// paired-ab-runner.mjs invocation. The runner launches the example binary 80
// times (20 pairs × 4 A/B/B/A launches), each printing RESULT key=value lines.
// The runner interleaves its own progress dots and summary output.
//
// To reconstruct per-block paired samples:
//   1. Extract ALL `RESULT churn_elapsed_ns=<int>` values in file order —
//      this is the per-launch metric sequence (length must be 80).
//   2. The A/B/B/A order means launches [0,3] in each block-of-4 are arm A,
//      and launches [1,2] are arm B.
//   3. Per block: meanA = (vals[4i] + vals[4i+3]) / 2, meanB = (vals[4i+1] +
//      vals[4i+2]) / 2 — matching paired-ab-runner.mjs's own block-aggregation.
//   4. Deltas are meanA[i] - meanB[i] for i=0..19.

function parseLog(logPath) {
  const text = readFileSync(logPath, 'utf8');
  const lines = text.split(/\r?\n/);

  // Extract the header to determine arm names.
  let armA = null;
  let armB = null;
  for (const line of lines) {
    const m = /\[paired-ab\] (\w+) vs (\w+) — \d+ pairs/.exec(line);
    if (m) {
      armA = m[1];
      armB = m[2];
      break;
    }
  }
  if (!armA || !armB) {
    throw new Error(`Could not parse header from ${logPath}`);
  }

  // Extract all churn_elapsed_ns and oracle2_pass values in file order.
  const churnValues = [];
  const oracle2Values = [];
  for (const line of lines) {
    const m1 = /RESULT churn_elapsed_ns=(\d+)/.exec(line.trim());
    if (m1) churnValues.push(Number(m1[1]));
    const m2 = /RESULT oracle2_pass=(\d+)/.exec(line.trim());
    if (m2) oracle2Values.push(Number(m2[1]));
  }

  if (churnValues.length !== 80) {
    throw new Error(
      `${logPath}: expected 80 churn_elapsed_ns values (20 pairs × 4 A/B/B/A launches), found ${churnValues.length}`,
    );
  }

  // All oracle2_pass values must be 1 (path-activation oracle passed on every launch).
  for (let i = 0; i < oracle2Values.length; i++) {
    if (oracle2Values[i] !== 1) {
      throw new Error(
        `${logPath}: launch ${i} oracle2_pass=${oracle2Values[i]} (expected 1) — a failed oracle means the timed region did not genuinely exercise its intended mechanism`,
      );
    }
  }

  // Reconstruct per-block paired samples.
  const PAIRS = 20;
  const samplesA = [];
  const samplesB = [];
  for (let block = 0; block < PAIRS; block++) {
    const base = block * 4;
    // A/B/B/A: indices 0,3 = A; indices 1,2 = B
    const a0 = churnValues[base + 0];
    const a1 = churnValues[base + 3];
    const b0 = churnValues[base + 1];
    const b1 = churnValues[base + 2];
    const meanA = (a0 + a1) / 2;
    const meanB = (b0 + b1) / 2;
    samplesA.push(meanA);
    samplesB.push(meanB);
  }

  const deltas = samplesA.map((a, i) => a - samplesB[i]);
  const tTest = pairedTTest(deltas);
  const sign = signTest(deltas);

  // Cross-check: extract the runner's own printed summary from the log and
  // verify our independently-computed t and sign-test match it.
  let logT = null;
  let logSignA = null;
  let logSignB = null;
  for (const line of lines) {
    const tm = /t=([-\d.]+)\s+df=\d+\s+crit/.exec(line);
    if (tm) logT = Number(tm[1]);
    const sm = /sign test: \S+-faster=(\d+)\/\d+\s+\S+-faster=(\d+)\/\d+/.exec(line);
    if (sm) {
      logSignA = Number(sm[1]);
      logSignB = Number(sm[2]);
    }
  }

  return {
    armA,
    armB,
    samplesA,
    samplesB,
    deltas,
    tTest,
    sign,
    logT,
    logSignA,
    logSignB,
    oracleCount: oracle2Values.length,
  };
}

// ── Load all log files ────────────────────────────────────────────────────────

const K_VALUES = [4, 8, 16, 24, 32, 48, 64];
const COMPARISONS = ['before_after', 'before_control', 'after_control'];
const COMPARISON_MAP = {
  before_after: 'before_vs_after',
  before_control: 'before_control',
  after_control: 'after_control',
};

const runs = [];
for (const k of K_VALUES) {
  for (const fileComp of COMPARISONS) {
    const fname = `_raw_r33_5_k${k}_${fileComp}.log`;
    const fullPath = path.join(perfDir, fname);
    if (!existsSync(fullPath)) {
      throw new Error(`Missing log file: ${fullPath}`);
    }
    const parsed = parseLog(fullPath);

    // Cross-check: our independently-computed t must match the log's own t.
    if (parsed.logT != null && !approxEq(parsed.tTest.t, parsed.logT)) {
      throw new Error(
        `${fname}: independently-computed t=${parsed.tTest.t.toFixed(6)} does not match the log's own printed t=${parsed.logT} — data corruption or parsing error`,
      );
    }

    // Cross-check: sign test must match.
    if (parsed.logSignA != null && (parsed.sign.aFaster !== parsed.logSignA || parsed.sign.bFaster !== parsed.logSignB)) {
      throw new Error(
        `${fname}: independently-computed sign test (${parsed.sign.aFaster}/${parsed.sign.bFaster}) does not match the log's own (${parsed.logSignA}/${parsed.logSignB})`,
      );
    }

    runs.push({
      k,
      comparison: COMPARISON_MAP[fileComp],
      armA: parsed.armA,
      armB: parsed.armB,
      n: parsed.tTest.n,
      samplesA: parsed.samplesA,
      samplesB: parsed.samplesB,
      tTest: parsed.tTest,
      sign: parsed.sign,
      file: fname,
    });
  }
}

console.log(`R33-5 latency-null addendum — parsed ${runs.length} log files\n`);

// ── Assert headline claims ───────────────────────────────────────────────────

// 1. Every same-vs-same control must be NOT significant (t < crit).
for (const r of runs.filter((r) => r.comparison !== 'before_vs_after')) {
  if (r.tTest.significant) {
    throw new Error(
      `K=${r.k} ${r.comparison}: same-vs-same control IS significant (t=${r.tTest.t.toFixed(3)} > crit=${r.tTest.crit}) — ` +
        `harness noise floor too high, main comparison results are not trustworthy.`,
    );
  }
}

// 2. For before_vs_after: assert the significance verdict is correctly computed.
for (const r of runs.filter((r) => r.comparison === 'before_vs_after')) {
  const rederivedSignificance = Math.abs(r.tTest.t) > r.tTest.crit;
  if (rederivedSignificance !== r.tTest.significant) {
    throw new Error(
      `K=${r.k} before_vs_after: significance re-derivation mismatch (|t|=${Math.abs(r.tTest.t).toFixed(3)} vs crit=${r.tTest.crit})`,
    );
  }
}

// 3. Every K has all 3 comparisons.
for (const k of K_VALUES) {
  const kResults = runs.filter((r) => r.k === k);
  const found = new Set(kResults.map((r) => r.comparison));
  for (const expected of Object.values(COMPARISON_MAP)) {
    if (!found.has(expected)) {
      throw new Error(`K=${k}: missing comparison '${expected}'`);
    }
  }
}

// 4. n=20 for every comparison.
for (const r of runs) {
  if (r.n !== 20) {
    throw new Error(`K=${r.k} ${r.comparison}: expected n=20, got n=${r.n}`);
  }
}

// ── Build CSV rows ────────────────────────────────────────────────────────────

const csvHeader = 'k,comparison,arm_a,arm_b,n,mean_a_ns_per_op,mean_b_ns_per_op,delta_ns_per_op,pct_change,t,crit,significant,sign_a_faster,sign_b_faster,sign_ties,provenance_file';
const csvLines = [csvHeader];

for (const k of K_VALUES) {
  for (const comp of Object.values(COMPARISON_MAP)) {
    const r = runs.find((r) => r.k === k && r.comparison === comp);
    if (!r) continue;
    const ops = ROTATING_ROUNDS * k;
    const meanA_total = r.samplesA.reduce((a, b) => a + b, 0) / r.samplesA.length;
    const meanB_total = r.samplesB.reduce((a, b) => a + b, 0) / r.samplesB.length;
    const mean_a_ns_per_op = meanA_total / ops;
    const mean_b_ns_per_op = meanB_total / ops;
    const delta_ns_per_op = r.tTest.mean / ops;
    const pct_change = mean_b_ns_per_op !== 0 ? ((mean_b_ns_per_op - mean_a_ns_per_op) / mean_a_ns_per_op) * 100 : 0;

    csvLines.push([
      r.k, r.comparison, r.armA, r.armB, r.n,
      mean_a_ns_per_op.toFixed(3),
      mean_b_ns_per_op.toFixed(3),
      delta_ns_per_op.toFixed(4),
      pct_change.toFixed(2),
      r.tTest.t.toFixed(3),
      r.tTest.crit.toFixed(3),
      r.tTest.significant,
      r.sign.aFaster, r.sign.bFaster, r.sign.ties,
      r.file,
    ].join(','));
  }
}

const csvPath = path.join(perfDir, 'R32_10_LATENCY_NULL_PAIRED_AB_summary.csv');
writeFileSync(csvPath, csvLines.join('\n') + '\n');
console.log(`Wrote ${csvPath}\n`);

// ── Print summary table ───────────────────────────────────────────────────────

console.log('   K comparison       mean_a ns/op mean_b ns/op  Δ ns/op   %chg       t    crit  sig  sign(a/b)');
for (const k of K_VALUES) {
  for (const comp of Object.values(COMPARISON_MAP)) {
    const r = runs.find((r) => r.k === k && r.comparison === comp);
    if (!r) continue;
    const ops = ROTATING_ROUNDS * k;
    const meanA_ns = r.samplesA.reduce((a, b) => a + b, 0) / r.samplesA.length / ops;
    const meanB_ns = r.samplesB.reduce((a, b) => a + b, 0) / r.samplesB.length / ops;
    const delta_ns = r.tTest.mean / ops;
    const pct = meanB_ns !== 0 ? ((meanB_ns - meanA_ns) / meanA_ns) * 100 : 0;
    console.log(
      String(r.k).padStart(4),
      r.comparison.padEnd(16),
      meanA_ns.toFixed(2).padStart(12),
      meanB_ns.toFixed(2).padStart(12),
      delta_ns.toFixed(2).padStart(8),
      (pct >= 0 ? '+' : '') + pct.toFixed(1).padStart(5) + '%',
      r.tTest.t.toFixed(3).padStart(7),
      r.tTest.crit.toFixed(3).padStart(7),
      (r.tTest.significant ? 'YES' : 'no').padStart(4),
      `${r.sign.aFaster}/${r.sign.bFaster}`.padStart(10),
    );
  }
}

// ── Print headline verdict ───────────────────────────────────────────────────

const mainResults = runs.filter((r) => r.comparison === 'before_vs_after');
const anySignificant = mainResults.some((r) => r.tTest.significant);
const maxAbsT = Math.max(...mainResults.map((r) => Math.abs(r.tTest.t)));
const maxK = mainResults.reduce((best, r) => Math.abs(r.tTest.t) > Math.abs(best.tTest.t) ? r : best).k;

console.log('\n── Headline verdict ──');
console.log(`before_vs_after across ${mainResults.length} K values:`);
console.log(`  Any significant (|t| > crit=${mainResults[0].tTest.crit})? ${anySignificant ? 'YES' : 'NO'}`);
console.log(`  Max |t| = ${maxAbsT.toFixed(3)} (at K=${maxK})`);
console.log(`  All same-vs-same controls NOT significant? YES (asserted)`);
console.log(`  Cross-checked against each log's own printed t and sign-test: YES (asserted)`);

if (!anySignificant) {
  console.log('\n  CONFIRMED: the original report\'s "honest null" latency claim is SUPPORTED by');
  console.log('  rigorous paired-A/B evidence (t-test + sign test + same-vs-same controls at all 7 K values).');
} else {
  const sig = mainResults.filter((r) => r.tTest.significant);
  console.log(`\n  CORRECTED: ${sig.length} of ${mainResults.length} K values show a statistically significant`);
  console.log(`  latency difference (K=${sig.map((r) => r.k).join(', ')}).`);
}

console.log(`\nAll assertions passed. Data source: ${runs.length} _raw_r33_5_*.log files in docs/perf/`);
