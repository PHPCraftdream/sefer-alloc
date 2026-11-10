import {
  ADAPTIVE_ARMS, BUCKETS, CELL_MATRIX, SCHEMA, TRAINED_WINDOWS,
  parseSyntheticJsonl, validateSyntheticFields,
} from './j13b_schema.mjs';

export const T_CRIT_11 = 2.201;
export const EQUIVALENCE_MARGIN = Object.freeze([0.95, 1.05]);
export const PAIRS = Object.freeze([
  ['base2', 'base'], ['d', 'base'], ['n', 'base'], ['a', 'f'], ['a', 'base'], ['b', 'n'], ['ab', 'b'], ['ab', 'n'],
]);
const SUBMETRICS = Object.freeze([
  'full_window_ns', 'alloc_ns', 'touch_ns', 'free_ns',
  'rss_end_bytes', 'rss_peak_bytes', 'anon_huge_pages_bytes', 'minor_faults', 'major_faults',
]);
const BUCKET_FIELDS = Object.freeze(['eligible', 'controls_issued', 'controls_observed', 'valid_observations', 'share']);
const INCONCLUSIVE_STATES = Object.freeze(['pending', 'insufficient_signal', 'training_incomplete']);

const mean = values => values.reduce((sum, value) => sum + value, 0) / values.length;
const sum = values => values.reduce((total, value) => total + value, 0);

function sampleSd(values) {
  const center = mean(values);
  return Math.sqrt(sum(values.map(value => (value - center) ** 2)) / (values.length - 1));
}

function median(values) {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = sorted.length >> 1;
  return sorted.length % 2 ? sorted[middle] : (sorted[middle - 1] + sorted[middle]) / 2;
}

export function pairedLogRatioCI(numerators, denominators) {
  if (!Array.isArray(numerators) || !Array.isArray(denominators) || numerators.length !== 12 || denominators.length !== 12) {
    throw Error('12 paired blocks required');
  }
  const logs = numerators.map((numerator, index) => {
    const denominator = denominators[index];
    if (!(Number.isFinite(numerator) && numerator > 0 && Number.isFinite(denominator) && denominator > 0)) {
      throw Error('positive finite samples required');
    }
    return Math.log(numerator) - Math.log(denominator);
  });
  const center = mean(logs);
  const half = T_CRIT_11 * sampleSd(logs) / Math.sqrt(12);
  const result = { ratio: Math.exp(center), lo: Math.exp(center - half), hi: Math.exp(center + half) };
  if (!Object.values(result).every(value => Number.isFinite(value) && value > 0) || result.lo > result.hi) {
    throw Error('ratio interval is not representable');
  }
  return result;
}

const validInterval = interval => Number.isFinite(interval?.lo) && Number.isFinite(interval?.hi)
  && interval.lo > 0 && interval.lo <= interval.hi;
export const intervalContainsOne = interval => validInterval(interval) && interval.lo <= 1 && interval.hi >= 1;
export const equivalentWithinFivePercent = interval => validInterval(interval)
  && interval.lo >= EQUIVALENCE_MARGIN[0] && interval.hi <= EQUIVALENCE_MARGIN[1];

export const CELL_DEFINITIONS = Object.freeze(CELL_MATRIX.flatMap(
  cell => cell.windows.map(window => ({ id: cell.id, window, arms: cell.arms })),
));

const sampleKey = (cell, window, arm, block, repeat) => `${cell}|${window}|${arm}|${block}|${repeat}`;

function indexRows(rows) {
  const index = new Map();
  for (const row of rows) {
    const key = sampleKey(row.cell_id, row.window, row.arm, row.block, row.repeat);
    if (index.has(key)) throw Error(`duplicate sample ${key}`);
    index.set(key, row);
  }
  return index;
}

function armRows(index, cell, window, arm) {
  const rows = [];
  for (let block = 0; block < 12; block++) {
    for (let repeat = 0; repeat < 2; repeat++) {
      const row = index.get(sampleKey(cell, window, arm, block, repeat));
      if (!row) throw Error(`missing sample ${sampleKey(cell, window, arm, block, repeat)}`);
      rows.push(row);
    }
  }
  return rows;
}

function blockMinimums(index, cell, window, arm, metric) {
  return Array.from({ length: 12 }, (_, block) => Math.min(
    ...[0, 1].map(repeat => {
      const row = index.get(sampleKey(cell, window, arm, block, repeat));
      if (!row) throw Error(`missing sample ${sampleKey(cell, window, arm, block, repeat)}`);
      return row.sample[metric];
    }),
  ));
}

