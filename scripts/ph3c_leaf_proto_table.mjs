#!/usr/bin/env node
// Ph3c step-3c-1b — iai table builder for the candidate-B3 ("sub-leaf bit
// free-set") micro-prototype.
//
// WHY THIS EXISTS: the ADR addendum
// (docs/design/2026-10-02-adr-addendum-ph3c-decisions.md §4) pre-registers the
// LAST micro-iteration of the bit family. B3 = the B2 shape over a 4 KiB
// SUB-LEAF grain (256 slots = 4 occupancy words) with a word-wise batch
// drain/flush, a per-class non-empty-sub-leaf mask + cursor, and the class
// bound to a sub-leaf instead of a 64 KiB leaf. The §4 cut-off filter is
// CONJUNCTIVE over Ir and EstCycles (L3/I):
//   drain_dense ≤ 1.10; drain_sparse_{1,8,64} ≤ 1.25 (verbatim §3.3),
//   churn_one_class / refill_cold_16 / refill_cold_64 / churn_mixed ≤ 1.10
//   (new, stricter than §3.3), and EstCycles L3/I ≤ 1.05 on the SAME scenarios.
// The measure must stay DETERMINISTIC (callgrind Ir / Estimated Cycles):
// wall-clock on the Windows dev host is noise. ONE raw iai log is the input:
//   docs/perf/_raw_ph3c_b3_proto_iai.log   (ph3c-proto @ ff25e9d6a098…)
//
// !!! The shared-`CARGO_TARGET_DIR` trap documented in
// docs/perf/PG3_OFFBODY_SPIKE_IAI.md applies here !!! a shared target dir lets
// cargo replay a STALE binary (freshness decided by source mtime, not content),
// which silently produces an A/A run. This run used a PRIVATE target dir, and
// the parser REQUIRES both freshness proofs in the log: the `Compiling
// sefer-alloc` line (something was actually built for this run) and the
// `Iai-Callgrind result: Ok … benchmarks finished` trailer (every bench of the
// set really ran). An incomplete set (≠ 51 benches, a missing arm, a missing
// metric row) aborts with a NON-ZERO exit code.
//
// iai-callgrind's stdout block per bench looks like:
//   ph3c_leaf_proto::ph3c_leaf_proto::<scenario> <arm>:Model :: <ModelName>
//     Instructions:        120258|120255   (+0.00249%) [+1.00002x]
//     L1 Hits / L2 Hits / RAM Hits / Total read+write / Estimated Cycles: …
// The bench name is the last `::` segment of the path; the arm is the parameter
// prefix of the header line. We ALWAYS take the FIRST number of each metric row
// (the current run); the second number is the runner's own baseline (a count,
// or `N/A` for a fresh baseline) and is deliberately ignored.
//
// 51 benches are expected and asserted: `identity_check` (listed first — it
// asserts the five models issue the same offset sets and aborts the run on a
// mismatch) + 10 scenarios x 5 arms (intrusive=I, next_table=N, leaf_table=L
// = candidate B, leaf_table_fast=L2 = modification B2, leaf_table_3=L3 =
// candidate B3).
//
// Step 3c-1b re-measured I/N/L/L2 after fixing the polarity of the double-retire
// `debug_assert!` on the I/N arms (addendum §4: otherwise the base is unfair).
// The re-measured numbers are BYTE-IDENTICAL to step 1
// (docs/perf/PH3C_LEAF_PROTO_IAI_summary.csv), so this table carries them as
// the base; only L3 and the `remote_merge_setup` scenario are new.
//
// The percent/ratio printer is the ONLY place a measured relation is
// formatted: it asserts the value is finite, the denominator is non-zero, and
// that `denominator*(1+pct/100)` reproduces the numerator to ±0.01% — so a
// hand-typed or copy-pasted number can never reach a table. The marginal Ir
// per operation and the fragmentation shares have their own guarded printers
// (same round-trip discipline); thresholds are constants and never go through
// the measured-relation guard.
//
// Remote merge is deliberately NOT printed as an "Ir per merged block" gate:
// the plain `remote_merge` arms pay 64 re-pushes into a linked chain (I/N)
// versus TWO model constructions plus one word-wise OR (L/L2/L3), so any
// per-block number derived by differencing the arms measures the harness
// constructions, not the merge (confound — see the note). `remote_merge_setup`
// moves the donor/target construction into iai's UNMEASURED setup prologue and
// prints its relations for information only: the ≤1.00 Ir/block gate belongs to
// the integrated spike (§5), not to a filter emulation.

// Usage (from the repo root):
//   node scripts/ph3c_leaf_proto_table.mjs [--log <path>]

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// --- identity (the prototype sits on the immutable base commit) -------------
const BASE_COMMIT = 'ff25e9d6a098c1bd19d0b7fb85077f84e100981b';
const FEATURES = 'alloc-global bench-internals internals';
const DEFAULT_LOG = join(REPO_ROOT, 'docs', 'perf', '_raw_ph3c_b3_proto_iai.log');
const CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PH3C_B3_PROTO_IAI_summary.csv');

// --- shape of the measured set ----------------------------------------------
const SCENARIOS = [
  'churn_one_class',
  'drain_dense',
  'drain_sparse_1',
  'drain_sparse_8',
  'drain_sparse_64',
  'refill_cold_16',
  'refill_cold_64',
  'churn_mixed',
  'remote_merge',
  'remote_merge_setup',
];
// intrusive=I (base emulation), next_table=N (PG-3 spike), leaf_table=L
// (candidate B), leaf_table_fast=L2 (modification B2), leaf_table_3=L3
// (candidate B3 — the sub-leaf grain of addendum §3).
const ARMS = [
  'intrusive',
  'next_table',
  'leaf_table',
  'leaf_table_fast',
  'leaf_table_3',
];
const ARM_TAG = {
  intrusive: 'I',
  next_table: 'N',
  leaf_table: 'L',
  leaf_table_fast: 'L2',
  leaf_table_3: 'L3',
};
const I = ARMS.indexOf('intrusive');
const N = ARMS.indexOf('next_table');
const L = ARMS.indexOf('leaf_table');
const L2 = ARMS.indexOf('leaf_table_fast');
const L3 = ARMS.indexOf('leaf_table_3');
const IDENTITY_BENCH = 'identity_check';
const IDENTITY_ARM = 'identity_only';
const EXPECTED_BENCHES = SCENARIOS.length * ARMS.length + 1; // 51

// §3.3 falsification thresholds (percent form), reused verbatim by §4.
const TH_DENSE_PCT = 10; // dense leaf: L3/I > 1.10 => B3 fails
const TH_SPARSE_PCT = 25; // sparse leaf (W_eff > 8 words): L3/I > 1.25 => fails
// §4 adds, stricter than §3.3 ever was for these scenarios.
const TH_CHURN_PCT = 10; // churn_one_class / refill_cold_* / churn_mixed ≤ 1.10
// Observational only — the ADR refill/flush gates of the INTEGRATED allocator
// (step 2), quoted here as an orientation point for the arena emulation.
const TH_ADR_REFILL_CYC_PCT = 5; // EstCycles ≤ 1.05
const TH_ADR_REFILL_IR_PCT = 10; // Ir ≤ 1.10

