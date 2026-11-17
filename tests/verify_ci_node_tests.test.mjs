import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { computeCoverage } from '../scripts/verify-ci-node-tests.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');

function fixture(t, { tests = ['one.test.mjs'], workflow = '', actions = {} } = {}) {
  const root = mkdtempSync(join(tmpdir(), 'ci-node-tests-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  mkdirSync(join(root, 'tests'), { recursive: true });
  mkdirSync(join(root, '.github', 'workflows'), { recursive: true });
  mkdirSync(join(root, '.github', 'actions'), { recursive: true });
  for (const name of tests) writeFileSync(join(root, 'tests', name), '');
  if (workflow) writeFileSync(join(root, '.github', 'workflows', 'ci.yml'), workflow);
  for (const [name, contents] of Object.entries(actions)) {
    const file = join(root, '.github', 'actions', name, 'action.yml');
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, contents);
  }
  return root;
}

test('one explicit CI reference covers a test exactly once', (t) => {
  const root = fixture(t, { workflow: 'run: node --test tests/one.test.mjs\n' });
  assert.deepEqual(computeCoverage(root).coverage, { 'one.test.mjs': 1 });
});

test('duplicate explicit CI references are rejected', (t) => {
  const root = fixture(t, { workflow: 'run: node --test tests/one.test.mjs\nrun: node --test tests/one.test.mjs\n' });
  assert.throws(() => computeCoverage(root), /tests\/one\.test\.mjs runs 2 times/);
});

test('a test with no CI reference is rejected', (t) => {
  const root = fixture(t);
  assert.throws(() => computeCoverage(root), /tests\/one\.test\.mjs runs 0 times/);
});

test('an exclusion naming a nonexistent test is rejected', (t) => {
  const root = fixture(t, {
    tests: ['one.test.mjs'],
    workflow: String.raw`run: ls *.test.mjs | grep -Ev '^tests/(missing)\.test\.mjs$'` + '\n',
  });
  assert.throws(() => computeCoverage(root), /exclusion names nonexistent test tests\/missing\.test\.mjs/);
});

test('a glob exclusion must be covered by another CI step', (t) => {
  const root = fixture(t, {
    tests: ['one.test.mjs', 'two.test.mjs'],
    workflow: String.raw`run: ls *.test.mjs | grep -Ev '^tests/(two)\.test\.mjs$'` + '\n',
  });
  assert.throws(() => computeCoverage(root), /exclusion removes tests\/two\.test\.mjs, which is not covered by another CI step/);
});

test('real repository Node tests are each covered exactly once', () => {
  const result = computeCoverage(repoRoot);
  assert.ok(result.tests.length > 0);
  assert.ok(result.tests.every((name) => result.coverage[name] === 1));
});
