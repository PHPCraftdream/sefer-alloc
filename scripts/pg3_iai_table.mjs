#!/usr/bin/env node
// PG-3 spike "off-body NextTable" — iai A/B table builder.
//
// WHY THIS EXISTS: the PG-2 off-body-NextTable spike (commit 8a028616) must be
// judged against the PG-3 gates of docs/design/2026-10-01-adr-physical-boundary-
// and-progress.md on a DETERMINISTIC axis (callgrind Ir / Estimated Cycles),
// because wall-clock on this Windows dev host is noise. Two raw iai logs exist:
//   base  = docs/perf/_raw_pg3_iai_base.log   (pg3-base @ 7c0c796b)
//   spike = docs/perf/_raw_pg3_iai_spike.log              (pg3-spike @ 8a028616)
//
// !!! The FIRST spike run was DISCARDED as an artifact !!! `scripts/iai.mjs` drives
// cargo with the shared WSL `CARGO_TARGET_DIR=/tmp/sefer-iai`, so both worktrees
// built into one directory: "freshness" was decided by source mtime, not content.
// The spike run therefore executed the binary built from pg3-base (no `Compiling`
// line, `Finished ... in 2.91s`), which made 76/85 benches bit-identical to base
// and produced the fake Δ≈0% / fake PASS. That run was thrown away; the log kept
// at `docs/perf/_raw_pg3_iai_spike.log` is the RE-MEASUREMENT taken in a private
// `CARGO_TARGET_DIR=/tmp/sefer-iai-verify` (fresh build out of pg3-spike). It is
// recognisable by the `N|N/A` row format — a fresh baseline, nothing to diff
// against — and by the `Iai-Callgrind result: Ok ... 85 benchmarks finished in
// 28.3103s` trailer. The parser MUST accept both shapes.
//
// This script parses both, prints the A/B markdown table, writes the CSV the
// gate wants, and prints the threshold summary (hot ≤1.02x, refill ≤1.05x Est-
// Cycles, PG-3's ≥25% EstCycles regressions).
//
// iai-callgrind's stdout block per bench looks like:
//   perf_gate_iai::perf_gate::<name>
//     Instructions:        196387|196387   (No change)      <- run|baseline
//     L1 Hits: ...
//     Estimated Cycles: ...
// and, when the runner had no stored baseline (our re-measurement):
//     Instructions:         392243|N/A      (*********)     <- run|N/A
// We ALWAYS take the FIRST number of each row (the current run's absolute
// count); the second number is whatever baseline the runner happened to have,
// which is NOT the other worktree's run, so using it would silently corrupt the
// A/B. The full header line is the row key (bench names may repeat with
// suffixes — never key by the short name).
//
// The percent printer is the ONLY place a delta is formatted: it asserts the
// value is finite AND that base*(1+pct/100) reproduces spike to ±0.01%, so a
// hand-typed or copy-pasted number can never reach the table.
//
// Usage (from the pg3-spike repo root):
//   node scripts/pg3_iai_table.mjs [--cache] [--base <path>] [--spike <path>]
// `--cache` additionally prints the L1/L2/RAM table for the group-marked rows.

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// --- identity (PG-3 requires immutable SHAs + the exact feature string) ------
const SPIKE_COMMIT = '8a0286166a07c3ff35258836400d94f16c7994cd';
const FEATURES = 'production bench-internals';

const DEFAULT_BASE_LOG = join(
  REPO_ROOT,
  'docs',
  'perf',
  '_raw_pg3_iai_base.log',
);
const DEFAULT_SPIKE_LOG = join(
  REPO_ROOT,
  'docs',
  'perf',
  '_raw_pg3_iai_spike.log',
);
const CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PG3_OFFBODY_SPIKE_IAI_summary.csv');

// --- CLI ---------------------------------------------------------------------
const argv = process.argv.slice(2);
const opts = { cache: false, base: DEFAULT_BASE_LOG, spike: DEFAULT_SPIKE_LOG };
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '--cache') opts.cache = true;
  else if (argv[i] === '--base') opts.base = resolve(argv[++i]);
  else if (argv[i] === '--spike') opts.spike = resolve(argv[++i]);
  else throw new Error(`unknown argument: ${argv[i]}`);
}

