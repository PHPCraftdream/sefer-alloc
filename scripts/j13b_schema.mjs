export const SCHEMA = 'j13b.raw.v4';
export const ARMS = Object.freeze(['d', 'n', 'f', 'b', 'a', 'ab', 'base', 'base2', 'mi']);
export const ADAPTIVE_ARMS = Object.freeze(['a', 'ab']);
export const BUCKETS = Object.freeze(['0-1', '1-8', '8-32', '32-128', '128-512', '512-inf']);
export const METRICS = Object.freeze([
  'alloc_ns', 'touch_ns', 'free_ns', 'full_window_ns',
  'rss_end_bytes', 'rss_peak_bytes', 'anon_huge_pages_bytes', 'minor_faults', 'major_faults',
]);
export const TRAINING_CAP = 1536;
export const TRAINING_TARGET = 96;
const MIB = 1024 * 1024;
const INITIAL_ONLY = Object.freeze(['initial']);

const SIMPLE_CELLS = [
  { id: 'cycle-1m', scenario: 'cycle', size_mib: 1, count: 1 },
  { id: 'cycle-128m', scenario: 'cycle', size_mib: 128, count: 1 },
  { id: 'cycle-1g', scenario: 'cycle', size_mib: 1024, count: 1 },
  { id: 'grow-1g', scenario: 'grow', size_mib: 1024, count: 1, growth_start_mib: 1 },
  { id: 'many-128x16m', scenario: 'many', size_mib: 16, count: 128, touch: 'dense' },
  { id: 'sparse-128x16m', scenario: 'sparse', size_mib: 16, count: 128, touch: 'first-byte' },
  { id: 'stride-64x16m', scenario: 'stride', size_mib: 16, count: 64, touch: 'every-16th-2m-region' },
  { id: 'tail-32x16m', scenario: 'tail', size_mib: 16, count: 32, touch: 'last-quarter' },
  { id: 'edge-128m', scenario: 'edge', size_mib: 128, count: 1, touch: 'two-pages' },
  {
    id: 'churn-4x1024', scenario: 'churn', size_mib: 16, count: 4, threads: 4,
    operations_per_thread: 1024, minimum_request_mib: 8, touch: 'quarter', cache: 'disabled',
  },
  {
    id: 'churn-small-4x4096', scenario: 'churn-small', size_mib: 1, count: 4, threads: 4,
    operations_per_thread: 4096, minimum_request_bytes: 0, touch: 'quarter', cache: 'disabled',
  },
];

export const CELL_MATRIX = Object.freeze([
  ...SIMPLE_CELLS.map(cell => ({ ...cell, windows: INITIAL_ONLY, arms: ARMS })),
  {
    id: 'phased', scenario: 'phased', size_mib: 16, count: 1, cache: 'disabled',
    windows: ['initial', 'sparse-training', 'sparse-trained', 'dense-training', 'dense-trained'],
    arms: ARMS,
  },
  {
    id: 'mixed', scenario: 'mixed', cache: 'disabled',
    buckets: [
      { size_mib: 16, count: 8, bucket: '8-32', touch: 'dense' },
      { size_mib: 64, count: 8, bucket: '32-128', touch: 'sparse-two-percent' },
    ],
    windows: ['dense-training', 'sparse-training', 'dense-only', 'sparse-only', 'co-resident'],
    arms: ARMS,
  },
]);
export const CELL_WINDOWS = Object.freeze(Object.fromEntries(CELL_MATRIX.map(cell => [cell.id, cell.windows])));
export const CELL_IDS = Object.freeze(CELL_MATRIX.map(cell => cell.id));

export const TRAINING_WINDOWS = Object.freeze({
  'sparse-training': { class: 'sparse', buckets: { phased: '8-32', mixed: '32-128' } },
  'dense-training': { class: 'dense', buckets: { phased: '8-32', mixed: '8-32' } },
});
export const TRAINED_WINDOWS = Object.freeze({
  'sparse-trained': ['sparse-training'],
  'dense-trained': ['dense-training'],
  'sparse-only': ['sparse-training'],
  'dense-only': ['dense-training'],
  'co-resident': ['dense-training', 'sparse-training'],
});
export const trainingBucket = (cellId, window) => TRAINING_WINDOWS[window].buckets[cellId];

export function expectedTupleFor(arm, cellId) {
  const common = {
    min_bytes: cellId === 'churn-small-4x4096' ? 0 : 8 * MIB,
    budget_bytes: 0,
    gradient: 'Cautious',
  };
  switch (arm) {
    case 'd':
    case 'base':
    case 'base2':
      return { ...common, mode: 'Always' };
    case 'n':
      return { ...common, mode: 'Off' };
    case 'f':
      return { ...common, mode: 'Always', learning: 'disabled', fixed_share: 1, control_instrumentation: true };
    case 'b':
      return { ...common, mode: 'Always', budget_bytes: 256 * MIB };
    case 'a':
      return { ...common, mode: 'Auto' };
    case 'ab':
      return { ...common, mode: 'Auto', budget_bytes: 256 * MIB };
    case 'mi':
      return { mechanism: 'mimalloc', reference_only: true };
    default:
      throw Error(`unknown arm ${String(arm)}`);
  }
}

