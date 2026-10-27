// Mutation check of the Invariant-Check trailer linter: each mutant removes one
// check from scripts/verify-invariant-trailers.mjs's `checkCommit`, and the
// linter's self-test must reject every one of them (plan §5.2: "the linter is
// itself closed by mutants").
//   node scripts/verify-invariant-trailers-mutants.mjs
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { REPO_ROOT } from './lib.mjs';

const MUTANTS = [
  ['no-missing-trailer', "    if (!b) { errors.push(`missing Invariant-Check", "    if (!b) { continue; errors.push(`missing Invariant-Check"],
  ['no-unknown-rule', '    if (!c.rules.has(b.id)) errors.push(', '    if (false) errors.push('],
  ['no-empty-change', "    if (!b.change) errors.push(", '    if (false) errors.push('],
  ['no-why-length', '    if (w.length < WHY_MIN) errors.push(', '    if (false) errors.push('],
  ['no-why-copy', "    if (whys.filter((x) => x === w.toLowerCase()).length > 1) errors.push(", '    if (false) errors.push('],
  ['no-why-terms', '    if (terms.length && hit.length < need) errors.push(', '    if (false) errors.push('],
  ['no-ghost-id-check', "        if (!c.ghostIds.has(item.slice(6))) errors.push(", '        if (false) errors.push('],
  ['no-exists-check', "      if (!c.exists(path)) { errors.push(", '      if (false) { errors.push('],
  ['no-test-required', '    if (b.covered.length && tests === 0) errors.push(', '    if (false) errors.push('],
  ['no-params-evidence', '      if (!evidenceChanged) errors.push(', '      if (false) errors.push('],
];

const dir = mkdtempSync(join(tmpdir(), 'inv-trailer-mut-'));
try {
  copyFileSync(join(REPO_ROOT, 'scripts/lib.mjs'), join(dir, 'lib.mjs'));
  const orig = readFileSync(join(REPO_ROOT, 'scripts/verify-invariant-trailers.mjs'), 'utf8');
  let rejected = 0;
  for (const [i, [name, from, to]] of [['baseline', '', ''], ...MUTANTS].entries()) {
    assert.ok(name === 'baseline' || orig.includes(from), `mutant ${name}: anchor not found (linter changed — update the mutant)`);
    const file = join(dir, `m${i}.mjs`);
    writeFileSync(file, name === 'baseline' ? orig : orig.replace(from, to));
    const mod = await import(pathToFileURL(file).href);
    const failed = mod.SELF_TEST.filter(([, mk, want]) => {
      const { errors } = mod.checkCommit(mk());
      return want === null ? errors.length !== 0 : !errors.some((e) => want.test(e));
    }).map(([n]) => n);
    if (name === 'baseline') { assert.deepEqual(failed, [], 'baseline self-test must pass'); continue; }
    console.log(`${failed.length ? 'REJECTED' : 'ACCEPTED'} ${name}${failed.length ? ` — by case(s): ${failed.join(', ')}` : ''}`);
    if (failed.length) rejected++;
  }
  console.log(`[verify-invariant-trailers-mutants] ${rejected}/${MUTANTS.length} linter mutants rejected by the self-test`);
  assert.equal(rejected, MUTANTS.length, 'every linter mutant must be rejected');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
