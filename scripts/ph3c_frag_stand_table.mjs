#!/usr/bin/env node
// Ph3c step 1' — fragmentation diagnostic stand (frag-stand) table builder.
//
// WHAT: parses the raw RSS logs of the diagnostic stand described in
// docs/design/2026-10-02-adr-addendum-ph3c-step1prime.md §4 and prints the
// C/B fragmentation table (arm B = base + patch S; arm C = the current B3
// spike), the A/A spread of the B arm, and the C/B <= 1.10 gate verdict for
// workloads W1–W3 (W4 is the ~1.00 control). The metric is
//   frag = (rss_end_kib − rss_empty_kib) * 1024 / live_requested_bytes
// (dimensionless RSS/live-requested ratio, VmRSS from /proc/self/status).
//
// WHY: the addendum §4 prediction, written down BEFORE the run, was
// W1 ≈ 2.0 (FAIL), W2 ≈ 1.17 (FAIL), W4 ≈ 1.00 on the CURRENT spike; a C/B
// gate of <= 1.10 per W1–W3 decides whether the spike passes. If W1 came out
// <= 1.10 the stand would be invalid as an oracle.
//
// INPUT (defaults, relative to the repo root; override with --dir):
//   docs/perf/_raw_ph3c_frag_stand_w1.log … w4.log — 10 blocks each
//     (`== RUN B<rep> ==` / `== RUN C<rep> ==`, alternating B,C,B,C,...),
//     each block `RESULT key=value` lines.
//   docs/perf/_raw_ph3c_frag_stand_aa.log — 12 `== RUN B<rep> ==` blocks
//     (4 workloads x 3 runs, order w1,w2,w3,w4) — the A/A arm.
//   docs/perf/_raw_ph3c_frag_stand_build_base.log — build freshness evidence.
//
// PARSER DISCIPLINE (strict, pg3r_iai_table.mjs style): every block MUST
// contain ALL expected keys or we throw; config_conflicts_before and
// config_conflicts_after MUST both be 0; live_requested_bytes MUST be within
// 1% of 64 MiB. Every printed ratio/percent is recomputed from parsed numbers
// through a guarded printer that asserts `base*(1+pct/100)` reproduces the
// numerator — no hand-typed number can reach the output.
//
// SIDE EFFECT: overwrites docs/perf/PH3C_FRAG_STAND_summary.csv with
// workload,b_median_frag,c_median_frag,c_over_b,gate_110,aa_spread_pct,
// b_segments,c_segments (medians over the 5 runs).
//
// Usage (from the repo root):
//   node scripts/ph3c_frag_stand_table.mjs [--dir <perf-log-dir>]

import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// --- CLI ---------------------------------------------------------------------
const argv = process.argv.slice(2);
let perfDir = join(REPO_ROOT, 'docs', 'perf');
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '--dir') perfDir = resolve(argv[++i]);
  else throw new Error(`unknown argument: ${argv[i]}`);
}
const logPath = (name) => join(perfDir, name);

const CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PH3C_FRAG_STAND_summary.csv');

// --- constants ----------------------------------------------------------------
const REQUIRED_KEYS = [
  'workload',
  'rss_empty_kib',
  'commit_empty_kib',
  'config_conflicts_before',
  'resolved_config',
  'live_requested_bytes',
  'rss_end_kib',
  'commit_end_kib',
  'segments_reserved_total',
  'config_conflicts_after',
];
const LIVE_64MIB = 64 * 1024 * 1024; // 67108896
const LIVE_TOLERANCE = 0.01; // ±1%
const AA_SPREAD_MAX_PCT = 1.0; // (max−min)/median < 1%
const GATE_CB_MAX = 1.1; // C/B <= 1.10 on W1–W3
const GATED_WORKLOADS = ['w1', 'w2', 'w3'];
const CONTROL_WORKLOAD = 'w4';
const RUNS_PER_ARM = 5;

// --- guarded printers (the ONLY place a ratio/percent is formatted) -----------
function ratioOf(base, spike, what) {
  assert(Number.isFinite(base) && Number.isFinite(spike), `non-finite metric for ${what}`);
  assert(base > 0, `non-positive denominator for ${what}`);
  const r = spike / base;
  assert(Number.isFinite(r), `non-finite ratio for ${what}`);
  const back = base * r;
  assert(
    Math.abs(back - spike) <= 0.01 + Math.abs(spike) * 1e-9,
    `round-trip mismatch for ${what}: base=${base} spike=${spike} ratio=${r}`,
  );
  return r;
}

