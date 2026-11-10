import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import {
  ARMS, BUCKETS, CELL_IDS, CELL_WINDOWS, SCHEMA, expectedTupleFor,
  parseJsonl, parseSyntheticJsonl, validateRuntimeRow, validateSyntheticFields,
} from '../scripts/j13b_schema.mjs';
import { campaignSchedule } from '../scripts/j13b_campaign.mjs';
import {
  CELL_DEFINITIONS, CSV_HEADER, deriveSynthetic, equivalentWithinFivePercent, intervalContainsOne,
  pairedLogRatioCI, renderSyntheticCsv, renderSyntheticMarkdown, reportSyntheticJsonl,
} from '../scripts/j13b_stats.mjs';
import { executeChildJobs } from '../scripts/j13b_run.mjs';
import { runReportCli } from '../scripts/j13b_report.mjs';

const MIB = 1024 * 1024;
const HAND_WINDOWS = {
  'cycle-1m': ['initial'], 'cycle-128m': ['initial'], 'cycle-1g': ['initial'], 'grow-1g': ['initial'],
  'many-128x16m': ['initial'], 'sparse-128x16m': ['initial'], 'stride-64x16m': ['initial'],
  'tail-32x16m': ['initial'], 'edge-128m': ['initial'], 'churn-4x1024': ['initial'], 'churn-small-4x4096': ['initial'],
  phased: ['initial', 'sparse-training', 'sparse-trained', 'dense-training', 'dense-trained'],
  mixed: ['dense-training', 'sparse-training', 'dense-only', 'sparse-only', 'co-resident'],
};
const HAND_REFS = {
  'sparse-trained': ['sparse-training'], 'dense-trained': ['dense-training'],
  'sparse-only': ['sparse-training'], 'dense-only': ['dense-training'],
  'co-resident': ['dense-training', 'sparse-training'],
};
const trainingBucketOf = (cell, window) => (window.startsWith('sparse') && cell === 'mixed' ? '32-128' : '8-32');

function trainingFor(arm, cell, window) {
  if (arm !== 'a' && arm !== 'ab') return { status: 'not_applicable' };
  if (window === 'sparse-training' || window === 'dense-training') {
    return {
      status: 'complete', class: window.split('-')[0], bucket: trainingBucketOf(cell, window),
      eligible: 768, issued: 96, observed: 96, valid: 96, attained_share: window === 'sparse-training' ? 0.25 : 0.95,
    };
  }
  if (HAND_REFS[window]) {
    return {
      status: 'trained', training_refs: [...HAND_REFS[window]],
      attained_shares: Object.fromEntries(HAND_REFS[window].map(ref => [trainingBucketOf(cell, ref), 0.5])),
    };
  }
  return { status: 'not_applicable' };
}

function fixture(arm, patch = {}) {
  const cell = patch.cell_id ?? 'cycle-1m';
  const window = patch.window ?? 'initial';
  const buckets = Object.fromEntries(BUCKETS.map(name => [
    name, { eligible: 1, controls_issued: 0, controls_observed: 0, valid_observations: 0, share: 0.5 },
  ]));
  const requested = arm === 'mi' ? null : expectedTupleFor(arm, cell);
  const advice = arm === 'n' || arm === 'mi' ? 0 : 1;
  return {
    schema: SCHEMA, evidence_class: 'synthetic', status: 'fixture', cell_id: cell, window, arm, block: 0, repeat: 0,
    process_isolation: 'one-process-per-arm-config',
    identity: {
      run_id: 'fixture', invocation_id: `${cell}-${arm}-0-0`, source_sha256: 'a'.repeat(64), binary_sha256: 'b'.repeat(64),
      host: 'fixture', rustc: 'fixture', features: 'fixture', thp_mode: 'unknown',
    },
    requested_tuple: requested,
    resolved_tuple: arm === 'mi' ? null : expectedTupleFor(arm, cell),
    tuple_conflict_delta: arm === 'mi' ? null : 0,
    unavailable: ['runtime_adapter'],
    mechanism: {
      advised_bytes_now: advice, advised_bytes_peak: advice, advised_bytes_total: advice,
      charged_bytes: requested?.budget_bytes > 0 ? 1 : null,
      eligible_allocations: 1, controls_issued: 0, controls_observed: 0, valid_observations: 0, buckets,
    },
    sample: {
      alloc_ns: 1, touch_ns: 1, free_ns: 1, full_window_ns: 100, rss_end_bytes: 10, rss_peak_bytes: 12,
      anon_huge_pages_bytes: 0, minor_faults: 1, major_faults: 0,
    },
    training: trainingFor(arm, cell, window),
    ...patch,
  };
}

