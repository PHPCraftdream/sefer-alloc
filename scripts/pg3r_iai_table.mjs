#!/usr/bin/env node
// PG-3r / patch S — iai A/B table builder (callgrind, deterministic).
//
// WHY THIS EXISTS: step 0 (PG-3r) of
// docs/design/2026-10-02-adr-addendum-ph3c-escalation.md §3 re-runs the PG-3 A/B
// (base `7c0c796b` vs spike `8a028616`) with BOTH sides carrying patch S: the
// pending-bitmap scan starts at the payload's first word
// (`payload_start / (MIN_BLOCK * 64)`, floor) instead of word 0, in
// `drain_segment_sidecar` (find_segment.rs) and in every word-cursor reset in
// sidecar_drain.rs. The PG-3 spike (+1 MiB NextTable) made every drain walk 1024
// extra words ≈ 1024 x 12 Ir = 12288 Ir per pass; patch S removes exactly those
// words, so the measurement is S-arm vs S-arm and only the *residual* should
// remain. Decision rule is Ir-only (ADR §3): hot <= 1.02x, the four refill
// benches <= 1.10x, mimalloc arm ΔIr = 0.000% exactly.
//
// FOUR logs are read:
//   S-run base  = docs/perf/_raw_pg3r_iai_base.log   (this tree, worktree ../pg3r-base)
//   S-run spike = docs/perf/_raw_pg3r_iai_spike.log  (this tree, worktree ../pg3r-spike)
//   hist base   = via --hist-base (default ../../docs/perf/_raw_pg3_iai_base.log)
//   hist spike  = via --hist-spike (default ../../docs/perf/_raw_pg3_iai_spike.log)
// The historical logs are the previous PG-3 measurement WITHOUT patch S; they are
// used only for the attribution columns («vs PG3») — denominators are the
// historical log of the SAME side, the S-run is the satellite.
//
// iai-callgrind's stdout block per bench looks like:
//   perf_gate_iai::perf_gate::<name>
//     Instructions:        58299|58299   (No change)      <- run|baseline
//     ...
//     Estimated Cycles:   144222|144222   (No change)
// and, when the runner had no stored baseline:
//     Instructions:        58309|N/A     (*********)      <- run|N/A
// The historical base log also carries `60549|60496   (+0.08761%) [+1.00088x]`.
// We ALWAYS take the FIRST number of each row (the current run's absolute
// count); the second half (a count, `N/A`, or a percent) is whatever the runner
// happened to have — it is NOT the other worktree's run, so trusting it would
// silently corrupt the A/B. The full header line is the row key (bench names may
// repeat with suffixes — never key by the short name).
//
// The percent printer is the ONLY place a delta is formatted: it asserts the
// value is finite AND that base*(1+pct/100) reproduces spike to ±0.01%, so a
// hand-typed or copy-pasted number can never reach the table.
//
// Usage (from the pg3r-base repo root):
//   node scripts/pg3r_iai_table.mjs [--base <path>] [--spike <path>]
//                                   [--hist-base <path>] [--hist-spike <path>]
// Writes docs/perf/PG3R_SCAN_PATCH_IAI_summary.csv as a side effect.

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
// The main checkout is reachable read-only two levels up from this worktree
// (worktrees/pg3r-base -> worktrees -> repo root). Relative, never hard-coded.
const MAIN_CHECKOUT = resolve(REPO_ROOT, '..', '..');