export function validateSyntheticCampaign(rows) {
  if (!Array.isArray(rows) || rows.length === 0) throw Error('synthetic fixture rows required');
  for (const row of rows) validateSyntheticFields(row);
  const index = indexRows(rows);
  for (const definition of CELL_DEFINITIONS) {
    for (const arm of definition.arms) {
      if (![...index.keys()].some(key => key.startsWith(`${definition.id}|${definition.window}|${arm}|`))) {
        throw Error(`missing arm ${arm} for ${definition.id}/${definition.window}`);
      }
      armRows(index, definition.id, definition.window, arm);
    }
  }
  const childOfInvocation = new Map();
  const invocationOfChild = new Map();
  for (const row of rows) {
    const child = `${row.cell_id}|${row.arm}|${row.block}|${row.repeat}`;
    const invocation = row.identity.invocation_id;
    if (invocationOfChild.has(child) && invocationOfChild.get(child) !== invocation) {
      throw Error(`invocation id differs across windows of child ${child}`);
    }
    invocationOfChild.set(child, invocation);
    if (childOfInvocation.has(invocation) && childOfInvocation.get(invocation) !== child) {
      throw Error(`invocation id ${invocation} reused across children`);
    }
    childOfInvocation.set(invocation, child);
  }
  const sourceHashes = new Set(rows.map(row => row.identity.source_sha256.toLowerCase()));
  if (sourceHashes.size !== 1) throw Error('source identity differs across rows');
  for (const definition of CELL_DEFINITIONS) {
    for (let block = 0; block < 12; block++) {
      for (let repeat = 0; repeat < 2; repeat++) {
        const binary = arm => index.get(sampleKey(definition.id, definition.window, arm, block, repeat)).identity.binary_sha256.toLowerCase();
        if (binary('base') !== binary('base2') || binary('base') !== binary('mi')) {
          throw Error(`A/A binary identity mismatch ${definition.id}/${definition.window}/${block}/${repeat}`);
        }
      }
    }
  }
  return rows;
}

function rowTrainingState(row, index) {
  if (!ADAPTIVE_ARMS.includes(row.arm)) return 'not_applicable';
  const status = row.training.status;
  if (status === 'not_applicable') return 'not_applicable';
  if (status !== 'trained') return status === 'complete' ? 'complete' : status;
  const incomplete = TRAINED_WINDOWS[row.window].some(ref => {
    const referenced = index.get(sampleKey(row.cell_id, ref, row.arm, row.block, row.repeat));
    return referenced.training.status !== 'complete';
  });
  return incomplete ? 'training_incomplete' : 'complete';
}

function armTrainingState(rows, index) {
  const states = new Set(rows.map(row => rowTrainingState(row, index)));
  for (const state of INCONCLUSIVE_STATES) if (states.has(state)) return state;
  return states.has('complete') ? 'complete' : 'not_applicable';
}

function summarize(rows) {
  const physical = Object.fromEntries(SUBMETRICS.map(key => [`${key}_median`, median(rows.map(row => row.sample[key]))]));
  const charged = rows.map(row => row.mechanism.charged_bytes);
  return {
    physical,
    mechanism: {
      eligible_allocations_sum: sum(rows.map(row => row.mechanism.eligible_allocations)),
      valid_observations_sum: sum(rows.map(row => row.mechanism.valid_observations)),
      advised_bytes_peak_max: Math.max(...rows.map(row => row.mechanism.advised_bytes_peak)),
      advised_bytes_total_max: Math.max(...rows.map(row => row.mechanism.advised_bytes_total)),
      charged_bytes_max: charged.every(value => value === null) ? null : Math.max(...charged.map(value => value ?? 0)),
      bucket_medians: Object.fromEntries(BUCKETS.map(bucket => [
        bucket,
        Object.fromEntries(BUCKET_FIELDS.map(field => [field, median(rows.map(row => row.mechanism.buckets[bucket][field]))])),
      ])),
    },
  };
}

