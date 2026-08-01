// R31-8 (task #488): derives `docs/perf/R31_8_NARROW_GATE_MATCHED_STATE_REMEASUREMENT_summary.csv`
// from the raw provenance JSON files `scripts/paired-ab-runner.mjs` wrote for
// each of this task's 7 runs (narrow N=1/2/4 real-process A/B + 2 same-vs-same
// controls, scan-isolation microjudge A/B + 1 same-vs-same control). This is
// the ONE checked script that turns the raw per-sample data into the report's
// tables and headline ratios (CLAUDE.md's R30-9 rule: "raw per-sample data
// written first, tables/ratios computed and asserted by one checked script,
// never hand-transcribed" — same pattern as `scripts/r31_1_derive_report_data.mjs`).
//
// Also asserts this report's own headline claims in-script (CLAUDE.md rule 6):
// the segments_reserved_total delta is IDENTICAL across arms at every N (the
// contradiction this task exists to resolve), and the narrow-timing / scan-
// isolation signals are both real (t past crit) and both same-vs-same
// controls are noise (t under crit).
//
// Usage:
//   node scripts/r31_8_derive_report_data.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from 'node:fs';

const ROOT = new URL('../', import.meta.url);
const read = (p) => JSON.parse(readFileSync(new URL(p, ROOT), 'utf8'));

const landingCommit = process.argv[2] || 'UNFILLED';
// Immutable source identity (CLAUDE.md's R29-6 rule): a git tree-object SHA
// computed via `git write-tree` against a SCOPED temporary index (base
// commit ac7845d + every file this task changed/added staged into a
// throwaway index file, never touching the shared repo index/working tree
// of any concurrently-running agent). Covers ALL of this task's
// source/harness/script files (see this report's §8.0 for the full file
// list) -- this is the tree every number in this CSV was produced by, after
// the zero-trust-review cleanup pass and the corresponding fresh re-run
// (§8.0's "post-measurement source cleanup" note explains the one round of
// dead-code removal + oracle strengthening that happened between the first
// and final measurement pass).
const baseCommit = 'ac7845d0ed90ca4ba34e2abbc2ba9a4113ef1d36';
const sourceTreeSha = 'a35dd00d9336a88a28059809391e2f44d0fe900c';

const RUNS = {
  narrow_n1: 'docs/perf/paired_ab_runs/2026-08-01T21-48-36-430Z.json',
  narrow_n1_same_vs_same: 'docs/perf/paired_ab_runs/2026-08-01T21-48-46-358Z.json',
  narrow_n2: 'docs/perf/paired_ab_runs/2026-08-01T21-49-04-021Z.json',
  narrow_n4: 'docs/perf/paired_ab_runs/2026-08-01T21-49-13-946Z.json',
  narrow_n4_same_vs_same: 'docs/perf/paired_ab_runs/2026-08-01T21-49-23-652Z.json',
  scan_isolation: 'docs/perf/paired_ab_runs/2026-08-01T22-22-39-119Z.json',
  scan_isolation_same_vs_same: 'docs/perf/paired_ab_runs/2026-08-01T22-22-43-158Z.json',
};

const data = {};
for (const [key, path] of Object.entries(RUNS)) {
  data[key] = read(path);
}

// ── Extract the paired t-test / sign-test summary for each run ─────────────
function ttestRow(runKey, label) {
  const j = data[runKey];
  const cmp = j.comparisons[0];
  return {
    run: label,
    metric: j.metric,
    arm_a: cmp.arm_a,
    arm_b: cmp.arm_b,
    n: cmp.paired_t_test.n,
    mean_delta: cmp.paired_t_test.mean,
    sd: cmp.paired_t_test.sd,
    se: cmp.paired_t_test.se,
    t: cmp.paired_t_test.t,
    df: cmp.paired_t_test.df,
    crit: cmp.paired_t_test.crit,
    significant: cmp.paired_t_test.significant,
    sign_a_faster: cmp.sign_test.aFaster,
    sign_b_faster: cmp.sign_test.bFaster,
    sign_ties: cmp.sign_test.ties,
  };
}

const ttestRows = [
  ttestRow('narrow_n1', 'narrow_n1_off_vs_on'),
  ttestRow('narrow_n1_same_vs_same', 'narrow_n1_same_vs_same_control'),
  ttestRow('narrow_n2', 'narrow_n2_off_vs_on'),
  ttestRow('narrow_n4', 'narrow_n4_off_vs_on'),
  ttestRow('narrow_n4_same_vs_same', 'narrow_n4_same_vs_same_control'),
  ttestRow('scan_isolation', 'scan_isolation_off_vs_on'),
  ttestRow('scan_isolation_same_vs_same', 'scan_isolation_same_vs_same_control'),
];