// --- group definitions (ADR §Гейты + the perf-gate bench families) -----------
// Matched against the SHORT bench name. `prefix:` entries are anchored at the
// start of the name (so the mimalloc_* comparison arms are NOT smuggled into a
// SeferAlloc gate); `sub:` entries are plain substrings, as written in the ADR.
const GROUPS = [
  { id: 'hot-churn', label: 'hot churn', prefix: ['small_churn', 'churn'] },
  { id: 'cold', label: 'cold', prefix: ['cold_alloc'] },
  { id: 'recycle', label: 'recycle', prefix: ['recycle'] },
  { id: 'flush', label: 'flush', sub: ['flush'] },
  { id: 'magazine', label: 'magazine', sub: ['magazine'] },
  { id: 'refill', label: 'refill path', prefix: ['carve_batch', 'alloc_zeroed_calloc_virgin'] },
  { id: 'decommit', label: 'decommit', prefix: ['seg_cycle_decommit', 'multiseg_cold'] },
];

// ADR §Гейты "Горячий tcache" row names the four hot benches explicitly; the
// `small_churn*`/`churn*` families above cover three of them, `aligned_churn_*`
// is checked here so the 1.02x gate is applied to the ADR's own set too.
const ADR_HOT = [
  'small_churn_16b',
  'churn_256b',
  'churn_write_256b',
  'aligned_churn_640b_a128',
];
// ADR §Гейты "Refill/flush" row — EstCycles ≤ 1.05x (this is a DIFFERENT set
// from the `carve_batch*`/`alloc_zeroed_calloc_virgin*` "refill path" family
// above; the gate covers cold/recycle/multiseg, so both are reported).
const ADR_REFILL = [
  'cold_alloc_free_256x16b',
  'cold_alloc_free_256x16b_2n',
  'cold_alloc_free_256x16b_4n',
  'cold_alloc_free_256x64b',
  'recycle_alloc_free_256x16b',
  'recycle_alloc_free_256x64b',
  'multiseg_cold_256k',
];

const TH_HOT_PCT = 2; // ADR: hot path EstCycles ≤ 1.02x
const TH_REFILL_PCT = 5; // ADR: refill/flush EstCycles ≤ 1.05x
const TH_REFILL_IR_PCT = 10; // ADR: refill/flush Ir ≤ 1.10x
const TH_PG3_PCT = 25.0;

// --- log parsing -------------------------------------------------------------
const ROW_LABEL = new Set([
  'Instructions',
  'L1 Hits',
  'L2 Hits',
  'RAM Hits',
  'Total read+write',
  'Estimated Cycles',
]);
/** One metric row: `<label>: <current>|<baseline> (<note>)`. The baseline half is
 *  EITHER a number (`N|N` — a stored baseline was reused) OR the literal `N/A`
 *  (`N|N/A` — a fresh run with nothing to compare against, which is the shape of
 *  the re-measured spike log). Both are accepted; the FIRST number is the
 *  measurement, and it is the only thing ever read out of the row. */
const METRIC_ROW_RE =
  /^([A-Za-z][A-Za-z0-9 +]*?):\s+(\d[\d,]*)\s*\|\s*(\d[\d,]*|N\/A)\b/;
const HEADER_RE = /^([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)+)\s*$/;

function shortName(fullHeader) {
  const parts = fullHeader.split('::');
  return parts[parts.length - 1];
}

/** Parse one raw iai log into an ordered list of bench rows (first number of
 *  every metric row = the current run's absolute count). */