/** Already-computed percent must satisfy base*(1+p/100) == numerator to ±0.01. */
function pctCheck(base, numerator, p, what) {
  assert(Number.isFinite(p), `non-finite percent for ${what}`);
  const back = base * (1 + p / 100);
  assert(
    Math.abs(back - numerator) <= 0.01,
    `percent round-trip mismatch for ${what}: base=${base} num=${numerator} p=${p} back=${back}`,
  );
  return p;
}

const f3 = (base, spike, what) => ratioOf(base, spike, what).toFixed(3);

/** spread% = (max−min)/median × 100, guarded. */
function spreadPct(min, median, max, what) {
  assert(median > 0, `zero median for ${what}`);
  const p = ((max - min) / median) * 100;
  // round-trip: median*(1+p/100) reproduces max, and median*(1-p/100) the min
  pctCheck(median, max, p, `${what} max`);
  pctCheck(median, min, -p, `${what} min`);
  return p;
}

/** C/B as a percent overshoot over 1, guarded: B*(1+p/100)=C. */
function cbPct(b, c, what) {
  return (ratioOf(b, c, what) - 1) * 100;
}

const median = (arr) => {
  assert(arr.length > 0, 'median of empty array');
  const s = [...arr].sort((a, b) => a - b);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
};

// --- parsing -------------------------------------------------------------------
function parseStandLog(text, path, expectedArms) {
  const blocks = [];
  let current = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    const h = /^== RUN ([BC])(\d+) ==$/.exec(line);
    if (h) {
      current = { arm: h[1], rep: Number.parseInt(h[2], 10), kv: new Map(), path };
      blocks.push(current);
      continue;
    }
    const m = /^RESULT ([a-z_0-9]+)=(.*)$/.exec(line);
    if (m && current) {
      assert(!current.kv.has(m[1]), `${path}: duplicate key ${m[1]} in RUN ${current.arm}${current.rep}`);
      current.kv.set(m[1], m[2]);
    }
  }
  assert(blocks.length > 0, `${path}: no RUN blocks parsed`);
  for (const b of blocks) {
    assert(expectedArms.has(b.arm), `${path}: unexpected arm ${b.arm} in RUN ${b.arm}${b.rep}`);
    for (const key of REQUIRED_KEYS) {
      assert(b.kv.has(key), `${path}: RUN ${b.arm}${b.rep} missing key ${key}`);
    }
    for (const key of ['rss_empty_kib', 'commit_empty_kib', 'live_requested_bytes', 'rss_end_kib', 'commit_end_kib', 'segments_reserved_total', 'config_conflicts_before', 'config_conflicts_after']) {
      const v = Number(b.kv.get(key));
      assert(Number.isFinite(v), `${path}: RUN ${b.arm}${b.rep} non-numeric ${key}=${b.kv.get(key)}`);
      b[key] = v;
    }
    b.workload = b.kv.get('workload');
    assert(/^w[1-4]$/.test(b.workload), `${path}: bad workload "${b.workload}"`);
    assert(
      b.config_conflicts_before === 0 && b.config_conflicts_after === 0,
      `${path}: RUN ${b.arm}${b.rep} config_conflicts delta != 0 (before=${b.config_conflicts_before} after=${b.config_conflicts_after})`,
    );
    assert(
      Math.abs(b.live_requested_bytes - LIVE_64MIB) / LIVE_64MIB <= LIVE_TOLERANCE,
      `${path}: RUN ${b.arm}${b.rep} live_requested_bytes=${b.live_requested_bytes} not within ±1% of 64 MiB`,
    );
    // the metric itself, dimensionless (RSS KiB -> bytes via *1024)
    b.frag = ((b.rss_end_kib - b.rss_empty_kib) * 1024) / b.live_requested_bytes;
    assert(Number.isFinite(b.frag) && b.frag > 0, `${path}: non-positive frag for RUN ${b.arm}${b.rep}`);
  }
  return blocks;
}