function campaignFixtures({ mutate = () => {} } = {}) {
  const rows = [];
  for (const definition of CELL_DEFINITIONS) {
    for (const arm of definition.arms) {
      for (let block = 0; block < 12; block++) {
        for (let repeat = 0; repeat < 2; repeat++) {
          const row = fixture(arm, { cell_id: definition.id, window: definition.window, block, repeat });
          row.identity.invocation_id = `${definition.id}-${arm}-${block}-${repeat}`;
          mutate(row);
          rows.push(row);
        }
      }
    }
  }
  return rows;
}

let baselineCells;
const derivedBaseline = () => (baselineCells ??= deriveSynthetic(campaignFixtures()));
const find = (cells, cell, window, pair) => cells.find(c => c.cell === cell && c.window === window && c.pair === pair);

function parseCsv(text) {
  const records = [];
  let record = [];
  let field = '';
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const char = text[i];
    if (quoted) {
      if (char === '"' && text[i + 1] === '"') { field += '"'; i++; }
      else if (char === '"') quoted = false;
      else field += char;
    } else if (char === '"') quoted = true;
    else if (char === ',') { record.push(field); field = ''; }
    else if (char === '\n') { record.push(field); records.push(record); record = []; field = ''; }
    else field += char;
  }
  return records;
}

function rejects(arm, mutate, pattern, base = {}) {
  const control = fixture(arm, base);
  assert.doesNotThrow(() => validateSyntheticFields(control));
  const mutated = structuredClone(control);
  mutate(mutated);
  assert.throws(() => validateSyntheticFields(mutated), pattern);
}

test('canonical cell/window matrix equals the hand-written preregistered lists', () => {
  assert.deepEqual(Object.fromEntries(Object.entries(CELL_WINDOWS).map(([k, v]) => [k, [...v]])), HAND_WINDOWS);
  assert.deepEqual([...CELL_IDS], Object.keys(HAND_WINDOWS));
  assert.equal(CELL_DEFINITIONS.length, 21);
});

test('expected tuples: churn-small threshold 0, others 8 MiB, f and mi not invented', () => {
  assert.equal(expectedTupleFor('a', 'churn-small-4x4096').min_bytes, 0);
  assert.equal(expectedTupleFor('a', 'cycle-1m').min_bytes, 8 * MIB);
  assert.deepEqual(expectedTupleFor('f', 'cycle-1m'), {
    min_bytes: 8 * MIB, budget_bytes: 0, gradient: 'Cautious', mode: 'Always',
    learning: 'disabled', fixed_share: 1, control_instrumentation: true,
  });
  assert.deepEqual(expectedTupleFor('mi', 'cycle-1m'), { mechanism: 'mimalloc', reference_only: true });
  assert.equal(expectedTupleFor('ab', 'cycle-1m').budget_bytes, 256 * MIB);
  assert.equal(expectedTupleFor('n', 'cycle-1m').mode, 'Off');
});

test('requested and resolved fixture tuples are independent objects', () => {
  const row = fixture('b');
  assert.notEqual(row.requested_tuple, row.resolved_tuple);
  row.requested_tuple.budget_bytes = 1;
  assert.equal(row.resolved_tuple.budget_bytes, 256 * MIB);
});

test('schedule has unique children and mi runs in every cell for every block and repeat', () => {
  const jobs = campaignSchedule();
  const keys = jobs.map(j => `${j.cell_id}|${j.arm}|${j.block}|${j.repeat}`);
  assert.equal(new Set(keys).size, jobs.length);
  assert.equal(jobs.length, 13 * 9 * 24);
  for (const cell of Object.keys(HAND_WINDOWS)) {
    assert.equal(jobs.filter(j => j.cell_id === cell && j.arm === 'mi').length, 24, cell);
    assert.ok(ARMS.every(arm => jobs.filter(j => j.cell_id === cell && j.arm === arm).length === 24));
  }
  assert.ok(jobs.every(job => !('phase' in job) && !('window' in job)));
});