// ── Matched-state proof: segments_reserved_total delta must be IDENTICAL
//    across arms at every N (this is the headline claim this task exists to
//    check — the OLD report showed off=10 vs on=14, unexplained and
//    backwards from its own prose) ─────────────────────────────────────────
function matchedStateRows(runKey, nLabel) {
  const j = data[runKey];
  const cmp = j.comparisons[0];
  const byArm = { off: [], on: [] };
  for (const launch of cmp.raw_process_launches) {
    const armKey = launch.arm.endsWith('_off') ? 'off' : 'on';
    byArm[armKey].push(launch);
  }
  const rows = [];
  for (const armKey of ['off', 'on']) {
    const launches = byArm[armKey];
    const baselineSet = new Set(launches.map((l) => l.segments_reserved_baseline));
    const preTimingSet = new Set(launches.map((l) => l.segments_reserved_pre_timing));
    const usedBytesSet = new Set(launches.map((l) => l.large_cache_used_bytes_pre_timing));
    const oracleSlotsSet = new Set(launches.map((l) => l.oracle_total_slots));
    if (baselineSet.size !== 1 || preTimingSet.size !== 1 || usedBytesSet.size !== 1 || oracleSlotsSet.size !== 1) {
      throw new Error(
        `HEADLINE ASSERTION FAILED: ${nLabel}/${armKey} expected every one of ${launches.length} ` +
          `process launches to report IDENTICAL segments_reserved_baseline/pre_timing/used_bytes/oracle_total_slots ` +
          `(matched-state proof) -- got baseline=${[...baselineSet]}, pre_timing=${[...preTimingSet]}, ` +
          `used_bytes=${[...usedBytesSet]}, oracle_slots=${[...oracleSlotsSet]}`,
      );
    }
    const baseline = [...baselineSet][0];
    const preTiming = [...preTimingSet][0];
    rows.push({
      n_label: nLabel,
      arm: armKey,
      n_launches: launches.length,
      segments_reserved_baseline: baseline,
      segments_reserved_pre_timing: preTiming,
      segments_reserved_delta: preTiming - baseline,
      large_cache_used_bytes_pre_timing: [...usedBytesSet][0],
      oracle_total_slots: [...oracleSlotsSet][0],
    });
  }
  return rows;
}

const matchedRowsN1 = matchedStateRows('narrow_n1', 'n=1');
const matchedRowsN2 = matchedStateRows('narrow_n2', 'n=2');
const matchedRowsN4 = matchedStateRows('narrow_n4', 'n=4');
const allMatchedRows = [...matchedRowsN1, ...matchedRowsN2, ...matchedRowsN4];

// Assert: segments_reserved_delta is IDENTICAL between off and on, at every N
// (the contradiction this task exists to resolve -- it must now be gone).
for (const nLabel of ['n=1', 'n=2', 'n=4']) {
  const off = allMatchedRows.find((r) => r.n_label === nLabel && r.arm === 'off');
  const on = allMatchedRows.find((r) => r.n_label === nLabel && r.arm === 'on');
  if (off.segments_reserved_delta !== on.segments_reserved_delta) {
    throw new Error(
      `HEADLINE ASSERTION FAILED: ${nLabel} expected segments_reserved_delta to be IDENTICAL between ` +
        `off (${off.segments_reserved_delta}) and on (${on.segments_reserved_delta}) -- the R31-3/task #488 ` +
        `contradiction (off=10 vs on=14 in the superseded report) was supposed to be resolved by the ` +
        `matched-state fix, not still present.`,
    );
  }
  if (off.segments_reserved_delta !== 9) {
    throw new Error(
      `HEADLINE ASSERTION FAILED: ${nLabel} expected segments_reserved_delta == 9 (MATERIALIZE_N, one ` +
        `fresh OS reserve per burst-phase cache miss) in both arms, got ${off.segments_reserved_delta}`,
    );
  }
  // Expected asymmetry: ON retains exactly one more entry than OFF (the
  // smallest materialisation size, nine_sizes[0], which OFF's FIFO eviction
  // dropped and ON's wider cache kept) -- named, predicted, asserted exactly.
  const usedBytesDelta = on.large_cache_used_bytes_pre_timing - off.large_cache_used_bytes_pre_timing;
  const EXPECTED_S0_BYTES = 8388608; // 2 segments x 4 MiB = the smallest of the 9 materialize_sizes()
  if (usedBytesDelta !== EXPECTED_S0_BYTES) {
    throw new Error(
      `HEADLINE ASSERTION FAILED: ${nLabel} expected large_cache_used_bytes_pre_timing delta (on - off) ` +
        `== ${EXPECTED_S0_BYTES} (exactly one nine_sizes[0]-sized entry, the one ON retains that OFF ` +
        `evicted), got ${usedBytesDelta}`,
    );
  }
}

