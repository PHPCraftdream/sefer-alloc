// TIS backoff-spin-cap sweep (task tis-r8-Group1 #1758): the ONE checked
// derivation script for `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE.md` and its
// companion `docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv`
// (CLAUDE.md "tables derived by one checked script, not hand-transcribed"
// plus rule 6: a script that computes a headline claim must assert the
// arithmetic it prints).
//
// Inputs (committed raw artifacts):
//   docs/perf/_raw_tis_backoff_cap_sweep_run1.log          (cap sweep, rep=1)
//   docs/perf/_raw_tis_backoff_cap_sweep_run2_repeat16.log (16-thread repeat)
//   docs/perf/_raw_tis_backoff_per_call_latency.log        (per-call pop tails)
//   docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv       (cross-checked)
//
// It re-derives every table and headline claim of the gate report, asserts
// them in-script, and pins the round-8 review findings P2-1 / P2-2 / P3-2 /
// P3-3 (the two false claims round 8 caught fail loudly here).
//
// Round 9 (task tis-r9-Group1): CSV latency rows fixed to the correct
// 20-column width (6 empty throughput fields, not 9 — P2-1), --write made
// idempotent against its own output, verify-block offsets corrected, D4
// extended to all five threshold cells (P2-2), and new D5/D6 assertions pin
// the percentile columns and worst-pop ratio spreads (P3-2/P4-7).
//
// Usage:
//   node scripts/tis_backoff_cap_sweep_derive_report_data.mjs            # verify + print tables
//   node scripts/tis_backoff_cap_sweep_derive_report_data.mjs --write    # also regenerate the summary CSV

import { readFileSync, writeFileSync } from 'node:fs';

const WRITE = process.argv.includes('--write');

const ROOT = new URL('../', import.meta.url);
const read = (p) => readFileSync(new URL(p, ROOT), 'utf8');

const SWEEP1 = 'docs/perf/_raw_tis_backoff_cap_sweep_run1.log';
const SWEEP2 = 'docs/perf/_raw_tis_backoff_cap_sweep_run2_repeat16.log';
const LATENCY = 'docs/perf/_raw_tis_backoff_per_call_latency.log';
const CSV = 'docs/perf/TIS_BACKOFF_CAP_SWEEP_GATE_summary.csv';

let assertionCount = 0;
function pass(desc) {
  assertionCount++;
  console.log(`PASS: ${desc}`);
}
function fail(desc) {
  console.error(`ASSERTION FAILED: ${desc}`);
  process.exit(1);
}
function assert(cond, desc) {
  if (!cond) fail(desc);
  else pass(desc);
}
const near = (a, b, tol) => Math.abs(a - b) < tol;

// ---------------------------------------------------------------------------
// (1) parse the sweep logs
// ---------------------------------------------------------------------------
function parseSweepLog(text, run) {
  const lines = text.split(/\r?\n/);
  const records = [];
  let cap = null;
  let threads = null;
  let rep = 1;
  let bench = null;
  for (const line of lines) {
    let m = line.match(/^=== cap=(\d+) threads=(\d+)(?: rep=(\d+))? ===/);
    if (m) {
      cap = Number(m[1]);
      threads = Number(m[2]);
      rep = m[3] ? Number(m[3]) : 1;
      bench = null;
      continue;
    }
    m = line.match(/^contention\/(\w+): (\d+) ops\/sec total/);
    if (m) {
      bench = m[1];
      const totalOps = Number(m[2]);
      records.push({ run, cap, threads, rep, bench, totalOps, breakdown: [] });
      continue;
    }
    m = line.match(/^\s*Per-thread breakdown: \[([\d,\s]*)\]/);
    if (m && records.length > 0) {
      records[records.length - 1].breakdown = m[1].split(',').map((s) => Number(s.trim()));
    }
  }
  return records;
}

const sweep1Text = read(SWEEP1);
const sweep2Text = read(SWEEP2);
assert(sweep1Text.includes('base_sha: 47c81e9087d6bf353d537e15e362c5b65925c90e'), 'run-1 sweep log contains base_sha: 47c81e9087d6bf353d537e15e362c5b65925c90e');
assert(sweep2Text.includes('base_sha: 47c81e9087d6bf353d537e15e362c5b65925c90e'), 'run-2 sweep log contains base_sha: 47c81e9087d6bf353d537e15e362c5b65925c90e');

const run1Records = parseSweepLog(sweep1Text, 1);
const run2Records = parseSweepLog(sweep2Text, 2);
assert(run1Records.length === 40, `run-1 sweep log yields exactly 40 records (got ${run1Records.length})`);
assert(run2Records.length === 12, `run-2 sweep log yields exactly 12 records (got ${run2Records.length})`);

// ---------------------------------------------------------------------------
// (2) derived per-record quantities
// ---------------------------------------------------------------------------
function derive(rec) {
  const b = rec.breakdown;
  const max = Math.max(...b);
  const min = Math.min(...b);
  const mean = b.reduce((a, x) => a + x, 0) / b.length;
  rec.perThreadMax = max;
  rec.perThreadMin = min;
  rec.perThreadMean = mean;
  rec.maxOverMin = max / min;
  rec.minOverMean = min / mean;
}
for (const rec of [...run1Records, ...run2Records]) derive(rec);

const findRec = (run, cap, threads, rep, bench) =>
  [...run1Records, ...run2Records].find(
    (r) => r.run === run && r.cap === cap && r.threads === threads && r.rep === rep && r.bench === bench,
  );

// latency log parse (needed by the run=3 cross-check below)
const latText = read(LATENCY);
const latLines = latText.split(/\r?\n/);
const latRecords = latLines
  .filter((l) => l.startsWith('{'))
  .map((l) => JSON.parse(l));

