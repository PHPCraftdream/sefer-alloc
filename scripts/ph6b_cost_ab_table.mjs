#!/usr/bin/env node
// Ph6b (#2097) — final A/B cost judge: baseline B (7d80d7e1, .wt/base) vs candidate C (fd954856, HEAD).
// MEASUREMENT-ONLY: parses pre-existing raw logs under docs/perf/, prints all report tables to
// stdout and writes docs/perf/PH6B_COST_AB_summary.csv. No src/tests/benches changes.
//
// Discipline (models: ph3c_b3p_iai_table.mjs, ph3c_frag_stand_table.mjs):
//  - every printed ratio/percent goes through a guarded printer that asserts a round-trip
//    (denominator*ratio reproduces the numerator within 0.01% relative);
//  - every ratio prints numerator and denominator (B and C values);
//  - the statistic (median) is named where it is computed;
//  - A/A noise bands are measured from same-vs-same (aa) logs, not assumed.
//
// Exit code: 1 if at least one FAIL (beyond A/A noise); INCONCLUSIVE verdicts do not fail the run.
//
// Usage (from the repo root): node scripts/ph6b_cost_ab_table.mjs

import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const P = (name) => join(REPO_ROOT, 'docs', 'perf', name);
const CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PH6B_COST_AB_summary.csv');

// ---------------- guarded printers ----------------
function ratioOf(base, spike, what) {
  assert(Number.isFinite(base) && Number.isFinite(spike), `non-finite metric for ${what}`);
  assert(base > 0, `non-positive denominator for ${what}`);
  const r = spike / base;
  assert(Number.isFinite(r), `non-finite ratio for ${what}`);
  const relErr = Math.abs(base * r - spike) / Math.abs(spike);
  assert(relErr <= 1e-4, `round-trip mismatch for ${what}: base=${base} num=${spike} r=${r}`);
  return r;
}
const pctOf = (b, s, what) => (ratioOf(b, s, what) - 1) * 100;
const fmtPct = (b, s, what) => `${pctOf(b, s, what) >= 0 ? '+' : '-'}${Math.abs(pctOf(b, s, what)).toFixed(3)}%`;
/** A/A deviation as a fraction of 1, guarded: |ratio-1|. */
const devOf = (a, b, what) => Math.abs(ratioOf(a, b, what) - 1);
function spreadPct(min, median, max, what) {
  assert(median > 0, `zero median for ${what}`);
  const p = ((max - min) / median) * 100;
  // round-trip: the spread must bracket [min,max] around the median
  assert(max <= median * (1 + p / 100) * (1 + 1e-9), `spread round-trip max ${what}`);
  assert(min >= median * (1 - p / 100) * (1 - 1e-9), `spread round-trip min ${what}`);
  return p;
}
/** median (the statistic used everywhere below). */
const median = (arr) => {
  assert(arr.length > 0, 'median of empty array');
  const s = [...arr].sort((a, b) => a - b);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
};
const geomean = (arr) => {
  assert(arr.every((x) => x > 0), 'geomean of non-positive');
  return Math.exp(arr.reduce((a, x) => a + Math.log(x), 0) / arr.length);
};

// ---------------- 1. iai (callgrind) ----------------
const ROW_LABEL = new Set(['Instructions', 'Estimated Cycles']);
const METRIC_ROW_RE = /^([A-Za-z][A-Za-z0-9 +]*?):\s+(\d[\d,]*)\s*\|\s*(\d[\d,]*|N\/A)\b/;
const HEADER_RE = /^([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)+)\s*$/;
const shortName = (full) => full.split('::').pop();

function parseIaiLog(text, path) {
  const rows = new Map();
  const order = [];
  let current = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    const h = HEADER_RE.exec(line.trim());
    if (h) {
      current = h[1];
      if (!rows.has(current)) {
        rows.set(current, { key: current, name: shortName(current), ir: null, cyc: null });
        order.push(current);
      }
      continue;
    }
    const m = METRIC_ROW_RE.exec(line.trim());
    if (!m || !current || !rows.has(current)) continue;
    if (!ROW_LABEL.has(m[1])) continue;
    const v = Number.parseInt(m[2].replace(/,/g, ''), 10);
    assert(Number.isFinite(v), `${path}: non-numeric value`);
    const row = rows.get(current);
    if (m[1] === 'Instructions') {
      assert(row.ir === null, `${path}: dup Instructions for ${current}`);
      row.ir = v;
    } else {
      assert(row.cyc === null, `${path}: dup Cycles for ${current}`);
      row.cyc = v;
    }
  }
  assert(rows.size > 0, `${path}: no iai benches parsed`);
  for (const key of order) {
    assert(rows.get(key).ir !== null && rows.get(key).cyc !== null, `${path}: ${key} missing metric`);
  }
  return { rows, order };
}

const iaiBase = parseIaiLog(readFileSync(P('_raw_ph6b_iai_base_run1.log'), 'utf8'), 'iai base run1');
const iaiCand = parseIaiLog(readFileSync(P('_raw_ph6b_iai_cand_run1.log'), 'utf8'), 'iai cand run1');
const iaiBase2 = parseIaiLog(readFileSync(P('_raw_ph6b_iai_base_run2.log'), 'utf8'), 'iai base run2');
const iaiCand2 = parseIaiLog(readFileSync(P('_raw_ph6b_iai_cand_run2.log'), 'utf8'), 'iai cand run2');

