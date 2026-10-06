#!/usr/bin/env node
// R12-05 — deterministic counter gate for the bounded Large-ingress probe on `alloc_batch` misses.
// MEASUREMENT-ONLY: counts Large-route inspections via the bench-internals oracle; no wall-clock.
//
// Discipline (CLAUDE.md gate-report rules): tables and CSV are DERIVED from the raw logs here;
// every ratio names numerator/denominator and is asserted; the statistic is the plain integer
// counter `dbg_large_sidecar_slot_inspections` delta (no mean/median is computed).
//
// Usage (repo root):
//   node scripts/r12_05_batch_gate.mjs          run the patched test, write the raw log + CSV, print tables
//   node scripts/r12_05_batch_gate.mjs --check  parse the committed raw logs; fail unless the CSV and
//                                               the report's tables match what is derived here
// Env: CARGO_TARGET_DIR / RUSTC_WRAPPER are passed through unchanged.

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const P = (n) => join(REPO_ROOT, 'docs', 'perf', n);
const REPORT = P('R12_05_BATCH_LARGE_BOUNDED_GATE.md');
const CSV = P('R12_05_BATCH_LARGE_BOUNDED_GATE_summary.csv');
const RAW_PATCHED = P('_raw_r12_05_batch_counter.log');
const RAW_MUTANT = P('_raw_r12_05_mutant_full_drain.log');
const FEATURES = 'production,batch-api,internals,bench-internals';
const BEGIN = '<!-- r12_05:tables:begin -->';
const END = '<!-- r12_05:tables:end -->';

const norm = (s) => s.replace(/\r\n?/g, '\n');

if (!process.argv.includes('--check')) {
  const args = ['test', '--features', FEATURES, '--test', 'r12_05_batch_large_bounded', '--', '--nocapture', '--test-threads=1'];
  const run = spawnSync('cargo', args, { cwd: REPO_ROOT, encoding: 'utf8', env: process.env });
  const text = norm(run.stdout ?? ''); // stdout only: cargo's build chatter (stderr) carries local paths
  const body = text.slice(Math.max(0, text.indexOf('running ')));
  writeFileSync(
    RAW_PATCHED,
    `# cmd: cargo test --features ${FEATURES} --test r12_05_batch_large_bounded -- --nocapture --test-threads=1\n# exit=${run.status}\n${body}`,
  );
  assert.equal(run.status, 0, 'patched test run failed; see raw log');
}

function parsePatched() {
  const t = norm(readFileSync(RAW_PATCHED, 'utf8'));
  const num = (s) => Number(s);
  const sm = /R12-05 small L=(\d+) K=(\d+) B=(\d+) inspections=(\d+) rescues=(\d+)/.exec(t);
  const lg = /R12-05 large L=(\d+) K=(\d+) B=(\d+) inspections=(\d+) rescues=(\d+)/.exec(t);
  const late = /R12-05 late publication retired after (\d+) misses \(table=(\d+) B=(\d+)\)/.exec(t);
  assert(sm && lg && late, 'raw patched log lacks an expected R12-05 line');
  assert(/test result: ok\. 3 passed; 0 failed/.test(t), 'raw patched log: not 3 passed');
  const row = (m) => ({ L: num(m[1]), K: num(m[2]), B: num(m[3]), insp: num(m[4]), resc: num(m[5]) });
  return { small: row(sm), large: row(lg), late: { misses: num(late[1]), table: num(late[2]), B: num(late[3]) } };
}

function parseMutant() {
  const t = norm(readFileSync(RAW_MUTANT, 'utf8'));
  const blocks = t.split('## mutant=').slice(1);
  assert.equal(blocks.length, 3, 'mutant raw log must hold 3 runs');
  const out = {};
  for (const b of blocks) {
    const [head] = b.split('\n');
    const [site, testPart] = head.split(' test=');
    const m = /inspected (\d+)(?:; budget=(\d+))?/.exec(b);
    assert(m, `mutant run ${testPart}: no "inspected N" panic message`);
    assert(/0xc0000409|panicked at|test failed/.test(b), `mutant run ${testPart}: not red`);
    out[testPart] = { site, insp: Number(m[1]), budget: m[2] === undefined ? null : Number(m[2]) };
  }
  return out;
}

