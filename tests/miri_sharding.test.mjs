// Test suite for scripts/verify-miri-sharding.mjs (miri job sharding equivalence).
//
// Strategy: ground truth by construction. Synthetic pre/post-sharding expanded
// workflow texts are compared at the function level; RED counterfactuals flip
// one feature, drop one target, flip the provenance model and drop a shard
// job entirely — every mutation must turn the verdict red.
//
// Node >= 24, ESM, zero npm dependencies. No cargo, no network, no writes.

import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { test } from 'node:test';

import { effectiveWorkflow } from '../scripts/ci/effective-workflow.mjs';

import { OLD_JOBS, NEW_JOBS, SHARD_GROUPS, FIXTURE_PATH, jobRecords, recordsForJob, verifySharding } from '../scripts/verify-miri-sharding.mjs';

const STRICT = '-Zmiri-strict-provenance -Zmiri-disable-isolation';
const PLAIN = '-Zmiri-disable-isolation -Zmiri-preemption-rate=0.5';
const TB = '-Zmiri-strict-provenance -Zmiri-disable-isolation -Zmiri-tree-borrows';

const job = (id, envFlags, steps) => ['  ' + id + ':', '    name: x',
  ...(envFlags ? ['    env:', '      MIRIFLAGS: "' + envFlags + '"'] : []),
  '    steps:', ...steps].join('\n');
const step = (flags, script) => ['      - name: s',
  ...(flags ? ['        env:', '          MIRIFLAGS: "' + flags + '"'] : []),
  '        run: |',
  ...script.split('\n').map((l) => '          ' + l)].join('\n');
const beforeRecords = () => jobRecords(beforeText);
const stub = (id, envFlags, steps) => job(id, envFlags, steps);

// A tiny pre-sharding world shaped like the real one: miri-core holds a
// TB-flagged command plus a plain strict command; miri-fastbin and miri-plain
// mirror the real flags inheritance (job-level env, no step-level override).
const beforeText = [
  'name: ci',
  'on: push',
  'jobs:',
  job('miri-core', STRICT, [
    step(TB, 'cargo miri test --features "alloc-global fastbin internals" --test miri_depot_cycle 2>&1 | tee "$RUNNER_TEMP/x.log"\ngrep -F "[x] COMPLETE" "$RUNNER_TEMP/x.log"'),
    step(null, 'cargo miri test --locked --features "alloc-core" --test class_leaves_publication_miri'),
  ]),
  job('miri-fastbin', STRICT, [
    step(null, 'cargo miri test --features "alloc-global alloc-xthread fastbin internals" --test regression_merged -- regression_magazine_oracles::'),
  ]),
  job('miri-plain', PLAIN, [
    step(null, 'cargo miri test --features "alloc-global alloc-xthread internals bench-internals" --test a --test b'),
  ]),
].join('\n');

test('structure constants: three old jobs, seven shard jobs', () => {
  assert.deepEqual(OLD_JOBS, ['miri-core', 'miri-fastbin', 'miri-plain']);
  assert.deepEqual(Object.keys(SHARD_GROUPS), OLD_JOBS);
  for (const shards of Object.values(SHARD_GROUPS)) assert.ok(shards.length >= 2);
  assert.deepEqual(new Set(NEW_JOBS), new Set(Object.values(SHARD_GROUPS).flat()));
});

test('record normalization: features order-insensitive, flags and args preserved', () => {
  const rec = (script) => JSON.parse(recordsForJob(job('j', STRICT, [step(STRICT, script)]), '')[0]);
  assert.deepEqual(rec('cargo miri test --features "b a" --test t').features, ['a', 'b']);
  assert.deepEqual(rec('cargo miri test -p crate_x --test t -- --exact f').args, ['--exact', 'f']);
  assert.equal(rec('cargo miri test --test t').model, 'strict');
  assert.equal(JSON.parse(recordsForJob(job('j', PLAIN, [step(null, 'cargo miri test --test t')]), '')[0]).model, 'plain');
  assert.equal(JSON.parse(recordsForJob(job('j', PLAIN, [step(null, 'cargo miri test --test t')]), '')[0]).flags, PLAIN);
});

test('command split equivalence: one multi-target run == two single-target runs', () => {
  const one = recordsForJob(job('j', STRICT, [step(STRICT, 'cargo miri test --features "f1" --test a --test b')]), '');
  const two = recordsForJob(job('j', STRICT, [step(STRICT, 'cargo miri test --features "f1" --test a\ncargo miri test --features "f1" --test b')]), '');
  assert.deepEqual(one, two);
});