export function deriveSynthetic(rows) {
  validateSyntheticCampaign(rows);
  const index = indexRows(rows);
  const output = [];
  for (const { id: cell, window } of CELL_DEFINITIONS) {
    const minimum = arm => blockMinimums(index, cell, window, arm, 'full_window_ns');
    const baseMinimums = minimum('base');
    const aaRatios = minimum('base2').map((value, block) => value / baseMinimums[block]);
    const aaInterval = pairedLogRatioCI(aaRatios, Array(12).fill(1));
    const readable = intervalContainsOne(aaInterval);
    const states = {};
    const rowsByArm = {};
    for (const arm of new Set(PAIRS.flat())) {
      rowsByArm[arm] = armRows(index, cell, window, arm);
      states[arm] = armTrainingState(rowsByArm[arm], index);
    }
    for (const [arm, baseline] of PAIRS) {
      const inconclusive = [arm, baseline].some(name => INCONCLUSIVE_STATES.includes(states[name]));
      const ci = readable && !inconclusive ? pairedLogRatioCI(minimum(arm), minimum(baseline)) : null;
      output.push({
        evidence_class: 'synthetic', cell, window, arm, pair: `${arm}/${baseline}`,
        aa_readable: readable, inconclusive, training_state: states[arm], baseline_training_state: states[baseline],
        ci, ...summarize(rowsByArm[arm]),
      });
    }
  }
  return output;
}

function csvField(value) {
  const text = value === null || value === undefined ? '' : String(value);
  return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
}

export const CSV_HEADER = Object.freeze([
  'evidence_class', 'cell', 'window', 'pair', 'aa_readable', 'inconclusive', 'training_state', 'baseline_training_state',
  'ratio', 'ci95_lo', 'ci95_hi',
  ...SUBMETRICS.map(key => `${key}_median`),
  'eligible_allocations_sum', 'valid_observations_sum', 'advised_bytes_peak_max', 'advised_bytes_total_max', 'charged_bytes_max',
  'bucket_medians_json',
]);

export function renderSyntheticCsv(cells) {
  const lines = cells.map(cell => [
    cell.evidence_class, cell.cell, cell.window, cell.pair, cell.aa_readable, cell.inconclusive,
    cell.training_state, cell.baseline_training_state, cell.ci?.ratio, cell.ci?.lo, cell.ci?.hi,
    ...SUBMETRICS.map(key => cell.physical[`${key}_median`]),
    cell.mechanism.eligible_allocations_sum, cell.mechanism.valid_observations_sum,
    cell.mechanism.advised_bytes_peak_max, cell.mechanism.advised_bytes_total_max, cell.mechanism.charged_bytes_max,
    JSON.stringify(cell.mechanism.bucket_medians),
  ].map(csvField).join(','));
  return `${CSV_HEADER.join(',')}\n${lines.join('\n')}\n`;
}

function markdownStatus(cell) {
  if (cell.inconclusive) return `INCONCLUSIVE (${cell.training_state}/${cell.baseline_training_state})`;
  return cell.aa_readable ? 'readable' : 'UNREADABLE — A/A failed';
}

export function renderSyntheticMarkdown(cells) {
  const lines = cells.map(cell => {
    const ratio = cell.ci
      ? `${cell.ci.ratio.toFixed(4)} (${cell.ci.lo.toFixed(4)}–${cell.ci.hi.toFixed(4)})`
      : '—';
    return `| ${cell.cell} | ${cell.window} | ${cell.pair} | ${markdownStatus(cell)} | ${ratio} | ${cell.physical.full_window_ns_median} |`;
  });
  return [
    '# SYNTHETIC FIXTURES — NOT MEASUREMENTS',
    '',
    `${SCHEMA}; no runtime evidence or verdict. Windows are never pooled.`,
    'Timing columns are medians; the full-window median is a separate statistic from the alloc/touch/free medians in the CSV.',
    'Aggregate counters in the CSV are labelled by their max/sum/median aggregation and are not physical proof;',
    'advised/charged bytes are kept apart from RSS, THP and fault columns.',
    '',
    '| Cell | Window | Pair | Status | Ratio (95% CI) | Full-window median (ns) |',
    '|---|---|---|---|---:|---:|',
    ...lines,
    '',
  ].join('\n');
}

export function parseAndDerive(text) {
  void text;
  throw Error('runtime report admission disabled: no reviewed SeferAlloc adapter exists');
}

export function reportSyntheticRows(rows) {
  const cells = deriveSynthetic(rows);
  return { cells, csv: renderSyntheticCsv(cells), markdown: renderSyntheticMarkdown(cells) };
}

export function reportSyntheticJsonl(text) {
  return reportSyntheticRows(parseSyntheticJsonl(text));
}