const isCount = value => Number.isSafeInteger(value) && value >= 0;
const isUnit = value => typeof value === 'number' && Number.isFinite(value) && value >= 0 && value <= 1;
const fail = message => { throw Error(message); };

function validateIdentity(row) {
  const identity = row.identity;
  if (!identity || typeof identity !== 'object') fail('identity missing');
  for (const key of ['run_id', 'invocation_id', 'host', 'rustc', 'features', 'thp_mode']) {
    if (typeof identity[key] !== 'string' || !identity[key]) fail(`identity ${key} missing`);
  }
  for (const key of ['source_sha256', 'binary_sha256']) {
    if (typeof identity[key] !== 'string' || !/^[a-f0-9]{64}$/i.test(identity[key])) fail(`identity ${key} invalid`);
  }
  if (!['always', 'madvise', 'never', 'unknown'].includes(identity.thp_mode)) fail('THP mode invalid');
}

function validateTuples(row) {
  if (row.arm === 'mi') {
    if (row.requested_tuple !== null || row.resolved_tuple !== null || row.tuple_conflict_delta !== null) {
      fail('mimalloc fixture must not claim THP tuple');
    }
    return;
  }
  const expected = expectedTupleFor(row.arm, row.cell_id);
  for (const name of ['requested_tuple', 'resolved_tuple']) {
    const actual = row[name];
    if (!actual || typeof actual !== 'object') fail('fixture tuple evidence missing');
    for (const key of Object.keys(expected)) if (actual[key] !== expected[key]) fail(`tuple ${key} mismatch`);
    for (const key of Object.keys(actual)) if (!(key in expected)) fail(`tuple ${key} unexpected`);
  }
  if (row.tuple_conflict_delta !== 0) fail('fixture conflict delta invalid');
}

function validateMechanism(row) {
  const m = row.mechanism;
  if (!m || typeof m !== 'object') fail('mechanism missing');
  const counters = [
    'advised_bytes_now', 'advised_bytes_peak', 'advised_bytes_total',
    'eligible_allocations', 'controls_issued', 'controls_observed', 'valid_observations',
  ];
  for (const key of counters) if (!isCount(m[key])) fail(`mechanism ${key} invalid`);
  const ordered = m.advised_bytes_now <= m.advised_bytes_peak
    && m.advised_bytes_peak <= m.advised_bytes_total
    && m.valid_observations <= m.controls_observed
    && m.controls_observed <= m.controls_issued
    && m.controls_issued <= m.eligible_allocations;
  if (!ordered) fail('mechanism counter order invalid');
  if (m.charged_bytes !== null && !isCount(m.charged_bytes)) fail('charged ledger invalid');
  const budget = row.requested_tuple?.budget_bytes ?? 0;
  if (m.charged_bytes === null && budget > 0) fail('budgeted charged ledger missing');
  if (m.charged_bytes !== null && budget > 0 && m.charged_bytes > budget) fail('charged budget exceeded');
  if (row.arm === 'n' && m.advised_bytes_total !== 0) fail('Off advice must be zero');
  if (!m.buckets || typeof m.buckets !== 'object') fail('bucket map missing');
  for (const name of BUCKETS) {
    const bucket = m.buckets[name];
    if (!bucket || typeof bucket !== 'object') fail(`bucket ${name} missing`);
    for (const key of ['eligible', 'controls_issued', 'controls_observed', 'valid_observations']) {
      if (!isCount(bucket[key])) fail(`bucket ${name}.${key} invalid`);
    }
    if (!(bucket.valid_observations <= bucket.controls_observed
      && bucket.controls_observed <= bucket.controls_issued
      && bucket.controls_issued <= bucket.eligible)) fail(`bucket ${name} counter order invalid`);
    if (!isUnit(bucket.share)) fail(`bucket ${name} share invalid`);
  }
}

function validateSample(row) {
  const sample = row.sample;
  if (!sample || typeof sample !== 'object') fail('sample missing');
  for (const key of METRICS) {
    if (typeof sample[key] !== 'number' || !Number.isFinite(sample[key]) || sample[key] < 0) fail(`sample ${key} invalid`);
  }
  for (const key of ['alloc_ns', 'touch_ns', 'free_ns', 'full_window_ns']) {
    if (sample[key] === 0) fail(`sample ${key} must be positive`);
  }
  if (sample.rss_peak_bytes < sample.rss_end_bytes) fail('RSS peak below end');
}