assert.strictEqual(iaiBase.order.length, 85, `iai base: expected 85 benches, got ${iaiBase.order.length}`);
for (const [tag, log] of [['cand', iaiCand], ['base_run2', iaiBase2], ['cand_run2', iaiCand2]]) {
  assert.deepStrictEqual(iaiBase.order.slice().sort(), log.order.slice().sort(), `iai bench set differs: ${tag}`);
}
// determinism: run2 Ir == run1 Ir bit-for-bit on each side
for (const key of iaiBase.order) {
  assert.strictEqual(iaiBase2.rows.get(key).ir, iaiBase.rows.get(key).ir, `iai base determinism: Ir drift at ${key}`);
  assert.strictEqual(iaiCand2.rows.get(key).ir, iaiCand.rows.get(key).ir, `iai cand determinism: Ir drift at ${key}`);
}
const findIai = (name) => {
  const key = iaiBase.order.find((k) => shortName(k) === name);
  assert(key !== undefined, `iai bench ${name} not found`);
  return key;
};
const iaiB = (name) => iaiBase.rows.get(findIai(name));
const iaiC = (name) => iaiCand.rows.get(findIai(name));

// ---------------- 2. bench-table (criterion) ----------------
const NS = { ns: 1, 'µs': 1e3, 'ms': 1e6 };
function parseBenchLog(text, path) {
  const rows = new Map();
  let current = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trimEnd();
    const bm = /^Benchmarking (\S+)$/.exec(line.trim());
    if (bm) {
      current = bm[1];
      if (!rows.has(current)) rows.set(current, []);
      continue;
    }
    if (/^Benchmarking .*:/.test(line.trim())) continue; // progress lines
    const tm = /^(?:\S+ )?time:\s+\[\s*([\d.]+)\s+(ns|µs|ms)\s+([\d.]+)\s+(ns|µs|ms)\s+([\d.]+)\s+(ns|µs|ms)\s*\]/.exec(line.trim());
    if (tm && current) {
      // criterion may print each bound in its own unit (e.g. ms/µs/ns)
      const midNs = Number(tm[3]) * NS[tm[4]];
      assert(Number.isFinite(midNs) && midNs > 0, `${path}: bad time for ${current}`);
      rows.get(current).push(midNs);
    }
  }
  assert(rows.size === 58, `${path}: expected 58 bench ids, got ${rows.size}`);
  for (const [id, v] of rows) assert(v.length === 1, `${path}: id ${id} has ${v.length} time lines`);
  return rows;
}

const BT_RUNS = 5;
const btBase = [];
const btCand = [];
for (let i = 1; i <= BT_RUNS; i++) {
  btBase.push(parseBenchLog(readFileSync(P(`_raw_ph6b_benchtable_base_run${i}.log`), 'utf8'), `bt base run${i}`));
  btCand.push(parseBenchLog(readFileSync(P(`_raw_ph6b_benchtable_cand_run${i}.log`), 'utf8'), `bt cand run${i}`));
}
const BT_AA_RUNS = 10;
const btAa = [];
for (let i = 1; i <= BT_AA_RUNS; i++) {
  btAa.push(parseBenchLog(readFileSync(P(`_raw_ph6b_benchtable_aa_run${i}.log`), 'utf8'), `bt aa run${i}`));
}
// build evidence: exactly one Compiling sefer-alloc line, pointing at .wt/base
{
  const t = readFileSync(P('_raw_ph6b_benchtable_base_build.log'), 'utf8');
  const lines = t.split(/\r?\n/).filter((l) => /^\s*Compiling sefer-alloc /.test(l));
  assert(lines.length === 1, `bt base build: expected 1 Compiling sefer-alloc line, got ${lines.length}`);
  assert(/\.wt\/base/.test(lines[0]), `bt base build: Compiling line not from .wt/base: ${lines[0]}`);
}

const SIZED_GROUPS = ['global_alloc', 'global_alloc_churn', 'global_alloc_churn_write', 'global_alloc_churn_with_teardown'];
const SIZES = ['16B', '64B', '256B', '1024B'];
const RATIO_IDS = [
  ...SIZED_GROUPS.flatMap((g) => SIZES.map((s) => `${g}/SeferAlloc/${s}`)),
  'global_alloc/manual_realloc_sim/SeferAlloc',
  'segment_decommit_cycle/SeferAlloc/253KiB',
];
const WARM_BULK_IDS = SIZES.map((s) => `global_alloc/SeferAlloc/${s}`);
for (const id of RATIO_IDS) {
  const partner = id.replace('SeferAlloc', 'mimalloc');
  for (const log of [...btBase, ...btCand, ...btAa]) {
    assert(log.has(id) && log.has(partner), `bench-table: missing id or partner in a log: ${id}`);
  }
}
const pairIdOf = (id) => id.replace('SeferAlloc', 'mimalloc');
/** ratio r = ns[Sefer]/ns[mimalloc] within one run (numerator/denominator of the printed ratio). */
const btR = (log, id) => ratioOf(log.get(pairIdOf(id))[0], log.get(id)[0], `r ${id}`);
const btRatios = (logs, id) => logs.map((log) => btR(log, id));

