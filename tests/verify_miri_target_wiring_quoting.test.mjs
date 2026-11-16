// node --test suite for the YAML single-quote handling in
// scripts/verify-miri-target-wiring.mjs (P.02 merge pilot).
//
// Zero deps, no cargo, no git, nothing written inside the repo.

import { test } from 'node:test';
import assert from 'node:assert/strict';

import {
  unquoteRunScalar, extractMiriTestCommand,
} from '../scripts/verify-miri-target-wiring.mjs';
import { MIRI_TARGETS } from '../scripts/miri-matrix.mjs';

const QUOTED = `'cargo miri test --locked --features "alloc-core internals" --test regression_merged -- regression_r2_05_diag_provenance_miri::'`;
const UNQUOTED = QUOTED.slice(1, -1);

function moduleFilterArgs(scriptText) {
  const argv = extractMiriTestCommand(scriptText);
  assert.ok(argv, 'command must extract');
  const at = argv.indexOf('--');
  assert.notEqual(at, -1, 'command must carry a -- separator');
  return argv.slice(at);
}

test('quoted run scalar: trailing quote is stripped from the module filter', () => {
  const script = unquoteRunScalar(QUOTED);
  const args = moduleFilterArgs(script);
  assert.equal(args.at(-1), 'regression_r2_05_diag_provenance_miri::');
  assert.ok(!args.at(-1).endsWith("'"), 'no trailing quote may leak');
});

test('unquoted run scalar passes through byte-identically (helper is a no-op)', () => {
  assert.equal(unquoteRunScalar(UNQUOTED), UNQUOTED);
  const args = moduleFilterArgs(UNQUOTED);
  assert.deepEqual(args, ['--', 'regression_r2_05_diag_provenance_miri::']);
});

test("escaped internal quotes '' are restored to a single quote", () => {
  const script = unquoteRunScalar(`'echo ''done'' -- x::'`);
  assert.equal(script, `echo 'done' -- x::`);
});

test('block-scalar starter passes through unchanged', () => {
  const block = '| cargo miri test --locked --test regression_merged';
  assert.equal(unquoteRunScalar(block), block);
  assert.equal(unquoteRunScalar('|'), '|');
});

test('parsed args deepEqual the manifest row args (manifest identity shape)', () => {
  const row = MIRI_TARGETS.find((r) => Array.isArray(r.args)
    && r.args[0] === '--' && r.args[1] === 'regression_r2_05_diag_provenance_miri::');
  assert.ok(row, 'manifest row must exist');
  assert.deepEqual(moduleFilterArgs(unquoteRunScalar(QUOTED)), row.args);
  assert.deepEqual(row.args, ['--', 'regression_r2_05_diag_provenance_miri::']);
});
