// R31-10 cost side (task #492): derives `docs/perf/R31_10_TRIM_CURRENT_THREAD_RSS_GATE_summary.csv`'s
// companion cost-side rows from the raw provenance JSON files
// `scripts/paired-ab-runner.mjs` wrote for the TRIM-vs-NO_TRIM burst2-latency
// A/B and its same-vs-same control. This is the ONE checked script that turns
// the raw per-sample data into the report's tables and headline numbers
// (CLAUDE.md's rule: "raw per-sample data written first, tables/ratios
// computed and asserted by one checked script, never hand-transcribed" — same
// pattern as `scripts/r31_8_derive_report_data.mjs`).
//
// Also asserts this report's own headline claims in-script (CLAUDE.md rule
// 6): the TRIM-vs-NO_TRIM burst2 signal is real (t past crit, heavily
// lopsided sign test) and the same-vs-same control is noise (t under crit).
//
// Usage:
//   node scripts/r31_10_derive_cost_report_data.mjs <ab_run.json> <same_vs_same_run.json> [landing_commit_sha]

import { readFileSync, writeFileSync } from 'node:fs';

const ROOT = new URL('../', import.meta.url);
const read = (p) => JSON.parse(readFileSync(new URL(p, ROOT), 'utf8'));

const [, , abRunArg, sameVsSameArg, landingCommitArg] = process.argv;
if (!abRunArg || !sameVsSameArg) {
  throw new Error(
    'usage: node scripts/r31_10_derive_cost_report_data.mjs <ab_run.json> <same_vs_same_run.json> [landing_commit_sha]',
  );
}
const landingCommit = landingCommitArg || 'UNFILLED';

// Immutable source identity (CLAUDE.md's R29-6 rule, form 2: git tree-object
// SHA via `git write-tree` against a SCOPED temporary index — base commit +
// every file this task changed/added staged into a throwaway index, never
// touching the shared repo index/working tree of any concurrently-running
// agent). Covers exactly this task's (#492) changed/added files:
// src/global/sefer_alloc.rs, src/global/tls_heap.rs,
// tests/r31_10_trim_current_thread_api.rs, examples/r31_10_trim_cost_gate.rs,
// docs/perf/r31_10_cost_ab_config.json.
const baseCommit = '45f3d83fdfcc2326f8dc33d12d5ad45fc36d3f5f';
const sourceTreeSha = '63eb2faa84cba9131786871ba01c26d0460d2b5b';

const abRun = read(abRunArg);
const sameVsSame = read(sameVsSameArg);

function ttestRow(json, label) {
  const cmp = json.comparisons[0];
  return {
    run: label,
    metric: json.metric,
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

const abRow = ttestRow(abRun, 'trim_vs_no_trim_burst2_elapsed_ns');
const controlRow = ttestRow(sameVsSame, 'trim_vs_trim_same_vs_same_control');

// ── Headline assertions (CLAUDE.md rule 6: a script that computes a headline
//    ratio/verdict must assert the arithmetic/significance it prints, not
//    just print a hand-computed string) ──────────────────────────────────
const SIGN_TEST_MAJORITY_FLOOR = 17; // out of 20 -- this project's "heavily lopsided" bar

if (!abRow.significant) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: TRIM vs NO_TRIM burst2 elapsed_ns expected a REAL (significant) ` +
      `signal, got t=${abRow.t} (not significant)`,
  );
}
if (abRow.mean_delta <= 0) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: expected mean_delta (TRIM - NO_TRIM) > 0 (TRIM's burst2 SLOWER), ` +
      `got ${abRow.mean_delta}`,
  );
}
if (abRow.sign_b_faster < SIGN_TEST_MAJORITY_FLOOR) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: expected a heavily-lopsided sign test (NO_TRIM-faster >= ` +
      `${SIGN_TEST_MAJORITY_FLOOR}/${abRow.n}), got NO_TRIM-faster=${abRow.sign_b_faster}/${abRow.n}`,
  );
}
if (controlRow.significant) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: same-vs-same control expected NOISE (not significant), got t=${controlRow.t} (significant)`,
  );
}

