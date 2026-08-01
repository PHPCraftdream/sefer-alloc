// R32-0 (task #490) — derives `docs/perf/R32_0_VIRGIN_ZERO_SKIP_COST_SIDE_GATE_summary.csv`
// from:
//   (a) the deterministic bench gate's raw text logs
//       (`docs/perf/_raw_r32_0_{off,on}.log` + `_run2` repeats), and
//   (b) the wall-clock paired-AB provenance JSON `scripts/paired-ab-runner.mjs`
//       wrote for the off-vs-on comparison and the off-vs-off same-vs-same
//       control.
//
// ONE checked script, per CLAUDE.md's R30-9 rule: raw per-sample data written
// first, tables/headline numbers computed and ASSERTED here, never
// hand-transcribed into the report's prose. Pattern follows
// `scripts/r31_8_derive_report_data.mjs` / `scripts/r31_0_summary.mjs`.
//
// Usage:
//   node scripts/r32_0_derive_report_data.mjs [landing_commit_sha]

import { readFileSync, writeFileSync } from 'node:fs';

const ROOT = new URL('../', import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), 'utf8');
const readJson = (p) => JSON.parse(read(p));

const landingCommit = process.argv[2] || 'UNFILLED';

// Immutable source identity (CLAUDE.md's R29-6 rule): base commit + a git
// tree-object SHA computed via `git write-tree` against a SCOPED temporary
// index (base commit + every file this task added/changed staged into a
// throwaway index file at /tmp/r32_0_scoped_index, never touching the shared
// repo index/working tree of any concurrently-running agent). Computed BEFORE
// this derive script ran (recorded here, not re-derived).
const baseCommit = '4ccd18b02d2558a173b8d4bb1d55b4915a835702';
const sourceTreeSha = '2f2876de521a1104bdfcbdaf61ee279567b673f7';

// ── Parse the deterministic bench-gate CSV rows out of the raw text logs ───
function parseGateLog(path) {
  const text = read(path);
  const rows = { alloc_virgin: [], alloc_recycled: [], realloc: [] };
  for (const line of text.split(/\r?\n/)) {
    if (line.startsWith('R32_0_ALLOC_virgin,')) {
      const [, size, reps, refill_n, mean_hits, min_hits, expected_hits, mean_ns, p50_ns, min_ns, max_ns, mask_oracle] =
        line.split(',');
      rows.alloc_virgin.push({
        size,
        reps: Number(reps),
        refill_n: Number(refill_n),
        mean_hits: Number(mean_hits),
        min_hits: Number(min_hits),
        expected_hits: Number(expected_hits),
        mean_ns: Number(mean_ns),
        p50_ns: Number(p50_ns),
        min_ns: Number(min_ns),
        max_ns: Number(max_ns),
        mask_oracle,
      });
    } else if (line.startsWith('R32_0_ALLOC_recycled,')) {
      const [, size, reps, refill_n, mean_hits, min_hits, expected_hits, mean_ns, p50_ns, min_ns, max_ns, mask_oracle] =
        line.split(',');
      rows.alloc_recycled.push({
        size,
        reps: Number(reps),
        refill_n: Number(refill_n),
        mean_hits: Number(mean_hits),
        min_hits: Number(min_hits),
        expected_hits: Number(expected_hits),
        mean_ns: Number(mean_ns),
        p50_ns: Number(p50_ns),
        min_ns: Number(min_ns),
        max_ns: Number(max_ns),
        mask_oracle,
      });
    } else if (line.startsWith('R32_0_REALLOC,')) {
      const [, label, reps, mean_ns, p50_ns] = line.split(',');
      rows.realloc.push({ label, reps: Number(reps), mean_ns: Number(mean_ns), p50_ns: Number(p50_ns) });
    }
  }
  return rows;
}

const off1 = parseGateLog('docs/perf/_raw_r32_0_off.log');
const on1 = parseGateLog('docs/perf/_raw_r32_0_on.log');
const off2 = parseGateLog('docs/perf/_raw_r32_0_off_run2.log');
const on2 = parseGateLog('docs/perf/_raw_r32_0_on_run2.log');