// ---------------------------------------------------------------------------
// (3) cross-check against the committed CSV
// ---------------------------------------------------------------------------
const csvText = read(CSV);
const csvLines = csvText.split(/\r?\n/).filter((l) => l.trim() !== '');
const csvHeader = csvLines[0].split(',');
const csvRows = csvLines.slice(1).map((l) => l.split(','));
const round3 = (x) => Math.round(x * 1000) / 1000;
// sweep rows = rows whose bench is a throughput bench; only the first 11
// fields are read so a post-`--write` CSV (extra latency columns + appended
// run=3 rows) verifies identically to the committed pre-write shape.
const sweepRows = csvRows.filter((vals) => vals[4] !== 'pop_latency');
const latencyRows = csvRows.filter((vals) => vals[4] === 'pop_latency');
const mismatches = [];
for (const vals of sweepRows) {
  const [run, cap, threads, rep, bench] = vals.slice(0, 11);
  const rec = findRec(Number(run), Number(cap), Number(threads), Number(rep), bench);
  if (!rec) {
    mismatches.push(`no raw record for CSV row ${vals.join(',')}`);
    continue;
  }
  const csvMean = Number(vals[8]);
  const bad =
    rec.totalOps !== Number(vals[5]) ||
    rec.perThreadMax !== Number(vals[6]) ||
    rec.perThreadMin !== Number(vals[7]) ||
    !near(rec.perThreadMean, csvMean, 0.05) ||
    !near(round3(rec.maxOverMin), Number(vals[9]), 0.0005) ||
    !near(round3(rec.minOverMean), Number(vals[10]), 0.0005);
  if (bad) {
    mismatches.push(
      `row ${vals.join(',')}: raw total=${rec.totalOps} max=${rec.perThreadMax} min=${rec.perThreadMin} mean=${rec.perThreadMean} max/min=${rec.maxOverMin} min/mean=${rec.minOverMean}`,
    );
  }
}
if (mismatches.length > 0) {
  console.error('CSV disagrees with the raw logs — offending rows:');
  for (const m of mismatches) console.error('  ' + m);
  console.error('ASSERTION FAILED: committed CSV must be regenerated from the raw logs, not papered over');
  process.exit(1);
}
assert(sweepRows.length === 52, `summary CSV contains exactly 52 sweep (run-1/run-2) rows (got ${sweepRows.length})`);
pass(`all ${sweepRows.length} summary CSV sweep rows match the raw logs (totals, per-thread max/min, mean, ratios)`);

// ---------------------------------------------------------------------------
// (3b) cross-check run=3 latency rows against the per-call latency log
// ---------------------------------------------------------------------------
if (latencyRows.length === 0) {
  pass('summary CSV contains no run=3 pop_latency rows (pre-write shape) — latency-row cross-check skipped');
} else {
  assert(latencyRows.length === 18, `summary CSV contains exactly 18 run=3 pop_latency rows (got ${latencyRows.length})`);
  const fmtCount = (x) => String(x);
  for (const vals of latencyRows) {
    const [run, cap, threads, rep, bench] = vals.slice(0, 11);
    assert(run === '3' && bench === 'pop_latency', `latency CSV row ${vals.slice(0, 5).join(',')} is run=3 bench=pop_latency`);
    assert(
      vals.slice(5, 11).every((v) => v === ''),
      `latency CSV row ${vals.slice(0, 5).join(',')}: the 6 throughput fields (header cols 6-11) are empty`,
    );
    const rec = latRecords.find(
      (r) => r.cap_label === cap && r.threads === Number(threads) && r.rep === Number(rep),
    );
    assert(!!rec, `latency CSV row (3,${cap},${threads},${rep},pop_latency) has a matching JSON line in the latency log`);
    if (!rec) continue;
    assert(
      vals.slice(11, 20).length === 9,
      `latency CSV row (3,${cap},${threads},${rep}): has 9 latency fields`,
    );
    const want = [
      rec.pop_p50_ms.toFixed(3),
      rec.pop_p90_ms.toFixed(3),
      rec.pop_p99_ms.toFixed(3),
      rec.pop_p999_ms.toFixed(3),
      rec.pop_max_ms.toFixed(3),
      fmtCount(rec.pop_over_1ms),
      fmtCount(rec.pop_over_10ms),
      fmtCount(rec.pop_over_100ms),
      rec.wall_ms.toFixed(1),
    ];
    const got = vals.slice(11, 20);
    const ok = want.every((w, i) => got[i] === w);
    if (WRITE) {
      // --write is about to regenerate the latency rows from the raw log;
      // a stale pre-write CSV (old 23-field shape) must not block it. The
      // regenerated file is re-verified by a plain verify-mode run.
      pass(`latency CSV row (3,${cap},${threads},${rep}): stale pre-write row skipped (--write regenerates it)`);
    } else {
      assert(ok, `latency CSV row (3,${cap},${threads},${rep}): all 9 latency fields match the JSON log exactly (got [${got.join(',')}])`);
    }
  }
  // P2-1 pin (round 9): the gate report's "pop_p999_ms = 0.001 in the CSV
  // rows" sentence is asserted against the actual CSV column — the value is
  // only correct now that latency rows carry 6 empty throughput fields
  // instead of 9 (the old 23-field shape shifted every latency value three
  // columns right, hiding 0.001 under pop_p999_ms as 0.000).
  const p999Col = 11 + 3;
  // NOTE (round 9): the review's draft pin expected all six 8-thread rows to
  // carry 0.001, but the CSV's cap-0 rows correctly carry 0.057/0.054/0.056
  // (matching the raw log); only the cap-6 rows are 0.001. Pin follows the
  // data.
  const rows8c6 = latencyRows.filter((v) => v[2] === '8' && v[1] === '6');
  const rows8c0 = latencyRows.filter((v) => v[2] === '8' && v[1] === '0');
  assert(
    rows8c6.length === 3 && rows8c6.every((v) => v[p999Col] === '0.001') &&
      rows8c0.length === 3 &&
      rows8c0.every((v) => ['0.057', '0.054', '0.056'].includes(v[p999Col])),
    `P2-1 pin: all 6 CSV 8-thread pop_latency rows carry pop_p999_ms in the correctly-aligned column (cap 6 = 0.001, got [${rows8c6.map((v) => v[p999Col]).join(', ')}]; cap 0 = 0.054/0.056/0.057, got [${rows8c0.map((v) => v[p999Col]).join(', ')}])`,
  );
}