test('paired CI equals an independent hand calculation', () => {
  const values = Array.from({ length: 12 }, (_, i) => 100 * Math.exp(i < 6 ? 0.1 : -0.1));
  const ci = pairedLogRatioCI(values, Array(12).fill(100));
  const half = 2.201 * Math.sqrt(0.12 / 11) / Math.sqrt(12);
  assert.ok(Math.abs(ci.ratio - 1) < 1e-12);
  assert.ok(Math.abs(ci.lo - Math.exp(-half)) < 1e-12);
  assert.ok(Math.abs(ci.hi - Math.exp(half)) < 1e-12);
  assert.equal(equivalentWithinFivePercent(ci), false);
  assert.equal(intervalContainsOne(ci), true);
  assert.throws(() => pairedLogRatioCI(values.slice(1), Array(11).fill(100)), /12 paired/);
  assert.throws(() => pairedLogRatioCI(Array(12).fill(0), Array(12).fill(1)), /positive finite/);
  assert.throws(() => pairedLogRatioCI(Array(12).fill(Number.MAX_VALUE), Array(12).fill(Number.MIN_VALUE)), /representable/);
});

test('block statistic is the minimum over repeats, not mean or first/last repeat', () => {
  const rows = campaignFixtures({
    mutate: row => {
      if (row.arm === 'd') row.sample.full_window_ns = (row.block + row.repeat) % 2 === 0 ? 200 : 100;
    },
  });
  const result = find(deriveSynthetic(rows), 'cycle-1m', 'initial', 'd/base');
  assert.equal(result.ci.ratio, 1);
  assert.equal(result.ci.lo, 1);
  assert.equal(result.ci.hi, 1);
  assert.equal(result.physical.full_window_ns_median, 150);
});

test('A/A shift makes every pair in that window unreadable with no ratios', () => {
  const rows = campaignFixtures({
    mutate: row => {
      if (row.arm === 'base2' && row.cell_id === 'phased' && row.window === 'dense-trained') row.sample.full_window_ns = 120;
    },
  });
  const cells = deriveSynthetic(rows);
  const shifted = cells.filter(c => c.cell === 'phased' && c.window === 'dense-trained');
  assert.equal(shifted.length, 8);
  assert.ok(shifted.every(c => c.aa_readable === false && c.ci === null && c.inconclusive === false));
  assert.ok(find(cells, 'phased', 'sparse-trained', 'd/base').ci !== null);
});

test('insufficient or pending adaptive training propagates INCONCLUSIVE even when A/A passes', () => {
  const insufficient = {
    status: 'insufficient_signal', class: 'sparse', bucket: '8-32',
    eligible: 1536, issued: 96, observed: 90, valid: 90, attained_share: 0.5,
  };
  const pending = { status: 'pending', class: 'dense', bucket: '8-32', eligible: 500, issued: 60, observed: 60, valid: 50, attained_share: 0.2 };
  const cells = deriveSynthetic(campaignFixtures({
    mutate: row => {
      if (row.cell_id !== 'phased' || row.arm !== 'a') return;
      if (row.window === 'sparse-training') row.training = structuredClone(insufficient);
      if (row.window === 'dense-training') row.training = structuredClone(pending);
    },
  }));
  const sparse = find(cells, 'phased', 'sparse-training', 'a/f');
  assert.equal(sparse.aa_readable, true);
  assert.equal(sparse.inconclusive, true);
  assert.equal(sparse.ci, null);
  assert.equal(sparse.training_state, 'insufficient_signal');
  assert.equal(find(cells, 'phased', 'dense-training', 'a/base').training_state, 'pending');
  assert.equal(find(cells, 'phased', 'dense-training', 'a/base').inconclusive, true);
  const trained = find(cells, 'phased', 'sparse-trained', 'a/base');
  assert.equal(trained.training_state, 'training_incomplete');
  assert.equal(trained.inconclusive, true);
  assert.equal(trained.ci, null);
  assert.equal(find(cells, 'phased', 'sparse-trained', 'ab/n').inconclusive, false);
  assert.ok(find(cells, 'phased', 'sparse-trained', 'd/base').ci !== null);
  assert.ok(find(cells, 'phased', 'initial', 'a/base').ci !== null);
  assert.match(renderSyntheticMarkdown(cells), /INCONCLUSIVE \(insufficient_signal\/not_applicable\)/);
});