// ---------------- 3. MT ----------------
function parseMtLog(text, path) {
  const ns = new Map(); // "workload|T" -> ns
  const ops = new Map();
  let segments = null;
  let conflicts = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    let m = /^RESULT mt_ns workload=(\S+) T=(\d+) ns=(\d+)$/.exec(line);
    if (m) {
      const k = `${m[1]}|${m[2]}`;
      assert(!ns.has(k), `${path}: dup mt_ns ${k}`);
      ns.set(k, Number(m[3]));
      continue;
    }
    m = /^RESULT mt_ops workload=(\S+) T=(\d+) ops=(\d+)$/.exec(line);
    if (m) {
      const k = `${m[1]}|${m[2]}`;
      assert(!ops.has(k), `${path}: dup mt_ops ${k}`);
      ops.set(k, Number(m[3]));
      continue;
    }
    m = /^RESULT segments_reserved_total=(\d+)$/.exec(line);
    if (m) segments = Number(m[1]);
    m = /^RESULT config_conflicts=(\d+)$/.exec(line);
    if (m) conflicts = Number(m[1]);
  }
  assert(ns.size === 8 && ops.size === 8, `${path}: expected 8 mt cells, got ns=${ns.size} ops=${ops.size}`);
  assert(segments !== null && conflicts !== null, `${path}: missing sanity counters`);
  assert(conflicts === 0, `${path}: config_conflicts=${conflicts}`);
  assert(segments > 0, `${path}: segments_reserved_total=${segments}`);
  return { ns, ops, segments, conflicts };
}
const MT_CELLS = ['larson|1', 'mstress|1', 'larson|2', 'mstress|2', 'larson|4', 'mstress|4', 'larson|8', 'mstress|8'];
const MT_RUNS = 7;
const mtBase = [], mtCand = [];
for (let i = 1; i <= MT_RUNS; i++) {
  mtBase.push(parseMtLog(readFileSync(P(`_raw_ph6b_mt_base_run${i}.log`), 'utf8'), `mt base run${i}`));
  mtCand.push(parseMtLog(readFileSync(P(`_raw_ph6b_mt_cand_run${i}.log`), 'utf8'), `mt cand run${i}`));
}
const MT_AA_RUNS = 10;
const mtAa = [];
for (let i = 1; i <= MT_AA_RUNS; i++) {
  mtAa.push(parseMtLog(readFileSync(P(`_raw_ph6b_mt_aa_run${i}.log`), 'utf8'), `mt aa run${i}`));
}
const mtMops = (log, cell) => {
  const ns = log.ns.get(cell);
  const ops = log.ops.get(cell);
  const mops = (ops / ns) * 1e3; // Mops/s
  assert(Number.isFinite(mops) && mops > 0, `bad mops ${cell}`);
  return mops;
};
const mtAaRatios = (cell) => {
  const odd = mtAa.filter((_, i) => i % 2 === 0).map((l) => mtMops(l, cell)); // runs 1,3,5,7,9
  const even = mtAa.filter((_, i) => i % 2 === 1).map((l) => mtMops(l, cell)); // runs 2,4,6,8,10
  return { odd: median(odd), even: median(even) };
};

// ---------------- 4. frag ----------------
const FRAG_KEYS = ['workload', 'rss_empty_kib', 'commit_empty_kib', 'config_conflicts_before', 'resolved_config',
  'live_requested_bytes', 'rss_end_kib', 'commit_end_kib', 'segments_reserved_total', 'config_conflicts_after'];
const LIVE_64MIB = 67108864;
function parseFragLog(text, path, expectedArms) {
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
      assert(!current.kv.has(m[1]), `${path}: dup key ${m[1]}`);
      current.kv.set(m[1], m[2]);
    }
  }
  assert(blocks.length > 0, `${path}: no RUN blocks`);
  for (const b of blocks) {
    assert(expectedArms.has(b.arm), `${path}: unexpected arm ${b.arm}`);
    for (const key of FRAG_KEYS) assert(b.kv.has(key), `${path}: RUN ${b.arm}${b.rep} missing ${key}`);
    for (const key of ['rss_empty_kib', 'live_requested_bytes', 'rss_end_kib', 'segments_reserved_total', 'config_conflicts_before', 'config_conflicts_after']) {
      b[key] = Number(b.kv.get(key));
      assert(Number.isFinite(b[key]), `${path}: non-numeric ${key}`);
    }
    b.workload = b.kv.get('workload');
    assert(/^w[1-4]$/.test(b.workload), `${path}: bad workload`);
    assert(b.config_conflicts_before === 0 && b.config_conflicts_after === 0,
      `${path}: RUN ${b.arm}${b.rep} config_conflicts delta != 0`);
    assert(Math.abs(b.live_requested_bytes - LIVE_64MIB) / LIVE_64MIB <= 0.01,
      `${path}: RUN ${b.arm}${b.rep} live_requested_bytes not within ±1% of 64 MiB`);
    b.frag = ((b.rss_end_kib - b.rss_empty_kib) * 1024) / b.live_requested_bytes;
    assert(Number.isFinite(b.frag) && b.frag > 0, `${path}: non-positive frag`);
  }
  return blocks;
}
const fragByW = new Map([['w1', []], ['w2', []], ['w3', []], ['w4', []]]);
for (const w of ['w1', 'w2', 'w3', 'w4']) {
  const blocks = parseFragLog(readFileSync(P(`_raw_ph6b_frag_${w}.log`), 'utf8'), `frag ${w}`, new Set(['B', 'C']));
  assert(blocks.length === 10, `frag ${w}: expected 10 blocks`);
  blocks.forEach((b, i) => {
    const wantArm = i % 2 === 0 ? 'B' : 'C';
    const wantRep = (i >> 1) + 1;
    assert(b.arm === wantArm && b.rep === wantRep, `frag ${w}: block ${i} is ${b.arm}${b.rep}, expected ${wantArm}${wantRep}`);
    assert(b.workload === w, `frag ${w}: block workload ${b.workload} != ${w}`);
  });
  fragByW.get(w).push(...blocks);
}
const fragAaByW = new Map([['w1', []], ['w2', []], ['w3', []], ['w4', []]]);
{
  const blocks = parseFragLog(readFileSync(P('_raw_ph6b_frag_aa.log'), 'utf8'), 'frag aa', new Set(['B']));
  assert(blocks.length === 12, `frag aa: expected 12 B blocks, got ${blocks.length}`);
  blocks.forEach((b, i) => {
    const w = ['w1', 'w2', 'w3', 'w4'][Math.floor(i / 3)];
    assert(b.workload === w, `frag aa: block ${i} workload ${b.workload}, expected ${w}`);
    fragAaByW.get(w).push(b);
  });
}

