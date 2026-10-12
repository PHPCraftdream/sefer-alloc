// Derives the PRF01 run summaries (CSV + Markdown tables) from the judge's saved output and the judge input.
// Every number is recomputed from the input rows and asserted equal to the judge's own saved table before it is written.
// Usage: node scripts/r19_prf01_gate_excerpt.mjs --judge-output FILE --judge-input FILE --out-dir DIR
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve } from 'node:path';

const args = process.argv.slice(2), opt = {};
for (let i = 0; i < args.length; i += 2) opt[args[i]] = args[i + 1];
for (const k of ['--judge-output', '--judge-input', '--out-dir']) assert.ok(opt[k], 'missing ' + k);
const out = JSON.parse(readFileSync(opt['--judge-output'], 'utf8')), inp = JSON.parse(readFileSync(opt['--judge-input'], 'utf8'));
const dir = resolve(opt['--out-dir']); mkdirSync(dir, { recursive: true });

const HOT = ['small_churn_16b', 'churn_256b', 'churn_write_256b', 'aligned_churn_640b_a128'];
const RATIO = ['multiseg_cold_256k', 'seg_cycle_decommit_256k'];
const GROWTH = ['growth16_n', 'growth16_2n', 'growth16_4n'];
const PRODUCER = 'r19_prf01_joined_producer_same_word', PUBLICATION = 'r19_prf01_publication', PRODUCER_N = 4096;
const A_LABELS = ['A1', 'A2'], B_LABELS = ['B1', 'B2'];
const csv = rows => rows.map(r => r.join(',')).join('\n') + '\n';
const md = (head, rows) => ['| ' + head.join(' | ') + ' |', '|' + head.map(() => '---').join('|') + '|', ...rows.map(r => '| ' + r.join(' | ') + ' |')].join('\n') + '\n';

const ir = new Map();
for (const r of inp.iai) { const k = r.name + '/' + r.arm; assert.ok(!ir.has(k), 'duplicate ' + k); ir.set(k, r.Ir); }
const get = (name, arm) => { const v = ir.get(name + '/' + arm); assert.ok(Number.isInteger(v), 'missing ' + name + '/' + arm); return v; };
const names = [...new Set(inp.iai.map(r => r.name))].sort();
assert.equal(names.length, 24);
for (const n of names) for (const a of [...A_LABELS, ...B_LABELS, 'C0']) get(n, a);

// expectation per judge row, recomputed from the input
const expectedFor = (name, a, b) => {
  const A = get(name, a), B = get(name, b), delta = B - A;
  if (RATIO.includes(name)) return { mode: 'mode1', value: B / A, pass: B / A <= 0.25 };
  if (HOT.includes(name)) { const T = Math.max(10, Math.abs(get(name, 'A1') - get(name, 'A2')), Math.abs(get(name, 'C0') - get(name, 'A1'))); return { mode: 'mode1', T, pass: Math.abs(delta) <= T }; }
  if (name === PRODUCER) return { mode: 'mode2', value: delta / PRODUCER_N, pass: delta / PRODUCER_N <= 6 };
  if (name.startsWith('mimalloc_')) return { mode: 'mode1', pass: delta === 0 };
  return { mode: 'descriptive', pass: true };
};
const judged = out.tables.iai;
assert.equal(judged.length, names.length * 4, 'one judge row per cell and cross pair');
const iaiRows = [];
for (const row of judged) {
  const e = expectedFor(row.name, row.A, row.B), A = get(row.name, row.A), B = get(row.name, row.B);
  assert.equal(row.delta, B - A, 'delta ' + row.name); assert.equal(row.mode, e.mode, 'mode ' + row.name);
  assert.equal(row.pass, e.pass, 'pass ' + row.name + ' ' + row.A + row.B);
  if (e.T !== undefined) assert.equal(row.T, e.T, 'T ' + row.name);
  if (e.value !== undefined) assert.ok(Math.abs(row.value - e.value) < 1e-12, 'value ' + row.name);
  iaiRows.push([row.name, row.A, row.B, A, B, row.delta, row.mode, row.T ?? '', row.value ?? '', row.pass]);
}
writeFileSync(resolve(dir, 'iai_cross_pairs.csv'), csv([['name', 'A', 'B', 'IrA', 'IrB', 'deltaIr', 'mode', 'T', 'value', 'pass'], ...iaiRows]));

// per-cell Ir table (all labels bit-identical within A1/A2/C0 and within B1/B2 is itself a recorded fact)
const cellRows = names.map(n => {
  const a = get(n, 'A1'), b = get(n, 'B1');
  assert.equal(get(n, 'A2'), a); assert.equal(get(n, 'C0'), a); assert.equal(get(n, 'B2'), b);
  return [n, a, b, b - a];
});
writeFileSync(resolve(dir, 'iai_summary.csv'), csv([['name', 'IrA(A1=A2=C0)', 'IrB(B1=B2)', 'deltaIr'], ...cellRows]));