function validateTrainingWindow(row, training) {
  const spec = TRAINING_WINDOWS[row.window];
  if (!['complete', 'insufficient_signal', 'pending'].includes(training.status)) fail('training status invalid');
  if (training.class !== spec.class) fail('training class mismatch');
  if (training.bucket !== spec.buckets[row.cell_id]) fail('training bucket mismatch');
  for (const key of ['eligible', 'issued', 'observed', 'valid']) if (!isCount(training[key])) fail('training counters invalid');
  if (training.eligible > TRAINING_CAP) fail('training eligible exceeds cap');
  if (!(training.valid <= training.observed && training.observed <= training.issued && training.issued <= training.eligible)) {
    fail('training counter order invalid');
  }
  if (!isUnit(training.attained_share)) fail('training attained_share invalid');
  if (training.status === 'complete' && training.valid < TRAINING_TARGET) fail('complete training below 96 valid');
  if (training.status === 'insufficient_signal' && !(training.eligible === TRAINING_CAP && training.valid < TRAINING_TARGET)) {
    fail('insufficient_signal requires cap and valid<96');
  }
  if (training.status === 'pending' && !(training.eligible < TRAINING_CAP && training.valid < TRAINING_TARGET)) {
    fail('pending training requires uncapped valid<96');
  }
}

function validateTrainedWindow(row, training) {
  const refs = TRAINED_WINDOWS[row.window];
  if (training.status !== 'trained' || !Array.isArray(training.training_refs)
    || training.training_refs.length !== refs.length || !refs.every((ref, i) => training.training_refs[i] === ref)) {
    fail('trained window requires training_refs');
  }
  const shares = training.attained_shares;
  const buckets = refs.map(ref => trainingBucket(row.cell_id, ref));
  if (!shares || typeof shares !== 'object' || Object.keys(shares).length !== buckets.length
    || !buckets.every(bucket => isUnit(shares[bucket]))) fail('trained window attained_shares invalid');
}

function validateTraining(row) {
  const training = row.training;
  if (!training || typeof training !== 'object') fail('training evidence missing');
  if (!ADAPTIVE_ARMS.includes(row.arm)) {
    if (training.status !== 'not_applicable') fail('non-adaptive arm requires training not_applicable');
    return;
  }
  if (TRAINING_WINDOWS[row.window]) validateTrainingWindow(row, training);
  else if (TRAINED_WINDOWS[row.window]) validateTrainedWindow(row, training);
  else if (training.status !== 'not_applicable') fail('adaptive initial window requires training not_applicable');
}

export function validateSyntheticFields(row) {
  if (!row || typeof row !== 'object' || row.schema !== SCHEMA
    || row.evidence_class !== 'synthetic' || row.status !== 'fixture') fail('synthetic fixture marker required');
  if (!CELL_WINDOWS[row.cell_id]?.includes(row.window) || !ARMS.includes(row.arm)) fail('cell/window/arm missing or invalid');
  if (!Array.isArray(row.unavailable) || !row.unavailable.includes('runtime_adapter')) fail('unavailable adapter evidence missing');
  if (!isCount(row.block) || row.block >= 12 || !isCount(row.repeat) || row.repeat >= 2) fail('block/repeat invalid');
  if (row.process_isolation !== 'one-process-per-arm-config') fail('process_isolation invalid');
  validateIdentity(row);
  validateTuples(row);
  validateMechanism(row);
  validateSample(row);
  validateTraining(row);
  return row;
}

export function validateRuntimeRow() {
  throw Error('runtime report admission disabled: no reviewed SeferAlloc adapter exists');
}

function parseLines(text, validateLine) {
  return text.trim().split(/\r?\n/).map((line, index) => {
    let row;
    try {
      row = JSON.parse(line);
    } catch (error) {
      throw Error(`line ${index + 1}: invalid JSON: ${error.message}`);
    }
    try {
      return validateLine(row);
    } catch (error) {
      throw Error(`line ${index + 1}: ${error.message}`);
    }
  });
}

export function parseJsonl(text) {
  if (typeof text !== 'string' || !text.trim()) throw Error('raw JSONL empty');
  return parseLines(text, row => {
    if (row?.evidence_class === 'synthetic') throw Error('synthetic fixture cannot enter runtime JSONL');
    return validateRuntimeRow(row);
  });
}

export function parseSyntheticJsonl(text) {
  if (typeof text !== 'string' || text.length > 32 * 1024 * 1024 || !text.trim()) {
    throw Error('synthetic raw JSONL empty or exceeds 32 MiB');
  }
  return parseLines(text, validateSyntheticFields);
}

export function syntheticFixture(fields) {
  return { schema: SCHEMA, evidence_class: 'synthetic', status: 'fixture', ...fields };
}
