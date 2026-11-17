import assert from 'node:assert/strict';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { effectiveWorkflow } from '../scripts/ci/effective-workflow.mjs';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const ownersWithExistingCrateRows = [
  'aligned-vmem', 'numa-shim', 'sefer-region', 'once-ptr-cell',
  'size-classes', 'globalalloc-model', 'tagged-index-stack', 'proc-memstat',
];

test('test-workspace retains all-targets clippy coverage for benchmark/example owners', () => {
  const { text, origins } = effectiveWorkflow(repoRoot);
  const lines = text.split('\n');
  const mallocCommand = 'cargo clippy -p malloc-bench-rs --all-targets -- -D warnings';
  const mallocIndex = lines.findIndex((line) => line.trim() === `- run: ${mallocCommand}`);
  assert.notEqual(mallocIndex, -1, 'malloc-bench-rs all-targets clippy row must remain in effective CI');
  assert.equal(origins[mallocIndex].job, 'test-workspace', 'malloc-bench-rs row must be wired through test-workspace');

  for (const owner of ownersWithExistingCrateRows) {
    assert.ok(lines.some((line) => /\bcargo clippy\b/.test(line)
      && new RegExp(`-p ${owner}(?:\\s|$)`).test(line)
      && /--all-targets\b/.test(line)), `${owner} must retain its crate-scoped all-targets clippy row`);
  }
});