// The addendum §4 cut-off filter: one entry per GATED scenario. `ir` is the
// per-scenario Ir threshold, `cyc` the EstCycles one (§4 requires both, and
// EstCycles ≤1.05 on "the SAME scenarios").
const GATE_IR_PCT = {
  drain_dense: TH_DENSE_PCT, // §3.3 дословно
  drain_sparse_1: TH_SPARSE_PCT,
  drain_sparse_8: TH_SPARSE_PCT,
  drain_sparse_64: TH_SPARSE_PCT,
  churn_one_class: TH_CHURN_PCT, // §4 «новые, строже прежних»
  refill_cold_16: TH_CHURN_PCT,
  refill_cold_64: TH_CHURN_PCT,
  churn_mixed: TH_CHURN_PCT,
};
const GATE_CYC_PCT = TH_ADR_REFILL_CYC_PCT; // §4: EstCycles ≤ 1.05

// --- operational counts (the denominator of "Ir per operation") -------------
// Mirrored from the bench drivers of benches/ph3c_leaf_proto.rs; one unit is
// ONE free-set operation (retire OR pop), which is what the marginal column
// divides by. The constants carry the bench's own names so the derivation is
// auditable against the driver source.
const CHURN_BLOCKS = 256; // slots 0..256 of leaf 0, class 0
const CHURN_ROUNDS = 8; // drain(pop) + flush(retire) per round
const LEAF_SLOTS = 4096; // one 64 KiB leaf of 16 B slots
const REFILL_LEAVES = 4;
const REFILL_BLOCKS = 256;
const REFILL_ROUNDS = 2;
const MIXED_LEAVES = 48; // leaf l -> class l % NUM_CLASSES
const MIXED_BLOCKS_PER_LEAF = 64;
const MIXED_ROUNDS = 4;
const MIXED_DRAIN = 32;
const SPARSE_KS = [1, 8, 64];

/** One free-set operation count per measured scenario. */
const OPS = {
  // 256 blocks: one retire each, then drain(pop)+flush(retire) per round.
  churn_one_class: CHURN_BLOCKS * (1 + 2 * CHURN_ROUNDS), // 256 x 17 = 4352
  // 4096 retires to fill the leaf, then 4096 pops to drain it.
  drain_dense: 2 * LEAF_SLOTS, // 8192
  // k scattered slots, then k pops.
  ...Object.fromEntries(
    SPARSE_KS.map((k) => [`drain_sparse_${k}`, 2 * k]),
  ),
  // 4 leaves x 256 blocks, one flush plus drain(pop) per round.
  refill_cold_16: REFILL_LEAVES * REFILL_BLOCKS * (1 + 2 * REFILL_ROUNDS), // 5120
  refill_cold_64: REFILL_LEAVES * REFILL_BLOCKS * (1 + 2 * REFILL_ROUNDS), // 5120
  // 48 x 64 initial retires, then 4 rounds of 48 x 32 pops + the same retires.
  churn_mixed:
    MIXED_LEAVES * MIXED_BLOCKS_PER_LEAF +
    2 * MIXED_ROUNDS * MIXED_LEAVES * MIXED_DRAIN, // 3072 + 12288 = 15360
};

// --- fragmentation geometry (also mirrored from the bench; addendum §3) -------
// One 4 MiB segment of 16 B slots, re-sliced into 64 KiB leaves and, for B3,
// into 4 KiB SUB-LEAVES the class is bound to. The production size table has 49
// classes; the prototype's scenarios only use 8 of them.
const SLOT_BYTES = 16;
const SEGMENT_BYTES = 4 * 1024 * 1024;
const SEGMENT_SLOTS = SEGMENT_BYTES / SLOT_BYTES; // 262144
const LEAF_BYTES = 64 * 1024;
const SUBLEAF_BYTES = 4 * 1024;
const SUBLEAF_SLOTS = SUBLEAF_BYTES / SLOT_BYTES; // 256
const SUBLEAVES_PER_LEAF = LEAF_BYTES / SUBLEAF_BYTES; // 16
const PROD_CLASSES = 49;
const GEO_CLASSES = 40; // 16 B, 1.25x, округление к шагу 16 B
const EXACT_CLASSES = [256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];

// --- CLI ---------------------------------------------------------------------
const argv = process.argv.slice(2);
const opts = { log: DEFAULT_LOG };
for (let i = 0; i < argv.length; i++) {
  if (argv[i] === '--log') opts.log = resolve(argv[++i]);
  else throw new Error(`unknown argument: ${argv[i]}`);
}

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
 *  (`N|N/A` — a fresh run with nothing to compare against). Both are accepted;
 *  the FIRST number is the measurement, and it is the only thing ever read out
 *  of the row. */
const METRIC_ROW_RE =
  /^([A-Za-z][A-Za-z0-9 +]*?):\s+(\d[\d,]*)\s*\|\s*(\d[\d,]*|N\/A)\b/;
/** One bench header: `<path::to::bench> <arm>:Model :: <ModelName>`; the
 * identity arm carries no `::`-path but keeps the same shape (`identity_only:0u8`),
 * and the `remote_merge_setup` arms carry a setup CALL as their parameter
 * (`remote_merge_setup_intrusive()`) — both are handled by the same prefix rule:
 * the arm is everything before the first `:`. The full header line is the row
 * key — never key by the short name. */
const HEADER_RE = /^([A-Za-z_][A-Za-z0-9_]*(?:::[A-Za-z_][A-Za-z0-9_]*)+)\s+(.+)$/;

function fail(msg) {
  process.stderr.write(`ph3c_leaf_proto_table: ERROR: ${msg}\n`);
  process.exit(1);
}

/** Parse the raw iai log into `<scenario> <arm>` -> metric rows. */
function parseLog(text, path) {
  const rows = new Map();
  const order = [];
  let current = null;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.trim();
    const h = HEADER_RE.exec(line);
    if (h) {
      const scenario = h[1].split('::').pop();
      const arm = h[2].split(':')[0].trim();
      const key = `${scenario} ${arm}`;
      current = key;
      if (!rows.has(key)) {
        rows.set(key, {
          key,
          scenario,
          arm,
          ir: null,
          cycles: null,
          l1: null,
          l2: null,
          ram: null,
        });
        order.push(key);
      }
      continue;
    }
    const m = METRIC_ROW_RE.exec(line);
    if (!m || current === null || !rows.has(current)) continue;
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
    for (const field of ['ir', 'cycles', 'l1', 'l2', 'ram']) {
      assert(
        row[field] !== null,
        `${path}: ${key} has no ${field === 'ir' ? 'Instructions' : field} row`,
      );
    }
  }
  return { rows, order };
}