function parseLog(text, path) {
  const rows = new Map(); // full header line -> row (dup headers: first wins)
  const order = [];
  let current = null;
  let duplicates = 0;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    const h = HEADER_RE.exec(line.trim());
    if (h) {
      current = h[1];
      if (rows.has(current)) duplicates += 1;
      else {
        rows.set(current, {
          key: current,
          name: shortName(current),
          ir: null,
          cycles: null,
          l1: null,
          l2: null,
          ram: null,
        });
        order.push(current);
      }
      continue;
    }
    const m = METRIC_ROW_RE.exec(line.trim());
    if (!m || !current || !rows.has(current)) continue;
    const label = m[1];
    if (!ROW_LABEL.has(label)) continue;
    // FIRST number = this run's measurement (base-10, separators stripped).
    const value = Number.parseInt(m[2].replace(/,/g, ''), 10);
    assert(Number.isFinite(value), `${path}: non-numeric "${label}" value`);
    const row = rows.get(current);
    if (label === 'Instructions') {
      assert(row.ir === null, `${path}: duplicate Instructions for ${current}`);
      row.ir = value;
    } else if (label === 'Estimated Cycles') row.cycles = value;
    else if (label === 'L1 Hits') row.l1 = value;
    else if (label === 'L2 Hits') row.l2 = value;
    else if (label === 'RAM Hits') row.ram = value;
  }
  assert(rows.size > 0, `${path}: no benches parsed`);
  for (const key of order) {
    const row = rows.get(key);
    assert(row.ir !== null, `${path}: ${key} has no Instructions row`);
    assert(row.cycles !== null, `${path}: ${key} has no Estimated Cycles row`);
  }
  if (duplicates > 0) {
    process.stderr.write(
      `[pg3] note: ${path}: ${duplicates} repeated bench header(s); first block kept\n`,
    );
  }
  return { rows, order };
}

// --- percent printer (the ONLY delta formatter; guards against hand numbers) --
// DELETE / hand-editing a number in a log and this asserts.
function pct(base, spike, what) {
  assert(
    Number.isFinite(base) && Number.isFinite(spike),
    `non-finite metric for ${what}`,
  );
  assert(base !== 0, `zero denominator for ${what}`);
  const p = ((spike - base) / base) * 100;
  assert(Number.isFinite(p), `non-finite percent for ${what}`);
  // Round-trip guard: base*(1+p/100) must reproduce spike to ±0.01%.
  const back = base * (1 + p / 100);
  const relErr = spike === 0 ? Math.abs(back) : Math.abs(back - spike) / Math.abs(spike);
  assert(
    relErr <= 1e-4,
    `round-trip mismatch for ${what}: base=${base} spike=${spike} pct=${p} back=${back}`,
  );
  return p;
}

function fmtPct(base, spike, what) {
  const p = pct(base, spike, what);
  const sign = p > 0 ? '+' : p < 0 ? '-' : '+';
  return `${sign}${Math.abs(p).toFixed(3)}%`;
}

function factor(base, spike, what) {
  return (1 + pct(base, spike, what) / 100).toFixed(5);
}

/** Format an already-computed percent with an explicit sign. The value MUST
 *  have come from `pct()` (finite, round-trip-checked). */
function fmtSigned(p) {
  assert(Number.isFinite(p), 'non-finite percent passed to fmtSigned');
  return `${p >= 0 ? '+' : ''}${p.toFixed(3)}%`;
}
function fmtFactor(p) {
  return `x${(1 + p / 100).toFixed(5)}`;
}

const int = (n) => n.toLocaleString('en-US');

/** Display a path relative to the repo root (the note cites relative paths). */
function rel(p) {
  return relative(REPO_ROOT, resolve(p)).replace(/\\/g, '/');
}

function groupsFor(name) {
  const hits = [];
  for (const g of GROUPS) {
    const hit =
      (g.prefix && g.prefix.some((p) => name.startsWith(p))) ||
      (g.sub && g.sub.some((s) => name.includes(s)));
    if (hit) hits.push(g.id);
  }
  return hits;
}

// --- main --------------------------------------------------------------------
const base = parseLog(readFileSync(opts.base, 'utf8'), opts.base);
const spike = parseLog(readFileSync(opts.spike, 'utf8'), opts.spike);

// The two runs must cover EXACTLY the same bench set.
const baseKeys = base.order;
const spikeKeys = spike.order;
assert.deepStrictEqual(
  baseKeys.slice().sort(),
  spikeKeys.slice().sort(),
  'bench sets differ between base and spike logs',
);

const rows = baseKeys.map((key) => {
  const b = base.rows.get(key);
  const s = spike.rows.get(key);
  return {
    key,
    name: b.name,
    groups: groupsFor(b.name),
    irBase: b.ir,
    irSpike: s.ir,
    cycBase: b.cycles,
    cycSpike: s.cycles,
    l1Base: b.l1,
    l1Spike: s.l1,
    l2Base: b.l2,
    l2Spike: s.l2,
    ramBase: b.ram,
    ramSpike: s.ram,
  };
});

const out = [];
const say = (line = '') => out.push(line);

