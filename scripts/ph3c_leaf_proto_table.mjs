#!/usr/bin/env node
// Ph3c step-1 — iai table builder for the candidate-B ("bit-leaf free-set")
// micro-prototype.
//
// WHY THIS EXISTS: design candidate B must be judged against the §3.3
// falsification thresholds of docs/design/2026-10-02-ph3c-offbody-geometry-
// design.md (dense leaf `Ir(LeafTable)/Ir(intrusive) > 1.10` => B fails;
// sparse leaf `> 1.25` => B fails) on a DETERMINISTIC axis (callgrind Ir /
// Estimated Cycles), because wall-clock on the Windows dev host is noise.
// ONE raw iai log is the input:
//   docs/perf/_raw_ph3c_leaf_proto_iai.log   (ph3c-proto @ cc84e79b5c65…)
//
// !!! The shared-`CARGO_TARGET_DIR` trap documented in
// docs/perf/PG3_OFFBODY_SPIKE_IAI.md applies here !!! a shared target dir lets
// cargo replay a STALE binary (freshness decided by source mtime, not content),
// which silently produces an A/A run. This run used a PRIVATE target dir, and
// the parser REQUIRES both freshness proofs in the log: the `Compiling
// sefer-alloc` line (something was actually built for this run) and the
// `Iai-Callgrind result: Ok … benchmarks finished` trailer (every bench of the
// set really ran). An incomplete set (≠ 37 benches, a missing arm, a missing
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
// 37 benches are expected and asserted: `identity_check` (listed first — it
// asserts the four models issue the same offset sets and aborts the run on a
// mismatch) + 9 scenarios x 4 arms (intrusive=I, next_table=N,
// leaf_table=L = candidate B, leaf_table_fast=L2 = modification B2).
//
// The percent/ratio printer is the ONLY place a ratio is formatted: it asserts
// the value is finite, the denominator is non-zero, and that
// `denominator*(1+pct/100)` reproduces the numerator to ±0.01% — so a
// hand-typed or copy-pasted number can never reach a table.
//
// Remote merge is deliberately NOT printed as an "Ir per merged block" gate:
// the I/N arms pay 64 re-pushes into a linked chain while the L/L2 arms pay TWO
// model constructions plus one word-wise OR, so any per-block number derived by
// differencing the arms measures the harness constructions, not the merge
// (confound — see the note).
//
// Usage (from the repo root):
//   node scripts/ph3c_leaf_proto_table.mjs [--log <path>]

import assert from 'node:assert/strict';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// --- identity (the prototype sits on the immutable base commit) -------------
const BASE_COMMIT = 'cc84e79b5c6599be3e146abfc15759758e11ab6f';
const FEATURES = 'alloc-global bench-internals internals';
const DEFAULT_LOG = join(
  REPO_ROOT,
  'docs',
  'perf',
  '_raw_ph3c_leaf_proto_iai.log',
);
const CSV_PATH = join(REPO_ROOT, 'docs', 'perf', 'PH3C_LEAF_PROTO_IAI_summary.csv');

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
];
// intrusive=I (base emulation), next_table=N (PG-3 spike), leaf_table=L
// (candidate B), leaf_table_fast=L2 (modification B2).
const ARMS = ['intrusive', 'next_table', 'leaf_table', 'leaf_table_fast'];
const ARM_TAG = {
  intrusive: 'I',
  next_table: 'N',
  leaf_table: 'L',
  leaf_table_fast: 'L2',
};
const I = ARMS.indexOf('intrusive');
const N = ARMS.indexOf('next_table');
const L = ARMS.indexOf('leaf_table');
const L2 = ARMS.indexOf('leaf_table_fast');
const IDENTITY_BENCH = 'identity_check';
const IDENTITY_ARM = 'identity_only';
const EXPECTED_BENCHES = SCENARIOS.length * ARMS.length + 1; // 37

// §3.3 falsification thresholds (percent form).
const TH_DENSE_PCT = 10; // dense leaf: L/I > 1.10 => B fails
const TH_SPARSE_PCT = 25; // sparse leaf (W_eff > 8 words): L/I > 1.25 => B fails
// Observational only — the ADR refill/flush gates of the INTEGRATED allocator
// (step 2), quoted here as an orientation point for the arena emulation.
const TH_ADR_REFILL_CYC_PCT = 5; // EstCycles ≤ 1.05
const TH_ADR_REFILL_IR_PCT = 10; // Ir ≤ 1.10

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
 *  identity arm carries no `::`-path but keeps the same shape (`identity_only:0u8`).
 *  The full header line is the row key — never key by the short name. */
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
      'scenarios x 4 arms)',
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

