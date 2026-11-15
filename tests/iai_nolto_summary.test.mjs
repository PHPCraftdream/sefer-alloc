import assert from 'node:assert/strict';
import { test } from 'node:test';
// node:test suite for scripts/iai-nolto-summary.mjs, styled after
// tests/module_graph.test.mjs. Imports are relative to this file (no cwd
// dependence); no network, no cargo. EVERY fixture below is a synthetic
// string built HERE from arbitrary constants — no real CI data, no
// measurements; expected values are derived from the same fixture constants
// (never hand-typed twice).
import {
  buildCompareCsv,
  buildCsv,
  buildMarkdownSection,
  compareRuns,
  parseCsv,
  parseIaiLog,
} from '../scripts/iai-nolto-summary.mjs';

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
