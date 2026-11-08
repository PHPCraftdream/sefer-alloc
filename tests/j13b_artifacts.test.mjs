import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { ARMS } from '../scripts/j13b_schema.mjs';
import { bindArtifactJobs, hashExecutable, readArtifactManifest, validateArtifactManifest,
  verifyArtifactRecord, verifyJobArtifact } from '../scripts/j13b_artifacts.mjs';
import { executeChildJobs } from '../scripts/j13b_run.mjs';

const digest = value => createHash('sha256').update(value).digest('hex');
const bytes = 'fixture bytes, never executed';
const artifact = (variant, path, source) => ({
  path, variant, source_commit: 'a'.repeat(40), source_sha256: digest(source),
  binary_sha256: digest(bytes), rustc: 'fixture rustc', features: 'production internals bench-internals',
  original_source_sha256: null, overlay_sha256: null,
});

function manifest() {
  const candidate = artifact('candidate', 'candidate.bin', 'candidate');
  const baseline = artifact('baseline', 'baseline.bin', 'baseline');
  const fixed = { ...artifact('static-control', 'fixed.bin', 'overlaid candidate'),
    binary_sha256: digest('fixed fixture bytes, never executed'),
    original_source_sha256: candidate.source_sha256, overlay_sha256: digest('reviewed overlay') };
  return { schema: 'j13b.artifacts.v1', run_id: 'fixture-run',
    artifacts: { candidate, baseline, fixed },
    arms: Object.fromEntries(ARMS.map(arm => [arm,
      ['base', 'base2', 'mi'].includes(arm) ? 'baseline' : arm === 'f' ? 'fixed' : 'candidate'])),
  };
}

