// Cargo-free selection/license counterfactuals for the actual extraction boundary.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { effectiveWorkflow } from '../scripts/ci/effective-workflow.mjs';
import { classifyPath, parseJobs } from '../scripts/ci-plan.mjs';
import { problems } from '../scripts/license-check-workspaces.mjs';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const expanded = effectiveWorkflow(root);
const jobs = parseJobs(expanded.text);
for (const [file, owners] of expanded.dependencies) {
  const expected = new Set(jobs.filter(job => owners.has(job.id))
    .map(job => ['weekly', 'dispatch'].includes(job.group) ? 'fast' : job.group));
  assert.deepEqual(classifyPath(file), expected, `${file}: select all owning job groups`);
}
assert.deepEqual(classifyPath('.github/actions/ci-unknown/action.yml'), new Set(['fast']));
const workflow = `jobs:
  check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v5
      - uses: ./.github/actions/ci-fixture
`;
const action = `name: fixture
description: fixture
runs:
  using: composite
  steps:
    - uses: owner/nested@v1
`;
const files = {
  '.github/workflows/ci.yml': workflow,
  '.github/actions/ci-fixture/action.yml': action,
  '.github/actions/ci-unused/action.yml': action.replace('owner/nested', 'evil/unreachable'),
};
const deny = '[licenses]\nallow = ["MIT"]\n';
const actions = { 'actions/checkout': 'MIT', 'owner/nested': 'MIT' };
assert.deepEqual(problems({ files, deny, actions }), [], 'reachable nested third-party action licensed; unused file ignored');
assert(problems({ files, deny, actions: { 'actions/checkout': 'MIT' } })
  .some(error => error.includes('action not in ACTIONS: owner/nested')));
assert(problems({ files, deny, actions: { ...actions, 'owner/nested': 'GPL-3.0' } })
  .some(error => error.includes('license not allowed')));
assert.throws(() => problems({ files: { ...files, '.github/actions/ci-fixture/action.yml': undefined }, deny, actions }));
const detached = { ...files, '.github/workflows/ci.yml': workflow.replace('      - uses: ./.github/actions/ci-fixture\n', '') };
assert(problems({ files: detached, deny, actions }).some(error => error.includes('stale ACTIONS entry')));
const raw = readFileSync(resolve(root, '.github/actions/ci-j13b-judge-preparation/action.yml'), 'utf8');
assert(raw.includes(String.raw`|\.github/actions/ci-j13b-judge-preparation/action\.yml`), 'J13b guard includes its own action source');
console.log(`[ci-decomposition-contracts] ${expanded.dependencies.size} action routes plus license reachability/missing/unknown/forbidden/detached counterfactuals passed`);