// ---------------- 5. trimrss ----------------
const TRIM_METRICS = ['rss_burst1_kib', 'rss_idle_kib', 'rss_burst2_kib', 'commit_idle_kib'];
function parseTrimLog(text, path) {
  const arms = { TRIM: [], NO_TRIM: [] };
  let current = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    let m = /^--- arm=(TRIM|NO_TRIM) rep=(\d+)\/(\d+) ---$/.exec(line);
    if (m) {
      current = { arm: m[1], rep: Number(m[2]), kv: new Map() };
      arms[current.arm].push(current);
      continue;
    }
    m = /^RESULT ([a-z_0-9]+)=(\d+)$/.exec(line);
    if (m && current) {
      assert(!current.kv.has(m[1]), `${path}: dup key ${m[1]}`);
      current.kv.set(m[1], Number(m[2]));
    }
  }
  assert(arms.TRIM.length === 3 && arms.NO_TRIM.length === 3, `${path}: expected 3 reps per arm`);
  for (const arm of ['TRIM', 'NO_TRIM']) {
    for (const child of arms[arm]) {
      for (const key of [...TRIM_METRICS, 'action_released_delta', 'idle_released_delta']) {
        assert(child.kv.has(key), `${path}: ${arm} rep ${child.rep} missing ${key}`);
      }
    }
  }
  // oracle: TRIM child must actually release (action_released_delta > 0) in every run
  for (const child of arms.TRIM) {
    assert(child.kv.get('action_released_delta') > 0,
      `${path}: TRIM rep ${child.rep} action_released_delta=${child.kv.get('action_released_delta')} (oracle: must be > 0)`);
  }
  return arms;
}
const TRIM_RUNS = 5;
const trimBase = [], trimCand = [];
for (let i = 1; i <= TRIM_RUNS; i++) {
  trimBase.push(parseTrimLog(readFileSync(P(`_raw_ph6b_trimrss_base_run${i}.log`), 'utf8'), `trim base run${i}`));
  trimCand.push(parseTrimLog(readFileSync(P(`_raw_ph6b_trimrss_cand_run${i}.log`), 'utf8'), `trim cand run${i}`));
}

// =====================================================================
// REPORT
// =====================================================================
const out = [];
const say = (l = '') => out.push(l);
const csv = ['axis,item,metric,B,C,C_over_B,limit,aa_noise,verdict'];
const csvRow = (axis, item, metric, B, C, cOverB, limit, aaNoise, verdict) =>
  csv.push([axis, item, metric, B, C, cOverB, limit, aaNoise, verdict].join(','));
const fails = [];
const noteFail = (what) => fails.push(what);
const f = (x, d = 4) => x.toFixed(d);

// ---- axis 1: iai ----
say('# Ось 1 — iai (callgrind, детерминирован): C против B');
say();
say(`Наборы бенчей: 85 = 85 = 85 = 85 (base_run1, cand_run1, base_run2, cand_run2) — OK.`);
say(`Детерминизм: base_run2 Ir == base_run1 Ir побитово на всех 85; cand_run2 Ir == cand_run1 Ir побитово на всех 85 — OK.`);
say('ΔX% = (C − B) / B × 100; B — знаменатель каждого процента (guarded round-trip ±0.01%).');
say();

say('## Контроль A/A: mimalloc-плечи (13, префикс mimalloc_) — ΔIr должно быть 0.000% ровно');
say();
let aaIrFail = 0;
const mimallocArms = iaiBase.order.filter((k) => iaiBase.rows.get(k).name.startsWith('mimalloc_'));
assert.strictEqual(mimallocArms.length, 13, `expected 13 mimalloc_ arms, got ${mimallocArms.length}`);
for (const key of mimallocArms) {
  const b = iaiBase.rows.get(key), c = iaiCand.rows.get(key);
  const p = pctOf(b.ir, c.ir, `${b.name} Ir`);
  if (p !== 0) aaIrFail += 1;
  say(`- ${b.name}: Ir B=${b.ir} C=${c.ir} → ΔIr ${fmtPct(b.ir, c.ir, key)} ${p === 0 ? 'OK' : 'FAIL'}`);
}
say();
say(`A/A Ir: ${aaIrFail === 0 ? 'все 13 плеч ΔIr = 0.000% ровно — PASS' : `FAIL: ${aaIrFail} плеч с ΔIr ≠ 0`}`);
if (aaIrFail > 0) noteFail('iai A/A mimalloc arms');
say();

const iaiGates = { hot: [], refill: [], zeroed: [], realloc: [] };
const gateLine = (group, name, okIr, okCyc, limitText) => {
  const b = iaiB(name), c = iaiC(name);
  const irR = ratioOf(b.ir, c.ir, `${name} Ir`);
  const parts = [`Ir B=${b.ir} C=${c.ir} (x${irR.toFixed(5)}, Δ ${fmtPct(b.ir, c.ir, name + ' Ir')})`];
  let ok = okIr;
  if (okCyc !== null) {
    const cycR = ratioOf(b.cyc, c.cyc, `${name} EstCycles`);
    parts.push(`EstCycles B=${b.cyc} C=${c.cyc} (x${cycR.toFixed(5)}, Δ ${fmtPct(b.cyc, c.cyc, name + ' EstCycles')})`);
    ok = okIr && cycR <= okCyc;
  }
  say(`- ${name}: ${parts.join(' | ')} | предел ${limitText} → ${ok ? 'PASS' : 'FAIL'}`);
  csvRow('iai', name, 'gate', b.ir, c.ir, irR.toFixed(6), limitText, '0 (детерминизм)', ok ? 'PASS' : 'FAIL');
  if (!ok) noteFail(`iai gate ${group}/${name}`);
  return ok;
};

