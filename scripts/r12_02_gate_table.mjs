#!/usr/bin/env node
// R12-02 — iai A/B judge for the `clear_magazine_on_issue` mask change.
// MEASUREMENT-ONLY: parses the committed raw `npm run iai` logs under docs/perf/,
// prints the report tables to stdout and writes docs/perf/R12_02_MAGAZINE_MASK_GATE_summary.csv.
//
// Discipline (CLAUDE.md gate-report rules): every ratio is printed through a guarded printer that
// asserts a round-trip with its numerator and denominator; determinism is asserted (A1==A2,
// B1==B2 on every numeric column); the same-source/different-path control (C0) bounds the
// layout noise; the statistic name ("per-hit Ir", an arithmetic quotient) is printed here.
//
// Usage (repo root): node scripts/r12_02_gate_table.mjs [--check]
//   --check  fail unless docs/perf/R12_02_MAGAZINE_MASK_GATE.md embeds exactly the tables printed here
//            between the r12_02:tables markers.

import assert from 'node:assert/strict';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const P = (n) => join(REPO_ROOT, 'docs', 'perf', n);
const REPORT = P('R12_02_MAGAZINE_MASK_GATE.md');
const CSV = P('R12_02_MAGAZINE_MASK_GATE_summary.csv');
const MAGAZINE_FILL = 16; // ops in the alloc_clear_magazine_only_16b pair (benches/perf_gate_iai.rs)
const BENCHES = [
  'small_churn_16b',
  'small_churn_16b_2n',
  'alloc_magazine_hit_only_16b',
  'alloc_clear_magazine_only_16b_prefix',
  'alloc_clear_magazine_only_16b',
];

function parse(name) {
  const text = readFileSync(P(name), 'utf8').replace(/\r\n?/g, '\n');
  const rows = new Map();
  const re = /^ {2}(\S+) +([\d,]+) +([\d,]+) +([\d,]+) +([\d,]+) +([\d,]+) +(\S+)$/;
  for (const line of text.split('\n')) {
    const m = re.exec(line);
    if (!m || !BENCHES.includes(m[1])) continue;
    const n = (s) => Number(s.replace(/,/g, ''));
    rows.set(m[1], { ir: n(m[2]), l1: n(m[3]), l2: n(m[4]), ram: n(m[5]), est: n(m[6]) });
  }
  for (const b of BENCHES) assert(rows.has(b), `${name}: missing bench row ${b}`);
  return rows;
}

const A1 = parse('_raw_r12_02_A1.log');
const A2 = parse('_raw_r12_02_A2.log');
const B1 = parse('_raw_r12_02_B1.log');
const B2 = parse('_raw_r12_02_B2.log');
const C0 = parse('_raw_r12_02_C0_control_unpatched_other_path.log');

// determinism: callgrind Ir/cache columns must be identical run-to-run on the same binary
for (const [x, y, tag] of [[A1, A2, 'A1==A2'], [B1, B2, 'B1==B2']]) {
  for (const b of BENCHES) assert.deepEqual(x.get(b), y.get(b), `${tag} differs on ${b}`);
}

function ratioOf(den, num, what) {
  assert(Number.isFinite(den) && Number.isFinite(num) && den > 0, `bad operands for ${what}`);
  const r = num / den;
  assert(Math.abs(den * r - num) / Math.abs(num || 1) <= 1e-9, `round-trip mismatch for ${what}`);
  return r;
}
const pct = (den, num, what) => {
  const p = (ratioOf(den, num, what) - 1) * 100;
  return `${p >= 0 ? '+' : '-'}${Math.abs(p).toFixed(3)}%`;
};
const f = (n) => n.toLocaleString('en-US');

