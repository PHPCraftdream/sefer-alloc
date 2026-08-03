// R32-12 (task #503, F8 sub-change (2)): derives
// `docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE_summary.csv` from the
// raw per-sample data this task's measurement produced: two
// `paired-ab-runner.mjs` provenance JSON files (native wall-clock A/B +
// same-vs-same control, free-slot-search isolation microjudge) plus the
// hand-extracted iai-callgrind `Instructions` counts from the truncated raw
// logs committed alongside this script (CLAUDE.md's R30-9 rule: raw
// per-sample data written first, tables/ratios computed and asserted by one
// checked script, never hand-transcribed).
//
// Also asserts this report's own headline claims in-script (CLAUDE.md rule
// 6): the wall-clock A/B at scan_bound=8 is NOT statistically significant
// (t under crit) — the survey's own honest prediction — while the same-
// vs-same control is ALSO not significant (harness sanity), and the
// Ir-level worst-case free-slot-search marginal cost (shared-prefix
// subtraction, R23-3 pattern) is negative (AFTER cheaper than BEFORE).
//
// Usage:
//   node scripts/r32_12_derive_report_data.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from 'node:fs';
import { execSync } from 'node:child_process';

const ROOT = new URL('../', import.meta.url);
const read = (p) => JSON.parse(readFileSync(new URL(p, ROOT), 'utf8'));

const landingCommit = process.argv[2] || execSync('git rev-parse HEAD', { encoding: 'utf8' }).trim();
// Immutable source identity (CLAUDE.md's R29-6 rule): a git tree-object SHA
// computed via `git write-tree` against a SCOPED temporary index (base
// commit + every file this task changed/added staged into a throwaway
// index, never touching the shared repo index/working tree of any
// concurrently-running agent) — same technique as
// `scripts/r31_8_derive_report_data.mjs`.
const baseCommit = 'e784dbc537752c4b4537a043130fc8da2b2573b1';
const sourceTreeSha = '705cb3487c556e1bd3897644c1bae9ac1f3b1bd2';

// ── Native wall-clock A/B (free-slot-search isolation microjudge) ─────────
const RUNS = {
  before_vs_after: 'docs/perf/paired_ab_runs/2026-08-02T21-17-17-448Z.json',
  same_vs_same_control: 'docs/perf/paired_ab_runs/2026-08-02T21-17-29-807Z.json',
};
const data = {};
for (const [key, path] of Object.entries(RUNS)) {
  data[key] = read(path);
}

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

const rowBeforeAfter = ttestRow('before_vs_after', 'wallclock_before_vs_after');
const rowSameVsSame = ttestRow('same_vs_same_control', 'wallclock_same_vs_same_control');