// ── Assert the narrow-timing and scan-isolation signals are real, and both
//    same-vs-same controls are noise ─────────────────────────────────────
// "Overwhelming majority", not strict unanimity: t past crit is the actual
// significance criterion this project's own methodology doc
// (docs/perf/R5_R2_CHURN_REGRESSION_PAIRED_AB.md) uses ("t past crit ... OR
// sign test heavily lopsided, e.g. 17+/20"); requiring literal 20/20 here
// would reject a run that is genuinely significant by t but has one
// borderline pair (as narrow_n1 does: 19/20, still far past the 17+/20
// "heavily lopsided" bar this project's own runner-output guidance names).
const SIGN_TEST_MAJORITY_FLOOR = 17; // out of 20 -- this project's own "heavily lopsided" bar
function assertSignificant(row, expectDirection) {
  if (!row.significant) {
    throw new Error(`HEADLINE ASSERTION FAILED: ${row.run} expected a REAL (significant) signal, got t=${row.t} (not significant)`);
  }
  if (expectDirection === 'on_slower' && row.mean_delta >= 0) {
    throw new Error(`HEADLINE ASSERTION FAILED: ${row.run} expected mean_delta (off - on) < 0 (on SLOWER), got ${row.mean_delta}`);
  }
  if (row.sign_a_faster < SIGN_TEST_MAJORITY_FLOOR) {
    throw new Error(
      `HEADLINE ASSERTION FAILED: ${row.run} expected a heavily-lopsided sign test (off-faster >= ${SIGN_TEST_MAJORITY_FLOOR}/${row.n}), ` +
        `got off-faster=${row.sign_a_faster}/${row.n}, on-faster=${row.sign_b_faster}/${row.n}`,
    );
  }
}
function assertNoise(row) {
  if (row.significant) {
    throw new Error(`HEADLINE ASSERTION FAILED: ${row.run} expected a NOISE (not significant) same-vs-same control, got t=${row.t} (significant)`);
  }
}

assertSignificant(ttestRows[0], 'on_slower'); // narrow_n1
assertNoise(ttestRows[1]); // narrow_n1 same-vs-same
assertSignificant(ttestRows[2], 'on_slower'); // narrow_n2
assertSignificant(ttestRows[3], 'on_slower'); // narrow_n4
assertNoise(ttestRows[4]); // narrow_n4 same-vs-same
assertSignificant(ttestRows[5], 'on_slower'); // scan_isolation
assertNoise(ttestRows[6]); // scan_isolation same-vs-same

console.log('=== Headline assertions (checked in-script per CLAUDE.md rule 6) ===\n');
console.log('Matched-state proof (segments_reserved_total delta, off vs on, per N):');
for (const nLabel of ['n=1', 'n=2', 'n=4']) {
  const off = allMatchedRows.find((r) => r.n_label === nLabel && r.arm === 'off');
  const on = allMatchedRows.find((r) => r.n_label === nLabel && r.arm === 'on');
  console.log(
    `  ${nLabel}: off delta=${off.segments_reserved_delta} (baseline=${off.segments_reserved_baseline}->pre_timing=${off.segments_reserved_pre_timing}), ` +
      `on delta=${on.segments_reserved_delta} (baseline=${on.segments_reserved_baseline}->pre_timing=${on.segments_reserved_pre_timing}) -- IDENTICAL, confirmed`,
  );
}
console.log('\nNarrow real-process A/B (elapsed_ns, off - on, negative = ON slower):');
for (const row of [ttestRows[0], ttestRows[2], ttestRows[3]]) {
  console.log(`  ${row.run}: mean_delta=${row.mean_delta.toFixed(1)} ns  t=${row.t.toFixed(3)}  crit=${row.crit}  sign off-faster=${row.sign_a_faster}/${row.n}  => ${row.significant ? 'REAL' : 'NOISE'}`);
}
console.log('\nSame-vs-same controls (harness sanity):');
for (const row of [ttestRows[1], ttestRows[4], ttestRows[6]]) {
  console.log(`  ${row.run}: mean_delta=${row.mean_delta.toFixed(1)} ns  t=${row.t.toFixed(3)}  crit=${row.crit}  => ${row.significant ? 'REAL (BUG!)' : 'NOISE, as expected'}`);
}
console.log('\nScan-isolation microjudge (ns_per_round, off - on, negative = wider scan slower):');
{
  const row = ttestRows[5];
  console.log(`  ${row.run}: mean_delta=${row.mean_delta.toFixed(1)} ns  t=${row.t.toFixed(3)}  crit=${row.crit}  sign off-faster=${row.sign_a_faster}/${row.n}  => ${row.significant ? 'REAL' : 'NOISE'}`);
}

