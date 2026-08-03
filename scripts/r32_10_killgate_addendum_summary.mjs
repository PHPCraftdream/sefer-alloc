#!/usr/bin/env node
// R32-10 addendum (task #501 follow-up correction): derives and asserts the
// standing +-10 raw-Ir churn kill-gate numbers the original R32-10 report
// (docs/perf/R32_10_OWN_CACHE_TIER1_THRASH_GATE.md, section 5.1) said were
// "argued but not measured, no Linux/Valgrind on this dev host" -- WSL was
// in fact available and the numbers below were obtained from it.
//
// Reads three raw iai-callgrind logs (base = OWN_CACHE_SIZE 4, no counter;
// isolate = OWN_CACHE_SIZE 16, counter temporarily commented out as a
// scratch (uncommitted) edit; head = OWN_CACHE_SIZE 16 + counter, i.e. the
// actual shipped state) and asserts:
//   1. base -> isolate delta isolates the OWN_CACHE_SIZE-alone cost.
//   2. isolate -> head delta isolates the new bench-internals-only counter's
//      cost (never ships in a plain `production` build).
//   3. base -> head is the number the standing kill-gate convention would
//      have reported, for the record.

import { readFileSync, writeFileSync } from 'node:fs';
import { execSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const perfDir = path.join(__dirname, '..', 'docs', 'perf');

const BENCHES = [
  'small_churn_16b',
  'aligned_churn_640b_a128',
  'cold_alloc_free_256x16b',
  'recycle_alloc_free_256x16b',
  'churn_256b',
];

function parseInstructions(logPath) {
  const text = readFileSync(logPath, 'utf8');
  const lines = text.split('\n');
  const out = {};
  for (let i = 0; i < lines.length; i++) {
    for (const bench of BENCHES) {
      if (lines[i].trim() === `perf_gate_iai::perf_gate::${bench}`) {
        const instrLine = lines[i + 1];
        const m = instrLine.match(/Instructions:\s+(\d+)/);
        if (!m) throw new Error(`could not parse Instructions line for ${bench} in ${logPath}: ${instrLine}`);
        out[bench] = Number(m[1]);
      }
    }
  }
  for (const bench of BENCHES) {
    if (!(bench in out)) throw new Error(`bench ${bench} not found in ${logPath}`);
  }
  return out;
}

const base = parseInstructions(path.join(perfDir, '_raw_r32_10_killgate_cache4_nocounter.log'));
const isolate = parseInstructions(path.join(perfDir, '_raw_r32_10_killgate_cache16_nocounter.log'));
const head = parseInstructions(path.join(perfDir, '_raw_r32_10_killgate_cache16_withcounter.log'));

const rows = BENCHES.map((bench) => {
  const b = base[bench];
  const iso = isolate[bench];
  const h = head[bench];
  const deltaCacheSizeAlone = iso - b;
  const deltaCounterAlone = h - iso;
  const deltaTotal = h - b;
  return { bench, base: b, isolate: iso, head: h, deltaCacheSizeAlone, deltaCounterAlone, deltaTotal };
});

// Assert the headline claims this addendum makes, not just print them.

// 1. The OWN_CACHE_SIZE-alone delta must be small and roughly constant
//    across the flat single-op benches (small_churn_16b, aligned_churn,
//    churn_256b) -- consistent with a ONE-TIME per-heap-construction
//    zero-init cost (bigger own_cache array), not a per-op cost. "Roughly
//    constant" is asserted as: all three flat benches within 5 Ir of each
//    other.
const flatBenches = ['small_churn_16b', 'aligned_churn_640b_a128', 'churn_256b'];
const flatDeltas = rows.filter((r) => flatBenches.includes(r.bench)).map((r) => r.deltaCacheSizeAlone);
const flatMin = Math.min(...flatDeltas);
const flatMax = Math.max(...flatDeltas);
if (flatMax - flatMin > 5) {
  throw new Error(
    `OWN_CACHE_SIZE-alone delta is NOT roughly constant across flat churn benches (min=${flatMin}, max=${flatMax}) -- the one-time-bootstrap-cost hypothesis does not hold, do not publish that claim`
  );
}
// Must be small in absolute terms (well under 100 Ir) to support "negligible
// one-time cost" framing.
for (const r of rows) {
  if (Math.abs(r.deltaCacheSizeAlone) > 100) {
    throw new Error(`OWN_CACHE_SIZE-alone delta for ${r.bench} is ${r.deltaCacheSizeAlone}, not small -- re-examine before publishing "negligible" framing`);
  }
}

// 2. The counter-alone delta must scale with the benches' known relative
//    op counts: cold_alloc_free_256x16b and recycle_alloc_free_256x16b (both
//    256-plus-iteration benches) must show a materially LARGER counter delta
//    than the flat single-op-shaped benches -- consistent with "the counter
//    adds a small fixed cost PER contains_base call", not a one-time cost.
const flatCounterDeltas = rows.filter((r) => flatBenches.includes(r.bench)).map((r) => r.deltaCounterAlone);
const coldCounterDelta = rows.find((r) => r.bench === 'cold_alloc_free_256x16b').deltaCounterAlone;
const recycleCounterDelta = rows.find((r) => r.bench === 'recycle_alloc_free_256x16b').deltaCounterAlone;
const flatCounterMax = Math.max(...flatCounterDeltas);
if (!(coldCounterDelta > flatCounterMax * 2 && recycleCounterDelta > flatCounterMax * 2)) {
  throw new Error(
    `counter-alone delta does not scale with op count as expected (flatMax=${flatCounterMax}, cold=${coldCounterDelta}, recycle=${recycleCounterDelta}) -- re-examine the "per-call counter cost" claim before publishing`
  );
}

// 3. Sanity: total (base -> head) delta must equal the sum of the two
//    component deltas for every bench (pure arithmetic identity, but assert
//    it anyway per CLAUDE.md's "assert the arithmetic" rule).
for (const r of rows) {
  if (r.deltaCacheSizeAlone + r.deltaCounterAlone !== r.deltaTotal) {
    throw new Error(`arithmetic mismatch for ${r.bench}: ${r.deltaCacheSizeAlone} + ${r.deltaCounterAlone} != ${r.deltaTotal}`);
  }
}

const csvHeader = 'bench,base_cache4_nocounter_ir,isolate_cache16_nocounter_ir,head_cache16_withcounter_ir,delta_cache_size_alone,delta_counter_alone,delta_total,landing_commit';
const landingCommit = process.argv[2] || execSync('git rev-parse HEAD', { encoding: 'utf8' }).trim();
const csvLines = [csvHeader, ...rows.map((r) =>
  `${r.bench},${r.base},${r.isolate},${r.head},${r.deltaCacheSizeAlone},${r.deltaCounterAlone},${r.deltaTotal},${landingCommit}`
)];
const csvPath = path.join(perfDir, 'R32_10_KILLGATE_ADDENDUM_summary.csv');
writeFileSync(csvPath, csvLines.join('\n') + '\n');

console.log('R32-10 kill-gate addendum -- derived and asserted:');
console.log('bench'.padEnd(28), 'base'.padStart(8), 'isolate'.padStart(8), 'head'.padStart(8), 'Δcache'.padStart(8), 'Δcounter'.padStart(9), 'Δtotal'.padStart(8));
for (const r of rows) {
  console.log(
    r.bench.padEnd(28),
    String(r.base).padStart(8),
    String(r.isolate).padStart(8),
    String(r.head).padStart(8),
    String(r.deltaCacheSizeAlone).padStart(8),
    String(r.deltaCounterAlone).padStart(9),
    String(r.deltaTotal).padStart(8)
  );
}
console.log(`\nAll assertions passed. Wrote ${csvPath}`);