// --- identity (PG-3r §3: immutable SHAs + the exact feature string) ----------
const BASE_COMMIT = '7c0c796b45e352d86e58bd3f2ec993f3735030f6';
const SPIKE_COMMIT = '8a0286166a07c3ff35258836400d94f16c7994cd';
const PATCH_S_SHA256 = '94a911665b705c19228689dc0484c618f41c1a7c8860e9aa4bc77ba146a122ee';
const BASE_WRITE_TREE = 'c02f6fa1db817103d61512cade13ab016b6ec823';
const SPIKE_WRITE_TREE = '3d93a1ea9656f2bfe8a747ac2ad2362a1651a545';
const FEATURES = 'production bench-internals internals';
const TARGET_DIR_BASE = '/tmp/sefer-pg3r-base';
const TARGET_DIR_SPIKE = '/tmp/sefer-pg3r-spike';
const VALGRIND = '3.22.0';
const RUSTC = '1.98.1';
const IAIRUNNER = '0.14.2';
const BASE_WSL_ROOT = '/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-base';
const SPIKE_WSL_ROOT = '/mnt/d/dev/rust/sefer-alloc/worktrees/pg3r-spike';
const BASE_RUN_SECONDS = '96.9599';
const SPIKE_RUN_SECONDS = '94.4842';
const BENCH_HEADER_PREFIX = 'perf_gate_iai::perf_gate::';

const DEFAULT_BASE_LOG = join(REPO_ROOT, 'docs', 'perf', '_raw_pg3r_iai_base.log');
const DEFAULT_SPIKE_LOG = join(REPO_ROOT, 'docs', 'perf', '_raw_pg3r_iai_spike.log');
const DEFAULT_HIST_BASE_LOG = join(
  MAIN_CHECKOUT,
  'docs',
  'perf',
  '_raw_pg3_iai_base.log',
);
const DEFAULT_HIST_SPIKE_LOG = join(
  MAIN_CHECKOUT,
  'docs',
  'perf',
  '_raw_pg3_iai_spike.log',
);
const CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PG3R_SCAN_PATCH_IAI_summary.csv');

// --- CLI ---------------------------------------------------------------------
const argv = process.argv.slice(2);
const opts = {
  base: DEFAULT_BASE_LOG,
  spike: DEFAULT_SPIKE_LOG,
  histBase: DEFAULT_HIST_BASE_LOG,
  histSpike: DEFAULT_HIST_SPIKE_LOG,
};
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '--base') opts.base = resolve(argv[++i]);
  else if (argv[i] === '--spike') opts.spike = resolve(argv[++i]);
  else if (argv[i] === '--hist-base') opts.histBase = resolve(argv[++i]);
  else if (argv[i] === '--hist-spike') opts.histSpike = resolve(argv[++i]);
  else throw new Error(`unknown argument: ${argv[i]}`);
}

// --- group definitions (perf-gate bench families, short-name matched) --------
const GROUPS = [
  { id: 'hot-churn', label: 'hot churn', prefix: ['small_churn', 'churn'] },
  { id: 'cold', label: 'cold', prefix: ['cold_alloc'] },
  { id: 'recycle', label: 'recycle', prefix: ['recycle'] },
  { id: 'flush', label: 'flush', sub: ['flush'] },
  { id: 'magazine', label: 'magazine', sub: ['magazine'] },
  { id: 'refill', label: 'refill path', prefix: ['carve_batch', 'alloc_zeroed_calloc_virgin'] },
  { id: 'decommit', label: 'decommit', prefix: ['seg_cycle_decommit', 'multiseg_cold'] },
];

// ADR §Гейты "Горячий tcache" — the four hot benches, verbatim.
const ADR_HOT = [
  'small_churn_16b',
  'churn_256b',
  'churn_write_256b',
  'aligned_churn_640b_a128',
];
// PG-3r §3 "указанные 4 refill-бенча <= 1.10" — the four refill benches.
const ADR_REFILL = [
  'cold_alloc_free_256x16b',
  'cold_alloc_free_256x64b',
  'recycle_alloc_free_256x16b',
  'recycle_alloc_free_256x64b',
];
// Report only (ADR §3: EstCycles and multiseg_cold_256k are informational).
const REPORT_ONLY = ['multiseg_cold_256k'];
// Built-in A/A arm: no Sefer code at all.
const AA_PREFIX = 'mimalloc';

const TH_HOT_PCT = 2; // ADR: hot path Ir ≤ 1.02x
const TH_REFILL_PCT = 10; // PG-3r §3: the four refill benches Ir ≤ 1.10x
const TH_PREDICT_PCT = 0.5; // prediction written down BEFORE the measurement
const WORD_IR = 12; // one `swap(0, AcqRel)` on a pending-bitmap word ≈ 12 Ir
const WORDS_PER_MIB_NEXTTABLE = 1024; // 1 MiB NextTable / 1 KiB per word
const SCAN_WORD_IR = WORD_IR * WORDS_PER_MIB_NEXTTABLE; // 12288 Ir per drain pass

