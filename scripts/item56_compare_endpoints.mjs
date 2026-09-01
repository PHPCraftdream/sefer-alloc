// Compare two raw iai-callgrind logs (as produced by scripts/iai.mjs) at the
// instruction-count (Ir) level and emit a checked comparison table + CSV.
//
// Usage:
//   node scripts/item56_compare_endpoints.mjs <greenLog> <redLog> <outCsv>
//
// Parses the summary table at the bottom of a raw log (name + 6 numeric
// columns with US thousands separators) and diffs green vs red Ir per bench.
// All arithmetic is round-trip asserted; benches present in only one log are
// reported, never silently dropped. Exit 0 on success, non-zero on any
// parse/arith failure.

/**
 * Parse "name -> Ir" from a raw iai log's summary table.
 *
 * Row shape: optional whitespace, bench name (no spaces), then 6 numeric
 * columns (Ir, L1, L2, RAM, EstCycles, Ir/op where Ir/op may be `-`).
 *
 * Disambiguation: the same log ALSO contains a ratio table further down whose
 * rows start with a bench name too, e.g.
 *   `  small_churn_16b                     74.1          55.9         1.326`
 * To keep only real summary rows we require the SECOND field to be a pure
 * integer (optional commas) — that excludes ratio rows (decimal second field),
 * the header/separator lines, and footnote lines. First occurrence per bench
 * name wins (de-dupe).
 */
function parseIrByName(logText) {
  // The final Ir/op column may be a decimal (e.g. 74.1) or "-".
  const re = /^\s*(\S+)\s+([0-9][0-9,]*)\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+[\d,]+\s+(?:[0-9][0-9,.]*|-)\s*$/;
  const byName = new Map();
  for (const line of logText.split(/\r?\n/)) {
    const m = re.exec(line);
    if (!m) continue;
    const name = m[1];
    if (!byName.has(name)) {
      byName.set(name, Number(m[2].replace(/,/g, '')));
    }
  }
  return byName;
}

/**
 * pct = (red - green) / green * 100, with a round-trip assert so a bad
 * pct/green/delta combination can never silently propagate into the report.
 * Requires greenIr > 0.
 */
function pctOf(greenIr, redIr) {
  if (!(greenIr > 0)) throw new Error(`greenIr must be > 0, got ${greenIr}`);
  const delta = redIr - greenIr;
  const pct = (delta / greenIr) * 100;
  const roundTrip = (pct * greenIr) / 100;
  if (Math.abs(roundTrip - delta) >= 1e-6) {
    throw new Error(
      `pct round-trip failed: green=${greenIr} red=${redIr} delta=${delta} pct=${pct} roundTrip=${roundTrip}`,
    );
  }
  return { delta, pct };
}

import { readFileSync, writeFileSync } from 'node:fs';

function main() {
  const [greenLog, redLog, outCsv] = process.argv.slice(2);
  if (!greenLog || !redLog || !outCsv) {
    console.error(
      'usage: node scripts/item56_compare_endpoints.mjs <greenLog> <redLog> <outCsv>',
    );
    process.exit(2);
  }
  const green = parseIrByName(readFileSync(greenLog, 'utf8'));
  const red = parseIrByName(readFileSync(redLog, 'utf8'));

  const rows = [];
  for (const [name, greenIr] of green) {
    if (!red.has(name)) continue;
    const { delta, pct } = pctOf(greenIr, red.get(name));
    rows.push({ name, greenIr, redIr: red.get(name), delta, pct });
  }
  rows.sort((a, b) => b.pct - a.pct);

  const onlyGreen = [...green].filter(([n]) => !red.has(n));
  const onlyRed = [...red].filter(([n]) => !green.has(n));
  for (const [name, ir] of onlyGreen) console.error(`[only-green] ${name} ir=${ir}`);
  for (const [name, ir] of onlyRed) console.error(`[only-red] ${name} ir=${ir}`);

  const w = Math.max(...rows.map((r) => r.name.length), 'bench'.length);
  console.log(
    `${'bench'.padEnd(w)}  ${'greenIr'.padStart(10)}  ${'redIr'.padStart(10)}  ${'delta'.padStart(10)}  ${'pct'.padStart(8)}`,
  );
  console.log(`${'-'.repeat(w)}  ${'-'.repeat(10)}  ${'-'.repeat(10)}  ${'-'.repeat(10)}  ${'-'.repeat(8)}`);
  for (const r of rows) {
    console.log(
      `${r.name.padEnd(w)}  ${String(r.greenIr).padStart(10)}  ${String(r.redIr).padStart(10)}  ${String(r.delta).padStart(10)}  ${r.pct.toFixed(2).padStart(8)}`,
    );
  }

  const over = rows.filter((r) => r.pct > 10).length;
  const under = rows.filter((r) => r.pct < -10).length;
  console.log(`summary: >+10%: ${over}  <-10%: ${under}`);
  for (const name of [
    'large_alloc_free_cycle',
    'small_churn_16b',
    'mimalloc_bootstrap_proxy',
    'mimalloc_small_churn_16b',
  ]) {
    const r = rows.find((x) => x.name === name);
    if (!r) {
      console.log(`${name}: (not present in both logs)`);
    } else {
      console.log(
        `${name}: green=${r.greenIr} red=${r.redIr} delta=${r.delta} pct=${r.pct.toFixed(2)}`,
      );
    }
  }

  const csv =
    'bench,green_ir,red_ir,delta_ir,pct_of_green\n' +
    rows
      .map(
        (r) =>
          `${r.name},${r.greenIr},${r.redIr},${r.delta},${r.pct.toFixed(2)}`,
      )
      .join('\n') +
    '\n';
  writeFileSync(outCsv, csv);
}

main();