test('windows are separate rows and never pooled', () => {
  const cells = derivedBaseline();
  assert.equal(cells.length, 21 * 8);
  for (const cell of ['phased', 'mixed', 'cycle-1m']) {
    const windows = cells.filter(c => c.cell === cell && c.pair === 'd/base').map(c => c.window);
    assert.deepEqual(windows, HAND_WINDOWS[cell]);
  }
  assert.equal(find(derivedBaseline(), 'mixed', 'co-resident', 'a/f').training_state, 'complete');
});

test('every validator negative control mutates one valid synthetic row', () => {
  const cases = [
    ['d', row => { row.evidence_class = 'runtime'; }, /synthetic fixture marker/],
    ['d', row => { row.window = 'warmup'; }, /cell\/window\/arm/],
    ['d', row => { row.unavailable = []; }, /unavailable adapter/],
    ['d', row => { row.block = 12; }, /block\/repeat invalid/],
    ['d', row => { row.identity.binary_sha256 = 'bad'; }, /identity binary_sha256 invalid/],
    ['d', row => { delete row.identity.host; }, /identity host missing/],
    ['d', row => { row.process_isolation = 'shared'; }, /process_isolation invalid/],
    ['d', row => { row.requested_tuple.mode = 'Off'; }, /tuple mode mismatch/],
    ['d', row => { row.resolved_tuple.extra = 1; }, /tuple extra unexpected/],
    ['d', row => { row.requested_tuple.min_bytes = 0; }, /tuple min_bytes mismatch/],
    ['a', row => { row.requested_tuple.min_bytes = 8 * MIB; }, /tuple min_bytes mismatch/, { cell_id: 'churn-small-4x4096' }],
    ['f', row => { row.requested_tuple.mode = 'Fixed'; }, /tuple mode mismatch/],
    ['f', row => { row.resolved_tuple.fixed_share = 0.5; }, /tuple fixed_share mismatch/],
    ['mi', row => { row.requested_tuple = { mode: 'Always' }; }, /mimalloc fixture must not claim/],
    ['d', row => { row.tuple_conflict_delta = 1; }, /conflict delta invalid/],
    ['d', row => { delete row.mechanism.buckets; }, /bucket map missing/],
    ['d', row => { row.mechanism.buckets = {}; }, /bucket 0-1 missing/],
    ['d', row => { row.mechanism.controls_observed = 1; }, /mechanism counter order invalid/],
    ['d', row => { row.mechanism.valid_observations = 1; }, /mechanism counter order invalid/],
    ['d', row => { row.mechanism.buckets['8-32'].controls_observed = 1; }, /bucket 8-32 counter order invalid/],
    ['d', row => { row.mechanism.buckets['8-32'].share = 2; }, /bucket 8-32 share invalid/],
    ['b', row => { row.mechanism.charged_bytes = 300_000_000; }, /charged budget exceeded/],
    ['b', row => { row.mechanism.charged_bytes = null; }, /budgeted charged ledger missing/],
    ['n', row => { row.mechanism.advised_bytes_total = 1; }, /Off advice must be zero/],
    ['d', row => { row.sample.rss_peak_bytes = 5; }, /RSS peak below end/],
    ['d', row => { row.sample.full_window_ns = 0; }, /sample full_window_ns must be positive/],
    ['d', row => { row.sample.minor_faults = Number.NaN; }, /sample minor_faults invalid/],
    ['d', row => { row.training = { status: 'complete' }; }, /non-adaptive arm requires training not_applicable/],
    ['a', row => { row.training = { status: 'complete' }; }, /adaptive initial window requires training not_applicable/],
    ['a', row => { delete row.training.attained_share; }, /training attained_share invalid/, { cell_id: 'phased', window: 'sparse-training' }],
    ['a', row => { row.training.attained_share = Number.NaN; }, /attained_share invalid/, { cell_id: 'phased', window: 'sparse-training' }],
    ['a', row => { row.training.attained_share = 1.5; }, /attained_share invalid/, { cell_id: 'phased', window: 'sparse-training' }],
    ['a', row => { row.training.class = 'dense'; }, /training class mismatch/, { cell_id: 'phased', window: 'sparse-training' }],
    ['ab', row => { row.training.bucket = '32-128'; }, /training bucket mismatch/, { cell_id: 'phased', window: 'dense-training' }],
    ['a', row => { row.training.valid = 90; }, /complete training below 96 valid/, { cell_id: 'phased', window: 'sparse-training' }],
    ['a', row => { row.training.eligible = 1537; }, /training eligible exceeds cap/, { cell_id: 'phased', window: 'sparse-training' }],
    ['a', row => { row.training.valid = 97; }, /training counter order invalid/, { cell_id: 'phased', window: 'sparse-training' }],
    [
      'a',
      row => { row.training = { ...row.training, status: 'insufficient_signal', eligible: 1000, issued: 10, observed: 10, valid: 10 }; },
      /insufficient_signal requires cap and valid<96/,
      { cell_id: 'phased', window: 'sparse-training' },
    ],
    [
      'a',
      row => { row.training = { ...row.training, status: 'pending', eligible: 1536, issued: 10, observed: 10, valid: 10 }; },
      /pending training requires uncapped/,
      { cell_id: 'phased', window: 'sparse-training' },
    ],
    ['a', row => { row.training = { status: 'not_applicable' }; }, /training status invalid/, { cell_id: 'phased', window: 'sparse-training' }],
    ['a', row => { delete row.training.training_refs; }, /trained window requires training_refs/, { cell_id: 'phased', window: 'dense-trained' }],
    [
      'a',
      row => { row.training.training_refs = ['sparse-training']; },
      /trained window requires training_refs/,
      { cell_id: 'phased', window: 'dense-trained' },
    ],
    ['ab', row => { delete row.training.attained_shares['32-128']; }, /attained_shares invalid/, { cell_id: 'mixed', window: 'co-resident' }],
    ['a', row => { row.training.attained_shares['8-32'] = Number.NaN; }, /attained_shares invalid/, { cell_id: 'mixed', window: 'dense-only' }],
  ];
  for (const [arm, mutate, pattern, base] of cases) rejects(arm, mutate, pattern, base);
});