// --- main -----------------------------------------------------------------------
const W_LOGS = ['w1', 'w2', 'w3', 'w4'].map((w) => logPath(`_raw_ph3c_frag_stand_${w}.log`));
const AA_LOG = logPath('_raw_ph3c_frag_stand_aa.log');
const BUILD_BASE_LOG = logPath('_raw_ph3c_frag_stand_build_base.log');

const byWorkload = new Map([['w1', []], ['w2', []], ['w3', []], ['w4', []]]);
for (const p of W_LOGS) {
  const blocks = parseStandLog(readFileSync(p, 'utf8'), p, new Set(['B', 'C']));
  for (const b of blocks) byWorkload.get(b.workload).push(b);
}

// Alternation check: within each log, odd block positions must be B, even C,
// and reps must be 1..5 in order.
for (const p of W_LOGS) {
  const blocks = parseStandLog(readFileSync(p, 'utf8'), p, new Set(['B', 'C']));
  assert(blocks.length === RUNS_PER_ARM * 2, `${p}: expected 10 blocks, got ${blocks.length}`);
  blocks.forEach((b, i) => {
    const wantArm = i % 2 === 0 ? 'B' : 'C';
    const wantRep = (i >> 1) + 1;
    assert(b.arm === wantArm && b.rep === wantRep, `${p}: block ${i} is ${b.arm}${b.rep}, expected ${wantArm}${wantRep}`);
  });
}

// A/A log: 12 B blocks, 3 per workload in order w1,w2,w3,w4.
const aaBlocks = parseStandLog(readFileSync(AA_LOG, 'utf8'), AA_LOG, new Set(['B']));
assert(aaBlocks.length === 12, `${AA_LOG}: expected 12 blocks, got ${aaBlocks.length}`);
const aaByWorkload = new Map([['w1', []], ['w2', []], ['w3', []], ['w4', []]]);
aaBlocks.forEach((b, i) => {
  const w = ['w1', 'w2', 'w3', 'w4'][Math.floor(i / 3)];
  assert(b.workload === w, `${AA_LOG}: block ${i} workload ${b.workload}, expected ${w}`);
  aaByWorkload.get(w).push(b);
});

// Build freshness: exactly one `Compiling sefer-alloc` line, with its own path.
const buildText = readFileSync(BUILD_BASE_LOG, 'utf8');
const compiling = buildText.split(/\r?\n/).filter((l) => /Compiling sefer-alloc /.test(l));
assert(compiling.length === 1, `${BUILD_BASE_LOG}: expected exactly 1 'Compiling sefer-alloc' line, got ${compiling.length}`);

// --- per-workload stats -----------------------------------------------------------
const results = [];
for (const w of ['w1', 'w2', 'w3', 'w4']) {
  const blocks = byWorkload.get(w);
  const bFrag = blocks.filter((b) => b.arm === 'B').map((b) => b.frag);
  const cFrag = blocks.filter((b) => b.arm === 'C').map((b) => b.frag);
  assert(bFrag.length === RUNS_PER_ARM && cFrag.length === RUNS_PER_ARM, `w=${w}: expected 5 B and 5 C blocks`);
  const bMed = median(bFrag);
  const cMed = median(cFrag);
  const bSeg = median(blocks.filter((b) => b.arm === 'B').map((b) => b.segments_reserved_total));
  const cSeg = median(blocks.filter((b) => b.arm === 'C').map((b) => b.segments_reserved_total));
  const cb = ratioOf(bMed, cMed, `C/B ${w}`);

  // A/A spread from the aa log (B vs B).
  const aaFrag = aaByWorkload.get(w).map((b) => b.frag);
  assert(aaFrag.length === 3, `A/A ${w}: expected 3 runs`);
  const aaMin = Math.min(...aaFrag);
  const aaMax = Math.max(...aaFrag);
  const aaMed = median(aaFrag);
  const aaSpread = spreadPct(aaMin, aaMed, aaMax, `A/A ${w}`);

  results.push({
    w,
    bMed,
    cMed,
    cb,
    cbPctOvershoot: cbPct(bMed, cMed, `C/B ${w}`),
    bSeg,
    cSeg,
    aaSpread,
    gated: GATED_WORKLOADS.includes(w),
    isControl: w === CONTROL_WORKLOAD,
  });
}

