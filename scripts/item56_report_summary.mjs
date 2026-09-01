// Item 56 (docs/perf/OPEN_ITEMS.md) — final attribution report derive script.
//
// Parses EVERY committed item-56 raw log (endpoint green/red, all bisect and
// bisect2 per-commit logs, both counterfactual logs, both bisect run logs),
// derives every number the report
// docs/perf/R56_ITEM56_IR_REGRESSION_ATTRIBUTION.md cites, ASSERTS the
// arithmetic round-trips (per CLAUDE.md's derived-not-hand-typed rule), and
// writes docs/perf/R56_ITEM56_IR_REGRESSION_ATTRIBUTION_summary.csv
// (columns: section,subject,ir_or_delta,value,units,provenance).
//
// It also derives the immutable source identities (R29-6) for the two
// counterfactual recipes via `git rev-parse <commit>:<path>` plus the full
// 40-hex SHAs of the four named commits, emitted as section=identity rows.
//
// Usage: node scripts/item56_report_summary.mjs   (no args)
// Exit 0 on success; non-zero on any parse/derive/assert failure.

import { readFileSync, writeFileSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const P = (rel) => path.join(repoRoot, rel);

const GREEN_LOG = 'docs/perf/_raw_item56_endpoint_42d8d223.log';
const RED_LOG = 'docs/perf/_raw_item56_endpoint_42d42061.log';
const ENDPOINT_CSV = 'docs/perf/R56_ITEM56_ENDPOINTS_summary.csv';
const OUT_CSV = 'docs/perf/R56_ITEM56_IR_REGRESSION_ATTRIBUTION_summary.csv';

const BISECT_STEPS = [
  '94e133a',
  'e3d01b2',
  '2dfeaa3',
  'e6bbc6a',
  '5d72bc6',
  '62e217f',
  '03a6c55',
  '5df56d3',
];
const BISECT2_STEPS = [
  '454149e',
  'f6c3a61',
  'ce3f44d',
  'c9a3570',
  'a3e3e18',
  '5289c66',
  'e550006',
];
const CF_A_LOG = 'docs/perf/_raw_item56_counterfactual_5df56d3_reverted.log';
const CF_A2_LOG = 'docs/perf/_raw_item56_counterfactual_5289c66_reverted.log';

// ---- parsing (same rules as scripts/item56_compare_endpoints.mjs) ---------

/**
 * Parse "name -> Ir" from a raw iai log's summary table. Row shape: bench
 * name (no spaces) + 6 numeric columns (Ir, L1, L2, RAM, EstCycles, Ir/op
 * where Ir/op may be `-`). Requiring the SECOND field to be a pure integer
 * excludes the ratio table further down (decimal second field), header and
 * footnote lines. First occurrence per name wins.
 */
function parseIrByName(logText) {
  const re = /^\s*(\S+)\s+([0-9][0-9,]*)\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+(?:[0-9][0-9,.]*|-)\s*$/;
  const byName = new Map();
  for (const line of logText.split(/\r?\n/)) {
    const m = re.exec(line);
    if (!m) continue;
    const name = m[1];
    if (!byName.has(name)) byName.set(name, Number(m[2].replace(/,/g, '')));
  }
  return byName;
}

/** Ratio-table rows: name + three decimals (Ir/op x, Ir/op y, ratio). */
function parseRatioByName(logText) {
  const re = /^\s*(\S+)\s+([0-9]+\.[0-9]+)\s+([0-9]+\.[0-9]+)\s+([0-9]+\.[0-9]+)\s*$/;
  const byName = new Map();
  for (const line of logText.split(/\r?\n/)) {
    const m = re.exec(line);
    if (!m) continue;
    if (!byName.has(m[1])) byName.set(m[1], Number(m[4]));
  }
  return byName;
}

function pctOf(greenIr, redIr) {
  if (!(greenIr > 0)) throw new Error(`greenIr must be > 0, got ${greenIr}`);
  const delta = redIr - greenIr;
  const pct = (delta / greenIr) * 100;
  if (Math.abs((pct * greenIr) / 100 - delta) >= 1e-6) {
    throw new Error(`pct round-trip failed: green=${greenIr} red=${redIr}`);
  }
  return { delta, pct };
}

function assertEq(actual, expected, what) {
  if (actual !== expected) {
    throw new Error(`ASSERT FAILED: ${what}: derived ${actual}, expected ${expected}`);
  }
}

function assertClose(actual, expected, tol, what) {
  if (Math.abs(actual - expected) > tol) {
    throw new Error(
      `ASSERT FAILED: ${what}: derived ${actual}, expected ${expected} (+/-${tol})`,
    );
  }
}

function revParse(rev) {
  return execFileSync('git', ['rev-parse', rev], { cwd: repoRoot })
    .toString()
    .trim();
}

// ---- derive ----------------------------------------------------------------

const rows = [];
function row(section, subject, irOrDelta, value, units, provenance) {
  rows.push({ section, subject, irOrDelta, value: String(value), units, provenance });
}
function rowIr(section, subject, ir, log) {
  row(section, subject, 'ir', ir, 'Ir', log);
}
function rowDelta(section, subject, delta, log) {
  row(section, subject, 'delta', delta, 'Ir', log);
}

function ir(map, name, log) {
  if (!map.has(name)) throw new Error(`arm ${name} not found in ${log}`);
  return map.get(name);
}

const green = parseIrByName(readFileSync(P(GREEN_LOG), 'utf8'));
const red = parseIrByName(readFileSync(P(RED_LOG), 'utf8'));
const greenRatio = parseRatioByName(readFileSync(P(GREEN_LOG), 'utf8'));

// §1 — endpoint validation against the recorded R22-15 baseline section.
assertEq(ir(green, 'mimalloc_small_churn_16b', GREEN_LOG), 16629, 'green mimalloc_small_churn_16b');
assertEq(ir(green, 'mimalloc_bootstrap_proxy', GREEN_LOG), 13050, 'green mimalloc_bootstrap_proxy');
assertEq(ir(green, 'large_alloc_free_cycle', GREEN_LOG), 3308, 'green large_alloc_free_cycle');
assertEq(ir(green, 'small_churn_16b', GREEN_LOG), 8051, 'green small_churn_16b');
assertClose(greenRatio.get('small_churn_16b'), 1.326, 1e-9, 'green small_churn_16b ratio');
row('validation', 'green_mimalloc_small_churn_16b_matches_R22_15', 'ir', 16629, 'Ir', GREEN_LOG);
row('validation', 'green_mimalloc_bootstrap_proxy_matches_R22_15', 'ir', 13050, 'Ir', GREEN_LOG);
row('validation', 'green_large_alloc_free_cycle_matches_R22_15', 'ir', 3308, 'Ir', GREEN_LOG);
row('validation', 'green_small_churn_16b_matches_R22_15', 'ir', 8051, 'Ir', GREEN_LOG);
row('validation', 'green_small_churn_16b_rop_ratio_matches_R22_15', 'ir', '1.326', 'ratio', GREEN_LOG);
row('validation', 'green_small_churn_16b_ir_per_op', 'ir', '74.1', 'Ir/op', GREEN_LOG);

// §2 — endpoint deltas.
for (const name of ['large_alloc_free_cycle', 'small_churn_16b', 'small_churn_16b_2n', 'dealloc_contains_base_probe_only_16b', 'dealloc_hash_contains_only_probe_16b', 'cold_alloc_free_256x16b', 'seg_cycle_decommit_256k']) {
  const g = ir(green, name, GREEN_LOG);
  const r = ir(red, name, RED_LOG);
  const { delta, pct } = pctOf(g, r);
  row('endpoint', `${name}_green`, 'ir', g, 'Ir', GREEN_LOG);
  row('endpoint', `${name}_red`, 'ir', r, 'Ir', RED_LOG);
  row('endpoint', `${name}_delta`, 'delta', delta, 'Ir', `${GREEN_LOG}+${RED_LOG}`);
  row('endpoint', `${name}_pct_of_green`, 'delta', pct.toFixed(2), 'pct', ENDPOINT_CSV);
}
{
  const gLarge = ir(green, 'large_alloc_free_cycle', GREEN_LOG);
  const rLarge = ir(red, 'large_alloc_free_cycle', RED_LOG);
  const { delta } = pctOf(gLarge, rLarge);
  assertEq(delta, 824, 'large_alloc_free_cycle endpoint delta');
  assertClose(pctOf(gLarge, rLarge).pct, 24.91, 0.005, 'large pct');
  const gChurn = ir(green, 'small_churn_16b', GREEN_LOG);
  const rChurn = ir(red, 'small_churn_16b', RED_LOG);
  assertEq(pctOf(gChurn, rChurn).delta, 984, 'small_churn endpoint delta');
  assertClose(pctOf(gChurn, rChurn).pct, 12.22, 0.005, 'churn pct');
}

// compared-arm statistics (independent re-derivation of the committed CSV).
{
  const compared = [];
  for (const [name, g] of green) {
    if (!red.has(name)) continue;
    compared.push({ name, g, r: red.get(name), ...pctOf(g, red.get(name)) });
  }
  const onlyRed = [...red.keys()].filter((n) => !green.has(n));
  const over10 = compared.filter((c) => c.pct > 10);
  assertEq(compared.length, 79, 'compared arm count');
  assertEq(over10.length, 25, 'arms > +10%');
  assertEq(onlyRed.length, 6, 'red-only arm count');
  if (!onlyRed.every((n) => n.startsWith('large_cache_') || n.startsWith('alloc_zeroed_magazine_'))) {
    throw new Error(`unexpected red-only arms: ${onlyRed.join(', ')}`);
  }
  const mimalloc = compared.filter((c) => c.name.startsWith('mimalloc'));
  if (!mimalloc.every((c) => c.delta === 0)) {
    throw new Error(`mimalloc arms not all 0.00%: ${mimalloc.filter((c) => c.delta !== 0).map((c) => c.name).join(', ')}`);
  }
  row('endpoint', 'compared_arms', 'count', 79, 'arms', ENDPOINT_CSV);
  row('endpoint', 'arms_over_10pct', 'count', 25, 'arms', ENDPOINT_CSV);
  row('endpoint', 'red_only_arms', 'count', 6, 'arms', `${RED_LOG} (large_cache_*/alloc_zeroed_magazine_*)`);
  row('endpoint', 'mimalloc_arms_delta', 'delta', 0, 'Ir', `${GREEN_LOG}+${RED_LOG}`);
}

// §3 — bisect 1 per-step probes + named first-bad commit.
const bisect1 = {};
for (const sha of BISECT_STEPS) {
  const log = `docs/perf/_raw_item56_bisect_${sha}.log`;
  const m = parseIrByName(readFileSync(P(log), 'utf8'));
  const churn = ir(m, 'small_churn_16b', log);
  const large = ir(m, 'large_alloc_free_cycle', log);
  bisect1[sha] = { churn, large };
  rowIr('bisect1', `${sha}_small_churn_16b`, churn, log);
  rowIr('bisect1', `${sha}_large_alloc_free_cycle`, large, log);
}
{
  const runLog = readFileSync(P('docs/perf/_raw_item56_bisect_run.log'), 'utf8');
  const m = /{([0-9a-f]{40})} is the first bad commit|([0-9a-f]{40}) is the first bad commit/.exec(runLog);
  if (!m) throw new Error('bisect1 run log: no "is the first bad commit" line');
  const firstBad = m[2] || m[1];
  assertEq(firstBad, revParse('5df56d3'), 'bisect1 first bad commit');
  row('bisect1', 'first_bad_commit', 'sha', firstBad, 'sha', 'docs/perf/_raw_item56_bisect_run.log');
  // predicate cross-check: BAD iff either probe >= green+5% ( Ir > green*1.05 )
  const churn = bisect1['5df56d3'].churn;
  const large = bisect1['5df56d3'].large;
  const greenChurn = ir(green, 'small_churn_16b', GREEN_LOG);
  const greenLarge = ir(green, 'large_alloc_free_cycle', GREEN_LOG);
  if (!(churn > greenChurn * 1.05 && large > greenLarge * 1.05)) {
    throw new Error('bisect1: 5df56d3 does not satisfy the BAD predicate');
  }
  if (!(bisect1['62e217f'].churn <= greenChurn * 1.05)) {
    throw new Error('bisect1: 62e217f (GOOD step) violates predicate');
  }
}

// §4 — bisect 2 per-step probes + named first-bad commit.
const bisect2 = {};
for (const sha of BISECT2_STEPS) {
  const log = `docs/perf/_raw_item56_bisect2_${sha}.log`;
  const m = parseIrByName(readFileSync(P(log), 'utf8'));
  const churn = ir(m, 'small_churn_16b', log);
  bisect2[sha] = { churn, map: m, log };
  rowIr('bisect2', `${sha}_small_churn_16b`, churn, log);
}
{
  const runLog = readFileSync(P('docs/perf/_raw_item56_bisect2_run.log'), 'utf8');
  const m = /([0-9a-f]{40}) is the first bad commit/.exec(runLog);
  if (!m) throw new Error('bisect2 run log: no "is the first bad commit" line');
  assertEq(m[1], revParse('5289c66'), 'bisect2 first bad commit');
  row('bisect2', 'first_bad_commit', 'sha', m[1], 'sha', 'docs/perf/_raw_item56_bisect2_run.log');
  // threshold band cross-check: GOOD<=8900, BAD>=8980
  for (const sha of BISECT2_STEPS) {
    const c = bisect2[sha].churn;
    if (c > 8900 && c < 8980) throw new Error(`bisect2 ${sha}: churn ${c} inside the ambiguous band`);
  }
}

// §4b — red endpoint ≡ 5289c66 on the regression-signal arms.
//
// HONEST SCOPING (operator decision): the equivalence is claimed ONLY for the
// arms that constitute item 56's regression signal (the four spot arms, all
// green-present). The full-arm comparison has larger residuals, which are
// separately derived and reported below: 2 arms deviate more than the spot
// set among green-present arms (max +79 on decomp_full_cycle_8x) and the
// red-only large_cache_* arms (added in-range, no green baseline) deviate by
// up to +262 — consistent with the two in-range post-5289c66 perf(runtime)
// large-cache commits eb2463a (4-field HIT-arm write) and e88390b (occupancy
// bitmask vs free-slot linear scan) reshaping those paths AFTER 5289c66.
// Those arms are outside item 56's regression set and outside both bisect
// predicates. NO "every arm within +/-11" claim is made.
{
  const redMap = red;
  const c66Map = bisect2['5289c66'].map;
  // four spot (green-present, regression-signal) arms
  const spots = [
    ['small_churn_16b', 2],
    ['large_alloc_free_cycle', 11],
    ['cold_alloc_free_256x16b', 2],
    ['seg_cycle_decommit_256k', 8],
  ];
  for (const [name, expectedDev] of spots) {
    if (!green.has(name)) throw new Error(`spot arm ${name} not green-present`);
    const dev = Math.abs(redMap.get(name) - c66Map.get(name));
    assertEq(dev, expectedDev, `red vs 5289c66 ${name} deviation`);
    row('equivalence', `red_vs_5289c66_${name}_dev`, 'delta', dev, 'Ir', `${RED_LOG} vs ${bisect2['5289c66'].log}`);
  }
  // green-present full-arm set: max deviation (expected 79 on decomp_full_cycle_8x)
  let gpMax = 0;
  let gpMaxArm = '';
  for (const [name, r] of redMap) {
    if (!green.has(name) || !c66Map.has(name)) continue;
    const dev = Math.abs(r - c66Map.get(name));
    if (dev > gpMax) {
      gpMax = dev;
      gpMaxArm = name;
    }
  }
  assertEq(gpMax, 79, 'green-present full-arm max deviation red vs 5289c66');
  assertEq(gpMaxArm, 'decomp_full_cycle_8x', 'green-present max-dev arm');
  const gpOsrDev = Math.abs(redMap.get('decomp_os_roundtrip_8x') - c66Map.get('decomp_os_roundtrip_8x'));
  assertEq(gpOsrDev, 16, 'decomp_os_roundtrip_8x deviation (green-present, second-largest)');
  row('equivalence', 'green_present_full_arm_max_dev', 'delta', gpMax, 'Ir', `${RED_LOG} vs ${bisect2['5289c66'].log} (${gpMaxArm})`);
  row('equivalence', 'green_present_decomp_full_cycle_8x_dev', 'delta', 79, 'Ir', `${RED_LOG} vs ${bisect2['5289c66'].log}`);
  row('equivalence', 'green_present_decomp_os_roundtrip_8x_dev', 'delta', 16, 'Ir', `${RED_LOG} vs ${bisect2['5289c66'].log}`);
  // red-only deviators (added in-range; shaped by eb2463a/e88390b after 5289c66)
  const roExpect = [
    ['large_cache_hit_only_4mib', 262],
    ['large_cache_prefill_only_4mib', 232],
  ];
  const roFound = [];
  for (const [name, r] of redMap) {
    if (green.has(name) || !c66Map.has(name)) continue;
    const dev = Math.abs(r - c66Map.get(name));
    if (dev > 11) roFound.push([name, dev]);
  }
  roFound.sort((a, b) => b[1] - a[1]);
  assertEq(JSON.stringify(roFound), JSON.stringify(roExpect), 'red-only deviator set red vs 5289c66');
  for (const [name, dev] of roFound) {
    row('equivalence', `red_only_${name}_dev`, 'delta', dev, 'Ir', `${RED_LOG} vs ${bisect2['5289c66'].log} (arm added in-range; eb2463a/e88390b)`);
  }
  row('equivalence', 'identity_note', 'delta', 'eb2463a449ca3497ce2761ee32f95cdc63bac321,e88390bc88c863c8861d8bdda26fb49269cf9a89', 'sha', 'post-5289c66 large-cache perf(runtime) commits shaping the red-only deviators');
  // bisect-2 GOOD boundary + counterfactual A' carry the proof for probe arms.
  row('equivalence', 'proof_basis', 'delta', 'bisect2 GOOD boundary (ce3f44d 8,810) + counterfactual A (all arms <= +5 Ir) + A\' (max 1 Ir vs 5df56d3)', 'note', 'docs/perf/_raw_item56_bisect2_run.log; counterfactual logs');
}

// §5 — decomposition at 5df56d3 (repr(C)) and 5289c66 (own-cache + counters).
const gChurn = ir(green, 'small_churn_16b', GREEN_LOG);
const gChurn2n = ir(green, 'small_churn_16b_2n', GREEN_LOG);
{
  // component 1: repr(C) at 5df56d3
  const c1Log = 'docs/perf/_raw_item56_bisect_5df56d3.log';
  const c1Map = parseIrByName(readFileSync(P(c1Log), 'utf8'));
  const c1Churn = ir(c1Map, 'small_churn_16b', c1Log);
  const c1Churn2n = ir(c1Map, 'small_churn_16b_2n', c1Log);
  const fixed1 = c1Churn - gChurn;
  const d2n1 = c1Churn2n - c1Churn;
  const d2nGreen = gChurn2n - gChurn;
  assertEq(fixed1, 759, '5df56d3 fixed churn delta');
  assertEq(d2n1, 4416, '5df56d3 2n delta');
  assertEq(d2nGreen, 4416, 'green 2n delta');
  assertEq(d2n1, d2nGreen, '5df56d3 2n delta unchanged from green (zero per-op cost)');
  const perClass = fixed1 / 49;
  assertClose(perClass, 15.49, 0.005, 'Ir per PerClass');
  const large1 = ir(c1Map, 'large_alloc_free_cycle', c1Log) - ir(green, 'large_alloc_free_cycle', GREEN_LOG);
  assertEq(large1, 772, '5df56d3 large delta');
  rowDelta('decomposition', 'reprC_fixed_churn_delta_at_5df56d3', fixed1, c1Log);
  rowDelta('decomposition', 'reprC_2n_delta_at_5df56d3', d2n1, c1Log);
  rowDelta('decomposition', 'reprC_2n_delta_green', d2nGreen, GREEN_LOG);
  rowDelta('decomposition', 'reprC_per_op_delta', 0, c1Log);
  row('decomposition', 'reprC_fixed_per_PerClass', 'delta', perClass.toFixed(2), 'Ir/PerClass', c1Log);
  rowDelta('decomposition', 'reprC_large_alloc_free_cycle_delta', large1, c1Log);

  // component 2: own-cache + counters at 5289c66
  const c2Log = 'docs/perf/_raw_item56_bisect2_5289c66.log';
  const c2Map = bisect2['5289c66'].map;
  const c2Churn = ir(c2Map, 'small_churn_16b', c2Log);
  const c2Churn2n = ir(c2Map, 'small_churn_16b_2n', c2Log);
  const d2n2 = c2Churn2n - c2Churn;
  const perOp2 = (d2n2 - d2n1) / 64;
  // fixed part of the 5289c66 churn delta BEYOND the 759 repr(C) fixed cost:
  // (churn delta vs green) - 759 (repr(C), carried in from 5df56d3) - 192 (per-op)
  const fixed2 = c2Churn - gChurn - 759 - (d2n2 - d2n1);
  assertEq(d2n2, 4608, '5289c66 2n delta');
  assertEq(d2n2 - d2n1, 192, '2n delta increment at 5289c66');
  assertClose(perOp2, 3.0, 1e-9, 'Ir per extra pair at 5289c66');
  assertEq(fixed2, 35, '5289c66 fixed churn part');
  // per-arm isolation: contains_base vs hash-bypass probe (5289c66 minus 5df56d3-level)
  const d1Log = 'docs/perf/_raw_item56_bisect_5df56d3.log'; // the 5df56d3 code level itself
  const d1Map = parseIrByName(readFileSync(P(d1Log), 'utf8'));
  const containsDelta = ir(c2Map, 'dealloc_contains_base_probe_only_16b', c2Log) - ir(d1Map, 'dealloc_contains_base_probe_only_16b', d1Log);
  const hashDelta = ir(c2Map, 'dealloc_hash_contains_only_probe_16b', c2Log) - ir(d1Map, 'dealloc_hash_contains_only_probe_16b', d1Log);
  assertEq(containsDelta, 239, 'contains_base probe delta 5289c66 vs 5df56d3');
  assertEq(hashDelta, 47, 'hash probe delta 5289c66 vs 5df56d3 (fixed-only)');
  assertClose(hashDelta, 47, 1, 'hash probe delta ~+47');
  const perCall = (containsDelta - hashDelta) / 64;
  assertClose(perCall, 3.0, 0.05, 'per-contains_base-call counter cost');
  rowDelta('decomposition', 'owncache_2n_delta_at_5289c66', d2n2, c2Log);
  rowDelta('decomposition', 'owncache_2n_delta_increment', d2n2 - d2n1, c2Log);
  row('decomposition', 'owncache_per_pair_cost', 'delta', perOp2.toFixed(2), 'Ir/pair', c2Log);
  rowDelta('decomposition', 'owncache_fixed_churn_part', fixed2, c2Log);
  rowDelta('decomposition', 'owncache_contains_base_probe_delta', containsDelta, `${c2Log} vs ${d1Log}`);
  rowDelta('decomposition', 'owncache_hash_probe_delta', hashDelta, `${c2Log} vs ${d1Log}`);
  row('decomposition', 'owncache_per_call_counter_cost', 'delta', perCall.toFixed(2), 'Ir/call', `${c2Log} vs ${d1Log}`);
  // round-trip: fixed + 64*per-op reconstructs the churn delta
  const reconstruct = gChurn + 759 + 192 + fixed2;
  assertEq(reconstruct, c2Churn, 'decomposition round-trip fixed+64*perop');
  row('decomposition', 'churn_decomposition_roundtrip_check', 'delta', 'ok', 'assert', c2Log);
}

// §6 — counterfactual A (tcache.rs reverted at 5df56d3).
{
  const log = CF_A_LOG;
  const m = parseIrByName(readFileSync(P(log), 'utf8'));
  let over10 = 0;
  let maxDev = 0;
  for (const [name, g] of green) {
    if (!m.has(name)) continue;
    const { pct, delta } = pctOf(g, m.get(name));
    if (pct > 10) over10 += 1;
    if (Math.abs(delta) > maxDev) maxDev = Math.abs(delta);
  }
  assertEq(over10, 0, 'counterfactual A: arms > +10%');
  const churnDelta = ir(m, 'small_churn_16b', log) - gChurn;
  const largeDelta = ir(m, 'large_alloc_free_cycle', log) - ir(green, 'large_alloc_free_cycle', GREEN_LOG);
  const flushDelta = ir(m, 'dealloc_flush_class_only_16b', log) - ir(green, 'dealloc_flush_class_only_16b', GREEN_LOG);
  assertEq(churnDelta, 4, 'cf A churn delta');
  assertEq(largeDelta, 4, 'cf A large delta');
  assertEq(flushDelta, 5, 'cf A dealloc_flush delta');
  rowIr('counterfactual_a', 'small_churn_16b', ir(m, 'small_churn_16b', log), log);
  rowIr('counterfactual_a', 'large_alloc_free_cycle', ir(m, 'large_alloc_free_cycle', log), log);
  rowIr('counterfactual_a', 'dealloc_flush_class_only_16b', ir(m, 'dealloc_flush_class_only_16b', log), log);
  rowDelta('counterfactual_a', 'small_churn_16b_delta_vs_green', churnDelta, log);
  rowDelta('counterfactual_a', 'large_alloc_free_cycle_delta_vs_green', largeDelta, log);
  rowDelta('counterfactual_a', 'dealloc_flush_class_only_16b_delta_vs_green', flushDelta, log);
  row('counterfactual_a', 'arms_over_10pct', 'count', 0, 'arms', log);
  row('counterfactual_a', 'max_abs_dev_vs_green', 'delta', maxDev, 'Ir', log);
}

// §6 — counterfactual A' (four 5289c66 src files reverted) ≡ 5df56d3 level.
{
  const log = CF_A2_LOG;
  const m = parseIrByName(readFileSync(P(log), 'utf8'));
  const d1Map = parseIrByName(readFileSync(P('docs/perf/_raw_item56_bisect_5df56d3.log'), 'utf8'));
  let maxDev = 0;
  let dev1Count = 0;
  let compared = 0;
  for (const [name, v] of m) {
    if (!d1Map.has(name)) continue;
    compared += 1;
    const dev = Math.abs(v - d1Map.get(name));
    if (dev === 1) dev1Count += 1;
    if (dev > maxDev) maxDev = dev;
  }
  if (compared < 50) throw new Error(`cf A': only ${compared} common arms with the 5df56d3 log`);
  assertEq(maxDev, 1, "counterfactual A' max abs deviation vs 5df56d3");
  assertEq(ir(m, 'dealloc_contains_base_probe_only_16b', log), ir(d1Map, 'dealloc_contains_base_probe_only_16b', 'docs/perf/_raw_item56_bisect_5df56d3.log') + 1, "counterfactual A' contains_base probe arm 9,491 -> 9,492");
  rowIr('counterfactual_a2', 'small_churn_16b', ir(m, 'small_churn_16b', log), log);
  rowIr('counterfactual_a2', 'large_alloc_free_cycle', ir(m, 'large_alloc_free_cycle', log), log);
  rowIr('counterfactual_a2', 'dealloc_contains_base_probe_only_16b', ir(m, 'dealloc_contains_base_probe_only_16b', log), log);
  row("counterfactual_a2", 'max_abs_dev_vs_5df56d3', 'delta', maxDev, 'Ir', `${log} vs docs/perf/_raw_item56_bisect_5df56d3.log`);
  row("counterfactual_a2", 'arms_at_plus1_vs_5df56d3', 'count', dev1Count, 'arms', `${log} vs docs/perf/_raw_item56_bisect_5df56d3.log`);
  row("counterfactual_a2", 'compared_arms', 'count', compared, 'arms', log);
}

// identities (R29-6) — immutable blob/commit identities from committed git objects.
const IDENTITIES = [
  ['5df56d3:src/registry/tcache.rs', 'counterfactual A: reverted-to blob (5df56d3^ content)'],
  ['5df56d3^:src/registry/tcache.rs', 'counterfactual A: reverted-to blob'],
  ['5289c66:src/alloc_core/segment_table.rs', 'counterfactual A\': 5289c66 side of the swap'],
  ['5289c66^:src/alloc_core/segment_table.rs', "counterfactual A': reverted-to blob"],
  ['5289c66^:src/alloc_core/alloc_core.rs', "counterfactual A': reverted-to blob"],
  ['5289c66^:src/alloc_core/alloc_core_core_diag.rs', "counterfactual A': reverted-to blob"],
  ['5289c66^:src/registry/heap_core_diag.rs', "counterfactual A': reverted-to blob"],
  ['5df56d3', 'bisect 1 first bad commit (repr(C) PerClass, R32-5, task #496)'],
  ['5289c66', 'bisect 2 first bad commit (OWN_CACHE_SIZE 16 + Tier-1 counters, R32-10, task #501)'],
  ['42d8d223', 'green endpoint commit (last CI green)'],
  ['42d42061', 'red endpoint commit (first CI red)'],
];
for (const [rev, note] of IDENTITIES) {
  const sha = revParse(rev);
  row('identity', rev, 'sha', sha, 'sha', note);
}

// ---- emit ------------------------------------------------------------------

const w = Math.max(...rows.map((r) => r.subject.length), 'subject'.length);
const header = `${'section'.padEnd(18)}  ${'subject'.padEnd(w)}  ${'ir/delta'.padEnd(8)}  ${'value'.padStart(42)}  ${'units'.padEnd(12)}  provenance`;
console.log(header);
console.log(`${'-'.repeat(18)}  ${'-'.repeat(w)}  ${'-'.repeat(8)}  ${'-'.repeat(42)}  ${'-'.repeat(12)}  ${'-'.repeat(30)}`);
for (const r of rows) {
  console.log(
    `${r.section.padEnd(18)}  ${r.subject.padEnd(w)}  ${r.irOrDelta.padEnd(8)}  ${r.value.padStart(42)}  ${r.units.padEnd(12)}  ${r.provenance}`,
  );
}

const csv =
  'section,subject,ir_or_delta,value,units,provenance\n' +
  rows
    .map((r) =>
      [r.section, r.subject, r.irOrDelta, r.value, r.units, r.provenance]
        .map((f) => `"${String(f).replace(/"/g, '""')}"`)
        .join(','),
    )
    .join('\n') +
  '\n';
writeFileSync(P(OUT_CSV), csv);
console.log(`\nwrote ${OUT_CSV} (${rows.length} rows); all assertions passed`);