// Headline assertion 1: at scan_bound=8 (production's actual base cache
// size), the wall-clock A/B must be a NOISE-BAND null (survey's own honest
// prediction: the scan is already cheap at only 8 elements) — NOT claimed
// as a wall-clock win.
if (rowBeforeAfter.significant) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: expected the scan_bound=8 wall-clock A/B to be a noise-band NULL ` +
      `(t=${rowBeforeAfter.t} vs crit=${rowBeforeAfter.crit}), got SIGNIFICANT -- re-check the report's framing`,
  );
}
// Headline assertion 2: the same-vs-same control must ALSO be noise (harness sanity).
if (rowSameVsSame.significant) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: same-vs-same control expected NOISE (t=${rowSameVsSame.t} vs crit=${rowSameVsSame.crit}), got SIGNIFICANT -- harness bug`,
  );
}

// ── Ir-level decomposition (iai-callgrind, WSL/Valgrind) ───────────────────
// Hand-extracted ONCE from the two truncated raw logs committed alongside
// this script (`docs/perf/_raw_r32_12_before_killgate.log`,
// `docs/perf/_raw_r32_12_after_killgate.log`) -- the raw logs are the
// citable evidence; this script re-derives every ratio/delta from these
// same absolute numbers so the report's tables cannot silently disagree
// with what is actually in the logs.
const IR = {
  // 5 standing small-object kill-gate benches (must stay within +/-10 Ir --
  // this change is scoped to Large-object cache management).
  killgate: {
    small_churn_16b: { before: 9038, after: 9039 },
    aligned_churn_640b_a128: { before: 8974, after: 8975 },
    churn_256b: { before: 9038, after: 9039 },
    cold_alloc_free_256x16b: { before: 51548, after: 51549 },
    recycle_alloc_free_256x16b: { before: 100764, after: 100758 },
  },
  // Free-slot-search worst-case isolation pair (this task's own new bench,
  // `benches/perf_gate_iai.rs::large_cache_free_slot_search_{prefill,cycle}_only`).
  free_slot_search: {
    prefill_only: { before: 7924, after: 7924 },
    cycle_only: { before: 11045, after: 11005 },
  },
  cycles: 8, // FREE_SLOT_SEARCH_CYCLES
};

const KILLGATE_THRESHOLD = 10;
const killgateRows = [];
for (const [name, { before, after }] of Object.entries(IR.killgate)) {
  const delta = after - before;
  if (Math.abs(delta) > KILLGATE_THRESHOLD) {
    throw new Error(
      `HEADLINE ASSERTION FAILED: kill-gate bench ${name} delta ${delta} exceeds +/-${KILLGATE_THRESHOLD} Ir -- this change must stay flat on the small-object hot path`,
    );
  }
  killgateRows.push({ bench: name, before, after, delta });
}

const prefillDelta = IR.free_slot_search.prefill_only.after - IR.free_slot_search.prefill_only.before;
if (prefillDelta !== 0) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: expected the free-slot-search prefill (shared-prefix, decoy-population-only) arm to be BYTE-IDENTICAL between before/after (delta=0) -- got delta=${prefillDelta}. ` +
      `The bitmask change should not alter the decoy-deposit path's instruction count.`,
  );
}

const beforeMarginal = IR.free_slot_search.cycle_only.before - IR.free_slot_search.prefill_only.before;
const afterMarginal = IR.free_slot_search.cycle_only.after - IR.free_slot_search.prefill_only.after;
const marginalDelta = afterMarginal - beforeMarginal;
const perRoundDelta = marginalDelta / IR.cycles;

