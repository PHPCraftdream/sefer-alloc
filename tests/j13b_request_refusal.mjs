// CI-only executable test. This program never builds; do not run locally without authorization.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const [path, ...extra] = process.argv.slice(2);
assert(path && extra.length === 0, 'usage: node tests/j13b_request_refusal.mjs <built-child>');
const candidate = {
  run_id: 'ci-contract', artifact_id: 'candidate', variant: 'candidate',
  source_commit: 'a'.repeat(40), source_sha256: 'b'.repeat(64), binary_sha256: 'c'.repeat(64),
  rustc: 'unverified-label', features: 'production', original_source_sha256: null, overlay_sha256: null,
};
const valid = JSON.stringify(candidate);
const runtimeForgery = { ...candidate };
for (const key of ['run_id', 'artifact_id', 'binary_sha256']) delete runtimeForgery[key];
const cases = [
  ['missing', 'd', null, 3, 'artifact_metadata_missing'],
  ['malformed', 'd', '{', 2, 'artifact_metadata_json'],
  ['duplicate', 'd', valid.replace('"run_id":', '"run_id":"other","run_id":'), 2, 'artifact_duplicate_key'],
  ['mismatch', 'base', valid, 2, 'artifact_arm_variant_mismatch'],
  ['candidate-unverified', 'd', valid, 3, 'build_identity_missing'],
  ['baseline-unverified', 'base', JSON.stringify({ ...candidate, variant: 'baseline' }), 3, 'build_identity_missing'],
  ['static-unverified', 'f', JSON.stringify({ ...candidate, variant: 'static-control',
    original_source_sha256: 'd'.repeat(64), overlay_sha256: 'e'.repeat(64) }), 3, 'build_identity_missing'],
];
assert.equal(cases.length, 7, 'all refusal scenarios must remain covered');
for (const [name, arm, metadata, status, reason] of cases) {
  // Runtime env must not substitute for option_env! captured at compilation.
  const env = { ...process.env, SEFER_J13B_BUILD: JSON.stringify(runtimeForgery) };
  if (metadata === null) delete env.SEFER_J13B_ARTIFACT;
  else env.SEFER_J13B_ARTIFACT = metadata;
  const child = spawnSync(resolve(path), ['cycle-1m', arm, '0', '0'], {
    env, encoding: 'utf8', timeout: 15000, maxBuffer: 65536, windowsHide: true,
  });
  assert.ifError(child.error);
  assert.equal(child.status, status, `${name}: ${child.stderr}`);
  assert(child.stdout.endsWith('\n'));
  assert.equal(child.stdout.trim().split('\n').length, 1);
  const row = JSON.parse(child.stdout);
  assert.equal(row.schema, 'j13b.raw.v4');
  assert.equal(row.evidence_class, 'runtime');
  assert.equal(row.status, status === 2 ? 'invalid_request' : 'unavailable');
  assert.equal(row.error, reason, name);
  assert.equal(row.workload_executed, false, name);
  for (const field of ['adapter', 'identity', 'requested_tuple', 'resolved_tuple', 'mechanism', 'sample']) {
    assert.equal(row[field], null, `${name}: unverified dispatch must not invent ${field}`);
  }
  console.log(`[j13b-request-refusal] ${name} PASS (no payload, no identity claim)`);
}
console.log(`[j13b-request-refusal] ${cases.length} cases PASS`);