const request = { cell_id: 'cycle-1m', arm: 'd', block: 0, repeat: 0 };
function withFiles(run) {
  const directory = mkdtempSync(join(tmpdir(), 'j13b-artifact-test-'));
  try {
    for (const path of ['candidate.bin', 'baseline.bin', 'fixed.bin']) {
      writeFileSync(join(directory, path), path === 'fixed.bin' ? 'fixed fixture bytes, never executed' : bytes);
    }
    const input = manifest();
    const [job] = bindArtifactJobs(input, [request], directory);
    return run({ directory, input, job });
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}

function record(job) {
  const { path, ...provenance } = job.artifact;
  return { ...request, window: 'initial', status: 'sample', workload_executed: true,
    identity: { ...provenance, artifact_id: job.artifact_id, run_id: job.run_id, invocation_id: 'fixture-child' } };
}
const encode = row => Buffer.from(`${JSON.stringify(row)}\n`);

test('artifact routing owns frozen copies and keeps A/A and mimalloc on the historical executable', () => {
  const input = manifest();
  const checked = validateArtifactManifest(input);
  input.artifacts.candidate.rustc = 'mutated later';
  assert.equal(checked.artifacts.candidate.rustc, 'fixture rustc');
  assert.ok(Object.isFrozen(checked));
  assert.ok(Object.isFrozen(checked.artifacts.candidate));
  const jobs = bindArtifactJobs(manifest(), ARMS.map(arm => ({ ...request, arm })), '/fixture-root');
  const byArm = Object.fromEntries(jobs.map(job => [job.arm, job]));
  assert.equal(byArm.base.executable, byArm.base2.executable);
  assert.equal(byArm.base.executable, byArm.mi.executable);
  assert.notEqual(byArm.base.executable, byArm.d.executable);
  assert.notEqual(byArm.f.executable, byArm.a.executable);
  assert.equal(byArm.f.artifact.variant, 'static-control');
});

test('malformed identity, routing, overlay, features and toolchain contracts fail before execution', () => {
  const mutations = [
    m => { m.schema = 'j13b.artifacts.v0'; },
    m => { m.run_id = ''; },
    m => { delete m.arms.ab; },
    m => { m.arms.extra = 'candidate'; },
    m => { m.arms.a = 'missing'; },
    m => { m.arms.base2 = 'candidate'; },
    m => { m.arms.mi = 'candidate'; },
    m => { m.arms.f = 'candidate'; },
    m => { m.arms.n = 'baseline'; },
    m => { m.artifacts.candidate.binary_sha256 = 'a'.repeat(63); },
    m => { m.artifacts.candidate.source_commit = ['a'.repeat(40)]; },
    m => { m.artifacts.candidate.path = ''; },
    m => { m.artifacts.candidate.overlay_sha256 = digest('unclaimed'); },
    m => { m.artifacts.fixed.overlay_sha256 = null; },
    m => { m.artifacts.fixed.binary_sha256 = m.artifacts.candidate.binary_sha256; },
    m => { m.artifacts.fixed.original_source_sha256 = digest('different original'); },
    m => { m.artifacts.fixed.source_sha256 = m.artifacts.fixed.original_source_sha256; },
    m => { m.artifacts.fixed.rustc = 'different compiler'; },
    m => { m.artifacts.fixed.features = 'different features'; },
    m => { m.artifacts.fixed.source_commit = 'b'.repeat(40); },
    m => { m.artifacts.fixed.path = m.artifacts.candidate.path; },
    m => { m.artifacts.baseline.unexpected = true; },
  ];
  for (const mutate of mutations) {
    const input = manifest();
    mutate(input);
    assert.throws(() => validateArtifactManifest(input));
  }
  const input = manifest();
  input.artifacts.fixed.path = './candidate.bin';
  assert.throws(() => bindArtifactJobs(input, [request], '/fixture-root'), /resolved artifact paths/);
});

test('manifest reader bounds input and rejects malformed UTF-8 without spawning', () => withFiles(({ directory }) => {
  const path = join(directory, 'artifacts.json');
  writeFileSync(path, JSON.stringify(manifest()));
  assert.equal(readArtifactManifest(path).schema, 'j13b.artifacts.v1');
  writeFileSync(path, Buffer.alloc(64 * 1024 + 1, 32));
  assert.throws(() => readArtifactManifest(path), /exceeds 64 KiB/);
  writeFileSync(path, Buffer.from([0xff, 0xfe]));
  assert.throws(() => readArtifactManifest(path), /encoded data|encoding/i);
  writeFileSync(path, '{');
  assert.throws(() => readArtifactManifest(path), SyntaxError);
}));

test('actual fixture bytes are hashed and a stale executable is rejected', () => withFiles(({ job }) => {
  assert.equal(hashExecutable(job.executable), digest(bytes));
  verifyJobArtifact(job);
  writeFileSync(job.executable, 'changed');
  assert.throws(() => verifyJobArtifact(job), /SHA256 mismatch/);
  assert.throws(() => verifyJobArtifact({ ...request, executable: job.executable }), /immutable artifact binding/);
}));

test('driver rejects unbound and stale jobs without invoking a child', () => withFiles(({ job }) => {
  let calls = 0;
  const options = { sink: () => {}, spawn: () => { calls += 1; throw Error('must not spawn'); } };
  assert.throws(() => executeChildJobs({ ...options, jobs: [{ ...request, executable: job.executable }] }), /immutable artifact binding/);
  writeFileSync(job.executable, 'stale');
  assert.throws(() => executeChildJobs({ ...options, jobs: [job] }), /SHA256 mismatch/);
  assert.equal(calls, 0);
}));

test('driver preserves raw bytes before rejecting an executable changed during the child', () => withFiles(({ job }) => {
  const raw = encode(record(job));
  const captured = [];
  assert.throws(() => executeChildJobs({ jobs: [job], sink: value => captured.push(value),
    validate: () => { throw Error('must not validate'); },
    spawn: () => { writeFileSync(job.executable, 'changed during child'); return { stdout: raw, status: 0 }; },
  }), /SHA256 mismatch/);
  assert.deepEqual(captured, [raw]);
}));

test('every artifact identity field is bound to the dispatched manifest', () => withFiles(({ job }) => {
  verifyArtifactRecord(record(job), job);
  for (const key of ['run_id', 'artifact_id', 'source_sha256', 'binary_sha256', 'rustc', 'features',
    'variant', 'source_commit', 'original_source_sha256', 'overlay_sha256']) {
    const row = record(job);
    row.identity[key] = 'wrong';
    assert.throws(() => verifyArtifactRecord(row, job), /mismatch/);
  }
}));

test('bound transport checks hashes twice, omits artifact paths from child metadata, and retains runtime refusal', () => withFiles(({ job }) => {
  const calls = [];
  const captured = [];
  const spawn = (executable, args, options) => {
    calls.push('spawn');
    assert.equal(executable, job.executable);
    assert.deepEqual(args, ['cycle-1m', 'd', '0', '0']);
    const metadata = JSON.parse(options.env.SEFER_J13B_ARTIFACT);
    assert.equal(metadata.artifact_id, job.artifact_id);
    assert.equal(metadata.path, undefined);
    return { stdout: encode(record(job)), status: 0 };
  };
  const verifyArtifact = bound => { calls.push('hash'); verifyJobArtifact(bound); };
  const rows = executeChildJobs({ jobs: [job], sink: raw => { calls.push('sink'); captured.push(raw); },
    spawn, verifyArtifact, validate: () => {} });
  assert.deepEqual(calls, ['hash', 'spawn', 'sink', 'hash']);
  assert.equal(rows.length, 1);
  assert.equal(captured.length, 1);
  assert.throws(() => executeChildJobs({ jobs: [job], sink: () => {}, spawn }), /runtime report admission disabled/);
}));