// ── Per-launch mechanism-oracle + trim-latency extraction ──────────────────
// Every raw launch in the AB run carries `trim_call_ns` (0 for NO_TRIM),
// `action_released_delta` (must be >0 for every TRIM launch, ==0 for every
// NO_TRIM launch), and `burst2_reserved_delta` (fresh-OS-reservation count
// during the timed burst2 — the cold-restart-cost mechanism evidence).
const cmp = abRun.comparisons[0];
const trimLaunches = cmp.raw_process_launches.filter((l) => l.arm === 'TRIM');
const noTrimLaunches = cmp.raw_process_launches.filter((l) => l.arm === 'NO_TRIM');

for (const l of trimLaunches) {
  if (!(l.action_released_delta > 0)) {
    throw new Error(
      `HEADLINE ASSERTION FAILED (mechanism oracle): TRIM launch (block=${l.block}, slot=${l.slot}) ` +
        `expected action_released_delta > 0, got ${l.action_released_delta}`,
    );
  }
  if (!(l.burst2_reserved_delta > 0)) {
    throw new Error(
      `HEADLINE ASSERTION FAILED (mechanism oracle): TRIM launch (block=${l.block}, slot=${l.slot}) ` +
        `expected burst2_reserved_delta > 0 (cold re-materialisation), got ${l.burst2_reserved_delta}`,
    );
  }
}
for (const l of noTrimLaunches) {
  if (l.action_released_delta !== 0) {
    throw new Error(
      `HEADLINE ASSERTION FAILED (mechanism oracle): NO_TRIM launch (block=${l.block}, slot=${l.slot}) ` +
        `expected action_released_delta == 0, got ${l.action_released_delta}`,
    );
  }
  if (l.burst2_reserved_delta !== 0) {
    throw new Error(
      `HEADLINE ASSERTION FAILED (mechanism oracle): NO_TRIM launch (block=${l.block}, slot=${l.slot}) ` +
        `expected burst2_reserved_delta == 0 (warm cache, no fresh reservation), got ${l.burst2_reserved_delta}`,
    );
  }
}

const mean = (xs) => xs.reduce((a, b) => a + b, 0) / xs.length;
const trimCallNsMean = mean(trimLaunches.map((l) => l.trim_call_ns));
const trimBurst2Mean = mean(trimLaunches.map((l) => l.elapsed_ns));
const noTrimBurst2Mean = mean(noTrimLaunches.map((l) => l.elapsed_ns));
const coldStartDeltaNs = trimBurst2Mean - noTrimBurst2Mean;
const coldStartRatio = trimBurst2Mean / noTrimBurst2Mean;