test('green path: shard unions equal the old job unions', () => {
  const afterText = [
    'name: ci',
    'on: push',
    'jobs:',
    stub('miri-core-pin-cache', STRICT, [step(TB, 'cargo miri test --features "alloc-global fastbin internals" --test miri_depot_cycle 2>&1 | tee "$RUNNER_TEMP/x.log"\ngrep -F "[x] COMPLETE" "$RUNNER_TEMP/x.log"')]),
    stub('miri-core-rest', STRICT, [step(null, 'cargo miri test --locked --features "alloc-core" --test class_leaves_publication_miri')]),
    stub('miri-fastbin-oracles', STRICT, [step(null, 'cargo miri test --features "alloc-global alloc-xthread fastbin internals" --test regression_merged -- regression_magazine_oracles::')]),
    stub('miri-fastbin-integrity-nl7', STRICT, []),
    stub('miri-fastbin-integrity-rest', STRICT, []),
    stub('miri-plain-heavy', PLAIN, [step(null, 'cargo miri test --features "alloc-global alloc-xthread internals bench-internals" --test a')]),
    stub('miri-plain-rest', PLAIN, [step(null, 'cargo miri test --features "alloc-global alloc-xthread internals bench-internals" --test b')]),
  ].join('\n');
  const r = verifySharding(beforeRecords(), afterText);
  assert.deepEqual(r.failures, [], 'unexpected failures');
  assert.equal(r.ok, true);
});

test('RED: dropping one target from a shard is detected', () => {
  // All seven shards exist, but the miri-core pair "loses" the class-leaves target.
  const afterText = [
    'name: ci',
    'on: push',
    'jobs:',
    stub('miri-core-pin-cache', STRICT, [step(TB, 'cargo miri test --features "alloc-global fastbin internals" --test miri_depot_cycle')]),
    stub('miri-core-rest', STRICT, []),
    stub('miri-fastbin-oracles', STRICT, [step(null, 'cargo miri test --features "alloc-global alloc-xthread fastbin internals" --test regression_merged -- regression_magazine_oracles::')]),
    stub('miri-fastbin-integrity-nl7', STRICT, []),
    stub('miri-fastbin-integrity-rest', STRICT, []),
    stub('miri-plain-heavy', PLAIN, [step(null, 'cargo miri test --features "alloc-global alloc-xthread internals bench-internals" --test a --test b')]),
    stub('miri-plain-rest', PLAIN, []),
  ].join('\n');
  const r = verifySharding(beforeRecords(), afterText);
  assert.equal(r.ok, false);
  assert.ok(r.failures.some((f) => f.includes('miri-core vs [miri-core-pin-cache, miri-core-rest] mismatch')
    && f.includes('class_leaves_publication_miri')), r.failures.join('\n'));
});

test('RED: flag drift (feature change or provenance flip) is detected', () => {
  const featureDrift = beforeText
    .replaceAll('  miri-core:', '  miri-core-pin-cache:')
    .replaceAll('  miri-fastbin:', '  miri-fastbin-oracles:')
    .replaceAll('  miri-plain:', '  miri-plain-heavy:')
    .replace('--features "alloc-global fastbin internals"', '--features "alloc-global fastbin internals extra"')
    + ['', '  miri-core-rest:', '    steps: []', '', '  miri-fastbin-integrity-nl7:', '    steps: []', '', '  miri-fastbin-integrity-rest:', '    steps: []', '', '  miri-plain-rest:', '    steps: []'].join('\n');
  const r = verifySharding(beforeRecords(), featureDrift);
  assert.equal(r.ok, false);
  assert.ok(r.failures.some((f) => f.includes('"extra"')), r.failures.join('\n'));

  const provenanceDrift = beforeText
    .replaceAll('  miri-core:', '  miri-core-pin-cache:')
    .replaceAll('  miri-fastbin:', '  miri-fastbin-oracles:')
    .replaceAll('  miri-plain:', '  miri-plain-heavy:')
    .replace('MIRIFLAGS: "' + PLAIN + '"', 'MIRIFLAGS: "' + STRICT + '"')
    + ['', '  miri-core-rest:', '    steps: []', '', '  miri-fastbin-integrity-nl7:', '    steps: []', '', '  miri-fastbin-integrity-rest:', '    steps: []', '', '  miri-plain-rest:', '    steps: []'].join('\n');
  const rp = verifySharding(beforeRecords(), provenanceDrift);
  assert.equal(rp.ok, false);
  assert.ok(rp.failures.some((f) => f.includes('miri-plain')), rp.failures.join('\n'));
});

test('RED: missing shard job is detected', () => {
  const r = verifySharding(beforeRecords(), beforeText); // no shard jobs at all
  assert.equal(r.ok, false);
  for (const id of NEW_JOBS) assert.ok(r.failures.some((f) => f.includes(`shard job ${id} not found`)), id);
});

test('jobRecords extracts per-job record sets from the real-world shape', () => {
  const records = jobRecords(beforeText);
  assert.deepEqual(Object.keys(records), OLD_JOBS);
  assert.equal(records['miri-core'].length, 2); // 1 TB depot + 1 class leaves
  assert.equal(records['miri-plain'].length, 2);
});

