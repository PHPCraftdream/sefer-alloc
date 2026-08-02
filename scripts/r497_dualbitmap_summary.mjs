// R32-6 (task #497) — derives the summary CSV from the two raw `npm run
// iai`-style logs already committed under `docs/perf/_raw_r497_dualbitmap_*`.
// Per CLAUDE.md's checked-script rule: this script COMPUTES every number the
// report's table cites (never hand-transcribed), and asserts the arithmetic
// before printing/writing anything.
//
// Usage: node scripts/r497_dualbitmap_summary.mjs [landing_commit_sha]
//
// `landing_commit_sha` is optional — this task did NOT land any src/ change
// (REJECT verdict, see the report), so there is no landing commit for the
// diff itself; if supplied it is recorded as the commit that carries this
// summary/report pair.

import { readFileSync, writeFileSync } from 'node:fs';
import { REPO_ROOT } from './lib.mjs';
import path from 'node:path';

const BEFORE_LOG = path.join(REPO_ROOT, 'docs/perf/_raw_r497_dualbitmap_before_production.log');
const AFTER_LOG = path.join(REPO_ROOT, 'docs/perf/_raw_r497_dualbitmap_after_production.log');
const OUT_CSV = path.join(REPO_ROOT, 'docs/perf/R32_6_DUAL_BITMAP_GATE_summary.csv');

const BENCHES = [
  'small_churn_16b',
  'churn_256b',
  'aligned_churn_640b_a128',
  'cold_alloc_free_256x16b',
  'recycle_alloc_free_256x16b',
  'large_alloc_free_cycle',
];

// Op counts per bench, for the ±10 raw-Ir churn kill gate (small_churn_16b /
// churn_256b / aligned_churn_640b_a128) and per-op deltas on the larger
// cold/recycle arms (256 ops each). `large_alloc_free_cycle` is the
// bootstrap-cost proxy (1 alloc+free of a Large segment; never touches the
// small-class bitmaps) — reported as raw Ir only, no per-op derivation.
const OP_COUNTS = {
  small_churn_16b: 1,
  churn_256b: 1,
  aligned_churn_640b_a128: 1,
  cold_alloc_free_256x16b: 256,
  recycle_alloc_free_256x16b: 256,
  large_alloc_free_cycle: null,
};

function parseIr(logText, bench) {
  // Matches the per-bench raw block: "perf_gate_iai::perf_gate::<bench>"
  // followed by "  Instructions:  <n>|..." on the next non-blank line.
  const re = new RegExp(
    String.raw`perf_gate_iai::perf_gate::${bench}\r?\n\s+Instructions:\s+(\d+)\|`,
  );
  const m = logText.match(re);
  if (!m) {
    throw new Error(`could not find raw Ir for bench "${bench}" in log`);
  }
  return Number(m[1]);
}

const beforeText = readFileSync(BEFORE_LOG, 'utf8');
const afterText = readFileSync(AFTER_LOG, 'utf8');

const rows = [];
for (const bench of BENCHES) {
  const before = parseIr(beforeText, bench);
  const after = parseIr(afterText, bench);
  const delta = after - before;
  const ops = OP_COUNTS[bench];
  const deltaPerOp = ops ? delta / ops : null;
  rows.push({ bench, before, after, delta, ops, deltaPerOp });
}

// ---- Assert the arithmetic before printing/writing anything ----

// 1. Every delta must equal after - before exactly (sanity on the parse).
for (const r of rows) {
  if (r.after - r.before !== r.delta) {
    throw new Error(`arithmetic mismatch for ${r.bench}`);
  }
}

// 2. The bootstrap-cost proxy (large_alloc_free_cycle) must be EXACTLY 0
//    delta — this is the load-bearing check that rules out a bootstrap/
//    codegen-shift explanation (the R32-5 pattern) for the regression seen
//    on every small-class-bitmap-touching bench below.
const bootstrap = rows.find((r) => r.bench === 'large_alloc_free_cycle');
if (bootstrap.delta !== 0) {
  throw new Error(
    `expected large_alloc_free_cycle (bootstrap proxy) delta == 0, got ${bootstrap.delta} — ` +
      `the "genuine per-op regression, not a bootstrap shift" claim does not hold; do not ship this report as-is`,
  );
}

// 3. Every small-class-bitmap-touching bench must have regressed (delta > 0)
//    — the report's REJECT verdict rests on this being a consistent
//    regression, not a mixed/noisy result.
for (const r of rows) {
  if (r.bench === 'large_alloc_free_cycle') continue;
  if (!(r.delta > 0)) {
    throw new Error(
      `expected ${r.bench} to regress (delta > 0), got ${r.delta} — the "every bitmap-touching bench regressed" claim does not hold`,
    );
  }
}

// 4. small_churn_16b and churn_256b must move by the EXACT SAME delta
//    (structurally identical churn shapes, same class) — a sanity check
//    that the two logs were captured under matched conditions.
const sc = rows.find((r) => r.bench === 'small_churn_16b');
const c256 = rows.find((r) => r.bench === 'churn_256b');
if (sc.delta !== c256.delta) {
  throw new Error(
    `expected small_churn_16b and churn_256b to move by the same delta (structurally identical churn shapes), got ${sc.delta} vs ${c256.delta}`,
  );
}

// ---- Write the CSV ----

const header = 'bench,before_ir,after_ir,delta_ir,ops,delta_ir_per_op\n';
const lines = rows.map((r) => {
  const perOp = r.deltaPerOp === null ? '' : r.deltaPerOp.toFixed(3);
  return `${r.bench},${r.before},${r.after},${r.delta},${r.ops ?? ''},${perOp}`;
});
const csv = header + lines.join('\n') + '\n';
writeFileSync(OUT_CSV, csv, 'utf8');

console.log(`Wrote ${OUT_CSV}`);
console.log(csv);
console.log(
  'Verdict basis: bootstrap-proxy delta == 0 (confirmed), every bitmap-touching ' +
    'bench regressed (confirmed), small_churn_16b == churn_256b delta (confirmed) — ' +
    'REJECT is supported by this data.',
);

const landingSha = process.argv[2];
if (landingSha) {
  if (!/^[0-9a-f]{40}$/.test(landingSha)) {
    throw new Error(`landing_commit_sha must be a full 40-hex SHA, got: ${landingSha}`);
  }
  console.log(`landing_commit: ${landingSha}`);
}