say('# PG-3 off-body NextTable spike — iai A/B (callgrind, deterministic)');
say();
say(`base:  \`${rel(opts.base)}\` (pg3-base @ 7c0c796b45e352d86e58bd3f2ec993f3735030f6, tree clean)`);
say(
  `spike: \`${rel(opts.spike)}\` (pg3-spike @ ${SPIKE_COMMIT}, tree clean — \`git diff\` empty)`,
);
say(`features: \`${FEATURES}\` (log header: \`production bench-internals internals\`)  |  benches: ${rows.length}`);
say();
say('Every metric column is the FIRST number of the iai row (the current run); the');
say('second number is the runner\'s own baseline (a count, or `N/A` for a fresh');
say('run) and is deliberately ignored.');
say('ΔX% = (spike − base) / base × 100. Every percent goes through a guarded');
say('printer that asserts `Number.isFinite(pct)` and that');
say('`base*(1+pct/100)` reproduces spike to ±0.01%.');
say();
say('| bench | Ir base | Ir spike | ΔIr % | Cycles base | Cycles spike | ΔCycles % | groups |');
say('|---|---:|---:|---:|---:|---:|---:|---|');
for (const r of rows) {
  say(
    `| ${r.name} | ${r.irBase} | ${r.irSpike} | ${fmtPct(r.irBase, r.irSpike, `Ir ${r.name}`)} | ${r.cycBase} | ${r.cycSpike} | ${fmtPct(r.cycBase, r.cycSpike, `EstCycles ${r.name}`)} | ${r.groups.join(' ') || ''} |`,
  );
}
say();