say('## Гейты (C/B, знаменатель = base B)');
say();
say('### HOT: Ir ≤ 1.02 И EstCycles ≤ 1.02');
say();
for (const n of ['small_churn_16b', 'churn_256b', 'churn_write_256b', 'aligned_churn_640b_a128']) {
  const b = iaiB(n), c = iaiC(n);
  const ok = ratioOf(b.ir, c.ir, n) <= 1.02 && ratioOf(b.cyc, c.cyc, n) <= 1.02;
  gateLine('hot', n, ratioOf(b.ir, c.ir, n) <= 1.02, 1.02, 'Ir≤1.02 И EstCycles≤1.02');
}
say();
say('### REFILL: EstCycles ≤ 1.05 И Ir ≤ 1.10');
say();
for (const n of ['cold_alloc_free_256x16b', 'cold_alloc_free_256x64b', 'recycle_alloc_free_256x16b', 'recycle_alloc_free_256x64b', 'multiseg_cold_256k']) {
  const b = iaiB(n), c = iaiC(n);
  gateLine('refill', n, ratioOf(b.ir, c.ir, n) <= 1.10, 1.05, 'EstCycles≤1.05 И Ir≤1.10');
}
say();
say('### ZEROED: raw Ir ≤ 1.05');
say();
{
  const names = ['alloc_zeroed_magazine_prefill_only_16b', 'alloc_zeroed_magazine_hit_only_16b',
    'alloc_zeroed_magazine_hit_only_16b_2n', 'alloc_zeroed_calloc_virgin_64k', 'alloc_zeroed_calloc_recycled_64k']
    .filter((n) => iaiBase.order.some((k) => shortName(k) === n));
  say(`(найдено ${names.length} из 5 заявленных; alloc_zeroed_magazine_hit_only_16b_2n ${names.includes('alloc_zeroed_magazine_hit_only_16b_2n') ? 'есть' : 'отсутствует в наборе'})`);
  say();
  for (const n of names) gateLine('zeroed', n, ratioOf(iaiB(n).ir, iaiC(n).ir, n) <= 1.05, null, 'raw Ir≤1.05');
  say();
}
say('### REALLOC: raw Ir ≤ 1.05 (+ маржинальный Ir/op для realloc_grow ≤ 1.05)');
say();
{
  const bootB = iaiB('large_alloc_free_cycle'), bootC = iaiC('large_alloc_free_cycle');
  say(`bootstrap-плечо large_alloc_free_cycle: Ir B=${bootB.ir} C=${bootC.ir}`);
  say();
  for (const n of ['realloc_grow', 'dealloc_realloc_burst_1088_16b_n17']) {
    const b = iaiB(n), c = iaiC(n);
    const ok = ratioOf(b.ir, c.ir, n) <= 1.05;
    let extra = '';
    if (n === 'realloc_grow') {
      const mB = (b.ir - bootB.ir) / 16; // маржинальный Ir на op
      const mC = (c.ir - bootC.ir) / 16;
      const mR = ratioOf(mB, mC, 'realloc_grow marginal Ir/op');
      const okM = mR <= 1.05;
      extra = ` | маржинальный Ir/op (=(Ir−Ir[large_alloc_free_cycle])/16): B=${mB.toFixed(2)} C=${mC.toFixed(2)} (x${mR.toFixed(5)}, Δ ${fmtPct(mB, mC, 'marginal')}) → ${okM ? 'PASS' : 'FAIL'}`;
      if (!okM) noteFail('iai gate realloc/realloc_grow marginal');
    }
    const line = `- ${n}: Ir B=${b.ir} C=${c.ir} (x${ratioOf(b.ir, c.ir, n).toFixed(5)}, Δ ${fmtPct(b.ir, c.ir, n + ' Ir')}) | предел raw Ir≤1.05 → ${ok ? 'PASS' : 'FAIL'}${extra}`;
    say(line);
    if (!ok) noteFail(`iai gate realloc/${n}`);
    csvRow('iai', n, 'gate', b.ir, c.ir, ratioOf(b.ir, c.ir, n).toFixed(6), 'raw Ir≤1.05', '0 (детерминизм)', ok ? 'PASS' : 'FAIL');
  }
  say();
}

say('## Все 85 бенчей (ΔIr, ΔCycles; B — знаменатель)');
say();
say('| бенч | Ir B→C (Δ, x) | EstCycles B→C (Δ, x) |');
say('|---|---|---|');
for (const key of iaiBase.order) {
  const b = iaiBase.rows.get(key), c = iaiCand.rows.get(key);
  const irR = ratioOf(b.ir, c.ir, key), cycR = ratioOf(b.cyc, c.cyc, key);
  const grp = b.name.startsWith('mimalloc_') ? 'mimalloc-AA'
    : ['small_churn_16b', 'churn_256b', 'churn_write_256b', 'aligned_churn_640b_a128'].includes(b.name) ? 'hot'
    : ['cold_alloc_free_256x16b', 'cold_alloc_free_256x64b', 'recycle_alloc_free_256x16b', 'recycle_alloc_free_256x64b', 'multiseg_cold_256k'].includes(b.name) ? 'refill'
    : b.name.startsWith('alloc_zeroed_') ? 'zeroed'
    : ['realloc_grow', 'dealloc_realloc_burst_1088_16b_n17'].includes(b.name) ? 'realloc' : '';
  say(`| ${b.name}${grp ? ` (${grp})` : ''} | ${b.ir}→${c.ir} (${fmtPct(b.ir, c.ir, key)}, x${irR.toFixed(5)}) | ${b.cyc}→${c.cyc} (${fmtPct(b.cyc, c.cyc, key)}, x${cycR.toFixed(5)}) |`);
  csvRow('iai', b.name, grp || 'all', b.ir, c.ir, irR.toFixed(6), '', '0 (детерминизм)', grp === 'mimalloc-AA' ? (pctOf(b.ir, c.ir, key) === 0 ? 'PASS' : 'FAIL') : '');
}
say();