test('mixed fixtures carry per-bucket training for dense 8-32 and sparse 32-128', () => {
  const dense = fixture('a', { cell_id: 'mixed', window: 'dense-training' }).training;
  const sparse = fixture('a', { cell_id: 'mixed', window: 'sparse-training' }).training;
  assert.deepEqual([dense.bucket, dense.class, sparse.bucket, sparse.class], ['8-32', 'dense', '32-128', 'sparse']);
  assert.deepEqual(Object.keys(fixture('a', { cell_id: 'mixed', window: 'co-resident' }).training.attained_shares).sort(), ['32-128', '8-32']);
});

test('campaign-level negative controls', () => {
  const rows = campaignFixtures();
  assert.doesNotThrow(() => deriveSynthetic(rows));
  assert.throws(() => deriveSynthetic(rows.slice(0, -1)), /missing sample/);
  assert.throws(() => deriveSynthetic([...rows, structuredClone(rows[0])]), /duplicate sample/);
  assert.throws(() => deriveSynthetic(rows.filter(row => row.arm !== 'f')), /missing arm f for cycle-1m\/initial/);
  assert.throws(() => deriveSynthetic([...rows, { ...structuredClone(rows[0]), window: 'warmup' }]), /cell\/window\/arm/);
  assert.throws(() => deriveSynthetic(campaignFixtures({
    mutate: row => { if (row.arm === 'base2') row.identity.binary_sha256 = 'c'.repeat(64); },
  })), /A\/A binary identity mismatch/);
  assert.throws(() => deriveSynthetic(campaignFixtures({
    mutate: row => { if (row.arm === 'mi' && row.cell_id === 'cycle-1m') row.identity.binary_sha256 = 'c'.repeat(64); },
  })), /A\/A binary identity mismatch/);
  assert.throws(() => deriveSynthetic(campaignFixtures({
    mutate: row => { if (row.arm === 'd' && row.cell_id === 'cycle-1m') row.identity.source_sha256 = 'c'.repeat(64); },
  })), /source identity differs/);
  assert.throws(() => deriveSynthetic(campaignFixtures({
    mutate: row => { if (row.cell_id === 'cycle-1m' && row.arm === 'd' && row.block === 1) row.identity.invocation_id = 'cycle-1m-d-0-0'; },
  })), /reused across children/);
  assert.throws(() => deriveSynthetic(campaignFixtures({
    mutate: row => {
      if (row.cell_id === 'phased' && row.arm === 'd' && row.window === 'dense-trained' && row.block === 0 && row.repeat === 0) {
        row.identity.invocation_id = 'other';
      }
    },
  })), /differs across windows/);
});

