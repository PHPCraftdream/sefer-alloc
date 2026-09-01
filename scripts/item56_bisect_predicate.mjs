// Git-bisect predicate for the item-56 Ir regression. Run from the repo root
// at whatever commit is checked out; it rebuilds and re-measures the two
// probe benches via scripts/iai.mjs and classifies the commit:
//   exit 0   = GOOD  (neither probe's Ir exceeds its green endpoint value by
//              more than CUTOFF_PCT)
//   exit 1   = BAD   (EITHER probe's Ir >= green * (1 + CUTOFF_PCT/100))
//   exit 125 = SKIP  (build/measurement failure or probe row missing)
//
// Threshold rationale: a 5% cutoff sits well ABOVE callgrind's observed
// run-to-run determinism (byte-identical Ir on identical binaries, i.e. 0%
// noise) and well BELOW the ~10-13.8% regression signal being hunted, so any
// classification is unambiguous.
//
// Self-contained by design (bisect runs it standalone): the log parsing is
// inlined here rather than imported from item56_compare_endpoints.mjs.

import { execSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';

// Green endpoint reference values, from docs/perf/_raw_item56_endpoint_42d8d223.log
// (commit 42d8d223): large_alloc_free_cycle Ir = 3308, small_churn_16b Ir = 8051.
const GREEN_LARGE_IR = 3308;
const GREEN_SMALL_IR = 8051;
// A commit is BAD iff EITHER probe's Ir >= green * (1 + CUTOFF_PCT/100).
// 5%: well above callgrind's byte-identical determinism (0% run-to-run),
// well below the ~10-13.8% item-56 signal.
const CUTOFF_PCT = 5;

// Same parsing rules as item56_compare_endpoints.mjs: only accept rows whose
// SECOND field is a pure integer (excludes the ratio table, headers,
// separators, footnotes); first row per bench name wins (de-dupe).
function parseIrByName(logText) {
  // The final Ir/op column may be a decimal (e.g. 74.1) or "-".
  const re = /^\s*(\S+)\s+([0-9][0-9,]*)\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+(?:[0-9][0-9,.]*|-)\s*$/;
  const byName = new Map();
  for (const line of logText.split(/\r?\n/)) {
    const m = re.exec(line);
    if (!m) continue;
    if (!byName.has(m[1])) {
      byName.set(m[1], Number(m[2].replace(/,/g, '')));
    }
  }
  return byName;
}

/** pct with the same round-trip assert as item56_compare_endpoints.mjs. */
function pctOf(greenIr, ir) {
  if (!(greenIr > 0)) throw new Error(`greenIr must be > 0, got ${greenIr}`);
  const delta = ir - greenIr;
  const pct = (delta / greenIr) * 100;
  if (Math.abs((pct * greenIr) / 100 - delta) >= 1e-6) {
    throw new Error(`pct round-trip failed: green=${greenIr} ir=${ir}`);
  }
  return pct;
}

function main() {
  const shortsha = execSync('git rev-parse --short HEAD', {
    encoding: 'utf8',
  }).trim();
  const logPath = `docs/perf/_raw_item56_bisect_${shortsha}.log`;
  let out;
  let runFailed = false;
  try {
    out = execSync('node scripts/iai.mjs', {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    });
  } catch (e) {
    runFailed = true;
    out = String(e.stdout ?? '') + String(e.stderr ?? '');
  }
  writeFileSync(logPath, out);
  if (
    runFailed ||
    out.split(/\r?\n/).some((l) => /^error(\[|:)/.test(l))
  ) {
    console.log(`[bisect] iai run reported errors; log: ${logPath}`);
    process.exit(125);
  }
  const ir = parseIrByName(out);
  const large = ir.get('large_alloc_free_cycle');
  const small = ir.get('small_churn_16b');
  if (large == null || small == null) {
    console.log(`[bisect] probe rows missing (large=${large} small=${small}); log: ${logPath}`);
    process.exit(125);
  }
  const pctLarge = pctOf(GREEN_LARGE_IR, large);
  const pctSmall = pctOf(GREEN_SMALL_IR, small);
  const bad =
    large >= GREEN_LARGE_IR * (1 + CUTOFF_PCT / 100) ||
    small >= GREEN_SMALL_IR * (1 + CUTOFF_PCT / 100);
  console.log(
    `PROBE large_alloc_free_cycle ir=${large} green=${GREEN_LARGE_IR} pct=${pctLarge.toFixed(2)} | small_churn_16b ir=${small} green=${GREEN_SMALL_IR} pct=${pctSmall.toFixed(2)} -> ${bad ? 'BAD' : 'GOOD'}`,
  );
  process.exit(bad ? 1 : 0);
}

main();
