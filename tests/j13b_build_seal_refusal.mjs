// CI-only consistency fixture, deliberately NOT real build provenance or payload admission.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';

const compiled = {
  source_commit: 'a'.repeat(40), source_sha256: 'b'.repeat(64),
  rustc: 'synthetic-build-label', features: 'production', variant: 'candidate',
  original_source_sha256: null, overlay_sha256: null,
};
const args = process.argv.slice(2);
if (args.length === 1 && args[0] === '--seal') {
  console.log(JSON.stringify(compiled));
} else {
  assert(args.length === 1 && args[0], 'usage: node tests/j13b_build_seal_refusal.mjs <built-sealed-child>');
  const base = { ...compiled, run_id: 'ci-consistency', artifact_id: 'candidate', binary_sha256: 'c'.repeat(64) };
  const changed = { ...base, source_sha256: 'f'.repeat(64) };
  const cases = [
    ['matching-not-identity', 'd', base, 3, 'identity_backend_missing', compiled],
    ['dynamic-labels-not-sealed', 'a', { ...base, run_id: 'other', artifact_id: 'other-artifact', binary_sha256: 'e'.repeat(64) }, 3, 'identity_backend_missing', compiled],
    ['source-mismatch', 'd', changed, 2, 'build_seal_mismatch', compiled],
    ['compiler-mismatch', 'd', { ...base, rustc: 'different-compiler' }, 2, 'build_seal_mismatch', compiled],
    ['features-mismatch', 'd', { ...base, features: 'production internals' }, 2, 'build_seal_mismatch', compiled],
    ['static-variant-mismatch', 'f', { ...base, variant: 'static-control', original_source_sha256: 'd'.repeat(64), overlay_sha256: 'e'.repeat(64) }, 2, 'build_seal_mismatch', compiled],
    ['runtime-env-cannot-override', 'd', changed, 2, 'build_seal_mismatch', { ...compiled, source_sha256: changed.source_sha256 }],
  ];
  assert.equal(cases.length, 7);
  for (const [name, arm, request, status, reason, runtimeForgery] of cases) {
    const child = spawnSync(resolve(args[0]), ['cycle-1m', arm, '0', '0'], {
      env: { ...process.env, SEFER_J13B_ARTIFACT: JSON.stringify(request), SEFER_J13B_BUILD: JSON.stringify(runtimeForgery) },
      encoding: 'utf8', timeout: 15000, maxBuffer: 65536, windowsHide: true,
    });
    assert.ifError(child.error);
    assert.equal(child.status, status, `${name}: ${child.stderr}`);
    assert(child.stdout.endsWith('\n'));
    assert.equal(child.stdout.trim().split('\n').length, 1);
    const row = JSON.parse(child.stdout);
    assert.equal(row.schema, 'j13b.raw.v4');
    assert.equal(row.error, reason, name);
    assert.equal(row.workload_executed, false);
    for (const field of ['adapter', 'identity', 'requested_tuple', 'resolved_tuple', 'mechanism', 'sample']) {
      assert.equal(row[field], null, `${name}: consistency is not actual identity`);
    }
    console.log(`[j13b-build-seal-refusal] ${name} PASS (no payload or identity claim)`);
  }
  console.log(`[j13b-build-seal-refusal] ${cases.length} cases PASS`);
}