test('one invocation id is shared by all windows of a child and unique across children', () => {
  const rows = campaignFixtures();
  const phased = rows.filter(r => r.cell_id === 'phased' && r.arm === 'a' && r.block === 3 && r.repeat === 1);
  assert.equal(phased.length, 5);
  assert.equal(new Set(phased.map(r => r.identity.invocation_id)).size, 1);
  const children = new Set(rows.map(r => `${r.cell_id}|${r.arm}|${r.block}|${r.repeat}`));
  assert.equal(new Set(rows.map(r => r.identity.invocation_id)).size, children.size);
});

test('runtime rows are refused and JSONL error paths name the line', () => {
  const synthetic = fixture('d');
  assert.throws(() => validateRuntimeRow(synthetic), /runtime report admission disabled/);
  assert.throws(() => validateRuntimeRow({ evidence_class: 'runtime', status: 'sample' }), /runtime report admission disabled/);
  assert.throws(() => parseJsonl(JSON.stringify(synthetic)), /line 1: synthetic fixture cannot enter runtime JSONL/);
  assert.throws(() => parseJsonl(JSON.stringify({ evidence_class: 'runtime' })), /line 1: runtime report admission disabled/);
  assert.throws(() => parseSyntheticJsonl(`${JSON.stringify(synthetic)}\n{nope`), /line 2: invalid JSON/);
  const runtimeLabelled = JSON.stringify({ ...synthetic, evidence_class: 'runtime' });
  assert.throws(
    () => parseSyntheticJsonl(`${JSON.stringify(synthetic)}\n${runtimeLabelled}`),
    /line 2: synthetic fixture marker required/,
  );
  assert.throws(() => parseSyntheticJsonl('  \n'), /empty/);
  assert.equal(parseSyntheticJsonl(JSON.stringify(synthetic)).length, 1);
});

test('CSV header is golden and the hand-derived d/base row matches', () => {
  assert.equal(CSV_HEADER.join(','), [
    'evidence_class,cell,window,pair,aa_readable,inconclusive,training_state,baseline_training_state,ratio,ci95_lo,ci95_hi',
    'full_window_ns_median,alloc_ns_median,touch_ns_median,free_ns_median,rss_end_bytes_median,rss_peak_bytes_median',
    'anon_huge_pages_bytes_median,minor_faults_median,major_faults_median,eligible_allocations_sum,valid_observations_sum',
    'advised_bytes_peak_max,advised_bytes_total_max,charged_bytes_max,bucket_medians_json',
  ].join(','));
  const records = parseCsv(renderSyntheticCsv(derivedBaseline()));
  assert.deepEqual(records[0], [...CSV_HEADER]);
  assert.equal(records.length, 1 + 21 * 8);
  const record = records.find(r => r[1] === 'cycle-1m' && r[2] === 'initial' && r[3] === 'd/base');
  const bucket = { eligible: 1, controls_issued: 0, controls_observed: 0, valid_observations: 0, share: 0.5 };
  assert.deepEqual(record.slice(0, 26), [
    'synthetic', 'cycle-1m', 'initial', 'd/base', 'true', 'false', 'not_applicable', 'not_applicable', '1', '1', '1',
    '100', '1', '1', '1', '10', '12', '0', '1', '0', '24', '0', '1', '1', '', JSON.stringify(Object.fromEntries(BUCKETS.map(n => [n, bucket]))),
  ]);
});