// ---------------------------------------------------------------------------
// (4) tables
// ---------------------------------------------------------------------------
const CAPS = [0, 4, 6, 8, 10];
const ARM_ORDER = [];
for (const t of [2, 4, 8, 16]) for (const b of ['push_pop', 'churn']) ARM_ORDER.push([t, b]);
const r1 = (cap, t, b) => findRec(1, cap, t, 1, b);
const fmtOps = (x) => x.toLocaleString('en-US');
const fmtPct = (x) => (x >= 0 ? '+' : '') + x.toFixed(1) + '%';
const delta = (capN, cap6) => (100 * (capN - cap6)) / cap6;

console.log('## Throughput (run 1, ops/sec)');
console.log('| threads | bench | cap 0 | cap 4 | cap 6 | cap 8 | d8 | cap 10 | d10 |');
console.log('|---|---|---|---|---|---|---|---|---|');
const deltaCells = [];
for (const [t, b] of ARM_ORDER) {
  const ops = CAPS.map((c) => r1(c, t, b).totalOps);
  const d8 = delta(ops[3], ops[2]);
  const d10 = delta(ops[4], ops[2]);
  deltaCells.push(d8, d10);
  console.log(`| ${t} | ${b} | ${fmtOps(ops[0])} | ${fmtOps(ops[1])} | ${fmtOps(ops[2])} | ${fmtOps(ops[3])} | ${fmtPct(d8)} | ${fmtOps(ops[4])} | ${fmtPct(d10)} |`);
}

console.log('\n## Fairness, min/mean (run 1)');
console.log('| threads | bench | cap 0 | cap 4 | cap 6 | cap 8 | cap 10 |');
console.log('|---|---|---|---|---|---|---|');
for (const [t, b] of ARM_ORDER) {
  const cells = CAPS.map((c) => r1(c, t, b).minOverMean.toFixed(3));
  console.log(`| ${t} | ${b} | ${cells.join(' | ')} |`);
}

console.log('\n## Fairness, max/min (run 1)');
console.log('| threads | bench | cap 0 | cap 4 | cap 6 | cap 8 | cap 10 |');
console.log('|---|---|---|---|---|---|---|');
for (const [t, b] of ARM_ORDER) {
  const cells = CAPS.map((c) => r1(c, t, b).maxOverMin.toFixed(3));
  console.log(`| ${t} | ${b} | ${cells.join(' | ')} |`);
}

console.log('\n## Run-2 repeat (16 threads)');
console.log('| cap | rep | bench | ops/sec | max/min | min/mean |');
console.log('|---|---|---|---|---|---|');
for (const cap of [6, 8, 10]) {
  for (const rep of [1, 2]) {
    for (const b of ['push_pop', 'churn']) {
      const rec = findRec(2, cap, 16, rep, b);
      console.log(`| ${cap} | ${rep} | ${b} | ${fmtOps(rec.totalOps)} | ${rec.maxOverMin.toFixed(3)} | ${rec.minOverMean.toFixed(3)} |`);
    }
  }
}

console.log('\n## Six-sample 16-thread fairness averages per cap');
console.log('| cap | avg max/min | avg min/mean |');
console.log('|---|---|---|');
const sixSample = {};
for (const cap of [6, 8, 10]) {
  const recs = [
    r1(cap, 16, 'push_pop'),
    r1(cap, 16, 'churn'),
    findRec(2, cap, 16, 1, 'push_pop'),
    findRec(2, cap, 16, 1, 'churn'),
    findRec(2, cap, 16, 2, 'push_pop'),
    findRec(2, cap, 16, 2, 'churn'),
  ];
  const avgMaxOverMin = recs.reduce((a, r) => a + r.maxOverMin, 0) / 6;
  const avgMinOverMean = recs.reduce((a, r) => a + r.minOverMean, 0) / 6;
  sixSample[cap] = { avgMaxOverMin, avgMinOverMean };
  console.log(`| ${cap} | ${avgMaxOverMin.toFixed(2)} | ${avgMinOverMean.toFixed(3)} |`);
}

// ---------------------------------------------------------------------------
// latency log parse
// ---------------------------------------------------------------------------
const fmtMs = (x) => x.toFixed(3);
const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const n = s.length;
  return n % 2 === 1 ? s[(n - 1) / 2] : (s[n / 2 - 1] + s[n / 2]) / 2;
};
const latFor = (capLabel, threads, iters) =>
  latRecords.filter((r) => r.cap_label === capLabel && r.threads === threads && r.iters === iters).sort((a, b) => a.rep - b.rep);
const SHAPES = [
  [4, 20000],
  [8, 200000],
  [16, 200000],
];