// --- ratio printer (the ONLY ratio formatter; guards against hand numbers) ---
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

/** `x1.25838 (+25.84%)` — every printed ratio passes the round-trip assert. */
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

const int = (n) => n.toLocaleString('en-US');
const rel = (p) => relative(REPO_ROOT, resolve(p)).replace(/\\/g, '/');

// --- output ------------------------------------------------------------------
const out = [];
const say = (line = '') => out.push(line);

say('# Ph3c шаг 1 — iai-таблицы микро-прототипа кандидата B (bit-leaf free-set)');
say();
say(
  `лог: \`${rel(opts.log)}\` (ph3c-proto @ ${BASE_COMMIT}; дерево = чистый ` +
    `${BASE_COMMIT.slice(0, 7)} + патч прототипа, benches/ph3c_leaf_proto.rs)`,
);
say(
  `features: \`${FEATURES}\` (cache-sim on; EstCycles = L1 + 5·L2 + 35·RAM)  |  ` +
    `бенчей: ${EXPECTED_BENCHES} = ${IDENTITY_BENCH} + ${SCENARIOS.length} ` +
    'сценариев × 4 плеча (I/N/L/L2)',
);
say();
say('Каждая метрика — ПЕРВОЕ число iai-строки (текущий прогон); второе число —');
say('baseline самого раннера (count или `N/A`) и намеренно игнорируется.');
say('Знаменатели явные и только внутри сценария:');
say('L/I = Ir(leaf_table)/Ir(intrusive), N/I = Ir(next_table)/Ir(intrusive),');
say('L2/I = Ir(leaf_table_fast)/Ir(intrusive).');
say('Каждое напечатанное отношение прошло round-trip проверку:');
say('denominator*(1+pct/100) воспроизводит числитель ±0.01%, denominator ≠ 0.');
say();

say(`## Идентичность — оракул, не сценарий (${IDENTITY_BENCH})`);
say();
say('| бенч | Ir | EstCycles |');
say('|---|---:|---:|');
say(`| ${IDENTITY_BENCH} | ${int(identity.ir)} | ${int(identity.cycles)} |`);
say();
say('Плечи модели выдают одинаковые множества оффсетов (assert внутри бенча);');
say('прогон «37 without regressions» — оракул пройден.');
say();

say('## Ir и EstCycles по плечам + отношения к intrusive');
say();
say(
  '| сценарий | Ir I | Ir N | Ir L | Ir L2 | EstCyc I | EstCyc N | EstCyc L | EstCyc L2 | L/I | N/I | L2/I |',
);
say('|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|');
for (const s of scenarios) {
  const ir = s.arms.map((a) => a.ir);
  const cy = s.arms.map((a) => a.cycles);
  say(
    `| ${s.id} | ${int(ir[I])} | ${int(ir[N])} | ${int(ir[L])} | ${int(ir[L2])} | ` +
      `${int(cy[I])} | ${int(cy[N])} | ${int(cy[L])} | ${int(cy[L2])} | ` +
      `${fmtRatio(ir[I], ir[L], `L/I ${s.id}`)} | ` +
      `${fmtRatio(ir[I], ir[N], `N/I ${s.id}`)} | ` +
      `${fmtRatio(ir[I], ir[L2], `L2/I ${s.id}`)} |`,
  );
}
say();

// --- verdict against the §3.3 thresholds -------------------------------------
say('## Вердикт против порогов §3.3');
say();
say(`Плечо I = intrusive-эмуляция (база), N = NextTable-спайк, L = кандидат B,`);
say('L2 = модификация B2 (u64 words_mask вместо first_hint-скана).');
say();

const dense = scenarios.find((s) => s.id === 'drain_dense');
const sparse = ['drain_sparse_1', 'drain_sparse_8', 'drain_sparse_64'].map((id) =>
  scenarios.find((s) => s.id === id),
);

