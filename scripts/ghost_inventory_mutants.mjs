// Mutation check of the G0 inventory scanner: each mutant removes one step of
// scripts/ghost_inventory.mjs, and the self-test (scripts/ghost_inventory_selftest.mjs) must reject
// every one of them. A mutant the self-test accepts means that step has no fixture — red here.
//   node scripts/ghost_inventory_mutants.mjs
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync, copyFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { REPO_ROOT } from './lib.mjs';

const MUTANTS = [
  ['no-blank', 'export function blank(src) {', 'export function blank(src) { return src;'],
  ['no-bracket-depth-in-header', "      if (ch === '(' || ch === '[') depth++;", "      if (ch === '(') depth++;"],
  ['no-wrappers', '    if (!isOp && !isFence && ords.length === 0) continue;', '    if (!isOp && !isFence) continue;'],
  ['no-ordering-param', '    const orderingParam = isOp && ', '    const orderingParam = false && '],
  ['no-top-level-args', "    for (const ch of args) { if (ch === '(' || ch === '[' || ch === '{') depth++; else if (ch === ')' || ch === ']' || ch === '}') depth--; else if (depth === 0) top += ch; }", '    top = args;'],
  ['no-test-fn-exclusion', '(inTestFn ? s.id === catalog.test_fn_scope : true) && claims(s, site)', 'claims(s, site)'],
  ['no-double-claims', '    else if (hit.length > 1) doubles.push', '    else if (false) doubles.push'],
  ['no-cfg-capture', '    const attr = ', '    const attr = null && '],
  ['no-char-literals', "      const m = /^'(", "      const m = null && /^'("],
  ['no-keyword-filter', "    if (['if', 'while'", "    if (['__'"],
  ['no-dead-scopes', '  const dead = [...perScope].filter(', '  const dead = [].filter('],
];

const dir = mkdtempSync(join(tmpdir(), 'ghost-inv-mut-'));
try {
  for (const f of ['ghost_inventory.mjs', 'ghost_inventory_selftest.mjs', 'ghost_inventory_report.mjs', 'lib.mjs']) copyFileSync(join(REPO_ROOT, 'scripts', f), join(dir, f));
  const orig = readFileSync(join(REPO_ROOT, 'scripts/ghost_inventory.mjs'), 'utf8').replace('\nif (import.meta.url', '\nif (false && import.meta.url');
  const { selftest } = await import(pathToFileURL(join(dir, 'ghost_inventory_selftest.mjs')).href);
  let rejected = 0;
  for (const [i, [name, from, to]] of [['baseline', '', ''], ...MUTANTS].entries()) {
    assert.ok(name === 'baseline' || orig.includes(from), `mutant ${name}: anchor not found (scanner changed — update the mutant)`);
    const file = join(dir, `inv_${i}.mjs`);
    writeFileSync(file, name === 'baseline' ? orig : orig.replace(from, to));
    const mod = await import(pathToFileURL(file).href);
    let red = null;
    try { selftest(mod); } catch (e) { red = e.message.split('\n')[0]; }
    if (name === 'baseline') { assert.equal(red, null, `baseline must pass: ${red}`); continue; }
    console.log(`${red ? 'REJECTED' : 'ACCEPTED'} ${name}${red ? ' — ' + red.slice(0, 90) : ''}`);
    if (red) rejected++;
  }
  console.log(`[ghost-inventory-mutants] ${rejected}/${MUTANTS.length} scanner mutants rejected by the self-test`);
  assert.equal(rejected, MUTANTS.length, 'every scanner mutant must be rejected');
} finally {
  rmSync(dir, { recursive: true, force: true });
}