// ── Headline assertion 1: path-activation oracle — every ON-binary virgin
//    cell's mask_oracle must be PASS (never FAIL/NA) on both runs; every
//    OFF-binary cell must be NA (the field does not exist there). ─────────
for (const [label, data, expectOn] of [
  ['run1 off', off1, false],
  ['run1 on', on1, true],
  ['run2 off', off2, false],
  ['run2 on', on2, true],
]) {
  for (const row of data.alloc_virgin) {
    const expected = expectOn ? 'PASS' : 'NA';
    if (row.mask_oracle !== expected) {
      throw new Error(
        `HEADLINE ASSERTION FAILED: ${label} alloc_virgin/${row.size} expected mask_oracle=${expected}, got ${row.mask_oracle}`,
      );
    }
  }
}

// ── Headline assertion 2: magazine-hit parity — mean_hits must equal
//    expected_hits (rounded) on EVERY cell, on BOTH binaries, on BOTH runs —
//    the cross-binary proof the production magazine path ran identically
//    regardless of the feature flag (same discipline as R31-0 §2.2). ──────
for (const [label, data] of [
  ['run1 off', off1],
  ['run1 on', on1],
  ['run2 off', off2],
  ['run2 on', on2],
]) {
  for (const arm of ['alloc_virgin', 'alloc_recycled']) {
    for (const row of data[arm]) {
      if (Math.round(row.mean_hits) !== row.expected_hits) {
        throw new Error(
          `HEADLINE ASSERTION FAILED: ${label} ${arm}/${row.size} expected mean_hits ~= expected_hits ` +
            `(${row.expected_hits}), got ${row.mean_hits}`,
        );
      }
    }
  }
}

// ── Wall-clock paired-AB: off-vs-on (real signal) + off-vs-off (control) ──
const RUNS = {
  off_vs_on: 'docs/perf/paired_ab_runs/2026-08-01T23-10-48-002Z.json',
  off_vs_off_control: 'docs/perf/paired_ab_runs/2026-08-01T23-11-07-888Z.json',
};
const abData = {};
for (const [key, path] of Object.entries(RUNS)) abData[key] = readJson(path);

function ttestRow(runKey, label) {
  const j = abData[runKey];
  const cmp = j.comparisons[0];
  return {
    run: label,
    metric: j.metric,
    arm_a: cmp.arm_a,
    arm_b: cmp.arm_b,
    n: cmp.paired_t_test.n,
    mean_delta: cmp.paired_t_test.mean,
    sd: cmp.paired_t_test.sd,
    t: cmp.paired_t_test.t,
    df: cmp.paired_t_test.df,
    crit: cmp.paired_t_test.crit,
    significant: cmp.paired_t_test.significant,
    sign_a_faster: cmp.sign_test.aFaster,
    sign_b_faster: cmp.sign_test.bFaster,
    sign_ties: cmp.sign_test.ties,
  };
}

const offVsOn = ttestRow('off_vs_on', 'wallclock_alloc_recycled_4k_off_vs_on');
const offVsOffControl = ttestRow('off_vs_off_control', 'wallclock_alloc_recycled_4k_off_vs_off_control');