// Sanity-assert the derived ratio is not a wild harness artifact.
if (!(coldStartRatio > 1.0)) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: cold-start ratio (TRIM burst2 / NO_TRIM burst2) expected > 1.0, got ${coldStartRatio}`,
  );
}

console.log('=== Headline assertions (checked in-script per CLAUDE.md rule 6) ===\n');
console.log('Mechanism oracle (per-launch, CLAUDE.md R30-8 path-activation rule):');
console.log(
  `  TRIM: ${trimLaunches.length}/${trimLaunches.length} launches action_released_delta > 0, ` +
    `${trimLaunches.length}/${trimLaunches.length} launches burst2_reserved_delta > 0 -- CONFIRMED`,
);
console.log(
  `  NO_TRIM: ${noTrimLaunches.length}/${noTrimLaunches.length} launches action_released_delta == 0, ` +
    `${noTrimLaunches.length}/${noTrimLaunches.length} launches burst2_reserved_delta == 0 -- CONFIRMED`,
);
console.log('\nTRIM vs NO_TRIM burst2 elapsed_ns (A - B, positive = TRIM slower):');
console.log(
  `  ${abRow.run}: mean_delta=${(abRow.mean_delta / 1e6).toFixed(3)} ms  t=${abRow.t.toFixed(3)}  ` +
    `crit=${abRow.crit}  sign NO_TRIM-faster=${abRow.sign_b_faster}/${abRow.n}  => ${abRow.significant ? 'REAL' : 'NOISE'}`,
);
console.log('\nSame-vs-same control (harness sanity):');
console.log(
  `  ${controlRow.run}: mean_delta=${(controlRow.mean_delta / 1e3).toFixed(3)} us  t=${controlRow.t.toFixed(3)}  ` +
    `crit=${controlRow.crit}  => ${controlRow.significant ? 'REAL (BUG!)' : 'NOISE, as expected'}`,
);
console.log('\nDerived cost figures (arithmetic mean across TRIM/NO_TRIM launches):');
console.log(`  trim_call_ns (TRIM only):        ${(trimCallNsMean / 1e6).toFixed(3)} ms`);
console.log(`  burst2 elapsed_ns (TRIM):         ${(trimBurst2Mean / 1e6).toFixed(3)} ms`);
console.log(`  burst2 elapsed_ns (NO_TRIM):      ${(noTrimBurst2Mean / 1e6).toFixed(3)} ms`);
console.log(`  cold-start delta (TRIM-NO_TRIM):  ${(coldStartDeltaNs / 1e6).toFixed(3)} ms`);
console.log(`  cold-start ratio (TRIM/NO_TRIM):  ${coldStartRatio.toFixed(2)}x`);

// ── Write summary CSV (companion to R31_10_TRIM_CURRENT_THREAD_RSS_GATE_summary.csv) ──
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
    [baseCommit, sourceTreeSha, landingCommit, artifact, metric, arm, nLabel, value, unit, `"${String(notes).replace(/"/g, "'")}"`].join(','),
  );
}

for (const row of [abRow, controlRow]) {
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
pushRow('cost_side', 'trim_call_ns_arithmetic_mean', 'TRIM', `n=${trimLaunches.length}`, trimCallNsMean.toFixed(1), 'ns', 'wall-clock of the trim_current_thread() call itself');
pushRow('cost_side', 'burst2_elapsed_ns_arithmetic_mean', 'TRIM', `n=${trimLaunches.length}`, trimBurst2Mean.toFixed(1), 'ns', 'second-burst wall time after trim destroyed the warm cache (cold re-materialisation)');
pushRow('cost_side', 'burst2_elapsed_ns_arithmetic_mean', 'NO_TRIM', `n=${noTrimLaunches.length}`, noTrimBurst2Mean.toFixed(1), 'ns', 'second-burst wall time with the cache still warm (no-trim control)');
pushRow('cost_side', 'cold_start_delta_ns', 'TRIM_minus_NO_TRIM', `n=${abRow.n}`, coldStartDeltaNs.toFixed(1), 'ns', 'burst2 cold-start cost trim specifically causes, isolated by the paired control');
pushRow('cost_side', 'cold_start_ratio', 'TRIM_over_NO_TRIM', `n=${abRow.n}`, coldStartRatio.toFixed(3), 'ratio', 'asserted > 1.0 in-script');
pushRow('cost_side', 'mechanism_oracle_action_released_delta_gt0_launches', 'TRIM', `n=${trimLaunches.length}`, trimLaunches.length, 'count', `${trimLaunches.length}/${trimLaunches.length} launches confirmed`);
pushRow('cost_side', 'mechanism_oracle_burst2_reserved_delta_gt0_launches', 'TRIM', `n=${trimLaunches.length}`, trimLaunches.length, 'count', `${trimLaunches.length}/${trimLaunches.length} launches confirmed`);

const outPath = 'docs/perf/R31_10_TRIM_CURRENT_THREAD_COST_GATE_summary.csv';
writeFileSync(new URL(outPath, ROOT), csvLines.join('\n') + '\n');
console.log(`\nwrote ${csvLines.length - 1} data rows to ${outPath} (base_commit=${baseCommit}, immutable_tree_sha=${sourceTreeSha}, landing_commit=${landingCommit})`);