console.log('\n## Per-call pop tail latency (median over 3 reps per shape)');
console.log('| shape | cap | median pop_max ms | max pop_max ms | min over_1ms | max over_1ms | median wall ms |');
console.log('|---|---|---|---|---|---|---|');
const latStats = {};
for (const [t, it] of SHAPES) {
  for (const cl of ['6', '0']) {
    const recs = latFor(cl, t, it);
    const medMax = median(recs.map((r) => r.pop_max_ms));
    const maxMax = Math.max(...recs.map((r) => r.pop_max_ms));
    const minOver1 = Math.min(...recs.map((r) => r.pop_over_1ms));
    const maxOver1 = Math.max(...recs.map((r) => r.pop_over_1ms));
    const medWall = median(recs.map((r) => r.wall_ms));
    latStats[`${t}x${it}/${cl}`] = { medMax, medWall, recs };
    console.log(`| ${t}x${it} | ${cl} | ${fmtMs(medMax)} | ${fmtMs(maxMax)} | ${minOver1} | ${maxOver1} | ${medWall.toFixed(1)} |`);
  }
}
console.log('\nPer-shape ratios (median-to-median):');
for (const [t, it] of SHAPES) {
  const six = latStats[`${t}x${it}/6`];
  const zero = latStats[`${t}x${it}/0`];
  const worstPopRatio = six.medMax / zero.medMax;
  const wallSpeedup = zero.medWall / six.medWall;
  const sixes = latFor('6', t, it).map((r) => r.pop_max_ms);
  const zeros = latFor('0', t, it).map((r) => r.pop_max_ms);
  const minPl = Math.min(...sixes) / Math.max(...zeros);
  const maxPl = Math.max(...sixes) / Math.min(...zeros);
  console.log(`| ${t}x${it} | cap6/cap0 worst-pop ratio ${worstPopRatio.toFixed(1)}x (plausible ${minPl.toFixed(1)}x-${maxPl.toFixed(1)}x across rep pairings) | cap0/cap6 wall speedup ${wallSpeedup.toFixed(2)}x |`);
}
console.log('\n## Tail mass by threshold (per-rep counts, cap6 | cap0)');
console.log('| shape/threshold | cap 6 | cap 0 |');
console.log('|---|---|---|');
for (const [name, t, it, field] of [
  ['8x200000 >1ms', 8, 200000, 'pop_over_1ms'],
  ['8x200000 >10ms', 8, 200000, 'pop_over_10ms'],
  ['16x200000 >1ms', 16, 200000, 'pop_over_1ms'],
  ['16x200000 >10ms', 16, 200000, 'pop_over_10ms'],
  ['16x200000 >100ms', 16, 200000, 'pop_over_100ms'],
]) {
  const fmt = (cl) => {
    const xs = latFor(cl, t, it).map((r) => r[field]);
    return `[${xs.join(', ')}]`;
  };
  console.log(`| ${name} | ${fmt('6')} | ${fmt('0')} |`);
}
console.log('\n## Percentiles, cap6 vs cap0 (median (min-max) over 3 reps, ms)');
console.log('| shape | cap6 p50 | cap0 p50 | cap6 p90 | cap0 p90 | cap6 p99 | cap0 p99 | cap6 p999 | cap0 p999 |');
console.log('|---|---|---|---|---|---|---|---|---|');
for (const [t, it] of SHAPES) {
  const cell = (cl, p) => {
    const xs = latFor(cl, t, it).map((r) => r[p]);
    const mn = Math.min(...xs);
    const mx = Math.max(...xs);
    return mn === mx ? mn.toFixed(3) : `${median(xs).toFixed(3)} (${mn.toFixed(3)}-${mx.toFixed(3)})`;
  };
  const ps = ['pop_p50_ms', 'pop_p90_ms', 'pop_p99_ms', 'pop_p999_ms'];
  const cells = ps.map((p) => [cell('6', p), cell('0', p)]).flat();
  console.log(`| ${t}x${it} | ${cells.join(' | ')} |`);
}

// ---------------------------------------------------------------------------
// (5) assertions
// ---------------------------------------------------------------------------
console.log('\n-- assertions --');