// Per-arm ns_per_round for the scan-isolation ratio (8 vs 40 slots)
const scanCmp = data.scan_isolation.comparisons[0];
const scanOffMean = scanCmp.samples_a_ns.reduce((a, b) => a + b, 0) / scanCmp.samples_a_ns.length;
const scanOnMean = scanCmp.samples_b_ns.reduce((a, b) => a + b, 0) / scanCmp.samples_b_ns.length;
const scanRatio = scanOnMean / scanOffMean;
console.log(
  `\nScan-isolation per-arm arithmetic mean: off(8 slots)=${scanOffMean.toFixed(1)} ns/round, ` +
    `on(40 slots)=${scanOnMean.toFixed(1)} ns/round, ratio on/off=${scanRatio.toFixed(2)}x`,
);
// Sanity-assert the ratio is in a plausible range (not a wild outlier from a
// harness bug) -- a genuine O(N) scan-bound cost with 5x more slots (40 vs 8)
// should show SOME multiple > 1x, but need not be exactly 5x (fixed overhead
// + branch prediction + cache-line effects dilute a naive linear scaling
// prediction) -- asserting it is REAL and in the SAME direction as the
// paired t-test already confirmed, not re-deriving a new claim.
if (!(scanRatio > 1.0)) {
  throw new Error(`HEADLINE ASSERTION FAILED: scan-isolation ratio on/off expected > 1.0 (wider scan costs more), got ${scanRatio}`);
}

// ── Write summary CSV ───────────────────────────────────────────────────────
const META = {
  cpu: 'Intel_Core_i7-11800H_2.30GHz',
  os: 'Windows_10_Pro_10.0.19045',
  rustc: '1.97.0',
};

const header = [
  'base_commit',
  'immutable_tree_sha',
  'landing_commit',
  'artifact',
  'metric',
  'arm_or_comparison',
  'n_or_label',
  'value',
  'unit',
  'notes',
].join(',');

const csvLines = [header];
function pushRow(artifact, metric, arm, nLabel, value, unit, notes) {
  csvLines.push(
    [baseCommit, sourceTreeSha, landingCommit, artifact, metric, arm, nLabel, value, unit, `"${notes.replace(/"/g, "'")}"`].join(','),
  );
}

for (const row of ttestRows) {
  const isSameVsSame = row.arm_a === row.arm_b;
  const labelA = isSameVsSame ? `${row.arm_a}(A-slot)` : row.arm_a;
  const labelB = isSameVsSame ? `${row.arm_b}(B-slot)` : row.arm_b;
  pushRow(
    row.run,
    'mean_delta_ns_arithmetic_mean',
    `${labelA}_minus_${labelB}`,
    `n=${row.n}`,
    row.mean_delta.toFixed(3),
    'ns',
    `t=${row.t.toFixed(3)} df=${row.df} crit=${row.crit} sign ${labelA}-faster=${row.sign_a_faster}/${row.n} ${labelB}-faster=${row.sign_b_faster}/${row.n} significant=${row.significant}`,
  );
}
for (const r of allMatchedRows) {
  pushRow(
    'matched_state_proof',
    'segments_reserved_total_delta',
    r.arm,
    r.n_label,
    r.segments_reserved_delta,
    'count',
    `baseline=${r.segments_reserved_baseline} pre_timing=${r.segments_reserved_pre_timing} n_launches=${r.n_launches} identical-across-launches=1`,
  );
  pushRow(
    'matched_state_proof',
    'large_cache_used_bytes_pre_timing',
    r.arm,
    r.n_label,
    r.large_cache_used_bytes_pre_timing,
    'bytes',
    `oracle_total_slots=${r.oracle_total_slots}`,
  );
}
pushRow('scan_isolation', 'ns_per_round_arithmetic_mean', 'off_8slots', 'n=20', scanOffMean.toFixed(1), 'ns', 'worst-case scan position (fitting entry at last-scanned slot), AllocCore direct');
pushRow('scan_isolation', 'ns_per_round_arithmetic_mean', 'on_40slots', 'n=20', scanOnMean.toFixed(1), 'ns', 'worst-case scan position (fitting entry at last-scanned slot), AllocCore direct');
pushRow('scan_isolation', 'ratio_on_over_off', 'on_over_off', 'n=20', scanRatio.toFixed(3), 'ratio', 'wider-scan-bound cost multiple, asserted > 1.0 in-script');

const outPath = 'docs/perf/R31_8_NARROW_GATE_MATCHED_STATE_REMEASUREMENT_summary.csv';
writeFileSync(new URL(outPath, ROOT), csvLines.join('\n') + '\n');
console.log(`\nwrote ${csvLines.length - 1} data rows to ${outPath} (base_commit=${baseCommit}, immutable_tree_sha=${sourceTreeSha}, landing_commit=${landingCommit})`);