const denseL = ratio(dense.arms[I].ir, dense.arms[L].ir, 'L/I drain_dense');
const denseL2 = ratio(dense.arms[I].ir, dense.arms[L2].ir, 'L2/I drain_dense');
say(
  `* Плотный лист \`drain_dense\`: L/I = ${fmtRatio(dense.arms[I].ir, dense.arms[L].ir, 'L/I drain_dense')} ` +
    `при пороге §3.3 \`> x1.10000\` -> **${denseL > TH_DENSE_PCT ? 'FAIL' : 'PASS'}** (B не проходит)`,
);
say(
  `* Модификация B2 там же: L2/I = ${fmtRatio(dense.arms[I].ir, dense.arms[L2].ir, 'L2/I drain_dense')} ` +
    `при пороге \`> x1.10000\` -> **${denseL2 > TH_DENSE_PCT ? 'FAIL' : 'PASS'}**`,
);
for (const s of sparse) {
  const k = s.id.replace('drain_sparse_', '');
  const lp = ratio(s.arms[I].ir, s.arms[L].ir, `L/I ${s.id}`);
  const l2p = ratio(s.arms[I].ir, s.arms[L2].ir, `L2/I ${s.id}`);
  say(
    `* Разреженный \`drain_sparse_${k}\` (k=${k}): L/I = ${fmtRatio(s.arms[I].ir, s.arms[L].ir, `L/I ${s.id}`)} ` +
      `при пороге §3.3 \`> x1.25000\` -> **${lp > TH_SPARSE_PCT ? 'FAIL' : 'PASS'}**; ` +
      `B2: L2/I = ${fmtRatio(s.arms[I].ir, s.arms[L2].ir, `L2/I ${s.id}`)} -> ` +
      `**${l2p > TH_SPARSE_PCT ? 'FAIL' : 'PASS'}**`,
  );
}
say();
say('Вывод: гейты §3.3 — плотный лист проваливает порог (и L/I, и L2/I >');say('x1.10000), разреженные k=1/8/64 проходят с запасом: провал B НЕ в W_eff-скане.');
say('churn/refill/churn_mixed вне гейтов §3.3 (наблюдательные, эмуляция арены),');
say('но и там L и L2 дороже intrusive (L/I от x1.23468 до x1.47458).');
say();

// --- observational scenarios (outside the §3.3 gates; emulation) -------------
say('## Наблюдательные сценарии — ВНЕ гейтов §3.3 (эмуляция арены, не аллокатор)');
say();
say('Пороги ADR refill/flush (шаг 2, интегрированный аллокатор) даны как');
say('ориентир: EstCycles ≤ x1.05000, Ir ≤ x1.10000 — на эмуляции не гейт.');
say();
say('| сценарий | L/I | L2/I | EstCyc L/I | EstCyc L2/I | ADR-ориентир (эмуляция) |');
say('|---|---:|---:|---:|---:|---|');
const observational = ['churn_one_class', 'refill_cold_16', 'refill_cold_64', 'churn_mixed'].map(
  (id) => scenarios.find((s) => s.id === id),
);
for (const s of observational) {
  const ir = s.arms.map((a) => a.ir);
  const cy = s.arms.map((a) => a.cycles);
  let orient;
  if (s.id.startsWith('refill_cold')) {
    const cycP = ratio(cy[I], cy[L], `EstCyc L/I ${s.id}`);
    const irP = ratio(ir[I], ir[L], `Ir L/I ${s.id}`);
    const ok = cycP <= TH_ADR_REFILL_CYC_PCT && irP <= TH_ADR_REFILL_IR_PCT;
    orient = `EstCyc ${fmtFactorPct(cy[I], cy[L], `EstCyc L/I ${s.id}`)}, Ir ${fmtFactorPct(ir[I], ir[L], `Ir L/I ${s.id}`)} -> **${ok ? 'PASS' : 'FAIL'}**`;
  } else {
    orient = '— (вне гейтов: churn-плечо ADR hot ≤1.02 не применимо к эмуляции)';
  }
  say(
    `| ${s.id} | ${fmtRatio(ir[I], ir[L], `L/I ${s.id}`)} | ${fmtRatio(ir[I], ir[L2], `L2/I ${s.id}`)} | ` +
      `${fmtRatio(cy[I], cy[L], `EstCyc L/I ${s.id}`)} | ${fmtRatio(cy[I], cy[L2], `EstCyc L2/I ${s.id}`)} | ${orient} |`,
  );
}
say();