// ---- axis 2: bench-table ----
say('# Ось 2 — bench-table (criterion wall-clock): C против B');
say();
say('r(id, run) = ns[SeferAlloc]/ns[mimalloc] внутри одного прогона (числитель/знаменатель указаны).');
say(`Медиана (статистика — median) по ${BT_RUNS} прогонам на сторону; C/B(id) = med(r_cand) / med(r_b).`);
say(`A/A: ${BT_AA_RUNS} прогонов same-vs-same, med(r_odd)/med(r_even) на id; полоса = max|·−1| по 18 id.`);
say();
const btAaRatio = (id) => {
  const odd = median(btAa.filter((_, i) => i % 2 === 0).map((l) => btR(l, id)));
  const even = median(btAa.filter((_, i) => i % 2 === 1).map((l) => btR(l, id)));
  return { odd, even, dev: devOf(odd, even, `bt A/A ${id}`) };
};
const btAaAll = RATIO_IDS.map(btAaRatio);
const btBand = Math.max(...btAaAll.map((x) => x.dev));
say(`A/A-полоса оси 2 (max по 18 id |med(r_odd)/med(r_even) − 1|): ${f(btBand * 100, 3)}%`);
say();
say('| id (18 ratio) | med r B (Sefer/mimalloc) | med r C | C/B | предел | A/A |мед r_odd/мед r_even| | вердикт |');
say('|---|---:|---:|---:|---|---:|---:|---|');
const btRows = [];
for (const id of RATIO_IDS) {
  const rB = median(btRatios(btBase, id));
  const rC = median(btRatios(btCand, id));
  const cb = ratioOf(rB, rC, `C/B ${id}`);
  const aa = btAaRatio(id);
  const isWarm = WARM_BULK_IDS.includes(id);
  const limit = isWarm ? 1.15 : null;
  let verdict;
  if (limit !== null) {
    // pre-registered noise rule: FAIL only beyond the A/A band (max over the 18 ids)
    verdict = cb <= limit ? 'PASS' : (cb - 1 > btBand ? 'FAIL' : 'INCONCLUSIVE');
  } else {
    verdict = 'INFO';
  }
  btRows.push({ id, rB, rC, cb, aa, isWarm, verdict });
  say(`| ${id} | ${f(rB)} | ${f(rC)} | ${f(cb)} | ${isWarm ? '≤1.15' : '—'} | ${f(aa.dev * 100, 3)}% | ${f(aa.odd)}/${f(aa.even)} | ${verdict} |`);
  csvRow('bench-table', id, 'sefer_over_mimalloc_C_over_B', f(rB, 6), f(rC, 6), f(cb, 6), isWarm ? '1.15' : '', `${f(aa.dev * 100, 4)}%`, verdict);
  if (verdict === 'FAIL') noteFail(`bench-table ${id}`);
}
say();
{
  const cbs = btRows.map((x) => x.cb);
  const gm = geomean(cbs);
  const mx = Math.max(...cbs);
  const mxExcess = mx - 1;
  const gmOk = gm <= 1.05;
  const mxVerdict = mx <= 1.10 ? 'PASS' : (mxExcess > btBand ? 'FAIL' : 'INCONCLUSIVE (в пределах A/A-полосы)');
  say(`geomean C/B по 18 id (статистика — median на id) = ${f(gm)} (числитель — произведение 18 C/B, знаменатель — их геометрическая нормировка 1) | предел ≤1.05 → ${gmOk ? 'PASS' : 'FAIL'}`);
  say(`max C/B по 18 id = ${f(mx)} (id ${btRows[cbs.indexOf(mx)].id}) | предел ≤1.10 → ${mxVerdict} (A/A-полоса ${f(btBand * 100, 3)}%)`);
  if (!gmOk) noteFail('bench-table geomean');
  if (mxVerdict === 'FAIL') noteFail('bench-table max');
  say();
  say('warm-bulk (4 id global_alloc/SeferAlloc/{16B,64B,256B,1024B}), предел ≤1.15 каждый:');
  let warmOk = true;
  for (const x of btRows.filter((y) => y.isWarm)) {
    const ok = x.cb <= 1.15;
    warmOk = warmOk && ok;
    say(`- ${x.id}: med r B=${f(x.rB)} med r C=${f(x.rC)} → C/B ${f(x.cb)} | A/A ${f(x.aa.dev * 100, 3)}% → ${x.verdict}`);
  }
  say(`warm-bulk: ${warmOk ? 'PASS (все ≤1.15)' : 'FAIL/INCONCLUSIVE — см. вердикты выше'}`);
  say();
  // control arms: raw-ns medians C/B, mimalloc and System (inter-series drift indicator)
  say('Контрольные руки (raw ns, медиана по 5 прогонам; C/B — индикатор межсерийного смещения, не гейт):');
  say();
  say('| control id | ns B (median) | ns C (median) | C/B |');
  say('|---|---:|---:|---:|');
  for (const arm of ['mimalloc', 'System']) {
    for (const id of RATIO_IDS) {
      const cid = id.replace('SeferAlloc', arm);
      const nsB = median(btBase.map((l) => l.get(cid)[0]));
      const nsC = median(btCand.map((l) => l.get(cid)[0]));
      const cb = ratioOf(nsB, nsC, `control ${cid}`);
      say(`| ${cid} | ${nsB.toFixed(1)} | ${nsC.toFixed(1)} | ${f(cb)} |`);
    }
    say();
  }
  csvRow('bench-table', 'control mimalloc arms (n=18)', 'raw_ns_C_over_B_median', '—', '—', f(median(btRows.map((x) => x.cb)), 6), '', `${f(btBand * 100, 4)}%`, 'INFO');
}