// A1
{
  const expected = [21.2, 23.0, 17.4, 37.3, 4.7, 2.7, 6.7, -0.4, 17.4, 18.2, 26.8, 26.9, 44.6, 57.2, 53.6, 58.4];
  const ok = expected.every((e, i) => near(deltaCells[i], e, 0.05));
  assert(ok, `A1: all 16 cap8/cap10-vs-cap6 delta cells match the gate table within 0.05 (got [${deltaCells.map((d) => d.toFixed(1)).join(', ')}])`);
}
// A2
{
  const negs = deltaCells.filter((d) => d < 0);
  const idx = deltaCells.findIndex((d) => d < 0);
  const [t, b] = ARM_ORDER[Math.floor(idx / 2)];
  const isChurn10 = t === 4 && b === 'churn' && idx % 2 === 1;
  assert(negs.length === 1 && isChurn10, `A2: exactly one negative delta cell and it is (4, churn, cap10) (got ${negs.length} negative(s))`);
  pass(`cap 8/10 vs cap 6: 15 of 16 cells positive; sole exception 4/churn cap-10 at ${fmtPct(deltaCells[idx])} (round-8 P3-2: the old 'at EVERY thread count' heading was false)`);
}
// A3
{
  const min = Math.min(...deltaCells);
  const max = Math.max(...deltaCells);
  assert(near(min, -0.4, 0.05) && near(max, 58.4, 0.05), `A3: delta range is -0.4..+58.4 (got ${min.toFixed(1)}..${max.toFixed(1)}), NOT '+17% to +58%'`);
}
// A4
{
  const block = deltaCells.slice(0, 4);
  assert(near(Math.min(...block), 17.4, 0.05) && near(Math.max(...block), 37.3, 0.05), `A4: 2-thread delta block is +17.4..+37.3 (got ${Math.min(...block).toFixed(1)}..${Math.max(...block).toFixed(1)})`);
}
// A5
{
  const ratios = ARM_ORDER.map(([t, b]) => [t, b, r1(6, t, b).totalOps / r1(0, t, b).totalOps]);
  const mn = ratios.reduce((a, x) => (x[2] < a[2] ? x : a));
  const mx = ratios.reduce((a, x) => (x[2] > a[2] ? x : a));
  assert(near(mn[2], 1.60, 0.02) && mn[0] === 2 && mn[1] === 'push_pop', `A5: cap6/cap0 min ratio 1.60 at (2,push_pop) (got ${mn[2].toFixed(2)} at (${mn[0]},${mn[1]}))`);
  assert(near(mx[2], 9.50, 0.02) && mx[0] === 8 && mx[1] === 'churn', `A5: cap6/cap0 max ratio 9.50 at (8,churn) (got ${mx[2].toFixed(2)} at (${mx[0]},${mx[1]}))`);
}
// A6
{
  const ratios = ARM_ORDER.map(([t, b]) => [t, b, r1(6, t, b).totalOps / r1(4, t, b).totalOps]);
  const mn = ratios.reduce((a, x) => (x[2] < a[2] ? x : a));
  const mx = ratios.reduce((a, x) => (x[2] > a[2] ? x : a));
  assert(near(mn[2], 0.78, 0.02) && mn[0] === 2 && mn[1] === 'churn', `A6: cap6/cap4 min ratio 0.78 at (2,churn) (got ${mn[2].toFixed(2)} at (${mn[0]},${mn[1]}))`);
  assert(near(mx[2], 3.99, 0.02) && mx[0] === 16 && mx[1] === 'churn', `A6: cap6/cap4 max ratio 3.99 at (16,churn) (got ${mx[2].toFixed(2)} at (${mx[0]},${mx[1]}))`);
}
// B1
{
  let strict = 0;
  let tie = null;
  for (const [t, b] of ARM_ORDER) {
    const c0 = r1(0, t, b).minOverMean;
    const c6 = r1(6, t, b).minOverMean;
    if (c0 < c6 - 5e-4) fail(`B1: cap0 min/mean < cap6 at (${t},${b})`);
    if (c0 > c6 + 5e-4) strict++;
    else tie = [t, b];
  }
  assert(strict === 7 && tie[0] === 2 && tie[1] === 'push_pop' && near(r1(0, 2, 'push_pop').minOverMean, 0.950, 5e-4) && near(r1(6, 2, 'push_pop').minOverMean, 0.950, 5e-4), `B1: cap0 >= cap6 min/mean in all 8 arms, strictly greater in 7, single tie at (2,push_pop)=0.950 (got ${strict} strict, tie at (${tie[0]},${tie[1]}))`);
}
// B2
{
  const n = ARM_ORDER.filter(([t, b]) => r1(4, t, b).minOverMean > r1(6, t, b).minOverMean).length;
  assert(n === 6, `B2: cap4 min/mean > cap6 in exactly 6 of 8 arms (got ${n})`);
}
// B3
{
  const n8 = ARM_ORDER.filter(([t, b]) => r1(6, t, b).minOverMean > r1(8, t, b).minOverMean).length;
  const n10 = ARM_ORDER.filter(([t, b]) => r1(6, t, b).minOverMean > r1(10, t, b).minOverMean).length;
  assert(n8 === 7, `B3: cap6 min/mean > cap8 in exactly 7 of 8 arms (got ${n8})`);
  assert(n10 === 6, `B3: cap6 min/mean > cap10 in exactly 6 of 8 arms (got ${n10})`);
}
// B4
{
  const avg = (cap) => ARM_ORDER.reduce((a, [t, b]) => a + r1(cap, t, b).minOverMean, 0) / 8;
  const expected = { 0: 0.846, 4: 0.765, 6: 0.652, 8: 0.562, 10: 0.538 };
  for (const cap of CAPS) {
    assert(near(avg(cap), expected[cap], 0.0015), `B4: avg min/mean cap${cap} = ${expected[cap]} (got ${avg(cap).toFixed(4)})`);
  }
  assert(avg(0) > avg(4) && avg(4) > avg(6) && avg(6) > avg(8) && avg(8) > avg(10), 'B4: avg min/mean strictly ordered 0 > 4 > 6 > 8 > 10');
}
// B5
{
  const avg = (cap) => ARM_ORDER.reduce((a, [t, b]) => a + r1(cap, t, b).maxOverMin, 0) / 8;
  const expected = { 0: 2.670, 4: 2.307, 6: 3.056, 8: 5.333, 10: 3.653 };
  for (const cap of CAPS) {
    assert(near(avg(cap), expected[cap], 0.002), `B5: avg max/min cap${cap} = ${expected[cap]} (got ${avg(cap).toFixed(4)})`);
  }
  assert(avg(4) < avg(0) && avg(0) < avg(6) && avg(6) < avg(10) && avg(10) < avg(8), 'B5: avg max/min ordered 4 < 0 < 6 < 10 < 8');
  pass('B5 note: metric-dependence — min/mean orders 0 > 4 and 8 > 10, but max/min orders 4 < 0 and 8 < 10; the two orderings FLIP, and the report must state this rather than hide it');
}
// B6
{
  const c0 = r1(0, 16, 'push_pop').maxOverMin;
  const c6 = r1(6, 16, 'push_pop').maxOverMin;
  assert(c0 > c6, `B6: at (16,push_pop) cap0 max/min (${c0.toFixed(3)}) is worse than cap6's (${c6.toFixed(3)})`);
}
// B7 — the P2-1 pin
{
  let strictMaxArms = 0;
  for (const [t, b] of ARM_ORDER) {
    const c6 = r1(6, t, b).minOverMean;
    const others = CAPS.filter((c) => c !== 6).map((c) => r1(c, t, b).minOverMean);
    if (others.every((x) => x < c6 - 5e-4)) strictMaxArms++;
  }
  assert(strictMaxArms === 0, `B7 (P2-1 pin): cap 6 is the strict min/mean maximum in 0 of 8 arms (got ${strictMaxArms})`);
  pass(`cap 6 is the strict min/mean maximum in 0 of 8 arms — round-8 P2-1's false 'most fairness-conscious' conclusion is dead`);
}
// C1
{
  for (const cap of [6, 8, 10]) {
    const expMom = { 6: 6.12, 8: 13.12, 10: 20.64 }[cap];
    const expMomMean = { 6: 0.375, 8: 0.190, 10: 0.201 }[cap];
    assert(near(sixSample[cap].avgMaxOverMin, expMom, 0.01), `C1: six-sample avg max/min cap${cap} = ${expMom} (got ${sixSample[cap].avgMaxOverMin.toFixed(3)})`);
    assert(near(sixSample[cap].avgMinOverMean, expMomMean, 0.0015), `C1: six-sample avg min/mean cap${cap} = ${expMomMean} (got ${sixSample[cap].avgMinOverMean.toFixed(4)})`);
  }
}
// D1
{
  assert(/const BACKOFF_SPIN_CAP: u32 = 6;/.test(latText), 'D1: latency log contains const BACKOFF_SPIN_CAP: u32 = 6; (cap-6 arm resolved-cap evidence)');
  assert(/const BACKOFF_SPIN_CAP: u32 = 0;/.test(latText), 'D1: latency log contains const BACKOFF_SPIN_CAP: u32 = 0; (cap-0 arm resolved-cap evidence)');
  assert(/# patch_hash_sha256\(git diff\): [0-9a-f]{64}/.test(latText), 'D1: latency log contains patch_hash_sha256(git diff) with 64 hex chars');
  assert(latText.includes('# post-run restore verification:'), 'D1: latency log contains post-run restore verification line');
  assert(latText.includes('base_sha: 842c998'), 'D1: latency log contains base_sha: 842c998');
  assert(latRecords.filter((r) => r.cap_label === '6').length === 9, 'D1: exactly 9 JSON lines with cap_label "6"');
  assert(latRecords.filter((r) => r.cap_label === '0').length === 9, 'D1: exactly 9 JSON lines with cap_label "0"');
  assert(latRecords.every((r) => r.pop_samples === r.threads * r.iters), 'D1: every JSON line has pop_samples == threads*iters');
}
// D2
{
  const expMax = { '4x20000': [4.813, 0.159], '8x200000': [54.464, 2.031], '16x200000': [160.092, 42.335] };
  const expRatio = { '4x20000': 30.3, '8x200000': 26.8, '16x200000': 3.8 };
  for (const [t, it] of SHAPES) {
    const key = `${t}x${it}`;
    const six = latStats[`${key}/6`].medMax;
    const zero = latStats[`${key}/0`].medMax;
    assert(near(six, expMax[key][0], 0.01), `D2: ${key} median cap6 pop_max = ${expMax[key][0]} ms (got ${six.toFixed(3)})`);
    assert(near(zero, expMax[key][1], 0.01), `D2: ${key} median cap0 pop_max = ${expMax[key][1]} ms (got ${zero.toFixed(3)})`);
    assert(near(six / zero, expRatio[key], 0.1), `D2: ${key} cap6/cap0 worst-pop ratio = ${expRatio[key]} (got ${(six / zero).toFixed(2)})`);
  }
}
// D3
{
  const expWall = { '4x20000': [14.6, 61.0], '8x200000': [321.9, 1560.3], '16x200000': [867.5, 3509.6] };
  const expSpeed = { '4x20000': 4.18, '8x200000': 4.85, '16x200000': 4.05 };
  for (const [t, it] of SHAPES) {
    const key = `${t}x${it}`;
    const six = latStats[`${key}/6`].medWall;
    const zero = latStats[`${key}/0`].medWall;
    assert(near(six, expWall[key][0], 0.5), `D3: ${key} median cap6 wall = ${expWall[key][0]} ms (got ${six.toFixed(1)})`);
    assert(near(zero, expWall[key][1], 0.5), `D3: ${key} median cap0 wall = ${expWall[key][1]} ms (got ${zero.toFixed(1)})`);
    assert(near(zero / six, expSpeed[key], 0.03), `D3: ${key} wall speedup cap0/cap6 = ${expSpeed[key]} (got ${(zero / six).toFixed(3)})`);
  }
}
// D4 — extended in round 9 (P2-2/P4-7): ALL five threshold cells, both arms,
// exact per-rep triples (an earlier version pinned only the two cells that
// agreed with the prose and omitted the 16-thread >1 ms reversal).
{
  const cell = (cl, t, it, field) => latFor(cl, t, it).map((r) => r[field]);
  const expect = {
    '8x200000 >1ms': [[86, 66, 60], [8, 0, 3]],
    '8x200000 >10ms': [[34, 29, 26], [2, 0, 0]],
    '16x200000 >1ms': [[285, 266, 249], [553, 661, 650]],
    '16x200000 >10ms': [[178, 131, 169], [110, 161, 157]],
    '16x200000 >100ms': [[4, 3, 3], [0, 0, 0]],
  };
  const order = [
    ['8x200000 >1ms', 8, 200000, 'pop_over_1ms'],
    ['8x200000 >10ms', 8, 200000, 'pop_over_10ms'],
    ['16x200000 >1ms', 16, 200000, 'pop_over_1ms'],
    ['16x200000 >10ms', 16, 200000, 'pop_over_10ms'],
    ['16x200000 >100ms', 16, 200000, 'pop_over_100ms'],
  ];
  for (const [name, t, it, field] of order) {
    const six = cell('6', t, it, field);
    const zero = cell('0', t, it, field);
    const [e6, e0] = expect[name];
    assert(JSON.stringify(six) === JSON.stringify(e6), `D4: ${name} cap6 over-threshold counts = [${e6.join(', ')}] (got [${six.join(', ')}])`);
    assert(JSON.stringify(zero) === JSON.stringify(e0), `D4: ${name} cap0 over-threshold counts = [${e0.join(', ')}] (got [${zero.join(', ')}])`);
  }
  assert(Math.min(...cell('6', 8, 200000, 'pop_over_1ms')) > Math.max(...cell('0', 8, 200000, 'pop_over_1ms')), 'D4: 8x200000 >1ms — every cap6 rep (60-86) exceeds every cap0 rep (0-8)');
  assert(Math.min(...cell('6', 8, 200000, 'pop_over_10ms')) > Math.max(...cell('0', 8, 200000, 'pop_over_10ms')), 'D4: 8x200000 >10ms — every cap6 rep (26-34) exceeds every cap0 rep (0-2)');
  assert(Math.min(...cell('0', 16, 200000, 'pop_over_1ms')) > Math.max(...cell('6', 16, 200000, 'pop_over_1ms')), 'D4: 16x200000 >1ms REVERSES the 8-thread sign — every cap0 rep (553-661) exceeds every cap6 rep (249-285)');
  assert(near(median(cell('0', 16, 200000, 'pop_over_1ms')) / median(cell('6', 16, 200000, 'pop_over_1ms')), 2.44, 0.05), `D4: 16x200000 >1ms cap0/cap6 median-to-median ~2.44x (got ${(median(cell('0', 16, 200000, 'pop_over_1ms')) / median(cell('6', 16, 200000, 'pop_over_1ms'))).toFixed(2)}x)`);
  assert(Math.min(...cell('6', 16, 200000, 'pop_over_10ms')) < Math.max(...cell('0', 16, 200000, 'pop_over_10ms')) && Math.max(...cell('6', 16, 200000, 'pop_over_10ms')) > Math.min(...cell('0', 16, 200000, 'pop_over_10ms')), 'D4: 16x200000 >10ms — ranges OVERLAP (110-161 vs 131-178): roughly tied, neither arm dominates');
  assert(Math.min(...cell('6', 16, 200000, 'pop_over_100ms')) >= 3 && Math.max(...cell('0', 16, 200000, 'pop_over_100ms')) === 0, 'D4: 16x200000 >100ms — cap6 >= 3 in every rep, cap0 == 0 in every rep');
}
// D5 — round 9 (P2-2/P4-7): percentiles and quoted extremes. The report
// publishes cap 6 as better-or-equal at p50/p90/p99/p99.9 in EVERY shape and
// rep, plus specific range values; pin all of them.
{
  for (const [t, it] of SHAPES) {
    for (const p of ['pop_p50_ms', 'pop_p90_ms', 'pop_p99_ms', 'pop_p999_ms']) {
      const worse = latFor('6', t, it).filter((r) => {
        const z = latFor('0', t, it).find((x) => x.rep === r.rep);
        return r[p] > z[p];
      });
      assert(worse.length === 0, `D5: ${t}x${it} — cap6 ${p} <= cap0 in every rep (violations: ${worse.map((r) => `rep${r.rep}=${r[p]}`).join(', ') || 'none'})`);
    }
  }
  const six999_8 = latFor('6', 8, 200000).map((r) => r.pop_p999_ms);
  assert(six999_8.every((v) => v === 0.001), `D5: 8x200000 cap6 pop_p999_ms = 0.001 in every rep (got [${six999_8.join(', ')}])`);
  const zero999 = {
    '4x20000': latFor('0', 4, 20000).map((r) => r.pop_p999_ms),
    '8x200000': latFor('0', 8, 200000).map((r) => r.pop_p999_ms),
    '16x200000': latFor('0', 16, 200000).map((r) => r.pop_p999_ms),
  };
  assert(Math.min(...zero999['4x20000']) === 0.022 && Math.max(...zero999['4x20000']) === 0.037, `D5: 4x20000 cap0 pop_p999_ms range 0.022-0.037 (got [${zero999['4x20000'].join(', ')}])`);
  assert(Math.min(...zero999['8x200000']) === 0.054 && Math.max(...zero999['8x200000']) === 0.057, `D5: 8x200000 cap0 pop_p999_ms range 0.054-0.057 (got [${zero999['8x200000'].join(', ')}])`);
  assert(Math.min(...zero999['16x200000']) === 0.172 && Math.max(...zero999['16x200000']) === 0.182, `D5: 16x200000 cap0 pop_p999_ms range 0.172-0.182 (got [${zero999['16x200000'].join(', ')}])`);
  assert([...latFor('6', 4, 20000), ...latFor('6', 8, 200000), ...latFor('6', 16, 200000)].every((r) => r.pop_p50_ms === 0), 'D5: cap6 pop_p50_ms = 0.000 in every rep of every shape');
  const speeds = SHAPES.map(([t, it]) => median(latFor('0', t, it).map((r) => r.wall_ms)) / median(latFor('6', t, it).map((r) => r.wall_ms)));
  assert(near(Math.min(...speeds), 4.05, 0.01) && near(Math.max(...speeds), 4.85, 0.01), `D5: wall-clock speedup range 4.05-4.85 across shapes (got ${Math.min(...speeds).toFixed(2)}-${Math.max(...speeds).toFixed(2)})`);
  const expMaxOf3 = { '4x20000': [10.828, 0.297], '8x200000': [59.705, 23.567], '16x200000': [173.365, 46.301] };
  for (const [t, it] of SHAPES) {
    const key = `${t}x${it}`;
    assert(near(Math.max(...latFor('6', t, it).map((r) => r.pop_max_ms)), expMaxOf3[key][0], 0.01), `D5: ${key} cap6 worst-of-3 pop_max = ${expMaxOf3[key][0]} ms (got ${Math.max(...latFor('6', t, it).map((r) => r.pop_max_ms)).toFixed(3)})`);
    assert(near(Math.max(...latFor('0', t, it).map((r) => r.pop_max_ms)), expMaxOf3[key][1], 0.01), `D5: ${key} cap0 worst-of-3 pop_max = ${expMaxOf3[key][1]} ms (got ${Math.max(...latFor('0', t, it).map((r) => r.pop_max_ms)).toFixed(3)})`);
  }
  assert(latFor('0', 8, 200000).find((r) => r.rep === 1).pop_max_ms === 23.567, `D5: cap0 8x200000 rep-1 pop_max = 23.567 ms (the caveat-(1) scheduler-noise figure)`);
}
// D6 — round 9 (P3-2): the worst-pop cap6/cap0 ratio is a max-of-3-over-max-
// of-3 statistic; pin its plausible min/max across rep pairings per shape.
{
  const exp = { '4x20000': [4.3, 68.1], '8x200000': [1.8, 100.2], '16x200000': [2.8, 4.4] };
  for (const [t, it] of SHAPES) {
    const key = `${t}x${it}`;
    const sixes = latFor('6', t, it).map((r) => r.pop_max_ms);
    const zeros = latFor('0', t, it).map((r) => r.pop_max_ms);
    const minPl = Math.min(...sixes) / Math.max(...zeros);
    const maxPl = Math.max(...sixes) / Math.min(...zeros);
    assert(near(minPl, exp[key][0], 0.06), `D6: ${key} MIN plausible cap6/cap0 worst-pop ratio = ${exp[key][0]} (got ${minPl.toFixed(2)})`);
    assert(near(maxPl, exp[key][1], 0.2), `D6: ${key} MAX plausible cap6/cap0 worst-pop ratio = ${exp[key][1]} (got ${maxPl.toFixed(2)})`);
  }
}

// ---------------------------------------------------------------------------
// (6) --write: regenerate the summary CSV
// ---------------------------------------------------------------------------
const HEADER_EXTRA = ',pop_p50_ms,pop_p90_ms,pop_p99_ms,pop_p999_ms,pop_max_ms,pop_over_1ms,pop_over_10ms,pop_over_100ms,wall_ms';
if (WRITE) {
  // verify the current file's first 11 fields against the freshly derived rows
  // Latency (run=3) rows are regenerated from the raw latency log below, so
  // only sweep rows are re-validated here against the raw records.
  for (const vals of sweepRows) {
    const [run, cap, threads, rep, bench] = vals;
    const rec = findRec(Number(run), Number(cap), Number(threads), Number(rep), bench);
    const fresh = [
      String(rec.run),
      String(rec.cap),
      String(rec.threads),
      String(rec.rep),
      rec.bench,
      String(rec.totalOps),
      String(rec.perThreadMax),
      String(rec.perThreadMin),
      String(rec.perThreadMean),
      rec.maxOverMin.toFixed(3),
      rec.minOverMean.toFixed(3),
    ];
    const cur = vals.slice(0, 11);
    const same = fresh.every((f, i) => f === cur[i] || near(Number(f), Number(cur[i]), 1e-9));
    if (!same) {
      console.error(`ASSERTION FAILED: --write pre-check: current CSV row first 11 fields disagree with re-derived values\n  current: ${cur.join(',')}\n  derived: ${fresh.join(',')}`);
      process.exit(1);
    }
  }
  const latSorted = [...latRecords].sort((a, b) => {
    const ka = a.cap_label === '6' ? 0 : 1;
    const kb = b.cap_label === '6' ? 0 : 1;
    return ka - kb || a.threads - b.threads || a.rep - b.rep;
  });
  const latRows = latSorted.map(
    (r) =>
      `3,${r.cap_label},${r.threads},${r.rep},pop_latency,,,,,,,${r.pop_p50_ms.toFixed(3)},${r.pop_p90_ms.toFixed(3)},${r.pop_p99_ms.toFixed(3)},${r.pop_p999_ms.toFixed(3)},${r.pop_max_ms.toFixed(3)},${r.pop_over_1ms},${r.pop_over_10ms},${r.pop_over_100ms},${r.wall_ms.toFixed(1)}`,
  );
  const lines = [
    `run,cap,threads,rep,bench,total_ops_per_sec,per_thread_max,per_thread_min,per_thread_mean,max_over_min,min_over_mean${HEADER_EXTRA}`,
    ...sweepRows.map((vals) => `${vals.slice(0, 11).join(',')},,,,,,,,,`),
    ...latRows,
  ];
  writeFileSync(new URL(CSV, ROOT), lines.join('\n') + '\n');
  console.log(`\nwrote ${CSV}: header extended with 9 latency columns, 52 sweep rows padded, 18 latency rows appended`);
}

console.log(`\nALL ${assertionCount} ASSERTIONS PASSED`);
if (assertionCount < 1) {
  console.error('ASSERTION FAILED: assertion counter is zero');
  process.exit(1);
}