const out = [];
out.push('| bench | A Ir | B Ir | B−A Ir | B/A Ir (num B / den A) | A EstCycles | B EstCycles | B/A EstCycles |');
out.push('|---|---:|---:|---:|---|---:|---:|---|');
const csv = ['bench,a_ir,b_ir,delta_ir,ratio_ir,a_est_cycles,b_est_cycles,ratio_est_cycles'];
for (const b of BENCHES) {
  const a = A1.get(b);
  const c = B1.get(b);
  out.push(
    `| ${b} | ${f(a.ir)} | ${f(c.ir)} | ${c.ir - a.ir} | ${ratioOf(a.ir, c.ir, b + ' Ir').toFixed(5)} (${f(c.ir)}/${f(a.ir)}, ${pct(a.ir, c.ir, b)}) | ${f(a.est)} | ${f(c.est)} | ${ratioOf(a.est, c.est, b + ' est').toFixed(5)} (${f(c.est)}/${f(a.est)}, ${pct(a.est, c.est, b + ' est')}) |`,
  );
  csv.push(
    [b, a.ir, c.ir, c.ir - a.ir, ratioOf(a.ir, c.ir, b).toFixed(6), a.est, c.est, ratioOf(a.est, c.est, b + ' e').toFixed(6)].join(','),
  );
}

// per-hit clear cost: (Ir(clear_only) - Ir(prefix)) / MAGAZINE_FILL — the statistic is this arithmetic quotient
const perHit = (m) =>
  (m.get('alloc_clear_magazine_only_16b').ir - m.get('alloc_clear_magazine_only_16b_prefix').ir) / MAGAZINE_FILL;
const numHit = (m) => m.get('alloc_clear_magazine_only_16b').ir - m.get('alloc_clear_magazine_only_16b_prefix').ir;
const phA = perHit(A1);
const phB = perHit(B1);
assert(numHit(A1) === phA * MAGAZINE_FILL && numHit(B1) === phB * MAGAZINE_FILL, 'per-hit arithmetic');
out.push('');
out.push('| arm | Ir(clear_only) − Ir(prefix) | ÷ MAGAZINE_FILL | per-hit Ir (arithmetic quotient) |');
out.push('|---|---:|---:|---:|');
out.push(`| A (main \`c4725071\`) | ${numHit(A1)} | ${MAGAZINE_FILL} | ${phA.toFixed(4)} |`);
out.push(`| B (patched) | ${numHit(B1)} | ${MAGAZINE_FILL} | ${phB.toFixed(4)} |`);
out.push(
  `| B−A | ${numHit(B1) - numHit(A1)} | ${MAGAZINE_FILL} | ${(phB - phA).toFixed(4)} (${pct(phA, phB, 'per-hit')}; ${phB.toFixed(4)}/${phA.toFixed(4)}) |`,
);
csv.push(`per_hit_ir_clear_magazine_on_issue,${phA},${phB},${phB - phA},${ratioOf(phA, phB, 'ph').toFixed(6)},,,`);

// layout-noise control: identical source (unpatched) built at a different worktree path
let maxCtl = 0;
for (const b of BENCHES) maxCtl = Math.max(maxCtl, Math.abs(C0.get(b).ir - A1.get(b).ir));
out.push('');
out.push(
  `Control C0 (same unpatched source as A, different worktree path): max |ΔIr| over the ${BENCHES.length} benches = ${maxCtl} ` +
    `(denominator: ${BENCHES.length} bench rows). Every |B−A| Ir above that bound is outside layout noise.`,
);
csv.push(`control_max_abs_delta_ir,${maxCtl},,,,,,`);

for (const b of ['small_churn_16b', 'small_churn_16b_2n', 'alloc_magazine_hit_only_16b']) {
  assert(Math.abs(B1.get(b).ir - A1.get(b).ir) > maxCtl, `${b}: delta within layout noise`);
}

const table = out.join('\n');
const BEGIN = '<!-- r12_02:tables:begin -->';
const END = '<!-- r12_02:tables:end -->';

if (process.argv.includes('--check')) {
  const rep = readFileSync(REPORT, 'utf8').replace(/\r\n?/g, '\n');
  const i = rep.indexOf(BEGIN);
  const j = rep.indexOf(END);
  assert(i >= 0 && j > i, 'report lacks r12_02:tables markers');
  assert.equal(rep.slice(i + BEGIN.length, j).trim(), table.trim(), 'report tables differ from the generated ones');
  console.log('r12_02_gate_table --check: OK');
} else {
  writeFileSync(CSV, csv.join('\n') + '\n');
  console.log(table);
}