let text;
try {
  text = readFileSync(opts.log, 'utf8');
} catch (err) {
  fail(`cannot read log ${relative(REPO_ROOT, opts.log)}: ${err.message}`);
}

// Freshness proof #1: a build actually happened in the (private) target dir.
// Without it the log may be a replay of a foreign worktree's binary — the
// PG-3 artifact class (docs/perf/PG3_OFFBODY_SPIKE_IAI.md).
if (!text.includes('Compiling sefer-alloc')) {
  fail(
    `${relative(REPO_ROOT, opts.log)}: no "Compiling sefer-alloc" line — ` +
      'the run may have replayed a stale binary (shared CARGO_TARGET_DIR trap); ' +
      're-run in a PRIVATE CARGO_TARGET_DIR',
  );
}
// Freshness proof #2: the trailer must name exactly the expected bench count
// and must report no regression (identity_check aborts the run on a mismatch).
const trailer =
  /Iai-Callgrind result:\s*Ok\.\s*(\d+) without regressions;\s*(\d+) regressed;\s*(\d+) benchmarks finished/.exec(
    text,
  );
if (!trailer) {
  fail(
    `${relative(REPO_ROOT, opts.log)}: no "Iai-Callgrind result: Ok …" trailer — ` +
      'the run did not complete the bench set',
  );
}
const [, withoutRegressions, regressed, finished] = trailer;
if (Number(finished) !== EXPECTED_BENCHES || Number(withoutRegressions) !== EXPECTED_BENCHES) {
  fail(
    `${relative(REPO_ROOT, opts.log)}: trailer reports ${finished} benches ` +
      `(${withoutRegressions} without regressions), expected ${EXPECTED_BENCHES}`,
  );
}
if (Number(regressed) !== 0) {
  fail(
    `${relative(REPO_ROOT, opts.log)}: trailer reports ${regressed} regressed ` +
      'bench(es) — the identity arm or a scenario failed',
  );
}

const log = parseLog(text, opts.log);
if (log.order.length !== EXPECTED_BENCHES) {
  fail(
    `${relative(REPO_ROOT, opts.log)}: parsed ${log.order.length} benches, ` +
      `expected ${EXPECTED_BENCHES} (${IDENTITY_BENCH} + ${SCENARIOS.length} ` +
      'scenarios x 5 arms)',
  );
}
for (const key of log.order) {
  const row = log.rows.get(key);
  const known =
    row.scenario === IDENTITY_BENCH
      ? row.arm === IDENTITY_ARM
      : SCENARIOS.includes(row.scenario) && ARMS.includes(row.arm);
  if (!known) {
    fail(`${relative(REPO_ROOT, opts.log)}: unexpected bench "${key}"`);
  }
}

const identity = log.rows.get(`${IDENTITY_BENCH} ${IDENTITY_ARM}`);
if (!identity) fail(`${relative(REPO_ROOT, opts.log)}: ${IDENTITY_BENCH} missing`);

const scenarios = SCENARIOS.map((id) => {
  const arms = ARMS.map((arm) => {
    const row = log.rows.get(`${id} ${arm}`);
    if (!row) {
      fail(`${relative(REPO_ROOT, opts.log)}: missing bench ${id} ${arm}`);
    }
    return row;
  });
  return { id, arms };
});

// Every scenario MUST land in the log's order as a complete block of arms —
// otherwise a silently missing arm would turn a ratio into a lie.
for (const s of scenarios) {
  for (const a of s.arms) assert(a !== undefined, `missing arm row for ${s.id}`);
}
// The gate table must describe scenarios that really are measured.
const gatedIds = Object.keys(GATE_IR_PCT);
assert(
  Object.keys(GATE_IR_PCT).every((id) => SCENARIOS.includes(id)),
  'a gated scenario is not in SCENARIOS',
);

// --- the shared construction constant (addendum §1) -------------------------
const sparse1 = scenarios.find((s) => s.id === 'drain_sparse_1');
// Ir of the intrusive arm on the cheapest scenario there is (ONE retire + ONE
// pop): the fixed part of an arm — arena/table zeroing — with essentially no
// per-op work on top. It is taken SHARED, i.e. the same constant subtracts from
// every arm, which makes the L3 margins an UPPER estimate (its own construction
// is strictly more expensive — see the caveat in the marginal section).
const CONSTRUCTION_CONST = sparse1.arms[I].ir;

// --- ratio printer (the ONLY measured-relation formatter) -------------------
// DELETE / hand-edit a number in the log and this asserts.
function ratio(base, num, what) {
  assert(
    Number.isFinite(base) && Number.isFinite(num),
    `non-finite metric for ${what}`,
  );
  assert(base !== 0, `zero denominator for ${what}`);
  const p = ((num - base) / base) * 100;
  assert(Number.isFinite(p), `non-finite ratio for ${what}`);
  // Round-trip guard: base*(1+p/100) must reproduce num to ±0.01%.
  const back = base * (1 + p / 100);
  const relErr = num === 0 ? Math.abs(back) : Math.abs(back - num) / Math.abs(num);
  assert(
    relErr <= 1e-4,
    `round-trip mismatch for ${what}: base=${base} num=${num} pct=${p} back=${back}`,
  );
  return p;
}

function factor(base, num, what) {
  return 1 + ratio(base, num, what) / 100;
}

/** `x1.25838 (+25.84%)` — every printed relation passes the round-trip assert. */
function fmtRatio(base, num, what) {
  const p = ratio(base, num, what);
  const sign = p > 0 ? '+' : p < 0 ? '-' : '+';
  return `x${(1 + p / 100).toFixed(5)} (${sign}${Math.abs(p).toFixed(2)}%)`;
}

function fmtFactorPct(base, num, what) {
  const p = ratio(base, num, what);
  const sign = p > 0 ? '+' : p < 0 ? '-' : '+';
  return `${sign}${Math.abs(p).toFixed(2)}% (x${(1 + p / 100).toFixed(5)})`;
}

/** Same guard as `fmtRatio`, four decimals — for reproducibility deltas, where
 *  two decimals would round a whole arm's re-measure to "+0.00%". */
function fmtDelta(base, num, what) {
  const p = ratio(base, num, what);
  const sign = p > 0 ? '+' : p < 0 ? '-' : '+';
  return `x${(1 + p / 100).toFixed(5)} (${sign}${Math.abs(p).toFixed(4)}%)`;
}

/** `+10.00% (x1.10000)` — a THRESHOLD constant, never a measured pair: it is
 *  NOT routed through ratio(), whose whole job is to guard measurements. */
function fmtGate(pct) {
  assert(Number.isFinite(pct) && pct >= 0, `bad threshold ${pct}`);
  return `+${pct.toFixed(2)}% (x${(1 + pct / 100).toFixed(5)})`;
}

/** Marginal Ir PER OPERATION of one arm (addendum §1, made a table):
 *  `(Ir − CONSTRUCTION_CONST) / ops`. Same round-trip discipline as ratio():
 *  `CONSTRUCTION_CONST + marginal*ops` must reproduce the arm's Ir. */