if (opts.cache) {
  say('## Cache counters (L1 / L2 / RAM hits) — group-marked rows');
  say();
  say('| bench | L1 base | L1 spike | ΔL1 % | L2 base | L2 spike | ΔL2 % | RAM base | RAM spike | ΔRAM % |');
  say('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
  for (const r of rows) {
    if (!r.groups.length || r.l1Base === null) continue;
    say(
      `| ${r.name} | ${r.l1Base} | ${r.l1Spike} | ${fmtPct(r.l1Base, r.l1Spike, `L1 ${r.name}`)} | ${r.l2Base} | ${r.l2Spike} | ${fmtPct(r.l2Base, r.l2Spike, `L2 ${r.name}`)} | ${r.ramBase} | ${r.ramSpike} | ${fmtPct(r.ramBase, r.ramSpike, `RAM ${r.name}`)} |`,
    );
  }
  say();
}

// --- CSV ---------------------------------------------------------------------
const csvLines = [
  'bench,ir_base,ir_spike,cycles_base,cycles_spike,commit,features',
  ...rows.map(
    (r) =>
      `${r.name},${r.irBase},${r.irSpike},${r.cycBase},${r.cycSpike},${SPIKE_COMMIT},${FEATURES}`,
  ),
];
mkdirSync(dirname(CSV_PATH), { recursive: true });
writeFileSync(CSV_PATH, `${csvLines.join('\n')}\n`, 'utf8');

// --- threshold summary -------------------------------------------------------
const pctArr = (sel) => rows.filter(sel).map((r) => pct(r.cycBase, r.cycSpike, `EstCycles ${r.name}`));
const irPctArr = (sel) => rows.filter(sel).map((r) => pct(r.irBase, r.irSpike, `Ir ${r.name}`));
const maxOf = (arr) => (arr.length ? Math.max(...arr) : null);
const meanOf = (arr) => (arr.length ? arr.reduce((a, b) => a + b, 0) / arr.length : null);
const worst = (arr) => arr.slice().sort((a, b) => b - a)[0];

say('## Сводка против порогов (PG-3 / ADR §Гейты)');
say();

const hot = rows.filter((r) => r.groups.includes('hot-churn'));
const hotCyc = pctArr((r) => r.groups.includes('hot-churn'));
const hotIr = irPctArr((r) => r.groups.includes('hot-churn'));
const hotMax = worst(hotCyc);
const hotMaxIr = worst(hotIr);
const hotMean = meanOf(hotCyc);
say(
  `hot path (${hot.length} benches; \`small_churn*\`, \`churn*\`) — EstCycles ≤ +${TH_HOT_PCT}% :`,
);
say(
  `  max ΔEstCycles = ${fmtSigned(hotMax)} (${fmtFactor(hotMax)}), mean Δ = ${fmtSigned(hotMean)}  ->  ${hotMax <= TH_HOT_PCT ? 'PASS' : 'FAIL'}`,
);
say(
  `  max ΔIr        = ${fmtSigned(hotMaxIr)} (${fmtFactor(hotMaxIr)}), mean ΔIr = ${fmtSigned(meanOf(hotIr))}`,
);
const adrHotRows = ADR_HOT.map((n) => rows.find((r) => r.name === n)).filter(Boolean);
const adrHot = pctArr((r) => ADR_HOT.includes(r.name));
say(
  `  ADR hot set (${adrHotRows.map((r) => r.name).join(', ')}): max ΔEstCycles = ${fmtSigned(worst(adrHot))} (${fmtFactor(worst(adrHot))})`,
);
say();

const refill = rows.filter((r) => r.groups.includes('refill'));
const refillCyc = pctArr((r) => r.groups.includes('refill'));
say(
  `refill path (${refill.length} benches; \`carve_batch*\`, \`alloc_zeroed_calloc_virgin*\`) — EstCycles ≤ +${TH_REFILL_PCT}% :`,
);
say(
  `  max ΔEstCycles = ${fmtSigned(worst(refillCyc))} (${fmtFactor(worst(refillCyc))})  ->  ${worst(refillCyc) <= TH_REFILL_PCT ? 'PASS' : 'FAIL'}`,
);
const refillIr = irPctArr((r) => r.groups.includes('refill'));
say(`  max ΔIr        = ${fmtSigned(worst(refillIr))} (${fmtFactor(worst(refillIr))})`);
const adrRefill = pctArr((r) => ADR_REFILL.includes(r.name));
say(
  `  ADR refill/flush set (cold_alloc_free_*/recycle_alloc_free_*/multiseg_cold_256k, ${adrRefill.length} benches): max ΔEstCycles = ${fmtSigned(worst(adrRefill))} (${fmtFactor(worst(adrRefill))})`,
);
const adrRefillIr = irPctArr((r) => ADR_REFILL.includes(r.name));
say(
  `  ADR refill/flush Ir ≤ 1.10x: max ΔIr = ${fmtSigned(worst(adrRefillIr))} (${fmtFactor(worst(adrRefillIr))})`,
);say();

const allCyc = rows.map((r) => ({ r, d: pct(r.cycBase, r.cycSpike, `EstCycles ${r.name}`) }));
const over25 = allCyc.filter((x) => x.d >= TH_PG3_PCT);
say(`PG-3 («EstCycles хуже ≥ ${TH_PG3_PCT}% без пути улучшения — пересмотр геометрии»):`);
say(`  benches with ΔEstCycles ≥ +${TH_PG3_PCT}%: ${over25.length}`);
for (const x of over25) {
  say(`    ${x.r.name}: ${fmtSigned(x.d)} (${fmtFactor(x.d)})`);
}
say();

const allIr = rows.map((r) => ({ r, d: pct(r.irBase, r.irSpike, `Ir ${r.name}`) }));
say(`худшие 5 бенчей по ΔEstCycles:`);
for (const x of allCyc.slice().sort((a, b) => b.d - a.d).slice(0, 5)) {
  const irD = allIr.find((y) => y.r === x.r).d;
  say(
    `  ${x.r.name.padEnd(42)} ΔEstCycles ${fmtSigned(x.d)} (${fmtFactor(x.d)})  ΔIr ${fmtSigned(irD)}`,
  );
}
say();
say(`худшие 5 бенчей по ΔIr:`);
for (const x of allIr.slice().sort((a, b) => b.d - a.d).slice(0, 5)) {
  say(`  ${x.r.name.padEnd(42)} ΔIr ${fmtSigned(x.d)} (${fmtFactor(x.d)})`);
}
say();
say(`лучшие 3 по ΔEstCycles (может быть выигрыш):`);
for (const x of allCyc.slice().sort((a, b) => a.d - b.d).slice(0, 3)) {
  say(`  ${x.r.name.padEnd(42)} ΔEstCycles ${fmtSigned(x.d)}`);
}
say();
say(`CSV записан: ${rel(CSV_PATH)}`);
say(`  заголовок: ${csvLines[0]}`);
say(`  строк: ${rows.length}`);

process.stdout.write(`${out.join('\n')}\n`);
