// Real-code mutant double confirmation (docs/GHOST_CHECKS_PLAN.md §4.3). Every
// catalog rule may list `mutants`: an exact text replacement in a production
// file plus a runtime checker (a loom/miri/kani command). For each mutant this
// script applies the replacement in place, then requires BOTH
//   1. `ghost-lint check` to report a violation of that rule, and
//   2. the checker to fail with output matching `expect`,
// and restores the file byte-for-byte afterwards (also on error). A mutant that
// only one layer catches is a defect of the rule or of the model.
//
//   node scripts/ghost_mutants.mjs [--rule <id>] [--lint-only] [--out FILE.json]
// Needs a built ghost-lint (GHOST_LINT or target/debug/ghost-lint) and the
// toolchains the checkers name (miri: nightly; loom: RUSTFLAGS="--cfg loom").
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { REPO_ROOT } from './lib.mjs';

const argv = process.argv.slice(2);
const opt = (k) => { const i = argv.indexOf(k); return i >= 0 ? argv[i + 1] : null; };
const onlyRule = opt('--rule');
const lintOnly = argv.includes('--lint-only');
const out = opt('--out');

function lintBinary() {
  if (process.env.GHOST_LINT) return process.env.GHOST_LINT;
  for (const p of ['target/debug/ghost-lint', 'target/debug/ghost-lint.exe']) if (existsSync(join(REPO_ROOT, p))) return join(REPO_ROOT, p);
  throw new Error('ghost-lint not built: cargo build -p ghost-lint (or set GHOST_LINT)');
}

const catalog = JSON.parse(readFileSync(join(REPO_ROOT, 'docs/verification/ghost-rules.json'), 'utf8'));
const work = catalog.rules.flatMap((r) => (r.mutants || []).map((m) => ({ rule: r.id, ...m }))).filter((m) => !onlyRule || m.rule === onlyRule);
assert.ok(work.length > 0, 'no mutants selected');
const lint = lintBinary();
const results = [];
let red = 0;
for (const m of work) {
  const path = join(REPO_ROOT, m.file);
  const orig = readFileSync(path, 'utf8');
  assert.ok(orig.includes(m.from), `${m.rule}/${m.id}: anchor not found in ${m.file} (production changed — update the mutant)`);
  assert.equal(orig.split(m.from).length, 2, `${m.rule}/${m.id}: anchor is not unique in ${m.file}`);
  const row = { rule: m.rule, mutant: m.id, lint: null, checker: null };
  try {
    writeFileSync(path, orig.replace(m.from, m.to));
    const l = spawnSync(lint, ['check', '--root', REPO_ROOT], { encoding: 'utf8' });
    row.lint = l.status !== 0 && l.stdout.includes(`[${m.rule}]`) ? 'RED' : 'GREEN';
    if (!lintOnly) {
      const [cmd, ...args] = m.checker.cmd;
      const c = spawnSync(cmd, args, { cwd: REPO_ROOT, encoding: 'utf8', maxBuffer: 1 << 28, env: { ...process.env, ...(m.checker.env || {}) } });
      const text = `${c.stdout}\n${c.stderr}`;
      row.checker = c.status !== 0 && new RegExp(m.checker.expect).test(text) ? 'RED' : 'GREEN';
      if (row.checker !== 'RED') row.checker_tail = text.split('\n').slice(-12).join('\n');
    }
  } finally {
    writeFileSync(path, orig);
  }
  assert.equal(readFileSync(path, 'utf8'), orig, `${m.file} not restored`);
  const ok = row.lint === 'RED' && (lintOnly || row.checker === 'RED');
  if (ok) red++;
  console.log(`${ok ? 'CAUGHT ' : 'MISSED '} ${m.rule}/${m.id}: lint ${row.lint}${lintOnly ? '' : `, ${m.checker.name} ${row.checker}`}`);
  if (row.checker_tail) console.log(row.checker_tail);
  results.push(row);
}
console.log(`[ghost-mutants] ${red}/${work.length} real-code mutants caught by ${lintOnly ? 'the linter' : 'both the linter and their checker'}`);
if (out) writeFileSync(out, JSON.stringify(results, null, 2) + '\n');
if (red !== work.length) process.exitCode = 1;