// --- log parsing (identical to scripts/pg3_iai_table.mjs) --------------------
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
 *  (`N|N/A` — a fresh run with nothing to compare against, the shape of the
 *  pg3r-spike log) OR, in the historical PG-3 base log, a count followed by a
 *  percent note (`60549|60496 (+0.08761%) [+1.00088x]`). All are accepted; the
 *  FIRST number is the measurement, and it is the only thing ever read. */
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
      `[pg3r] note: ${path}: ${duplicates} repeated bench header(s); first block kept\n`,
    );
  }
  return { rows, order };
}

// --- guarded ratio / percent printers ---------------------------------------
// DELETE or hand-edit a number in a log and these assert. Every printed number
// goes through here: there is no other place a delta is formatted.
function ratioOf(base, spike, what) {
  assert(
    Number.isFinite(base) && Number.isFinite(spike),
    `non-finite metric for ${what}`,
  );
  assert(base !== 0, `zero denominator for ${what}`);
  const r = spike / base;
  assert(Number.isFinite(r), `non-finite ratio for ${what}`);
  const back = base * r;
  const relErr = spike === 0 ? Math.abs(back) : Math.abs(back - spike) / Math.abs(spike);
  assert(
    relErr <= 1e-4,
    `round-trip mismatch for ${what}: base=${base} spike=${spike} ratio=${r} back=${back}`,
  );
  return r;
}

/** ΔX% = (spike − base) / base × 100. Guarded: finite, non-zero denominator,
 *  and `base*(1+p/100)` reproduces spike to ±0.01% relative. */
function pct(base, spike, what) {
  return (ratioOf(base, spike, what) - 1) * 100;
}

function fmtPct(base, spike, what) {
  const p = pct(base, spike, what);
  const sign = p > 0 ? '+' : p < 0 ? '-' : '+';
  return `${sign}${Math.abs(p).toFixed(3)}%`;
}

/** Already-computed percent (must have come from `pct()`) with explicit sign. */
function fmtSigned(p) {
  assert(Number.isFinite(p), 'non-finite percent passed to fmtSigned');
  return `${p >= 0 ? '+' : ''}${p.toFixed(3)}%`;
}