// criteria
const crit = [];
for (const n of RATIO) { const a = get(n, 'A1'), b = get(n, 'B1'); crit.push([`${n} B/A`, `${b}/${a}`, (b / a).toFixed(6), '<= 0.25', b / a <= 0.25]); }
const slopes = out.tables.slope; assert.equal(slopes.length, 2);
for (const s of slopes) {
  const [n1, n2, n4] = GROWTH.map(n => get(n, s.arm)); const num = n4 - n2, den = n2 - n1;
  assert.equal(s.numerator, num); assert.equal(s.denominator, den); assert.ok(den > 0 && Math.abs(s.value - num / den) < 1e-12 && s.pass === (num / den <= 2.5));
  crit.push([`growth16 slope ${s.arm} (Ir4n-Ir2n)/(Ir2n-Irn)`, `${num}/${den}`, (num / den).toFixed(6), '<= 2.5', s.pass]);
}
for (const n of HOT) {
  const rows = judged.filter(r => r.name === n); assert.equal(rows.length, 4);
  const T = rows[0].T, deltas = [...new Set(rows.map(r => r.delta))]; assert.equal(deltas.length, 1);
  crit.push([`hot control ${n} |B-A|`, `${deltas[0]} vs T=${T} (4 cross pairs)`, String(Math.abs(deltas[0])), '<= T', rows.every(r => r.pass)]);
}
const mim = judged.filter(r => r.name.startsWith('mimalloc_')); assert.equal(mim.length, 13 * 4);
crit.push(['mimalloc cells delta (13 cells x 4 pairs)', `${mim.filter(r => r.delta === 0).length}/${mim.length} rows with delta 0`, '0', '== 0', mim.every(r => r.pass)]);
const prod = judged.filter(r => r.name === PRODUCER); assert.equal(prod.length, 4);
crit.push([`producer (IrB-IrA)/${PRODUCER_N} remote frees`, `${prod[0].delta}/${PRODUCER_N}`, (prod[0].delta / PRODUCER_N).toFixed(5), '<= 6', prod.every(r => r.pass)]);
const size = out.tables.size[0]; assert.equal(size.numerator, inp.sizeOf.B - inp.sizeOf.A);
crit.push(['size_of SmallSidecar B-A (bytes)', `${inp.sizeOf.B}-${inp.sizeOf.A}`, String(size.numerator), '== 512', size.pass]);
writeFileSync(resolve(dir, 'criteria_summary.csv'), csv([['criterion', 'numerator/denominator or operands', 'value', 'frozen limit', 'pass'], ...crit]));

// oracle
const orows = [];
for (const o of out.tables.oracle) for (const a of o.arms) orows.push([o.name, a.arm, a.pending_words_visited, a.pending_words_exchanged, a.nonempty_cuts, a.discovery_records_reclaimed, a.routed_scans, a.summary_words_visited, a.summary_rmw, a.summary_load, a.strict_trim_remaining ?? '']);
writeFileSync(resolve(dir, 'oracle_summary.csv'), csv([['name', 'arm', 'pending_words_visited', 'pending_words_exchanged', 'nonempty_cuts', 'discovery_records_reclaimed', 'routed_scans', 'summary_words_visited', 'summary_rmw', 'summary_load', 'strict_trim_remaining'], ...orows]));

// native: descriptive only (the judge did not evaluate this axis)
const median = xs => { const s = [...xs].sort((x, y) => x - y), m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const nrows = [];
for (const n of [1, 2, 4, 8]) for (const arm of ['A', 'AA', 'B']) {
  const rs = inp.native.filter(r => r.n === n && r.arm === arm); assert.equal(rs.length, 9);
  for (const k of ['p50Ns', 'p99Ns', 'nsPerOp']) { const xs = rs.map(r => r[k]); nrows.push([n, arm, k, rs.length, median(xs), Math.min(...xs), Math.max(...xs), xs.filter(x => x === 0).length]); }
}
writeFileSync(resolve(dir, 'native_summary.csv'), csv([['n', 'arm', 'metric', 'processes', 'median', 'min', 'max', 'zeroValues'], ...nrows]));

const zeroRows = inp.native.filter(r => r.p50Ns === 0);
writeFileSync(resolve(dir, 'report_tables.md'), [
  '## Frozen criteria (derived from the judge input, asserted equal to the judge\'s saved rows)\n', md(['criterion', 'operands', 'value', 'frozen limit', 'pass'], crit.map(r => r.map(String))),
  '## Per-cell Ir (A1=A2=C0 and B1=B2 are bit-identical)\n', md(['cell', 'Ir A', 'Ir B', 'delta'], cellRows.map(r => r.map(String))),
  `## Native axis (descriptive only; the judge stopped at the first p50Ns == 0: ${zeroRows.length} of ${inp.native.length} rows have p50Ns == 0)\n`, md(['n', 'arm', 'metric', 'processes', 'median', 'min', 'max', 'zero values'], nrows.map(r => r.map(String))),
].join('\n'));
console.log(JSON.stringify({ criteriaFail: crit.filter(r => !r[4]).map(r => r[0]), nativeZeroP50Rows: zeroRows.length, cells: names.length }, null, 2));