test('CSV quotes commas, quotes and newlines and round-trips', () => {
  const awkward = 'a,"b"\nc';
  const csv = renderSyntheticCsv([{ ...derivedBaseline()[0], cell: awkward }]);
  assert.match(csv, /"a,""b""\nc"/);
  const records = parseCsv(csv);
  assert.equal(records[1][1], awkward);
  assert.equal(records[1].length, CSV_HEADER.length);
});

test('Markdown is bannered and raw JSONL feeds both renderers with identical cells', () => {
  const rows = campaignFixtures();
  const report = reportSyntheticJsonl(rows.map(row => JSON.stringify(row)).join('\n'));
  assert.ok(report.markdown.startsWith('# SYNTHETIC FIXTURES — NOT MEASUREMENTS\n'));
  assert.match(report.markdown, /Windows are never pooled/);
  assert.equal(report.csv, renderSyntheticCsv(derivedBaseline()));
  assert.equal(report.markdown, renderSyntheticMarkdown(derivedBaseline()));
  assert.match(report.markdown, /\| phased \| sparse-trained \| a\/f \| readable \| 1\.0000 \(1\.0000–1\.0000\) \| 100 \|/);
});

test('bucket summaries are medians, not the first sample', () => {
  const rows = campaignFixtures({
    mutate: row => {
      if (row.arm === 'd' && row.cell_id === 'cycle-1m' && row.block === 0 && row.repeat === 0) {
        row.mechanism.buckets['8-32'].eligible = 99;
        row.mechanism.eligible_allocations = 99;
      }
    },
  });
  const result = find(deriveSynthetic(rows), 'cycle-1m', 'initial', 'd/base');
  assert.equal(result.mechanism.bucket_medians['8-32'].eligible, 1);
  assert.equal(result.mechanism.eligible_allocations_sum, 23 + 99);
});

function childBytes(job, windows, { invocation = 'child-1', extra = {} } = {}) {
  const lines = windows.map(window => JSON.stringify({
    cell_id: job.cell_id, arm: job.arm, block: job.block, repeat: job.repeat, window,
    status: 'sample', workload_executed: true, identity: { invocation_id: invocation }, ...extra,
  }));
  return Buffer.from(`${lines.join('\n')}\n`, 'utf8');
}
const simpleJob = { cell_id: 'cycle-1m', arm: 'd', block: 0, repeat: 0, executable: 'stub' };
const noValidation = () => {};

test('driver persists captured bytes before child error or decoding checks', () => {
  const writes = [];
  const sink = bytes => writes.push(Buffer.from(bytes));
  assert.throws(() => executeChildJobs({
    jobs: [simpleJob], sink, spawn: () => ({ stdout: Buffer.from([0xff, 0x0a]), error: new Error('spawn failed') }),
  }), /spawn failed/);
  assert.throws(() => executeChildJobs({
    jobs: [simpleJob], sink, spawn: () => ({ stdout: Buffer.from([0xff, 0xfe, 0x0a]), status: 0 }),
  }), /not valid UTF-8/);
  assert.throws(() => executeChildJobs({
    jobs: [simpleJob], sink, spawn: () => ({ stdout: childBytes(simpleJob, ['initial']), status: 3 }),
  }), /exit=3/);
  assert.deepEqual(writes.slice(0, 2), [Buffer.from([0xff, 0x0a]), Buffer.from([0xff, 0xfe, 0x0a])]);
  assert.equal(writes.length, 3);
});

test('driver never claims preservation when stdout capture is absent', () => {
  const writes = [];
  const sink = bytes => writes.push(bytes);
  assert.throws(() => executeChildJobs({ jobs: [simpleJob], sink, spawn: () => ({ status: 0 }) }), /capture absent; nothing was preserved/);
  assert.throws(() => executeChildJobs({
    jobs: [simpleJob], sink, spawn: () => ({ stdout: null, error: new Error('ENOENT') }),
  }), /ENOENT/);
  assert.equal(writes.length, 0);
});