// Headline assertion 3: the off-vs-on signal is REAL (t past crit) and its
// sign says ON is slower (mean_delta = off - on < 0 means off faster i.e. on
// slower) — matching the structural prediction (ON pays extra bookkeeping,
// gets zero benefit on this arm).
if (!offVsOn.significant) {
  throw new Error(`HEADLINE ASSERTION FAILED: off_vs_on expected a REAL (significant) signal, got t=${offVsOn.t}`);
}
if (offVsOn.mean_delta >= 0) {
  throw new Error(`HEADLINE ASSERTION FAILED: off_vs_on expected mean_delta (off - on) < 0 (on SLOWER), got ${offVsOn.mean_delta}`);
}
const SIGN_TEST_MAJORITY_FLOOR = 17; // this project's own "heavily lopsided" bar (see scripts/r31_8_derive_report_data.mjs)
if (offVsOn.sign_a_faster < SIGN_TEST_MAJORITY_FLOOR) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: off_vs_on expected a heavily-lopsided sign test (off-faster >= ${SIGN_TEST_MAJORITY_FLOOR}/${offVsOn.n}), got off-faster=${offVsOn.sign_a_faster}/${offVsOn.n}`,
  );
}

// Headline assertion 4: the same-vs-same control is NOISE (not significant)
// — the harness-sanity check.
if (offVsOffControl.significant) {
  throw new Error(`HEADLINE ASSERTION FAILED: off_vs_off_control expected NOISE (not significant), got t=${offVsOffControl.t} (significant)`);
}

// ── Headline assertion 5: relative magnitude — the ON-side extra cost on the
//    worst-case wall-clock cell must be small relative to R31-0's already-
//    published notouch win magnitudes (-89% to -98.6%). Compute the relative
//    cost as |mean_delta| / off-arm mean ns_per_round. ────────────────────
const offArmMeanNs = abData.off_vs_on.comparisons[0].samples_a_ns.reduce((a, b) => a + b, 0) / abData.off_vs_on.comparisons[0].samples_a_ns.length;
const relativeCostPct = (Math.abs(offVsOn.mean_delta) / offArmMeanNs) * 100;
if (!(relativeCostPct > 0 && relativeCostPct < 50)) {
  throw new Error(
    `HEADLINE ASSERTION FAILED: expected the worst-case wall-clock relative cost to be a small double-digit ` +
      `percentage (0-50%), got ${relativeCostPct.toFixed(1)}% -- re-check the arithmetic before publishing`,
  );
}

// ── Per-cell OFF/ON deltas for the deterministic gate's virgin/recycled
//    mean_ns, both runs — the §5.1/§5.2 report tables come from HERE, not
//    hand-computed (CLAUDE.md's R30-9 rule). ────────────────────────────────
function pctDelta(offNs, onNs) {
  return ((onNs - offNs) / offNs) * 100;
}
function deltaTable(scenario) {
  const rows = [];
  for (let i = 0; i < off1[scenario].length; i++) {
    const size = off1[scenario][i].size;
    const o1 = off1[scenario][i].mean_ns;
    const n1 = on1[scenario][i].mean_ns;
    const o2 = off2[scenario][i].mean_ns;
    const n2 = on2[scenario][i].mean_ns;
    rows.push({
      size,
      off_run1: o1,
      on_run1: n1,
      delta_run1_pct: pctDelta(o1, n1),
      off_run2: o2,
      on_run2: n2,
      delta_run2_pct: pctDelta(o2, n2),
    });
  }
  return rows;
}
const virginDeltaTable = deltaTable('alloc_virgin');
const recycledDeltaTable = deltaTable('alloc_recycled');

console.log('=== Headline assertions (checked in-script per CLAUDE.md rule 6) ===\n');
console.log('Deterministic-gate delta table (virgin scenario, OFF->ON, run1/run2):');
for (const r of virginDeltaTable) {
  console.log(`  ${r.size}: run1 ${r.delta_run1_pct.toFixed(1)}%  run2 ${r.delta_run2_pct.toFixed(1)}%`);
}
console.log('Deterministic-gate delta table (recycled scenario, OFF->ON, run1/run2):');
for (const r of recycledDeltaTable) {
  console.log(`  ${r.size}: run1 ${r.delta_run1_pct.toFixed(1)}%  run2 ${r.delta_run2_pct.toFixed(1)}%`);
}

console.log(`Path-activation oracle: ON-binary virgin cells 8/8 PASS (2 runs x 4 sizes); OFF-binary 8/8 NA (field absent) -- confirmed`);
console.log(`Magazine-hit parity: mean_hits == expected_hits on every cell, both binaries, both runs -- confirmed`);
console.log(`\nWall-clock (4k, recycled/steady-state-hit, worst-case arm):`);
console.log(`  off_vs_on: mean_delta=${offVsOn.mean_delta.toFixed(2)} ns/round  t=${offVsOn.t.toFixed(3)}  crit=${offVsOn.crit}  sign off-faster=${offVsOn.sign_a_faster}/${offVsOn.n}  => REAL, ON SLOWER`);
console.log(`  off_vs_off (control): mean_delta=${offVsOffControl.mean_delta.toFixed(2)} ns/round  t=${offVsOffControl.t.toFixed(3)}  crit=${offVsOffControl.crit}  => NOISE, as expected`);
console.log(`  relative cost: ${relativeCostPct.toFixed(1)}% of the OFF-arm's own mean ns/round`);

// ── Write summary CSV ──────────────────────────────────────────────────────
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
  'size_or_label',
  'reps_or_n',
  'value',
  'unit',
  'notes',
].join(',');

const csvLines = [header];
function pushRow(artifact, metric, arm, sizeLabel, repsOrN, value, unit, notes) {
  csvLines.push(
    [baseCommit, sourceTreeSha, landingCommit, artifact, metric, arm, sizeLabel, repsOrN, value, unit, `"${String(notes).replace(/"/g, "'")}"`].join(','),
  );
}