// Headline assertion 3: the isolated (shared-prefix-subtracted) worst-case
// free-slot-search marginal cost must be <= 0 (AFTER not more expensive than
// BEFORE) -- the whole point of replacing an O(N) scan with an O(1) bitmask
// lookup.
if (marginalDelta > 0) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: expected the isolated free-slot-search marginal Ir delta to be <= 0 (bitmask not slower than the linear scan), got +${marginalDelta} over ${IR.cycles} rounds`,
  );
}

console.log('=== Headline assertions (checked in-script per CLAUDE.md rule 6) ===\n');
console.log('Native wall-clock A/B (free-slot-search isolation, scan_bound=8, ns_per_round):');
console.log(
  `  ${rowBeforeAfter.run}: mean_delta=${rowBeforeAfter.mean_delta.toFixed(2)} ns  t=${rowBeforeAfter.t.toFixed(3)}  crit=${rowBeforeAfter.crit}  ` +
    `sign before-faster=${rowBeforeAfter.sign_a_faster}/${rowBeforeAfter.n} after-faster=${rowBeforeAfter.sign_b_faster}/${rowBeforeAfter.n}  => ${rowBeforeAfter.significant ? 'REAL (unexpected!)' : 'NOISE, as predicted by the survey at scan_bound=8'}`,
);
console.log(
  `  ${rowSameVsSame.run}: mean_delta=${rowSameVsSame.mean_delta.toFixed(2)} ns  t=${rowSameVsSame.t.toFixed(3)}  crit=${rowSameVsSame.crit}  => ${rowSameVsSame.significant ? 'REAL (BUG!)' : 'NOISE, as expected'}`,
);

console.log('\nStanding kill-gate (5 small-object benches, raw Instructions, before -> after):');
for (const row of killgateRows) {
  console.log(`  ${row.bench}: ${row.before} -> ${row.after}  delta=${row.delta >= 0 ? '+' : ''}${row.delta}  (within +/-${KILLGATE_THRESHOLD}: ${Math.abs(row.delta) <= KILLGATE_THRESHOLD})`);
}

console.log('\nIr-level free-slot-search worst-case decomposition (shared-prefix subtraction, R23-3 pattern):');
console.log(`  prefill (shared prefix): before=${IR.free_slot_search.prefill_only.before} after=${IR.free_slot_search.prefill_only.after} delta=${prefillDelta} (byte-identical, confirmed)`);
console.log(`  cycle (prefix + ${IR.cycles} admission rounds): before=${IR.free_slot_search.cycle_only.before} after=${IR.free_slot_search.cycle_only.after}`);
console.log(`  isolated marginal (${IR.cycles} rounds): before=${beforeMarginal} after=${afterMarginal} delta=${marginalDelta}`);
console.log(`  isolated marginal per-round: before=${(beforeMarginal / IR.cycles).toFixed(3)} after=${(afterMarginal / IR.cycles).toFixed(3)} delta=${perRoundDelta.toFixed(3)} Ir/round`);

// ── Write summary CSV ───────────────────────────────────────────────────────
const META = {
  cpu: 'Intel_Core_i7-11800H_2.30GHz',
  os: 'Windows_10_Pro_10.0.19045 (WSL2 Ubuntu 24.04 for the Ir/Valgrind axis)',
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

for (const row of [rowBeforeAfter, rowSameVsSame]) {
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
for (const row of killgateRows) {
  pushRow('killgate', 'instructions_raw', 'before_to_after', row.bench, row.delta, 'Ir_delta', `before=${row.before} after=${row.after}`);
}
pushRow('free_slot_search_ir', 'instructions_raw', 'prefill_before', 'shared_prefix', IR.free_slot_search.prefill_only.before, 'Ir', 'decoy-population-only arm, iai-callgrind');
pushRow('free_slot_search_ir', 'instructions_raw', 'prefill_after', 'shared_prefix', IR.free_slot_search.prefill_only.after, 'Ir', 'decoy-population-only arm, iai-callgrind');
pushRow('free_slot_search_ir', 'instructions_raw', 'cycle_before', `n_rounds=${IR.cycles}`, IR.free_slot_search.cycle_only.before, 'Ir', 'prefix + N admission rounds, iai-callgrind');
pushRow('free_slot_search_ir', 'instructions_raw', 'cycle_after', `n_rounds=${IR.cycles}`, IR.free_slot_search.cycle_only.after, 'Ir', 'prefix + N admission rounds, iai-callgrind');
pushRow('free_slot_search_ir', 'isolated_marginal_ir_total', 'before', `n_rounds=${IR.cycles}`, beforeMarginal, 'Ir', 'cycle_before - prefill_before (shared-prefix subtraction)');
pushRow('free_slot_search_ir', 'isolated_marginal_ir_total', 'after', `n_rounds=${IR.cycles}`, afterMarginal, 'Ir', 'cycle_after - prefill_after (shared-prefix subtraction)');
pushRow('free_slot_search_ir', 'isolated_marginal_ir_per_round', 'delta_after_minus_before', `n_rounds=${IR.cycles}`, perRoundDelta.toFixed(3), 'Ir/round', `total delta=${marginalDelta} over ${IR.cycles} rounds, asserted <= 0 in-script`);

const outPath = 'docs/perf/R32_12_LARGE_CACHE_OCCUPANCY_BITMASK_GATE_summary.csv';
writeFileSync(new URL(outPath, ROOT), csvLines.join('\n') + '\n');
console.log(`\nwrote ${csvLines.length - 1} data rows to ${outPath} (base_commit=${baseCommit}, immutable_tree_sha=${sourceTreeSha}, landing_commit=${landingCommit})`);
console.log(`meta: cpu=${META.cpu} os=${META.os} rustc=${META.rustc}`);
