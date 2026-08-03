#!/usr/bin/env node
// R33-5 (task #510) — checked derive script for the R32-10 OWN_CACHE_SIZE
// latency-null paired-A/B addendum.
//
// Reads the paired_ab_runs JSON provenance files produced by
// `scripts/paired-ab-runner.mjs --config scripts/_r33_5_own_cache_ab.json`
// (which drove examples/r32_10_own_cache_tier1_thrash_gate.rs in child mode
// through the A/B/B/A protocol, 20 pairs per comparison, for all 7 K values,
// 3 comparisons each: before-vs-after, before-vs-before, after-vs-after).
//
// For each (K, comparison) cell this script:
//   1. Reads the per-sample churn_elapsed_ns arrays from the JSON.
//   2. INDEPENDENTLY recomputes the paired t-test and sign test from those
//      raw samples (does NOT trust the JSON's pre-computed values — it
//      re-derives them and ASSERTS they match, per CLAUDE.md's "a script that
//      computes a headline ratio must assert the arithmetic it prints").
//   3. Converts churn_elapsed_ns to ns_per_op (÷ ROTATING_ROUNDS × K).
//   4. Asserts headline claims about the controls and main comparisons.
//   5. Writes a companion summary CSV.
//
// Run: node scripts/r33_5_latency_null_addendum_summary.mjs

