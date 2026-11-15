import assert from 'node:assert/strict';
import { test } from 'node:test';
// node:test suite for scripts/iai-nolto-summary.mjs, styled after
// tests/module_graph.test.mjs. Imports are relative to this file (no cwd
// dependence); no network, no cargo. EVERY fixture below is a synthetic
// string built HERE from arbitrary constants — no real CI measurements;
// expected values are derived from the same fixture constants (never
// hand-typed twice). ONE EXCEPTION: the colored-log fixture reproduces the
// exact line shape of the CI log quoted in the P1.0 defect report (GitHub
// Actions run 43712417106, job "no-LTO"); the first bench's values are from
// that quoted excerpt, the second bench's metric values are synthetic. All
// ESC bytes are written as '\x1b' escapes, never a raw ESC byte.
import {
  buildCompareCsv,
  buildCsv,
  buildMarkdownSection,
  compareRuns,
  parseCsv,
  parseIaiLog,
  main,
} from '../scripts/iai-nolto-summary.mjs';
import { rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

// --- synthetic fixtures -----------------------------------------------------

// Arbitrary integers (test constants, NOT measurements).
const A = { name: 'small_churn_16b', ir: 1234567, l1: 2345678, l2: 34567, ram: 456789, cycles: 5678901 };
const B = { name: 'cold_alloc_free_256x16b', ir: 7654321, l1: 8765432, l2: 98765, ram: 123456, cycles: 6789012 };
const C = { name: 'large_alloc_free_cycle', ir: 1111111, l1: 2222222, l2: 33333, ram: 444444, cycles: 5555555 };

// One iai-callgrind 0.14 bench block in the format scripts/iai.mjs's
// parseMetrics documents: header line, then indented `  <Label>: <int>|N/A`
// rows with thousands separators. Cache rows omitted when null.
function benchBlock(b) {
  const cell = (v) => v.toLocaleString('en-US');
  const row = (label, v) => `  ${label}:${' '.repeat(Math.max(1, 13 - cell(v).length))}${cell(v)}|N/A     (...)`;
  const lines = [`perf_gate_iai::perf_gate::${b.name}`, row('Instructions', b.ir)];
  for (const [label, key] of [
    ['L1 Hits', 'l1'],
    ['L2 Hits', 'l2'],
    ['RAM Hits', 'ram'],
    ['Estimated Cycles', 'cycles'],
  ]) {
    if (b[key] != null) lines.push(row(label, b[key]));
  }
  return lines.join('\n');
}

const expectedRow = (b) => ({ name: b.name, ir: b.ir, l1: b.l1, l2: b.l2, ram: b.ram, cycles: b.cycles });

const COMPILE_NOISE = [
  '   Compiling sefer-alloc v0.3.0',
  '    Finished `bench` profile [optimized] target(s) in 12.34s',
  '     Running unittests src/lib.rs (target/debug/deps/sefer_alloc_abc123)',
];

// 3 benches, each with Instructions + all four cache rows, wrapped in cargo
// compile noise (none of which matches the bench-header regex).
const fixtureAllCaches = [...COMPILE_NOISE, benchBlock(A), '', benchBlock(B), benchBlock(C), ''].join('\n');

// Compare-mode fixtures: same 3 bench names on both sides, different Ir.
// Deliberately mixed signs (head > base on A and C, head < base on B) so the
// delta sign is meaningful; B carries no cache rows on either side so null
// round-trip is exercised too.
const CMP_BASE = [
  { name: A.name, ir: 2000000, l1: 2100000, l2: 21000, ram: 220000, cycles: 2300000 },
  { name: B.name, ir: 3000000, l1: null, l2: null, ram: null, cycles: null },
  { name: C.name, ir: 500000, l1: 510000, l2: 5200, ram: 53000, cycles: 540000 },
];
const CMP_HEAD = [
  { name: A.name, ir: 2100000, l1: 2200000, l2: 22000, ram: 230000, cycles: 2400000 },
  { name: B.name, ir: 2900000, l1: null, l2: null, ram: null, cycles: null },
  { name: C.name, ir: 510000, l1: 520000, l2: 5300, ram: 54000, cycles: 550000 },
];
const cmpBaseLog = CMP_BASE.map(benchBlock).join('\n');
const cmpHeadLog = CMP_HEAD.map(benchBlock).join('\n');

const META = {
  commit: 'a'.repeat(40),
  baseCommit: 'b'.repeat(40),
  features: 'production bench-internals internals',
  profile: 'bench profile + CARGO_PROFILE_BENCH_LTO=false + CARGO_PROFILE_BENCH_CODEGEN_UNITS=16 (cargo env overrides of [profile.bench]; user-default codegen)',
  note: 'synthetic note, with comma and "quotes"',
};

// Case 1: cargo noise is ignored; Ir + all four cache rows parse, deep-equal
// to expectations derived from the fixture constants.
test('parse: strips cargo noise and reads Ir + all four cache rows', () => {
  const parsed = parseIaiLog(fixtureAllCaches);
  assert.deepEqual(parsed, [A, B, C].map(expectedRow));
});

// Case 2: GitHub Actions log prefixes (tab-separated job/step fields + ISO
// timestamp, and bare timestamps) normalize away to an identical parse.
test('parse: Actions tab/timestamp line prefixes yield an identical parse', () => {
  const ts = '2026-11-15T10:00:00.0000000Z ';
  const prefixed = fixtureAllCaches
    .split('\n')
    .map((line, i) => (i % 2 === 0 ? `job123\tstep4\t${ts}` : ts) + line)
    .join('\n');
  assert.deepEqual(parseIaiLog(prefixed), parseIaiLog(fixtureAllCaches));
});

// Case 3: a bench with only an Instructions row keeps all cache fields null.
test('parse: bench with no cache rows keeps l1/l2/ram/cycles null', () => {
  const solo = { name: 'solo_bench_no_caches', ir: 987654, l1: null, l2: null, ram: null, cycles: null };
  assert.deepEqual(parseIaiLog(benchBlock(solo) + '\n'), [solo]);
});

// Case 4: a bench header with no Instructions line is a hard error naming it.
test('parse: bench header without an Instructions row throws naming the bench', () => {
  assert.throws(
    () =>
      parseIaiLog(
        'perf_gate_iai::perf_gate::dropped_bench\n  L1 Hits:                   1234|N/A     (...)\n',
      ),
    (e) => e.message.includes('dropped_bench'),
    'a header with no Instructions row must be a hard error listing the bench name',
  );
});

// Case 5: `N/A` in the first metric column must not match the metric regex
// ([\d,]+); the bench then trips the no-Instructions hard error instead of
// silently producing a fake number.
test('parse: N/A first column does not parse as a metric (throws)', () => {
  assert.throws(
    () =>
      parseIaiLog(
        'perf_gate_iai::perf_gate::na_bench\n  Instructions:              N/A|1,111     (...)\n',
      ),
    (e) => e.message.includes('na_bench'),
    'N/A must not be read as an instruction count',
  );
});

// Case 6: compare mode CSV — deltas, delta_pct formula, round-trip via
// parseCsv, identity quoting (note contains comma + quotes), and the
// counterfactual swap guard on delta signs. buildCsv (record side) is
// round-tripped too.
test('compare: CSV deltas, formula, round-trip, and swap-detector signs', () => {
  const cmp = compareRuns(parseIaiLog(cmpBaseLog), parseIaiLog(cmpHeadLog));
  const csv = buildCompareCsv(cmp, META);
  const parsed = parseCsv(csv);
  assert.equal(parsed.length, CMP_BASE.length + 2, 'header + benches + TOTAL');
  assert.deepEqual(parsed[0], [
    'bench', 'ir_base', 'ir_head', 'delta_ir', 'delta_pct',
    'l1_base', 'l2_base', 'ram_base', 'cycles_base',
    'l1_head', 'l2_head', 'ram_head', 'cycles_head',
    'commit_base', 'commit_head', 'features', 'profile', 'note',
  ]);
  for (let i = 0; i < CMP_BASE.length; i++) {
    const p = parsed[i + 1];
    const b = CMP_BASE[i];
    const h = CMP_HEAD[i];
    assert.equal(p[0], b.name);
    // Fields round-trip to the fixture constants (catches base/head swaps in
    // the ir_base/ir_head columns themselves):
    assert.equal(Number(p[1]), b.ir, `ir_base round-trip for ${b.name}`);
    assert.equal(Number(p[2]), h.ir, `ir_head round-trip for ${b.name}`);
    const delta = Number(p[3]);
    assert.equal(delta, h.ir - b.ir, `delta_ir === ir_head - ir_base for ${b.name}`);
    const expectPct = b.ir === 0 ? null : Math.round(((h.ir - b.ir) / b.ir) * 10000) / 100;
    assert.equal(p[4] === '' ? null : Number(p[4]), expectPct, `delta_pct formula for ${b.name}`);
    // Counterfactual guard: delta_ir > 0 EXACTLY where head ir > base ir —
    // fails if base/head were ever swapped or the delta flipped.
    assert.equal(delta > 0, h.ir > b.ir, `delta sign must match head-vs-base for ${b.name}`);
    const caches = p.slice(5, 13).map((c) => (c === '' ? null : Number(c)));
    assert.deepEqual(caches, [b.l1, b.l2, b.ram, b.cycles, h.l1, h.l2, h.ram, h.cycles]);
    assert.deepEqual(p.slice(13), [META.baseCommit, META.commit, META.features, META.profile, META.note]);
  }
  const t = parsed[parsed.length - 1];
  assert.equal(t[0], 'TOTAL');
  assert.equal(Number(t[1]), CMP_BASE.reduce((s, b) => s + b.ir, 0));
  assert.equal(Number(t[2]), CMP_HEAD.reduce((s, b) => s + b.ir, 0));
  // Record-side CSV also round-trips (header + per-bench Ir).
  const recCsv = buildCsv(parseIaiLog(cmpBaseLog), META);
  const recParsed = parseCsv(recCsv);
  assert.deepEqual(recParsed[0], ['bench', 'ir', 'l1', 'l2', 'ram', 'cycles', 'commit', 'features', 'profile', 'note']);
  assert.equal(recParsed.length, CMP_BASE.length + 1);
  for (let i = 0; i < CMP_BASE.length; i++) {
    assert.equal(recParsed[i + 1][0], CMP_BASE[i].name);
    assert.equal(Number(recParsed[i + 1][1]), CMP_BASE[i].ir);
  }
});

// Case 7: mismatched bench-name sets throw, listing the offenders.
test('compare: mismatched bench sets throw listing the offenders', () => {
  const row = (name, ir) => ({ name, ir, l1: null, l2: null, ram: null, cycles: null });
  assert.throws(
    () => compareRuns([row('bench_a', 1), row('bench_b', 2)], [row('bench_a', 1), row('bench_c', 3)]),
    (e) => e.message.includes('only in base: [bench_b]') && e.message.includes('only in head: [bench_c]'),
    'the error must list which benches are only in base and only in head',
  );
});

// Case 8: single-record markdown table contains each bench's Ir formatted
// with toLocaleString('en-US') — derived from the fixture, not hand-typed.
test('record markdown: table shows each bench Ir formatted en-US (derived)', () => {
  const rows = parseIaiLog(fixtureAllCaches);
  const md = buildMarkdownSection({ rows, meta: { commit: 'c'.repeat(40) } });
  assert.ok(md.includes('| bench | Ir | L1 hits | L2 hits | RAM hits | Est. cycles |'), 'record table header');
  for (const b of [A, B, C]) {
    assert.ok(
      md.includes(b.ir.toLocaleString('en-US')),
      `markdown table must contain the formatted Ir of ${b.name}`,
    );
  }
});

// Case 9: the markdown totals line's numbers equal the sums recomputed from
// the per-bench CSV rows — both sides parsed from the generated outputs, no
// string typed twice.
test('totals: CSV per-bench delta sum equals the markdown totals line', () => {
  const cmp = compareRuns(parseIaiLog(cmpBaseLog), parseIaiLog(cmpHeadLog));
  const csv = buildCompareCsv(cmp, META);
  const md = buildMarkdownSection({ compare: cmp, meta: META });
  const perBench = parseCsv(csv).slice(1, CMP_BASE.length + 1);
  const m = /Total Ir base\/head\/delta: ([\d,]+) \/ ([\d,]+) \/ (-?[\d,]+)/.exec(md);
  assert.ok(m, 'markdown must carry the totals line');
  const [totBase, totHead, totDelta] = [m[1], m[2], m[3]].map((s) => Number(s.replace(/,/g, '')));
  assert.equal(totBase, perBench.reduce((s, r) => s + Number(r[1]), 0));
  assert.equal(totHead, perBench.reduce((s, r) => s + Number(r[2]), 0));
  assert.equal(totDelta, perBench.reduce((s, r) => s + Number(r[3]), 0));
});

// --- colored (real Actions) fixtures -----------------------------------------

// Real CI log excerpt shape (P1.0 defect report, Actions run 43712417106,
// job "no-LTO"; CARGO_TERM_COLOR=always makes iai color its output). The
// first bench's values are from the quoted excerpt; the second bench's are
// synthetic test constants.
const REAL_TS = '2026-11-15T12:11:06.6808109Z ';
const REAL_B1 = { name: 'small_churn_16b', ir: 53313, l1: 98029, l2: 203, ram: 1243, cycles: 142549 };
const REAL_B2 = { name: 'small_churn_16b_2n', ir: 98765, l1: 11111, l2: 22, ram: 333, cycles: 4444 };

const coloredRow = (label, v) =>
  '\x1b[90m  \x1b[0m' + label + ':' + ' '.repeat(Math.max(1, 30 - label.length)) +
  '\x1b[1m' + v + '\x1b[0m|N/A                  (\x1b[90m*********\x1b[0m)';
const coloredHeader = (name) => REAL_TS + '\x1b[32mperf_gate_iai::perf_gate::' + name + '\x1b[0m';
const coloredBlock = (b) =>
  [coloredHeader(b.name), coloredRow('Instructions', b.ir), coloredRow('L1 Hits', b.l1),
    coloredRow('L2 Hits', b.l2), coloredRow('RAM Hits', b.ram),
    coloredRow('Estimated Cycles', b.cycles)].join('\n');

// (a) the exact 8 quoted lines for bench 1 + bench 2 with synthetic rows.
const coloredFixture = [coloredBlock(REAL_B1), coloredBlock(REAL_B2)].join('\n') + '\n';
const stripAnsi = (s) => s.replace(/\x1b\[[0-9;:?]*[A-Za-z]/g, '');

test('parse: colored (real-shape) Actions log parses the exact 2 benches', () => {
  assert.deepEqual(parseIaiLog(coloredFixture), [
    { name: REAL_B1.name, ir: REAL_B1.ir, l1: REAL_B1.l1, l2: REAL_B1.l2, ram: REAL_B1.ram, cycles: REAL_B1.cycles },
    { name: REAL_B2.name, ir: REAL_B2.ir, l1: REAL_B2.l1, l2: REAL_B2.l2, ram: REAL_B2.ram, cycles: REAL_B2.cycles },
  ]);
});

// (b) colored === colorless: stripping ANSI with a locally-declared regex of
// the same shape as the script's ANSI_CSI_RE must change nothing.
test('parse: colored and colorless logs parse identically (ANSI is inert)', () => {
  const colorless = stripAnsi(coloredFixture);
  assert.notEqual(colorless, coloredFixture, 'the twin must really have escapes removed');
  assert.deepEqual(parseIaiLog(coloredFixture), parseIaiLog(colorless));
  const first = parseIaiLog(colorless)[0];
  assert.deepEqual(
    first,
    { name: REAL_B1.name, ir: REAL_B1.ir, l1: REAL_B1.l1, l2: REAL_B1.l2, ram: REAL_B1.ram, cycles: REAL_B1.cycles },
    'full 6-metric first bench row',
  );
});

// (c1) zero benches parsed is a hard error with diagnostic context, not [].
test('parse: 0 benches parsed throws with Instructions-line diagnostics', () => {
  // Colored metric rows but no bench header: nothing attaches to a bench.
  const noHeader = coloredRow('Instructions', 12345) + '\n';
  assert.throws(
    () => parseIaiLog(noHeader),
    (e) =>
      e.message.includes('0 benches parsed') &&
      e.message.includes('1 line(s)') &&
      e.message.includes('Instructions:'),
    'metric rows without a bench header must hard-error with diagnostics',
  );
  // Garbage log with no Instructions mention at all.
  assert.throws(
    () => parseIaiLog('some random cargo noise\n'),
    (e) => e.message.includes('no line contains'),
    'a log with no Instructions mention must hard-error too',
  );
});

// (c2) end-to-end via main(): the old defect printed "OK — 0 bench(es)" and
// exited 0; now it must FAIL with exit code 1 and no OK line.
test('cli: garbage log FAILs with exit 1 and no OK line (zero-bench guard)', () => {
  const tmpPath = join(tmpdir(), 'iai-nolto-summary-zero-test-' + process.pid + '-' + Date.now() + '.log');
  const captured = [];
  const origError = console.error;
  console.error = (...args) => captured.push(args.join(' '));
  let code;
  try {
    writeFileSync(tmpPath, 'some random cargo noise\n');
    code = main(['--log', tmpPath]);
  } finally {
    console.error = origError;
    rmSync(tmpPath, { force: true });
  }
  const stderr = captured.join('\n');
  assert.equal(code, 1, 'must exit 1, NOT the old 0');
  assert.ok(stderr.includes('[iai-nolto-summary] FAIL'), 'stderr must carry the FAIL line');
  assert.ok(!stderr.includes('OK —'), 'stderr must NOT contain an OK line');
});

// (d) compare mode with the real iai 0.14 baseline-column shapes. Metric-row
// format precedents: scripts/pg3_iai_table.mjs (~lines 29-33) and
// scripts/ph3c_leaf_proto_table.mjs (~line 31):
//   `Instructions:        196387|196387   (No change)`
//   `Instructions:         392243|N/A      (*********)`
//   `Instructions:        120258|120255   (+0.00249%) [+1.00002x]`
// plus the observed `|N/A ... (*********)` shape from the defect excerpt.
// The parser must always take the FIRST integer column (current run).
const CMP2_BASE = [
  { name: 'cmp_rise_bench', ir: 1200, l1: 1300, l2: 14, ram: 150, cycles: 1600 },
  { name: 'cmp_fall_bench', ir: 9000, l1: 9500, l2: 96, ram: 970, cycles: 9800 },
];
const CMP2_HEAD = [
  { name: 'cmp_rise_bench', ir: 1234, l1: 1333, l2: 15, ram: 152, cycles: 1601 },
  { name: 'cmp_fall_bench', ir: 8888, l1: 9400, l2: 95, ram: 960, cycles: 9700 },
];
const cmpColoredRow = (label, v, baseline) =>
  '\x1b[90m  \x1b[0m' + label + ':' + ' '.repeat(Math.max(1, 30 - label.length)) +
  '\x1b[1m' + v + '\x1b[0m|' + baseline;
const cmpHeadBlock = (b) =>
  [coloredHeader(b.name),
    cmpColoredRow('Instructions', b.ir, `${b.ir}   (+${Math.abs(((b.ir - 1) / b.ir) * 100).toFixed(5)}%) [+1.0${b.ir % 10}x]`),
    cmpColoredRow('L1 Hits', b.l1, `${b.l1}`),
    cmpColoredRow('L2 Hits', b.l2, `${b.l2}`),
    cmpColoredRow('RAM Hits', b.ram, `${b.ram}`),
    cmpColoredRow('Estimated Cycles', b.cycles, `${b.cycles}`),
  ].join('\n');
const cmpBaseBlock = (b) =>
  [coloredHeader(b.name),
    cmpColoredRow('Instructions', b.ir, 'N/A                  (\x1b[90m*********\x1b[0m)'),
    cmpColoredRow('L1 Hits', b.l1, 'N/A                  (\x1b[90m*********\x1b[0m)'),
    cmpColoredRow('L2 Hits', b.l2, 'N/A                  (\x1b[90m*********\x1b[0m)'),
    cmpColoredRow('RAM Hits', b.ram, 'N/A                  (\x1b[90m*********\x1b[0m)'),
    cmpColoredRow('Estimated Cycles', b.cycles, 'N/A                  (\x1b[90m*********\x1b[0m)'),
  ].join('\n');
const cmpHeadLog2 = CMP2_HEAD.map(cmpHeadBlock).join('\n') + '\n';
const cmpBaseLog2 = CMP2_BASE.map(cmpBaseBlock).join('\n') + '\n';

test('compare: baseline-column colored logs take the FIRST column on both sides', () => {
  const baseRows = parseIaiLog(cmpBaseLog2);
  const headRows = parseIaiLog(cmpHeadLog2);
  for (let i = 0; i < CMP2_BASE.length; i++) {
    const b = CMP2_BASE[i];
    const h = CMP2_HEAD[i];
    assert.deepEqual(headRows[i], { name: h.name, ir: h.ir, l1: h.l1, l2: h.l2, ram: h.ram, cycles: h.cycles });
    assert.deepEqual(baseRows[i], { name: b.name, ir: b.ir, l1: b.l1, l2: b.l2, ram: b.ram, cycles: b.cycles });
  }
  const cmp = compareRuns(baseRows, headRows);
  for (let i = 0; i < CMP2_BASE.length; i++) {
    const b = CMP2_BASE[i];
    const h = CMP2_HEAD[i];
    const r = cmp.rows[i];
    assert.equal(r.irBase, b.ir, `irBase takes base's FIRST column (${b.name})`);
    assert.equal(r.irHead, h.ir, `irHead takes head's FIRST column (${h.name})`);
    assert.equal(r.deltaIr, h.ir - b.ir, `deltaIr === head - base (${h.name})`);
    const expectPct = Math.round(((h.ir - b.ir) / b.ir) * 10000) / 100;
    assert.equal(r.deltaPct, expectPct, `deltaPct === two-decimal formula (${h.name})`);
    assert.equal(r.deltaIr > 0, h.ir > b.ir, `delta sign matches head-vs-base (${h.name})`);
  }
  // The rise bench exercises +34 / +2.83% exactly.
  assert.equal(cmp.rows[0].deltaIr, 34);
  assert.equal(cmp.rows[0].deltaPct, 2.83);
  assert.ok(cmp.rows[1].deltaIr < 0, 'the head<base bench must exercise the negative path');
  // CSV round-trip via parseCsv.
  const parsed = parseCsv(buildCompareCsv(cmp, META));
  assert.equal(parsed.length, CMP2_BASE.length + 2, 'header + benches + TOTAL');
  for (let i = 0; i < CMP2_BASE.length; i++) {
    const p = parsed[i + 1];
    assert.equal(p[0], CMP2_BASE[i].name);
    assert.equal(Number(p[1]), CMP2_BASE[i].ir, `ir_base round-trip (${p[0]})`);
    assert.equal(Number(p[2]), CMP2_HEAD[i].ir, `ir_head round-trip (${p[0]})`);
    assert.equal(Number(p[3]), CMP2_HEAD[i].ir - CMP2_BASE[i].ir, `delta round-trip (${p[0]})`);
  }
});