const ratio = (num, den, what) => {
  assert(Number.isInteger(num) && Number.isInteger(den) && den > 0, `bad operands for ${what}`);
  const r = num / den;
  assert(Math.abs(r * den - num) < 1e-9, `round-trip mismatch for ${what}`);
  return r;
};

const a = parsePatched();
const m = parseMutant();
const mSmall = m.small_batch_miss_probes_at_most_the_hot_budget;
const mLarge = m.large_batch_probes_at_most_the_hot_budget;
const mLate = m.late_large_publication_is_retired_by_rotating_small_batch_probes;

for (const r of [a.small, a.large]) {
  assert(r.insp > 0 && r.insp <= r.B && r.resc === 0, 'patched row violates budget/rescue oracle');
}
assert.equal(a.small.B, a.large.B);
assert.equal(a.late.B, a.small.B);
assert(a.late.misses >= 1 && a.late.misses <= Math.ceil(a.late.table / a.late.B) + 1, 'late publication not retired in bound');
assert.equal(mSmall.insp, a.small.L, 'small full-drain mutant must inspect every active Large route');
assert.equal(mLarge.insp, a.large.L, 'large full-drain mutant must inspect every active Large route');
assert(mSmall.insp > a.small.B && mLarge.insp > a.large.B && mLate.insp > a.late.B, 'mutants must exceed the budget');

const out = [];
out.push('| scenario | active Large routes L | batch misses K | budget B | inspections (patched) | inspections (full-drain mutant) | mutant/budget (num mutant / den B) | rescues |');
out.push('|---|---:|---:|---:|---:|---:|---|---:|');
const csv = ['scenario,L,K,budget,patched_inspections,mutant_inspections,mutant_over_budget,rescues,table_slots,retire_bound'];
for (const [name, r, mu] of [['small_batch_miss', a.small, mSmall], ['large_batch', a.large, mLarge]]) {
  const x = ratio(mu.insp, r.B, name);
  out.push(`| ${name} | ${r.L} | ${r.K} | ${r.B} | ${r.insp} | ${mu.insp} | ${x.toFixed(2)} (${mu.insp}/${r.B}) | ${r.resc} |`);
  csv.push([name, r.L, r.K, r.B, r.insp, mu.insp, x.toFixed(4), r.resc, '', ''].join(','));
}
out.push('');
out.push('| late-publication scenario | table slots | budget B | misses to retire | bound ⌈table/B⌉+1 | first-miss inspections (full-drain mutant) |');
out.push('|---|---:|---:|---:|---:|---:|');
const bound = Math.ceil(a.late.table / a.late.B) + 1;
out.push(`| small batch misses, one late Large publication | ${a.late.table} | ${a.late.B} | ${a.late.misses} | ${bound} | ${mLate.insp} |`);
csv.push(['late_publication_small_batch', '', a.late.misses, a.late.B, '', mLate.insp, '', 0, a.late.table, bound].join(','));
const table = out.join('\n');

if (process.argv.includes('--check')) {
  const rep = norm(readFileSync(REPORT, 'utf8'));
  const i = rep.indexOf(BEGIN);
  const j = rep.indexOf(END);
  assert(i >= 0 && j > i, 'report lacks r12_05:tables markers');
  assert.equal(rep.slice(i + BEGIN.length, j).trim(), table.trim(), 'report tables differ from the derived ones');
  assert.equal(norm(readFileSync(CSV, 'utf8')), csv.join('\n') + '\n', 'summary CSV differs from the derived one');
  console.log('r12_05_batch_gate --check: OK');
} else {
  writeFileSync(CSV, csv.join('\n') + '\n');
  console.log(table);
}