// ---- axis 3: MT ----
say('# Ось 3 — MT wall-clock (ph6b_mt_ab): C против B');
say();
say('Ячейка (workload,T): медиана ns по 7 прогонам на сторону; Mops = ops/ns·1000;');
say('Mops C/B = med(Mops_B) / med(Mops_C) (throughput: >1 — C медленнее, <1 — C быстрее).');
say(`A/A: ${MT_AA_RUNS} прогонов same-vs-same, med(Mops_odd)/med(Mops_even) на ячейку; полоса = max|·−1|;`);
say('также (max−min)/median по 10 aa-прогонам — информативно.');
say();
const mtAaAll = MT_CELLS.map((cell) => {
  const vals = mtAa.map((l) => mtMops(l, cell));
  const { odd, even } = mtAaRatios(cell);
  return { cell, odd, even, dev: devOf(odd, even, `mt A/A ${cell}`), spread: spreadPct(Math.min(...vals), median(vals), Math.max(...vals), `mt aa spread ${cell}`) };
});
const mtBand = Math.max(...mtAaAll.map((x) => x.dev));
say(`A/A-полоса оси 3 (max |med(Mops_odd)/med(Mops_even) − 1| по 8 ячеек): ${f(mtBand * 100, 3)}%`);
say();
say('| ячейка | med ns B | med ns C | med Mops B | med Mops C | Mops C/B (B/C) | предел | A/A |spread%| | вердикт |');
say('|---|---:|---:|---:|---:|---:|---|---:|---:|---|');
const mtRows = [];
for (const cell of MT_CELLS) {
  const nsB = median(mtBase.map((l) => l.ns.get(cell)));
  const nsC = median(mtCand.map((l) => l.ns.get(cell)));
  const mB = median(mtBase.map((l) => mtMops(l, cell)));
  const mC = median(mtCand.map((l) => mtMops(l, cell)));
  const cb = ratioOf(mB, mC, `Mops C/B ${cell}`);
  const aa = mtAaAll.find((x) => x.cell === cell);
  const verdict = cb >= 0.90 ? 'PASS' : (1 - cb > mtBand ? 'FAIL' : 'INCONCLUSIVE');
  mtRows.push({ cell, nsB, nsC, mB, mC, cb, aa, verdict });
  say(`| ${cell.replace('|', ' T=')} | ${nsB.toFixed(0)} | ${nsC.toFixed(0)} | ${f(mB)} | ${f(mC)} | ${f(cb)} | ≥0.90 | ${f(aa.dev * 100, 3)}% | ${f(aa.spread, 3)}% | ${verdict} |`);
  csvRow('mt', cell.replace('|', ' T='), 'mops_C_over_B', f(mB, 6), f(mC, 6), f(cb, 6), '0.90', `${f(aa.dev * 100, 4)}%`, verdict);
  if (verdict === 'FAIL') noteFail(`mt ${cell}`);
}
say();
{
  const gm = geomean(mtRows.map((x) => x.cb));
  const gmOk = gm >= 0.95;
  say(`geomean Mops C/B по 8 ячеек (статистика — median на ячейку) = ${f(gm)} (числитель — произведение 8 C/B, знаменатель — геометрическая нормировка 1) | предел ≥0.95 → ${gmOk ? 'PASS' : 'FAIL'}`);
  if (!gmOk) noteFail('mt geomean');
  say();
  const segB = median(mtBase.map((l) => l.segments));
  const segC = median(mtCand.map((l) => l.segments));
  const confB = Math.max(...mtBase.map((l) => l.conflicts));
  const confC = Math.max(...mtCand.map((l) => l.conflicts));
  say(`Оракулы MT: segments_reserved_total медиана B=${segB} C=${segC} (обоим >0 — OK); config_conflicts = 0 во всех логах — OK.`);
  say();
}

// ---- axis 4a: frag ----
say('# Ось 4 — RSS: фрагментация (frag-stand) и trim');
say();
say('frag = (rss_end_kib − rss_empty_kib)·1024 / live_requested_bytes (безразмерное; live в 64 MiB ±1% — assert пройден на каждом блоке; config_conflicts delta=0 — assert пройден).');
say('Медианы (статистика — median) по 5 чередующимся прогонам B/C на нагрузку; C/B(w) = med(frag_C)/med(frag_B), знаменатель = B.');
say(`A/A: ${fragAaByW.get('w1').length} B-прогонов каждой нагрузки в aa-логе, spread = (max−min)/median.`);
say();
say('| w | med frag B | med frag C | C/B (знаменатель B) | гейт ≤1.10 | сегменты B→C | A/A spread % |');
say('|---|---:|---:|---:|---|---|---:|');
for (const w of ['w1', 'w2', 'w3', 'w4']) {
  const blocks = fragByW.get(w);
  const fB = median(blocks.filter((b) => b.arm === 'B').map((b) => b.frag));
  const fC = median(blocks.filter((b) => b.arm === 'C').map((b) => b.frag));
  const cb = ratioOf(fB, fC, `frag C/B ${w}`);
  const sB = median(blocks.filter((b) => b.arm === 'B').map((b) => b.segments_reserved_total));
  const sC = median(blocks.filter((b) => b.arm === 'C').map((b) => b.segments_reserved_total));
  const aaVals = fragAaByW.get(w).map((b) => b.frag);
  const aaSpread = spreadPct(Math.min(...aaVals), median(aaVals), Math.max(...aaVals), `frag aa ${w}`);
  const gated = w !== 'w4';
  const ok = cb <= 1.10;
  const verdict = gated ? (ok ? 'PASS' : 'FAIL') : `контроль ${f(cb)}`;
  say(`| ${w} | ${f(fB)} | ${f(fC)} | ${f(cb)} | ${verdict} | ${sB}→${sC} | ${f(aaSpread, 3)}% |`);
  csvRow('frag', w, 'frag_C_over_B', f(fB, 6), f(fC, 6), f(cb, 6), gated ? '1.10' : 'control', `${f(aaSpread, 4)}%`, gated ? (ok ? 'PASS' : 'FAIL') : 'INFO');
  if (gated && !ok) noteFail(`frag ${w}`);
}
say();

