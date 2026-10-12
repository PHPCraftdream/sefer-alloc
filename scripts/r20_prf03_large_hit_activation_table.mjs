// Derived summary table for the PRF-03 Large-cache payload-change probe
// activation run (task r20).
//
// Reads docs/perf/_raw_r20_prf03_large_hit_activation.log, parses every
// JSON line with a "workload" AND "large_requests" + "hits" key (the
// per-workload probe-delta lines and the TOTAL aggregate). The size_sweep
// detail lines ({"workload": "size_sweep", "size": ...}) have no "hits"
// key and are skipped: they are a boundary check, not hit-rate data.
//
// Asserts per parsed line:
//   large_requests === hits + misses
//   changed_payload_hits <= hits
//   all values are non-negative integers
//
// Prints a table whose percentage columns state their numerator and
// denominator, and writes the same rows as CSV to
// docs/perf/R20_PRF03_LARGE_HIT_ACTIVATION_summary.csv.
//
// Usage: node scripts/r20_prf03_large_hit_activation_table.mjs

import { REPO_ROOT } from './lib.mjs';
import { readFileSync, writeFileSync } from 'node:fs';
import { join, basename } from 'node:path';
import assert from 'node:assert';

const LOG = join(REPO_ROOT, 'docs', 'perf', '_raw_r20_prf03_large_hit_activation.log');
const CSV = join(REPO_ROOT, 'docs', 'perf', 'R20_PRF03_LARGE_HIT_ACTIVATION_summary.csv');

const text = readFileSync(LOG, 'utf8');
const rows = [];
for (const line of text.split(/\r?\n/)) {
  const t = line.trim();
  if (!t.startsWith('{') || !t.includes('"workload"') || !t.includes('"hits"')) continue;
  const o = JSON.parse(t);
  for (const k of ['large_requests', 'hits', 'misses', 'changed_payload_hits']) {
    assert(Number.isInteger(o[k]) && o[k] >= 0, `${o.workload}: ${k} must be a non-negative integer, got ${o[k]}`);
  }
  assert.strictEqual(
    o.large_requests, o.hits + o.misses,
    `${o.workload}: large_requests (${o.large_requests}) != hits + misses (${o.hits} + ${o.misses})`,
  );
  assert(o.changed_payload_hits <= o.hits, `${o.workload}: changed_payload_hits > hits`);
  rows.push(o);
}
assert(rows.length >= 2, `expected at least 2 data lines in ${basename(LOG)}, got ${rows.length}`);
const total = rows.find((r) => r.workload === 'TOTAL');
assert(total, 'missing TOTAL line');

const pct = (n, d) => (d === 0 ? 'n/a' : ((100 * n) / d).toFixed(2));

const header = 'workload,large_requests,hits,misses,changed_payload_hits,hit_pct_of_requests,changed_pct_of_hits,changed_pct_of_requests';
const csvLines = [header];
for (const r of rows) {
  csvLines.push([
    r.workload, r.large_requests, r.hits, r.misses, r.changed_payload_hits,
    pct(r.hits, r.large_requests), pct(r.changed_payload_hits, r.hits), pct(r.changed_payload_hits, r.large_requests),
  ].join(','));
}
writeFileSync(CSV, csvLines.join('\n') + '\n');

const w = [10, 14, 8, 8, 21, 22, 20, 24];
const fmt = (cells) => cells.map((c, i) => String(c).padStart(w[i])).join(' ');
console.log('PRF-03 Large-cache payload-change probe — hit-rate table');
console.log('source: docs/perf/_raw_r20_prf03_large_hit_activation.log');
console.log(fmt(['workload', 'requests', 'hits', 'misses', 'changed_payload_hits',
  'hit-rate % (a)', 'changed % hits (b)', 'changed % reqs (c)']));
console.log(fmt(['-'.repeat(w[0]), '-'.repeat(w[1]), '-'.repeat(w[2]), '-'.repeat(w[3]), '-'.repeat(w[4]), '-'.repeat(w[5]), '-'.repeat(w[6]), '-'.repeat(w[7])]));
for (const r of rows) {
  console.log(fmt([r.workload, r.large_requests, r.hits, r.misses, r.changed_payload_hits,
    pct(r.hits, r.large_requests), pct(r.changed_payload_hits, r.hits), pct(r.changed_payload_hits, r.large_requests)]));
}
console.log('\n(a) numerator = hits, denominator = large_requests');
console.log('(b) numerator = changed_payload_hits, denominator = hits');
console.log('(c) numerator = changed_payload_hits, denominator = large_requests');
console.log(`wrote ${basename(CSV)} (${rows.length} data rows)`);