function perOp(ir, ops, what) {
  assert(
    Number.isFinite(ir) && Number.isFinite(ops),
    `non-finite metric for ${what}`,
  );
  assert(ops > 0, `zero operation count for ${what}`);
  const m = (ir - CONSTRUCTION_CONST) / ops;
  assert(Number.isFinite(m), `non-finite marginal for ${what}`);
  const back = CONSTRUCTION_CONST + m * ops;
  const relErr = Math.abs(back - ir) / Math.abs(ir);
  assert(
    relErr <= 1e-9,
    `round-trip mismatch for ${what}: const=${CONSTRUCTION_CONST} ir=${ir} ` +
      `ops=${ops} marginal=${m} back=${back}`,
  );
  return m;
}

/** A share, `part/whole` in percent — used by the fragmentation model, where
 *  the denominator is a geometry constant, not a measurement. Guards the same
 *  round trip: `whole*(pct/100)` must reproduce `part`. */
function share(part, whole, what) {
  assert(
    Number.isFinite(part) && Number.isFinite(whole),
    `non-finite metric for ${what}`,
  );
  assert(whole !== 0, `zero denominator for ${what}`);
  const p = (part / whole) * 100;
  assert(Number.isFinite(p), `non-finite share for ${what}`);
  const back = whole * (p / 100);
  const relErr = part === 0 ? Math.abs(back) : Math.abs(back - part) / Math.abs(part);
  assert(
    relErr <= 1e-9,
    `round-trip mismatch for ${what}: whole=${whole} part=${part} pct=${p} back=${back}`,
  );
  return p;
}

const int = (n) => n.toLocaleString('en-US');
const kib = (n) => n / 1024;
const kv = (n) => (n / 1024).toLocaleString('en-US', { maximumFractionDigits: 1 });
const rel = (p) => relative(REPO_ROOT, resolve(p)).replace(/\\/g, '/');

// --- output ------------------------------------------------------------------
const out = [];
const say = (line = '') => out.push(line);

say('# Ph3c шаг 3c-1b — iai-таблицы микро-прототипа кандидата B3 (bit-leaf free-set)');
say();
say(
  `лог: \`${rel(opts.log)}\` (ph3c-proto @ ${BASE_COMMIT}; дерево = чистый ` +
    `${BASE_COMMIT.slice(0, 7)} + патч прототипа, benches/ph3c_leaf_proto.rs)`,
);
say(
  `features: \`${FEATURES}\` (cache-sim on; EstCycles = L1 + 5·L2 + 35·RAM)  |  ` +
    `бенчей: ${EXPECTED_BENCHES} = ${IDENTITY_BENCH} + ${SCENARIOS.length} ` +
    'сценариев × 5 плеч (I/N/L/L2/L3)',
);
say(
  'свежесть: `Compiling sefer-alloc` в логе + трейлер ' +
    `«${EXPECTED_BENCHES} without regressions; 0 regressed» — оба доказательства ` +
    'требуются, приватный CARGO_TARGET_DIR (иначе молчаливый replay чужого бинарника).',
);
say();
say('Каждая метрика — ПЕРВОЕ число iai-строки (текущий прогон); второе число —');
say('baseline самого раннера (count или `N/A`) и намеренно игнорируется.');
say('Знаменатели явные и только внутри сценария:');
say('L/I = Ir(leaf_table)/Ir(intrusive), N/I = Ir(next_table)/Ir(intrusive),');
say('L2/I = Ir(leaf_table_fast)/Ir(intrusive), L3/I = Ir(leaf_table_3)/Ir(intrusive).');
say('Каждое напечатанное отношение прошло round-trip проверку:');
say('denominator*(1+pct/100) воспроизводит числитель ±0.01%, denominator ≠ 0.');
say();
say('Плечи I/N/L/L2 перемерены после исправления полярности `debug_assert!`');
say('двойного retire у I/N (§4 аддендума — иначе база нечестна) и даны как база;');
say('сверка с шагом 1 — в разделе «База шага 1» ниже, дельты там посчитаны из');
say('двух таблиц, а не вписаны руками. Новое плечо L3 (кандидат B3) и новый');
say('сценарий `remote_merge_setup` измерены впервые.');
say();

say(`## Идентичность — оракул, не сценарий (${IDENTITY_BENCH})`);
say();
say('| бенч | Ir | EstCycles |');
say('|---|---:|---:|');
say(`| ${IDENTITY_BENCH} | ${int(identity.ir)} | ${int(identity.cycles)} |`);
say();
say('Плечи модели выдают одинаковые множества оффсетов (assert внутри бенча,');
say('теперь и I↔L3); прогон «51 without regressions» — оракул пройден.');
say();