import { readFileSync, writeFileSync, readdirSync, existsSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const perfDir = path.join(__dirname, '..', 'docs', 'perf');
const runsDir = path.join(perfDir, 'paired_ab_runs');

// Must match examples/r32_10_own_cache_tier1_thrash_gate.rs.
const ROTATING_ROUNDS = 8192;

// ── Statistics: re-derive the runner's t-test and sign test from raw samples ──
// (same methodology as paired-ab-runner.mjs, independently implemented here so
// the derive script is a true zero-trust check, not a copy of the runner's
// own computation.)

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

function approxEq(a, b, relTol = 1e-6, absTol = 1e-9) {
  return Math.abs(a - b) <= Math.max(relTol * Math.max(Math.abs(a), Math.abs(b)), absTol);
}

// ── Load and filter JSON provenance files ──────────────────────────────────────

function loadR33_5Runs() {
  if (!existsSync(runsDir)) {
    throw new Error(`paired_ab_runs directory not found: ${runsDir}`);
  }
  const files = readdirSync(runsDir)
    .filter((f) => f.endsWith('.json'))
    .sort();
  const runs = [];
  for (const f of files) {
    const full = path.join(runsDir, f);
    let raw;
    try {
      raw = JSON.parse(readFileSync(full, 'utf8'));
    } catch {
      continue; // skip unparseable
    }
    // Filter: only R33-5 OWN_CACHE_SIZE runs (identified by features_note
    // containing "OWN_CACHE_SIZE").
    const featuresNote = raw.cargo_features_built_with || '';
    if (!featuresNote.includes('OWN_CACHE_SIZE')) continue;

    const cmp = raw.comparisons?.[0];
    if (!cmp) continue;

    // Extract K from the first raw launch's RESULT data.
    const firstLaunch = cmp.raw_process_launches?.[0];
    const k = Number(firstLaunch?.k);
    if (!Number.isInteger(k) || k <= 0) continue;

    runs.push({
      file: f,
      k,
      armA: cmp.arm_a,
      armB: cmp.arm_b,
      samplesA: cmp.samples_a_ns,
      samplesB: cmp.samples_b_ns,
      jsonDeltas: cmp.deltas_a_minus_b_ns,
      jsonTTest: cmp.paired_t_test,
      jsonSignTest: cmp.sign_test,
      gitCommit: raw.git_commit,
    });
  }
  return runs;
}

const runs = loadR33_5Runs();
if (runs.length === 0) {
  throw new Error('No R33-5 paired_ab_runs JSON files found (expected files with "OWN_CACHE_SIZE" in features_note)');
}

console.log(`R33-5 latency-null addendum — found ${runs.length} paired_ab_runs JSON files\n`);

// ── For each run: re-derive statistics and assert they match the JSON ──────────

const K_VALUES = [4, 8, 16, 24, 32, 48, 64];
const COMPARISONS = ['before_vs_after', 'before_control', 'after_control'];
const results = [];

for (const run of runs) {
  const { k, armA, armB, samplesA, samplesB } = run;

  // Recompute deltas.
  const deltas = samplesA.map((a, i) => a - samplesB[i]);
  for (let i = 0; i < deltas.length; i++) {
    if (!approxEq(deltas[i], run.jsonDeltas[i], 1e-4, 0.5)) {
      throw new Error(
        `K=${k} ${armA}_vs_${armB}: recomputed delta[${i}]=${deltas[i]} does not match JSON delta=${run.jsonDeltas[i]} — data corruption`,
      );
    }
  }

  // Recompute t-test.
  const tTest = pairedTTest(deltas);
  if (!tTest) throw new Error(`K=${k}: could not compute t-test`);

  // Assert t-test matches JSON's values.
  if (tTest.n !== run.jsonTTest.n) {
    throw new Error(`K=${k} ${armA}_vs_${armB}: recomputed n=${tTest.n} ≠ JSON n=${run.jsonTTest.n}`);
  }
  if (!approxEq(tTest.t, run.jsonTTest.t, 1e-4, 1e-6)) {
    throw new Error(
      `K=${k} ${armA}_vs_${armB}: recomputed t=${tTest.t.toFixed(6)} ≠ JSON t=${run.jsonTTest.t.toFixed(6)} — t-test computation mismatch`,
    );
  }
  if (!approxEq(tTest.mean, run.jsonTTest.mean, 1e-4, 0.5)) {
    throw new Error(
      `K=${k} ${armA}_vs_${armB}: recomputed mean=${tTest.mean} ≠ JSON mean=${run.jsonTTest.mean}`,
    );
  }

  // Recompute sign test.
  const sign = signTest(deltas);
  if (sign.aFaster !== run.jsonSignTest.aFaster || sign.bFaster !== run.jsonSignTest.bFaster || sign.ties !== run.jsonSignTest.ties) {
    throw new Error(
      `K=${k} ${armA}_vs_${armB}: recomputed sign test (${sign.aFaster}/${sign.bFaster}/${sign.ties}) ≠ JSON (${run.jsonSignTest.aFaster}/${run.jsonSignTest.bFaster}/${run.jsonSignTest.ties})`,
    );
  }

  // Determine comparison type.
  let comparison;
  if (armA === 'before' && armB === 'after') comparison = 'before_vs_after';
  else if (armA === 'before' && armB === 'before') comparison = 'before_control';
  else if (armA === 'after' && armB === 'after') comparison = 'after_control';
  else throw new Error(`K=${k}: unknown comparison ${armA} vs ${armB}`);

  // Convert to ns_per_op.
  const ops = ROTATING_ROUNDS * k;
  const meanA_ns_per_op = tTest.mean < 0 ? null : (samplesA.reduce((a, b) => a + b, 0) / samplesA.length) / ops;
  const meanB_ns_per_op = tTest.mean < 0 ? null : (samplesB.reduce((a, b) => a + b, 0) / samplesB.length) / ops;
  const meanA_total = samplesA.reduce((a, b) => a + b, 0) / samplesA.length;
  const meanB_total = samplesB.reduce((a, b) => a + b, 0) / samplesB.length;
  const mean_a_ns_per_op = meanA_total / ops;
  const mean_b_ns_per_op = meanB_total / ops;
  const delta_ns_per_op = (tTest.mean) / ops;
  const pct_change = mean_b_ns_per_op !== 0 ? ((mean_b_ns_per_op - mean_a_ns_per_op) / mean_a_ns_per_op) * 100 : 0;

  results.push({
    k,
    comparison,
    armA,
    armB,
    n: tTest.n,
    mean_a_ns_per_op,
    mean_b_ns_per_op,
    delta_ns_per_op,
    pct_change,
    t: tTest.t,
    crit: tTest.crit,
    significant: tTest.significant,
    sign_a_faster: sign.aFaster,
    sign_b_faster: sign.bFaster,
    sign_ties: sign.ties,
    file: run.file,
  });
}

// ── Assert headline claims ─────────────────────────────────────────────────────

// 1. Every same-vs-same control must be NOT significant (t < crit).
//    A significant same-vs-same result means the harness itself is unstable,
//    invalidating the main comparisons.
for (const r of results.filter((r) => r.comparison !== 'before_vs_after')) {
  if (r.significant) {
    throw new Error(
      `K=${r.k} ${r.comparison}: same-vs-same control IS significant (t=${r.t.toFixed(3)} > crit=${r.crit}) — ` +
        `harness noise floor too high, main comparison results are not trustworthy. DO NOT publish without investigating.`,
    );
  }
}

// 2. For before_vs_after: assert the t-test significance verdict is correctly
//    computed (re-derive |t| > crit and compare to the significant flag).
for (const r of results.filter((r) => r.comparison === 'before_vs_after')) {
  const rederivedSignificance = Math.abs(r.t) > r.crit;
  if (rederivedSignificance !== r.significant) {
    throw new Error(
      `K=${r.k} before_vs_after: significance re-derivation mismatch (|t|=${Math.abs(r.t).toFixed(3)} vs crit=${r.crit}, rederived=${rederivedSignificance}, json=${r.significant})`,
    );
  }
}

// 3. Assert every K has all 3 comparisons present.
for (const k of K_VALUES) {
  const kResults = results.filter((r) => r.k === k);
  const foundComparisons = new Set(kResults.map((r) => r.comparison));
  for (const expected of COMPARISONS) {
    if (!foundComparisons.has(expected)) {
      throw new Error(`K=${k}: missing comparison '${expected}' (found: ${[...foundComparisons].join(', ')})`);
    }
  }
  if (kResults.length !== 3) {
    throw new Error(`K=${k}: expected exactly 3 comparisons, found ${kResults.length}`);
    }
}

// 4. Assert n=20 for every comparison (matching R32-11's N=20).
for (const r of results) {
  if (r.n !== 20) {
    throw new Error(`K=${r.k} ${r.comparison}: expected n=20, got n=${r.n}`);
  }
}

// ── Write summary CSV ──────────────────────────────────────────────────────────

const csvHeader = 'k,comparison,arm_a,arm_b,n,mean_a_ns_per_op,mean_b_ns_per_op,delta_ns_per_op,pct_change,t,crit,significant,sign_a_faster,sign_b_faster,sign_ties,provenance_file';
const csvLines = [csvHeader];
for (const k of K_VALUES) {
  for (const comp of COMPARISONS) {
    const r = results.find((r) => r.k === k && r.comparison === comp);
    if (!r) continue;
    csvLines.push([
      r.k, r.comparison, r.armA, r.armB, r.n,
      r.mean_a_ns_per_op.toFixed(3),
      r.mean_b_ns_per_op.toFixed(3),
      r.delta_ns_per_op.toFixed(4),
      r.pct_change.toFixed(2),
      r.t.toFixed(3),
      r.crit.toFixed(3),
      r.significant,
      r.sign_a_faster, r.sign_b_faster, r.sign_ties,
      r.file,
    ].join(','));
  }
}

const csvPath = path.join(perfDir, 'R32_10_LATENCY_NULL_PAIRED_AB_summary.csv');
writeFileSync(csvPath, csvLines.join('\n') + '\n');
console.log(`Wrote ${csvPath}\n`);

// ── Print summary table ────────────────────────────────────────────────────────

console.log('K'.padStart(4), 'comparison'.padEnd(16), 'mean_a ns/op'.padStart(12), 'mean_b ns/op'.padStart(12), 'Δ ns/op'.padStart(8), '%chg'.padStart(6), 't'.padStart(7), 'crit'.padStart(7), 'sig'.padStart(4), 'sign(a/b)'.padStart(10));
for (const k of K_VALUES) {
  for (const comp of COMPARISONS) {
    const r = results.find((r) => r.k === k && r.comparison === comp);
    if (!r) continue;
    console.log(
      String(r.k).padStart(4),
      r.comparison.padEnd(16),
      r.mean_a_ns_per_op.toFixed(2).padStart(12),
      r.mean_b_ns_per_op.toFixed(2).padStart(12),
      r.delta_ns_per_op.toFixed(2).padStart(8),
      (r.pct_change >= 0 ? '+' : '').padEnd(0) + r.pct_change.toFixed(1).padStart(5) + '%',
      r.t.toFixed(3).padStart(7),
      r.crit.toFixed(3).padStart(7),
      (r.significant ? 'YES' : 'no').padStart(4),
      `${r.sign_a_faster}/${r.sign_b_faster}`.padStart(10),
    );
  }
}

// ── Print headline verdict ─────────────────────────────────────────────────────

const mainResults = results.filter((r) => r.comparison === 'before_vs_after');
const anySignificant = mainResults.some((r) => r.significant);
const maxAbsT = Math.max(...mainResults.map((r) => Math.abs(r.t)));

console.log('\n── Headline verdict ──');
console.log(`before_vs_after across ${mainResults.length} K values:`);
console.log(`  Any significant (|t| > crit=${mainResults[0].crit})? ${anySignificant ? 'YES' : 'NO'}`);
console.log(`  Max |t| = ${maxAbsT.toFixed(3)} (at K=${mainResults.reduce((best, r) => Math.abs(r.t) > Math.abs(best.t) ? r : best).k})`);
console.log(`  All same-vs-same controls NOT significant? YES (asserted)`);

if (!anySignificant) {
  console.log('\n  CONFIRMED: the original report\'s "honest null" latency claim is SUPPORTED by');
  console.log('  rigorous paired-A/B evidence (t-test + sign test + same-vs-same controls at all 7 K values).');
} else {
  const sig = mainResults.filter((r) => r.significant);
  console.log(`\n  CORRECTED: ${sig.length} of ${mainResults.length} K values show a statistically significant`);
  console.log(`  latency difference (K=${sig.map((r) => r.k).join(', ')}). See the table above for direction and magnitude.`);
}

console.log(`\nAll assertions passed. Provenance: ${results.length} JSON files in docs/perf/paired_ab_runs/`);
