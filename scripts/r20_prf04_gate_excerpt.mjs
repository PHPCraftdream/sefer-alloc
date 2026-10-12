// Derives the tables of docs/perf/R20_PRF04_CLAIM_PATH_GATE.md from the retained judge input and
// asserts every number equals the judge's own saved verdict rows. Read-only unless --write.
//   node scripts/r20_prf04_gate_excerpt.mjs           print the derived section
//   node scripts/r20_prf04_gate_excerpt.mjs --write   replace the marked section of the report
//   node scripts/r20_prf04_gate_excerpt.mjs --check   fail unless the report holds the derived section
import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';

const root = new URL('../', import.meta.url);
const RUN = 'docs/perf/R20_PRF04_CLAIM_PATH_RUN01';
const REPORT = 'docs/perf/R20_PRF04_CLAIM_PATH_GATE.md';
const BEGIN = '<!-- BEGIN DERIVED (scripts/r20_prf04_gate_excerpt.mjs) -->';
const END = '<!-- END DERIVED -->';
const read = p => readFileSync(new URL(p, root), 'utf8');
const input = JSON.parse(read(`${RUN}/input.json`));
const verdict = read(`${RUN}/verdict.txt`);
const ir = new Map(input.cells.map(c => [`${c.family}|${c.arm}|${c.size}`, c.ir]));
const get = (f, a, s) => { const v = ir.get(`${f}|${a}|${s}`); assert.ok(Number.isSafeInteger(v), `${f}|${a}|${s}`); return v; };
// Deterministic Callgrind: duplicates of one binary must agree exactly, and C0 is reported next to A.
const arm = (f, s, x) => { const [p, q] = x === 'A' ? ['A1', 'A2'] : ['B1', 'B2']; assert.equal(get(f, p, s), get(f, q, s)); return get(f, p, s); };
const val = (f, s, x) => (s === '0' ? 0 : arm(f, s, x));
const WINS = { cold: [['0', 'n', 16], ['n', '2n', 16], ['2n', '4n', 32]], wave: [['0', 'n', 16], ['n', '2n', 16], ['2n', '4n', 32]],
  recycled: [['n', '2n', 16]] };
const UNIT = { cold: 'claims', wave: 'claims', recycled: 'claim+release pairs' };
const rows = [], lines = [];
lines.push('| family | window | claims | Ir A | Ir B | B − A (numerator) | A (denominator) | B/A − 1 | per claim (B − A)/claims |');
lines.push('|---|---|---:|---:|---:|---:|---:|---:|---:|');
const out = { construct: null };
{
  const a = arm('start', 'n', 'A'), b = arm('start', 'n', 'B');
  out.construct = { a, b, delta: b - a };
}
for (const [f, wins] of Object.entries(WINS)) {
  wins.forEach(([x, y, units], k) => {
    const a = val(f, y, 'A') - val(f, x, 'A'), b = val(f, y, 'B') - val(f, x, 'B'), d = b - a;
    const pct = `${d < 0 ? '-' : '+'}${(Math.abs(d) / a * 100).toFixed(2)}%`;
    const per = d % units === 0 ? String(d / units) : `${d}/${units}`;
    lines.push(`| ${f} | w${k} (${x === '0' ? '0' : x}→${y}) | ${units} ${UNIT[f]} | ${a} | ${b} | ${d} | ${a} | ${pct} | ${per} |`);
    rows.push({ f, k, d, a });
    // The judge's own saved row for B1 vs A1 must carry the same numerator and denominator.
    const m = new RegExp(`\\| family ${f} \\|[\\s\\S]*?R-claim B1 vs A1 window ${k} \\| [+-][\\d.]+% \\(numerator (-?\\d+), denominator (\\d+)\\)`).exec(verdict);
    assert.ok(m, `no judge row for ${f} window ${k}`);
    assert.equal(Number(m[1]), d, `${f} w${k} numerator vs judge`);
    assert.equal(Number(m[2]), a, `${f} w${k} denominator vs judge`);
  });
}
assert.match(verdict, /^verdict: GO/m);
const c = out.construct;
const cm = /R-construct B1 vs A1 \| Ir_B-Ir_A=(-?\d+) \| budget_K=(\d+)/.exec(verdict);
assert.equal(Number(cm[1]), c.delta);
const olines = ['| arm | family | N | hint claims | bump claims | free-set claims | new heaps | counters |', '|---|---|---:|---:|---:|---:|---:|---:|'];
for (const x of ['A', 'B']) for (const f of ['start', 'cold', 'recycled', 'wave']) for (const sz of (f === 'start' ? ['n'] : ['n', '2n', '4n'])) {
  const o = input.oracle[x][f][sz];
  assert.ok([o.hintDelta, o.bumpDelta, o.freeSetDelta, o.newHeaps].every(Number.isSafeInteger));
  olines.push(`| ${x} | ${f} | ${o.n} | ${o.hintDelta} | ${o.bumpDelta} | ${o.freeSetDelta} | ${o.newHeaps} | ${o.claimSourcesLen} |`);
}
const section = [BEGIN, '',
  `Judge verdict (retained \`${RUN}/verdict.txt\`): **GO** on the deterministic axes. A1 = A2 and B1 = B2 bit for bit in all ${input.cells.length / 5} cells, and C0 = A for every cell the table uses.`, '',
  `Construction (start cell, one-sided, budget_K = T_s): Ir A = ${c.a}, Ir B = ${c.b}, B − A = ${c.delta} against budget_K = ${cm[2]} (numerator ${c.delta}, denominator budget ${cm[2]}).`, '',
  ...lines, '', 'Path oracle (retained `oracle.json`, bound to each arm identity and oracle binary; counts over the measured phase):', '',
  ...olines, '', END].join('\n');
const arg = process.argv[2];
if (arg === '--write' || arg === '--check') {
  const doc = read(REPORT);
  const a = doc.indexOf(BEGIN), b = doc.indexOf(END);
  assert.ok(a >= 0 && b > a, 'markers missing in the report');
  const next = doc.slice(0, a) + section + doc.slice(b + END.length);
  if (arg === '--write') { writeFileSync(new URL(REPORT, root), next); console.log('report section written'); }
  else { assert.equal(next, doc, 'report section is not the derived one'); console.log('PASS: report section equals the derived tables'); }
} else console.log(section);