// ---- fixture-based tests (post-commit world) ----

const fixture = JSON.parse(readFileSync(FIXTURE_PATH, 'utf8'));

test('fixture: schema, ref, sources and expected record counts (40/4/6)', () => {
  assert.equal(fixture.schema, 'miri-sharding-before/v1');
  assert.match(fixture.ref, /^[0-9a-f]{40}$/);
  assert.ok(fixture.source.includes('.github/workflows/ci.yml'));
  for (const id of OLD_JOBS) {
    assert.ok(fixture.source.includes(`.github/actions/ci-${id}/action.yml`), id);
    assert.ok(Array.isArray(fixture.records[id]) && fixture.records[id].length > 0, id);
  }
  assert.equal(fixture.records['miri-core'].length, 40);
  assert.equal(fixture.records['miri-fastbin'].length, 4);
  // Task #223: +1 record for the p223_small_sidecar_zero_free_backing
  // miri-plain-rest target added to both fixture and CI (equivalence kept).
  assert.equal(fixture.records['miri-plain'].length, 7);
});

test('green: post-commit tree — old jobs absent, shards compared against fixture', () => {
  // The real current tree is the post-commit state: the expanded ci.yml has
  // only the seven shard jobs; the old miri-core/fastbin/plain jobs and their
  // actions are gone from the working tree. The fixture supplies the "before".
  const expanded = effectiveWorkflow(resolve(dirname(fileURLToPath(import.meta.url)), '..'));
  const present = jobRecords(expanded.text);
  for (const id of OLD_JOBS) assert.ok(!present[id], `old job ${id} must be absent after the merge`);
  const r = verifySharding(fixture.records, expanded.text);
  assert.deepEqual(r.failures, [], r.failures.join('\n'));
  assert.equal(r.ok, true);
});

test('RED: fixture-based before — dropping one shard target is detected', () => {
  const afterText = [
    'name: ci', 'on: push', 'jobs:',
    stub('miri-core-pin-cache', STRICT, [step(TB, 'cargo miri test --locked --features "alloc-global fastbin internals" --test a')]),
    stub('miri-core-rest', STRICT, []), // target lost
    stub('miri-fastbin-oracles', STRICT, [step(null, 'cargo miri test --features "alloc-global alloc-xthread fastbin internals" --test c -- d')]),
    stub('miri-fastbin-integrity-nl7', STRICT, [step(null, 'cargo miri test --features "fastbin internals" --test e')]),
    stub('miri-fastbin-integrity-rest', STRICT, [step(null, 'cargo miri test --features "fastbin internals" --test f')]),
    stub('miri-plain-heavy', PLAIN, [step(null, 'cargo miri test --features "internals" --test g --test h')]),
    stub('miri-plain-rest', PLAIN, [step(null, 'cargo miri test --features "alloc-xthread" --test i')]),
  ].join('\n');
  const r = verifySharding(fixture.records, afterText);
  assert.equal(r.ok, false);
  assert.ok(r.failures.some((f) => f.includes('miri-core vs') && f.includes('missing')), r.failures.join('\n'));
});

test('RED: fixture-based before — feature drift in a shard is detected', () => {
  const afterText = [
    'name: ci', 'on: push', 'jobs:',
    stub('miri-core-pin-cache', STRICT, [step(TB, 'cargo miri test --locked --features "alloc-global fastbin internals extra" --test a')]),
    stub('miri-core-rest', STRICT, [step(null, 'cargo miri test --locked --features "alloc-core" --test b')]),
    stub('miri-fastbin-oracles', STRICT, [step(null, 'cargo miri test --features "alloc-global alloc-xthread fastbin internals" --test c -- d')]),
    stub('miri-fastbin-integrity-nl7', STRICT, [step(null, 'cargo miri test --features "fastbin internals" --test e')]),
    stub('miri-fastbin-integrity-rest', STRICT, [step(null, 'cargo miri test --features "fastbin internals" --test f')]),
    stub('miri-plain-heavy', PLAIN, [step(null, 'cargo miri test --features "internals" --test g --test h')]),
    stub('miri-plain-rest', PLAIN, [step(null, 'cargo miri test --features "alloc-xthread" --test i')]),
  ].join('\n');
  const r = verifySharding(fixture.records, afterText);
  assert.equal(r.ok, false);
  assert.ok(r.failures.some((f) => f.includes('"extra"')), r.failures.join('\n'));
});

test('record normalization: --locked flag is part of the identity', () => {
  const withLocked = recordsForJob(job('j', STRICT, [step(null, 'cargo miri test --locked --test t')]), '')[0];
  const withoutLocked = recordsForJob(job('j', STRICT, [step(null, 'cargo miri test --test t')]), '')[0];
  assert.equal(JSON.parse(withLocked).locked, true);
  assert.equal(JSON.parse(withoutLocked).locked, false);
  assert.notEqual(withLocked, withoutLocked);
});