// --- report -----------------------------------------------------------------------
const out = [];
const say = (line = '') => out.push(line);

say('# Ph3c шаг 1′ — диагностический стенд фрагментации (frag-stand): B против C');
say();
say('Плечо B = base + патч S, плечо C = текущий спайк B3. Метрика frag =');
say('(rss_end_kib − rss_empty_kib)·1024 / live_requested_bytes (безразмерное');
say('RSS/запрошенные-байты, VmRSS из /proc/self/status). Медианы по 5');
say('чередующимся прогонам на плечо; судья — scripts/ph3c_frag_stand_table.mjs,');
say('см. docs/design/2026-10-02-adr-addendum-ph3c-step1prime.md §4.');
say('Все отношения/проценты перепроверены guarded-принтерами (round-trip ±0.01):');
say('никакое число в таблице не вбитo руками.');
say();
say('| нагрузка | B медиана frag | C медиана frag | C/B (знаменатель = медиана B) | гейт C/B ≤ 1.10 | сегменты B→C | A/A spread % |');
say('|---|---:|---:|---:|---|---|---:|');
for (const r of results) {
  const gate = r.gated ? (r.cb <= GATE_CB_MAX ? 'PASS' : 'FAIL') : 'контроль (не гейтится)';
  say(
    `| ${r.w} | ${r.bMed.toFixed(4)} | ${r.cMed.toFixed(4)} | ${f3(r.bMed, r.cMed, `C/B ${r.w}`)} | ${gate} | ${r.bSeg}→${r.cSeg} | ${r.aaSpread.toFixed(3)}% |`,
  );
}
say();
say('A/A (B против B, aa-лог, 3 прогона на нагрузку): порог (max−min)/median < 1%.');
for (const r of results) {
  say(
    `  ${r.w}: spread ${pctCheck(median(aaByWorkload.get(r.w).map((b) => b.frag)), Math.max(...aaByWorkload.get(r.w).map((b) => b.frag)), r.aaSpread, `A/A ${r.w} recheck`).toFixed(3)}% -> ${r.aaSpread < AA_SPREAD_MAX_PCT ? 'PASS' : 'FAIL'}`,
  );
}
say();
say('Вердикт гейта C/B ≤ 1.10 (W1–W3, знаменатель = медиана B той же нагрузки):');
for (const r of results.filter((x) => x.gated)) {
  say(`  ${r.w}: C/B = ${f3(r.bMed, r.cMed, `C/B ${r.w}`)} -> ${r.cb <= GATE_CB_MAX ? 'PASS' : 'FAIL'}`);
}
const overallPass = results.filter((x) => x.gated).every((x) => x.cb <= GATE_CB_MAX);
const aaPass = results.every((r) => r.aaSpread < AA_SPREAD_MAX_PCT);
say(`  общий: ${overallPass ? 'PASS' : 'FAIL'} (A/A ${aaPass ? 'PASS' : 'FAIL'})`);
say();
say('Контроль W4: C/B = ' + f3(results[3].bMed, results[3].cMed, 'C/B w4') + ' (ожидалось ≈ 1.00).');
say(`Свежесть: ${BUILD_BASE_LOG.split(/[\\/]/).pop()}: ровно одна строка «${compiling[0].trim()}».`);
say();

// --- CSV side effect ---------------------------------------------------------------
const csv = [
  'workload,b_median_frag,c_median_frag,c_over_b,gate_110,aa_spread_pct,b_segments,c_segments',
  ...results.map((r) =>
    [
      r.w,
      r.bMed.toFixed(6),
      r.cMed.toFixed(6),
      f3(r.bMed, r.cMed, `C/B ${r.w}`),
      r.gated ? (r.cb <= GATE_CB_MAX ? 'PASS' : 'FAIL') : 'control',
      r.aaSpread.toFixed(6),
      r.bSeg,
      r.cSeg,
    ].join(','),
  ),
].join('\n');
writeFileSync(CSV_PATH, `${csv}\n`, 'utf8');

process.stdout.write(`${out.join('\n')}\n`);
process.stdout.write(`[ph3c] wrote ${CSV_PATH}\n`);