// Deterministic gate rows (primary run-1 only in the CSV; run-2 is cited in
// prose for noise-stability, not double-counted here — same convention as
// R31-0's own summary CSV).
for (const [arm, data] of [['off', off1], ['on', on1]]) {
  for (const row of data.alloc_virgin) {
    pushRow(
      'r32_0_bench_gate',
      'ns_per_op_arithmetic_mean',
      `alloc_virgin_${arm}`,
      row.size,
      row.reps,
      row.mean_ns.toFixed(1),
      'ns',
      `p50=${row.p50_ns} min=${row.min_ns} max=${row.max_ns} refill_n=${row.refill_n} mean_hits=${row.mean_hits} expected_hits=${row.expected_hits} mask_oracle=${row.mask_oracle}`,
    );
  }
  for (const row of data.alloc_recycled) {
    pushRow(
      'r32_0_bench_gate',
      'ns_per_op_arithmetic_mean',
      `alloc_recycled_${arm}`,
      row.size,
      row.reps,
      row.mean_ns.toFixed(1),
      'ns',
      `p50=${row.p50_ns} min=${row.min_ns} max=${row.max_ns} refill_n=${row.refill_n} mean_hits=${row.mean_hits} expected_hits=${row.expected_hits} mask_oracle=${row.mask_oracle}`,
    );
  }
  for (const row of data.realloc) {
    pushRow('r32_0_bench_gate', 'ns_per_op_arithmetic_mean', `realloc_${arm}`, row.label, row.reps, row.mean_ns.toFixed(1), 'ns', `p50=${row.p50_ns} (confirmatory cell, not a full sweep)`);
  }
}

// Wall-clock paired-AB rows.
pushRow(
  'r32_0_wallclock_paired_ab',
  'mean_delta_ns_arithmetic_mean',
  'off_minus_on',
  '4k_recycled',
  offVsOn.n,
  offVsOn.mean_delta.toFixed(3),
  'ns',
  `t=${offVsOn.t.toFixed(3)} df=${offVsOn.df} crit=${offVsOn.crit} sign off-faster=${offVsOn.sign_a_faster}/${offVsOn.n} on-faster=${offVsOn.sign_b_faster}/${offVsOn.n} significant=${offVsOn.significant}`,
);
pushRow(
  'r32_0_wallclock_paired_ab',
  'mean_delta_ns_arithmetic_mean',
  'off(A-slot)_minus_off(B-slot)_control',
  '4k_recycled',
  offVsOffControl.n,
  offVsOffControl.mean_delta.toFixed(3),
  'ns',
  `t=${offVsOffControl.t.toFixed(3)} df=${offVsOffControl.df} crit=${offVsOffControl.crit} significant=${offVsOffControl.significant}`,
);
pushRow('r32_0_wallclock_paired_ab', 'relative_cost_pct', 'on_extra_over_off_mean', '4k_recycled', offVsOn.n, relativeCostPct.toFixed(2), 'pct', 'abs(mean_delta) / off-arm mean ns_per_round, asserted in-script to be in (0,50)%');

// Deterministic-gate delta rows — the machine-readable source for §5.1/§5.2's
// report tables (CLAUDE.md's R30-9 rule: prose tables must be derived, not
// hand-computed).
for (const [scenario, rows] of [['virgin', virginDeltaTable], ['recycled', recycledDeltaTable]]) {
  for (const r of rows) {
    pushRow(
      'r32_0_bench_gate_delta',
      'pct_delta_off_to_on',
      `alloc_${scenario}_run1`,
      r.size,
      '',
      r.delta_run1_pct.toFixed(1),
      'pct',
      `off_ns=${r.off_run1} on_ns=${r.on_run1}`,
    );
    pushRow(
      'r32_0_bench_gate_delta',
      'pct_delta_off_to_on',
      `alloc_${scenario}_run2`,
      r.size,
      '',
      r.delta_run2_pct.toFixed(1),
      'pct',
      `off_ns=${r.off_run2} on_ns=${r.on_run2}`,
    );
  }
}

const outPath = 'docs/perf/R32_0_VIRGIN_ZERO_SKIP_COST_SIDE_GATE_summary.csv';
writeFileSync(new URL(outPath, ROOT), csvLines.join('\n') + '\n');
console.log(`\nwrote ${csvLines.length - 1} data rows to ${outPath} (base_commit=${baseCommit}, immutable_tree_sha=${sourceTreeSha}, landing_commit=${landingCommit})`);