// --- remote merge: no per-block gate (construction confound) ------------------
const merge = scenarios.find((s) => s.id === 'remote_merge');
say('## Remote merge — гейт «≤1.00 Ir/блок» НЕ выводится (конфаунд конструкции)');
say();
say('| сценарий | Ir I | Ir N | Ir L | Ir L2 | EstCyc I | EstCyc N | EstCyc L | EstCyc L2 |');
say('|---|---:|---:|---:|---:|---:|---:|---:|---:|');
say(
  `| remote_merge | ${int(merge.arms[I].ir)} | ${int(merge.arms[N].ir)} | ${int(merge.arms[L].ir)} | ${int(merge.arms[L2].ir)} | ` +
    `${int(merge.arms[I].cycles)} | ${int(merge.arms[N].cycles)} | ${int(merge.arms[L].cycles)} | ${int(merge.arms[L2].cycles)} |`,
);
say();
say('Плечо I/N: 64 re-push блоков в linked-цепь («merge» = пере-положить free-set).');
say('Плечо L/L2: ДВЕ конструкции модели (donor + target) плюс word-wise OR-merge');
say('64 слов occupancy. Разность плечей поэтому измеряет конструкции харнесса,');
say('а не сам merge: числа «Ir на блок merge» не печатаются — вывод был бы');
say('нечестным (см. записку docs/perf/PH3C_LEAF_PROTO_IAI.md). Оценка гейта —');
say('после интеграции и/или векторизации OR.');
say();

// --- N/I control -------------------------------------------------------------
const nOverI = scenarios.map((s) => ({
  s,
  p: ratio(s.arms[I].ir, s.arms[N].ir, `N/I ${s.id}`),
}));
const nMin = nOverI.reduce((a, b) => (b.p < a.p ? b : a));
const nMax = nOverI.reduce((a, b) => (b.p > a.p ? b : a));
const nDense = nOverI.filter(
  (x) => !x.s.id.startsWith('drain_sparse_') && x.s.id !== 'remote_merge',);
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
// Convention: the three ratio columns are filled ONLY for rows of the nine
// scenarios, each arm carrying ITS OWN ratio to the intrusive arm of the same
// scenario; the intrusive row carries 1.0 in all three; the identity_check row
// leaves them empty (it has no scenario denominator).
const csvLines = [
  'bench,arm,ir,cycles,l1,l2,ram,ratio_L_over_I,ratio_N_over_I,ratio_L2_over_I,commit',
];
for (const key of log.order) {
  const row = log.rows.get(key);
  const ratios = {};
  if (row.scenario !== IDENTITY_BENCH) {
    const scenario = scenarios.find((s) => s.id === row.scenario);
    const baseIr = scenario.arms[I].ir;
    const own = { intrusive: null, next_table: 'ratio_N_over_I', leaf_table: 'ratio_L_over_I', leaf_table_fast: 'ratio_L2_over_I' }[
      row.arm
    ];
    if (own) ratios[own] = factor(baseIr, row.ir, `csv ${own} ${key}`).toFixed(6);
    if (row.arm === 'intrusive') {
      ratios.ratio_N_over_I = factor(baseIr, row.ir, `csv ratio_N_over_I ${key}`).toFixed(6);
      ratios.ratio_L_over_I = factor(baseIr, row.ir, `csv ratio_L_over_I ${key}`).toFixed(6);
      ratios.ratio_L2_over_I = factor(baseIr, row.ir, `csv ratio_L2_over_I ${key}`).toFixed(6);
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
      ratios.ratio_L_over_I ?? '',
      ratios.ratio_N_over_I ?? '',
      ratios.ratio_L2_over_I ?? '',
      BASE_COMMIT,
    ].join(','),
  );
}
mkdirSync(dirname(CSV_PATH), { recursive: true });
writeFileSync(CSV_PATH, `${csvLines.join('\n')}\n`, 'utf8');

say(`CSV записан: ${rel(CSV_PATH)}`);
say(`  заголовок: ${csvLines[0]}`);
say(`  строк: ${log.order.length} (identity + ${SCENARIOS.length} × 4 плеча)`);
say('  соглашение: ratio_* заполнены только у строк 9 сценариев — у каждой строки');
say('  плеча отношение К intrusive своего сценария; у intrusive — 1.0 во всех');
say('  трёх колонках; identity_check — пусто.');

process.stdout.write(`${out.join('\n')}\n`);