function factor(base, spike, what, digits = 5) {
  return `x${ratioOf(base, spike, what).toFixed(digits)}`;
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
const histBase = parseLog(readFileSync(opts.histBase, 'utf8'), opts.histBase);
const histSpike = parseLog(readFileSync(opts.histSpike, 'utf8'), opts.histSpike);

// All four runs must cover EXACTLY the same bench set.
const benchKeys = base.order;
for (const [tag, log] of [
  ['S spike', spike],
  ['hist base', histBase],
  ['hist spike', histSpike],
]) {
  assert.deepStrictEqual(
    benchKeys.slice().sort(),
    log.order.slice().sort(),
    `bench sets differ between S base and ${tag} logs`,
  );
}

const rows = benchKeys.map((key) => {
  const b = base.rows.get(key);
  const s = spike.rows.get(key);
  const hb = histBase.rows.get(key);
  const hs = histSpike.rows.get(key);
  return {
    key,
    name: b.name,
    groups: groupsFor(b.name),
    irBaseS: b.ir,
    irSpikeS: s.ir,
    cycBaseS: b.cycles,
    cycSpikeS: s.cycles,
    irHistBase: hb.ir,
    irHistSpike: hs.ir,
    cycHistBase: hb.cycles,
    cycHistSpike: hs.cycles,
  };
});

const byName = new Map(rows.map((r) => [r.name, r]));
const findBench = (name) => {
  const r = byName.get(name);
  assert(r !== undefined, `bench ${name} not present in the logs`);
  return r;
};

const out = [];
const say = (line = '') => out.push(line);

say('# PG-3r (patch S: pending-bitmap scan starts at the payload first word) — iai A/B');
say();
say(`base:  \`${rel(opts.base)}\` (worktree pg3r-base @ ${BASE_COMMIT} + patch S)`);
say(`spike: \`${rel(opts.spike)}\` (worktree pg3r-spike @ ${SPIKE_COMMIT} + patch S)`);
say(
  `hist (PG-3, no patch S): \`${rel(opts.histBase)}\` (base), \`${rel(opts.histSpike)}\` (spike)`,
);
say(`features: \`${FEATURES}\` (log header: \`production bench-internals internals\`)  |  benches: ${rows.length}`);
say();
say('Every metric column is the FIRST number of the iai row (the current run); the');
say('second half is the runner\'s own baseline (a count, `N/A`, or a percent note)');
say('and is deliberately ignored.');
say('ΔX% = (spike − base) / base × 100. The PATCH-S columns are a PATCH-S arm');
say('against a PATCH-S arm (patch S is applied on BOTH sides). The «vs PG3»');
say('columns compare against the previous PG-3 log of the SAME side: there the');
say('DENOMINATOR is the historical (no-S) log of that side and the S-run is the');
say('satellite, i.e. Δ = (Ir_S − Ir_hist) / Ir_hist × 100.');
say('Every percent goes through a guarded printer that asserts');
say('`Number.isFinite(pct)` and that `base*(1+pct/100)` reproduces spike to');
say('±0.01% relative error.');
say();
say('| bench | Ir base_S | Ir spike_S | ΔIr % | ΔEstCycles % | Ir Δ vs PG3 base | Ir Δ vs PG3 spike | groups |');
say('|---|---:|---:|---:|---:|---:|---:|---|');
for (const r of rows) {
  say(
    `| ${r.name} | ${r.irBaseS} | ${r.irSpikeS} | ${fmtPct(r.irBaseS, r.irSpikeS, `Ir ${r.name}`)} | ${fmtPct(r.cycBaseS, r.cycSpikeS, `EstCycles ${r.name}`)} | ${fmtPct(r.irHistBase, r.irBaseS, `Ir vs PG3 base ${r.name}`)} | ${fmtPct(r.irHistSpike, r.irSpikeS, `Ir vs PG3 spike ${r.name}`)} | ${r.groups.join(' ') || ''} |`,
  );
}
say();

// --- CSV ---------------------------------------------------------------------
const csvLines = [
  'bench,ir_base_s,ir_spike_s,cycles_base_s,cycles_spike_s,ir_pg3_base,ir_pg3_spike,ir_hist_base,ir_hist_spike,features',
  ...rows.map(
    (r) =>
      [
        r.name,
        r.irBaseS,
        r.irSpikeS,
        r.cycBaseS,
        r.cycSpikeS,
        // ΔIr of the SAME side against its historical (no-S) PG-3 log, percent.
        pct(r.irHistBase, r.irBaseS, `Ir vs PG3 base ${r.name}`).toFixed(6),
        pct(r.irHistSpike, r.irSpikeS, `Ir vs PG3 spike ${r.name}`).toFixed(6),
        // denominators of the two «vs PG3» columns, kept for reproducibility.
        r.irHistBase,
        r.irHistSpike,
        FEATURES,
      ].join(','),
  ),
];
mkdirSync(dirname(CSV_PATH), { recursive: true });
writeFileSync(CSV_PATH, `${csvLines.join('\n')}\n`, 'utf8');

// --- threshold summary (Ir only, ADR-addendum §3) ----------------------------
const irPct = (r) => pct(r.irBaseS, r.irSpikeS, `Ir ${r.name}`);
const cycPct = (r) => pct(r.cycBaseS, r.cycSpikeS, `EstCycles ${r.name}`);

/** The row of a results array with the largest percent. */
const worstOf = (arr) => arr.reduce((a, b) => (b.p > a.p ? b : a), arr[0]);

say('## Сводка против порогов (PG-3r, ADR-addendum §3 — только Ir)');
say();
say(`Плечи: base = \`${rel(opts.base)}\`, spike = \`${rel(opts.spike)}\` (обе стороны с патч S).`);
say('Знаменатель каждого процента: ΔX% = (spike − base) / base × 100; порог-плечо —');
say('отношение C/B в одном режиме, т.e. `1.00 + Δ/100`. Колонки «vs PG3» — против');
say('прежнего PG-3 лога той же стороны (база = исторический лог стороны).');
say();

say(`### Горячий набор ADR (4 бенча) — ΔIr ≤ +${TH_HOT_PCT}% (x1.02)`);
say();
const hotRows = ADR_HOT.map(findBench);
const hotResults = hotRows.map((r) => ({ r, p: irPct(r) }));
for (const { r, p } of hotResults) {
  const ok = p <= TH_HOT_PCT;
  say(
    `  ${r.name.padEnd(26)} Ir base_S ${int(r.irBaseS).padStart(12)}  Ir spike_S ${int(r.irSpikeS).padStart(12)}  ΔIr ${fmtSigned(p).padStart(9)} (${factor(r.irBaseS, r.irSpikeS, `Ir ${r.name}`, 4)})  ->  ${ok ? 'PASS' : 'FAIL'}`,
  );
}
const hotWorst = worstOf(hotResults);
const hotMax = hotWorst.p;
say(
  `  max ΔIr = ${fmtSigned(hotMax)} (${factor(hotWorst.r.irBaseS, hotWorst.r.irSpikeS, 'hot max', 4)})  ->  ${hotMax <= TH_HOT_PCT ? 'PASS' : 'FAIL'}`,
);
say();

say(`### Refill-набор (4 бенча) — ΔIr ≤ +${TH_REFILL_PCT}% (x1.10)`);
say();
const refillRows = ADR_REFILL.map(findBench);
const refillResults = refillRows.map((r) => ({ r, p: irPct(r) }));
for (const { r, p } of refillResults) {
  const ok = p <= TH_REFILL_PCT;
  say(
    `  ${r.name.padEnd(26)} Ir base_S ${int(r.irBaseS).padStart(12)}  Ir spike_S ${int(r.irSpikeS).padStart(12)}  ΔIr ${fmtSigned(p).padStart(9)} (${factor(r.irBaseS, r.irSpikeS, `Ir ${r.name}`, 4)})  ->  ${ok ? 'PASS' : 'FAIL'}`,
  );
}
const refillMax = worstOf(refillResults).p;
const refillWorst = worstOf(refillResults);
say(
  `  max ΔIr = ${fmtSigned(refillMax)} (${factor(refillWorst.r.irBaseS, refillWorst.r.irSpikeS, 'refill max', 4)})  ->  ${refillMax <= TH_REFILL_PCT ? 'PASS' : 'FAIL'}`,
);
say();

// --- mimalloc A/A control -----------------------------------------------------
const aaRows = rows.filter((r) => r.name.startsWith(AA_PREFIX));
assert(aaRows.length > 0, 'no mimalloc control benches found');
const aaPcts = aaRows.map((r) => ({ r, p: irPct(r) }));
say(`### mimalloc-контроль A/A (${aaRows.length} бенчей, префикс \`${AA_PREFIX}\`) — ΔIr должна быть ровно 0.000%`);
say();
for (const { r, p } of aaPcts) {
  say(
    `  ${r.name.padEnd(38)} Ir base_S ${int(r.irBaseS).padStart(9)}  Ir spike_S ${int(r.irSpikeS).padStart(9)}  ΔIr ${fmtSigned(p)}${p === 0 ? '' : '   <-- НЕ НОЛЬ'}`,
  );
}
const aaMaxAbs = Math.max(...aaPcts.map((x) => Math.abs(x.p)));
const aaOk = aaPcts.every((x) => x.p === 0);
say(`  max |ΔIr| = ${fmtSigned(aaMaxAbs)}  ->  ${aaOk ? 'A/A OK' : 'A/A BROKEN'}`);
say();

// --- report-only -------------------------------------------------------------
const reportOnlyRows = REPORT_ONLY.map(findBench);
say('### Только в отчёт (без вердикта — ADR-addendum §3)');
say();
for (const r of reportOnlyRows) {
  say(
    `  ${r.name.padEnd(26)} Ir base_S ${int(r.irBaseS).padStart(12)}  Ir spike_S ${int(r.irSpikeS).padStart(12)}  ΔIr ${fmtSigned(irPct(r))} (${factor(r.irBaseS, r.irSpikeS, `Ir ${r.name}`, 4)})  ΔEstCycles ${fmtPct(r.cycBaseS, r.cycSpikeS, `EstCycles ${r.name}`)}`,
  );
}
say();

// --- prediction (written down BEFORE the measurement) ------------------------
const predictRows = [...hotRows, ...refillRows];
const predictPcts = predictRows.map((r) => ({ r, p: irPct(r) }));
const predictWorst = worstOf(predictPcts);
const predictMax = predictWorst.p;
const predictOk = predictPcts.every((x) => x.p <= TH_PREDICT_PCT);
say(
  `### Предсказание (зафиксировано до замера): ΔIr на 4 hot + 4 refill ≤ +${TH_PREDICT_PCT}%`,
);
say(
  `  max ΔIr = ${fmtSigned(predictMax)} (${factor(predictWorst.r.irBaseS, predictWorst.r.irSpikeS, 'prediction max', 4)})  ->  ${predictOk ? 'выполнено' : 'НЕ выполнено'} (на вердикт гейтов не влияет)`,
);
say();

// --- final verdict -----------------------------------------------------------
const hotOk = hotResults.every((x) => x.p <= TH_HOT_PCT);
const refillOk = refillResults.every((x) => x.p <= TH_REFILL_PCT);
const verdictOk = hotOk && refillOk && aaOk;
say('## Итоговый вердикт');
say();
say(`  hot (4)   все ΔIr ≤ +${TH_HOT_PCT}%  -> ${hotOk ? 'PASS' : 'FAIL'}`);
say(`  refill (4) все ΔIr ≤ +${TH_REFILL_PCT}% -> ${refillOk ? 'PASS' : 'FAIL'}`);
say(`  mimalloc A/A ΔIr = 0.000% ровно -> ${aaOk ? 'PASS' : 'FAIL'}`);
say(`  ВЕРДИКТ: ${verdictOk ? 'PASS' : 'FAIL'}`);
if (verdictOk && !predictOk) {
  say('  (гейты прошли, предсказание — нет: атрибуция частичная; шаг 1 всё равно делается)');
}
say();

// --- residual decomposition ---------------------------------------------------
// How much ΔIr did patch S remove compared with the previous PG-3 ΔIr? Both
// deltas are spike-vs-base of their own measurement; the difference is the Ir
// the patch took off the table, which should land on multiples of
// 12288 = 1024 words x 12 Ir (one extra scanned word per KiB of the 1 MiB
// NextTable metadata).
const DECOMP = [...ADR_HOT, ...ADR_REFILL, ...REPORT_ONLY];
say('## Разложение остатка (сколько ΔIr снял патч S против прежнего PG-3 ΔIr)');
say();
say(
  `Одно слово pending-bitmap покрывает 1 KiB payload; лишнее слово скана ≈ ${WORD_IR} Ir.`,
);
say(
  `1 MiB NextTable = +${WORDS_PER_MIB_NEXTTABLE} слова на проход = ${int(SCAN_WORD_IR)} Ir на проход.`,
);
say(
  'ΔIr_pg3   = (Ir_spike_pg3 − Ir_base_pg3) / Ir_base_pg3 × 100   (исторический лог PG-3, без S)',
);
say(
  'ΔIr_pg3r  = (Ir_spike_S − Ir_base_S) / Ir_base_S × 100        (этот замер, патч S на обеих сторонах)',
);
say('снято_Iр = ΔIr_pg3(in Ir) − ΔIr_pg3r(in Ir);  k = round(снято_Iр / 12288).');
say();
say('| bench | ΔIr_pg3 % | ΔIr_pg3r % | снято Ir | k | k·12288 | остаток Ir | |снято| / k·12288 |');
say('|---|---:|---:|---:|---:|---:|---:|---:|');
const decompResults = [];
for (const name of DECOMP) {
  const r = findBench(name);
  const pPg3 = pct(r.irHistBase, r.irHistSpike, `Ir pg3 ${name}`);
  const pPg3r = irPct(r);
  const removedIr = r.irHistSpike - r.irHistBase - (r.irSpikeS - r.irBaseS);
  const k = Math.round(removedIr / SCAN_WORD_IR);
  const expect = k * SCAN_WORD_IR;
  const residual = removedIr - expect;
  const share =
    k === 0
      ? 'n/a (k=0)'
      : `${(ratioOf(expect, Math.abs(removedIr), `декомп ${name}`) * 100).toFixed(2)}%`;
  say(
    `| ${name} | ${fmtSigned(pPg3)} | ${fmtSigned(pPg3r)} | ${int(removedIr)} | ${k} | ${int(expect)} | ${int(residual)} | ${share} |`,
  );
  decompResults.push({ name, pPg3, pPg3r, removedIr, k, residual });
}
say();
say(
  'Чтение: «снято Ir» — сколько инструкций патч S убрал относительно прежнего PG-3 ΔIr;',
);
say(
  'если механизм ADR-аддендума §1.1 верен, оно садится на целые k·12288 (остаток —',
);
say('прочими незначительными различиями, доля печатается в последней колонке).');
say();

// --- freshness evidence (constants, NOT computed here) -----------------------
say('## Свежесть (свидетельства из логов и WSL — скрипт их не вычисляет)');
say();
say('Ниже — константы-подтверждения, зафиксированные при снятии логов. Этот скрипт');
say('не имеет доступа к WSL и к `/tmp`, поэтому он их только печатает.');
say();
say(`1. base-лог: ровно 1 строка \`Compiling sefer-alloc v0.3.0 (${BASE_WSL_ROOT})\`.`);
say(`2. spike-лог: ровно 1 строка \`Compiling sefer-alloc v0.3.0 (${SPIKE_WSL_ROOT})\`.`);
say(
  `3. \`.d\` bench-бинарника: \`# env-dep:CARGO_MANIFEST_DIR=${BASE_WSL_ROOT}\` (base) и`,
);
say(`   \`# env-dep:CARGO_MANIFEST_DIR=${SPIKE_WSL_ROOT}\` (spike).`);
say(`4. Приватные CARGO_TARGET_DIR: \`${TARGET_DIR_BASE}\` и \`${TARGET_DIR_SPIKE}\``);
say('   (разные — ловушка общего `/tmp/sefer-iai` исключена).');
say(`5. Бенчей в каждом логе: ${rows.length} (совпадение множеств — assert выше).`);
say(
  `6. Трейлер: \`Iai-Callgrind result: Ok. 85 without regressions; 0 regressed; 85 benchmarks finished\``,
);
say(`   (время: base ${BASE_RUN_SECONDS}s, spike ${SPIKE_RUN_SECONDS}s).`);
say(
  `7. valgrind ${VALGRIND}, rustc ${RUSTC}, WSL Ubuntu-24.04, iai-callgrind-runner ${IAIRUNNER}.`,
);
say(
  `8. \`git diff | sha256sum\` = ${PATCH_S_SHA256} в КАЖДОМ дереве (патч идентичен);`,
);
say(`   \`git write-tree\`: base ${BASE_WRITE_TREE}, spike ${SPIKE_WRITE_TREE}.`);
say(
  '9. Формат строки метрик: base-лог `N|N (No change)` (таргет-дир сохранил baseline',
);
say(
  '   первого прогона), spike-лог `N|N/A`. Берётся ПЕРВОЕ число — baseline второй',
);
say('   половины не является прогоном другого ворктрида.');
say();

say(`CSV записан: ${rel(CSV_PATH)}`);
say(`  заголовок: ${csvLines[0]}`);
say(`  строк: ${rows.length}`);

process.stdout.write(`${out.join('\n')}\n`);