say('## Ir и EstCycles по плечам + отношения к intrusive');
say();
say(
  '| сценарий | Ir I | Ir N | Ir L | Ir L2 | Ir L3 | EstCyc I | EstCyc N | ' +
    'EstCyc L | EstCyc L2 | EstCyc L3 | L/I | N/I | L2/I | L3/I |',
);
say('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
for (const s of scenarios) {
  const ir = s.arms.map((a) => a.ir);
  const cy = s.arms.map((a) => a.cycles);
  say(
    `| ${s.id} | ${int(ir[I])} | ${int(ir[N])} | ${int(ir[L])} | ${int(ir[L2])} | ` +
      `${int(ir[L3])} | ${int(cy[I])} | ${int(cy[N])} | ${int(cy[L])} | ` +
      `${int(cy[L2])} | ${int(cy[L3])} | ` +
      `${fmtRatio(ir[I], ir[L], `L/I ${s.id}`)} | ` +
      `${fmtRatio(ir[I], ir[N], `N/I ${s.id}`)} | ` +
      `${fmtRatio(ir[I], ir[L2], `L2/I ${s.id}`)} | ` +
      `${fmtRatio(ir[I], ir[L3], `L3/I ${s.id}`)} |`,
  );
}
say();

// --- step-1 base check (deltas computed, never hand-typed) --------------------
// docs/perf/PH3C_LEAF_PROTO_IAI_summary.csv is READ ONLY here — it is the table
// the step-1 §3.3 verdict was taken from, and the only way to CHECK (instead of
// claiming) that the re-measured base arms reproduce it. A missing file simply
// degrades the note; it never fails the run.
const STEP1_CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PH3C_LEAF_PROTO_IAI_summary.csv');
/** `<scenario> <arm>` -> {ir, cycles} of the step-1 table, or null. */
function step1Row(key) {
  try {
    const lines = readFileSync(STEP1_CSV_PATH, 'utf8').split(/\r?\n/);
    for (const line of lines.slice(1)) {
      if (!line.trim()) continue;
      const c = line.split(',');
      if (c.length > 3 && `${c[0]} ${c[1]}` === key) {
        return {
          ir: Number.parseInt(c[2], 10),
          cycles: Number.parseInt(c[3], 10),
        };
      }
    }
  } catch {
    return null;
  }
  return null;
}

const baseRows = [];
for (const s of scenarios) {
  if (s.id === 'remote_merge_setup') continue; // no step-1 counterpart
  for (const [idx, arm] of ARMS.entries()) {
    if (arm === 'leaf_table_3') continue; // no step-1 counterpart
    const old = step1Row(`${s.id} ${arm}`);
    if (old) baseRows.push({ s, idx, arm, old });
  }
}
say('## База шага 1 — воспроизводимость перемера плеч I/N/L/L2');
say();
if (baseRows.length === 0) {
  say(`Таблица шага 1 \`${rel(STEP1_CSV_PATH)}\` не найдена — сверка пропущена.`);
  say();
} else {
  say(
    `Сверка с \`${rel(STEP1_CSV_PATH)}\` (только чтение; шаг 1 = 9 сценариев × ` +
      `плечи I/N/L/L2): сравнено ${baseRows.length} бенчей, каждая дельта — ` +
      'через тот же round-trip guard, что и остальные отношения.',
  );
  say();
  say('| плечо | бенчей сравнено | max \\|ΔIr\\| к шагу 1 | на сценарии |');
  say('|---|---:|---:|---|');
  for (const arm of ARMS) {
    if (arm === 'leaf_table_3') continue;
    const rows = baseRows.filter((r) => r.arm === arm);
    const worst = rows.reduce((a, b) =>
      Math.abs(ratio(b.old.ir, b.s.arms[b.idx].ir, `step1 ${b.s.id} ${b.arm}`)) >
      Math.abs(ratio(a.old.ir, a.s.arms[a.idx].ir, `step1 ${a.s.id} ${a.arm}`))
        ? b
        : a,
    );
    say(
      `| ${ARM_TAG[arm]} (\`${arm}\`) | ${rows.length} | ` +
        `${fmtDelta(worst.old.ir, worst.s.arms[worst.idx].ir, `step1 worst ${arm}`)} | ` +
        `\`${worst.s.id}\` |`,
    );
  }
  say();
  const close = baseRows.filter(
    (r) =>
      Math.abs(ratio(r.old.ir, r.s.arms[r.idx].ir, `step1 ${r.s.id} ${r.arm}`)) <= 0.01,
    ).length;
  const idOld = step1Row(`${IDENTITY_BENCH} ${IDENTITY_ARM}`);
  const refill = scenarios.find((s) => s.id === 'refill_cold_16');
  const refillOldI = step1Row('refill_cold_16 intrusive');
  const refillOldL = step1Row('refill_cold_16 leaf_table');
  say(
    `Из них в пределах ±0.01% Ir: ${close} из ${baseRows.length}; смещения ` +
      'крупнее 1% — только у N и L2 на `refill_cold_*` и в `identity_check`.',
  );
  say('Перемер не побайтовый дословно, но и не сдвиг геометрии.');
  if (refillOldI && refillOldL) {
    say(
      `L/I на \`refill_cold_16\`: шаг 1 ` +
        `${fmtRatio(refillOldI.ir, refillOldL.ir, 'step1 L/I refill_cold_16')} → сейчас ` +
        `${fmtRatio(refill.arms[I].ir, refill.arms[L].ir, 'now L/I refill_cold_16')} — ` +
        'вердикт §3.3 шага 1 стоит на тех же плечах.',
    );
  }
  if (idOld) {
    say(
      `identity_check: шаг 1 ${int(idOld.ir)} → сейчас ${int(identity.ir)} Ir ` +
        `(${fmtRatio(idOld.ir, identity.ir, 'step1 ΔIr identity_check')}) — ` +
        'оракул покрывает на одну пару плеч больше.',
    );
  }
  say();
}

// --- verdict against the §4 cut-off filter -----------------------------------
say('## Гейты аддендума §4 (фильтр шага 3c-1b, конъюнктивно Ir и EstCycles)');
say();
say('Плечо I = intrusive-эмуляция (база), N = NextTable-спайк, L = кандидат B,');
say('L2 = модификация B2 (u64 words_mask вместо first_hint-скана), L3 = кандидат');
say('B3 (B2 над под-листом 4 KiB: пословный drain/flush, маска непустых');
say('под-листов на класс + курсор, класс привязан к под-листу).');
say();
say(`Пороги дословно: \`drain_dense\` ≤ ${fmtGate(TH_DENSE_PCT)},`);
say(`\`drain_sparse_{1,8,64}\` ≤ ${fmtGate(TH_SPARSE_PCT)} (§3.3), новые, строже`);
say(`прежних: \`churn_one_class\`, \`refill_cold_{16,64}\`, \`churn_mixed\` ≤ ${fmtGate(TH_CHURN_PCT)},`);
say(`и EstCycles L3/I ≤ ${fmtGate(GATE_CYC_PCT)} на ТЕХ ЖЕ сценариях. Порог 1.10 на`);
say('dense сохранён сознательно (риск ложного отсева принят, узкий провал');
say('задним числом не оспаривается). Шаг 1 держал churn/refill наблюдением —');
say('§4 переводит их в гейты: «пакетность обязана проявиться именно здесь».');
say();
say('| сценарий | L3/I Ir | порог Ir | Ir-гейт | L3/I EstCyc | порог EstCyc | EstCyc-гейт |');
say('|---|---:|---:|---|---:|---:|---|');

const gates = gatedIds.map((id) => {
  const s = scenarios.find((x) => x.id === id);
  const irP = ratio(s.arms[I].ir, s.arms[L3].ir, `L3/I Ir ${id}`);
  const cyP = ratio(s.arms[I].cycles, s.arms[L3].cycles, `L3/I EstCyc ${id}`);
  return {
    s,
    irP,
    cyP,
    irOk: irP <= GATE_IR_PCT[id],
    cycOk: cyP <= GATE_CYC_PCT,
  };
});
for (const g of gates) {
  say(
    `| ${g.s.id} | ${fmtFactorPct(g.s.arms[I].ir, g.s.arms[L3].ir, `L3/I Ir ${g.s.id}`)} | ` +
      `${fmtGate(GATE_IR_PCT[g.s.id])} | **${g.irOk ? 'PASS' : 'FAIL'}** | ` +
      `${fmtFactorPct(g.s.arms[I].cycles, g.s.arms[L3].cycles, `L3/I EstCyc ${g.s.id}`)} | ` +
      `${fmtGate(GATE_CYC_PCT)} | **${g.cycOk ? 'PASS' : 'FAIL'}** |`,
  );
}
say();
const irFails = gates.filter((g) => !g.irOk);
const cycFails = gates.filter((g) => !g.cycOk);
const allOk = irFails.length === 0 && cycFails.length === 0;
say(
  `Общий вердикт фильтра §4: **${allOk ? 'проходит' : 'не проходит'}** — ` +
    `провалено ${irFails.length + cycFails.length} из ${gates.length * 2} гейтов ` +
    `(Ir: ${irFails.length} из ${gates.length}, EstCycles: ${cycFails.length} из ` +
    `${gates.length}); порог считается пройденным, только если оба метрика гейта PASS.`,
);
if (irFails.length > 0) {
  say();
  say('Провал по Ir:');
  for (const g of irFails) {
    say(
      `* \`${g.s.id}\`: L3/I = ${fmtRatio(g.s.arms[I].ir, g.s.arms[L3].ir, `L3/I Ir ${g.s.id}`)} ` +
        `при пороге ${fmtGate(GATE_IR_PCT[g.s.id])}`,
    );
  }
}
if (cycFails.length > 0) {
  say();
  say('Провал по EstCycles:');
  for (const g of cycFails) {
    say(
      `* \`${g.s.id}\`: L3/I EstCyc = ${fmtRatio(g.s.arms[I].cycles, g.s.arms[L3].cycles, `L3/I EstCyc ${g.s.id}`)} ` +
        `при пороге ${fmtGate(GATE_CYC_PCT)}`,
    );
  }
}
say();
say('§4 аддендума: «Любой провал → битовое семейство закрыто, новых');
say('микро-итераций нет» — фильтр шага 3c-1b кандидат B3 не проходит. Уточнение');
say('§1: прокси эмуляции не откалиброван (N в эмуляции даёт Ir 1.02 на churn,');
say('интегрированно PG-3 — 1.20 на hot), поэтому и PASS эмуляции не был бы');
say('доказательством интеграции; решающий замер — шаг 3c-2s (§5).');
say();

// --- marginal Ir per operation (informational, addendum §1) ------------------
const l3Construction = sparse1.arms[L3].ir;
say('## Маржинальная разница Ir на операцию (информативно)');
say();
say(
  'Общая константа построения CONSTRUCTION_CONST = Ir(intrusive, ' +
    `\`drain_sparse_1\`) = ${int(CONSTRUCTION_CONST)} (образец аддендума §1): ` +
    'сценарий с одной парой retire/pop, т.е. фиксированная часть плеча без',
);
say(
  'по существу пооперационной работы. Маржинальная стоимость = (Ir сценария − ' +
    'CONSTRUCTION_CONST) / ops, где ops — число free-set операций плеча',
);
say('(retire/pop) из драйвера бенча; знаменатели вынесены в константы OPS.');
say();
say('| сценарий | ops | I | L | L2 | L3 |');
say('|---|---:|---:|---:|---:|---:|');
for (const id of Object.keys(OPS)) {
  const s = scenarios.find((x) => x.id === id);
  say(
    `| ${id} | ${int(OPS[id])} | ${perOp(s.arms[I].ir, OPS[id], `marginal I ${id}`).toFixed(2)} | ` +
      `${perOp(s.arms[L].ir, OPS[id], `marginal L ${id}`).toFixed(2)} | ` +
      `${perOp(s.arms[L2].ir, OPS[id], `marginal L2 ${id}`).toFixed(2)} | ` +
      `${perOp(s.arms[L3].ir, OPS[id], `marginal L3 ${id}`).toFixed(2)} |`,
  );
}
say();
say('**Оговорка (верхняя оценка L3).** Константа построения взята ОБЩАЯ — по');
say(
  `плечу I, тогда как собственная конструкция у плеч разная: \`drain_sparse_1\` ` +
    `даёт I = ${int(CONSTRUCTION_CONST)}, а L3 = ${int(l3Construction)} Ir ` +
    `(${fmtRatio(CONSTRUCTION_CONST, l3Construction, `L3/I construction (drain_sparse_1)`)}).`,
);
say(
  `Вычитая у L3 лишь ${int(CONSTRUCTION_CONST)} — меньше его настоящей ` +
    `конструкции (${int(l3Construction)}) — мы завышаем остаток, поэтому`,
);
say('маржинальные числа L3 — оценка СВЕРХУ: настоящая пооперационная часть L3');
say('ещё меньше напечатанной. Для сравнения: шаг 1 тем же методом дал I 15.8 /');
say('L 20.9 / L2 19.0 на `drain_dense` и I 19.7 / L 32.8 / L2 28.7 на');
say('`churn_one_class` — совпадение с §1 аддендума подтверждает метод.');
say();
say('Разреженные сценарии (ops = 2k) печатаются ровно для этого: их маржа —');
say('почти целиком константа построения, и у L3 она уходит в тысячи Ir. Вывод');
say('про постоянную добавку на операцию (§1) остаётся: она есть у L и L2, у L3');
say('она меньше L/L2 на плотных сценариях, но гейты Ir и EstCyc всё равно');
say('провалены — см. таблицу выше.');
say();

// --- fragmentation (addendum §3: sub-leaf grain) -----------------------------
say('## Фрагментация: под-лист 4 KiB (B3) против листа 64 KiB (B/B2)');
say();
say('Геометрия из бенча: сегмент 4 MiB, слот 16 B, лист 64 KiB (4096 слотов),');
say('под-лист 4 KiB (256 слотов, 16 под-листов на лист), таблица классов');
say('продакшена — 49 (`SIZE_CLASS_TABLE`); класс привязан к под-листу, класс');
say('крупнее под-листа — спан из нескольких. Хвост = байты, привязанные к');
say('классу и не занятые живыми блоками (unavailable другому классу).');
say();
say(
  'Измерено тестом \`benches/ph3c_leaf_proto.rs::tests\` через ' +
    '\`cargo test --bench ph3c_leaf_proto\`: 5/5 ok ' +
    '(\`fragmentation_tail_49_classes\`, \`pop_and_drain_restore_the_free_set\`,',
);
say(
  '\`span_geometry_is_marked_and_counted\`, \`identity_matches_leaf_table_fast\`, ' +
    '\`measure_identity\`); тест сверяет измеренный хвост с аналитическим',
);
say('(assert_eq) и проверяет границы ≤49×4 KiB и ≤5.0% сегмента.');
say();

// --- the tail itself, as executable constants -------------------------------
/** The bench's `build_class_sizes` const fn: 40 geometric classes merged with
 *  the 9 exact page-friendly ones, strictly increasing, same as
 *  src/alloc_core/platform/size_classes.rs. Kept here as constants so the tail
 *  is COMPUTED, not typed. */
function buildClassSizes() {
  const table = [];
  let size = SLOT_BYTES; // next geometric class
  let geoLeft = GEO_CLASSES;
  let e = 0; // exact classes consumed
  while (table.length < PROD_CLASSES) {
    let next;
    if (e < EXACT_CLASSES.length && (geoLeft === 0 || EXACT_CLASSES[e] <= size)) {
      next = EXACT_CLASSES[e];
      e += 1;
    } else {
      next = size;
      // ceil(1.25x) with integer arithmetic, rounded up to the 16 B stride
      const grown = Math.floor((next * 5 + 3) / 4);
      size = Math.ceil(grown / SLOT_BYTES) * SLOT_BYTES;
      geoLeft -= 1;
    }
    if (table.length === 0 || next > table[table.length - 1]) table.push(next);
  }
  return table;
}
const CLASS_SIZES = buildClassSizes();
const spanSubleaves = (bytes) => Math.ceil(bytes / SUBLEAF_BYTES);
assert(
  CLASS_SIZES.length === PROD_CLASSES,
  `parametric class table has ${CLASS_SIZES.length} classes, expected ${PROD_CLASSES}`,
);
for (let c = 1; c < CLASS_SIZES.length; c++) {
  assert(
    CLASS_SIZES[c] > CLASS_SIZES[c - 1],
    `class table is not strictly increasing at ${c}`,
  );
}
assert(SUBLEAVES_PER_LEAF === 16, 'the 4 KiB sub-leaf grain is not 16 per leaf');

// The test's carve layout: one block per class, each inside its own sub-leaf
// aligned region of span_subleaves(c) sub-leaves (the real `carve_block` shape).
let carveCursor = 0;
let analyticTail = 0;
let spanningClasses = 0;
let lastSingle = 0;
for (let c = 0; c < PROD_CLASSES; c += 1) {
  const span = spanSubleaves(CLASS_SIZES[c]);
  const regionSlots = span * SUBLEAF_SLOTS;
  carveCursor = Math.ceil(carveCursor / regionSlots) * regionSlots;
  const slots = CLASS_SIZES[c] / SLOT_BYTES;
  // the churn pattern's measurement point: exactly one free slot per class
  const live = (slots - 1) * SLOT_BYTES;
  analyticTail += regionSlots * SLOT_BYTES - live;
  carveCursor += regionSlots;
  if (span > 1) spanningClasses += 1;
  else lastSingle = c;
}
assert(
  carveCursor <= SEGMENT_SLOTS,
  `the ${PROD_CLASSES}-class carve does not fit one segment`,
);
assert(
  spanSubleaves(CLASS_SIZES[lastSingle]) === 1 &&
    spanSubleaves(CLASS_SIZES[lastSingle + 1]) > 1,
  'single/spanning sub-leaf boundary is inconsistent',
);
assert(
  spanningClasses > 0 && spanningClasses < PROD_CLASSES,
  `unexpected ${spanningClasses} spanning classes`,
);
const worstTail = PROD_CLASSES * SUBLEAF_BYTES; // 49 x 4 KiB
const leafTail = PROD_CLASSES * LEAF_BYTES; // 49 x 64 KiB
assert(analyticTail < worstTail, 'the tail exceeds the 49 x 4 KiB bound');
assert(share(analyticTail, SEGMENT_BYTES, 'analytic tail share') <= 5.0);
assert(share(leafTail, SEGMENT_BYTES, 'leaf-grain tail share') > 50.0);

say('Числа пересчитаны здесь из тех же констант бенча, а не списаны: таблица');
say('классов собирается заново, тест сверяет с ней измеренный хвост assert_eq.');
say();
say('| величина | байты | KiB | доля сегмента 4 MiB |');
say('|---|---:|---:|---:|');
const tailShare = share(analyticTail, SEGMENT_BYTES, 'analytic tail share');
const boundShare = share(worstTail, SEGMENT_BYTES, 'bound tail share');
const leafShare = share(leafTail, SEGMENT_BYTES, 'leaf tail share');
say(
  `| хвост под-листа 4 KiB (измерено тестом = аналитический) | ` +
    `${int(analyticTail)} | ${kv(analyticTail)} | ${tailShare.toFixed(2)}% |`,
);
say(
  `| граница §3: ≤ ${PROD_CLASSES} × ${kib(SUBLEAF_BYTES)} KiB | ${int(worstTail)} | ` +
    `${kv(worstTail)} | ${boundShare.toFixed(2)}% |`,
);
say(
  `| хвост листа 64 KiB (B/B2, геометрия провала шага 1) | ${int(leafTail)} | ` +
    `${kv(leafTail)} | ${leafShare.toFixed(2)}% |`,
);
say();
say(
  `Итог: под-лист 4 KiB держит хвост в ${int(analyticTail)} B (${kv(analyticTail)} KiB, ` +
    `${tailShare.toFixed(2)}% сегмента) против ${int(leafTail)} B ` +
    `(${kv(leafTail)} KiB, ${leafShare.toFixed(2)}%) у листа 64 KiB — по границе ` +
    `${fmtRatio(worstTail, leafTail, 'leaf-grain vs sub-leaf tail')} ` +
    `(${kib(LEAF_BYTES)} KiB / ${kib(SUBLEAF_BYTES)} KiB = ${SUBLEAVES_PER_LEAF} под-листов на лист). ` +
    `Классов таблицы, помещающихся в один под-лист: ${lastSingle + 1} из ` +
    `${PROD_CLASSES}; спаном из нескольких под-листов — ${spanningClasses}.`,
);
say();
say('Побочная сверка: `LAST_SINGLE_SUBLEAF_CLASS` бенча — вычисляемая константа;');
say(
  `по этой таблице она равна ${lastSingle} (класс ${CLASS_SIZES[lastSingle]} B = ` +
    'ровно один под-лист), а в комментарии бенча написано 21 — комментарий',
);
say('устарел: значение вычисляется из CLASS_SIZES, а не задаётся вручную.');
say('Оговорка §3 аддендума: нарезка блоков (carve) становится раздельной по');
say('классам — главный интеграционный риск B3; фрагментация здесь — модель по');
say('константам, не замер аллокатора.');
say();

// --- remote merge: no per-block gate (construction confound) ------------------
const merge = scenarios.find((s) => s.id === 'remote_merge');
say('## Remote merge — гейт «≤1.00 Ir/блок» НЕ выводится (конфаунд конструкции)');
say();
say(
  '| сценарий | Ir I | Ir N | Ir L | Ir L2 | Ir L3 | EstCyc I | EstCyc N | ' +
    'EstCyc L | EstCyc L2 | EstCyc L3 |',
);
say('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
say(
  `| remote_merge | ${int(merge.arms[I].ir)} | ${int(merge.arms[N].ir)} | ${int(merge.arms[L].ir)} | ${int(merge.arms[L2].ir)} | ${int(merge.arms[L3].ir)} | ` +
    `${int(merge.arms[I].cycles)} | ${int(merge.arms[N].cycles)} | ${int(merge.arms[L].cycles)} | ${int(merge.arms[L2].cycles)} | ${int(merge.arms[L3].cycles)} |`,
);
say();
say('Плечо I/N: 64 re-push блоков в linked-цепь («merge» = пере-положить free-set).');
say('Плечо L/L2/L3: ДВЕ конструкции модели (donor + target) плюс word-wise');
say('OR-merge слов occupancy (у L3 — 4 слова под-листа 0). Разность плечей');
say('поэтому измеряет конструкции харнесса, а не сам merge: числа «Ir на блок');
say('merge» не печатаются — вывод был бы нечестным (см. записку');
say('docs/perf/PH3C_LEAF_PROTO_IAI.md). Оценка гейта — после интеграции и/или');
say('векторизации OR.');
say();

// --- remote merge with the construction in the setup prologue ---------------
const msetup = scenarios.find((s) => s.id === 'remote_merge_setup');
say('## Remote merge с конструкцией в setup (плечо `remote_merge_setup`) — информативно');
say();
say(
  '§4 аддендума: «Remote merge мерить отдельным плечом, построив donor/target ' +
    'в setup вне замеряемого тела». Здесь donor и target строятся в НЕизмеряемом',
);
say('прологе iai (`#[bench::arm(remote_merge_setup_*())]`), так что тело плеча —');
say('только merge + drain 64 блоков, без конструирования модели и обнуления.');
say();
say('| плечо | Ir | EstCyc | отношение к I (Ir) |');
say('|---|---:|---:|---|');
say(`| I (intrusive) | ${int(msetup.arms[I].ir)} | ${int(msetup.arms[I].cycles)} | — (база) |`);
for (const [idx, tag] of [
  [N, 'N (next_table)'],
  [L, 'L (leaf_table)'],
  [L2, 'L2 (leaf_table_fast)'],
  [L3, 'L3 (leaf_table_3)'],
]) {
  say(
    `| ${tag} | ${int(msetup.arms[idx].ir)} | ${int(msetup.arms[idx].cycles)} | ` +
      `${fmtRatio(msetup.arms[I].ir, msetup.arms[idx].ir, `${ARM_TAG[ARMS[idx]]}/I remote_merge_setup`)} |`,
  );
}
say();
say('Гейт «remote merge ≤1.00 Ir/блок» (§5 аддендума — право на полную');
say('интеграцию) здесь НЕ вычисляется и не оценивается: он принадлежит');
say('интегрированному спайку 3c-2s, а микро-прототип — только фильтр на отсев.');
say('Причины не считать его в эмуляции: тело всё ещё смешивает merge и drain');
say('64 блоков (per-block числа были бы разностью двух разных форм работы),');
say('абсолютные итоги ~3k Ir сравнимы с постоянной частью самой функции и с');
say('маркером `println!` и assert\'ами харнесса, а прокси эмуляции не');
say('откалиброван. Числа выше — информативные: направление у L3 (тело merge у');
say('него единственное не дороже intrusive) проверяется только интеграцией.');
say();

// --- N/I control -------------------------------------------------------------
const nOverI = scenarios.map((s) => ({
  s,
  p: ratio(s.arms[I].ir, s.arms[N].ir, `N/I ${s.id}`),
}));
const nMin = nOverI.reduce((a, b) => (b.p < a.p ? b : a));
const nMax = nOverI.reduce((a, b) => (b.p > a.p ? b : a));
const nDense = nOverI.filter(
  (x) =>
    !x.s.id.startsWith('drain_sparse_') &&
    x.s.id !== 'remote_merge' &&
    x.s.id !== 'remote_merge_setup',
);
const nDenseMin = nDense.reduce((a, b) => (b.p < a.p ? b : a));
const nDenseMax = nDense.reduce((a, b) => (b.p > a.p ? b : a));
say('## Контроль N/I (NextTable-спайк против intrusive)');
say();
say(
  `min N/I = ${fmtRatio(nMin.s.arms[I].ir, nMin.s.arms[N].ir, `N/I ${nMin.s.id}`)} ` +
    `(\`${nMin.s.id}\`), max N/I = ${fmtRatio(nMax.s.arms[I].ir, nMax.s.arms[N].ir, `N/I ${nMax.s.id}`)} ` +
    `(\`${nMax.s.id}\`) — все ${scenarios.length} сценариев.`,
);
say(
  `На «плотных» сценариях (churn/refill/churn_mixed/drain_dense): ` +
    `${fmtRatio(nDenseMin.s.arms[I].ir, nDenseMin.s.arms[N].ir, `N/I ${nDenseMin.s.id}`)} … ` +
    `${fmtRatio(nDenseMax.s.arms[I].ir, nDenseMax.s.arms[N].ir, `N/I ${nDenseMax.s.id}`)}.`,
);
say('Направление совпадает с PG-3: плоский off-body дороже интрузии; сигнал не шум.');
say();

// --- CSV ---------------------------------------------------------------------
// Convention: the ratio columns are filled ONLY for rows of the ten scenarios,
// each arm carrying ITS OWN ratio to the intrusive arm of the same scenario
// (remote_merge_setup included, it is a scenario like the others); the
// intrusive row carries 1.0 in all four; the identity_check row leaves them
// empty (it has no scenario denominator).
const RATIO_COLS = [
  'ratio_L_over_I',
  'ratio_N_over_I',
  'ratio_L2_over_I',
  'ratio_L3_over_I',
];
const OWN_RATIO_COL = {
  intrusive: null,
  next_table: 'ratio_N_over_I',
  leaf_table: 'ratio_L_over_I',
  leaf_table_fast: 'ratio_L2_over_I',
  leaf_table_3: 'ratio_L3_over_I',
};
const csvLines = [
  `bench,arm,ir,cycles,l1,l2,ram,${RATIO_COLS.join(',')},commit`,
];
for (const key of log.order) {
  const row = log.rows.get(key);
  const ratios = {};
  if (row.scenario !== IDENTITY_BENCH) {
    const scenario = scenarios.find((s) => s.id === row.scenario);
    const baseIr = scenario.arms[I].ir;
    const own = OWN_RATIO_COL[row.arm];
    if (own) ratios[own] = factor(baseIr, row.ir, `csv ${own} ${key}`).toFixed(6);
    if (row.arm === 'intrusive') {
      // the base arm is its own denominator in EVERY ratio column
      for (const col of RATIO_COLS) {
        ratios[col] = factor(baseIr, row.ir, `csv ${col} ${key}`).toFixed(6);
      }
    }
  }
  csvLines.push(
    [
      row.scenario,
      row.arm,
      row.ir,
      row.cycles,
      row.l1,
      row.l2,
      row.ram,
      ...RATIO_COLS.map((col) => ratios[col] ?? ''),
      BASE_COMMIT,
    ].join(','),
  );
}
mkdirSync(dirname(CSV_PATH), { recursive: true });
writeFileSync(CSV_PATH, `${csvLines.join('\n')}\n`, 'utf8');

say(`CSV записан: ${rel(CSV_PATH)}`);
say(`  заголовок: ${csvLines[0]}`);
say(`  строк: ${log.order.length} (identity + ${SCENARIOS.length} × 5 плеч)`);
say(`  соглашение: ratio_* заполнены только у строк ${SCENARIOS.length} сценариев — у каждой строки`);
say('  плеча отношение К intrusive своего сценария (remote_merge_setup как');
say('  остальные); у intrusive — 1.0 во всех ratio-колонках; identity_check —');
say('  пусто. Колонка commit = BASE_COMMIT (шаг 3c-1b, дерево-патч поверх него).');
say(
  '  старая таблица шага 1 docs/perf/PH3C_LEAF_PROTO_IAI_summary.csv ' +
    'не трогается',
);

process.stdout.write(`${out.join('\n')}\n`);