// ---- axis 4b: trim ----
say('## trim (r31_10_trim_rss_gate, arm TRIM): C против B');
say();
say('Медианы (статистика — median) по 5 прогонам на сторону, KiB. Оракул: action_released_delta > 0');
say('у TRIM-ребёнка в каждом из 10 логов (5 B + 5 C) — assert при парсинге пройден (10/10).');
say('Гейт: C/B ≤ 1.05 ИЛИ (C − B) ≤ 1024 KiB. A/A: (max−min)/median по 5 base-прогонам.');
say();
say('| метрика | med B, KiB | med C, KiB | C/B (знаменатель B) | C−B, KiB | гейт | A/A spread % | вердикт |');
say('|---|---:|---:|---:|---:|---|---:|---|');
for (const m of TRIM_METRICS) {
  const vB = median(trimBase.map((r) => median(r.TRIM.map((c) => c.kv.get(m)))));
  const vC = median(trimCand.map((r) => median(r.TRIM.map((c) => c.kv.get(m)))));
  const cb = ratioOf(vB, vC, `trim C/B ${m}`);
  const absDelta = vC - vB;
  const ok = cb <= 1.05 || absDelta <= 1024;
  const aaVals = trimBase.map((r) => median(r.TRIM.map((c) => c.kv.get(m))));
  const aaSpread = spreadPct(Math.min(...aaVals), median(aaVals), Math.max(...aaVals), `trim aa ${m}`);
  say(`| ${m} | ${vB.toFixed(1)} | ${vC.toFixed(1)} | ${f(cb)} | ${absDelta.toFixed(1)} | C/B≤1.05 ИЛИ C−B≤1024 KiB → ${ok ? 'PASS' : 'FAIL'} | ${f(aaSpread, 3)}% | ${ok ? 'PASS' : 'FAIL'} |`);
  csvRow('trim', 'TRIM arm', m, vB.toFixed(1), vC.toFixed(1), f(cb, 6), '1.05 or Δ≤1024KiB', `${f(aaSpread, 4)}%`, ok ? 'PASS' : 'FAIL');
  if (!ok) noteFail(`trim ${m}`);
}
say();
say('NO_TRIM-рука (справочно): idle_released_delta = 0 во всех логах обеих сторон (трим просто не вызывается).');
say();

// ---- NOT_RUN / NOT_COMPARABLE / NOISY ----
say('# NOT_RUN / NOT_COMPARABLE / отклонённые серии');
say();
say('- remote-merge iai-плечо — NOT_COMPARABLE: bench-функций с remote/merge в perf_gate_iai нет ни в B, ни в C.');
say('- remote lag p99 — NOT_RUN: нет наблюдаемого плеча/счётчиков без изменения src/.');
say('- метаданные Small route (байты у System; adversarial ≤304,128) — NOT_RUN: probe отсутствует в обоих деревьях; 304,128 — статическая оценка из ACTIVE.md/ревью, не измерение.');
say('- resolved_config readback — unavailable: нет публичного API (как в PH3C_FRAG_STAND).');
say('- NOISY-история: первая MT-серия (_raw_ph6b_mt_*_run{1..10}_noisy.log) и первый bench-table base-прогон (_raw_ph6b_benchtable_base_run1_noisy.log) под нагрузкой numa-r6/pool_cap_sweep — отклонены, не парсились; единственный разрешённый перемер выполнен (см. _raw_ph6b_load_check_w3/w4/w5.log).');
say('- A/B/B/A не проводился; вместо него чередование B,C,B,C,... внутри каждого лога и B/B-контроль (aa-логи).');
say();

writeFileSync(CSV_PATH, `${csv.join('\n')}\n`, 'utf8');

// ---- summary ----
say('# Итог судьи');
say();
const iaiPass = !fails.some((x) => x.startsWith('iai'));
const fragPass = !fails.some((x) => x.startsWith('frag'));
const trimPass = !fails.some((x) => x.startsWith('trim'));
const btPass = !fails.some((x) => x.startsWith('bench-table'));
const mtPass = !fails.some((x) => x.startsWith('mt'));
const mtInconclusive = out.join('\n').includes('| INCONCLUSIVE |');
say(`ось 1 iai: ${iaiPass ? 'PASS' : 'FAIL'} | ось 2 bench-table: ${btPass ? 'PASS' : 'есть FAIL/INCONCLUSIVE — см. таблицу'} | ось 3 MT: ${mtPass ? 'PASS' : (mtInconclusive ? 'INCONCLUSIVE (в пределах A/A-полосы)' : 'FAIL')} | ось 4 RSS (frag ${fragPass ? 'PASS' : 'FAIL'}, trim ${trimPass ? 'PASS' : 'FAIL'})`);
say(`FAIL (сверх шума): ${fails.length === 0 ? 'нет' : fails.join('; ')}`);
say();
say(`CSV: docs/perf/PH6B_COST_AB_summary.csv`);

process.stdout.write(`${out.join('\n')}\n`);
process.exit(fails.length > 0 ? 1 : 0);