test('driver passes only cell/arm/block/repeat and stages all windows of a child', () => {
  const calls = [];
  const phased = { cell_id: 'phased', arm: 'a', block: 2, repeat: 1, executable: 'stub' };
  const windows = ['initial', 'sparse-training', 'sparse-trained', 'dense-training', 'dense-trained'];
  const staged = executeChildJobs({
    jobs: [phased], sink: () => {}, validate: noValidation,
    spawn: (exe, args, options) => { calls.push({ exe, args, options }); return { stdout: childBytes(phased, windows), status: 0 }; },
  });
  assert.deepEqual(calls[0].args, ['phased', 'a', '2', '1']);
  assert.equal(calls[0].options.encoding, 'buffer');
  assert.deepEqual(staged.map(r => r.window), windows);
  assert.throws(() => executeChildJobs({
    jobs: [phased], sink: () => {}, validate: noValidation,
    spawn: () => ({ stdout: childBytes(phased, windows.slice(1)), status: 0 }),
  }), /expected 5 window records/);
  assert.throws(() => executeChildJobs({
    jobs: [phased], sink: () => {}, validate: noValidation,
    spawn: () => ({ stdout: childBytes(phased, ['initial', 'initial', 'dense-trained', 'dense-training', 'sparse-training']), status: 0 }),
  }), /canonical cell windows/);
});

test('driver rejects invocation reuse across children and keeps runtime validation refusal', () => {
  const other = { ...simpleJob, block: 1 };
  assert.throws(() => executeChildJobs({
    jobs: [simpleJob, other], sink: () => {}, validate: noValidation,
    spawn: (exe, args) => ({
      stdout: childBytes({ cell_id: args[0], arm: args[1], block: Number(args[2]), repeat: Number(args[3]) }, ['initial']),
      status: 0,
    }),
  }), /invocation_id reused across children/);
  assert.throws(() => executeChildJobs({
    jobs: [simpleJob], sink: () => {}, spawn: () => ({ stdout: childBytes(simpleJob, ['initial']), status: 0 }),
  }), /runtime report admission disabled/);
});

test('report CLI default and runtime inputs are refused without writing', () => {
  const writes = [];
  const io = { readInput: () => { throw Error('must not read'); }, writePair: (...args) => writes.push(args) };
  const denied = runReportCli([], io);
  assert.equal(denied.code, 1);
  assert.match(denied.message, /runtime report admission disabled/);
  assert.equal(runReportCli(['runtime.jsonl'], io).code, 2);
  assert.equal(runReportCli(['--fixtures', 'a', 'b'], io).code, 2);
  assert.equal(writes.length, 0);
});

test('report CLI --fixtures writes labelled artifacts and rejects bad input', () => {
  const rows = campaignFixtures();
  const jsonl = rows.map(row => JSON.stringify(row)).join('\n');
  const writes = [];
  const io = bytes => ({ readInput: () => bytes, writePair: (...args) => writes.push(args) });
  const ok = runReportCli(['--fixtures', 'in.jsonl', 'out.md', 'out.csv'], io(Buffer.from(jsonl)));
  assert.equal(ok.code, 0);
  assert.equal(writes.length, 1);
  assert.deepEqual([writes[0][0], writes[0][2]], ['out.md', 'out.csv']);
  assert.match(writes[0][1], /^# SYNTHETIC FIXTURES — NOT MEASUREMENTS/);
  assert.match(writes[0][3], /^evidence_class,cell,window/);
  const mixed = `${jsonl}\n${JSON.stringify({ ...rows[0], evidence_class: 'runtime' })}`;
  const bad = runReportCli(['--fixtures', 'in', 'o.md', 'o.csv'], io(Buffer.from(mixed)));
  assert.equal(bad.code, 1);
  assert.match(bad.message, /synthetic fixture marker required/);
  assert.equal(runReportCli(['--fixtures', 'in', 'o.md', 'o.csv'], io(Buffer.from([0xff, 0xfe]))).code, 1);
  assert.equal(writes.length, 1);
});

test('Rust workload source carries every canonical cell id and window literal (static drift guard)', () => {
  const rust = readFileSync(new URL('../examples/_shared/j13b_workload.rs', import.meta.url), 'utf8');
  for (const id of CELL_IDS) assert.ok(rust.includes(`"${id}"`), `cell id ${id} missing from Rust source`);
  for (const [id, windows] of Object.entries(CELL_WINDOWS)) {
    for (const window of windows) assert.ok(rust.includes(`"${window}"`), `window ${window} (${id}) missing from Rust source`);
  }
});
