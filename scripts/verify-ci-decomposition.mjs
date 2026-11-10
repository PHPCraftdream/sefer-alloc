// Structural receipt for the CI extraction; no Cargo or workflow execution.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { effectiveWorkflow } from './ci/effective-workflow.mjs';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const baseline = resolve(root, 'docs/verification/ci-decomposition-baseline.json');
const canonical = s => s.split('\n').filter(l => l.trim() && !l.trimStart().startsWith('#')).join('\n');
const digest = s => createHash('sha256').update(canonical(s)).digest('hex');
const gate = '      - run: node scripts/verify-ci-decomposition.mjs\n';
const expanded = effectiveWorkflow(root);
const text = expanded.text.replace(gate, '').replace(String.raw`|\.github/actions/ci-j13b-judge-preparation/action\.yml`, '');
const at = text.indexOf('\njobs:\n');
assert(at >= 0, 'workflow must contain jobs');
const jobs = [...text.matchAll(/^  ([\w-]+):\s*$/gm)].filter(m => m.index > at);
const receipt = {
  schema: 'ci.decomposition.v1',
  orchestration: digest(text.slice(0, at)),
  jobs: jobs.map((m, i) => {
    const block = text.slice(m.index, jobs[i + 1]?.index ?? text.length);
    const steps = block.indexOf('\n    steps:\n');
    assert(steps > 0, `${m[1]}: steps missing`);
    return { id: m[1], metadata: digest(block.slice(0, steps)), steps: digest(block.slice(steps)) };
  }),
};
if (process.argv.includes('--record')) {
  assert.equal(expanded.dependencies.size, 0, 'capture baseline before extraction, never bless expanded changes');
  writeFileSync(baseline, `${JSON.stringify(receipt, null, 2)}\n`);
  console.log(`[ci-decomposition] captured ${receipt.jobs.length} original jobs before extraction`);
} else {
  assert.equal(expanded.text.split(gate).length, 2, 'integrity gate must remain reachable exactly once');
  assert.equal(expanded.text.split(String.raw`|\.github/actions/ci-j13b-judge-preparation/action\.yml`).length, 2,
    'J13b relocated source must remain selected exactly once');
  assert.deepEqual(receipt, JSON.parse(readFileSync(baseline, 'utf8')), 'effective CI differs from pre-extraction baseline');
  for (const [file, source] of expanded.sources) {
    assert(source.split('\n').length <= 1001, `${file}: exceeds 1000 lines`);
  }
  await import('../tests/ci_effective_workflow_fixtures.mjs');
  await import('../tests/ci_decomposition_contracts.mjs');
  console.log(`[ci-decomposition] ${receipt.jobs.length} jobs unchanged; ${expanded.dependencies.size} reachable local actions; orchestration/commands identical`);
}
