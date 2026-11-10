// Cargo-free fixtures for the restricted, read-only composite expansion.
import assert from 'node:assert/strict';
import { effectiveWorkflow } from '../scripts/ci/effective-workflow.mjs';
const action = '.github/actions/ci-fixture/action.yml';
const workflow = `name: fixture
jobs:
  check:
    runs-on: ubuntu-latest
    env:
      MIRIFLAGS: strict
    steps:
      - uses: actions/checkout@v5
      - uses: ./.github/actions/ci-fixture
`;
const composite = `name: fixture
description: fixture
runs:
  using: composite
  steps:
    - name: verify
      # ci-default-shell: preserve original implicit runner shell
      shell: bash -e {0}
      run: cargo miri test --test fixture
`;
const files = { '.github/workflows/ci.yml': workflow, [action]: composite };
let count = 0;
function run(over = {}) {
  const all = { ...files, ...over };
  return effectiveWorkflow('.', { read: file => {
    assert(Object.hasOwn(all, file), `missing ${file}`);
    return all[file];
  } });
}
function check(fn) { fn(); count++; }
check(() => {
  const result = run();
  assert(result.text.includes('      - name: verify\n        run: cargo miri test --test fixture'));
  assert(!result.text.includes('shell:'));
  assert.deepEqual([...result.dependencies.get(action)], ['check']);
  const at = result.text.split('\n').findIndex(l => l.includes('run: cargo'));
  assert.equal(result.origins[at].file, action);
  assert.equal(result.origins[at].line, 9);
  assert(result.text.includes('MIRIFLAGS: strict'));
});
check(() => assert.equal(run({ '.github/actions/ci-unused/action.yml': 'bad' }).dependencies.size, 1));
check(() => assert.throws(() => run({ [action]: undefined }), /replace/));
check(() => assert.throws(() => run({ [action]: composite.replace('using: composite', 'using: node20') }), /header/));
check(() => assert.throws(() => run({ [action]: composite.replace('shell: bash -e {0}', 'shell: bash') }), /incorrect default shell/));
check(() => assert.throws(() => run({ [action]: composite + 'inputs:\n  bad: value\n' }), /structure/));
check(() => assert.throws(() => run({ [action]: composite.replace('run: cargo miri test --test fixture', 'uses: ./.github/actions/ci-fixture') }), /nested local/));
check(() => assert.throws(() => run({ [action]: composite.replace('--test fixture', '--test ${{ matrix.target }}') }), /context/));
for (const value of ['../escape', './.github/actions/ci-fixture/../../escape', "'./.github/actions/ci-fixture'"]) {
  check(() => assert.throws(() => run({ '.github/workflows/ci.yml': workflow.replace('./.github/actions/ci-fixture', value) }), /unsupported local|missing/));
}
check(() => assert.throws(() => run({ '.github/workflows/ci.yml': workflow.replace('      - uses: actions/checkout@v5\n', '') }), /before checkout/));
check(() => assert.throws(() => run({ '.github/workflows/ci.yml': workflow.replace('./.github/actions/ci-fixture\n', './.github/actions/ci-fixture\n        if: false\n') }), /attributes/));
check(() => assert(!run({ '.github/workflows/ci.yml': workflow.replace('      - uses: ./.github/actions/ci-fixture\n', '') }).text.includes('--test fixture')));
check(() => assert.notEqual(run().text, run({ [action]: composite.replace('--test fixture', '--test wrong') }).text));
console.log(`[ci-effective-workflow] ${count} positive/negative static fixtures passed`);
