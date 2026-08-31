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
// Usage:
//   node scripts/tis_backoff_cap_sweep_derive_report_data.mjs            # verify + print tables
//   node scripts/tis_backoff_cap_sweep_derive_report_data.mjs --write    # also regenerate the summary CSV

import { readFileSync, writeFileSync } from 'node:fs';

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
      vals.slice(5, 14).every((v) => v === ''),
      `latency CSV row ${vals.slice(0, 5).join(',')}: the 6 throughput fields and 4 spacers are empty`,
    );
    const rec = latRecords.find(
      (r) => r.cap_label === cap && r.threads === Number(threads) && r.rep === Number(rep),
    );
    assert(!!rec, `latency CSV row (3,${cap},${threads},${rep},pop_latency) has a matching JSON line in the latency log`);
    if (!rec) continue;
    assert(
      vals.slice(14, 23).length === 9,
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
    const got = vals.slice(14, 23);
    const ok = want.every((w, i) => got[i] === w);
    assert(ok, `latency CSV row (3,${cap},${threads},${rep}): all 9 latency fields match the JSON log exactly (got [${got.join(',')}])`);
  }
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
  console.log(`| ${t}x${it} | cap6/cap0 worst-pop ratio ${worstPopRatio.toFixed(1)}x | cap0/cap6 wall speedup ${wallSpeedup.toFixed(2)}x |`);
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
// D4
{
  const sixOver1 = latFor('6', 8, 200000).map((r) => r.pop_over_1ms);
  const zeroOver1 = latFor('0', 8, 200000).map((r) => r.pop_over_1ms);
  assert(Math.min(...sixOver1) > Math.max(...zeroOver1), `D4: at 8x200000 min cap6 over_1ms (${Math.min(...sixOver1)}) > max cap0 over_1ms (${Math.max(...zeroOver1)})`);
  const sixOver100 = latFor('6', 16, 200000).map((r) => r.pop_over_100ms);
  const zeroOver100 = latFor('0', 16, 200000).map((r) => r.pop_over_100ms);
  assert(sixOver100.every((x) => x >= 3), `D4: at 16x200000 cap6 over_100ms >= 3 in every rep (got [${sixOver100.join(', ')}])`);
  assert(zeroOver100.every((x) => x === 0), `D4: at 16x200000 cap0 over_100ms == 0 in every rep (got [${zeroOver100.join(', ')}])`);
}

// ---------------------------------------------------------------------------
// (6) --write: regenerate the summary CSV
// ---------------------------------------------------------------------------
const WRITE = process.argv.includes('--write');
const HEADER_EXTRA = ',pop_p50_ms,pop_p90_ms,pop_p99_ms,pop_p999_ms,pop_max_ms,pop_over_1ms,pop_over_10ms,pop_over_100ms,wall_ms';
if (WRITE) {
  // verify the current file's first 11 fields against the freshly derived rows
  for (const vals of csvRows) {
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
      `3,${r.cap_label},${r.threads},${r.rep},pop_latency,,,,,,,,,,${r.pop_p50_ms.toFixed(3)},${r.pop_p90_ms.toFixed(3)},${r.pop_p99_ms.toFixed(3)},${r.pop_p999_ms.toFixed(3)},${r.pop_max_ms.toFixed(3)},${r.pop_over_1ms},${r.pop_over_10ms},${r.pop_over_100ms},${r.wall_ms.toFixed(1)}`,
  );
  const lines = [
    `run,cap,threads,rep,bench,total_ops_per_sec,per_thread_max,per_thread_min,per_thread_mean,max_over_min,min_over_mean${HEADER_EXTRA}`,
    ...csvRows.map((vals) => `${vals.slice(0, 11).join(',')},,,,,,,,,`),
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
